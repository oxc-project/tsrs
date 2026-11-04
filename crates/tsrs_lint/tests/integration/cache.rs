// The lint result cache (src/cache.rs): a warm run prints what a run without the cache prints, linting only the files
// whose semantic diagnostics TypeScript's incremental builder would invalidate.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tsrs_core::json::{self, Value};

const TSCONFIG: &str = r#"{"compilerOptions":{"strict":true,"target":"esnext","module":"esnext","types":[]},"include":["*.ts"]}"#;

struct Project {
    root: PathBuf,
    cache: PathBuf,
}

impl Project {
    fn new(name: &str, files: &[(&str, &str)]) -> Project {
        let base = std::env::current_dir().unwrap().join("target/lint-cache").join(name);
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("project");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("tsconfig.json"), TSCONFIG).unwrap();
        let project = Project { root, cache: base.join("cache") };
        for (name, text) in files {
            project.write(name, text);
        }
        project
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.root.join(name), text).unwrap();
    }

    fn payload(&self, files: &[&str], rules: &str, report_semantic: bool) -> String {
        let paths: Vec<Value> = files
            .iter()
            .map(|name| Value::String(self.root.join(name).to_string_lossy().into_owned()))
            .collect();
        format!(
            r#"{{"version":2,"report_semantic":{report_semantic},"configs":[{{"file_paths":{},"rules":{rules}}}]}}"#,
            json::marshal(&Value::Array(paths)).unwrap()
        )
    }

    /// Lints with the cache: stdout, and the number of files linted and replayed.
    fn lint(&self, payload: &str) -> (Vec<u8>, (usize, usize)) {
        let (stdout, stderr) = run(
            payload,
            &[
                ("TSRS_FAST", "1"),
                ("TSRS_LINT_CACHE", self.cache.to_str().unwrap()),
                ("TSRS_LINT_CACHE_STATS", "1"),
            ],
        );
        let stats = stderr
            .strip_prefix("tsrs-lint cache: ")
            .and_then(|line| line.trim_end().split_once(" linted, "))
            .and_then(|(linted, replayed)| {
                Some((linted.parse().ok()?, replayed.strip_suffix(" replayed")?.parse().ok()?))
            })
            .unwrap_or_else(|| panic!("no cache statistics: {stderr}"));
        (stdout, stats)
    }

    /// Lints with the cache and checks the output against a run without it.
    fn lint_checked(&self, payload: &str) -> (Vec<u8>, (usize, usize)) {
        let (stdout, stats) = self.lint(payload);
        let (uncached, stderr) = run(payload, &[("TSRS_FAST", "1")]);
        assert_eq!(stderr, "");
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&uncached),
            "cached output differs from a run without the cache"
        );
        (stdout, stats)
    }

    fn cache_files(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.cache).map_or_else(
            |_| Vec::new(),
            |entries| entries.map(|entry| entry.unwrap().path()).collect(),
        )
    }
}

fn run(payload: &str, env: &[(&str, &str)]) -> (Vec<u8>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs-lint"))
        .env_remove("TSRS_FAST")
        .env_remove("TSRS_LINT_CACHE")
        .env_remove("TSRS_LINT_CACHE_STATS")
        .envs(env.iter().copied())
        .arg("headless")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    (output.stdout, String::from_utf8(output.stderr).unwrap())
}

/// The rule ids reported per file name, in output order.
fn reports(stdout: &[u8]) -> Vec<(String, String)> {
    let mut reports = Vec::new();
    let mut bytes = stdout;
    while !bytes.is_empty() {
        let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let Value::Object(frame) =
            json::unmarshal(std::str::from_utf8(&bytes[5..5 + len]).unwrap()).unwrap()
        else {
            panic!()
        };
        let Value::String(file) = &frame["file_path"] else { panic!() };
        let name = Path::new(file).file_name().unwrap().to_string_lossy().into_owned();
        let id = match (&frame.get("rule"), &frame["message"]) {
            (Some(Value::String(rule)), _) => rule.clone(),
            (_, Value::Object(message)) => {
                let Value::String(id) = &message["id"] else { panic!() };
                id.clone()
            }
            _ => panic!(),
        };
        reports.push((name, id));
        bytes = &bytes[5 + len..];
    }
    reports
}

fn report(file: &str, id: &str) -> (String, String) {
    (file.into(), id.into())
}

