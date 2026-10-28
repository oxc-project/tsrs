// Node >= 22.15 may execve the tsrs binary with stdout/stderr in O_NONBLOCK.
// With that flag set, writes that hit a full pipe return EAGAIN; tsrs must
// still deliver all diagnostics, matching Go's handling of os.Stdout.

#![cfg(unix)]

use std::fmt::Write as _;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
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

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
    cmd.args(["-p", ".", "--pretty", "false"]).current_dir(&dir).stdout(Stdio::piped());
    let mut child = spawn_nonblocking(&mut cmd, libc::STDOUT_FILENO);

    let stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || read_slowly(stdout));

    let status = child.wait().unwrap();
    let output = reader.join().unwrap();
    let text = String::from_utf8_lossy(&output);
    let errors = text.lines().filter(|l| l.contains("error TS2322")).count();
    assert_eq!(errors, COUNT, "diagnostics were dropped; stdout output was {} bytes", output.len());
    assert_eq!(status.code(), Some(2), "expected exit code 2 with errors: {status:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nonblocking_stderr_preserves_server_flag_errors() {
    // A single flag fits within Unix argument limits but its diagnostic exceeds a pipe buffer.
    let flag_name = "x".repeat(96 * 1024);
    let flag = format!("--{flag_name}");
    for mode in ["--lsp", "--api"] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
        cmd.args([mode, &flag]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let expected = cmd.output().unwrap();
        assert_eq!(expected.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&expected.stderr).contains(&flag_name));

        let mut child = spawn_nonblocking(&mut cmd, libc::STDERR_FILENO);
        let stderr = child.stderr.take().unwrap();
        let reader = thread::spawn(move || read_slowly(stderr));
        let status = child.wait().unwrap();
        let output = reader.join().unwrap();
        assert_eq!(status.code(), Some(2), "{mode}: {status:?}");
        assert_eq!(output, expected.stderr, "{mode}: stderr was truncated");
    }
}

fn spawn_nonblocking(cmd: &mut Command, fd: i32) -> Child {
    // SAFETY: `pre_exec` runs only between fork and exec in the child, and the
    // closure only calls async-signal-safe fcntl and reads errno on failure.
    unsafe {
        cmd.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFL, 0);
            if flags == -1 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn().unwrap()
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
