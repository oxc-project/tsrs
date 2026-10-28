use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tsrs_dts::{BuildOptions, BuildResult, OutputMode, build};
use tsrs_vfs::{FS, vfstest};

use crate::filesystem;
use crate::program;
use filesystem::{NoWrites, memory_fs};

const CONFIG: &str = r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"outDir":"/must-not-write","declarationDir":"/must-not-write-types"},"include":["src/**/*.ts"]}"#;

fn filesystem(inputs: &[(&str, &str)]) -> (Arc<dyn FS>, Arc<AtomicUsize>) {
    filesystem_with_config(inputs, CONFIG)
}

fn filesystem_with_config(
    inputs: &[(&str, &str)],
    config: &str,
) -> (Arc<dyn FS>, Arc<AtomicUsize>) {
    let files = std::iter::once(("/project/tsconfig.json", config))
        .chain(inputs.iter().copied())
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    memory_fs(&files)
}

fn options() -> BuildOptions {
    BuildOptions {
        config_path: "/project/tsconfig.json".into(),
        entry: "/project/src/index.ts".into(),
        output: OutputMode::Bundle { file_name: "index.d.ts".into() },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: false,
        reuse_diagnostics: false,
    }
}

fn bundle(inputs: &[(&str, &str)]) -> BuildResult {
    let (fs, writes) = filesystem(inputs);
    let result = build(&options(), fs).unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    result
}

fn code(result: &BuildResult) -> &str {
    &result.files["index.d.ts"]
}