const FLOATING: &str = r#"[{"name":"no-floating-promises"}]"#;
const PROMISE: &str = "export function f(): Promise<number> { return Promise.resolve(1); }\n";

// a.ts exports `f`, b.ts calls it and re-exports it, c.ts calls the re-export, d.ts depends on nothing.
fn chain(name: &str) -> Project {
    Project::new(
        name,
        &[
            ("a.ts", PROMISE),
            ("b.ts", "import { f } from './a';\nexport { f };\nf();\n"),
            ("c.ts", "import { f } from './b';\nf();\n"),
            ("d.ts", "export const d = 1;\n"),
        ],
    )
}

const CHAIN: [&str; 4] = ["a.ts", "b.ts", "c.ts", "d.ts"];

#[test]
fn warm_run_without_changes_replays_every_file() {
    let project = chain("unchanged");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let (cold, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (4, 0));
    assert_eq!(
        reports(&cold),
        [report("b.ts", "no-floating-promises"), report("c.ts", "no-floating-promises")]
    );
    assert_eq!(project.cache_files().len(), 1);
    let (warm, stats) = project.lint(&payload);
    assert_eq!(stats, (0, 4));
    assert_eq!(warm, cold);
}

#[test]
fn body_only_edit_relints_only_the_edited_file() {
    let project = chain("body-edit");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let (cold, _) = project.lint_checked(&payload);
    project.write("a.ts", "export function f(): Promise<number> { return Promise.resolve(2); }\n");
    let (warm, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (1, 3));
    assert_eq!(warm, cold);
    // The edit's signature replaces the stand-in, so a second body edit relints only the file too.
    project.write("a.ts", "export function f(): Promise<number> { return Promise.resolve(3); }\n");
    assert_eq!(project.lint_checked(&payload).1, (1, 3));
}

#[test]
fn exported_type_change_relints_importers_and_their_semantic_diagnostics() {
    let project = Project::new(
        "type-change",
        &[
            ("a.ts", PROMISE),
            ("b.ts", "import { f } from './a';\nf();\nexport const n: number = f();\n"),
            ("d.ts", "export const d = 1;\n"),
        ],
    );
    let files = ["a.ts", "b.ts", "d.ts"];
    let payload = project.payload(&files, FLOATING, true);
    let (cold, _) = project.lint_checked(&payload);
    assert_eq!(reports(&cold), [report("b.ts", "TS2322"), report("b.ts", "no-floating-promises")]);
    project.write("a.ts", "export function f(): number { return 1; }\n");
    let (warm, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (2, 1));
    assert_eq!(reports(&warm), []);
    project.write("a.ts", PROMISE);
    let (warm, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (2, 1));
    assert_eq!(warm, cold);
}

#[test]
fn type_change_reaches_files_through_a_reexport_chain() {
    let project = chain("reexport");
    // b.ts re-exports without a declaration of its own, so its signature never changes.
    project.write("b.ts", "export * from './a';\n");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let (cold, _) = project.lint_checked(&payload);
    assert_eq!(reports(&cold), [report("c.ts", "no-floating-promises")]);
    project.write("a.ts", "export function f(): Promise<number> { return Promise.resolve(2); }\n");
    assert_eq!(project.lint_checked(&payload).1, (1, 3));
    project.write("a.ts", "export function f(): number { return 1; }\n");
    let (warm, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (3, 1));
    assert_eq!(reports(&warm), []);
}

#[test]
fn global_declaration_change_relints_every_file() {
    let project = chain("global");
    project.write("globals.d.ts", "declare function g(): Promise<void>;\n");
    project.write("d.ts", "g();\nexport {};\n");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let (cold, _) = project.lint_checked(&payload);
    assert_eq!(reports(&cold).len(), 3);
    project.write("globals.d.ts", "declare function g(): void;\n");
    let (warm, stats) = project.lint_checked(&payload);
    assert_eq!(stats, (4, 0));
    assert_eq!(
        reports(&warm),
        [report("b.ts", "no-floating-promises"), report("c.ts", "no-floating-promises")]
    );
}

