use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;
use tsrs_core::P;
use tsrs_lint::engine::{ConfigHost, find_tsconfig};
use tsrs_vfs::{FS, vfstest, wrapvfs};

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
fn project_discovery_reads_each_extended_config_once() {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let mut files = vec![
        (
            "/project/configs/root.json".to_owned(),
            r#"{"compilerOptions":{"strict":true}}"#.to_owned(),
        ),
        (
            "/project/configs/base.json".to_owned(),
            r#"{"extends":"./root.json","compilerOptions":{"noImplicitReturns":true}}"#.to_owned(),
        ),
    ];
    for project in ["a", "b", "c"] {
        files.push((
            format!("/project/{project}/tsconfig.json"),
            r#"{"extends":"../configs/base.json","include":["*.ts"]}"#.to_owned(),
        ));
        files.push((format!("/project/{project}/index.ts"), "export {};".to_owned()));
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
    for project in ["a", "b", "c"] {
        let expected = format!("/project/{project}/tsconfig.json");
        assert_eq!(
            find_tsconfig(&format!("/project/{project}/index.ts"), &host, &mut configs),
            Some(expected.clone()),
        );
        let options = configs[&tsrs_core::tspath::to_path(&expected, "/project", true)]
            .as_ref()
            .unwrap()
            .compiler_options()
            .unwrap();
        assert!(options.strict.is_true());
        assert!(options.no_implicit_returns.is_true());
    }
    let reads = reads.lock().unwrap();
    for path in ["/project/configs/root.json", "/project/configs/base.json"] {
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

#[test]
fn project_discovery_traverses_non_matching_composite_references() {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let fs = vfstest::from_map(
        [
            (
                "/project/tsconfig.json",
                r#"{"compilerOptions":{"composite":true,"strict":true,"strictNullChecks":false},"files":[],"references":[{"path":"./app/tsconfig.app.json"},{"path":"./tool/tsconfig.tool.json"}]}"#,
            ),
            (
                "/project/app/tsconfig.app.json",
                r#"{"extends":"../tsconfig.json","include":["*.ts"],"references":[{"path":"../lib/tsconfig.lib.json"}]}"#,
            ),
            (
                "/project/tool/tsconfig.tool.json",
                r#"{"extends":"../tsconfig.json","include":["*.ts"],"references":[{"path":"../lib/tsconfig.lib.json"}]}"#,
            ),
            (
                "/project/lib/tsconfig.lib.json",
                r#"{"extends":"../tsconfig.json","include":["*.ts"],"exclude":["excluded.ts"],"references":[{"path":"../core/tsconfig.core.json"},{"path":"./tsconfig.lib.json"}]}"#,
            ),
            (
                "/project/core/tsconfig.core.json",
                r#"{"extends":"../tsconfig.json","include":["*.ts"],"references":[{"path":"../app/tsconfig.app.json"}]}"#,
            ),
            ("/project/app/index.ts", "export {};"),
            ("/project/tool/index.ts", "export {};"),
            ("/project/lib/index.ts", "export {};"),
            ("/project/core/index.ts", "export {};"),
            ("/project/lib/excluded.ts", "export {};"),
            ("/project/loose.ts", "export {};"),
        ],
        true,
    );
    let host = ConfigHost::new(Arc::new(fs), "/project".into());
    let mut configs = FxHashMap::default();
    for _ in 0..2 {
        for project in ["core", "lib", "tool", "app"] {
            let expected = format!("/project/{project}/tsconfig.{project}.json");
            assert_eq!(
                find_tsconfig(&format!("/project/{project}/index.ts"), &host, &mut configs),
                Some(expected.clone()),
            );
            let config =
                configs[&tsrs_core::tspath::to_path(&expected, "/project", true)].as_ref().unwrap();
            assert!(config.compiler_options().unwrap().strict.is_true());
            assert!(config.compiler_options().unwrap().strict_null_checks.is_false());
        }
        for file in ["loose.ts", "lib/excluded.ts"] {
            assert_eq!(find_tsconfig(&format!("/project/{file}"), &host, &mut configs), None);
        }
    }
    assert_eq!(configs.len(), 5);
}

#[test]
fn project_discovery_matches_extension_shadowed_files_without_capturing_excluded_files() {
    let region = tsrs_core::arena::Region::new(1 << 20);
    let _scope = region.enter();
    let fs = vfstest::from_map(
        [
            ("/project/tsconfig.json", r#"{"include":["src"],"exclude":["src/excluded.tsx"]}"#),
            ("/project/src/widget.ts", "export const a = 1;"),
            ("/project/src/widget.tsx", "export const b = 2;"),
            ("/project/src/excluded.ts", "export const c = 3;"),
            ("/project/src/excluded.tsx", "export const d = 4;"),
            ("/project/src/script.js", "export const e = 5;"),
        ],
        true,
    );
    let host = ConfigHost::new(Arc::new(fs), "/project".into());
    let mut configs = FxHashMap::default();
    assert_eq!(
        find_tsconfig("/project/src/widget.tsx", &host, &mut configs),
        Some("/project/tsconfig.json".into()),
    );
    assert_eq!(find_tsconfig("/project/src/excluded.tsx", &host, &mut configs), None);
    assert_eq!(find_tsconfig("/project/src/script.js", &host, &mut configs), None);
}
