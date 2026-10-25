use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn case() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/regressions/base-type-cycle-entry")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-fast-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsrs"));
    command.current_dir(dir);
    for name in [
        "TSRS_CHECKER_ASSIGNMENT",
        "TSRS_HISTORY",
        "TSRS_LAZY_MEMBERS",
        "TSRS_LAZY_DTS",
        "TSRS_FILE_TIMES",
        "TSRS_MAX_MEMORY",
        "TSRS_CHECKER_COST_CACHE",
    ] {
        command.env_remove(name);
    }
    command
}

fn output(result: Output, code: i32) -> String {
    assert_eq!(result.status.code(), Some(code), "{result:?}");
    assert!(result.stderr.is_empty(), "{result:?}");
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn default_history_matches_typescript_and_fast_is_overridable() {
    // The pinned Go compiler reports nothing at its default four checkers. Canonical history reports TS2322.
    for flags in [
        vec![],
        vec!["--fast", "false"],
        vec!["--fast", "--fast", "false"],
        vec!["--fast", "--checkerAssignment", "go"],
        vec!["--checkerAssignment", "go", "--fast"],
    ] {
        let result =
            command(&case()).args(["-p", ".", "--pretty", "false"]).args(&flags).output().unwrap();
        assert_eq!(output(result, 0), "", "{flags:?}");
    }
    let expected = std::fs::read_to_string(case().join("expected.txt")).unwrap();
    for flags in [vec!["--fast"], vec!["--fast", "true"], vec!["--FAST"], vec!["-fast"]] {
        let result =
            command(&case()).args(["-p", ".", "--pretty", "false"]).args(&flags).output().unwrap();
        assert_eq!(output(result, 2), expected, "{flags:?}");
    }
}

#[test]
fn preset_controls_checker_count_and_lazy_members() {
    let dir = scratch("counts");
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"noEmit":true,"types":[],"skipLibCheck":true},"include":["*.ts"]}"#,
    )
    .unwrap();
    for i in 0..1024 {
        std::fs::write(dir.join(format!("file{i}.ts")), format!("export const value{i} = {i};\n"))
            .unwrap();
    }
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let adaptive = (cores / 2).max(cores.min(16)).clamp(4, 32);
    let times = dir.join("times.tsv");
    for (flags, count, lazy) in [
        (vec![], 4, false),
        (vec!["--fast", "false"], 4, false),
        (vec!["--fast", "--checkerAssignment", "locality"], adaptive, true),
        (vec!["--fast", "--checkers", "2", "--noLazyMembers"], 2, false),
        (vec!["--noLazyMembers", "--checkers", "2", "--fast"], 2, false),
        (vec!["--fast", "--singleThreaded"], 1, true),
        (vec!["--lazyMembers"], 4, true),
    ] {
        let result = command(&dir)
            .args(["-p", ".", "--pretty", "false", "--extendedDiagnostics"])
            .args(&flags)
            .env("TSRS_FILE_TIMES", &times)
            .output()
            .unwrap();
        let text = output(result, 0);
        assert_eq!(text.contains("Lazy member tables:"), lazy, "{flags:?}");
        let times = std::fs::read_to_string(&times).unwrap();
        let checkers: BTreeSet<_> =
            times.lines().map(|line| line.split('\t').next().unwrap()).collect();
        assert_eq!(checkers.len(), count, "{flags:?}: {checkers:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn response_files_builds_and_incremental_mode_switches() {
    let dir = scratch("incremental");
    for entry in std::fs::read_dir(case()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|ext| ext == "ts" || ext == "json") {
            std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
        }
    }
    let config = std::fs::read_to_string(dir.join("tsconfig.json")).unwrap();
    // Preserve the fixture's roots and options while enabling persistence.
    std::fs::write(
        dir.join("tsconfig.json"),
        config.replace("\"strict\":true", "\"strict\":true,\"incremental\":true"),
    )
    .unwrap();
    std::fs::write(dir.join("args.txt"), "--fast true --pretty false").unwrap();
    let expected = std::fs::read_to_string(case().join("expected.txt")).unwrap();
    for build in [false, true] {
        for fast in [false, true, true, false, false, true] {
            let mut command = command(&dir);
            command.args(if build { ["-b", "."] } else { ["-p", "."] });
            command.arg("@args.txt");
            if !fast {
                command.args(["--fast", "false"]);
            }
            let actual = output(command.output().unwrap(), if fast { 2 } else { 0 });
            assert_eq!(actual, if fast { &expected } else { "" }, "build={build}, fast={fast}");
            let info = std::fs::read_to_string(dir.join("tsconfig.tsbuildinfo")).unwrap();
            assert_eq!(info.contains("tsrsCanonicalCheckSignature"), fast);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_cache_tracks_overrides_and_keeps_warm_builds() {
    let dir = scratch("build-cache");
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"types":[],"outDir":"out"},"files":["a.ts"]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("a.ts"), "export const value = 1;\n").unwrap();
    let info = dir.join("out/tsconfig.tsbuildinfo");
    for (fast, lazy) in [(false, false), (true, true), (true, false), (false, true), (false, false)]
    {
        let run = || {
            output(
                command(&dir)
                    .args([
                        "-b",
                        ".",
                        "--pretty",
                        "false",
                        "--verbose",
                        "--fast",
                        if fast { "true" } else { "false" },
                    ])
                    .env("TSRS_LAZY_MEMBERS", if lazy { "1" } else { "0" })
                    .output()
                    .unwrap(),
                0,
            )
        };
        let cold = run();
        assert!(cold.contains("out of date"), "{cold}");
        let text = std::fs::read_to_string(&info).unwrap();
        let mode = match (fast, lazy) {
            (false, false) => "",
            (false, true) => "lazy",
            (true, false) => "canonical",
            (true, true) => "canonical,lazy",
        };
        if mode.is_empty() {
            assert!(!text.contains("tsrsPerformance"), "{text}");
        } else {
            assert!(text.contains(&format!("\"tsrsPerformance\":\"{mode}\"")), "{text}");
        }
        let modified = std::fs::metadata(&info).unwrap().modified().unwrap();
        let warm = run();
        assert!(warm.contains("up to date"), "{warm}");
        assert_eq!(std::fs::metadata(&info).unwrap().modified().unwrap(), modified);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
