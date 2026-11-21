//! `tsrs-lint server`, driven over its stdin and stdout as oxlint's language server drives it.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use tsrs_core::json::{self, Value};
use tsrs_lint::protocol::{self, MessageType};

const DID_OPEN: u8 = 16;
const DID_CHANGE: u8 = 17;
const DID_CLOSE: u8 = 18;
const FILES_CHANGED: u8 = 19;
const LINT: u8 = 20;
const SHUTDOWN: u8 = 21;

const ASYNC_A: &str = "export async function f(): Promise<number> { return 1; }\n";
const SYNC_A: &str = "export function f(): number { return 1; }\n";
const CALLS_A: &str = "import { f } from \"./a\";\nf();\n";

const TSCONFIG: &str = r#"{ "compilerOptions": { "strict": true, "target": "es2022", "module": "esnext", "moduleResolution": "bundler" }, "include": ["*.ts"] }"#;

/// A project in cargo's scratch directory for integration tests. Each run rewrites its files.
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tsrs-lint-server").join(name);
    project(&dir, ASYNC_A)
}

/// Writes a project with `a.ts` and a `b.ts` that calls `a.ts`'s `f` into `dir`.
fn project(dir: &Path, a: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();
    std::fs::write(dir.join("a.ts"), a).unwrap();
    std::fs::write(dir.join("b.ts"), CALLS_A).unwrap();
    dir
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().replace('\\', "/")
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    next_id: u64,
}

/// A lint request's answer: the file and rule of each diagnostic.
#[derive(Debug)]
struct Answer {
    diagnostics: Vec<(String, String)>,
    error: Option<String>,
}

impl Server {
    fn start(cwd: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs-lint"))
            .arg("server")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        Self { child, stdin, stdout, next_id: 1 }
    }

    fn send(&mut self, message_type: u8, value: &Value) {
        self.send_frames(&[frame(message_type, value)]);
    }

    /// Writes `frames` at once, so the server reads them back to back.
    fn send_frames(&mut self, frames: &[Vec<u8>]) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(&frames.concat()).unwrap();
        stdin.flush().unwrap();
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn open(&mut self, path: &str, version: i64, text: &str) {
        self.send(DID_OPEN, &document(path, version, text));
    }

    fn change(&mut self, path: &str, version: i64, text: &str) {
        self.send(DID_CHANGE, &document(path, version, text));
    }

    /// Lints `files` with `no-floating-promises`.
    fn lint(&mut self, files: &[&str]) -> Answer {
        self.lint_with_rule(files, "no-floating-promises")
    }

    fn lint_with_rule(&mut self, files: &[&str], rule: &str) -> Answer {
        let id = self.take_id();
        self.send_frames(&[lint_frame(id, files, rule)]);
        let mut answer = Answer { diagnostics: Vec::new(), error: None };
        loop {
            let (message_type, value) = self.read().expect("the server ended before answering");
            let Value::Object(fields) = &value else { panic!("{value:?}") };
            assert_eq!(number(fields.get("id")), Some(id), "{value:?}");
            match message_type {
                t if t == MessageType::Diagnostic as u8 => {
                    let rule = match fields.get("rule") {
                        Some(Value::String(rule)) => rule.clone(),
                        _ => String::new(),
                    };
                    let file = match fields.get("file_path") {
                        Some(Value::String(file)) => file.clone(),
                        _ => String::new(),
                    };
                    answer.diagnostics.push((file, rule));
                }
                t if t == MessageType::ServerLintDone as u8 => return answer,
                t if t == MessageType::Error as u8 => {
                    let Some(Value::String(error)) = fields.get("error") else { panic!() };
                    answer.error = Some(error.clone());
                    return answer;
                }
                other => panic!("unexpected frame type {other}: {value:?}"),
            }
        }
    }

    fn read(&mut self) -> Option<(u8, Value)> {
        let stdout: &mut dyn Read = &mut self.stdout;
        let (message_type, bytes) = protocol::read_frame(stdout).unwrap()?;
        Some((message_type, json::unmarshal(std::str::from_utf8(&bytes).unwrap()).unwrap()))
    }

    fn exit_code(mut self) -> i32 {
        drop(self.stdin.take());
        self.child.wait().unwrap().code().unwrap()
    }
}

fn frame(message_type: u8, value: &Value) -> Vec<u8> {
    raw_frame(message_type, json::marshal(value).unwrap().as_bytes())
}

/// A frame of type `message_type` whose payload is `bytes`, which need not be JSON.
fn raw_frame(message_type: u8, bytes: &[u8]) -> Vec<u8> {
    let mut frame = u32::try_from(bytes.len()).unwrap().to_le_bytes().to_vec();
    frame.push(message_type);
    frame.extend(bytes);
    frame
}

/// A lint request of `files` with `rule`.
fn lint_frame(id: u64, files: &[&str], rule: &str) -> Vec<u8> {
    let files = files.iter().map(|path| Value::String((*path).into())).collect();
    frame(
        LINT,
        &protocol::object([
            ("id", id_value(id)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(files)),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([(
                            "name",
                            Value::String(rule.into()),
                        )])]),
                    ),
                ])]),
            ),
            ("fix", Value::Bool(true)),
            ("fix_suggestions", Value::Bool(true)),
        ]),
    )
}

fn files_changed(changes: &[(&str, i64)]) -> Value {
    protocol::object([(
        "changes",
        Value::Array(
            changes
                .iter()
                .map(|(path, kind)| {
                    protocol::object([
                        ("path", Value::String((*path).into())),
                        ("kind", Value::Integer(*kind)),
                    ])
                })
                .collect(),
        ),
    )])
}

