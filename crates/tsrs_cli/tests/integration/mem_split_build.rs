// `TSRS_MEM_SPLIT=1` walks every heap page and reads every thread arena, which is safe only while no other thread
// allocates. A `tsc -b` builds projects on several builder threads and frees the built programs on a free thread: the
// per-program reports (`parse end`, `check end`) walked the heap while other builders allocated, and the exit report
// raced with the free thread, corrupting mimalloc's heap (an abort or a segfault). The programs of a build now report
// at exit only, and the build frees them in place.

use std::fmt::Write as _;
use std::process::Command;

#[test]
fn mem_split_reports_during_a_parallel_build() {
    let dir = std::env::temp_dir().join(format!("tsrs-mem-split-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut references = Vec::new();
    for p in 0..4 {
        let project = dir.join(format!("p{p}"));
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("tsconfig.json"),
            r#"{ "compilerOptions": { "composite": true, "strict": true, "rootDir": "src", "outDir": "out", "module": "esnext", "moduleResolution": "bundler", "target": "es2022", "types": [], "skipLibCheck": true }, "include": ["src"] }"#,
        )
        .unwrap();
        let mut source = String::new();
        for i in 0..20 {
            let _ = writeln!(
                source,
                "/** @param a first */\nexport function f{i}(a: number): number {{ return a + {i}; }}"
            );
        }
        std::fs::write(project.join("src/index.ts"), source).unwrap();
        references.push(format!(r#"{{ "path": "p{p}" }}"#));
    }
    std::fs::write(
        dir.join("tsconfig.json"),
        format!(r#"{{ "files": [], "references": [{}] }}"#, references.join(", ")),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-b", "--builders", "4"])
        .arg(dir.join("tsconfig.json"))
        .env("TSRS_MEM_SPLIT", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{:?}\n{stderr}", output.status);
    assert!(!stderr.contains("tsrs mem split: parse end"), "{stderr}");
    assert!(stderr.contains("tsrs mem split: exit\n"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
