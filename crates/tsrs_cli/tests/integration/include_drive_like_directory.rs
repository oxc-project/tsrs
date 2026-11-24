// The `include` walk resolves each directory entry with Go's `RootedDirectoryPath.ResolveDirectory`. A directory named
// like a drive root (`c:`) has a root, so Go resolves it to `c:/` and never matches the files below it against the
// include patterns of the project. tsrs appended the name as text, so it put `c:/a.ts` in the program and reported
// its errors. Expected outputs were taken from tsgo built from the pinned ts-ref commit on the same projects: empty
// stdout and exit status 0 in every case. A directory named `c:` cannot exist on Windows, so the tests run on Unix only.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-include-drive-like-directory-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    dir
}

fn tsrs(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

fn assert_clean(out: &std::process::Output) {
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const TSCONFIG: (&str, &str) = ("tsconfig.json", r#"{"compilerOptions":{"noEmit":true}}"#);

#[test]
fn default_include_skips_drive_like_directory() {
    let dir = scratch(
        "top",
        &[
            TSCONFIG,
            ("b.ts", "export const y = 1;\n"),
            ("c:/a.ts", "export const x: number = \"s\";\n"),
        ],
    );
    assert_clean(&tsrs(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn default_include_skips_nested_drive_like_directories() {
    let dir = scratch(
        "nested",
        &[
            TSCONFIG,
            ("b.ts", "export const y = 1;\n"),
            ("x/c:/a.ts", "export const x: number = \"s\";\n"),
            ("y/c:/b.ts", "export const z: number = \"s\";\n"),
        ],
    );
    assert_clean(&tsrs(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

// The walk reads directory listings ahead in parallel. A `c:` entry below `c:/` resolves back to `c:/`, so the
// read-ahead must not follow it, or it never ends.
#[test]
fn explicit_drive_include_does_not_reenter_root() {
    let dir = scratch(
        "reentry",
        &[
            ("tsconfig.json", r#"{"compilerOptions":{"noEmit":true},"include":["c:/**/*.ts"]}"#),
            ("c:/a.ts", "export const x = 1;\n"),
            ("c:/c:/b.ts", "export const y = 1;\n"),
        ],
    );
    assert_clean(&tsrs(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

// The read-ahead of `c:` below the project must not replace the listing of the base path `c:/`, which was matched
// against the `c:/a.ts` exclude.
#[test]
fn drive_root_excludes_apply_when_reached_from_project() {
    let dir = scratch(
        "excludes",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"noEmit":true},"include":["**/*.ts","c:/**/*.ts"],"exclude":["**/a.ts","c:/a.ts"]}"#,
            ),
            ("b.ts", "export const x = 1;\n"),
            ("c:/a.ts", "export const x: number = \"s\";\n"),
        ],
    );
    assert_clean(&tsrs(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

// A directory named `a\..` resolves to the project directory itself (Go normalizes the `\`), which the walk has
// already visited. The read-ahead must not follow it either, or it never ends.
#[test]
fn directory_name_with_backslash_does_not_loop() {
    let dir = scratch("backslash", &[TSCONFIG, ("ok.ts", "export const v = 1;\n")]);
    std::fs::create_dir_all(dir.join("a\\..")).unwrap();
    std::fs::write(dir.join("a\\..").join("x.ts"), "export const w: number = \"s\";\n").unwrap();
    assert_clean(&tsrs(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}
