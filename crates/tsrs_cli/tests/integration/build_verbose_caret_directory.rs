// Go prints the absolute name of a project when its path relative to the working directory starts with a root, such
// as `^/tsconfig.json` for a directory named `^` (`Orchestrator.relativePath` gets `false` from
// `RelativePathFromPath`). tsrs printed `^/tsconfig.json` in the `-b --verbose` output. Expected values were taken
// from tsgo built from the pinned ts-ref commit.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-build-verbose-caret-directory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    // The compilers print the real path of the working directory.
    std::fs::canonicalize(&dir).unwrap()
}

fn tsrs(args: &[&str], dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .args(["--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

#[test]
fn build_verbose_names_project_below_caret_directory() {
    let dir = scratch(&[
        ("tsconfig.json", r#"{"files":[],"references":[{"path":"./^"}]}"#),
        ("^/tsconfig.json", r#"{"compilerOptions":{"composite":true},"files":["b.ts"]}"#),
        ("^/b.ts", "export const b = 1;\n"),
    ]);
    let out = tsrs(&["-b", ".", "--verbose"], &dir);
    let stdout = String::from_utf8_lossy(&out.stdout).replace(&dir.display().to_string(), "<dir>");
    // Remove the time stamp before " - " on each line.
    let lines: Vec<&str> =
        stdout.lines().map(|line| line.split_once(" - ").map_or(line, |(_, rest)| rest)).collect();
    assert_eq!(
        (out.status.code(), lines),
        (
            Some(0),
            vec![
                "Projects in this build: ",
                "    * <dir>/^/tsconfig.json",
                "    * tsconfig.json",
                "",
                "Project '<dir>/^/tsconfig.json' is out of date because output file '<dir>/^/tsconfig.tsbuildinfo' does not exist",
                "",
                "Building project '<dir>/^/tsconfig.json'...",
                "",
            ]
        )
    );
    let _ = std::fs::remove_dir_all(&dir);
}
