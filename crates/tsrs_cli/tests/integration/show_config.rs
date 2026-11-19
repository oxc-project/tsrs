// `--showConfig` prints the effective tsconfig on stdout, with no trailing newline, and builds nothing. The expected
// output was taken from tsgo built from the pinned ts-ref commit on the same projects.

use std::process::{Command, Output};

fn show_config(name: &str, config: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("tsrs-show-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("tsconfig.json"), config).unwrap();
    std::fs::write(dir.join("src/index.ts"), "export const a = 1;\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .arg("--showConfig")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!dir.join("src/index.js").exists(), "--showConfig emitted");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn assert_shows(out: &Output, expected: &str) {
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn show_config_prints_implied_options() {
    let out = show_config(
        "implied",
        r#"{"files":["src/index.ts"],"compilerOptions":{"module":"nodenext"}}"#,
    );
    assert_shows(
        &out,
        r#"{
    "compilerOptions": {
        "module": "nodenext",
        "moduleResolution": "nodenext",
        "moduleDetection": "force"
    },
    "files": [
        "./src/index.ts"
    ]
}"#,
    );
}

#[test]
fn show_config_omits_unknown_options() {
    let out = show_config(
        "unknown",
        r#"{"files":["src/index.ts"],"compilerOptions":{"notAnOption":true}}"#,
    );
    assert_shows(
        &out,
        r#"{
    "compilerOptions": {},
    "files": [
        "./src/index.ts"
    ]
}"#,
    );
}
