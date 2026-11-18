// The TS5011 layout check computes the common source directory of the emitted files. With no emitted file, Go falls
// back to the program's base directory, the directory of the tsconfig (`program.go` `p.BaseDirectory()`), not the
// process working directory. Run from the parent of the config directory, the working directory gave a different
// directory, so tsrs reported TS5011 where tsgo reports nothing. Expected outputs were taken from tsgo built from the
// pinned ts-ref commit on the same projects: empty stdout and exit status 0 in every case.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-common-source-dir-config-dir-{}-{}",
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

fn assert_clean(out: &std::process::Output) {
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const DTS_ONLY: &[(&str, &str)] = &[
    ("sub/tsconfig.json", r#"{"compilerOptions":{"outDir":"../out"}}"#),
    ("sub/a.d.ts", "declare const x: number;\n"),
];

#[test]
fn project_in_subdirectory_with_only_declaration_inputs() {
    let dir = scratch("project", DTS_ONLY);
    assert_clean(&tsrs(&["-p", "sub"], &dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_in_subdirectory_with_only_declaration_inputs() {
    let dir = scratch("build", DTS_ONLY);
    assert_clean(&tsrs(&["-b", "sub"], &dir));
    let info = std::fs::read_to_string(dir.join("out/tsconfig.tsbuildinfo")).unwrap();
    assert!(!info.contains("\"errors\":true"), "{info}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn solution_style_config_with_out_dir_flag() {
    let dir = scratch(
        "solution",
        &[
            ("sub/tsconfig.json", r#"{"files":[],"references":[{"path":"./tsconfig.app.json"}]}"#),
            ("sub/tsconfig.app.json", r#"{"compilerOptions":{"composite":true},"files":["a.ts"]}"#),
            ("sub/a.ts", "export const a = 1;\n"),
        ],
    );
    assert_clean(&tsrs(&["-p", "sub", "--outDir", "out"], &dir));
    let _ = std::fs::remove_dir_all(&dir);
}
