// The `--fast` result cache (tsrs_execute resultcache.rs, notes/fast-result-cache.md): a `--fast --noEmit` check
// replays its recorded output only while every file system answer it depended on is unchanged. Each test changes one
// input the way an editor, a package manager or a shell does and checks that the next run misses and prints exactly
// what a run without the cache prints (and that an unchanged project hits). Every run is a child process with its own
// cache directory.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

struct Project {
    root: PathBuf,
    dir: PathBuf,
    cache: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct Output {
    stdout: String,
    status: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cache {
    Hit,
    Miss,
    Recorded,
    NotRecorded,
    Off,
    Silent,
}

impl Project {
    /// A project in `<tmp>/<name>/p` (files outside `p` are outside the include glob); paths are relative to `p`.
    fn new(name: &str, files: &[(&str, &str)]) -> Project {
        let root = std::env::temp_dir()
            .join(format!("tsrs-result-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = Project { dir: root.join("p"), cache: root.join("cache"), root };
        std::fs::create_dir_all(&project.dir).unwrap();
        for (path, text) in files {
            project.write(path, text);
        }
        project
    }

    fn path(&self, path: &str) -> PathBuf {
        self.dir.join(path)
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.path(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Waits until the files written so far are older than the cache's two-second racy window, so a recording
    /// vouches for them by their time stamps instead of hashing their text.
    fn settle() {
        std::thread::sleep(Duration::from_millis(2100));
    }

    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
        cmd.current_dir(&self.dir).args(args);
        for (name, _) in std::env::vars() {
            if name.starts_with("TSRS_") || name == "FORCE_COLOR" || name == "NO_COLOR" {
                cmd.env_remove(name);
            }
        }
        cmd.env("TSRS_RESULT_CACHE", "log").env("TSRS_RESULT_CACHE_DIR", &self.cache);
        for (name, value) in env {
            cmd.env(name, value);
        }
        cmd
    }

    /// Runs `tsrs <args>` with the cache, returning its output and what the cache did.
    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> (Output, Cache) {
        let out = self.command(args, env).output().unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        let cache = if stderr.contains("tsrs result cache: hit") {
            Cache::Hit
        } else if stderr.contains("tsrs result cache: recorded") {
            Cache::Recorded
        } else if stderr.contains("tsrs result cache: not recorded") {
            Cache::NotRecorded
        } else if stderr.contains("tsrs result cache: off") {
            Cache::Off
        } else {
            Cache::Silent
        };
        if stderr.contains("tsrs result cache: miss") {
            assert_eq!(cache, Cache::Recorded, "a miss records again: {stderr}");
            return (Output::of(out.stdout, out.status), Cache::Miss);
        }
        (Output::of(out.stdout, out.status), cache)
    }

    /// The same command without the cache.
    fn cold(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let out = self.command(args, env).env("TSRS_RESULT_CACHE", "0").output().unwrap();
        assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
        Output::of(out.stdout, out.status)
    }

    /// Runs with the cache, expecting `cache`, and checks the output against a run without it.
    fn check(&self, cache: Cache) -> Output {
        self.check_with(CHECK, &[], cache)
    }

    fn check_with(&self, args: &[&str], env: &[(&str, &str)], cache: Cache) -> Output {
        let (output, actual) = self.run(args, env);
        assert_eq!(actual, cache, "{args:?} {env:?}: {output:?}");
        assert_eq!(output, self.cold(args, env), "{args:?} {env:?}: the cache changed the output");
        output
    }

    fn entries(&self) -> Vec<PathBuf> {
        let Ok(dir) = std::fs::read_dir(&self.cache) else {
            return Vec::new();
        };
        dir.map(|e| e.unwrap().path()).collect()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Output {
    fn of(stdout: Vec<u8>, status: std::process::ExitStatus) -> Output {
        Output { stdout: String::from_utf8(stdout).unwrap(), status: status.code() }
    }
}

const CHECK: &[&str] = &["-p", ".", "--fast", "--pretty", "false"];
const TSCONFIG: &str = r#"{ "compilerOptions": { "noEmit": true, "strict": true }, "include": ["src"] }"#;

fn replace(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from));
    std::fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn unchanged_project_replays_the_output_of_a_cold_run() {
    let p = Project::new(
        "unchanged",
        &[
            ("tsconfig.json", TSCONFIG),
            ("src/a.ts", "import { f } from \"./b\";\nexport const x: number = f();\n"),
            ("src/b.ts", "export function f(): string { return \"s\"; }\n"),
        ],
    );
    let first = p.check(Cache::Recorded);
    assert_eq!(first.status, Some(2));
    assert!(first.stdout.contains("TS2322"), "{first:?}");
    assert_eq!(p.check(Cache::Hit), first);
    // The files were recorded by their text (they were too new to trust their time stamps); that still matches.
    Project::settle();
    assert_eq!(p.check(Cache::Hit), first);
    // Other arguments are another entry; the colored, pretty output replays byte for byte too.
    let pretty = ["-p", ".", "--fast", "--pretty", "true"];
    p.check_with(&pretty, &[], Cache::Recorded);
    p.check_with(&pretty, &[], Cache::Hit);
    // A clean project replays its empty output and status 0.
    replace(&p.path("src/b.ts"), "string { return \"s\"", "number { return 1");
    assert_eq!(p.check(Cache::Miss).status, Some(0));
    assert_eq!(p.check(Cache::Hit).status, Some(0));
}

#[test]
fn edited_file_misses_even_with_its_modification_time_restored() {
    let p = Project::new(
        "edit",
        &[("tsconfig.json", TSCONFIG), ("src/a.ts", "export const x: number = 1;\n")],
    );
    Project::settle();
    let (_, cache) = p.run(CHECK, &[]);
    assert_eq!(cache, Cache::Recorded);
    assert_eq!(p.check(Cache::Hit).status, Some(0));
    // Same size, same modification time: only the change time (and the content) differ.
    let file = p.path("src/a.ts");
    let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
    replace(&file, "number", "string");
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), modified);
    assert_eq!(p.check(Cache::Miss).status, Some(2));
    assert_eq!(p.check(Cache::Hit).status, Some(2));
}

#[test]
fn file_found_by_a_failed_probe_misses() {
    // `lib` and the ancestor `node_modules` are outside the include glob, so no directory listing sees them change:
    // only the resolver's failed probes do.
    let p = Project::new(
        "probe",
        &[
            ("tsconfig.json", TSCONFIG),
            (
                "src/a.ts",
                "import { v } from \"../lib/foo\";\nimport { w } from \"pkg\";\nexport const n: number = v;\nexport const m: number = w;\n",
            ),
            ("lib/foo.d.ts", "export declare const v: string;\n"),
        ],
    );
    Project::settle();
    let first = p.check(Cache::Recorded);
    assert!(first.stdout.contains("TS2307") && first.stdout.contains("TS2322"), "{first:?}");
    assert_eq!(p.check(Cache::Hit), first);
    // `foo.ts` wins over `foo.d.ts` once it exists.
    p.write("lib/foo.ts", "export const v = 1;\n");
    let second = p.check(Cache::Miss);
    assert_ne!(second, first);
    p.check(Cache::Hit);
    // A package appears in a `node_modules` above the project.
    let package = p.root.join("node_modules/pkg");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("index.d.ts"), "export declare const w: number;\n").unwrap();
    let third = p.check(Cache::Miss);
    assert_eq!(third.status, Some(0), "{third:?}");
    p.check(Cache::Hit);
}

#[test]
fn deleted_file_misses() {
    let p = Project::new(
        "delete",
        &[
            ("tsconfig.json", TSCONFIG),
            ("src/a.ts", "import { v } from \"../lib/foo\";\nexport const n: string = v;\n"),
            ("lib/foo.d.ts", "export declare const v: string;\n"),
        ],
    );
    Project::settle();
    assert_eq!(p.check(Cache::Recorded).status, Some(0));
    p.check(Cache::Hit);
    std::fs::remove_file(p.path("lib/foo.d.ts")).unwrap();
    assert_eq!(p.check(Cache::Miss).status, Some(2));
    p.check(Cache::Hit);
}

#[test]
fn changed_config_or_extended_config_misses() {
    let p = Project::new(
        "config",
        &[
            (
                "tsconfig.json",
                r#"{ "extends": "../base.json", "compilerOptions": { "noEmit": true }, "include": ["src"] }"#,
            ),
            ("../base.json", r#"{ "compilerOptions": { "strict": false } }"#),
            ("src/a.ts", "export function f(x) { return x; }\n"),
        ],
    );
    Project::settle();
    assert_eq!(p.check(Cache::Recorded).status, Some(0));
    p.check(Cache::Hit);
    replace(&p.root.join("base.json"), "false", "true");
    let strict = p.check(Cache::Miss);
    assert!(strict.stdout.contains("TS7006"), "{strict:?}");
    p.check(Cache::Hit);
    replace(&p.path("tsconfig.json"), "\"noEmit\": true", "\"noEmit\": true, \"noImplicitAny\": false");
    assert_eq!(p.check(Cache::Miss).status, Some(0));
    p.check(Cache::Hit);
}

#[test]
fn environment_selects_the_entry_or_turns_the_cache_off() {
    let p = Project::new(
        "env",
        &[("tsconfig.json", TSCONFIG), ("src/a.ts", "export const x: number = \"s\";\n")],
    );
    // No `--pretty`: FORCE_COLOR decides it, so it is part of the key.
    let args = ["-p", ".", "--fast"];
    let plain = p.check_with(&args, &[], Cache::Recorded);
    let colored = p.check_with(&args, &[("FORCE_COLOR", "1")], Cache::Recorded);
    assert_ne!(plain, colored);
    assert_eq!(p.check_with(&args, &[], Cache::Hit), plain);
    assert_eq!(p.check_with(&args, &[("FORCE_COLOR", "1")], Cache::Hit), colored);
    // TSRS_FAST=1 instead of `--fast` is another key but the same check.
    let check = ["-p", ".", "--pretty", "false"];
    p.check_with(&check, &[("TSRS_FAST", "1")], Cache::Recorded);
    p.check_with(&check, &[("TSRS_FAST", "1")], Cache::Hit);
    // Any other TSRS_ variable is an experiment or a measurement: such runs always happen.
    p.check_with(CHECK, &[("TSRS_FLOW_MEMO", "off")], Cache::Off);
    p.check_with(CHECK, &[("TSRS_FLOW_MEMO", "off")], Cache::Off);
    let count = p.entries().len();
    let (_, cache) = p.run(CHECK, &[("TSRS_RESULT_CACHE", "0")]);
    assert_eq!(cache, Cache::Silent);
    assert_eq!(p.entries().len(), count);
}

#[test]
fn retargeted_symlink_misses() {
    let p = Project::new(
        "symlink",
        &[
            ("tsconfig.json", TSCONFIG),
            ("src/a.ts", "import { v } from \"../lib/current\";\nexport const n: number = v;\n"),
            ("lib/one.d.ts", "export declare const v: number;\n"),
            ("lib/two.d.ts", "export declare const v: string;\n"),
        ],
    );
    std::os::unix::fs::symlink("one.d.ts", p.path("lib/current.d.ts")).unwrap();
    Project::settle();
    assert_eq!(p.check(Cache::Recorded).status, Some(0));
    p.check(Cache::Hit);
    // Replace the link atomically, as `ln -sfn` through a rename does.
    std::os::unix::fs::symlink("two.d.ts", p.path("lib/next.d.ts")).unwrap();
    std::fs::rename(p.path("lib/next.d.ts"), p.path("lib/current.d.ts")).unwrap();
    assert_eq!(p.check(Cache::Miss).status, Some(2));
    p.check(Cache::Hit);
}

#[test]
fn corrupt_entry_is_ignored_and_replaced() {
    let p = Project::new(
        "corrupt",
        &[("tsconfig.json", TSCONFIG), ("src/a.ts", "export const x: number = \"s\";\n")],
    );
    p.check(Cache::Recorded);
    let entries = p.entries();
    assert_eq!(entries.len(), 1);
    let mut bytes = std::fs::read(&entries[0]).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x20;
    std::fs::write(&entries[0], &bytes).unwrap();
    p.check(Cache::Miss);
    p.check(Cache::Hit);
    std::fs::write(&entries[0], b"").unwrap();
    p.check(Cache::Miss);
    p.check(Cache::Hit);
}

#[test]
fn concurrent_runs_record_and_replay_the_same_output() {
    let p = Project::new(
        "concurrent",
        &[
            ("tsconfig.json", TSCONFIG),
            ("src/a.ts", "import { f } from \"./b\";\nexport const x: number = f();\n"),
            ("src/b.ts", "export function f(): string { return \"s\"; }\n"),
        ],
    );
    let cold = p.cold(CHECK, &[]);
    let outputs: Vec<(Output, Cache)> = std::thread::scope(|scope| {
        // All four start before any is joined.
        let mut runs = Vec::new();
        for _ in 0..4 {
            runs.push(scope.spawn(|| p.run(CHECK, &[])));
        }
        runs.into_iter().map(|run| run.join().unwrap()).collect()
    });
    for (output, cache) in outputs {
        assert_eq!(output, cold);
        assert!(matches!(cache, Cache::Recorded | Cache::Hit), "{cache:?}");
    }
    assert_eq!(p.check(Cache::Hit), cold);
    assert_eq!(p.entries().len(), 1, "temporary files are renamed or removed");
}

#[test]
fn runs_that_cannot_be_replayed_are_not_recorded() {
    let p = Project::new(
        "ineligible",
        &[
            ("tsconfig.json", r#"{ "include": ["src"] }"#),
            ("src/a.ts", "export const x: number = 1;\n"),
        ],
    );
    let runs: [&[&str]; 4] = [
        // Not fast mode.
        &["-p", ".", "--noEmit", "--pretty", "false"],
        // Timings in the output.
        &["-p", ".", "--fast", "--noEmit", "--extendedDiagnostics", "--pretty", "false"],
        // Emits files.
        &["-p", ".", "--fast", "--outDir", "out", "--pretty", "false"],
        // Writes a build info file.
        &["-p", ".", "--fast", "--noEmit", "--incremental", "--pretty", "false"],
    ];
    for args in runs {
        let (_, cache) = p.run(args, &[]);
        let expected = if args.contains(&"--fast") { Cache::NotRecorded } else { Cache::Silent };
        assert_eq!(cache, expected, "{args:?}");
    }
    assert_eq!(p.entries(), Vec::<PathBuf>::new());
}

#[test]
fn cache_directory_keeps_the_newest_entries_and_nothing_else_is_touched() {
    let p = Project::new(
        "prune",
        &[("tsconfig.json", TSCONFIG), ("src/a.ts", "export const x: number = 1;\n")],
    );
    std::fs::create_dir_all(&p.cache).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_hours(24);
    for i in 0..70 {
        let file = std::fs::File::create(p.cache.join(format!("{i:032x}"))).unwrap();
        file.set_modified(old + Duration::from_secs(i)).unwrap();
    }
    let foreign = p.cache.join("notes.txt");
    std::fs::write(&foreign, "not an entry").unwrap();
    std::fs::File::options().write(true).open(&foreign).unwrap().set_modified(old).unwrap();
    p.check(Cache::Recorded);
    let entries = p.entries();
    assert_eq!(entries.len(), 65, "64 entries and the foreign file");
    assert!(foreign.exists());
    // The new entry and the 63 newest old ones remain.
    for i in 0..70 {
        assert_eq!(p.cache.join(format!("{i:032x}")).exists(), i >= 7, "{i}");
    }
    p.check(Cache::Hit);
}
