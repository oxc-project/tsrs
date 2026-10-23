// program_to_snapshot takes shortcuts when the new program shares files with the old state's program (versions of
// shared files, the deleted-file and global-scope passes, reference sets compared by their shared part once). Each
// test checks that the snapshot is the one the full computation gives.

use std::sync::{Arc, Mutex};

use tsrs_compiler::{
    CompilerHost, Context, Program as CompilerProgram, ProgramOptions, new_compiler_host,
    new_program,
};
use tsrs_core::P;
use tsrs_core::tspath::Path;
use tsrs_tsoptions::{self as tsoptions, ParseConfigHost};
use tsrs_vfs::{FS, vfstest};

use crate::emit::ProgramLike;
use crate::program::Program;
use crate::snapshot::FileEmitKind;

struct ConfigHost {
    fs: Arc<dyn FS>,
}

impl ParseConfigHost for ConfigHost {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

const FILES: [(&str, &str); 8] = [
    (
        "/src/tsconfig.json",
        r#"{ "compilerOptions": { "noLib": true, "types": [], "checkers": 2 } }"#,
    ),
    ("/src/a.ts", "export const a = 1;\n"),
    (
        "/src/b.ts",
        "/// <reference path=\"./g.ts\" />\nimport { a } from './a';\nexport const b = a;\n",
    ),
    // A script: it affects the global scope.
    ("/src/g.ts", "declare var g: number;\n"),
    // Ambient modules: every file references the files declaring them.
    ("/src/m.d.ts", "declare module \"m\" { export const x: number; }\n"),
    ("/src/c.ts", "import { x } from \"m\";\nexport const c = x;\n"),
    ("/src/d.ts", "import { b } from './b';\nexport const d = b;\n"),
    // A module augmentation: the importers of a.ts reference it.
    (
        "/src/aug.ts",
        "import './a';\ndeclare module './a' { interface I { x: number } }\nexport {};\n",
    ),
];

fn with(changes: &[(&'static str, Option<&'static str>)]) -> Vec<(&'static str, &'static str)> {
    let mut files: Vec<(&str, &str)> = FILES.to_vec();
    for &(name, text) in changes {
        files.retain(|&(n, _)| n != name);
        if let Some(text) = text {
            files.push((name, text));
        }
    }
    files
}

fn compiler_host(files: &[(&'static str, &'static str)]) -> Arc<dyn CompilerHost> {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().copied(), true));
    new_compiler_host("/", fs, "", None, None, None)
}

fn compiler_program(files: &[(&'static str, &'static str)]) -> &'static CompilerProgram {
    compiler_program_with(files, compiler_host(files))
}

fn compiler_program_with(
    files: &[(&'static str, &'static str)],
    host: Arc<dyn CompilerHost>,
) -> &'static CompilerProgram {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().copied(), true));
    let config_host: &'static ConfigHost = Box::leak(Box::new(ConfigHost { fs }));
    let (config, diagnostics) = tsoptions::get_parsed_command_line_of_config_file(
        "/src/tsconfig.json",
        None,
        None,
        config_host,
        None,
    );
    assert_eq!(diagnostics, [] as [tsrs_core::P<tsrs_ast::Diagnostic>; 0]);
    new_program(ProgramOptions::new(P::new(config.unwrap()), host))
}

// A host that hands out the file it parsed first for each path, like the watch's source file cache.
struct CachingHost {
    inner: Arc<dyn CompilerHost>,
    cache: Arc<Mutex<rustc_hash::FxHashMap<Path, P<tsrs_ast::SourceFile>>>>,
}

impl CompilerHost for CachingHost {
    fn fs(&self) -> &dyn FS {
        self.inner.fs()
    }
    fn default_library_path(&self) -> &str {
        self.inner.default_library_path()
    }
    fn get_current_directory(&self) -> &str {
        self.inner.get_current_directory()
    }
    fn trace(&self, msg: &'static tsrs_diagnostics::Message, args: &[&dyn std::fmt::Display]) {
        self.inner.trace(msg, args);
    }
    fn get_source_file(
        &self,
        opts: tsrs_ast::SourceFileParseOptions,
    ) -> Option<P<tsrs_ast::SourceFile>> {
        if let Some(&file) = self.cache.lock().unwrap().get(&opts.path) {
            return Some(file);
        }
        let path = opts.path.clone();
        let file = self.inner.get_source_file(opts)?;
        self.cache.lock().unwrap().insert(path, file);
        Some(file)
    }
    fn get_content_mapped_source_files(
        &self,
        parse_options: tsrs_ast::SourceFileParseOptions,
        mapper: &tsrs_tsoptions::Mapper,
    ) -> Result<tsrs_contentmapper::SourceFiles, tsrs_contentmapper::Error> {
        self.inner.get_content_mapped_source_files(parse_options, mapper)
    }
    fn content_mapper_project(&self) -> Option<Arc<dyn tsrs_contentmapper::Project>> {
        self.inner.content_mapper_project()
    }
    fn get_resolved_project_reference(
        &self,
        file_name: &str,
        path: Path,
    ) -> Option<P<tsoptions::ParsedCommandLine>> {
        self.inner.get_resolved_project_reference(file_name, path)
    }
}

