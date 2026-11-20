// Go decodes each `root`, `referencedMap`, and `resolvedRoot` entry of a `.tsbuildinfo` into a Go array (`[2]int`) with
// `encoding/json/v2`. That decoder fails if the JSON array has another length, so `ReadBuildInfo` returns nil and tsgo
// builds without an old state and writes the file again. `as_uint_tuple` in tsrs pads a short array with zeros and drops
// extra elements, so it accepts the entry. For `"referencedMap":[[5]]` the list ID is 0 and `to_file_path_set` panics
// (`attempt to subtract with overflow`, exit 5). For the other lengths tsrs keeps the bad file and writes nothing.
// The panic also happens in the release build. Expected results from tsgo: exit 0, no output, file written again.

use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tsrs-buildinfo-tuple-length-{}-{}",
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

fn tsrs(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .args(["--pretty", "false", "--singleThreaded"])
        .current_dir(dir)
        .env_remove("TSRS_FAST")
        .output()
        .unwrap()
}

const PROJECT: &[(&str, &str)] = &[
    (
        "tsconfig.json",
        r#"{"compilerOptions":{"lib":["es5"],"types":[],"incremental":true},"files":["a.ts","b.ts"]}"#,
    ),
    ("a.ts", "import { b } from \"./b\";\nexport const a: number = b + 1;\n"),
    ("b.ts", "export const b = 2;\n"),
];

// Builds the project, replaces `from` with `to` in the `.tsbuildinfo`, and builds again. The second build must treat
// the edited file as unreadable: it builds without an old state and writes the same text as the first build.
fn build_after_edit(name: &str, from: &str, to: &str) {
    let dir = scratch(name, PROJECT);
    let first = tsrs(&dir, &["-p", "."]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "first build: {}",
        String::from_utf8_lossy(&first.stdout)
    );
    let path = dir.join("tsconfig.tsbuildinfo");
    let original = std::fs::read_to_string(&path).unwrap();
    assert!(original.contains(from), "the first build did not write {from}: {original}");
    std::fs::write(&path, original.replacen(from, to, 1)).unwrap();

    let second = tsrs(&dir, &["-p", "."]);
    assert_eq!(
        (second.status.code(), String::from_utf8_lossy(&second.stdout).as_ref()),
        (Some(0), ""),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, original, "the second build must write the file again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn referenced_map_entry_with_one_element() {
    build_after_edit("refmap-one", r#""referencedMap":[[5,1]]"#, r#""referencedMap":[[5]]"#);
}

#[test]
fn referenced_map_entry_with_three_elements() {
    build_after_edit("refmap-three", r#""referencedMap":[[5,1]]"#, r#""referencedMap":[[5,1,2]]"#);
}

#[test]
fn root_entry_with_three_elements() {
    build_after_edit("root-three", r#""root":[[4,5]]"#, r#""root":[[1,2,3]]"#);
}

#[test]
fn root_entry_with_no_elements() {
    build_after_edit("root-empty", r#""root":[[4,5]]"#, r#""root":[[]]"#);
}
