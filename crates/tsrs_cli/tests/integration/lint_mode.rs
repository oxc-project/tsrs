use std::io::Write;
use std::process::{Command, Stdio};

use tsrs_core::json::{self, JsonMap, Value};
use tsrs_lint::protocol;

// `tsrs --lint` and the `tsrs-lint` binary both call `tsrs_lint::cli::run`; the subprocess must write the same frames
// and exit status as that function in process.
fn compare(input: &str, args: &[&str]) -> (i32, Vec<u8>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .arg("--lint")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let input = input.to_owned();
    let (status, frames) = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            let mut frames = Vec::new();
            (tsrs_lint::cli::run(owned, &mut input.as_bytes(), &mut frames), frames)
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(status),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, frames, "{args:?}");
    (status, frames)
}

fn frame_types(mut bytes: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    while !bytes.is_empty() {
        let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        types.push(bytes[4]);
        bytes = &bytes[5 + len..];
    }
    types
}

#[test]
fn lint_mode_writes_the_same_frames_as_the_linter() {
    let base = std::env::current_dir().unwrap().join("target/lint-mode");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    let mut overrides = JsonMap::default();
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"include":["*.ts"]}"#.into()),
    );
    overrides.insert(
        file.clone(),
        Value::String(
            "declare function load(): Promise<number>;\nexport async function run() {\n  load();\n  await 1;\n}\n".into(),
        ),
    );
    let rules = ["no-floating-promises", "await-thenable"]
        .map(|name| protocol::object([("name", Value::String(name.into()))]));
    let payload = json::marshal(&protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file)])),
                ("rules", Value::Array(rules.to_vec())),
            ])]),
        ),
        ("source_overrides", Value::Object(overrides)),
    ]))
    .unwrap();
    for args in [&["headless"][..], &["headless", "-fix", "-fix-suggestions"]] {
        let (status, frames) = compare(&payload, args);
        assert_eq!(status, 0, "{args:?}");
        let diagnostic = protocol::MessageType::Diagnostic as u8;
        assert_eq!(frame_types(&frames), [diagnostic, diagnostic], "{args:?}");
    }
    let (status, frames) = compare("{", &["headless"]);
    assert_eq!(status, 1);
    assert_eq!(frame_types(&frames), [protocol::MessageType::Error as u8]);
    assert_eq!(compare("", &[]), (1, Vec::new()));
}
