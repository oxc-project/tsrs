use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use tsrs_core::P;
use tsrs_lint::engine::{ConfigHost, find_tsconfig, lint_worker_count};
use tsrs_vfs::{FS, vfstest, wrapvfs};

#[test]
fn worker_counts_follow_available_parallelism_unless_overridden() {
    for count in [1, 4, 8, 12] {
        assert_eq!(lint_worker_count(100, Some(count), 12), count);
    }
    assert_eq!(lint_worker_count(100, None, 12), 12);
    assert_eq!(lint_worker_count(100, Some(0), 12), 12);
    assert_eq!(lint_worker_count(100, None, 2), 2);
    assert_eq!(lint_worker_count(3, None, 12), 3);
    assert_eq!(lint_worker_count(1000, None, 512), 256);
    assert_eq!(lint_worker_count(3, Some(12), 12), 3);
    assert_eq!(lint_worker_count(1000, Some(usize::MAX), 12), 256);
}

#[test]
fn shared_project_discovery_reads_each_config_once() {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let mut files = vec![
        (
            "/project/tsconfig.json".to_owned(),
            r#"{"files":[],"references":[{"path":"./a"},{"path":"./b"},{"path":"."}]}"#.to_owned(),
        ),
        (
            "/project/a/tsconfig.json".to_owned(),
            r#"{"compilerOptions":{"composite":true},"include":["*.ts"],"references":[{"path":".."}]}"#.to_owned(),
        ),
        (
            "/project/b/tsconfig.json".to_owned(),
            r#"{"include":["*.ts"]}"#.to_owned(),
        ),
        (
            "/project/invalid/tsconfig.json".to_owned(),
            r#"{"compilerOptions":"#.to_owned(),
        ),
        ("/project/loose.ts".to_owned(), "export {};".to_owned()),
    ];
    for directory in ["a", "b"] {
        for index in 0..100 {
            files.push((format!("/project/{directory}/file{index}.ts"), "export {};".to_owned()));
        }
    }
    let fs = P::new(vfstest::from_map(files, true)).get();
    let reads = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
    let count_reads = Arc::clone(&reads);
    let fs = wrapvfs::wrap(
        fs,
        wrapvfs::Replacements {
            read_file: Some(Box::new(move |path| {
                *count_reads.lock().unwrap().entry(path.to_owned()).or_default() += 1;
                fs.read_file(path)
            })),
            ..Default::default()
        },
    );
    let host = ConfigHost::new(Arc::new(fs), "/project".into());
    let mut configs = FxHashMap::default();
    for directory in ["a", "b"] {
        for index in 0..100 {
            assert_eq!(
                find_tsconfig(&format!("/project/{directory}/file{index}.ts"), &host, &mut configs),
                Some(format!("/project/{directory}/tsconfig.json")),
            );
        }
    }
    assert_eq!(find_tsconfig("/project/loose.ts", &host, &mut configs), None);
    assert_eq!(find_tsconfig("/project/loose.ts", &host, &mut configs), None);
    for name in ["first", "second"] {
        assert_eq!(
            find_tsconfig(&format!("/project/invalid/{name}.ts"), &host, &mut configs),
            None
        );
    }
    let reads = reads.lock().unwrap();
    for path in [
        "/project/tsconfig.json",
        "/project/a/tsconfig.json",
        "/project/b/tsconfig.json",
        "/project/invalid/tsconfig.json",
    ] {
        assert_eq!(reads.get(path), Some(&1), "{path}");
    }
}

#[test]
fn project_discovery_uses_canonical_file_membership() {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let fs = vfstest::from_map(
        [
            (
                "/Project/tsconfig.json",
                r#"{"compilerOptions":{"composite":true},"files":["./src/../src/a.ts"]}"#,
            ),
            ("/Project/src/a.ts", "export {};"),
        ],
        false,
    );
    let host = ConfigHost::new(Arc::new(fs), "/project".into());
    let mut configs = FxHashMap::default();
    for directory in ["/PROJECT", "/project"] {
        assert_eq!(
            find_tsconfig(&format!("{directory}/src/A.ts"), &host, &mut configs),
            Some(format!("{directory}/tsconfig.json")),
        );
    }
}
