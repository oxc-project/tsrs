use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tsrs_dts::{BuildOptions, BuildResult, BuildSession, Files, OutputMode, build, build_entries};

use crate::filesystem;

fn options(reuse_diagnostics: bool) -> BuildOptions {
    BuildOptions {
        config_path: "/project/tsconfig.json".into(),
        entry: "/project/index.ts".into(),
        output: OutputMode::Bundle { file_name: "index.d.ts".into() },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: false,
        reuse_diagnostics,
    }
}

fn files(source: &str, extra: &[(&str, &str)]) -> Files {
    let mut files = Files::from([
        ("/project/tsconfig.json".into(), r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[]},"include":["**/*.ts"]}"#.into()),
        ("/project/package.json".into(), r#"{"type":"module"}"#.into()),
        ("/project/index.ts".into(), source.into()),
    ]);
    files.extend(extra.iter().map(|(path, text)| (path.to_string(), text.to_string())));
    files
}

fn assert_same(plain: &BuildResult, reuse: &BuildResult) {
    assert_eq!(plain.files, reuse.files);
    assert_eq!(plain.emitted_declarations, reuse.emitted_declarations);
    assert_eq!(plain.retained_statements, reuse.retained_statements);
    assert_eq!(plain.removed_statements, reuse.removed_statements);
}