#[test]
fn source_errors_prevent_output_and_writes() {
    let (fs, writes) = filesystem(&[("/project/src/index.ts", "export const wrong: string = 1;")]);
    let result = build(&options(), fs);
    assert!(result.unwrap_err().to_string().contains("TS2322"));
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn independent_builds_have_deterministic_output() {
    let input = [("/project/src/index.ts", "export interface API { value: number }")];
    assert_eq!(bundle(&input).files, bundle(&input).files);
}

#[test]
fn compiler_name_reservations_preserve_nested_bindings_across_modules() {
    let inputs = [
        (
            "/project/src/index.ts",
            "export type { Item as First, Box } from './a.js'; export type { Item as Second } from './b.js';",
        ),
        (
            "/project/src/a.ts",
            "export interface Item { tag: 'Item_2' } export type Box<Item_1> = { item: Item; extra: Item_1 };",
        ),
        ("/project/src/b.ts", "export interface Item { value: number }"),
    ];
    let result = bundle(&inputs);
    assert_eq!(result.files, bundle(&inputs).files);
    assert!(code(&result).contains("\"Item_2\"") || code(&result).contains("'Item_2'"));
    check_consumer(
        &result.files,
        "import type { First, Second, Box } from './out/index.js'; const first: First = { tag: 'Item_2' }; const second: Second = { value: 1 }; const box: Box<number> = { item: first, extra: second.value };",
    );
}

fn check_consumer(outputs: &tsrs_dts::Files, consumer: &str) {
    check_consumer_with_files(outputs, consumer, &[]);
}

fn check_consumer_with_files(outputs: &tsrs_dts::Files, consumer: &str, extra: &[(&str, &str)]) {
    let mut files: Vec<(String, String)> =
        outputs.iter().map(|(name, text)| (format!("/check/out/{name}"), text.clone())).collect();
    files.extend(extra.iter().map(|(path, text)| (path.to_string(), text.to_string())));
    files.push(("/check/consumer.ts".into(), consumer.into()));
    if !files.iter().any(|(path, _)| path == "/check/package.json") {
        files.push(("/check/package.json".into(), r#"{"type":"module"}"#.into()));
    }
    files.push(("/check/tsconfig.json".into(), r#"{"compilerOptions":{"strict":true,"noEmit":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[]},"files":["consumer.ts"]}"#.into()));
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        files.iter().map(|(p, t)| (p.as_str(), t.as_str())),
        true,
    )));
    let host = program::HostProgram::new(fs, "/check/tsconfig.json");
    let messages = host.diagnostics();
    assert!(messages.is_empty(), "{}\nOutputs: {outputs:#?}", messages.join("\n"));
}

#[test]
fn link_cache_invalidates_source_map_positions_and_external_declarations() {
    let opts = options();
    let mut session = tsrs_dts::BuildSession::new();
    let config = CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true");
    let mut maps = Vec::new();
    for source in
        ["export interface API { value: string }", "\nexport interface API { value: string }"]
    {
        let (fs, _) = filesystem_with_config(&[("/project/src/index.ts", source)], &config);
        maps.push(session.build(&opts, fs).unwrap().files["index.d.ts.map"].clone());
    }
    assert_ne!(maps[0], maps[1]);
    assert_eq!(session.link_cache_hits(), 0);
    for declaration in
        ["export interface Item { value: string }", "export interface Item { value: number }"]
    {
        let (fs, _) = filesystem(&[
            ("/project/src/index.ts", "export type API = import('external').Item;"),
            (
                "/project/node_modules/external/package.json",
                r#"{"name":"external","types":"index.d.ts"}"#,
            ),
            ("/project/node_modules/external/index.d.ts", declaration),
        ]);
        session.build(&opts, fs).unwrap();
    }
    assert_eq!(session.link_cache_hits(), 0);
}

#[test]
fn implementation_changes_reuse_linking_but_public_types_invalidate_it() {
    let mut session = tsrs_dts::BuildSession::new();
    let opts = options();
    let mut outputs = Vec::new();
    for source in [
        "export function value(): number { return 1; }",
        "export function value(): number { return 2; }",
        "export function value(): string { return 'changed'; }",
    ] {
        let (fs, _) = filesystem(&[("/project/src/index.ts", source)]);
        outputs.push(session.build(&opts, fs).unwrap());
    }
    assert_eq!(session.cache_hits(), 0);
    assert_eq!(session.link_cache_hits(), 1);
    assert_eq!(outputs[0].files, outputs[1].files);
    assert!(code(&outputs[2]).contains("string"));
    let (fs, _) = filesystem(&[(
        "/project/src/index.ts",
        "export function value(): number { return 'invalid'; }",
    )]);
    assert!(session.build(&opts, fs).is_err());
    let (fs, _) = filesystem(&[(
        "/project/src/index.ts",
        "export function value(): string { return 'changed'; }",
    )]);
    session.build(&opts, fs).unwrap();
    assert_eq!(session.link_cache_hits(), 1);
}

#[test]
fn case_insensitive_filesystems_preserve_compiler_casing_diagnostics() {
    let inner: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        [
            ("/project/tsconfig.json", CONFIG),
            ("/project/src/index.ts", "export type { Item } from './dep.js';"),
            ("/project/src/Dep.ts", "export interface Item { value: string }"),
        ],
        false,
    )));
    let writes = Arc::new(AtomicUsize::new(0));
    let fs: Arc<dyn FS> = Arc::new(NoWrites { base: inner, writes: Arc::clone(&writes) });
    let error = build(&options(), fs).unwrap_err();
    assert!(error.0.contains("TS1149"), "{error}");
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn symlinked_external_packages_keep_public_specifiers() {
    let files: Vec<(&str, vfstest::MapFile)> = vec![
        ("/project/tsconfig.json", CONFIG.into()),
        (
            "/project/src/index.ts",
            "import type { Item } from 'external'; export interface API { item: Item }".into(),
        ),
        ("/project/node_modules/external", vfstest::symlink("/store/external")),
        ("/store/external/package.json", r#"{"name":"external","types":"index.d.ts"}"#.into()),
        ("/store/external/index.d.ts", "export interface Item { value: string }".into()),
    ];
    let inner: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(files, true)));
    let writes = Arc::new(AtomicUsize::new(0));
    let fs: Arc<dyn FS> = Arc::new(NoWrites { base: inner, writes: Arc::clone(&writes) });
    let mut session = tsrs_dts::BuildSession::new();
    let opts = options();
    let first = session.build(&opts, Arc::clone(&fs)).unwrap();
    assert!(Arc::ptr_eq(&first, &session.build(&opts, fs).unwrap()));
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert!(!code(&first).contains("/store/"));
    check_consumer_with_files(
        &first.files,
        "import type { API } from './out/index.js'; const api: API = { item: { value: 'ok' } };",
        &[
            (
                "/check/node_modules/external/package.json",
                r#"{"name":"external","types":"index.d.ts"}"#,
            ),
            ("/check/node_modules/external/index.d.ts", "export interface Item { value: string }"),
        ],
    );
}

