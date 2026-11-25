// EXPECTED is pinned tsgo's stdout for this project.

use std::process::Command;

const SOURCE: &str = "declare const f: number;
type X = typeof f<
>;
";

const TSCONFIG: &str = r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"esnext","types":[],"noEmit":true},"files":["a.ts"]}"#;

const EXPECTED: &str = "\u{1b}[96ma.ts\u{1b}[0m:\u{1b}[93m2\u{1b}[0m:\u{1b}[93m18\u{1b}[0m - \u{1b}[91merror\u{1b}[0m\u{1b}[90m TS1099: \u{1b}[0mType argument list cannot be empty.

\u{1b}[7m2\u{1b}[0m type X = typeof f<
\u{1b}[7m \u{1b}[0m \u{1b}[91m                 ~\u{1b}[0m
\u{1b}[7m3\u{1b}[0m >;
\u{1b}[7m \u{1b}[0m \u{1b}[91m~\u{1b}[0m

\u{1b}[96ma.ts\u{1b}[0m:\u{1b}[93m3\u{1b}[0m:\u{1b}[93m1\u{1b}[0m - \u{1b}[91merror\u{1b}[0m\u{1b}[90m TS2635: \u{1b}[0mType 'number' has no signatures for which the type argument list is applicable.



Found 2 errors in the same file, starting at: a.ts\u{1b}[90m:2\u{1b}[0m

";

#[test]
fn range_ending_on_an_earlier_line_has_no_snippet() {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-pretty-reversed-diagnostic-range-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.ts"), SOURCE).unwrap();
    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(&dir)
        .args(["-p", ".", "--pretty", "true", "--singleThreaded"])
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED);
    let _ = std::fs::remove_dir_all(&dir);
}
