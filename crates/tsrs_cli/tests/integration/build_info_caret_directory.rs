// Go `GetBuildInfoFileName` uses the config file name when the path of the config file relative to `rootDir` would
// start with a root, such as `^/tsconfig.json` for a directory named `^`. tsrs joined that text to `outDir` with
// `resolve_path`, which drops `outDir`, and wrote `^/tsconfig.tsbuildinfo` relative to the working directory. Expected
// values were taken from tsgo built from the pinned ts-ref commit.

use std::process::Command;

#[test]
fn build_info_of_config_below_caret_directory() {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-build-info-caret-directory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("^")).unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("^/tsconfig.json"),
        r#"{"compilerOptions":{"incremental":true,"outDir":"../out","rootDir":".."},"files":["a.ts"]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("^/a.ts"), "export const a = 1;\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", "../^", "--pretty", "false", "--singleThreaded"])
        .current_dir(dir.join("sub"))
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    let written = (
        dir.join("^/tsconfig.tsbuildinfo").is_file(),
        dir.join("sub/^/tsconfig.tsbuildinfo").exists(),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(written, (true, false));
}
