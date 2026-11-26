// Go `RelativePathFromDirectory` returns `false` when the relative path would start with a root, such as `^/m` for a
// directory named `^`. For a declaration emit specifier, Go then uses the absolute name (`getLocalModuleSpecifier`),
// and for an `exports` directory target it uses an empty fragment (`tryGetModuleNameFromExportsOrImports`). tsrs used
// the relative text, so it wrote `import("^/m")` and `import("^/x.js")`. Expected values were taken from tsgo built
// from the pinned ts-ref commit.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-declaration-specifier-caret-directory-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    // tsgo writes the real path of the project directory.
    std::fs::canonicalize(&dir).unwrap()
}

fn build(dir: &Path) {
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
}

#[test]
fn relative_specifier_for_directory_named_caret() {
    let dir = scratch(
        "relative",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"declaration":true,"emitDeclarationOnly":true},"files":["b.ts"]}"#,
            ),
            ("b.ts", "import { make } from \"./lib\";\nexport const v = make();\n"),
            ("lib.ts", "export { make } from \"./^/m\";\n"),
            (
                "^/m.ts",
                "export interface T { a: number }\nexport function make(): T { return { a: 1 }; }\n",
            ),
        ],
    );
    build(&dir);
    let declaration = std::fs::read_to_string(dir.join("b.d.ts")).unwrap();
    assert_eq!(
        declaration,
        format!("export declare const v: import(\"{}/^/m\").T;\n", dir.display())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exports_directory_target_below_caret_directory() {
    let dir = scratch(
        "exports",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"module":"nodenext","declaration":true,"emitDeclarationOnly":true,"outDir":"out"},"files":["a.ts"]}"#,
            ),
            ("a.ts", "import { make } from \"pkg\";\nexport const v = make();\n"),
            (
                "node_modules/pkg/package.json",
                r#"{"name":"pkg","version":"1.0.0","exports":{".":"./index.d.ts","./sub/":"./dist/"}}"#,
            ),
            (
                "node_modules/pkg/index.d.ts",
                "export declare function make(): import(\"./dist/^/x.js\").X;\n",
            ),
            ("node_modules/pkg/dist/^/x.d.ts", "export interface X { a: 1 }\n"),
        ],
    );
    build(&dir);
    let declaration = std::fs::read_to_string(dir.join("out/a.d.ts")).unwrap();
    assert_eq!(declaration, "export declare const v: import(\"pkg/sub/dist\").X;\n");
    let _ = std::fs::remove_dir_all(&dir);
}
