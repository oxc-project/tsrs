// Go resolves a `/// <reference path>` name with `RootedDirectoryPath.ResolveFile`, both in the file loader and in
// `Program.GetSourceFileFromReference`, which declaration emit uses to write the reference into the `.d.ts` file.
// `ResolveFile` removes a trailing separator (`./y/` becomes `<dir>/y`) and ends a path that is only a root with a
// separator (`C:` becomes `C:/`). tsrs kept the trailing `/`, so declaration emit dropped the reference, and it kept
// `C:` as it was, so the lookup of `C:.ts` panicked. Expected outputs were taken from tsgo built from the pinned
// ts-ref commit on the same projects.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-reference-path-resolve-file-{}-{}",
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

const TSCONFIG: &str = r#"{"compilerOptions":{"declaration":true,"emitDeclarationOnly":true,"outDir":"o","types":[]},"files":["a.ts"]}"#;

#[test]
fn declaration_emit_keeps_references_with_trailing_slash() {
    let dir = scratch(
        "trailing-slash",
        &[
            ("tsconfig.json", TSCONFIG),
            ("x.d.ts", "type X = number;\n"),
            ("y.ts", "declare const y: number;\n"),
            (
                "a.ts",
                "/// <reference path=\"./x.d.ts/\" preserve=\"true\" />\n/// <reference path=\"./y/\" preserve=\"true\" />\nexport const a: X = y;\n",
            ),
        ],
    );
    let out = tsrs(&dir);
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("o/a.d.ts")).unwrap(),
        "/// <reference path=\"../x.d.ts\" preserve=\"true\" />\n/// <reference path=\"y.d.ts\" preserve=\"true\" />\nexport declare const a: X;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn drive_root_reference_reports_unresolved_path() {
    let dir = scratch(
        "drive-root",
        &[
            ("tsconfig.json", TSCONFIG),
            ("y.ts", "declare const y: number;\n"),
            (
                "a.ts",
                "/// <reference path=\"C:\" preserve=\"true\" />\n/// <reference path=\"./y.ts\" preserve=\"true\" />\nexport const a = y;\n",
            ),
        ],
    );
    let out = tsrs(&dir);
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (
            Some(2),
            "a.ts(1,22): error TS6231: Could not resolve the path 'C:' with the extensions: '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n"
        ),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("o/a.d.ts")).unwrap(),
        "/// <reference path=\"y.d.ts\" preserve=\"true\" />\nexport declare const a: number;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
