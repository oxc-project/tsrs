// Go `getMapRootDirectory` first resolves `mapRoot` with `SourceMapLocation.ResolveDirectory`. A rooted value is
// normalized (`.`, `..` and empty segments are removed), and a relative value is resolved against the common source
// directory (`^` becomes `<dir>/^`). tsrs only calls `normalize_slashes` on the value, so the `sourceMappingURL`
// comment keeps `a/../m`, and `^` is taken as a rooted path, which changes the `sources` of the map. Expected values
// were taken from tsgo built from the pinned ts-ref commit on the same projects.

use std::path::{Path, PathBuf};
use std::process::Command;

// `@DIR@` in `map_root` stands for the real path of the project directory.
fn scratch(name: &str, map_root: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-maproot-not-resolved-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The compilers print the real path of the working directory.
    let dir = std::fs::canonicalize(&dir).unwrap();
    let map_root = map_root.replace("@DIR@", &dir.display().to_string());
    std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
    let config = format!(r#"{{"compilerOptions":{{"sourceMap":true,"mapRoot":"{map_root}"}}}}"#);
    std::fs::write(dir.join("tsconfig.json"), config).unwrap();
    dir
}

fn build(dir: &Path) {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap();
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn last_line(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap().lines().last().unwrap().to_string()
}

#[test]
fn url_map_root_with_dot_dot_segment() {
    let dir = scratch("url", "https://example.com/a/../m");
    build(&dir);
    let url = last_line(&dir.join("a.js"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(url, "//# sourceMappingURL=https://example.com/m/a.js.map");
}

#[test]
fn absolute_map_root_with_dot_segment() {
    let dir = scratch("abs", "@DIR@/./m");
    build(&dir);
    let url = last_line(&dir.join("a.js"));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(url, format!("//# sourceMappingURL={}/m/a.js.map", dir.display()));
}

#[test]
fn caret_map_root_is_relative() {
    let dir = scratch("caret", "^");
    build(&dir);
    let url = last_line(&dir.join("a.js"));
    let map = std::fs::read_to_string(dir.join("a.js.map")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(url, "//# sourceMappingURL=%5E/a.js.map");
    assert_eq!(
        map,
        r#"{"version":3,"file":"a.js","sources":["../a.ts"],"names":[],"mappings":"AAAA,MAAM,CAAC,MAAM,CAAC,GAAG,CAAC,CAAC"}"#
    );
}
