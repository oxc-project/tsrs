// The expected values are pinned tsgo's output for this project. After the invalid `¬`, the printer maps the end
// of the `{` token one byte into the character, and Go counts each byte of a split character as one UTF-16 unit.
// `n.ts` also maps a position before a cached one on the same line.

use std::process::Command;

const TSCONFIG: &str =
    r#"{"compilerOptions":{"sourceMap":true,"outDir":"out","types":[]},"files":["m.ts","n.ts"]}"#;

const DIAGNOSTICS: &str = "m.ts(1,17): error TS1127: Invalid character.
n.ts(1,12): error TS1127: Invalid character.
n.ts(1,14): error TS1130: 'case' or 'default' expected.
";

#[test]
fn invalid_multibyte_character_maps_like_go() {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-sourcemap-mid-character-position-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.ts"), "function Foo () \u{ac} { }\n").unwrap();
    std::fs::write(dir.join("n.ts"), "switch (a) \u{ac} {}\n").unwrap();
    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(&dir)
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), DIAGNOSTICS);
    assert_eq!(
        std::fs::read_to_string(dir.join("out/m.js.map")).unwrap(),
        r#"{"version":3,"file":"m.js","sources":["../m.ts"],"names":[],"mappings":";AAAA,SAAS,GAAG,KAAI,CAAC,AAAF;AAAG,CAAC,CAAC,CAAC"}"#
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("out/n.js.map")).unwrap(),
        r#"{"version":3,"file":"n.js","sources":["../n.ts"],"names":[],"mappings":";AAAA,QAAQ,CAAC,EAAE,CAAC;AAAE,CAAC,AAAH;AAAC,CAAC,CAAA,CAAC"}"#
    );
    let _ = std::fs::remove_dir_all(&dir);
}
