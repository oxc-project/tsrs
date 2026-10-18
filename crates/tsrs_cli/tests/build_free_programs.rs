// `tsrs -b` frees each project's program once the project is built (`Orchestrator::frees_built_programs`; Go's GC):
// its checkers, its snapshot and what its file loader allocated. Without that, a build of thousands of projects keeps
// every program and exhausts the arena reservation. The runs below use TSRS_ARENA_POISON=1 (freed arena memory is
// filled and kept), so a read of a freed program's data crashes or prints garbage instead of passing by luck.
//
// Graph: a; b references a; c references a and b. One builder, so the projects build in that order.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

const OPTIONS: &str = r#""compilerOptions": { "composite": true, "rootDir": "src", "outDir": "out", "module": "esnext", "moduleResolution": "bundler", "target": "es2022", "strict": true }"#;

fn graph(name: &str, a: &str, b: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-build-free-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let projects = [
        ("a", "", a.to_string()),
        ("b", r#""references": [{ "path": "../a" }],"#, b.to_string()),
        (
            "c",
            r#""references": [{ "path": "../a" }, { "path": "../b" }],"#,
            "import { a } from \"../../a/src/index\";\nimport { b } from \"../../b/src/index\";\nexport const c: number = a + b.x;\n"
                .to_string(),
        ),
    ];
    for (project, references, source) in projects {
        std::fs::create_dir_all(dir.join(project).join("src")).unwrap();
        std::fs::write(
            dir.join(project).join("tsconfig.json"),
            format!(r#"{{ {OPTIONS}, {references} "include": ["src"] }}"#),
        )
        .unwrap();
        std::fs::write(dir.join(project).join("src/index.ts"), source).unwrap();
    }
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "files": [], "references": [{ "path": "a" }, { "path": "b" }, { "path": "c" }] }"#,
    )
    .unwrap();
    dir
}

fn build(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-b", "--builders", "1"])
        .args(args)
        .current_dir(dir)
        .env("TSRS_ARENA_POISON", "1")
        .output()
        .expect("run tsrs")
}

fn stdout(out: &Output) -> String {
    let text = String::from_utf8_lossy(&out.stdout);
    // Drop the colors of `--pretty`.
    let mut plain = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            chars.by_ref().find(|&c| c == 'm');
        } else {
            plain.push(c);
        }
    }
    plain
}

// The error summary reads the files of the errors after their programs were freed: a project that reported
// diagnostics keeps the regions its files were parsed into.
#[test]
fn error_summary_reads_the_files_of_freed_programs() {
    let dir = graph(
        "errors",
        "export interface A { x: number }\nexport const a: number = \"one\";\n",
        "import { a, type A } from \"../../a/src/index\";\nexport const b: A = { x: a, y: 1 };\n",
    );
    let out = build(&dir, &["--pretty", "true"]);
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    let text = stdout(&out);
    assert!(
        text.ends_with(
            "Found 2 errors in 2 files.\n\nErrors  Files\n     1  a/src/index.ts:2\n     1  b/src/index.ts:2\n\n"
        ),
        "{text}"
    );
    assert!(text.contains("export const b: A = { x: a, y: 1 };"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// After a full build, b changes: a is up to date, so b's program is the first one built, and it fills a's command
// line's output maps before any shared declaration file is parsed into a region of its own. They must not go to b's
// loader regions, which are freed before c reads the same maps (`tsrs_core::arena::route_by_owner`).
#[test]
fn incremental_build_after_up_to_date_references() {
    let dir = graph(
        "incremental",
        "export interface A { x: number }\nexport const a: number = 1;\n",
        "import { a, type A } from \"../../a/src/index\";\nexport const b: A = { x: a };\n",
    );
    let out = build(&dir, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let b = dir.join("b/src/index.ts");
    let mut source = std::fs::read_to_string(&b).unwrap();
    source.push_str("export const b2: number = 2;\n");
    std::fs::write(&b, source).unwrap();
    let file = std::fs::File::options().write(true).open(&b).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(60)).unwrap();
    let out = build(&dir, &["--verbose"]);
    let text = stdout(&out);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("Project 'a/tsconfig.json' is up to date"), "{text}");
    assert!(text.contains("Building project 'b/tsconfig.json'"), "{text}");
    assert!(text.contains("Building project 'c/tsconfig.json'"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
