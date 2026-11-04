// `--fast` serves file text from a cache of the previous run's reads, validated by each file's stat (tsrs_vfs
// osvfs/sourcecache.rs, notes/perf-source-cache.md). Every run here is checked against a run of the same state with
// the cache off: the output and exit status must be the same whatever the cache holds. Each run is its own `tsrs`
// process, so fast mode stays out of this test process.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

struct Case {
    root: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct Run {
    diagnostics: String,
    status: Option<i32>,
    hits: u64,
    misses: u64,
}

impl Case {
    fn new(name: &str) -> Case {
        let root =
            std::env::temp_dir().join(format!("tsrs-source-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        // Diagnostics print paths relative to the working directory; resolve the temporary directory's symlinks
        // (macOS: /var -> /private/var) so both runs see one spelling.
        Case { root: fs::canonicalize(&root).unwrap() }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join("project").join(name)
    }

    fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn command(&self, cache: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
        cmd.current_dir(self.path(""))
            .args(["-p", ".", "--fast", "--pretty", "false", "--incremental", "false"])
            .args(["--extendedDiagnostics"])
            .env("TSRS_SOURCE_CACHE", cache)
            .env("TSRS_SOURCE_CACHE_DIR", self.cache_dir());
        for var in ["TSRS_FAST", "TSRS_CHECKER_ASSIGNMENT", "TSRS_FILE_TIMES"] {
            cmd.env_remove(var);
        }
        cmd
    }

    fn run(&self, cache: &str) -> Run {
        parse(&self.command(cache).output().unwrap())
    }

    // A run with the cache, after checking that a run with it off gives the same output.
    fn checked(&self) -> Run {
        let reference = self.run("0");
        let cached = self.run("1");
        assert_eq!(
            (&cached.diagnostics, cached.status),
            (&reference.diagnostics, reference.status),
            "the cached run differs from the uncached one"
        );
        cached
    }

    fn cache_files(&self, extension: &str) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.cache_dir()) else { return Vec::new() };
        entries
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == extension))
            .collect()
    }

    fn cache_file(&self) -> PathBuf {
        let files = self.cache_files("bin");
        assert_eq!(files.len(), 1, "one cache file: {files:?}");
        files.into_iter().next().unwrap()
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn parse(out: &Output) -> Run {
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.code().is_some_and(|c| c <= 2),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Diagnostics come before the statistics table, which starts with "Files:" (its counters vary between fast runs).
    let table = stdout.lines().position(|l| l.starts_with("Files:")).expect("statistics");
    let diagnostics: String = stdout.lines().take(table).flat_map(|l| [l, "\n"]).collect();
    let count = |row: &str| {
        stdout
            .lines()
            .find_map(|l| l.trim_start().strip_prefix(row))
            .map_or(0, |rest| rest.trim_start_matches(':').trim().parse().unwrap())
    };
    Run {
        diagnostics,
        status: out.status.code(),
        hits: count("Program:   source cache hits"),
        misses: count("Program:   source cache misses"),
    }
}

// Files are cached only once their modification and status-change times are 2 s old (the racy-write margin); the
// status-change time cannot be set, so wait it out.
fn settle() {
    std::thread::sleep(Duration::from_millis(2300));
}

fn set_mtime(path: &Path, mtime: std::time::SystemTime) {
    fs::File::options().write(true).open(path).unwrap().set_modified(mtime).unwrap();
}

#[test]
fn edits_deletes_renames_and_new_files_are_read_from_disk() {
    let case = Case::new("edits");
    case.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "strict": true, "noEmit": true }, "files": ["a.ts"] }"#,
    );
    case.write("a.ts", "import { b } from \"./b\";\nexport const a: string = b;\n");
    case.write("b.ts", "export const b = 12345;\n");
    settle();

    let cold = case.checked();
    assert!(cold.diagnostics.contains("TS2322"), "{cold:?}");
    assert_eq!((cold.hits, cold.misses), (0, 2), "the first run reads a.ts and b.ts from disk");
    let warm = case.checked();
    assert_eq!((warm.hits, warm.misses), (2, 0), "the second run reads both from the cache");

    // Same size, modification time restored: only the status-change time shows the edit.
    let mtime = fs::metadata(case.path("b.ts")).unwrap().modified().unwrap();
    let changed = std::time::Instant::now();
    case.write("b.ts", "export const b = \"123\";\n");
    set_mtime(&case.path("b.ts"), mtime);
    let edited = case.run("1");
    let fresh = changed.elapsed() < Duration::from_millis(1900);
    assert_eq!(edited, Run { hits: edited.hits, misses: edited.misses, ..case.run("0") });
    assert_eq!(edited.status, Some(0), "{edited:?}");
    assert_eq!((edited.hits, edited.misses), (1, 1));
    // A run that ended within 2 s of the edit read b.ts while it was unsettled, so it must not have stored it (on a
    // loaded machine the run can take longer; then b.ts may be stored and the check says nothing).
    if fresh {
        let again = case.checked();
        assert_eq!(
            (again.hits, again.misses),
            (1, 1),
            "a file read within 2 s of its last change is stored"
        );
    }

    fs::remove_file(case.path("b.ts")).unwrap();
    let deleted = case.checked();
    assert!(deleted.diagnostics.contains("TS2307"), "{deleted:?}");

    case.write("b.ts", "export const b = \"new\";\n");
    let created = case.checked();
    assert_eq!(created.status, Some(0), "{created:?}");

    case.write("b.next", "export const b = 54321;\n");
    fs::rename(case.path("b.next"), case.path("b.ts")).unwrap();
    let renamed = case.checked();
    assert!(renamed.diagnostics.contains("TS2322"), "{renamed:?}");

    settle();
    case.checked();
    let settled = case.checked();
    assert_eq!((settled.hits, settled.misses), (2, 0), "settled files are cached again");
    assert_eq!(case.cache_files("tmp"), Vec::<PathBuf>::new(), "no temporary file is left");
}

