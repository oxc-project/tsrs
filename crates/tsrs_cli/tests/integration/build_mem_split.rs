// `TSRS_MEM_SPLIT=1` under `tsrs -b` with more than one builder: a program's own reports (parse end, check end) read
// every thread's arena and walk the allocator's pages, which is only safe while the threads that allocate are parked.
// With programs built at once the other builders keep allocating, and the walk crashed or spun. Only the report at
// exit runs then, without the allocator walk.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn mem_split_reports_at_exit_with_concurrent_builders() {
    let dir = std::env::temp_dir().join(format!("tsrs-build-mem-split-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Independent projects, so that both builders build at once.
    let mut references = Vec::new();
    for p in 0..24 {
        let project = dir.join(format!("p{p}"));
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(
            project.join("tsconfig.json"),
            r#"{ "compilerOptions": { "composite": true, "rootDir": "src", "outDir": "out", "strict": true }, "include": ["src"] }"#,
        )
        .unwrap();
        std::fs::write(
            project.join("src/index.ts"),
            format!("export const value{p}: number = {p};\n"),
        )
        .unwrap();
        references.push(format!(r#"{{ "path": "p{p}" }}"#));
    }
    std::fs::write(
        dir.join("tsconfig.json"),
        format!(r#"{{ "files": [], "references": [{}] }}"#, references.join(", ")),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["-b", "--builders", "4"])
        .current_dir(&dir)
        .env("TSRS_MEM_SPLIT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run tsrs");
    // The walk could also spin forever: fail instead of hanging the test run.
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(120) {
            child.kill().unwrap();
            panic!("tsrs -b with TSRS_MEM_SPLIT=1 did not finish in 120 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{:?}\n{}{stderr}",
        out.status,
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(stderr.contains("tsrs mem split: exit"), "{stderr}");
    assert!(stderr.contains("allocator walk): skipped"), "{stderr}");
    assert!(!stderr.contains("tsrs mem split: parse end"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
