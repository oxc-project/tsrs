// Lazy declaration members must preserve incremental output and survive the freeing of an earlier project's
// program. A build that checks any declaration files must retain their bind diagnostics, even when an earlier
// project shares the same file with skipLibCheck enabled.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

fn write(dir: &Path, name: &str, text: &str) {
    let file = dir.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn project(dir: &Path, name: &str, options: &str, references: &str) {
    write(
        dir,
        &format!("{name}/tsconfig.json"),
        &format!(
            r#"{{ "compilerOptions": {{ "composite": true, "strict": true, "types": [],
                "module": "esnext", "moduleResolution": "bundler", "target": "es2022",
                "rootDir": "src", "outDir": "out", {options} }},
                "references": [{references}], "include": ["src"] }}"#
        ),
    );
}

fn graph(name: &str, options: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-lazy-dts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    project(&dir, "a", options, "");
    project(&dir, "b", options, r#"{ "path": "../a" }"#);
    write(
        &dir,
        "tsconfig.json",
        r#"{ "files": [], "references": [{ "path": "a" }, { "path": "b" }] }"#,
    );
    write(
        &dir,
        "lib.d.ts",
        "export interface First { value: string; nested: { count: number } }\n\
         export interface Later { first: First; extra: number }\n\
         export declare class Box { static make(): First; value: string }\n\
         export declare namespace Box { const tag: 'box'; }\n\
         export interface Unused { method(value: number, value: string): void }\n",
    );
    write(
        &dir,
        "a/src/index.ts",
        "import { Box } from '../../lib';\nexport const first = Box.make();\n",
    );
    write(
        &dir,
        "b/src/index.ts",
        "import { first } from '../../a/src/index';\n\
         import type { Later } from '../../lib';\n\
         export const later: Later = { first, extra: 1 };\n",
    );
    dir
}

fn run(dir: &Path, args: &[&str], lazy: bool) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .args(["--pretty", "false"])
        .current_dir(dir)
        .env("TSRS_LAZY_DTS", if lazy { "1" } else { "0" })
        .env("TSRS_ARENA_POISON", "1")
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn outputs(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for project in ["a", "b"] {
        for name in ["out/index.js", "out/index.d.ts", "tsconfig.tsbuildinfo"] {
            let name = format!("{project}/{name}");
            if let Ok(bytes) = std::fs::read(dir.join(&name)) {
                files.insert(name, bytes);
            }
        }
    }
    files
}

#[test]
fn lazy_declarations_preserve_builds_and_incremental_edits() {
    for options in [r#""skipLibCheck": true"#, r#""noCheck": true"#] {
        let dir = graph(if options.contains("skip") { "skip" } else { "nocheck" }, options);
        for args in [vec!["-p", "a"], vec!["-b", "--builders", "1"], vec!["-b"]] {
            let mut eager = Vec::new();
            for lazy in [false, true] {
                for project in ["a", "b"] {
                    let _ = std::fs::remove_dir_all(dir.join(project).join("out"));
                    let _ = std::fs::remove_file(dir.join(project).join("tsconfig.tsbuildinfo"));
                }
                write(
                    &dir,
                    "a/src/index.ts",
                    "import { Box } from '../../lib';\nexport const first = Box.make();\n",
                );
                for step in 0..3 {
                    if step == 2 {
                        // Change a's declaration shape, forcing b to consume a newly emitted declaration file.
                        write(
                            &dir,
                            "a/src/index.ts",
                            "import { Box } from '../../lib';\n\
                             export const first = Box.make();\nexport const tag = Box.tag;\n",
                        );
                        let file = std::fs::File::options()
                            .write(true)
                            .open(dir.join("a/src/index.ts"))
                            .unwrap();
                        file.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
                    }
                    let result = run(&dir, &args, lazy);
                    assert_eq!(result.0, Some(0), "{args:?}: {result:?}");
                    assert!(result.2.is_empty(), "{result:?}");
                    let snapshot = (result, outputs(&dir));
                    assert!(snapshot.1.contains_key("a/out/index.d.ts"));
                    if lazy {
                        assert_eq!(snapshot, eager[step], "{args:?}, step {step}");
                    } else {
                        eager.push(snapshot);
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn mixed_build_retains_shared_declaration_bind_diagnostics() {
    let dir = graph("mixed", r#""skipLibCheck": true"#);
    project(&dir, "b", r#""skipLibCheck": false"#, r#"{ "path": "../a" }"#);
    let args = ["-b", "--builders", "1", "--force"];
    let eager = run(&dir, &args, false);
    assert_eq!(eager.0, Some(2), "{eager:?}");
    assert!(eager.1.contains("Duplicate identifier 'value'"), "{eager:?}");
    assert_eq!(run(&dir, &args, true), eager);
    let _ = std::fs::remove_dir_all(dir);
}
