// `tsc -b` on a config that finds no input files (TS18003) writes a `.tsbuildinfo` file for a project that is not
// incremental. Go sets `root` with `core.Map` over the file names. For an empty list that gives a non-nil empty slice,
// and the tag `json:"root,omitzero"` omits only a nil slice, so tsgo writes `"root":[]`. tsrs keeps `root` in a `Vec`,
// which cannot tell nil from empty, and its writer skips the key. Stdout and the exit code are the same, but the
// written file differs. Expected text from tsgo.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-buildinfo-root-empty-{}-{}",
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

fn tsrs(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .args(["--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

#[test]
fn build_with_no_inputs_writes_an_empty_root_list() {
    let dir = scratch("no-inputs", &[("tsconfig.json", r#"{"include":["missing.ts"]}"#)]);
    let out = tsrs(&dir, &["-b", "."]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "stdout: {stdout}");
    assert!(
        stdout.contains("error TS18003: No inputs were found in config file"),
        "stdout: {stdout}"
    );
    let info = std::fs::read_to_string(dir.join("tsconfig.tsbuildinfo")).unwrap();
    assert!(info.starts_with(r#"{"version":""#), "{info}");
    assert!(info.ends_with(r#","errors":true,"root":[]}"#), "tsbuildinfo text: {info}");
    let _ = std::fs::remove_dir_all(&dir);
}
