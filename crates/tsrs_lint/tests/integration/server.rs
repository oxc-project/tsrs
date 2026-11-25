//! `tsrs-lint server`, driven over its stdin and stdout as oxlint's language server drives it.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use tsrs_core::json::{self, Value};
use tsrs_lint::protocol::{self, MessageType};
use tsrs_lint::server::PROTOCOL_VERSION;

const DID_OPEN: u8 = 16;
const DID_CHANGE: u8 = 17;
const DID_CLOSE: u8 = 18;
const FILES_CHANGED: u8 = 19;
const LINT: u8 = 20;
const SHUTDOWN: u8 = 21;
const HELLO: u8 = 22;
const CANCEL: u8 = 23;

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

/// Whether the server folds file names' case. The OS file system decides it as Go does: it is case-insensitive if the
/// executable exists under its name with the case of each letter swapped.
fn server_folds_case() -> bool {
    let swapped: String = env!("CARGO_BIN_EXE_tsrs-lint")
        .chars()
        .map(|c| if c.is_uppercase() { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() })
        .collect();
    Path::new(&swapped).exists()
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
    /// Starts a server and completes the handshake.
    fn start(cwd: &Path) -> Self {
        Self::start_with_workers(cwd, None)
    }

    /// Starts a server with `TSRS_LINT_SERVER_WORKERS` set to `workers`, and completes the handshake.
    fn start_with_workers(cwd: &Path, workers: Option<&str>) -> Self {
        let mut server = Self::spawn(cwd, workers);
        server.hello(PROTOCOL_VERSION);
        let (message_type, _) = server.read().expect("the server ended before its Hello");
        assert_eq!(message_type, MessageType::ServerHello as u8);
        server
    }

    /// Starts a server without a handshake.
    fn spawn(cwd: &Path, workers: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsrs-lint"));
        command.env_remove("TSRS_LINT_SERVER_WORKERS");
        if let Some(workers) = workers {
            command.env("TSRS_LINT_SERVER_WORKERS", workers);
        }
        let mut child = command
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

    fn hello(&mut self, protocol_version: u32) {
        self.send(
            HELLO,
            &protocol::object([("protocol_version", Value::Integer(protocol_version.into()))]),
        );
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

    /// Reads frames until the server closes its output, and groups request frames by `id`.
    fn read_to_end(&mut self) -> BTreeMap<u64, Vec<(u8, Value)>> {
        let mut frames: BTreeMap<u64, Vec<(u8, Value)>> = BTreeMap::new();
        while let Some((message_type, value)) = self.read() {
            let Value::Object(fields) = &value else { panic!("{value:?}") };
            let id = number(fields.get("id")).unwrap_or_else(|| panic!("no id: {value:?}"));
            frames.entry(id).or_default().push((message_type, value));
        }
        frames
    }

    /// Reads frames until each of `ids` has its terminal frame, and returns each request's terminal frame.
    fn read_terminals(&mut self, ids: &[u64]) -> BTreeMap<u64, Terminal> {
        let mut terminals = BTreeMap::new();
        while terminals.len() < ids.len() {
            let (message_type, value) = self.read().expect("the server ended before answering");
            let Value::Object(fields) = &value else { panic!("{value:?}") };
            let id = number(fields.get("id")).unwrap_or_else(|| panic!("no id: {value:?}"));
            assert!(ids.contains(&id), "a frame of another request: {value:?}");
            if message_type == MessageType::Diagnostic as u8 {
                continue;
            }
            let terminal = Terminal::of(message_type, &value);
            assert!(terminals.insert(id, terminal).is_none(), "a second terminal frame: {value:?}");
        }
        terminals
    }

    /// Reads request `id`'s frames through its terminal frame, which must be `Done`.
    fn read_terminals_with_diagnostics(&mut self, id: u64) -> Vec<(u8, Value)> {
        let mut frames = Vec::new();
        loop {
            let (message_type, value) = self.read().expect("the server ended before answering");
            let Value::Object(fields) = &value else { panic!("{value:?}") };
            assert_eq!(number(fields.get("id")), Some(id), "{value:?}");
            if message_type == MessageType::Diagnostic as u8 {
                frames.push((message_type, value));
                continue;
            }
            assert_eq!(Terminal::of(message_type, &value), Terminal::Done);
            return frames;
        }
    }

    fn exit_code(mut self) -> i32 {
        drop(self.stdin.take());
        self.child.wait().unwrap().code().unwrap()
    }
}

/// How a request ended.
#[derive(Debug, PartialEq, Eq)]
enum Terminal {
    Done,
    Canceled,
    Error(String),
}

impl Terminal {
    fn of(message_type: u8, value: &Value) -> Self {
        let Value::Object(fields) = value else { panic!("{value:?}") };
        if message_type == MessageType::ServerLintDone as u8 {
            return Self::Done;
        }
        assert_eq!(message_type, MessageType::Error as u8, "{value:?}");
        let Some(Value::String(error)) = fields.get("error") else { panic!("{value:?}") };
        match fields.get("canceled") {
            Some(Value::Bool(true)) => {
                assert_eq!(error, "request canceled");
                Self::Canceled
            }
            None => Self::Error(error.clone()),
            Some(other) => panic!("canceled: {other:?}"),
        }
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

fn cancel_frame(id: u64) -> Vec<u8> {
    frame(CANCEL, &protocol::object([("id", id_value(id))]))
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
    let answer = server.lint(&[&b]);
    eprintln!("DEBUG {answer:?}");
    assert_eq!(rules_of(&answer, &b), ["no-floating-promises"]);
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
fn the_handshake_answers_with_the_server_versions() {
    let dir = fixture("hello");
    let mut server = Server::spawn(&dir, None);
    server.hello(PROTOCOL_VERSION);
    let (message_type, value) = server.read().expect("the server ended before its Hello");
    assert_eq!(message_type, MessageType::ServerHello as u8);
    let Value::Object(fields) = &value else { panic!("{value:?}") };
    assert_eq!(number(fields.get("protocol_version")), Some(u64::from(PROTOCOL_VERSION)));
    assert_eq!(fields.get("version"), Some(&Value::String(env!("CARGO_PKG_VERSION").into())));
    server.send(SHUTDOWN, &protocol::object([]));
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_protocol_version_mismatch_ends_the_server_after_its_hello() {
    let dir = fixture("mismatch");
    let mut server = Server::spawn(&dir, None);
    server.hello(PROTOCOL_VERSION + 1);
    let (message_type, _) = server.read().expect("the server ended before its Hello");
    assert_eq!(message_type, MessageType::ServerHello as u8);
    assert_eq!(
        fatal_error(server.read()),
        format!(
            "protocol version mismatch: client speaks {}, tsrs-lint server speaks {PROTOCOL_VERSION}",
            PROTOCOL_VERSION + 1
        )
    );
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 1);
}

#[test]
fn a_first_frame_other_than_hello_ends_the_server() {
    let dir = fixture("no-hello");
    let b = path(&dir, "b.ts");
    let mut server = Server::spawn(&dir, None);
    server.open(&b, 1, CALLS_A);
    let error = fatal_error(server.read());
    assert!(error.contains("expected a Hello frame"), "{error}");
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 1);

    // A Hello without a protocol version is answered, then ends the server.
    let mut server = Server::spawn(&dir, None);
    server.send(HELLO, &protocol::object([]));
    let (message_type, _) = server.read().expect("the server ended before its Hello");
    assert_eq!(message_type, MessageType::ServerHello as u8);
    let error = fatal_error(server.read());
    assert!(error.starts_with("invalid Hello frame"), "{error}");
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 1);
}

#[test]
fn unknown_frame_types_are_skipped_after_the_handshake() {
    let dir = fixture("unknown");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    server.send_frames(&[
        raw_frame(99, b"not json"),
        raw_frame(99, b"{}"),
        raw_frame(250, b""),
        // A second Hello is skipped too.
        frame(HELLO, &protocol::object([("protocol_version", Value::Integer(0))])),
    ]);
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
fn queued_requests_are_answered_in_order() {
    let dir = fixture("queue");
    let b = path(&dir, "b.ts");
    // One worker: the requests wait in the queue and are linted first in, first out.
    let mut server = Server::start_with_workers(&dir, Some("1"));
    let ids: Vec<u64> = std::iter::repeat_with(|| server.take_id()).take(3).collect();
    let frames: Vec<Vec<u8>> =
        ids.iter().map(|id| lint_frame(*id, &[&b], "no-floating-promises")).collect();
    server.send_frames(&frames);
    let mut answered = Vec::new();
    while answered.len() < ids.len() {
        let (message_type, value) = server.read().expect("the server ended before answering");
        let Value::Object(fields) = &value else { panic!("{value:?}") };
        let id = number(fields.get("id")).unwrap_or_else(|| panic!("no id: {value:?}"));
        if message_type == MessageType::ServerLintDone as u8 {
            answered.push(id);
        } else {
            assert_eq!(message_type, MessageType::Diagnostic as u8, "{value:?}");
        }
    }
    assert_eq!(answered, ids);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn canceling_a_queued_request_answers_it_at_once() {
    let dir = fixture("cancel-queued");
    let b = path(&dir, "b.ts");
    // One worker: the second request waits behind the first.
    let mut server = Server::start_with_workers(&dir, Some("1"));
    let (first, second) = (server.take_id(), server.take_id());
    server.send_frames(&[
        lint_frame(first, &[&b], "no-floating-promises"),
        lint_frame(second, &[&b], "no-floating-promises"),
        cancel_frame(second),
    ]);
    let terminals = server.read_terminals(&[first, second]);
    assert_eq!(terminals[&first], Terminal::Done);
    assert_eq!(terminals[&second], Terminal::Canceled);
    // The server keeps serving.
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_canceled_request_drops_its_diagnostics() {
    let dir = fixture("cancel-started");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    // The request is canceled whether or not a worker has started it: a running request checks before each file
    // and before it answers.
    let id = server.take_id();
    server.send_frames(&[lint_frame(id, &[&b], "no-floating-promises"), cancel_frame(id)]);
    let frames = server.read_terminals(&[id]);
    assert_eq!(frames[&id], Terminal::Canceled);
    server.send(SHUTDOWN, &protocol::object([]));
    // No diagnostic of the canceled request follows.
    assert!(server.read_to_end().is_empty());
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn canceling_an_unknown_or_finished_request_is_ignored() {
    let dir = fixture("cancel-unknown");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    server.send_frames(&[cancel_frame(999)]);
    let answer = server.lint(&[&b]);
    assert_eq!(rules_of(&answer, &b), ["no-floating-promises"]);
    // The request above has finished.
    server.send_frames(&[cancel_frame(server.next_id - 1)]);
    // `lint` asserts that every frame it reads belongs to its own request.
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    server.send(SHUTDOWN, &protocol::object([]));
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn shutdown_cancels_queued_requests() {
    let dir = fixture("shutdown");
    let b = path(&dir, "b.ts");
    let mut server = Server::start_with_workers(&dir, Some("1"));
    let first = server.take_id();
    server.send_frames(&[lint_frame(first, &[&b], "no-floating-promises")]);
    // A round trip (a request that fails to parse is answered by the reader) gives the only worker time to start the
    // first request, which builds a program. Nothing tells the client that a request has started, though.
    assert!(server.lint_with_rule(&[&b], "not-a-rule").error.is_some());
    // The second request waits behind the first when the shutdown arrives.
    let second = server.take_id();
    server.send_frames(&[
        lint_frame(second, &[&b], "no-floating-promises"),
        frame(SHUTDOWN, &protocol::object([])),
    ]);
    let frames = server.read_to_end();
    let terminal = |id: u64| {
        let (message_type, value) = frames[&id].last().unwrap();
        Terminal::of(*message_type, value)
    };
    // A running request finishes: one diagnostic, then Done.
    match terminal(first) {
        Terminal::Done => assert_eq!(frames[&first].len(), 2, "{:?}", frames[&first]),
        Terminal::Canceled => assert_eq!(frames[&first].len(), 1, "{:?}", frames[&first]),
        Terminal::Error(error) => panic!("{error}"),
    }
    assert_eq!(terminal(second), Terminal::Canceled);
    assert_eq!(frames[&second].len(), 1, "{:?}", frames[&second]);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn every_request_gets_exactly_one_terminal_frame() {
    let dir = fixture("terminals");
    let b = path(&dir, "b.ts");
    let mut server = Server::start_with_workers(&dir, Some("1"));
    let ids: Vec<u64> = std::iter::repeat_with(|| server.take_id()).take(5).collect();
    server.send_frames(&[
        lint_frame(ids[0], &[&b], "no-floating-promises"),
        lint_frame(ids[1], &[&b], "not-a-rule"),
        lint_frame(ids[2], &[&b], "no-floating-promises"),
        cancel_frame(ids[2]),
        // Canceled twice: still one answer.
        cancel_frame(ids[2]),
        cancel_frame(999),
        lint_frame(ids[3], &[&b], "no-floating-promises"),
        lint_frame(ids[4], &[&b], "no-floating-promises"),
    ]);
    let terminals = server.read_terminals(&ids[..4]);
    assert_eq!(terminals[&ids[0]], Terminal::Done);
    assert!(matches!(&terminals[&ids[1]], Terminal::Error(error) if error.contains("not-a-rule")));
    assert_eq!(terminals[&ids[2]], Terminal::Canceled);
    assert_eq!(terminals[&ids[3]], Terminal::Done);
    server.send(SHUTDOWN, &protocol::object([]));
    // The last request finishes before the server exits, or is canceled if it was still queued.
    let rest = server.read_to_end();
    assert_eq!(rest.keys().copied().collect::<Vec<_>>(), [ids[4]], "{rest:?}");
    let terminals: Vec<Terminal> = rest[&ids[4]]
        .iter()
        .filter(|(message_type, _)| *message_type != MessageType::Diagnostic as u8)
        .map(|(message_type, value)| Terminal::of(*message_type, value))
        .collect();
    assert!(matches!(terminals[..], [Terminal::Done | Terminal::Canceled]), "{terminals:?}");
    assert_eq!(server.exit_code(), 0);
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

#[test]
fn closed_files_outside_every_tsconfig_are_linted() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tsrs-lint-server").join("no-tsconfig");
    let dir = project(&dir, ASYNC_A);
    std::fs::remove_file(dir.join("tsconfig.json")).unwrap();
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn closed_files_of_a_project_loaded_later_are_linted() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tsrs-lint-server").join("later");
    let first = project(&root.join("first"), ASYNC_A);
    let second = project(&root.join("second"), ASYNC_A);
    let root = root.canonicalize().unwrap();
    let (first_b, second_b) = (path(&first, "b.ts"), path(&second, "b.ts"));
    let mut server = Server::start(&root);
    assert_eq!(rules_of(&server.lint(&[&first_b]), &first_b), ["no-floating-promises"]);
    assert_eq!(rules_of(&server.lint(&[&second_b]), &second_b), ["no-floating-promises"]);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_request_sees_one_state_of_the_workspace() {
    let dir = fixture("one-snapshot");
    let a = path(&dir, "a.ts");
    let callers: Vec<String> = (0..40)
        .map(|index| {
            let name = format!("c{index}.ts");
            std::fs::write(dir.join(&name), CALLS_A).unwrap();
            path(&dir, &name)
        })
        .collect();
    let callers: Vec<&str> = callers.iter().map(String::as_str).collect();
    let mut server = Server::start_with_workers(&dir, Some("1"));
    server.open(&a, 1, ASYNC_A);
    for version in 2..12 {
        // The edit races the request: each file must see either the old `a.ts` or the new one, all the same.
        let id = server.take_id();
        let text = if version % 2 == 0 { SYNC_A } else { ASYNC_A };
        server.send_frames(&[
            lint_frame(id, &callers, "no-floating-promises"),
            frame(DID_CHANGE, &document(&a, version, text)),
        ]);
        let frames = server.read_terminals_with_diagnostics(id);
        let flagged = frames.iter().filter(|(t, _)| *t == MessageType::Diagnostic as u8).count();
        assert!(
            flagged == 0 || flagged == callers.len(),
            "{flagged} of {} files flagged",
            callers.len()
        );
    }
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn notification_paths_name_files_as_lint_requests_do() {
    let dir = fixture("paths");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    // Unsaved text that has no floating promise, opened under a path with a `..` segment.
    server.open(&path(&dir, "sub/../b.ts"), 1, "export {};\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    // A relative path names the same file, so closing it reads `b.ts` from disk again.
    server.send(DID_CLOSE, &protocol::object([("path", Value::String("b.ts".into()))]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    // And so does a relative open.
    server.open("./b.ts", 1, "export {};\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_fatal_error_is_the_last_frame() {
    let dir = fixture("fatal-last");
    let b = path(&dir, "b.ts");
    let mut server = Server::start_with_workers(&dir, Some("1"));
    // The first request runs and the second waits when the malformed notification arrives.
    let (first, second) = (server.take_id(), server.take_id());
    server.send_frames(&[
        lint_frame(first, &[&b], "no-floating-promises"),
        lint_frame(second, &[&b], "no-floating-promises"),
        raw_frame(DID_OPEN, b"not json"),
    ]);
    let mut frames = Vec::new();
    while let Some(frame) = server.read() {
        frames.push(frame);
    }
    let last = frames.pop();
    assert!(fatal_error(last).starts_with("invalid frame"));
    for (_, value) in &frames {
        let Value::Object(fields) = value else { panic!("{value:?}") };
        assert_ne!(fields.get("id"), Some(&Value::Null), "a second fatal error: {value:?}");
    }
    assert_eq!(server.exit_code(), 1);
}

#[test]
fn a_file_in_several_configs_gets_the_last_configs_rules() {
    let dir = fixture("several-configs");
    let b = path(&dir, "b.ts");
    let mut server = Server::start(&dir);
    let config = |rule: &str| {
        protocol::object([
            ("file_paths", Value::Array(vec![Value::String(b.clone())])),
            ("rules", Value::Array(vec![protocol::object([("name", Value::String(rule.into()))])])),
        ])
    };
    for (rules, expected) in [
        (["no-floating-promises", "await-thenable"], NONE.as_slice()),
        (["await-thenable", "no-floating-promises"], ["no-floating-promises"].as_slice()),
    ] {
        let id = server.take_id();
        server.send(
            LINT,
            &protocol::object([
                ("id", id_value(id)),
                ("configs", Value::Array(rules.iter().map(|rule| config(rule)).collect())),
            ]),
        );
        let frames = server.read_terminals_with_diagnostics(id);
        let flagged: Vec<&str> = frames
            .iter()
            .filter_map(|(_, value)| match value {
                Value::Object(fields) => match fields.get("rule") {
                    Some(Value::String(rule)) => Some(rule.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(flagged, expected, "configs of {rules:?}");
    }
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_frame_shorter_than_its_length_is_a_fatal_error() {
    let dir = fixture("short-frame");
    let mut server = Server::start(&dir);
    // A corrupt header announces 4 GiB of JSON; then the input ends.
    let mut header = u32::MAX.to_le_bytes().to_vec();
    header.push(DID_OPEN);
    header.extend(b"{}");
    server.send_frames(&[header]);
    drop(server.stdin.take());
    assert!(fatal_error(server.read()).starts_with("error reading frame"));
    assert!(server.read().is_none());
    assert_eq!(server.exit_code(), 1);
}

#[test]
fn an_open_file_is_parsed_by_its_extension_in_any_case() {
    let dir = fixture("extension-case");
    // JSDoc types apply in JavaScript only: parsed as TypeScript, `p` would be `any`.
    let text = "/** @type {Promise<number>} */\nexport let p;\np;\n";
    for name in ["c.js", "d.JS"] {
        std::fs::write(dir.join(name), text).unwrap();
        let file = path(&dir, name);
        let mut server = Server::start(&dir);
        assert_eq!(
            rules_of(&server.lint(&[&file]), &file),
            ["no-floating-promises"],
            "{name} on disk"
        );
        // Text other than the disk's, so the open file is parsed again.
        server.open(&file, 1, &format!("// open\n{text}"));
        assert_eq!(
            rules_of(&server.lint(&[&file]), &file),
            ["no-floating-promises"],
            "{name} open"
        );
        assert_eq!(server.exit_code(), 0);
    }
}

#[test]
fn a_projects_tsconfig_errors_are_reported_once_and_its_files_not_linted() {
    let dir = fixture("tsconfig-errors-once");
    let (a, b, tsconfig) = (path(&dir, "a.ts"), path(&dir, "b.ts"), path(&dir, "tsconfig.json"));
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "strict": true, "target": "not-a-target" }, "include": ["*.ts"] }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("a.ts"),
        "export async function f(): Promise<number> { return 1; }\nf();\n",
    )
    .unwrap();
    let mut server = Server::start(&dir);
    let answer = server.lint(&[&a, &b]);
    assert_eq!(rules_of(&answer, &a), NONE, "{answer:?}");
    assert_eq!(rules_of(&answer, &b), NONE, "{answer:?}");
    assert_eq!(rules_of(&answer, &tsconfig), [""], "one tsconfig error: {answer:?}");
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn names_that_differ_in_case_name_one_file_where_the_file_system_folds_case() {
    let dir = fixture("open-case");
    let (b, upper) = (path(&dir, "b.ts"), path(&dir, "B.TS"));
    let mut server = Server::start(&dir);
    if !server_folds_case() {
        // A case-sensitive file system: `B.TS` is another file, which neither opens nor closes `b.ts`.
        server.open(&upper, 1, "export {};\nPromise.resolve(1);\n");
        assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
        server.open(&b, 1, "export {};\n");
        server.send(DID_CLOSE, &protocol::object([("path", Value::String(upper.clone()))]));
        assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
        assert_eq!(server.exit_code(), 0);
        return;
    }
    server.open(&upper, 1, "export {};\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    server.change(&b, 2, "export {};\nPromise.resolve(1);\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    server.change(&upper, 3, "export {};\n");
    assert_eq!(rules_of(&server.lint(&[&b]), &b), NONE);
    // Closed under the other name: `b.ts` is read from disk again.
    server.send(DID_CLOSE, &protocol::object([("path", Value::String(b.clone()))]));
    assert_eq!(rules_of(&server.lint(&[&b]), &b), ["no-floating-promises"]);
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn several_closed_files_outside_every_tsconfig_are_linted() {
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("tsrs-lint-server").join("several-inferred");
    let dir = project(&dir, ASYNC_A);
    std::fs::remove_file(dir.join("tsconfig.json")).unwrap();
    std::fs::write(dir.join("c.ts"), CALLS_A).unwrap();
    let (b, c) = (path(&dir, "b.ts"), path(&dir, "c.ts"));
    let mut server = Server::start(&dir);
    // The session's inferred project holds the open files and one closed file at a time.
    let answer = server.lint(&[&b, &c]);
    assert_eq!(rules_of(&answer, &b), ["no-floating-promises"], "{answer:?}");
    assert_eq!(rules_of(&answer, &c), ["no-floating-promises"], "{answer:?}");
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn a_file_without_a_project_is_linted_once_a_notification_gives_it_one() {
    let dir = fixture("no-project");
    let (b, c) = (path(&dir, "b.ts"), path(&dir, "c.ts"));
    let _ = std::fs::remove_file(dir.join("c.ts"));
    let mut server = Server::start(&dir);
    // `c.ts` does not exist: it has no project, and later requests skip it.
    for _ in 0..2 {
        let answer = server.lint(&[&b, &c]);
        assert_eq!(rules_of(&answer, &b), ["no-floating-promises"]);
        assert_eq!(rules_of(&answer, &c), NONE);
    }
    // An edit of an open file gives no other file a project.
    server.open(&b, 1, CALLS_A);
    server.change(&b, 2, CALLS_A);
    assert_eq!(rules_of(&server.lint(&[&c]), &c), NONE);
    // Created on disk.
    std::fs::write(dir.join("c.ts"), CALLS_A).unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&c, 1)]));
    assert_eq!(rules_of(&server.lint(&[&b, &c]), &c), ["no-floating-promises"]);
    // Deleted, then opened: an open file has a project even when it is not on disk.
    std::fs::remove_file(dir.join("c.ts")).unwrap();
    server.send(FILES_CHANGED, &files_changed(&[(&c, 3)]));
    assert_eq!(rules_of(&server.lint(&[&c]), &c), NONE);
    server.open(&c, 1, CALLS_A);
    assert_eq!(rules_of(&server.lint(&[&c]), &c), ["no-floating-promises"]);
    assert_eq!(server.exit_code(), 0);
}
