// `TSRS_MEM_SPLIT=1` (`tsrs_core::memsplit`) on `tsrs -b`: a report walks the allocator's pages of every thread, so a
// builder that reported its program's `parse end` while other builders' checkers allocated corrupted the heap, and the
// build crashed (SIGSEGV) before the `exit` report. A build reports only `exit`; a single program keeps its points.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const OPTIONS: &str = r#""compilerOptions": { "composite": true, "strict": true, "target": "es2022", "module": "esnext", "moduleResolution": "bundler", "types": [], "skipLibCheck": true, "rootDir": "src", "outDir": "dist" }"#;

const PROJECTS: usize = 20;

fn workload(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-mem-split-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut references = Vec::new();
    for p in 0..PROJECTS {
        let project = dir.join(format!("p{p}"));
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("tsconfig.json"), format!("{{ {OPTIONS} }}")).unwrap();
        for f in 0..3 {
            std::fs::write(
                project.join(format!("src/f{f}.ts")),
                format!("export function f{p}_{f}(x: number): number {{ return x + {f}; }}\n"),
            )
            .unwrap();
        }
        references.push(format!(r#"{{ "path": "./p{p}" }}"#));
    }
    std::fs::write(
        dir.join("tsconfig.json"),
        format!(r#"{{ "files": [], "references": [{}] }}"#, references.join(", ")),
    )
    .unwrap();
    dir
}

/// Runs tsrs in `dir`; fails after two minutes (a corrupted heap also made the walk loop forever in a debug build).
fn run(dir: &Path, args: &[&str]) -> (ExitStatus, String) {
    let log = dir.join("stderr.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .current_dir(dir)
        .env("TSRS_MEM_SPLIT", "1")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .expect("run tsrs");
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("tsrs {args:?} hung\n{}", std::fs::read_to_string(&log).unwrap_or_default());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    (status, std::fs::read_to_string(&log).unwrap())
}

#[test]
fn build_prints_the_exit_report() {
    let dir = workload("build");
    let (status, stderr) = run(&dir, &["-b", "tsconfig.json"]);
    assert!(status.success(), "{status:?}\n{stderr}");
    assert!(stderr.contains("tsrs mem split: exit\n"), "{stderr}");
    assert!(!stderr.contains("tsrs mem split: parse end"), "{stderr}");
    assert!(dir.join(format!("p{}/dist/f0.js", PROJECTS - 1)).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn single_program_prints_its_points() {
    let dir = workload("program");
    let (status, stderr) = run(&dir, &["-p", "p0/tsconfig.json"]);
    assert!(status.success(), "{status:?}\n{stderr}");
    assert!(stderr.contains("tsrs mem split: parse end\n"), "{stderr}");
    assert!(stderr.contains("tsrs mem split: exit\n"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
