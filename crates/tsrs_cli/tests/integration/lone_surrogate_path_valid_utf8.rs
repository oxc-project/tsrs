// Go reads files through `os.DirFS`. Its `join` calls `fs.ValidPath`, which rejects a name that is not valid UTF-8.
// A module specifier with a lone surrogate escape (`"./n\uD800"`) becomes a path with the three WTF-8 bytes
// `ED A0 80`, so tsgo never finds a file with those bytes in its name and reports TS2307. The tsrs port of
// `valid_path` checks only the path elements, so tsrs resolves the import to that file, reads it, exits with code 0,
// and emits a file with the odd name. Expected values were taken from tsgo built from the pinned ts-ref commit.
// The test needs a file name with raw bytes, so it runs on Unix only.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// The name of the file on disk: `n`, the WTF-8 bytes of U+D800, and `.ts`.
fn odd_file_name() -> OsString {
    OsString::from_vec(b"n\xED\xA0\x80.ts".to_vec())
}

fn scratch(name: &str, tsconfig: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-lone-surrogate-path-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tsconfig.json"), tsconfig).unwrap();
    // The file holds the six characters `\uD800`, not the code unit.
    std::fs::write(dir.join("a.ts"), "import { x } from \"./n\\uD800\";\nexport const z: 1 = x;\n")
        .unwrap();
    std::fs::write(dir.join(odd_file_name()), "export const x = 1;\n").unwrap();
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

#[test]
fn import_of_a_lone_surrogate_name_is_not_resolved() {
    let dir = scratch("resolve", r#"{ "compilerOptions": { "noEmit": true }, "files": ["a.ts"] }"#);
    let out = tsrs(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    // The number of U+FFFD characters in the message is a separate question, so the test does not compare it.
    assert!(
        stdout.starts_with("a.ts(1,19): error TS2307: Cannot find module './n"),
        "stdout: {stdout:?}"
    );
    assert!(stdout.ends_with("' or its corresponding type declarations.\n"), "stdout: {stdout:?}");
    assert_eq!(out.status.code(), Some(2), "stdout: {stdout:?}");
}

#[test]
fn file_with_a_lone_surrogate_name_is_not_emitted() {
    let dir = scratch(
        "emit",
        r#"{ "compilerOptions": { "outDir": "out", "declaration": true }, "files": ["a.ts"] }"#,
    );
    let out = tsrs(&dir);
    let mut emitted: Vec<Vec<u8>> = std::fs::read_dir(dir.join("out"))
        .unwrap()
        .map(|entry| std::os::unix::ffi::OsStringExt::into_vec(entry.unwrap().file_name()))
        .collect();
    emitted.sort();
    let _ = std::fs::remove_dir_all(&dir);
    // tsgo writes only the files of `a.ts`, and exits with code 2 because of the TS2307 error.
    assert_eq!(
        emitted,
        vec![b"a.d.ts".to_vec(), b"a.js".to_vec()],
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(out.status.code(), Some(2));
}
