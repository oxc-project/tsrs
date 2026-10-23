// Whether a file is "from an external library" (declaration emit skips it) and whether a JS file beyond
// maxNodeModuleJsDepth is loaded depend on the order in which the file loader visits files: a file reached first
// through node_modules starts its imports at depth 1, and a later relative visit lowers only the file's own depth
// (filesparser.go `start`). tsrs loads in the order of Go's single-threaded work group (last queued first,
// core/workgroup.go), at any checker count. Here `app/node_modules/@scope/pkg` is a symlink to the workspace package
// `pkg`, and `app/main.ts` imports `pkg/src/user.ts` both through the package name and relatively, in either order.
// Go visits the last import first. Expected output from tsgo built from the pinned commit, with `--singleThreaded`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

const PACKAGE_FIRST: &str = "import type { User } from \"@scope/pkg/src/user\"\nimport type { User as User2 } from \"../pkg/src/user\"\nexport const user: [User, User2] | undefined = undefined\n";
const RELATIVE_FIRST: &str = "import type { User as User2 } from \"../pkg/src/user\"\nimport type { User } from \"@scope/pkg/src/user\"\nexport const user: [User, User2] | undefined = undefined\n";

// `<dir>/app` with `node_modules/@scope/pkg` linked to `<dir>/pkg`, and `files` written under `<dir>`.
fn scratch(name: &str, tsconfig: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-load-order-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("app/node_modules/@scope")).unwrap();
    std::fs::create_dir_all(dir.join("pkg/src")).unwrap();
    std::os::unix::fs::symlink("../../../pkg", dir.join("app/node_modules/@scope/pkg")).unwrap();
    std::fs::write(dir.join("app/tsconfig.json"), tsconfig).unwrap();
    std::fs::write(dir.join("pkg/package.json"), r#"{ "name": "@scope/pkg", "version": "1.0.0" }"#)
        .unwrap();
    for (path, content) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

// tsrs's output lines, with `dir` removed and lib files left out.
fn tsrs(dir: &Path, args: &[&str], single_threaded: bool) -> Vec<String> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsrs"));
    cmd.args(["-p", "tsconfig.json", "--pretty", "false"]).args(args).current_dir(dir.join("app"));
    if single_threaded {
        cmd.arg("--singleThreaded");
    }
    let out = cmd.output().unwrap();
    let prefix = format!("{}/", dir.canonicalize().unwrap().display());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.contains("/lib."))
        .map(|line| line.replace(&prefix, ""))
        .collect()
}

#[test]
fn declaration_emit_follows_go_load_order() {
    let tsconfig = r#"{ "compilerOptions": { "strict": true, "declaration": true, "emitDeclarationOnly": true, "outDir": "out", "module": "esnext", "moduleResolution": "bundler", "rootDir": "..", "types": [] }, "files": ["main.ts"] }"#;
    // Relative first: Go loads `user.ts` through the package name first, so `value.ts` is an external library.
    for (main, expected) in [
        (
            PACKAGE_FIRST,
            &[
                "TSFILE: app/out/pkg/src/value.d.ts",
                "TSFILE: app/out/pkg/src/user.d.ts",
                "TSFILE: app/out/app/main.d.ts",
            ][..],
        ),
        (
            RELATIVE_FIRST,
            &["TSFILE: app/out/pkg/src/user.d.ts", "TSFILE: app/out/app/main.d.ts"][..],
        ),
    ] {
        for single_threaded in [true, false] {
            let dir = scratch(
                "emit",
                tsconfig,
                &[
                    ("app/main.ts", main),
                    (
                        "pkg/src/user.ts",
                        "import type { Value } from \"./value\"\nexport type User = { value: Value }\n",
                    ),
                    ("pkg/src/value.ts", "export type Value = { id: string }\n"),
                ],
            );
            let actual = tsrs(&dir, &["--listEmittedFiles"], single_threaded);
            assert_eq!(actual, expected, "{main}single-threaded: {single_threaded}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

#[test]
fn node_modules_js_depth_follows_go_load_order() {
    // `user.ts` imports the JS package `dep`: at depth 1 when `user.ts` is loaded at depth 0, at depth 2 (beyond
    // maxNodeModuleJsDepth) when it is loaded through the package name first.
    let tsconfig = r#"{ "compilerOptions": { "strict": true, "noEmit": true, "allowJs": true, "maxNodeModuleJsDepth": 1, "module": "esnext", "moduleResolution": "bundler", "types": [] }, "files": ["main.ts"] }"#;
    let missing_types = "../pkg/src/user.ts(1,21): error TS7016: Could not find a declaration file for module 'dep'. 'pkg/node_modules/dep/index.js' implicitly has an 'any' type.";
    for (main, expected) in [
        (PACKAGE_FIRST, &["pkg/node_modules/dep/index.js", "pkg/src/user.ts", "app/main.ts"][..]),
        (
            RELATIVE_FIRST,
            &[
                missing_types,
                "  Try `npm i --save-dev @types/dep` if it exists or add a new declaration (.d.ts) file containing `declare module 'dep';`",
                "pkg/src/user.ts",
                "app/main.ts",
            ][..],
        ),
    ] {
        for single_threaded in [true, false] {
            let dir = scratch(
                "js-depth",
                tsconfig,
                &[
                    ("app/main.ts", &main.replace("import type", "import")),
                    (
                        "pkg/src/user.ts",
                        "import { dep } from \"dep\"\nexport type User = typeof dep\n",
                    ),
                    (
                        "pkg/node_modules/dep/package.json",
                        r#"{ "name": "dep", "version": "1.0.0", "main": "index.js" }"#,
                    ),
                    ("pkg/node_modules/dep/index.js", "exports.dep = 1;\n"),
                ],
            );
            let actual = tsrs(&dir, &["--listFiles"], single_threaded);
            assert_eq!(actual, expected, "{main}single-threaded: {single_threaded}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
