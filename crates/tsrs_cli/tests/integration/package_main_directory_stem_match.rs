// Go `isPackageMainFile` sets `mainIsDirectory` when the `main`, `types` or `typings` value ends with `/`, and it
// compares file stems only when that is false. tsrs builds the main file path with `to_path`, which removes the
// trailing `/`, and always compares the stems. So `lib/` makes `lib.d.ts` look like the package main file, and
// declaration emit writes `import("pkg").L` (which resolves to `lib/index.d.ts`) where tsgo writes
// `import("pkg/lib").L`. Expected values were taken from tsgo built from the pinned ts-ref commit on the same project.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, package_json: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-package-main-directory-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        ("tsconfig.json", r#"{"compilerOptions":{"declaration":true}}"#),
        ("node_modules/pkg/package.json", package_json),
        ("node_modules/pkg/lib/index.d.ts", "export {};\n"),
        ("node_modules/pkg/lib.d.ts", "export interface L { x: number }\n"),
        ("src/bar.ts", "import type { L } from \"pkg/lib\";\nexport const l: L = { x: 1 };\n"),
        ("src/main.ts", "import { l } from \"./bar\";\nexport const v = l;\n"),
    ];
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    dir
}

fn emitted_main_declaration(dir: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(dir.join("src/main.d.ts")).unwrap()
}

fn check(name: &str, package_json: &str) {
    let dir = scratch(name, package_json);
    let text = emitted_main_declaration(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(text, "export declare const v: import(\"pkg/lib\").L;\n");
}

#[test]
fn types_value_ends_with_slash() {
    check("types", r#"{"name":"pkg","types":"lib/"}"#);
}

#[test]
fn types_value_with_dot_slash_ends_with_slash() {
    check("types-dot", r#"{"name":"pkg","types":"./lib/"}"#);
}

#[test]
fn typings_value_ends_with_slash() {
    check("typings", r#"{"name":"pkg","typings":"lib/"}"#);
}

#[test]
fn main_value_ends_with_slash() {
    check("main", r#"{"name":"pkg","main":"lib/"}"#);
}
