// EXPECTED is pinned tsgo's declaration output for this project.

use std::process::Command;

const SOURCE: &str = "/** @callback F */

/**
 * @callback G
 * @param {string} s
 */

/** @type {G} */
export const g = (s) => s.length;
";

const TSCONFIG: &str = r#"{"compilerOptions":{"allowJs":true,"declaration":true,"emitDeclarationOnly":true,"stripInternal":true,"outDir":"out","strict":true,"target":"es2022","module":"esnext","types":[]},"files":["m.js"]}"#;

const EXPECTED: &str = "/** @callback F */
export type F = () => any;
export type G = (s: string) => any;
/**
 * @callback G
 * @param {string} s
 */
/** @type {G} */
export declare const g: G;
";

#[test]
fn callback_without_returns_keeps_its_any_return_type() {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-jsdoc-callback-strip-internal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.js"), SOURCE).unwrap();
    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(&dir)
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    assert_eq!(std::fs::read_to_string(dir.join("out/m.d.ts")).unwrap(), EXPECTED);
    let _ = std::fs::remove_dir_all(&dir);
}