#[test]
fn build_session_detects_new_resolution_candidates_and_glob_inputs() {
    let mut session = tsrs_dts::BuildSession::new();
    let opts = options();
    let root = ("/project/src/index.ts", "export type { Item } from './dep.js';");
    let declaration = ("/project/src/dep.d.ts", "export interface Item { fallback: true }");
    let (fs, _) = filesystem(&[root, declaration]);
    let first = session.build(&opts, fs).unwrap();
    assert!(code(&first).contains("fallback: true"));
    let (fs, _) = filesystem(&[
        root,
        declaration,
        ("/project/src/dep.ts", "export interface Item { preferred: true }"),
        ("/project/src/global.d.ts", "interface AddedGlobal { added: true }"),
    ]);
    let second = session.build(&opts, fs).unwrap();
    assert!(code(&second).contains("preferred: true"));
    assert!(!code(&second).contains("fallback: true"));
    assert!(code(&second).contains("AddedGlobal"));
    assert_eq!(session.cache_hits(), 0);
}

#[test]
fn build_session_reuses_results_and_invalidates_sources_configs_and_entries() {
    let mut session = tsrs_dts::BuildSession::new();
    let opts = options();
    let inputs = [("/project/src/index.ts", "export interface API { value: string }")];
    let (fs, _) = filesystem(&inputs);
    let first = session.build(&opts, Arc::clone(&fs)).unwrap();
    let second = session.build(&opts, fs).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(session.cache_hits(), 1);
    let (fs, _) =
        filesystem(&[("/project/src/index.ts", "export interface API { value: number }")]);
    let changed = session.build(&opts, fs).unwrap();
    assert!(!Arc::ptr_eq(&first, &changed));
    assert!(code(&changed).contains("value: number"));
    let (fs, _) = filesystem_with_config(
        &inputs,
        &CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true"),
    );
    let mapped = session.build(&opts, fs).unwrap();
    assert!(mapped.files.contains_key("index.d.ts.map"));
    let (fs, _) = filesystem(&inputs);
    let entries = std::collections::BTreeMap::from([("public.d.ts".into(), opts.entry.clone())]);
    let multi = session.build_entries(&opts, &entries, Arc::clone(&fs)).unwrap();
    assert!(multi.files.contains_key("public.d.ts"));
    let again = session.build_entries(&opts, &entries, Arc::clone(&fs)).unwrap();
    assert!(Arc::ptr_eq(&multi, &again));
    session.clear();
    assert!(!Arc::ptr_eq(&multi, &session.build_entries(&opts, &entries, fs).unwrap()));
}