#[test]
fn off_without_fast_or_when_disabled() {
    let case = Case::new("off");
    case.write("tsconfig.json", r#"{ "compilerOptions": { "noEmit": true }, "files": ["a.ts"] }"#);
    case.write("a.ts", "export const a = 1;\n");
    settle();
    let disabled = case.run("0");
    assert_eq!((disabled.hits, disabled.misses), (0, 0));
    let mut slow = case.command("1");
    slow.args(["--fast", "false"]);
    let slow = parse(&slow.output().unwrap());
    assert_eq!((slow.hits, slow.misses), (0, 0));
    assert!(!case.cache_dir().exists(), "no cache is written");
}

// pnpm's layout: node_modules/dep is a symlink into node_modules/.pnpm. Repointing it must be seen whether the
// program names the file by its real path (the default) or through the link (`preserveSymlinks`).
#[test]
fn repointed_symlinked_directories_are_read_from_disk() {
    for preserve in [false, true] {
        let case = Case::new(if preserve { "symlinks-preserved" } else { "symlinks" });
        case.write(
            "tsconfig.json",
            &format!(
                r#"{{ "compilerOptions": {{ "strict": true, "noEmit": true, "preserveSymlinks": {preserve} }}, "files": ["a.ts"] }}"#
            ),
        );
        case.write("a.ts", "import { v } from \"dep\";\nexport const a: string = v;\n");
        for (version, value) in [("1", "12345"), ("2", "\"123\"")] {
            let dir = format!("node_modules/.pnpm/dep@{version}/node_modules/dep");
            case.write(
                &format!("{dir}/package.json"),
                r#"{ "name": "dep", "types": "index.d.ts" }"#,
            );
            case.write(
                &format!("{dir}/index.d.ts"),
                &format!("export declare const v: {value};\n"),
            );
        }
        let link = case.path("node_modules/dep");
        std::os::unix::fs::symlink(".pnpm/dep@1/node_modules/dep", &link).unwrap();
        settle();

        case.checked();
        let warm = case.checked();
        assert!(warm.diagnostics.contains("TS2322"), "{warm:?}");
        assert!(warm.hits >= 2 && warm.misses == 0, "{warm:?}");

        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(".pnpm/dep@2/node_modules/dep", &link).unwrap();
        let repointed = case.checked();
        assert_eq!(repointed.status, Some(0), "preserveSymlinks {preserve}: {repointed:?}");
    }
}

// On a case-insensitive volume (macOS's default) an import spelled in another case resolves to the same file; a
// case-only rename keeps the inode. Elsewhere the import does not resolve. Either way the cache changes nothing.
#[test]
fn case_variants_and_case_only_renames_match_the_uncached_run() {
    let case = Case::new("case");
    case.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "strict": true, "noEmit": true }, "files": ["a.ts"] }"#,
    );
    case.write("a.ts", "import { l } from \"./Lib\";\nexport const a: string = l;\n");
    case.write("lib.ts", "export const l = 12345;\n");
    settle();
    case.checked();
    case.checked();
    fs::rename(case.path("lib.ts"), case.path("LIB.ts")).unwrap();
    case.checked();
    case.write("Lib.ts", "export const l = \"123\";\n");
    case.checked();
}

