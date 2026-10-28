// The npm launcher must restore blocking stdio before execve or spawning the binary.
// A full non-blocking pipe returns EAGAIN; diagnostics must still reach the reader.

#![cfg(unix)]

use std::fmt::Write as _;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

// Enough diagnostics to overflow a Linux pipe buffer (~64 KiB).
const COUNT: usize = 1_000;

#[test]
fn nonblocking_stdout_does_not_drop_diagnostics() {
    let dir = std::env::temp_dir().join(format!("tsrs-nonblocking-stdout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "files": ["a.ts"] }"#,
    )
    .unwrap();

    let source = (0..COUNT).fold(String::new(), |mut out, i| {
        writeln!(out, "const v{i}: number = \"s\";").unwrap();
        out
    });
    std::fs::write(dir.join("a.ts"), source).unwrap();

    let expected = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", ".", "--pretty", "false"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(expected.status.code(), Some(2));
    assert_eq!(expected.stderr, [] as [u8; 0]);

    for fallback in [false, true] {
        let mut cmd = launcher_command(fallback);
        cmd.args(["-p", ".", "--pretty", "false"]).current_dir(&dir).stdout(Stdio::piped());
        let mut child = cmd.spawn().unwrap();

        let stdout = child.stdout.take().unwrap();
        let reader = thread::spawn(move || read_slowly(stdout));

        let status = child.wait().unwrap();
        let output = reader.join().unwrap();
        let text = String::from_utf8_lossy(&output);
        let errors = text.lines().filter(|l| l.contains("error TS2322")).count();
        assert_eq!(
            errors,
            COUNT,
            "fallback={fallback}: diagnostics were dropped ({} bytes)",
            output.len()
        );
        assert_eq!(status.code(), Some(2), "expected exit code 2 with errors: {status:?}");
        assert_eq!(
            output, expected.stdout,
            "fallback={fallback}: output differs from blocking control"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nonblocking_stderr_preserves_server_flag_errors() {
    // A single flag fits within Unix argument limits but its diagnostic exceeds a pipe buffer.
    let flag_name = "x".repeat(96 * 1024);
    let flag = format!("--{flag_name}");
    for mode in ["--lsp", "--api"] {
        let expected =
            Command::new(env!("CARGO_BIN_EXE_tsrs")).args([mode, &flag]).output().unwrap();
        assert_eq!(expected.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&expected.stderr).contains(&flag_name));

        let mut cmd = launcher_command(false);
        cmd.args([mode, &flag]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let stderr = child.stderr.take().unwrap();
        let reader = thread::spawn(move || read_slowly(stderr));
        let status = child.wait().unwrap();
        let output = reader.join().unwrap();
        assert_eq!(status.code(), Some(2), "{mode}: {status:?}");
        assert_eq!(output, expected.stderr, "{mode}: stderr was truncated");
    }
}

fn launcher_command(fallback: bool) -> Command {
    let mut cmd = Command::new("node");
    // Set flags after Node initializes, so it cannot reset them before our launcher runs.
    cmd.args(["--input-type=module", "-e"])
        .arg(format!(
            r"
process.execve = {fallback} ? undefined : process.execve;
for (const stream of [process.stdin, process.stdout, process.stderr]) {{
    stream._handle?.setBlocking?.(false);
}}
await import(process.argv[1]);
"
        ))
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../npm/tsrs/lib/tsrs.js"))
        .env("TSRS_BINARY", env!("CARGO_BIN_EXE_tsrs"))
        .env_remove("NODE_OPTIONS");
    cmd
}

fn read_slowly(mut reader: impl Read) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let mut output = Vec::with_capacity(128 * 1024);
    loop {
        // Throttle the reader so the pipe fills and a non-blocking writer would see EAGAIN.
        thread::sleep(Duration::from_millis(50));
        let n = reader.read(&mut buf).unwrap();
        if n == 0 {
            return output;
        }
        output.extend_from_slice(&buf[..n]);
    }
}