#[test]
fn build_session_invalidates_package_resolution_and_failed_candidates() {
    let mut session = tsrs_dts::BuildSession::new();
    let opts = options();
    let source = "export type API = import('external').Item;";
    let mut previous = None;
    for types in ["a.d.ts", "b.d.ts"] {
        let package = format!(r#"{{"name":"external","types":"{types}"}}"#);
        let (fs, _) = filesystem(&[
            ("/project/src/index.ts", source),
            ("/project/node_modules/external/package.json", &package),
            ("/project/node_modules/external/a.d.ts", "export interface Item { a: string }"),
            ("/project/node_modules/external/b.d.ts", "export interface Item { b: number }"),
        ]);
        let result = session.build(&opts, fs).unwrap();
        assert!(code(&result).contains("external"));
        if let Some(previous) = previous.replace(Arc::clone(&result)) {
            assert!(!Arc::ptr_eq(&previous, &result));
        }
    }
    assert_eq!(session.cache_hits(), 0);
    let (fs, _) =
        filesystem(&[("/project/src/index.ts", "export type { Item } from './missing.js';")]);
    assert!(session.build(&opts, fs).is_err());
    let (fs, _) = filesystem(&[
        ("/project/src/index.ts", "export type { Item } from './missing.js';"),
        ("/project/src/missing.ts", "export interface Item { resolved: true }"),
    ]);
    assert!(code(&session.build(&opts, fs).unwrap()).contains("resolved: true"));
}

#[test]
fn project_reference_cycles_are_reported_without_writes() {
    let root = r#"{"compilerOptions":{"composite":true,"types":[]},"references":[{"path":"./dep"}],"files":["src/index.ts"]}"#;
    let dep = r#"{"compilerOptions":{"composite":true,"types":[]},"references":[{"path":".."}],"files":["index.ts"]}"#;
    let (fs, writes) = filesystem_with_config(
        &[
            ("/project/src/index.ts", "export interface API {}"),
            ("/project/dep/tsconfig.json", dep),
            ("/project/dep/index.ts", "export interface Dep {}"),
        ],
        root,
    );
    let error = build(&options(), fs).unwrap_err();
    assert!(error.0.contains("TS6202"), "{error}");
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn project_reference_errors_and_transitive_dependencies_are_checked() {
    let root = r#"{"compilerOptions":{"strict":true,"module":"nodenext","moduleResolution":"nodenext","types":[]},"references":[{"path":"./middle"}],"include":["src/**/*.ts"]}"#;
    let project = r#"{"compilerOptions":{"strict":true,"composite":true,"rootDir":"src","outDir":"dist","module":"nodenext","moduleResolution":"nodenext","types":[]},"include":["src/**/*.ts"]}"#;
    let middle =
        project.replace("\"include\"", "\"references\":[{\"path\":\"../leaf\"}],\"include\"");
    for source in ["export interface Item { value: string }", "export const value: string = 1;"] {
        let (fs, writes) = filesystem_with_config(
            &[
                ("/project/src/index.ts", "export type { Item } from '../middle/src/index.js';"),
                ("/project/middle/tsconfig.json", &middle),
                (
                    "/project/middle/src/index.ts",
                    "export type { Item } from '../../leaf/src/index.js';",
                ),
                ("/project/leaf/tsconfig.json", project),
                ("/project/leaf/src/index.ts", source),
            ],
            root,
        );
        let result = build(&options(), fs);
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        if source.contains("interface") {
            let result = result.unwrap();
            check_consumer(
                &result.files,
                "import type { Item } from './out/index.js'; const item: Item = { value: 'ok' };",
            );
        } else {
            assert!(result.unwrap_err().0.contains("TS2322"));
        }
    }
}

#[test]
fn project_references_compile_dependencies_without_writes() {
    let inputs = [
        (
            "/project/dep/tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"composite":true,"rootDir":"src","outDir":"dist","module":"nodenext","moduleResolution":"nodenext","target":"es2022","types":[]},"include":["src/**/*.ts"]}"#,
        ),
        (
            "/project/dep/src/index.ts",
            "export interface Item { value: string } export class Client { private brand!: void; value = 1 }",
        ),
        (
            "/project/src/index.ts",
            "export { Client } from '../dep/src/index.js'; import type { Item } from '../dep/src/index.js'; export interface API { item: Item }",
        ),
    ];
    let config = r#"{"compilerOptions":{"strict":true,"module":"nodenext","moduleResolution":"nodenext","target":"es2022","types":[]},"references":[{"path":"./dep"}],"include":["src/**/*.ts"]}"#;
    let (fs, writes) = filesystem_with_config(&inputs, config);
    let result = build(&options(), fs).unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    check_consumer(
        &result.files,
        "import { Client, type API } from './out/index.js'; const api: API = { item: { value: 'ok' } }; const client = new Client();",
    );
}

#[test]
fn malformed_declaration_maps_are_rejected() {
    for mappings in ["!", "ACAA", "CAAA,DAAA"] {
        let map = format!(
            r#"{{"version":3,"file":"index.d.ts","sources":["api.ts"],"mappings":"{mappings}"}}"#
        );
        let (fs, _) = filesystem_with_config(
            &[
                ("/project/src/index.d.ts", "export interface API { value: string }"),
                ("/project/src/index.d.ts.map", &map),
            ],
            &CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true"),
        );
        let mut opts = options();
        opts.entry = "/project/src/index.d.ts".into();
        let error = build(&opts, fs).unwrap_err();
        assert!(error.0.contains("invalid declaration map"), "{error}");
    }
}

#[test]
fn existing_declaration_maps_compose_without_original_source_files() {
    let (fs, _) = filesystem_with_config(
        &[
            (
                "/project/src/index.d.ts",
                "export interface API { value: string }\n//# sourceMappingURL=index.d.ts.map\n",
            ),
            (
                "/project/src/index.d.ts.map",
                r#"{"version":3,"file":"index.d.ts","sourceRoot":"../original","sources":["api.ts"],"sourcesContent":["export interface API { value: string }"],"names":[],"mappings":"AAKA"}"#,
            ),
        ],
        &CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true"),
    );
    let mut opts = options();
    opts.entry = "/project/src/index.d.ts".into();
    let result = build(&opts, fs).unwrap();
    let map = tsrs_sourcemap::RawSourceMap::from_json(
        &tsrs_core::json::unmarshal(&result.files["index.d.ts.map"]).unwrap(),
    )
    .unwrap();
    assert!(map.sources.iter().any(|source| source.ends_with("original/api.ts")), "{map:?}");
    assert!(
        tsrs_sourcemap::decode_mappings(&map.mappings)
            .values()
            .any(|mapping| mapping.source_line == Some(5))
    );
}

#[test]
fn map_review_multiple_entries_ambient_offsets_and_unicode_columns() {
    let inputs = [
        ("/project/src/index.ts", "export interface A { name: string }"),
        ("/project/src/other.ts", "export interface B { name: number }"),
        (
            "/project/src/globals.d.ts",
            "declare module 'virtual' { interface Item { value: string } } interface Global { '😀': string; café: number }",
        ),
    ];
    let config = CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true");
    let (fs, _) = filesystem_with_config(&inputs, &config);
    let entries = std::collections::BTreeMap::from([
        ("index.d.ts".into(), "/project/src/index.ts".into()),
        ("other.d.ts".into(), "/project/src/other.ts".into()),
    ]);
    let result = tsrs_dts::build_entries(&options(), &entries, fs).unwrap();
    for name in ["index.d.ts", "other.d.ts", "__shared.d.cts", "__shared.d.cts.ambient.d.ts"] {
        assert!(result.files.contains_key(&format!("{name}.map")));
        assert!(result.files[name].contains(&format!("sourceMappingURL={name}.map")));
    }
    let map = tsrs_sourcemap::RawSourceMap::from_json(
        &tsrs_core::json::unmarshal(&result.files["__shared.d.cts.map"]).unwrap(),
    )
    .unwrap();
    let mappings: Vec<_> = tsrs_sourcemap::decode_mappings(&map.mappings).values().collect();
    for (marker, source) in
        [("interface A", "index.ts"), ("interface B", "other.ts"), ("café:", "globals.d.ts")]
    {
        let line =
            result.files["__shared.d.cts"].lines().position(|line| line.contains(marker)).unwrap()
                as u32;
        assert!(
            mappings.iter().any(|mapping| mapping.generated_line == line
                && mapping
                    .source_index
                    .get()
                    .is_some_and(|index| map.sources[index as usize].ends_with(source))),
            "{marker}: {mappings:?}"
        );
    }
    let original = inputs[2].1;
    let original_column = original[..original.find("café").unwrap()].encode_utf16().count() as u32;
    let (line, text) = result.files["__shared.d.cts"]
        .lines()
        .enumerate()
        .find(|(_, text)| text.contains("café:"))
        .unwrap();
    let column = text[..text.find("café").unwrap()].encode_utf16().count() as u32;
    assert!(
        mappings.iter().any(|mapping| mapping.generated_line == line as u32
            && mapping.generated_character == column
            && mapping.source_line == Some(0)
            && mapping.source_character == Some(original_column)),
        "{mappings:?}"
    );
    check_consumer(
        &result.files,
        "import type { A } from './out/index.js'; import type { B } from './out/other.js'; const a: A = { name: 'ok' }; const b: B = { name: 1 }; const global: Global = { '😀': 'ok', café: 1 };",
    );
}

#[test]
fn declaration_maps_compose_bundle_positions_to_original_sources() {
    let inputs = [
        (
            "/project/src/index.ts",
            "/// <reference lib=\"esnext.disposable\" preserve=\"true\" />\nexport type { API } from './dep.js';",
        ),
        (
            "/project/src/dep.ts",
            "// shifted source line\nexport interface Dead { removed: true }\nexport interface API {\n  café: string;\n  value: number;\n}\n",
        ),
    ];
    let config = CONFIG.replace("\"strict\":true", "\"strict\":true,\"declarationMap\":true");
    let (fs, writes) = filesystem_with_config(&inputs, &config);
    let result = build(&options(), fs).unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    let map = tsrs_sourcemap::RawSourceMap::from_json(
        &tsrs_core::json::unmarshal(&result.files["index.d.ts.map"]).unwrap(),
    )
    .unwrap();
    let mappings: Vec<_> = tsrs_sourcemap::decode_mappings(&map.mappings).values().collect();
    assert_eq!(map.file, "index.d.ts");
    assert_eq!(map.source_root, "file:///");
    assert!(map.sources.iter().any(|source| source.ends_with("dep.ts")), "{map:?}");
    assert!(!map.sources.iter().any(|source| source.ends_with("dep.d.ts")));
    assert!(code(&result).contains("/// <reference lib=\"esnext.disposable\""));
    let generated_line =
        code(&result).lines().position(|line| line.contains("café:")).unwrap() as u32;
    assert!(
        mappings
            .iter()
            .any(|mapping| mapping.generated_line == generated_line
                && mapping.source_line == Some(3)),
        "{mappings:?}\n{}",
        code(&result)
    );
    let (line, text) =
        code(&result).lines().enumerate().find(|(_, text)| text.contains("interface API")).unwrap();
    let column = text.find("API").unwrap() as u32;
    assert!(
        mappings.iter().any(|mapping| mapping.generated_line == line as u32
            && mapping.generated_character == column
            && mapping.source_line == Some(2)
            && mapping.source_character == Some(17)),
        "{mappings:?}"
    );
    check_consumer(
        &result.files,
        "import type { API } from './out/index.js'; const api: API = { café: 'ok', value: 1 };",
    );
}

#[test]
fn multi_entry_review_mixed_formats_and_reserved_names() {
    let (fs, _) = filesystem_with_config(
        &[
            (
                "/project/src/main.cts",
                "class Client { private brand!: void; value = 1 } export = Client;",
            ),
            ("/project/src/other.mts", "import Client = require('./main.cjs'); export { Client };"),
        ],
        &CONFIG.replace("src/**/*.ts", "src/**/*"),
    );
    let mut opts = options();
    opts.entry = "/project/missing.ts".into();
    let entries = std::collections::BTreeMap::from([
        ("main.d.cts".into(), "/project/src/main.cts".into()),
        ("other.d.mts".into(), "/project/src/other.mts".into()),
    ]);
    let result = tsrs_dts::build_entries(&opts, &entries, Arc::clone(&fs)).unwrap();
    let repeated = tsrs_dts::build_entries(&opts, &entries, Arc::clone(&fs)).unwrap();
    assert_eq!(result.files, repeated.files);
    check_consumer(
        &result.files,
        "import Client = require('./out/main.cjs'); import { Client as Other } from './out/other.mjs'; const value: Client = new Other();",
    );
    let invalid = std::collections::BTreeMap::from([(
        "__shared.d.cts.ambient.d.ts".into(),
        "/project/src/main.cts".into(),
    )]);
    assert!(
        tsrs_dts::build_entries(&opts, &invalid, fs).unwrap_err().to_string().contains("reserved")
    );
}

#[test]
fn insensitive_entries_and_output_names_use_canonical_paths() {
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        [
            ("/project/tsconfig.json", CONFIG),
            ("/project/src/index.ts", "export interface API { value: string }"),
        ],
        false,
    )));
    let mut opts = options();
    opts.entry = "/PROJECT/src/index.ts".into();
    assert!(code(&build(&opts, Arc::clone(&fs)).unwrap()).contains("value: string"));
    for names in [
        ["Foo.d.ts", "foo.d.ts"],
        ["__SHARED.d.cts", "other.d.ts"],
        ["__SHARED.d.cts.ambient.d.ts", "other.d.ts"],
    ] {
        let entries = names.into_iter().map(|name| (name.into(), opts.entry.clone())).collect();
        let error = tsrs_dts::build_entries(&opts, &entries, Arc::clone(&fs)).unwrap_err();
        assert!(error.0.contains("duplicate or reserved"), "{error}");
    }
}