fn compare(files: &Files) {
    let (fs, writes) = filesystem::memory_fs(files);
    let plain = build(&options(false), Arc::clone(&fs));
    let reuse = build(&options(true), fs);
    match (plain, reuse) {
        (Ok(plain), Ok(reuse)) => assert_same(&plain, &reuse),
        (Err(plain), Err(reuse)) => assert_eq!(plain.to_string(), reuse.to_string()),
        (plain, reuse) => panic!("different build outcomes: {plain:?} / {reuse:?}"),
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn modules_maps_and_multiple_entries_match_full_checking() {
    let mut input = files(
        "export { Item } from './item.js'; export type API = ReadonlyArray<Promise<string>>;",
        &[("/project/item.ts", "export class Item { private brand!: void; value = 42; }")],
    );
    input.insert("/project/tsconfig.json".into(), r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"declarationMap":true},"include":["**/*.ts"]}"#.into());
    compare(&input);
    let (fs, writes) = filesystem::memory_fs(&input);
    let entries = BTreeMap::from([
        ("index.d.ts".into(), "/project/index.ts".into()),
        ("item.d.ts".into(), "/project/item.ts".into()),
    ]);
    let plain = build_entries(&options(false), &entries, Arc::clone(&fs)).unwrap();
    let reuse = build_entries(&options(true), &entries, fs).unwrap();
    assert_same(&plain, &reuse);
    assert!(reuse.files.contains_key("item.d.ts.map"));
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_modules_cycles_and_qualified_imports_match_full_checking() {
    for maps in [false, true] {
        let mut input = files(
            "export * from './a.js'; export type API = import('./a.js').A; export { item } from './factory.js';",
            &[
                (
                    "/project/a.ts",
                    "import type { B } from './b.js'; export interface A { b?: B }; export type { B } from './b.js';",
                ),
                ("/project/b.ts", "import type { A } from './a.js'; export interface B { a?: A }"),
                (
                    "/project/factory.ts",
                    "import { make } from './value.js'; export const item = make();",
                ),
                (
                    "/project/value.ts",
                    "export interface Item { value: string }; export function make(): Item { return { value: '' }; }",
                ),
            ],
        );
        input.insert("/project/tsconfig.json".into(), format!(r#"{{"compilerOptions":{{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"declarationMap":{maps}}},"include":["**/*.ts"]}}"#));
        let (fs, _) = filesystem::memory_fs(&input);
        assert!(build(&options(true), fs).is_ok());
        compare(&input);
    }
}

#[test]
fn closed_modules_preserve_multiple_entry_nominal_identity() {
    for maps in [false, true] {
        let mut input = files(
            "export { Item as A } from './a.js'; export { Item as B } from './b.js'; export * from './empty.js';",
            &[
                ("/project/a.ts", "export class Item { private brand!: void; value = 1; }"),
                ("/project/b.ts", "export class Item { private brand!: void; value = 2; }"),
                ("/project/empty.ts", "export {};"),
            ],
        );
        input.insert("/project/tsconfig.json".into(), format!(r#"{{"compilerOptions":{{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"declarationMap":{maps}}},"include":["**/*.ts"]}}"#));
        let (fs, writes) = filesystem::memory_fs(&input);
        let entries = BTreeMap::from([
            ("index.d.ts".into(), "/project/index.ts".into()),
            ("a.d.ts".into(), "/project/a.ts".into()),
            ("b.d.ts".into(), "/project/b.ts".into()),
        ]);
        let plain = build_entries(&options(false), &entries, Arc::clone(&fs)).unwrap();
        let reuse = build_entries(&options(true), &entries, fs).unwrap();
        assert_same(&plain, &reuse);
        assert_eq!(reuse.files.contains_key("a.d.ts.map"), maps);
        assert_eq!(writes.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn closed_module_errors_match_fresh_checking() {
    for input in [
        files(
            "export * from './a.js'; export const wrong: string = 1;",
            &[("/project/a.ts", "export const wrong: number = 'wrong';")],
        ),
        files(
            "export * from './a.js'; using resource = null; export const wrong: string = 1;",
            &[("/project/a.ts", "export interface A { value: string }")],
        ),
    ] {
        let (fs, _) = filesystem::memory_fs(&input);
        assert!(build(&options(false), fs).is_err());
        compare(&input);
    }
    let mut input = files(
        "export * from './a.js';",
        &[(
            "/project/a.ts",
            "/** @internal */ interface Hidden { value: string }; export interface API { item: Hidden }",
        )],
    );
    input.insert("/project/tsconfig.json".into(), r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"stripInternal":true},"include":["**/*.ts"]}"#.into());
    let (fs, _) = filesystem::memory_fs(&input);
    assert_eq!(
        build(&options(false), fs).unwrap_err().to_string(),
        "a.d.ts(2,11): error TS2304: Cannot find name 'Hidden'."
    );
    compare(&input);
}

#[test]
fn source_and_dependency_errors_match_full_checking() {
    for input in [
        files("export const wrong: string = 1;", &[]),
        files(
            "export type { Item } from './dependency.js';",
            &[("/project/dependency.d.ts", "export interface Item extends Missing {}")],
        ),
        files("export type { Missing } from './missing.js';", &[]),
        files("export * from Module;", &[]),
        files(
            "export * from './item.js'; import { Item } from Module;",
            &[("/project/item.ts", "export interface Item {}")],
        ),
        files("export interface Item { value: string; value: number; }", &[]),
        files("export const broken = ;", &[]),
        files("export type Item = Missing<OtherMissing>; export const wrong: string = 1;", &[]),
    ] {
        let (fs, _) = filesystem::memory_fs(&input);
        assert!(build(&options(false), fs).is_err());
        compare(&input);
    }
}

#[test]
fn deferred_global_errors_match_fresh_checking() {
    let mut input = files("using resource = null; export const wrong: string = 1;", &[]);
    input.insert(
        "/project/tsconfig.json".into(),
        r#"{"compilerOptions":{"strict":true,"target":"es2022","lib":["es2022"],"module":"nodenext","moduleResolution":"nodenext","types":[]},"files":["index.ts"]}"#.into(),
    );
    let (fs, _) = filesystem::memory_fs(&input);
    let ordinary = build(&options(false), Arc::clone(&fs)).unwrap_err().to_string();
    // Pinned Go's ordinary collection appends the late global after semantic
    // file diagnostics. Incremental collection caches it with the file instead.
    assert_eq!(
        ordinary,
        "index.ts(1,37): error TS2322: Type 'number' is not assignable to type 'string'.\nerror TS2318: Cannot find global type 'Disposable'."
    );
    let reuse = build(&options(true), fs).unwrap_err().to_string();
    assert_eq!(reuse, ordinary);
}

#[test]
fn dependency_diagnostic_order_matches_fresh_checking() {
    let input = files(
        "export { z } from './z.js'; export const wrong: string = 1;",
        &[("/project/z.ts", "export const z: number = 'wrong';")],
    );
    let (fs, _) = filesystem::memory_fs(&input);
    let ordinary = build(&options(false), Arc::clone(&fs)).unwrap_err().to_string();
    assert_eq!(
        ordinary,
        "index.ts(1,42): error TS2322: Type 'number' is not assignable to type 'string'.\nz.ts(1,14): error TS2322: Type 'string' is not assignable to type 'number'."
    );
    let reuse = build(&options(true), fs).unwrap_err().to_string();
    assert_eq!(reuse, ordinary);
}

#[test]
fn stripped_internal_declaration_errors_match_fresh_checking() {
    let mut input = files(
        "/** @internal */ interface Hidden { value: string }; export interface API { item: Hidden }",
        &[],
    );
    input.insert(
        "/project/tsconfig.json".into(),
        r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[],"stripInternal":true},"files":["index.ts"]}"#.into(),
    );
    let (fs, _) = filesystem::memory_fs(&input);
    let ordinary = build(&options(false), Arc::clone(&fs)).unwrap_err().to_string();
    // Pinned Go checks the source successfully, then emits API with a dangling
    // Hidden reference. Neither Program has a form that disables reuse, so this
    // error exercises the fresh declaration-Program fallback.
    assert_eq!(ordinary, "index.d.ts(2,11): error TS2304: Cannot find name 'Hidden'.");
    let reuse = build(&options(true), fs).unwrap_err().to_string();
    assert_eq!(reuse, ordinary);
}

#[test]
fn umd_and_indirect_dependency_changes_match_fresh_builds() {
    // Each pair of Programs is independent. The session must also invalidate
    // its owned result when a UMD provider or its imported type changes.
    let provider = "export as namespace UmdApi; export type { Item } from './shape.js';";
    let mut session = BuildSession::new();
    for (umd, shape, expected_code) in [
        (Some(provider), "export interface Item { value: number }", None),
        (Some(provider), "export interface Item { value: string }", Some("TS2322")),
        (
            Some("export type { Item } from './shape.js';"),
            "export interface Item { value: number }",
            Some("TS2503"),
        ),
        (Some(provider), "export interface Item { value: number }", None),
        (
            Some("export as namespace OtherApi; export type { Item } from './shape.js';"),
            "export interface Item { value: number }",
            Some("TS2503"),
        ),
        (None, "export interface Item { value: number }", Some("TS2503")),
        (Some(provider), "export interface Item { value: number }", None),
    ] {
        let mut input = files(
            "export interface API { value: number }",
            &[
                (
                    "/project/client.ts",
                    "declare const item: UmdApi.Item; const checked: number = item.value;",
                ),
                ("/project/shape.d.ts", shape),
            ],
        );
        if let Some(umd) = umd {
            input.insert("/project/umd.d.ts".into(), umd.into());
        }
        compare(&input);
        let (fs, _) = filesystem::memory_fs(&input);
        let expected = build(&options(false), Arc::clone(&fs));
        if let Some(code) = expected_code {
            assert!(expected.as_ref().unwrap_err().to_string().contains(code));
        } else {
            assert!(expected.is_ok(), "{expected:?}");
        }
        let actual = session.build(&options(true), fs);
        match (expected, actual) {
            (Ok(expected), Ok(actual)) => assert_same(&expected, &actual),
            (Err(expected), Err(actual)) => assert_eq!(expected.to_string(), actual.to_string()),
            (expected, actual) => panic!("different session outcomes: {expected:?} / {actual:?}"),
        }
    }
}

#[test]
fn ambient_global_and_module_augmentations_match_full_checking() {
    for input in [
        files(
            "export interface API { item: GlobalItem }",
            &[("/project/global.d.ts", "interface GlobalItem { value: string }")],
        ),
        files(
            "export interface API { item: GlobalItem }",
            &[("/project/global.ts", "interface GlobalItem { value: string }")],
        ),
        files(
            "export type { Item } from './item.js';",
            &[
                ("/project/item.ts", "export interface Item { value: string }"),
                (
                    "/project/augment.ts",
                    "import './item.js'; declare module './item.js' { interface Item { extra: number } }",
                ),
            ],
        ),
        files(
            "export interface API { item: GlobalItem }",
            &[(
                "/project/augment.ts",
                "export {}; declare global { interface GlobalItem { value: string } }",
            )],
        ),
    ] {
        compare(&input);
    }
}

#[test]
fn skipped_library_checks_and_project_references_match_full_checking() {
    let mut input = files(
        "export interface API { value: string }",
        &[("/project/dependency.d.ts", "export interface Item extends Missing {}")],
    );
    input.insert(
        "/project/tsconfig.json".into(),
        r#"{"compilerOptions":{"strict":true,"types":[],"skipLibCheck":true},"include":["*.ts"]}"#
            .into(),
    );
    compare(&input);
    let input = Files::from([
        ("/project/tsconfig.json".into(), r#"{"compilerOptions":{"strict":true,"types":[]},"files":["index.ts"],"references":[{"path":"./dep"}]}"#.into()),
        ("/project/index.ts".into(), "export type { Item } from './dep/src/index.js';".into()),
        ("/project/dep/tsconfig.json".into(), r#"{"compilerOptions":{"composite":true,"rootDir":"src","outDir":"dist","types":[]},"files":["src/index.ts"]}"#.into()),
        ("/project/dep/src/index.ts".into(), "export interface Item { value: string }".into()),
    ]);
    compare(&input);
}

#[test]
fn changing_reuse_option_does_not_reuse_session_result() {
    let input = files("export interface API { value: number }", &[]);
    let (fs, _) = filesystem::memory_fs(&input);
    let mut session = BuildSession::new();
    let plain = session.build(&options(false), Arc::clone(&fs)).unwrap();
    let reuse = session.build(&options(true), Arc::clone(&fs)).unwrap();
    assert!(!Arc::ptr_eq(&plain, &reuse));
    assert_same(&plain, &reuse);
    let repeated = session.build(&options(true), fs).unwrap();
    assert!(Arc::ptr_eq(&reuse, &repeated));
}