fn id_value(id: u64) -> Value {
    Value::Integer(i64::try_from(id).unwrap())
}

/// The error of a fatal error frame, which has a null id.
fn fatal_error(frame: Option<(u8, Value)>) -> String {
    let (message_type, value) = frame.expect("the server ended without an error");
    assert_eq!(message_type, MessageType::Error as u8, "{value:?}");
    let Value::Object(fields) = &value else { panic!("{value:?}") };
    assert_eq!(fields.get("id"), Some(&Value::Null), "{value:?}");
    let Some(Value::String(error)) = fields.get("error") else { panic!("{value:?}") };
    error.clone()
}

fn document(path: &str, version: i64, text: &str) -> Value {
    protocol::object([
        ("path", Value::String(path.into())),
        ("version", Value::Integer(version)),
        ("text", Value::String(text.into())),
    ])
}

fn number(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Integer(n) => u64::try_from(*n).ok(),
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "small ids")]
        Value::Number(n) => Some(*n as u64),
        _ => None,
    }
}

fn rules_of(answer: &Answer, file: &str) -> Vec<String> {
    assert_eq!(answer.error, None);
    answer
        .diagnostics
        .iter()
        .filter(|(path, _)| path == file)
        .map(|(_, rule)| rule.clone())
        .collect()
}

const NONE: [&str; 0] = [];

#[test]
fn lints_the_unsaved_text_of_open_files() {
    let dir = fixture("unsaved");
    let (a, b) = (path(&dir, "a.ts"), path(&dir, "b.ts"));
    let mut server = Server::start(&dir);
    server.open(&b, 1, CALLS_A);
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);

    // An unsaved edit of the linted file itself.
    server.change(&b, 2, "import { f } from \"./a\";\nvoid f();\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);

    // An unsaved edit of another open file changes the types the linted file sees.
    server.change(&b, 3, CALLS_A);
    server.open(&a, 1, SYNC_A);
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn files_changed_on_disk_are_picked_up() {
    let dir = fixture("disk");
    let (a, b) = (path(&dir, "a.ts"), path(&dir, "b.ts"));
    let mut server = Server::start(&dir);
    server.open(&b, 1, CALLS_A);
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);

    std::fs::write(dir.join("a.ts"), SYNC_A).unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&a, 2)]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);

    // A closed file is read from disk again.
    server.change(&b, 2, "export {};\nPromise.resolve(1);\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    server.send(DID_CLOSE, &protocol::object([("path", Value::String(b.clone()))]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn requests_fail_on_their_own_and_shutdown_ends_the_server() {
    let dir = fixture("errors");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    let answer = server.lint_with_rule(&[&b], "not-a-rule");
    assert!(answer.error.is_some(), "{answer:?}");
    // The server keeps serving after a failed request.
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    server.send(SHUTDOWN, &protocol::object([]));
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_malformed_notification_ends_the_server() {
    let dir = fixture("malformed");
    let mut server = Server::start(&dir);
    server.send(DID_OPEN, &protocol::object([("path", Value::Integer(5))]));
    fatal_error(server.read());
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 1);
}

#[test]
fn one_server_lints_several_projects() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tsrs-lint-server").join("projects");
    let first = project(&root.join("first"), ASYNC_A);
    let second = project(&root.join("second"), SYNC_A);
    let root = root.canonicalize().unwrap();
    let (first_b, second_b) = (path(&first, "b.ts"), path(&second, "b.ts"));
    let mut server = Server::start(&root);
    // Each `b.ts` sees its own project's `a.ts`.
    let answer = server.lint(&[&first_b, &second_b]);
    assert_eq!(rules_of(&answer, &first_b), ["no-floating-promises"]);
    assert_eq!(rules_of(&answer, &second_b), NONE);
    let answer = server.lint(&[&second_b]);
    assert_eq!(rules_of(&answer, &second_b), NONE);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_tsconfig_change_on_disk_is_picked_up() {
    let dir = fixture("tsconfig");
    let (b, tsconfig) = (path(&dir, "b.ts"), path(&dir, "tsconfig.json"));
    let mut server = Server::start(&dir);
    server.open(&b, 1, CALLS_A);
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);

    // An invalid option: the project's files are not linted, and the tsconfig error is reported.
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "strict": true, "target": "not-a-target" }, "include": ["*.ts"] }"#,
    )
    .unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&tsconfig, 2)]));
    let answer = server.lint(&[&b]);
    assert_eq!(rules_of(&answer, &b), NONE);
    assert_eq!(rules_of(&answer, &tsconfig), [""], "a tsconfig error: {answer:?}");

    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&tsconfig, 2)]));
    let answer = server.lint(&[&b]);
    assert_eq!(rules_of(&answer, &b), ["no-floating-promises"]);
    assert_eq!(rules_of(&answer, &tsconfig), NONE);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn files_created_and_deleted_on_disk_are_picked_up() {
    let dir = fixture("create-delete");
    let (b, c) = (path(&dir, "b.ts"), path(&dir, "c.ts"));
    let _ = std::fs::remove_file(dir.join("c.ts"));
    let calls_c = "import { g } from \"./c\";\ng();\n";
    std::fs::write(dir.join("b.ts"), calls_c).unwrap();
    let mut server = Server::start(&dir);
    // `./c` does not resolve: `g` has no type.
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);

    std::fs::write(dir.join("c.ts"), "export async function g(): Promise<void> {}\n").unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&c, 1)]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);

    std::fs::remove_file(dir.join("c.ts")).unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&c, 3)]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    assert_eq!(server.exit_code(), 0);
}
