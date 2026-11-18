// On Linux a batch compile hands its address space to a helper process just before it exits (src/exit_helper.rs), so
// the kernel frees the memory after the caller has seen the exit. The exit status and the output must not change, and
// the helper must be gone a moment after the process. The test process makes itself a child subreaper, so the
// orphaned helper becomes its child and can be counted; that is process-wide state, hence `isolated::in_child`.

#![cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(feature = "system-alloc")))]

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn tsrs(dir: &Path, helper: &str, extra: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsrs"));
    command
        .args(["-p", ".", "--pretty", "false"])
        .args(extra)
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .env("TSRS_EXIT_HELPER", helper);
    command
}

/// Reaps the processes this subreaper has been handed: waits up to `wait` for the first one, then takes the rest
/// that have exited. Returns each one's pid and wait status.
fn reap_orphans(wait: Duration) -> Vec<(i32, i32)> {
    let deadline = Instant::now() + wait;
    let mut reaped = Vec::new();
    loop {
        let mut status = 0;
        // SAFETY: `status` is valid for the call.
        let pid = unsafe { libc::waitpid(-1, &raw mut status, libc::WNOHANG) };
        if pid > 0 {
            reaped.push((pid, status));
            continue;
        }
        // -1: no children left (ECHILD); 0: a child is still running.
        if pid < 0 || !reaped.is_empty() || Instant::now() >= deadline {
            return reaped;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn assert_same_output(a: &Output, b: &Output) {
    assert_eq!(a.status.code(), b.status.code());
    assert!(a.stdout == b.stdout, "stdout differs");
    assert!(a.stderr == b.stderr, "stderr differs");
}

#[test]
fn helper_keeps_status_and_output_and_exits() {
    if !crate::isolated::in_child("exit_helper::helper_keeps_status_and_output_and_exits") {
        return;
    }
    // SAFETY: prctl with integer arguments; makes this process the reaper of its orphaned descendants.
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) }, 0);
    let dir = std::env::temp_dir().join(format!("tsrs-exit-helper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true }, "files": ["a.ts"] }"#,
    )
    .unwrap();
    // More diagnostics text than a pipe buffer holds, so the output is written while the reader drains it.
    let source = (0..3_000).fold(String::new(), |mut out, i| {
        writeln!(out, "const v{i}: number = \"s\";").unwrap();
        out
    });
    std::fs::write(dir.join("a.ts"), source).unwrap();

    // Off: no helper.
    let off = tsrs(&dir, "0", &[]).output().unwrap();
    assert_eq!(off.status.code(), Some(2), "{}", String::from_utf8_lossy(&off.stderr));
    assert!(off.stdout.len() > 64 << 10);
    assert_eq!(reap_orphans(Duration::from_millis(200)), []);

    // On, stdout and stderr pipes: the same status and bytes, and one helper that exits by itself right after.
    // `output` reads both pipes to the end, so it also returns only once the helper has closed its copies.
    let on = tsrs(&dir, "1", &[]).output().unwrap();
    assert_same_output(&off, &on);
    let helpers = reap_orphans(Duration::from_secs(10));
    assert_eq!(helpers.len(), 1, "{helpers:?}");
    let (_, status) = helpers[0];
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0, "{status:#x}");

    // On, stdout a file and stdin closed.
    let path = dir.join("out.txt");
    let file = std::fs::File::create(&path).unwrap();
    let status = tsrs(&dir, "1", &[]).stdin(Stdio::null()).stdout(file).status().unwrap();
    assert_eq!(status.code(), Some(2));
    assert!(std::fs::read(&path).unwrap() == off.stdout, "file output differs");
    assert_eq!(reap_orphans(Duration::from_secs(10)).len(), 1);

    // Fast mode starts it only above the size threshold; `--version` is far below it.
    let version = tsrs(&dir, "", &["--fast", "--version"]).output().unwrap();
    assert_eq!(version.status.code(), Some(0));
    assert_eq!(reap_orphans(Duration::from_millis(200)), []);

    let _ = std::fs::remove_dir_all(&dir);
}
