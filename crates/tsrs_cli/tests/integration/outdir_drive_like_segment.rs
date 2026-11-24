// Go `GetSourceFileNameInNewDir` appends the part of an input path below the common source directory to `outDir` as
// text (`ResolveRelativeFile`). The Rust port joined the part with `combine_paths`, which drops `outDir` when the part
// starts with a segment that reads as a root: `^/` (the root of an untitled file) or a drive name such as `c:/`.
// tsrs then wrote the outputs of `src/^/a.ts` to `^/a.js` below the project directory, and tsgo writes `out/^/a.js`.
// Expected values were taken from tsgo built from the pinned ts-ref commit: exit status 0, empty stdout, and the
// output files below `out/`. The second test uses a directory named `c:`, which is not valid on Windows.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-outdir-drive-like-segment-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    dir
}

fn tsrs(args: &[&str], dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .args(["--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

fn assert_clean(out: &std::process::Output) {
    assert_eq!(
        (out.status.code(), String::from_utf8_lossy(&out.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn directory_named_caret_below_root_dir() {
    let dir = scratch(
        "caret",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"outDir":"out","rootDir":"src","declaration":true}}"#,
            ),
            ("src/b.ts", "export const y = 1;\n"),
            ("src/^/a.ts", "export const x = 1;\n"),
        ],
    );
    assert_clean(&tsrs(&["-p", "."], &dir));
    assert!(dir.join("out/^/a.js").is_file(), "out/^/a.js is missing");
    assert!(dir.join("out/^/a.d.ts").is_file(), "out/^/a.d.ts is missing");
    assert!(!dir.join("^/a.js").exists(), "^/a.js is outside outDir");
    assert!(!dir.join("^/a.d.ts").exists(), "^/a.d.ts is outside outDir");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(windows))]
#[test]
fn imported_directory_named_like_drive_root() {
    let dir = scratch(
        "drive",
        &[
            ("tsconfig.json", r#"{"compilerOptions":{"outDir":"out"},"files":["a.ts"]}"#),
            ("a.ts", "import \"./c:/x\";\n"),
            ("c:/x.ts", "export const x = 1;\n"),
        ],
    );
    assert_clean(&tsrs(&["-p", "."], &dir));
    assert!(dir.join("out/c:/x.js").is_file(), "out/c:/x.js is missing");
    assert!(!dir.join("c:/x.js").exists(), "c:/x.js is outside outDir");
    let _ = std::fs::remove_dir_all(&dir);
}
