// Go `ResolveTypeReferenceDirective` sets `fromInferredTypesContainingFile` when the base name of the containing file
// is exactly `__inferred type names__.ts`. The Rust port tests that the full path ends with the same text, so a file
// such as `a__inferred type names__.ts` also sets the flag. With custom `typeRoots`, the flag skips the secondary lookup
// in `node_modules`. tsrs then reports TS2688 for a `/// <reference types="foo" />` that tsgo resolves from
// `node_modules/@types/foo`. Expected values were taken from tsgo built from the pinned ts-ref commit: empty stdout
// and exit status 0. The case needs a `node_modules` directory, which `.gitignore` excludes, so it is created at run time.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-inferred-types-file-name-suffix-{}-{}",
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

#[test]
fn type_reference_from_file_with_inferred_types_suffix() {
    let dir = scratch(
        "suffix",
        &[
            ("tsconfig.json", r#"{"compilerOptions":{"typeRoots":["./types"],"noEmit":true}}"#),
            ("a__inferred type names__.ts", "/// <reference types=\"foo\" />\n"),
            ("node_modules/@types/foo/index.d.ts", "declare const foo: number;\n"),
        ],
    );
    assert_clean(&tsrs(&["-p", "."], &dir));
    let _ = std::fs::remove_dir_all(&dir);
}
