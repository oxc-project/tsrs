use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use tsrs_ast::{self as ast, Kind, Symbol, SymbolFlags, SymbolId};
use tsrs_checker::Checker;
use tsrs_compiler::Context;
use tsrs_core::P;

use crate::filesystem;
use crate::program;

// The scan branch preserves the pre-API bundler algorithms as the differential oracle.
fn legacy_export(
    checker: &mut Checker,
    module: P<Symbol>,
    name: &str,
    visited: &mut BTreeSet<SymbolId>,
    direct_lookup: bool,
) -> Option<bool> {
    let id = ast::get_symbol_id(module);
    if !visited.insert(id) {
        return None;
    }
    let result = (|| {
        if let Some(symbol) = module.exports().and_then(|exports| exports.lookup(name)) {
            return Some(legacy_alias(checker, symbol));
        }
        let file = ast::get_source_file_of_module(module)?;
        let mut restriction = None;
        for &node in file.statements().nodes() {
            if node.kind() != Kind::ExportDeclaration {
                continue;
            }
            let data = node.as_export_declaration();
            if data.export_clause().is_some() || name == "default" {
                continue;
            }
            let Some(target) = data.module_specifier().and_then(|specifier| {
                checker.resolve_external_module_name_exported(specifier, None)
            }) else {
                continue;
            };
            let contains = if direct_lookup {
                checker.try_get_member_in_module_exports(name, target).is_some()
            } else {
                checker.get_exports_of_module_exported(target).iter().any(|s| s.name() == name)
            };
            if !contains {
                continue;
            }
            if let Some(type_only) = legacy_export(checker, target, name, visited, direct_lookup) {
                if !data.is_type_only() && !type_only {
                    return Some(false);
                }
                restriction = Some(true);
            }
        }
        restriction
    })();
    visited.remove(&id);
    result
}

fn compare(files: &BTreeMap<String, String>, use_metadata: bool) -> BTreeMap<String, (bool, bool)> {
    compare_lookup(files, use_metadata, false)
}

fn compare_lookup(
    files: &BTreeMap<String, String>,
    use_metadata: bool,
    direct_lookup: bool,
) -> BTreeMap<String, (bool, bool)> {
    let mut files = files.clone();
    files.insert(
        "/project/tsconfig.json".into(),
        r#"{"compilerOptions":{"strict":true,"types":[],"noEmit":true},"include":["*.ts"]}"#.into(),
    );
    let (fs, _) = filesystem::memory_fs(&files);
    let host = program::HostProgram::new(Arc::clone(&fs), "/project/tsconfig.json");
    let context = Context::background();
    let _diagnostics = host.diagnostics();
    let mut result = BTreeMap::new();
    for file in host.program.source_files() {
        if !files.contains_key(file.file_name()) {
            continue;
        }
        let Some(module) = file.symbol() else { continue };
        let mut checker = host.program.get_type_checker_for_file_exclusive(&context, *file);
        if use_metadata {
            // The query must initialize the export cache itself.
            assert!(checker.get_type_only_export_star_declaration(module, "missing").is_none());
        }
        let exports = checker.get_exports_of_module_exported(module);
        if direct_lookup {
            for name in exports.iter().map(|s| s.name()).chain(["missing", "default"]) {
                assert_eq!(
                    exports.iter().any(|s| s.name() == name),
                    checker.try_get_member_in_module_exports(name, module).is_some(),
                    "{}:{name}: export membership",
                    file.file_name(),
                );
            }
        }
        for symbol in exports {
            // Match production's old call order: inspect export paths before
            // the fallback alias query can initialize additional checker state.
            let restricted = if use_metadata {
                checker.get_type_only_export_star_declaration(module, symbol.name()).is_some()
                    || checker
                        .get_type_only_alias_declaration_ex(symbol, SymbolFlags::None)
                        .is_some()
            } else {
                legacy_export(
                    &mut checker,
                    module,
                    symbol.name(),
                    &mut BTreeSet::new(),
                    direct_lookup,
                )
                .unwrap_or_else(|| legacy_alias(&mut checker, symbol))
            };
            let alias = if use_metadata {
                checker.get_type_only_alias_declaration_ex(symbol, SymbolFlags::None).is_some()
            } else {
                legacy_alias(&mut checker, symbol)
            };
            result.insert(format!("{}:{}", file.file_name(), symbol.name()), (alias, restricted));
        }
    }
    result
}