fn incremental(program: &'static CompilerProgram, old: Option<P<Program>>) -> P<Program> {
    crate::new_program(program, old, crate::create_host(compiler_host(&FILES)), None, false)
}

// Per file: the version, whether it affects the global scope, and its references, sorted.
fn file_states(program: &Program) -> Vec<(Path, String, bool, Vec<String>)> {
    let snapshot = program.snapshot;
    let mut paths = snapshot.file_infos.keys();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let info = snapshot.file_infos.load(&path).unwrap();
            let mut references: Vec<String> = snapshot
                .referenced_map
                .get_references(&path)
                .map(|refs| refs.iter().map(std::string::ToString::to_string).collect())
                .unwrap_or_default();
            references.sort();
            (path, info.version, info.affects_global_scope, references)
        })
        .collect()
}

fn changed_files(program: &Program) -> Vec<String> {
    let mut changed: Vec<String> = program
        .snapshot
        .changed_files_set
        .keys()
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    changed.sort();
    changed
}

#[test]
fn a_reused_program_has_the_snapshot_of_a_fresh_one() {
    let old_program = compiler_program(&FILES);
    let old = incremental(old_program, None);
    let edited = with(&[("/src/a.ts", Some("export const a = 1;\nconst local = 2;\n"))]);
    let (reused, _, ok) =
        old_program.reuse_program(&Path::from("/src/a.ts"), compiler_host(&edited), None, None);
    assert!(ok);
    let reused = incremental(reused.unwrap(), Some(old));

    // The same files, parsed again: nothing is shared with the old state's program.
    let fresh_program = compiler_program(&edited);
    let fresh = incremental(fresh_program, Some(old));
    let from_scratch = incremental(fresh_program, None);

    assert_eq!(file_states(&reused), file_states(&from_scratch));
    assert_eq!(file_states(&fresh), file_states(&from_scratch));
    assert_eq!(changed_files(&reused), ["/src/a.ts"]);
    assert_eq!(changed_files(&fresh), ["/src/a.ts"]);
    let b = file_states(&reused).into_iter().find(|(p, ..)| p.as_str() == "/src/b.ts").unwrap();
    assert_eq!(b.3, ["/src/a.ts", "/src/aug.ts", "/src/g.ts", "/src/m.d.ts"]);
}

// A reused program keeps the references of the files it did not reparse (programtosnapshot.rs `keeps_references`):
// they are the ones a fresh snapshot computes when the reparsed file declares module augmentations or ambient modules.
#[test]
fn a_reused_program_keeps_the_references_of_the_other_files() {
    for (name, text) in [
        (
            "/src/aug.ts",
            "import './a';\ndeclare module './a' { interface I { y: string } }\nexport {};\n",
        ),
        (
            "/src/m.d.ts",
            "declare module \"m\" { export const x: string; export const z: number; }\n",
        ),
        ("/src/g.ts", "declare var g: string;\n"),
    ] {
        let old_program = compiler_program(&FILES);
        let old = incremental(old_program, None);
        let edited = with(&[(name, Some(text))]);
        let (reused, _, ok) =
            old_program.reuse_program(&Path::from(name), compiler_host(&edited), None, None);
        assert!(ok, "{name}");
        let reused = incremental(reused.unwrap(), Some(old));
        assert_eq!(
            file_states(&reused),
            file_states(&incremental(compiler_program(&edited), None)),
            "{name}"
        );
        assert_eq!(changed_files(&reused), [name]);
    }
}

#[test]
fn deleting_a_global_file_changes_every_file() {
    let old = incremental(compiler_program(&FILES), None);
    let new = incremental(compiler_program(&with(&[("/src/g.ts", None)])), Some(old));
    assert_eq!(
        changed_files(&new),
        ["/src/a.ts", "/src/aug.ts", "/src/b.ts", "/src/c.ts", "/src/d.ts", "/src/m.d.ts"]
    );
}

#[test]
fn deleting_a_module_changes_its_importers_only() {
    let old = incremental(compiler_program(&FILES), None);
    let new = incremental(compiler_program(&with(&[("/src/d.ts", None)])), Some(old));
    assert_eq!(changed_files(&new), [] as [std::string::String; 0]);
    assert!(new.snapshot.build_info_emit_pending.get());
}

