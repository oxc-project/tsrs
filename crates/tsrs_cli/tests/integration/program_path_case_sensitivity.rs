// Go `Program` compares paths with the case sensitivity of the host file system (`p.caseSensitivity`). tsrs used a
// case-insensitive comparison, so on a case-sensitive file system `SRC/b.ts` was inside `rootDir` `src`, and `Proj` and
// `proj` were the same directory. Expected outputs were taken from tsgo built from the pinned ts-ref commit on the same
// projects, with the project directory replaced by `<dir>`. The tests do nothing on a case-insensitive file system,
// where the two directories cannot both exist.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-program-path-case-sensitivity-{}-{}",
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

fn case_sensitive_file_system(dir: &Path) -> bool {
    std::fs::write(dir.join("case-probe"), "").unwrap();
    let sensitive = !dir.join("CASE-PROBE").exists();
    std::fs::remove_file(dir.join("case-probe")).unwrap();
    sensitive
}

fn tsrs(project: &str, dir: &Path) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", project, "--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    let dir = dir.to_str().unwrap();
    (out.status.code(), String::from_utf8_lossy(&out.stdout).replace(dir, "<dir>"))
}

#[test]
fn root_dir_does_not_contain_a_directory_that_differs_in_case() {
    let dir = scratch(
        "root-dir",
        &[
            ("tsconfig.json", r#"{"compilerOptions":{"rootDir":"src","outDir":"out","types":[]}}"#),
            ("src/a.ts", "export const a = 1;\n"),
            ("SRC/b.ts", "export const b = 2;\n"),
        ],
    );
    if case_sensitive_file_system(&dir) {
        assert_eq!(
            tsrs(".", &dir),
            (
                Some(2),
                "error TS6059: File '<dir>/SRC/b.ts' is not under 'rootDir' '<dir>/src'. 'rootDir' is expected to contain all source files.\n  The file is in the program because:\n    Matched by default include pattern '**/*'\n".to_string()
            )
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn composite_root_does_not_contain_a_directory_that_differs_in_case() {
    let dir = scratch(
        "composite",
        &[
            (
                "Proj/tsconfig.json",
                r#"{"compilerOptions":{"outDir":"out","declaration":true,"composite":true,"types":[]},"files":["src/a.ts"]}"#,
            ),
            ("Proj/src/a.ts", "import \"../../proj/lib/b\";export const a = 1;\n"),
            ("proj/lib/b.ts", "export const b = 1;\n"),
        ],
    );
    if case_sensitive_file_system(&dir) {
        assert_eq!(
            tsrs("Proj", &dir),
            (
                Some(2),
                "Proj/src/a.ts(1,8): error TS6059: File '<dir>/proj/lib/b.ts' is not under 'rootDir' '<dir>/Proj'. 'rootDir' is expected to contain all source files.\nProj/src/a.ts(1,8): error TS6307: File '<dir>/proj/lib/b.ts' is not listed within the file list of project '<dir>/Proj/tsconfig.json'. Projects must list all files or use an 'include' pattern.\n".to_string()
            )
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn base_url_hint_keeps_a_directory_that_differs_in_case() {
    let dir = scratch(
        "base-url",
        &[
            (
                "Proj/tsconfig.json",
                r#"{"compilerOptions":{"baseUrl":"../proj/src","noEmit":true,"types":[]}}"#,
            ),
            ("Proj/a.ts", "export const a = 1;\n"),
            ("proj/src/x.ts", "export const x = 1;\n"),
        ],
    );
    if case_sensitive_file_system(&dir) {
        assert_eq!(
            tsrs("Proj", &dir),
            (
                Some(2),
                "Proj/tsconfig.json(1,21): error TS5102: Option 'baseUrl' has been removed. Please remove it from your configuration.\n  Use '\"paths\": {\"*\": [\"../proj/src/*\"]}' instead.\n".to_string()
            )
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
