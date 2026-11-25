// A relative import rewritten into a referenced project on another drive. Go `resolveExternalModule` reports TS2878
// when either the root directories or the output directories of the two projects have no relative path between them
// (`RelativePathFromDirectory` returns false). The expected output was taken from tsgo built from the pinned ts-ref
// commit on the same projects. On Windows `C:` and `D:` are real drive roots, so the test runs on Unix only.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-rewrite-import-cross-drive-reference-{}", std::process::id()));
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
fn referenced_project_on_another_drive_reports_ts2878() {
    let dir = scratch(&[
        (
            "C:/app/tsconfig.json",
            r#"{
  "files": ["main.ts"],
  "references": [{ "path": "D:/lib" }],
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "nodenext",
    "types": [],
    "rootDir": ".",
    "rootDirs": ["C:/app", "D:/lib"],
    "rewriteRelativeImportExtensions": true,
    "noEmit": true
  }
}"#,
        ),
        ("C:/app/main.ts", "import {} from \"./dep.ts\";\n"),
        (
            "D:/lib/tsconfig.json",
            r#"{
  "files": ["dep.ts"],
  "compilerOptions": {
    "strict": true,
    "target": "esnext",
    "module": "nodenext",
    "types": [],
    "rootDir": ".",
    "composite": true
  }
}"#,
        ),
        ("D:/lib/dep.ts", "export {};\n"),
        ("D:/lib/dep.d.ts", "export {};\n"),
    ]);
    let expected = "C:/app/main.ts(1,16): error TS2878: This import path is unsafe to rewrite because it resolves \
                    to another project, and the relative path between the projects' output files is not the same \
                    as the relative path between its input files.\n";
    for args in [&["-p", "C:/app"][..], &["-p", "C:/app", "--fast"][..]] {
        let out = tsrs(args, &dir);
        assert_eq!(
            (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
            (Some(2), expected),
            "args: {args:?}, stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
