// After the `tsconfig.json` of a referenced project `a` changes, Go resets the task of `a` and the host cache of
// `a`'s parsed configuration. The next compilation of the downstream project `b` calls `compiler.NewProgram`, which runs
// `verifyProjectReferences` against the current configuration of `a`, so an edit of `b/b.ts` prints `TS6306` for `b`.
// tsrs reuses the previous program of `b` for a one-file edit (`reuse_watch_program`), which keeps the old resolved
// reference and does not verify the references again, so it prints the type error of `b.ts` and no `TS6306`.
// Expected behavior was taken from tsgo built from the pinned ts-ref commit.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const A_TS: &str = "export const a = 1;\nexport type A = number;\n";
const B_TS: &str = "import { a, A } from \"../a/a\";\nexport const b: A = a;\n";
const B_TS_EDITED: &str = "import { a } from \"../a/a\";\nexport const b: boolean = a;\n";
const COMPOSITE_OPTIONS: &str = r#"{"compilerOptions":{"composite":true,"outDir":"out","declaration":true,"strict":true},"include":["*.ts"]"#;

struct Watch {
    child: Child,
    lines: Receiver<String>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Watch {
    // Starts `tsc -b --watch` and returns after the first build and 1.5 s without output.
    fn start(dir: &Path) -> Watch {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
            .args(["-b", "--watch", "--pretty", "false", "--singleThreaded"])
            .current_dir(dir)
            .env_remove("TSRS_FAST")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (tx, lines) = channel();
        let stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let watch = Watch { child, lines };
        let first = watch.next_cycle();
        assert!(first.iter().any(|l| l.contains("Found 0 errors.")), "first build: {first:?}");
        watch
    }

    // Waits up to 60 s for the next `Watching for file changes` line, then for 1.5 s without output (tsgo can run
    // one more cycle after an edit). Returns the lines up to that point, without the empty lines.
    fn next_cycle(&self) -> Vec<String> {
        let mut out = Vec::new();
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let left = end.checked_duration_since(Instant::now()).unwrap_or_default();
            match self.lines.recv_timeout(left) {
                Ok(line) if line.trim().is_empty() => {}
                Ok(line) => {
                    let done = line.contains("Watching for file changes");
                    out.push(line);
                    if done {
                        break;
                    }
                }
                Err(error) => {
                    panic!("no `Watching for file changes` line in 60 s ({error}): {out:?}")
                }
            }
        }
        while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(1500)) {
            if !line.trim().is_empty() {
                out.push(line);
            }
        }
        out
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-watch-b-stale-reference-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    let write = |path: &str, text: &str| std::fs::write(dir.join(path), text).unwrap();
    write("tsconfig.json", r#"{"files":[],"references":[{"path":"a"},{"path":"b"}]}"#);
    write("a/tsconfig.json", &format!("{COMPOSITE_OPTIONS}}}"));
    write("a/a.ts", A_TS);
    write(
        "b/tsconfig.json",
        &format!("{COMPOSITE_OPTIONS},\"references\":[{{\"path\":\"../a\"}}]}}"),
    );
    write("b/b.ts", B_TS);
    dir
}

// Starts the watch, writes `a_config` over `a/tsconfig.json`, waits for the rebuild, edits `b/b.ts`, and returns the
// lines that the watch prints after the edit of `b/b.ts`.
fn edit_a_config_then_b(name: &str, a_config: &str) -> Vec<String> {
    let dir = scratch(name);
    let watch = Watch::start(&dir);
    std::fs::write(dir.join("a/tsconfig.json"), a_config).unwrap();
    watch.next_cycle();
    std::fs::write(dir.join("b/b.ts"), B_TS_EDITED).unwrap();
    let lines = watch.next_cycle();
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
    lines
}

fn assert_composite_error(lines: &[String]) {
    // tsgo reports the error on the `references` entry of `b/tsconfig.json`, and not the type error of `b/b.ts`.
    assert!(
        lines.iter().any(|l| l.contains("b/tsconfig.json(")
            && l.contains("error TS6306: Referenced project '")
            && l.contains("must have setting \"composite\": true.")),
        "{lines:?}"
    );
    assert!(!lines.iter().any(|l| l.contains("error TS2322")), "{lines:?}");
}

#[test]
fn edit_of_b_after_the_config_of_a_became_invalid_reports_ts6306() {
    let lines = edit_a_config_then_b("invalid", "{ \"compilerOptions\": { ");
    assert!(lines.iter().any(|l| l.contains("a/tsconfig.json(1,24): error TS1005:")), "{lines:?}");
    assert_composite_error(&lines);
}

#[test]
fn edit_of_b_after_composite_was_turned_off_in_a_reports_ts6306() {
    let lines = edit_a_config_then_b(
        "composite-off",
        r#"{"compilerOptions":{"composite":false,"outDir":"out","declaration":true,"strict":true},"include":["*.ts"]}"#,
    );
    assert_composite_error(&lines);
}