#[test]
fn untyped_require_imports_remain_external() {
    // Without noImplicitAny, an untyped JavaScript package resolves to no module symbol
    // and no diagnostic.
    let config = r#"{"compilerOptions":{"strict":false,"module":"node16","moduleResolution":"node16","types":[]},"include":["src/**/*.ts"]}"#;
    let (fs, _) = filesystem_with_config(
        &[
            (
                "/project/node_modules/untyped/package.json",
                r#"{"name":"untyped","main":"index.js"}"#,
            ),
            ("/project/node_modules/untyped/index.js", "module.exports = 1;"),
            (
                "/project/src/index.ts",
                "import x = require('untyped'); export const y: typeof x = x;",
            ),
        ],
        config,
    );
    let result = build(&options(), fs).unwrap();
    let output = code(&result);
    assert!(output.contains("import x_1 = require(\"untyped\");"), "{output}");
    assert!(output.contains("declare const y_1: typeof x_1;"), "{output}");
}

#[test]
fn companion_names_do_not_depend_on_declaration_maps() {
    // Each value import from a conditional package needs its own companion.
    let mut inputs = vec![
        ("/project/package.json".to_string(), r#"{"type":"module"}"#.to_string()),
        (
            "/project/src/index.ts".into(),
            "import { a } from 'first'; import { b } from 'second'; export const values: [typeof a, typeof b] = [a, b];".into(),
        ),
    ];
    for (name, export) in [("first", "a"), ("second", "b")] {
        let root = format!("/project/node_modules/{name}");
        inputs.extend([
            (
                format!("{root}/package.json"),
                format!(r#"{{"name":"{name}","exports":{{"import":"./import.d.mts","require":"./require.d.cts"}}}}"#),
            ),
            (format!("{root}/import.d.mts"), format!("export declare const {export}: 'import';")),
            (format!("{root}/require.d.cts"), format!("export declare const {export}: 'require';")),
        ]);
    }
    let inputs: Vec<_> = inputs.iter().map(|(path, text)| (path.as_str(), text.as_str())).collect();
    let names = |maps: bool| {
        let config = format!(
            r#"{{"compilerOptions":{{"strict":true,"module":"nodenext","moduleResolution":"nodenext","types":[],"declarationMap":{maps}}},"include":["src/**/*.ts"]}}"#
        );
        let (fs, _) = filesystem_with_config(&inputs, &config);
        let result = build(&options(), fs).unwrap();
        result.files.into_keys().filter(|name| !name.contains(".map")).collect::<Vec<_>>()
    };
    let without_maps = names(false);
    assert!(
        without_maps.iter().filter(|name| name.contains(".external")).count() >= 2,
        "{without_maps:?}"
    );
    assert_eq!(without_maps, names(true));
}
