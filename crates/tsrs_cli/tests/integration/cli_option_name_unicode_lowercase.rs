// Go lowercases a command-line option name with `strings.ToLower`, which maps U+0130 (`İ`, capital I with dot above)
// to `i`. Rust `str::to_lowercase` uses the full Unicode mapping and gives `i` and U+0307. So `--noEmİt` is the option
// `noemit` in tsgo, and an unknown option in tsrs. The same holds for the first argument `--buİld` and for an option
// that is only valid with `--build`. Expected values were taken from tsgo built from the pinned ts-ref commit.
// The `\u{130}` below is U+0130.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-cli-option-name-unicode-lowercase-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tsconfig.json"), "{}\n").unwrap();
    std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
    dir
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

fn stdout_and_code(out: &std::process::Output) -> (Option<i32>, String) {
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn option_name_with_dotted_capital_i() {
    let dir = scratch("option");
    let out = tsrs(&["-p", ".", "--noEm\u{130}t"], &dir);
    assert_eq!(stdout_and_code(&out), (Some(0), String::new()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_flag_as_first_argument_with_dotted_capital_i() {
    let dir = scratch("build");
    let out = tsrs(&["--bu\u{130}ld"], &dir);
    assert_eq!(stdout_and_code(&out), (Some(0), String::new()));
    assert!(dir.join("a.js").is_file(), "a.js is missing");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_only_option_without_build_with_dotted_capital_i() {
    let dir = scratch("build-only");
    let out = tsrs(&["-p", ".", "--stopBu\u{130}ldOnErrors"], &dir);
    assert_eq!(
        stdout_and_code(&out),
        (
            Some(1),
            "error TS5093: Compiler option '--stopBu\u{130}ldOnErrors' may only be used with '--build'.\n".to_string()
        )
    );
    let _ = std::fs::remove_dir_all(&dir);
}