#[test]
fn a_file_that_stops_affecting_the_global_scope_changes_every_file() {
    let old = incremental(compiler_program(&FILES), None);
    let new = incremental(
        compiler_program(&with(&[("/src/g.ts", Some("export {};\ndeclare var g: number;\n"))])),
        Some(old),
    );
    assert_eq!(
        changed_files(&new),
        [
            "/src/a.ts",
            "/src/aug.ts",
            "/src/b.ts",
            "/src/c.ts",
            "/src/d.ts",
            "/src/g.ts",
            "/src/m.d.ts"
        ]
    );
}

#[test]
fn a_new_ambient_module_changes_the_shared_references() {
    let old_program = compiler_program(&FILES);
    let old = incremental(old_program, None);
    let new_program = compiler_program(&with(&[(
        "/src/n.d.ts",
        Some("declare module \"n\" { export const y: number; }\n"),
    )]));
    let new = incremental(new_program, Some(old));
    assert_eq!(file_states(&new), file_states(&incremental(new_program, None)));
    let a = file_states(&new).into_iter().find(|(p, ..)| p.as_str() == "/src/a.ts").unwrap();
    assert_eq!(a.3, ["/src/m.d.ts", "/src/n.d.ts"]);
}

// A program built in full keeps no references, even for the files it shares with the old state's program: the
// module resolution may differ (here a file that was missing appears).
#[test]
fn a_full_rebuild_that_shares_files_looks_up_their_references_again() {
    let cache = Arc::new(Mutex::new(rustc_hash::FxHashMap::default()));
    let before = with(&[("/src/e.ts", Some("import { x } from './x';\nexport const e = x;\n"))]);
    let after = with(&[
        ("/src/e.ts", Some("import { x } from './x';\nexport const e = x;\n")),
        ("/src/x.ts", Some("export const x = 1;\n")),
    ]);
    let old_program = compiler_program_with(
        &before,
        Arc::new(CachingHost { inner: compiler_host(&before), cache: Arc::clone(&cache) }),
    );
    let old = incremental(old_program, None);
    let new_program = compiler_program_with(
        &after,
        Arc::new(CachingHost { inner: compiler_host(&after), cache }),
    );
    let e = Path::from("/src/e.ts");
    assert_eq!(
        old_program.get_source_file_by_path(&e),
        new_program.get_source_file_by_path(&e),
        "e.ts is shared"
    );
    let new = incremental(new_program, Some(old));
    assert_eq!(file_states(&new), file_states(&incremental(new_program, None)));
    let e = file_states(&new).into_iter().find(|(p, ..)| p.as_str() == "/src/e.ts").unwrap();
    assert_eq!(e.3, ["/src/m.d.ts", "/src/x.ts"]);
}

#[test]
fn mixed_deletions_visit_a_global_file_after_a_non_global_file() {
    let old = incremental(compiler_program(&FILES), None);
    let deleted = [Path::from("/src/d.ts"), Path::from("/src/g.ts")];
    let ordered: Vec<Path> =
        old.snapshot.file_infos.keys().into_iter().filter(|path| deleted.contains(path)).collect();
    assert_eq!(ordered.len(), 2);
    // Make the last deleted entry global so stopping at the first deletion misses it.
    for (index, path) in ordered.iter().enumerate() {
        let mut info = old.snapshot.file_infos.load(path).unwrap();
        info.affects_global_scope = index == 1;
        old.snapshot.file_infos.store(path.clone(), info);
        old.snapshot.changed_files_set.add(path.clone());
        old.snapshot.affected_files_pending_emit.store(path.clone(), FileEmitKind::Js);
    }
    old.snapshot.build_info_emit_pending.set(false);

    let remaining = with(&[("/src/d.ts", None), ("/src/g.ts", None)]);
    let current = incremental(compiler_program(&remaining), Some(old));
    assert_eq!(
        changed_files(&current),
        ["/src/a.ts", "/src/aug.ts", "/src/b.ts", "/src/c.ts", "/src/m.d.ts"]
    );
    for path in &deleted {
        assert!(!current.snapshot.changed_files_set.has(path), "{path} remained changed");
        assert!(
            current.snapshot.affected_files_pending_emit.load(path).is_none(),
            "{path} remained pending emit"
        );
    }
    assert!(current.snapshot.build_info_emit_pending.get());
}

#[test]
fn deleted_pending_paths_without_file_info_are_not_reused() {
    let old = incremental(compiler_program(&FILES), None);
    let missing = Path::from("/src/missing.ts");
    assert!(old.snapshot.file_infos.load(&missing).is_none());
    old.snapshot.changed_files_set.add(missing.clone());
    old.snapshot.affected_files_pending_emit.store(missing.clone(), FileEmitKind::Js);
    old.snapshot.build_info_emit_pending.set(false);

    let current = incremental(compiler_program(&FILES), Some(old));
    assert!(!current.snapshot.changed_files_set.has(&missing));
    assert!(current.snapshot.affected_files_pending_emit.load(&missing).is_none());
    assert!(current.snapshot.build_info_emit_pending.get());
}

