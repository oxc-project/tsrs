// Regression test for https://github.com/oxc-project/tsrs/issues/1015.
// When stdout is a pipe in O_NONBLOCK mode, tsrs must still print all of its
// output (as the TypeScript Go compiler does) instead of stopping once the
// pipe buffer fills.

#![cfg(unix)]

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

// Enough errors that the output comfortably exceeds the Linux default pipe
// buffer size (64 KiB) and exercises the poll/retry path.
const ERROR_COUNT: usize = 3000;
// Enough files that the --listFilesOnly listing exceeds 64 KiB while keeping
// the test fast.
const FILES_COUNT: usize = 2000;

#[test]
fn all_diagnostics_are_printed_to_a_non_blocking_stdout() {
    let dir = new_temp_dir("diagnostics");
    let mut a = String::new();
    for i in 0..ERROR_COUNT {
        let _ = writeln!(a, "export const v{i}: number = \"x\";");
    }
    fs::write(dir.join("a.ts"), a).unwrap();
    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"files":["a.ts"]}"#,
    )
    .unwrap();

    for mode in [PipeMode::Blocking, PipeMode::NonBlocking] {
        let out = run_tsrs_with_pipe(&dir, &["-p", ".", "--pretty", "false"], mode);
        let diagnostics = String::from_utf8_lossy(&out.stdout).matches("error TS").count();
        assert_eq!(
            diagnostics, ERROR_COUNT,
            "{mode:?}: expected all {ERROR_COUNT} diagnostics, got {diagnostics}"
        );
        assert!(
            out.status.success() || out.status.code() == Some(2),
            "{mode:?}: unexpected exit {status:?}",
            status = out.status
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn list_files_only_is_complete_on_a_non_blocking_stdout() {
    let dir = new_temp_dir("listfiles");
    for i in 0..FILES_COUNT {
        fs::write(
            dir.join(format!("module_with_a_reasonably_long_name_{i:04}.ts")),
            format!("export const v{i} = {i};\n"),
        )
        .unwrap();
    }
    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"include":["*.ts"]}"#,
    )
    .unwrap();

    let blocking = run_tsrs_with_pipe(&dir, &["-p", ".", "--listFilesOnly"], PipeMode::Blocking);
    let nonblocking =
        run_tsrs_with_pipe(&dir, &["-p", ".", "--listFilesOnly"], PipeMode::NonBlocking);

    for (label, out) in [("Blocking", &blocking), ("NonBlocking", &nonblocking)] {
        assert!(
            out.status.success(),
            "{label}: expected exit 0, got {status:?}",
            status = out.status
        );
        assert!(
            out.stdout.len() > 65_536,
            "{label}: --listFilesOnly output ({n} bytes) should exceed the pipe buffer",
            n = out.stdout.len()
        );
    }

    let blocking_lines: Vec<_> = String::from_utf8_lossy(&blocking.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();
    let nonblocking_lines: Vec<_> = String::from_utf8_lossy(&nonblocking.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();

    assert_eq!(
        blocking_lines, nonblocking_lines,
        "blocking and non-blocking --listFilesOnly output should match exactly"
    );

    // Every generated file must appear in the listing.
    let listing = blocking_lines.join("\n");
    for i in 0..FILES_COUNT {
        assert!(
            listing.contains(&format!("module_with_a_reasonably_long_name_{i:04}.ts")),
            "generated file {i:04}.ts missing from listing"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[derive(Debug, Clone, Copy)]
enum PipeMode {
    Blocking,
    NonBlocking,
}

fn new_temp_dir(label: &str) -> std::path::PathBuf {
    let suffix =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "tsrs_nonblocking_stdout_{label}_{}_{}",
        std::process::id(),
        suffix
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_tsrs_with_pipe(dir: &std::path::Path, args: &[&str], mode: PipeMode) -> Output {
    let (mut reader, writer) = std::io::pipe().unwrap();

    if matches!(mode, PipeMode::NonBlocking) {
        let flags = fcntl_getfl(&writer).unwrap();
        fcntl_setfl(&writer, flags | OFlags::NONBLOCK).unwrap();
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::from(writer))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Let the pipe buffer fill before we start draining it.
    std::thread::sleep(Duration::from_millis(500));

    let mut stdout = Vec::new();
    reader.read_to_end(&mut stdout).unwrap();

    let status = child.wait().unwrap();

    Output { stdout, status }
}

struct Output {
    stdout: Vec<u8>,
    status: std::process::ExitStatus,
}
