// Go's `getBasePaths` normalizes an absolute `include` entry before it takes the base path of the directory walk
// (`vfsmatch.go`). tsrs walked the entry as written, so the walk visited `<shared>/./a.ts`, which never matched the
// normalized include pattern: an entry such as `<shared>/./*.ts` found no files (TS18003), and `<shared>/sub/..` found
// only the files below `sub`. Expected outputs were taken from tsgo built from the pinned ts-ref commit on the same
// layout: the TS2322 errors of the files that the include entry names, and exit status 2. The tests write absolute
// POSIX paths into `tsconfig.json`, so they run on Unix only.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

const ERROR: &str = "(1,14): error TS2322: Type 'string' is not assignable to type 'number'.\n";

// `<tmp>/proj/tsconfig.json` with the given include entry, and two files outside the project, `<tmp>/shared/a.ts`
// and `<tmp>/shared/sub/b.ts`. The include entry can use `$SHARED` for the absolute path of `<tmp>/shared`.
fn scratch(name: &str, include: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-include-rooted-dot-segments-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let shared = dir.join("shared");
    std::fs::create_dir_all(shared.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join("proj")).unwrap();
    std::fs::write(shared.join("a.ts"), "export const a: number = \"s\";\n").unwrap();
    std::fs::write(shared.join("sub/b.ts"), "export const b: number = \"s\";\n").unwrap();
    let include = include.replace("$SHARED", shared.to_str().unwrap());
    std::fs::write(
        dir.join("proj/tsconfig.json"),
        format!(r#"{{"compilerOptions":{{"noEmit":true}},"include":["{include}"]}}"#),
    )
    .unwrap();
    dir
}

fn tsrs(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .current_dir(dir.join("proj"))
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

fn assert_output(out: &std::process::Output, expected: &str) {
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(2), expected),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn absolute_include_with_dot_segment() {
    let dir = scratch("dot", "$SHARED/./*.ts");
    assert_output(&tsrs(&dir), &format!("../shared/a.ts{ERROR}"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn absolute_include_with_dot_dot_segment() {
    let dir = scratch("dotdot", "$SHARED/sub/..");
    assert_output(&tsrs(&dir), &format!("../shared/a.ts{ERROR}../shared/sub/b.ts{ERROR}"));
    let _ = std::fs::remove_dir_all(&dir);
}
