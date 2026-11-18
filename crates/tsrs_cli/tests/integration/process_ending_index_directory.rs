// Declaration emit names a type from `lib/index.ts` with a module specifier. Go `processEnding` removes `/index` only
// if no file has the name of the target file's directory plus an extension (`lib.ts` next to `lib/`). tsrs checked the
// specifier without `/index` against the current directory instead, so it removed `/index` when `lib.ts` was next to
// `lib/` and kept it when `lib.ts` was in the current directory. Expected outputs were taken from tsgo built from the
// pinned ts-ref commit on the same projects: empty stdout, exit status 0, and the `a.d.ts` text below.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-process-ending-index-directory-{}-{}",
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

fn emit_declaration(dir: &Path, declaration: &str) -> String {
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
    std::fs::read_to_string(dir.join(declaration)).unwrap()
}

const TSCONFIG: &str =
    r#"{"compilerOptions":{"declaration":true,"emitDeclarationOnly":true,"types":[]}}"#;
const LIB_INDEX: &str =
    "export interface T { v: number }\nexport function make(): T { return { v: 1 }; }\n";
const A: &str = "import { make } from \"./re\";\nexport const v = make();\n";

#[test]
fn keeps_index_when_a_file_has_the_directory_name() {
    let dir = scratch(
        "sibling",
        &[
            ("tsconfig.json", TSCONFIG),
            ("src/lib/index.ts", LIB_INDEX),
            ("src/lib.ts", "export const other = 1;\n"),
            ("src/re.ts", "export { make } from \"./lib/index\";\n"),
            ("src/a.ts", A),
        ],
    );
    assert_eq!(
        emit_declaration(&dir, "src/a.d.ts"),
        "export declare const v: import(\"./lib/index\").T;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ignores_a_file_in_the_current_directory() {
    let dir = scratch(
        "root",
        &[
            ("tsconfig.json", TSCONFIG),
            ("lib.ts", "export const other = 1;\n"),
            ("src/lib/index.ts", LIB_INDEX),
            ("src/re.ts", "export { make } from \"./lib/index\";\n"),
            ("src/a.ts", A),
        ],
    );
    assert_eq!(
        emit_declaration(&dir, "src/a.d.ts"),
        "export declare const v: import(\"./lib\").T;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_index_for_a_target_outside_the_importing_directory() {
    let dir = scratch(
        "outside",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"declaration":true,"emitDeclarationOnly":true,"types":[],"paths":{"@shared/*":["./shared/*"]}},"include":["app/src"]}"#,
            ),
            ("shared/lib/index.ts", LIB_INDEX),
            ("shared/lib.ts", "export const other = 1;\n"),
            ("app/src/re.ts", "export { make } from \"../../shared/lib/index\";\n"),
            ("app/src/a.ts", A),
        ],
    );
    assert_eq!(
        emit_declaration(&dir, "app/src/a.d.ts"),
        "export declare const v: import(\"../../shared/lib/index\").T;\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