#[test]
fn queries_initialize_metadata_without_a_diagnostic_pass() {
    let files = BTreeMap::from([
        (
            "/project/tsconfig.json".into(),
            r#"{"compilerOptions":{"types":[]},"files":["index.ts","named.ts"]}"#.into(),
        ),
        ("/project/index.ts".into(), "export type * from './dep';".into()),
        ("/project/named.ts".into(), "export type { C } from './dep';".into()),
        ("/project/dep.ts".into(), "export class C {}".into()),
    ]);
    let (fs, _) = filesystem::memory_fs(&files);
    let host = program::HostProgram::new(fs, "/project/tsconfig.json");
    let context = Context::background();
    let file = host.program.get_source_file("/project/index.ts").unwrap();
    let mut checker = host.program.get_type_checker_for_file_exclusive(&context, file);
    let module = file.symbol().unwrap();
    let declaration = checker.get_type_only_export_star_declaration(module, "C").unwrap();
    assert_eq!(declaration, file.statements().nodes()[0]);
    assert!(checker.get_type_only_export_star_declaration(module, "default").is_none());
    assert!(checker.get_type_only_export_star_declaration(module, "missing").is_none());
    drop(checker);
    let file = host.program.get_source_file("/project/named.ts").unwrap();
    let mut checker = host.program.get_type_checker_for_file_exclusive(&context, file);
    let module = file.symbol().unwrap();
    assert!(checker.get_type_only_export_star_declaration(module, "C").is_none());
    let symbol = checker
        .get_exports_of_module_exported(module)
        .into_iter()
        .find(|s| s.name() == "C")
        .unwrap();
    assert!(checker.get_type_only_alias_declaration_ex(symbol, SymbolFlags::Alias).is_none());
    let declaration =
        checker.get_type_only_alias_declaration_ex(symbol, SymbolFlags::None).unwrap();
    assert_eq!(ast::get_source_file_of_node(declaration), Some(file));
}

#[test]
fn metadata_matches_export_paths_cycles_and_merged_symbols() {
    let cases = [
        ("named chain", "export { C } from './middle';", "export { C } from './typed';"),
        ("type star", "export type * from './middle';", "export * from './dep';"),
        (
            "named type through ordinary star",
            "export * from './middle';",
            "export { C } from './typed';",
        ),
        (
            "type then value",
            "export type * from './typed'; export * from './middle';",
            "export * from './dep';",
        ),
        (
            "value then type",
            "export * from './middle'; export type * from './typed';",
            "export * from './dep';",
        ),
        (
            "direct override",
            "export type * from './typed'; export { C } from './dep';",
            "export * from './dep';",
        ),
        (
            "star cycle",
            "export * from './middle';",
            "export * from './index'; export type * from './dep';",
        ),
        ("alias cycle", "export { C } from './middle';", "export { C } from './index';"),
        (
            "merged alias",
            "import type { C } from './dep'; interface C { extra: string } export { C };",
            "export * from './dep';",
        ),
        (
            "merged value",
            "import type { C } from './dep'; const C = 1; export { C };",
            "export * from './dep';",
        ),
        (
            "merged namespace",
            "export * from './middle';",
            "import { C } from './typed'; namespace C { export interface Extra {} } export { C };",
        ),
        (
            "namespace alias",
            "export { C } from './middle';",
            "import type * as C from './dep'; export { C };",
        ),
        (
            "qualified alias",
            "export { C } from './middle';",
            "import * as N from './typed'; import C = N.C; export { C };",
        ),
    ];
    for (name, index, middle) in cases {
        let files = BTreeMap::from([
            ("/project/index.ts".into(), index.into()),
            ("/project/middle.ts".into(), middle.into()),
            ("/project/typed.ts".into(), "export type { C } from './dep';".into()),
            ("/project/dep.ts".into(), "export class C {}".into()),
        ]);
        let legacy = compare(&files, false);
        assert_eq!(legacy, compare_lookup(&files, false, true), "direct lookup");
        let metadata = compare(&files, true);
        assert_eq!(legacy.keys().collect::<Vec<_>>(), metadata.keys().collect::<Vec<_>>());
        let mut mismatches = Vec::new();
        for (key, &(alias, restricted)) in &legacy {
            let &(new_alias, new_restricted) = &metadata[key];
            assert_eq!(alias, new_alias, "{name} {key}: alias query");
            if restricted != new_restricted {
                assert!(matches!(name, "type then value" | "value then type"), "{name} {key}");
                assert_eq!((restricted, new_restricted), (false, true));
                mismatches.push(key.as_str());
            }
        }
        // The selected star alias depends on checker visitation order. Either
        // result is possible, but a disagreement is confined to this competing
        // path and must never affect equivalence of the alias query itself.
        assert!(mismatches.is_empty() || mismatches == ["/project/index.ts:C"], "{name}");
    }
}