#[test]
fn rule_option_change_relints_every_file() {
    let project = chain("options");
    project.write("d.ts", "void Promise.resolve(1);\nexport {};\n");
    let (cold, _) = project.lint_checked(&project.payload(&CHAIN, FLOATING, false));
    assert_eq!(reports(&cold).len(), 2);
    let ignore_void = r#"[{"name":"no-floating-promises","options":{"ignoreVoid":false}}]"#;
    let (warm, stats) = project.lint_checked(&project.payload(&CHAIN, ignore_void, false));
    assert_eq!(stats, (4, 0));
    assert_eq!(reports(&warm).len(), 3);
    assert_eq!(project.lint(&project.payload(&CHAIN, ignore_void, false)).1, (0, 4));
}

#[test]
fn added_and_deleted_files() {
    let project = chain("added-deleted");
    project.lint_checked(&project.payload(&CHAIN, FLOATING, false));
    project.write("e.ts", "import { f } from './a';\nf();\n");
    let with_e = ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts"];
    let (warm, stats) = project.lint_checked(&project.payload(&with_e, FLOATING, false));
    assert_eq!(stats, (1, 4));
    assert_eq!(reports(&warm).len(), 3);
    std::fs::remove_file(project.root.join("e.ts")).unwrap();
    assert_eq!(project.lint_checked(&project.payload(&CHAIN, FLOATING, false)).1, (0, 4));
}

#[test]
fn deleted_dependency_relints_its_importers() {
    let project = Project::new(
        "deleted",
        &[
            ("a.ts", PROMISE),
            ("e.ts", "import { f } from './a';\nf();\n"),
            ("d.ts", "export {};\n"),
        ],
    );
    let (cold, _) =
        project.lint_checked(&project.payload(&["a.ts", "e.ts", "d.ts"], FLOATING, false));
    assert_eq!(reports(&cold), [report("e.ts", "no-floating-promises")]);
    // Without a.ts, `f` is an error type.
    std::fs::remove_file(project.root.join("a.ts")).unwrap();
    let (warm, stats) = project.lint_checked(&project.payload(&["e.ts", "d.ts"], FLOATING, false));
    assert_eq!(stats, (1, 1));
    assert_eq!(reports(&warm), []);
}

#[test]
fn corrupt_cache_file_means_a_cold_run() {
    let project = chain("corrupt");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let (cold, _) = project.lint_checked(&payload);
    let [file] = project.cache_files().try_into().unwrap();
    let data = std::fs::read(&file).unwrap();
    for corrupt in [&data[..data.len() - 3], &data[..40], b"tsrs-lint-cache 1\n", &[0xff; 64][..]] {
        std::fs::write(&file, corrupt).unwrap();
        let (output, stats) = project.lint(&payload);
        assert_eq!(stats, (4, 0));
        assert_eq!(output, cold);
    }
    assert_eq!(project.lint(&payload).1, (0, 4));
}

#[test]
fn cache_is_off_without_the_fast_preset_the_directory_or_on_disk_sources() {
    let project = chain("off");
    let payload = project.payload(&CHAIN, FLOATING, false);
    let cache = project.cache.to_str().unwrap();
    for env in [
        vec![("TSRS_LINT_CACHE", cache), ("TSRS_LINT_CACHE_STATS", "1")],
        vec![("TSRS_FAST", "0"), ("TSRS_LINT_CACHE", cache), ("TSRS_LINT_CACHE_STATS", "1")],
        vec![("TSRS_FAST", "1"), ("TSRS_LINT_CACHE_STATS", "1")],
        vec![("TSRS_FAST", "1"), ("TSRS_LINT_CACHE", ""), ("TSRS_LINT_CACHE_STATS", "1")],
    ] {
        let (stdout, stderr) = run(&payload, &env);
        assert_eq!(stderr, "", "{env:?}");
        assert_eq!(reports(&stdout).len(), 2, "{env:?}");
    }
    let overrides = format!(
        r#"{{"version":2,"source_overrides":{{{}:"f();"}},"configs":[{{"file_paths":{},"rules":{FLOATING}}}]}}"#,
        json::marshal(&Value::String(project.root.join("c.ts").to_string_lossy().into_owned()))
            .unwrap(),
        json::marshal(&Value::Array(vec![Value::String(
            project.root.join("c.ts").to_string_lossy().into_owned()
        )]))
        .unwrap(),
    );
    let (_, stderr) = run(
        &overrides,
        &[("TSRS_FAST", "1"), ("TSRS_LINT_CACHE", cache), ("TSRS_LINT_CACHE_STATS", "1")],
    );
    assert_eq!(stderr, "");
    assert_eq!(project.cache_files(), [] as [PathBuf; 0]);
}
