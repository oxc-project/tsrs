// The incremental check pass steals work in fast mode (notes/perf-incremental-pass-stealing.md). It must store and
// print what the static pass stores and prints (`TSRS_INCREMENTAL_STEAL=0`): the same output and the same build info
// bytes after every step of an edit sequence. The program is given `lib: ["es2015"]`, so an async generator makes its
// checker report a global diagnostic (TS2318), which the incremental pass attributes to the file that found it first
// on its checker. p2/f0.ts has one from the start, so the cold run takes the stolen pass's fallback (a static check
// on fresh checkers); a warm run whose declaration signatures already reported one checks statically from the start.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

fn run(dir: &Path, checkers: &str, steal: bool) -> (String, Vec<u8>) {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(dir)
        .args(["-p", ".", "--pretty", "false", "--fast", "--checkers", checkers])
        .env_remove("TSRS_CHECKER_ASSIGNMENT")
        .env_remove("TSRS_HISTORY")
        .env("TSRS_INCREMENTAL_STEAL", if steal { "1" } else { "0" })
        .output()
        .unwrap();
    let stdout = format!("{}exit {:?}", String::from_utf8(out.stdout).unwrap(), out.status.code());
    (stdout, std::fs::read(dir.join("tsconfig.tsbuildinfo")).unwrap())
}

#[test]
fn incremental_pass_stealing_matches_static_pass() {
    let dir = std::env::temp_dir().join(format!("tsrs-incremental-steal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let write = |name: &str, text: &str| {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let append = |name: &str, text: &str| {
        let old = std::fs::read_to_string(dir.join(name)).unwrap();
        write(name, &(old + text));
    };
    write(
        "tsconfig.json",
        r#"{"compilerOptions":{"target":"es2018","lib":["es2015"],"strict":true,"noEmit":true,"incremental":true,"module":"esnext","moduleResolution":"bundler","types":[]},"include":["**/*.ts"]}"#,
    );
    write(
        "hub/core.ts",
        "export interface Item { id: number }\nexport function make(n: number): Item { return { id: n }; }\n",
    );
    let mut originals = Vec::new();
    for d in 1..=4 {
        for f in 0..4 {
            let name = format!("p{d}/f{f}.ts");
            let mut text = format!(
                "import {{ make, type Item }} from '../hub/core';\nexport function use{f}(x: number): Item {{ return make(x + {f}); }}\n"
            );
            if f == 1 {
                writeln!(text, "export const bad{d}: string = use{f}(1).id;").unwrap();
            }
            if d == 2 && f < 2 {
                text.push_str("export async function* first() { yield 0; }\n");
            }
            for i in 0..20 {
                writeln!(
                    text,
                    "export function g{i}(a: {{ x: number }}[]) {{ return a.map(e => ({{ ...e, z: e.x + {i} }})); }}"
                )
                .unwrap();
            }
            write(&name, &text);
            originals.push((name, text));
        }
    }
    write("global.ts", "declare var G: number;\nconst gs: number = G;\n");
    let original = |name: &str| originals.iter().find(|(n, _)| n == name).unwrap().1.clone();
    let generator = "export async function* agen() { yield 1; }\n";
    let steps: Vec<(&str, Box<dyn Fn()>)> = vec![
        ("cold", Box::new(|| {})),
        ("no change", Box::new(|| {})),
        ("exported shape", Box::new(|| append("hub/core.ts", "export const added = 1;\n"))),
        ("global script error", Box::new(|| append("global.ts", "const gs2: string = G;\n"))),
        (
            "global diagnostic in two files",
            Box::new(|| {
                append("p1/f2.ts", generator);
                append("p4/f3.ts", generator);
            }),
        ),
        ("no change after it", Box::new(|| {})),
        ("one of them removed", Box::new(|| write("p1/f2.ts", &original("p1/f2.ts")))),
        ("both removed", Box::new(|| write("p4/f3.ts", &original("p4/f3.ts")))),
        (
            "first one removed",
            Box::new(|| {
                write(
                    "p2/f0.ts",
                    &original("p2/f0.ts")
                        .replace("export async function* first() { yield 0; }\n", ""),
                );
            }),
        ),
    ];
    for checkers in ["2", "4"] {
        let _ = std::fs::remove_file(dir.join("tsconfig.tsbuildinfo"));
        let (mut stolen_info, mut static_info) = (None::<Vec<u8>>, None::<Vec<u8>>);
        for (name, step) in &steps {
            step();
            let mut stolen_run = None;
            for (steal, info) in [(true, &mut stolen_info), (false, &mut static_info)] {
                match info {
                    Some(bytes) => std::fs::write(dir.join("tsconfig.tsbuildinfo"), bytes).unwrap(),
                    None => {
                        let _ = std::fs::remove_file(dir.join("tsconfig.tsbuildinfo"));
                    }
                }
                let (stdout, bytes) = run(&dir, checkers, steal);
                *info = Some(bytes.clone());
                match &stolen_run {
                    None => stolen_run = Some((stdout, bytes)),
                    Some((stolen_stdout, stolen_bytes)) => {
                        assert_eq!(stolen_stdout, &stdout, "{name}, {checkers} checkers: output");
                        assert!(stolen_bytes == &bytes, "{name}, {checkers} checkers: build info");
                    }
                }
            }
        }
        // Restore the sources for the next checker count.
        for (name, text) in &originals {
            write(name, text);
        }
        write(
            "hub/core.ts",
            "export interface Item { id: number }\nexport function make(n: number): Item { return { id: n }; }\n",
        );
        write("global.ts", "declare var G: number;\nconst gs: number = G;\n");
    }
    // The cold run found the global diagnostic in the stolen pass and checked again statically.
    let _ = std::fs::remove_file(dir.join("tsconfig.tsbuildinfo"));
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(&dir)
        .args([
            "-p",
            ".",
            "--pretty",
            "false",
            "--fast",
            "--checkers",
            "4",
            "--extendedDiagnostics",
        ])
        .env_remove("TSRS_CHECKER_ASSIGNMENT")
        .env_remove("TSRS_INCREMENTAL_STEAL")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("TS2318"), "{stdout}");
    assert!(stdout.contains("Checkers: incremental pass rechecked statically"), "{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}
