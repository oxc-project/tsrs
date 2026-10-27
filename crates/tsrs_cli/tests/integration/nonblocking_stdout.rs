// Node >= 22.15 may execve the tsrs binary with stdout/stderr in O_NONBLOCK.
// With that flag set, writes that hit a full pipe return EAGAIN; tsrs must
// still deliver all diagnostics, matching Go's handling of os.Stdout.

#![cfg(unix)]

use std::fmt::Write as _;
use std::io::Read;
use std::os::unix::process::CommandExt;
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

    // SAFETY: `pre_exec` runs only between fork and exec in the child and the
    // supplied closure uses only the async-signal-safe `fcntl` calls.
    let mut child = unsafe {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
        cmd.args(["-p", ".", "--pretty", "false"])
            .current_dir(&dir)
            .stdout(Stdio::piped());
        // Force the same condition Node creates when it execves us into a pipe.
        cmd.pre_exec(|| {
            let flags = libc::fcntl(libc::STDOUT_FILENO, libc::F_GETFL, 0);
            if flags != -1 {
                libc::fcntl(libc::STDOUT_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
            Ok(())
        });
        cmd.spawn().unwrap()
    };

    let mut stdout = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut output = Vec::with_capacity(128 * 1024);
        loop {
            // Throttle the reader so the pipe fills and a non-blocking writer
            // would see EAGAIN.
            thread::sleep(Duration::from_millis(50));
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
            }
        }
        output
    });

    let status = child.wait().unwrap();
    let output = reader.join().unwrap();
    let text = String::from_utf8_lossy(&output);
    let errors = text.lines().filter(|l| l.contains("error TS2322")).count();
    assert_eq!(errors, COUNT, "diagnostics were dropped; stdout output was {} bytes", output.len());
    assert_eq!(status.code(), Some(2), "expected exit code 2 with errors: {status:?}");

    let _ = std::fs::remove_dir_all(&dir);
}