const DELETION_TEST_LIB: &str = "interface Array<T> {}\ninterface Boolean {}\ninterface CallableFunction {}\ninterface Function {}\ninterface IArguments {}\ninterface NewableFunction {}\ninterface Number {}\ninterface Object {}\ninterface RegExp {}\ninterface String {}\ninterface LibraryConsumer { property: RemovedGlobal; }\n";
const DELETION_TEST_GLOBAL: &str = "interface RemovedGlobal {}\n";
const DELETION_TEST_CONFIG: &str = r#"{ "compilerOptions": { "target": "es5", "incremental": true, "noEmit": true, "types": [], "tsBuildInfoFile": "/src/state.tsbuildinfo" } }"#;
const DELETION_TEST_SKIP_LIB_CONFIG: &str = r#"{ "compilerOptions": { "target": "es5", "incremental": true, "noEmit": true, "types": [], "tsBuildInfoFile": "/src/state.tsbuildinfo", "skipLibCheck": true } }"#;
const DELETION_TEST_SKIP_DEFAULT_LIB_CONFIG: &str = r#"{ "compilerOptions": { "target": "es5", "incremental": true, "noEmit": true, "types": [], "tsBuildInfoFile": "/src/state.tsbuildinfo", "skipDefaultLibCheck": true } }"#;

fn deletion_test_files(config: &'static str, global: bool) -> Vec<(&'static str, &'static str)> {
    let mut files = vec![
        ("/src/tsconfig.json", config),
        ("/lib/lib.d.ts", DELETION_TEST_LIB),
        ("/src/consumer.ts", "export const unchanged = 1;\n"),
    ];
    if global {
        files.push(("/src/global.d.ts", DELETION_TEST_GLOBAL));
    }
    files
}

fn deletion_test_program(files: &[(&'static str, &'static str)]) -> &'static CompilerProgram {
    let fs: Arc<dyn FS> = Arc::new(vfstest::from_map(files.iter().copied(), true));
    compiler_program_with(files, new_compiler_host("/", fs, "/lib", None, None, None))
}

#[test]
fn global_deletion_invalidates_live_default_library_diagnostics() {
    let before = deletion_test_files(DELETION_TEST_CONFIG, true);
    let old = incremental(deletion_test_program(&before), None);
    let ctx = Context::default();
    assert_eq!(old.get().get_semantic_diagnostics(&ctx, None).len(), 0);
    let library = Path::from("/lib/lib.d.ts");
    assert!(
        old.snapshot.semantic_diagnostics_per_file.load(&library).is_some(),
        "default={} files={:?}",
        tsoptions::get_default_lib_file_name(&old.program.unwrap().options()),
        old.program.unwrap().get_source_files().iter().map(|f| f.file_name()).collect::<Vec<_>>()
    );

    let after = deletion_test_files(DELETION_TEST_CONFIG, false);
    let current = incremental(deletion_test_program(&after), Some(old));
    assert!(current.snapshot.semantic_diagnostics_per_file.load(&library).is_none());
    assert!(!current.snapshot.changed_files_set.has(&library));
    let diagnostics = current.get().get_semantic_diagnostics(&ctx, None);
    assert_eq!(diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>(), [2304]);
    assert_eq!(diagnostics[0].file().unwrap().path(), &library);
}

#[test]
fn global_deletion_preserves_skipped_default_library_diagnostics() {
    for config in [DELETION_TEST_SKIP_LIB_CONFIG, DELETION_TEST_SKIP_DEFAULT_LIB_CONFIG] {
        let before = deletion_test_files(config, true);
        let old = incremental(deletion_test_program(&before), None);
        let ctx = Context::default();
        assert_eq!(old.get().get_semantic_diagnostics(&ctx, None).len(), 0);
        let library = Path::from("/lib/lib.d.ts");
        let old_cache = old.snapshot.semantic_diagnostics_per_file.load(&library).unwrap();

        let after = deletion_test_files(config, false);
        let current = incremental(deletion_test_program(&after), Some(old));
        let new_cache = current.snapshot.semantic_diagnostics_per_file.load(&library).unwrap();
        assert!(Arc::ptr_eq(&old_cache, &new_cache), "{config}");
        assert!(!current.snapshot.changed_files_set.has(&library), "{config}");
        assert_eq!(current.get().get_semantic_diagnostics(&ctx, None).len(), 0, "{config}");
    }
}
