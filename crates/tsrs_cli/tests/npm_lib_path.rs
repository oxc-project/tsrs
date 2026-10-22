// Library selection follows the build mode: npm (noembed) binaries read libraries beside the executable;
// standalone embedded binaries use bundled libraries even when disk libraries are present.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsrs_vfs::osvfs;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tsrs-npm-lib-path-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("project")).unwrap();
    std::fs::write(
        dir.join("project/tsconfig.json"),
        include_str!("../../../testdata/regressions/noembed-adjacent-libraries/tsconfig.json"),
    )
    .unwrap();
    std::fs::write(
        dir.join("project/a.ts"),
        "const names: Array<string> = [\"ok\"];\nnames.push(\"next\");\n",
    )
    .unwrap();
    // Compiler paths use ordinary drive paths on Windows, not canonicalize's
    // verbatim paths. Resolve macOS's temporary-directory symlink as well.
    PathBuf::from(osvfs::fs().realpath(&dir.to_string_lossy()))
}

// The test binary under `bin`, linked rather than copied where the file system allows it (it is large).
fn install_binary(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(if cfg!(windows) { "tsrs.exe" } else { "tsrs" });
    if std::fs::hard_link(env!("CARGO_BIN_EXE_tsrs"), &exe).is_err() {
        std::fs::copy(env!("CARGO_BIN_EXE_tsrs"), &exe).unwrap();
    }
    exe
}

fn run(exe: &Path, dir: &Path) -> Output {
    Command::new(exe)
        .args(["-p", "tsconfig.json", "--pretty", "false", "--listFiles"])
        .current_dir(dir.join("project"))
        .env_remove("TSRS_LIB_PATH")
        .output()
        .unwrap()
}

fn list_files(exe: &Path, dir: &Path) -> Vec<String> {
    let out = run(exe, dir);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect()
}

#[test]
fn libraries_next_to_the_binary_follow_the_build_mode() {
    let dir = scratch("beside");
    let exe = install_binary(&dir);
    for entry in
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../tsrs_vfs/libs")).unwrap()
    {
        let path = entry.unwrap().path();
        std::fs::copy(&path, dir.join("bin").join(path.file_name().unwrap())).unwrap();
    }
    let bin = dir.join("bin").to_string_lossy().replace('\\', "/");
    let files = list_files(&exe, &dir);
    if cfg!(feature = "noembed") {
        assert!(files.contains(&format!("{bin}/lib.es5.d.ts")), "{files:?}");
        assert!(!files.iter().any(|f| f.starts_with("bundled:")), "{files:?}");
        std::fs::write(
            dir.join("project/a.ts"),
            include_str!("../../../testdata/regressions/noembed-adjacent-libraries/a.ts"),
        )
        .unwrap();
        let out = run(&exe, &dir);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(out.stderr.is_empty(), "{out:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        let diagnostics: Vec<_> = stdout.lines().filter(|line| line.contains("error TS")).collect();
        assert_eq!(
            diagnostics.join("\n") + "\n",
            include_str!("../../../testdata/regressions/noembed-adjacent-libraries/expected.txt"),
            "{stdout}"
        );
    } else {
        assert!(files.contains(&"bundled:///libs/lib.es5.d.ts".to_string()), "{files:?}");
        assert!(!files.iter().any(|f| f.starts_with(&bin)), "{files:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_disk_libraries_follow_the_build_mode() {
    let dir = scratch("missing");
    let exe = install_binary(&dir);
    if cfg!(feature = "noembed") {
        let out = run(&exe, &dir);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "noembed must not fall back to embedded libraries");
        assert!(stderr.contains("this executable may be misplaced"), "{stderr}");
    } else {
        let files = list_files(&exe, &dir);
        assert!(files.contains(&"bundled:///libs/lib.es5.d.ts".to_string()), "{files:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
