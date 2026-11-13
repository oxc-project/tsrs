// The declaration signatures a warm incremental run computes for the files a change affects are emitted with work
// stealing in fast mode (notes/perf-signature-emit-stealing.md). The run must print and store what the static walk
// prints and stores (`TSRS_SIGNATURE_STEAL=0`): the same output and the same build info bytes after every step of an
// edit sequence. A file that imports the hub of the program is in the walk after every hub edit; adding an async
// generator under `lib: ["es2015"]` makes its signature emit report a global diagnostic (TS2318), which takes the
// walk's fallback (the walk again, statically, on fresh checkers).

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

fn tsrs(dir: &Path, checkers: &str, steal: bool, extra: &[&str]) -> (String, Vec<u8>) {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(dir)
        .args(["-p", ".", "--pretty", "false", "--fast", "--checkers", checkers])
        .args(extra)
        .env_remove("TSRS_CHECKER_ASSIGNMENT")
        .env_remove("TSRS_HISTORY")
        .env_remove("TSRS_INCREMENTAL_STEAL")
        .env("TSRS_SIGNATURE_STEAL", if steal { "1" } else { "0" })
        .output()
        .unwrap();
    let stdout = format!("{}exit {:?}", String::from_utf8(out.stdout).unwrap(), out.status.code());
    (stdout, std::fs::read(dir.join("tsconfig.tsbuildinfo")).unwrap())
}

#[test]
fn signature_emit_stealing_matches_static_walk() {
    let dir = std::env::temp_dir().join(format!("tsrs-signature-steal-{}", std::process::id()));
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
    let hub = "export interface Item { id: number }\nexport function make(n: number): Item { return { id: n }; }\n";
    write("hub/core.ts", hub);
    let mut originals = Vec::new();
    for d in 1..=4 {
        for f in 0..4 {
            let name = format!("p{d}/f{f}.ts");
            let mut text = String::from("import { make, type Item } from '../hub/core';\n");
            writeln!(text, "export function use{f}(x: number) {{ return make(x + {f}); }}")
                .unwrap();
            writeln!(text, "export const pair{f} = {{ item: use{f}(1), list: [use{f}(2)] }};")
                .unwrap();
            if f == 1 {
                writeln!(text, "export const bad{d}: string = use{f}(1).id;").unwrap();
            }
            for i in 0..20 {
                writeln!(
                    text,
                    "export function g{i}(a: Item[]) {{ return a.map(e => ({{ ...e, z: e.id + {i} }})); }}"
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
        ("hub export added", Box::new(|| append("hub/core.ts", "export const added = 1;\n"))),
        (
            "hub shape changed",
            Box::new(|| append("hub/core.ts", "export interface Item { name?: string }\n")),
        ),
        ("no change", Box::new(|| {})),
        ("global script edited", Box::new(|| append("global.ts", "declare var H: string;\n"))),
        ("generator added", Box::new(|| append("p2/f0.ts", generator))),
        (
            "hub edit with the generator",
            Box::new(|| append("hub/core.ts", "export const added2 = 2;\n")),
        ),
        ("generator removed", Box::new(|| write("p2/f0.ts", &original("p2/f0.ts")))),
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
                let (stdout, bytes) = tsrs(&dir, checkers, steal, &[]);
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
        for (name, text) in &originals {
            write(name, text);
        }
        write("hub/core.ts", hub);
        write("global.ts", "declare var G: number;\nconst gs: number = G;\n");
    }
    // A signature emit that reports a global diagnostic takes the fallback.
    let _ = std::fs::remove_file(dir.join("tsconfig.tsbuildinfo"));
    tsrs(&dir, "4", true, &[]);
    append("p3/f2.ts", generator);
    let (stdout, _) = tsrs(&dir, "4", true, &["--extendedDiagnostics"]);
    assert!(stdout.contains("Signatures: walk rerun statically"), "{stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}
