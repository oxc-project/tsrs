// Go `GetBuildInfoFileName` resolves the base name of the config file with its extension (`c:.json`) in `outDir`, and
// removes the extension at the end. The name `c:.json` has no root, so Go writes `out/c:.tsbuildinfo`. The Rust port
// removes the extension first. The base name `c:` is then a drive root, `combine_paths` drops `outDir`, and the
// relative name `c:.tsbuildinfo` makes the virtual file system panic ("path is not absolute", exit status 5). The
// `rootDir` branch has the same cause. The file name `c:.json` is not valid on Windows, so the files are created at
// run time and the tests do not run there. Expected values were taken from tsgo built from the pinned ts-ref commit:
// exit status 0, empty stdout, and the files `out/a.js` and `out/c:.tsbuildinfo`.

#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, config: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-buildinfo-name-config-drive-stem-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("c:.json"), config).unwrap();
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

fn assert_build_info_in_out_dir(out: &std::process::Output, dir: &Path) {
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("out/a.js").is_file(), "out/a.js is missing");
    assert!(dir.join("out/c:.tsbuildinfo").is_file(), "out/c:.tsbuildinfo is missing");
}

#[test]
fn project_with_out_dir() {
    let dir = scratch("project", r#"{"compilerOptions":{"incremental":true,"outDir":"out"}}"#);
    assert_build_info_in_out_dir(&tsrs(&["-p", "c:.json"], &dir), &dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn project_with_out_dir_and_root_dir() {
    let dir = scratch(
        "root-dir",
        r#"{"compilerOptions":{"incremental":true,"outDir":"out","rootDir":"."}}"#,
    );
    assert_build_info_in_out_dir(&tsrs(&["-p", "c:.json"], &dir), &dir);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_with_out_dir() {
    let dir = scratch("build", r#"{"compilerOptions":{"incremental":true,"outDir":"out"}}"#);
    assert_build_info_in_out_dir(&tsrs(&["-b", "c:.json"], &dir), &dir);
    let _ = std::fs::remove_dir_all(&dir);
}
