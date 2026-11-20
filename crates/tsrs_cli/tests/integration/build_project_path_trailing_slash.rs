// Go resolves each `--build` project with `ResolveFile`, which normalizes the path and so removes a trailing `/`. The
// argument `tsconfig.json/` then names the file `<dir>/tsconfig.json`, and tsgo builds it. tsrs resolves the argument
// with `resolve_path`, which keeps the trailing `/`. It then treats the argument as a directory and looks for
// `<dir>/tsconfig.json/tsconfig.json`: it reports TS6053, exits with 1, and writes no output. Expected values were
// taken from tsgo built from the pinned ts-ref commit: empty output, exit status 0, `a.js` and `tsconfig.tsbuildinfo`
// written.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-build-project-trailing-slash-{}-{}",
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
fn build_config_file_name_with_trailing_slash() {
    let dir =
        scratch("file", &[("tsconfig.json", r#"{"files":["a.ts"]}"#), ("a.ts", "export {};\n")]);
    let out = tsrs(&["-b", "tsconfig.json/"], &dir);
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("a.js").is_file(), "a.js was not written");
    assert!(dir.join("tsconfig.tsbuildinfo").is_file(), "tsconfig.tsbuildinfo was not written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_config_file_in_directory_with_trailing_slash() {
    let dir = scratch(
        "nested",
        &[("x/tsconfig.json", r#"{"files":["a.ts"]}"#), ("x/a.ts", "export {};\n")],
    );
    let out = tsrs(&["-b", "x/tsconfig.json/"], &dir);
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("x/a.js").is_file(), "x/a.js was not written");
    let _ = std::fs::remove_dir_all(&dir);
}