#[test]
fn a_corrupt_or_truncated_cache_file_is_ignored() {
    let case = Case::new("corrupt");
    case.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "strict": true, "noEmit": true }, "files": ["a.ts"] }"#,
    );
    case.write("a.ts", "import { b } from \"./b\";\nexport const a: string = b;\n");
    case.write("b.ts", "export const b = 12345;\n");
    settle();
    case.checked();
    assert_eq!(case.checked().hits, 2);
    let file = case.cache_file();
    let good = fs::read(&file).unwrap();

    let truncated = &good[..good.len() / 2];
    let mut flipped = good.clone();
    *flipped.last_mut().unwrap() ^= 0x20;
    let garbage = vec![0xAB; good.len()];
    let mut wrong_length = good.clone();
    wrong_length.extend_from_slice(b"tail");
    for (name, bytes, hits) in [
        ("truncated", truncated, 0),
        ("one data byte flipped", flipped.as_slice(), 1),
        ("garbage", garbage.as_slice(), 0),
        ("empty", &[][..], 0),
        ("longer than its header says", wrong_length.as_slice(), 0),
    ] {
        fs::write(&file, bytes).unwrap();
        let run = case.checked();
        assert_eq!(run.hits, hits, "{name}: {run:?}");
        assert_eq!(run.hits + run.misses, 2, "{name}: {run:?}");
        fs::write(&file, &good).unwrap();
    }
    assert_eq!(case.checked().hits, 2, "the intact file still serves every read");
}

#[test]
fn concurrent_runs_each_see_one_complete_cache() {
    let case = Case::new("concurrent");
    case.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "strict": true, "noEmit": true }, "include": ["*.ts"] }"#,
    );
    for i in 0..40 {
        case.write(&format!("f{i}.ts"), &format!("export const v{i}: string = {i};\n"));
    }
    settle();
    let reference = case.run("0");
    for round in 0..3 {
        let children: Vec<_> = std::iter::repeat_with(|| {
            case.command("1").stdout(std::process::Stdio::piped()).spawn().unwrap()
        })
        .take(4)
        .collect();
        for child in children {
            let run = parse(&child.wait_with_output().unwrap());
            assert_eq!(
                (&run.diagnostics, run.status),
                (&reference.diagnostics, reference.status),
                "round {round}"
            );
            if round > 0 {
                assert_eq!((run.hits, run.misses), (40, 0), "round {round}");
            }
        }
    }
    assert_eq!(case.cache_files("tmp"), Vec::<PathBuf>::new(), "no temporary file is left");
}
