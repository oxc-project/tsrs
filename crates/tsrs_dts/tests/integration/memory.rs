// A build frees everything its Programs allocate. The files are bound on the calling thread, outside any region, when
// the bundler asks for diagnostics; their binder data (symbols, symbol tables, flow nodes) goes to the Programs' file
// regions (`ProgramRegions`, fileregions.rs `bind`) and the checkers' diagnostics to their checkers' regions
// (`checker_diagnostic_regions`). Otherwise both stay in the calling thread's never-freed arena: about 3.7 MiB for
// each build of a 40-module project.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use tsrs_dts::{BuildOptions, BuildSession, OutputMode, build};

use crate::filesystem::memory_fs;

fn inputs(version: usize) -> tsrs_dts::Files {
    let config = r#"{"compilerOptions":{"strict":true,"module":"nodenext","moduleResolution":"nodenext","types":[],"declarationMap":true},"include":["src/**/*.ts"]}"#;
    let mut files = BTreeMap::from([
        ("/project/tsconfig.json".to_string(), config.to_string()),
        ("/project/package.json".to_string(), r#"{"type":"module"}"#.to_string()),
    ]);
    let mut index = String::new();
    for i in 0..40 {
        files.insert(
            format!("/project/src/m{i}.ts"),
            format!(
                "/** A value. @deprecated use I{i} */\nexport interface I{i} {{ value: number; next?: I{i} }}\n/** Holds {i}. */\nexport class C{i} {{ private brand = {version}; get(a: number): I{i} {{ return {{ value: a + {i} }}; }} }}\nexport type U{i} = I{i} | C{i};\n"
            ),
        );
        let _ = writeln!(index, "export * from './m{i}.js';");
    }
    files.insert("/project/src/index.ts".into(), index);
    files
}

fn options(reuse_diagnostics: bool) -> BuildOptions {
    BuildOptions {
        config_path: "/project/tsconfig.json".into(),
        entry: "/project/src/index.ts".into(),
        output: OutputMode::Bundle { file_name: "index.d.ts".into() },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: false,
        reuse_diagnostics,
    }
}

/// What the calling thread's never-freed arena grows by over 5 builds, after one that fills process-wide caches.
fn growth(mut build: impl FnMut(usize)) -> usize {
    build(0);
    let before = tsrs_core::arena::thread_arena_used();
    for i in 1..=5 {
        build(i);
    }
    tsrs_core::arena::thread_arena_used() - before
}

#[test]
fn builds_keep_nothing_in_the_thread_arena() {
    let (fs, _) = memory_fs(&inputs(0));
    for reuse in [false, true] {
        let grown = growth(|_| {
            build(&options(reuse), Arc::clone(&fs)).unwrap();
        });
        assert!(grown < 64 << 10, "reuse_diagnostics {reuse}: {grown} bytes kept by 5 builds");
    }
    let mut session = BuildSession::new();
    let grown = growth(|i| {
        session.build(&options(false), memory_fs(&inputs(i)).0).unwrap();
    });
    assert!(grown < 64 << 10, "session: {grown} bytes kept by 5 builds");
}