#[test]
fn direct_lookup_matches_default_string_names_and_commonjs() {
    for dep in [
        "const local = 1; export { local as renamed }; export default local;",
        "const local = 1; export { local as 'hyphen-name', local as '__name', local as 'λ' };",
        "namespace API { export class C {} export interface T {} } export = API;",
        "export namespace API { export class C {} } export import Alias = API.C;",
    ] {
        let files = BTreeMap::from([
            ("/project/index.ts".into(), "export * from './middle';".into()),
            ("/project/middle.ts".into(), "export * from './dep';".into()),
            ("/project/dep.ts".into(), dep.into()),
        ]);
        assert_eq!(compare(&files, false), compare_lookup(&files, false, true), "{dep}");
    }
}

#[test]
fn augmentation_keeps_export_key_distinct_from_symbol_name() {
    let files = BTreeMap::from([
        (
            "/project/tsconfig.json".into(),
            r#"{"compilerOptions":{"strict":true,"types":[],"noEmit":true},"files":["index.d.ts","aug.d.ts"]}"#.into(),
        ),
        ("/project/index.d.ts".into(), "export * from './dep';".into()),
        (
            "/project/dep.d.ts".into(),
            "interface Original { a: number } export { Original as Renamed };".into(),
        ),
        (
            "/project/aug.d.ts".into(),
            "import './dep'; declare module './dep' { interface Renamed { b: number } }".into(),
        ),
    ]);
    let (fs, _) = filesystem::memory_fs(&files);
    let host = program::HostProgram::new(fs, "/project/tsconfig.json");
    let context = Context::background();
    let diagnostics = tsrs_compiler::get_diagnostics_of_any_program(
        &context,
        host.program,
        None,
        false,
        &mut |ctx, file| host.program.get_bind_diagnostics(ctx, file),
        &mut |ctx, file| host.program.get_semantic_diagnostics(ctx, file),
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let file = host.program.get_source_file("/project/dep.d.ts").unwrap();
    let module = file.symbol().unwrap();
    let mut checker = host.program.get_type_checker_for_file_exclusive(&context, file);
    // The source-file symbol retains its original table; augmentation is visible
    // through the checker's merged module symbol.
    let module = checker.get_merged_symbol(module);
    let exports = checker.get_exports_of_module_exported(module);
    let original = exports.iter().copied().find(|symbol| symbol.name() == "Original").unwrap();
    assert!(checker.try_get_member_in_module_exports("Original", module).is_none());
    assert_eq!(checker.try_get_member_in_module_exports("Renamed", module), Some(original));
    // The old membership scan searches values by symbol name, not table keys.
    // Module augmentation disproves equivalence of replacing it with key lookup.
    assert!(exports.iter().any(|symbol| symbol.name() == "Original"));
}

#[test]
fn metadata_matches_existing_type_only_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut differences = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("type_only_") && name != "namespace_review_type_only_alias_chain" {
            continue;
        }
        let files = std::fs::read_dir(entry.path())
            .unwrap()
            .filter_map(|entry| {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_str().unwrap();
                (path.is_file()
                    && path.extension().is_some_and(|ext| ext == "ts")
                    && name != "consumer.ts")
                    .then(|| (format!("/project/{name}"), std::fs::read_to_string(&path).unwrap()))
            })
            .collect();
        let legacy = compare(&files, false);
        assert_eq!(legacy, compare_lookup(&files, false, true), "direct lookup");
        let metadata = compare(&files, true);
        for (key, value) in &legacy {
            if metadata.get(key) != Some(value) {
                differences.push(format!("{name} {key}: {value:?} vs {:?}", metadata.get(key)));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

fn legacy_alias(checker: &mut Checker, mut symbol: P<Symbol>) -> bool {
    let mut visited = BTreeSet::new();
    while symbol.flags().contains(SymbolFlags::Alias) {
        if !visited.insert(ast::get_symbol_id(symbol)) {
            return false;
        }
        if checker.get_type_only_alias_declaration(symbol).is_some() {
            return true;
        }
        let Some(next) = checker.get_immediate_aliased_symbol_exported(symbol) else {
            return false;
        };
        symbol = next;
    }
    false
}
