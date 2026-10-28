use std::sync::Arc;

use crate::program::HostProgram;
use tsrs_compiler::Context;
use tsrs_core::{CompilerOptions, Tristate, arena::Region};
use tsrs_dts::diagnostics::{closed_project, eligible};
use tsrs_vfs::{FS, vfstest};

use tsrs_dts::{CapturedDeclarations, DeclarationContext, Files, emit_declarations};

const CONFIG: &str = r#"{"compilerOptions":{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[]},"include":["**/*.ts"]}"#;
const ITEM: &str = "export interface Item { value: string }";

fn source_program(
    source: &str,
    extra: &[(&str, &str)],
    config: &str,
    run_external_code: bool,
) -> (HostProgram, Files) {
    let mut files = vec![
        ("/project/tsconfig.json", config),
        ("/project/package.json", r#"{"type":"module"}"#),
        ("/project/index.ts", source),
        ("/project/item.ts", ITEM),
    ];
    files.extend_from_slice(extra);
    let files: Files = files.into_iter().map(|(path, text)| (path.into(), text.into())).collect();
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        files.iter().map(|(path, text)| (path.as_str(), text.as_str())),
        true,
    )));
    let source = HostProgram::with_config(fs, "/project/tsconfig.json", |config| {
        let original = config.parsed_config.compiler_options.unwrap();
        config.parsed_config.compiler_options = Some(tsrs_core::P::new(CompilerOptions {
            declaration: Tristate::True,
            emit_declaration_only: Tristate::True,
            no_emit_on_error: Tristate::True,
            run_external_code: tsrs_core::bool_to_tristate(run_external_code),
            ..original.get().clone()
        }));
    });
    (source, files)
}

fn declaration_program(
    context: &DeclarationContext,
    captured: &CapturedDeclarations,
    mut files: Files,
) -> HostProgram {
    for source in captured.source_to_declaration.keys() {
        files.remove(source);
    }
    files.extend(captured.files.clone());
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(vfstest::from_map(
        files.iter().map(|(path, text)| (path.as_str(), text.as_str())),
        true,
    )));
    HostProgram::with_config(fs, "/project/tsconfig.json", |config| {
        config.parsed_config.file_names = captured.files.keys().cloned().collect();
        config.parsed_config.compiler_options = Some(tsrs_core::P::new(CompilerOptions {
            no_emit: Tristate::True,
            declaration: Tristate::False,
            emit_declaration_only: Tristate::False,
            declaration_map: Tristate::False,
            ..context.compiler_options.clone()
        }));
    })
}

fn assert_source_and_declarations(
    name: &str,
    source: &str,
    extra: &[(&str, &str)],
    expected_closed: bool,
    expected_eligible: bool,
) -> CapturedDeclarations {
    // The outer region also owns allocations escaping checker/emit scratch scopes.
    // Both Programs drop before it; no AST or checker handles escape this helper.
    let region = Region::new_scratch(1 << 20);
    let _scope = region.enter();
    let (source, fs) = source_program(source, extra, CONFIG, false);
    assert_eq!(closed_project(source.program), expected_closed, "{name}: source closure");
    assert_eq!(eligible(source.program), expected_eligible, "{name}: source eligibility");
    let context = DeclarationContext::from_program(source.program);
    let captured = emit_declarations(source.program, &Context::background())
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    let declarations = declaration_program(&context, &captured, fs);
    assert_eq!(
        closed_project(declarations.program),
        expected_closed,
        "{name}: declaration closure"
    );
    assert_eq!(
        eligible(declarations.program),
        expected_eligible,
        "{name}: declaration eligibility"
    );
    captured
}

#[test]
fn closed_internal_modules_are_eligible_before_and_after_emit() {
    for (name, source, extra) in [
        ("star export", "export * from './item.js';", vec![]),
        ("named export", "export type { Item } from './item.js';", vec![]),
        (
            "cycle",
            "export * from './a.js';",
            vec![
                ("/project/a.ts", "export * from './b.js'; export interface A { a: string }"),
                ("/project/b.ts", "export * from './a.js'; export interface B { b: number }"),
            ],
        ),
        (
            "qualified import type",
            "export * from './item.js'; export type API = import('./item.js').Item;",
            vec![],
        ),
    ] {
        assert_source_and_declarations(name, source, &extra, true, true);
    }
}

#[test]
fn generated_qualified_import_type_remains_eligible() {
    let captured = assert_source_and_declarations(
        "generated qualified import type",
        "import { make } from './factory.js'; export * from './item.js'; export const item = make();",
        &[
            ("/project/item.ts", "export class Item { value = 'item'; }"),
            (
                "/project/factory.ts",
                "import { Item } from './item.js'; export function make() { return new Item(); }",
            ),
        ],
        true,
        true,
    );
    let declaration = &captured.files["/project/index.d.ts"];
    assert!(declaration.contains("import(\"./item.js\").Item"), "{declaration}");
}

#[test]
fn ordering_sensitive_project_syntax_is_not_closed_before_or_after_emit() {
    // Each entry has a star export to rule out the original eligibility alternative.
    for (name, source, extra) in [
        (
            "whole-module import type",
            "export * from './item.js'; export type API = typeof import('./item.js');",
            vec![],
        ),
        (
            "namespace import",
            "import * as items from './item.js'; export * from './item.js'; export { items };",
            vec![],
        ),
        (
            "namespace export",
            "export * from './item.js'; export * as items from './item.js';",
            vec![],
        ),
        (
            "transitive namespace target",
            "export * from './bridge.js'; export type API = import('./bridge.js').Items.Item;",
            vec![("/project/bridge.ts", "export * as Items from './item.js';")],
        ),
        (
            "namespace declaration",
            "export * from './item.js'; export namespace API { export type Value = string; }",
            vec![],
        ),
        (
            "import equals",
            "export * from './item.js'; import API = require('./legacy.cjs'); export { API };",
            vec![("/project/legacy.cts", "export interface Item { value: string }")],
        ),
        (
            "global augmentation",
            "export * from './item.js'; declare global { interface GlobalItem { value: string } }",
            vec![],
        ),
        (
            "module augmentation",
            "export * from './item.js'; declare module './item.js' { interface Item { extra: number } }",
            vec![],
        ),
        (
            "global script",
            "export * from './item.js';",
            vec![("/project/global.d.ts", "interface GlobalItem { value: string }")],
        ),
        (
            "external qualified import type",
            "export * from './item.js'; export type API = import('dependency').External;",
            vec![
                (
                    "/project/node_modules/dependency/package.json",
                    r#"{"name":"dependency","type":"module","types":"index.d.ts"}"#,
                ),
                (
                    "/project/node_modules/dependency/index.d.ts",
                    "export interface External { value: string }",
                ),
            ],
        ),
        (
            "external module",
            "export * from './item.js'; export type { External } from 'dependency';",
            vec![
                (
                    "/project/node_modules/dependency/package.json",
                    r#"{"name":"dependency","type":"module","types":"index.d.ts"}"#,
                ),
                (
                    "/project/node_modules/dependency/index.d.ts",
                    "export interface External { value: string }",
                ),
            ],
        ),
    ] {
        assert_source_and_declarations(name, source, &extra, false, false);
    }
}

#[test]
fn emitted_whole_module_import_type_disables_declaration_reuse() {
    let region = Region::new_scratch(1 << 20);
    let _scope = region.enter();
    let (source, fs) = source_program(
        "export * from './item.js'; export const load = () => import('./item.js');",
        &[],
        CONFIG,
        false,
    );
    assert!(eligible(source.program));
    let context = DeclarationContext::from_program(source.program);
    let captured = emit_declarations(source.program, &Context::background()).unwrap();
    assert!(captured.files["/project/index.d.ts"].contains("typeof import("));
    let declarations = declaration_program(&context, &captured, fs);
    assert!(!closed_project(declarations.program));
    assert!(!eligible(declarations.program));
}

#[test]
fn original_eligibility_alternative_remains_available() {
    for (name, source) in [
        ("whole-module import type", "export type API = typeof import('./item.js');"),
        ("namespace declaration", "export namespace API { export type Value = string; }"),
    ] {
        assert_source_and_declarations(name, source, &[], false, true);
    }
}

#[test]
fn unresolved_and_nonliteral_module_edges_are_not_closed() {
    for (name, source) in [
        ("unresolved export", "export * from './missing.js';"),
        ("nonliteral export", "export * from Module;"),
        ("nonliteral import", "export * from './item.js'; import { Item } from Module;"),
        (
            "unresolved qualified import type",
            "export * from './item.js'; export type API = import('./missing.js').Item;",
        ),
        (
            "nonliteral import type",
            "export * from './item.js'; type Module = './item.js'; export type API = import(Module).Item;",
        ),
    ] {
        let (owner, _) = source_program(source, &[], CONFIG, false);
        assert!(!closed_project(owner.program), "{name}");
        assert!(!eligible(owner.program), "{name}");
    }
}

#[test]
fn dependency_namespace_syntax_is_allowed_but_dependency_umd_is_not() {
    for (name, dependency, expected_eligible) in [
        (
            "dependency namespace import and export",
            "import * as Shape from './shape.js'; export { Shape }; export namespace API { type Value = string; }",
            true,
        ),
        ("dependency UMD", "export as namespace Dependency; export interface API {}", false),
    ] {
        assert_source_and_declarations(
            name,
            "/// <reference types=\"dependency\" preserve=\"true\" />\nexport * from './item.js';",
            &[
                (
                    "/project/node_modules/dependency/package.json",
                    r#"{"name":"dependency","type":"module","types":"index.d.ts"}"#,
                ),
                ("/project/node_modules/dependency/index.d.ts", dependency),
                ("/project/node_modules/dependency/shape.d.ts", ITEM),
            ],
            true,
            expected_eligible,
        );
    }
}

#[test]
fn option_gates_disable_source_eligibility() {
    let (ordinary, _) = source_program("export * from './item.js';", &[], CONFIG, false);
    assert!(eligible(ordinary.program));
    for option in ["skipLibCheck", "skipDefaultLibCheck"] {
        let config = format!(
            r#"{{"compilerOptions":{{"strict":true,"types":[],"{option}":true}},"include":["*.ts"]}}"#
        );
        let (owner, _) = source_program("export * from './item.js';", &[], &config, false);
        assert!(closed_project(owner.program), "{option}");
        assert!(!eligible(owner.program), "{option}");
    }
    let (external, _) = source_program("export * from './item.js';", &[], CONFIG, true);
    assert!(closed_project(external.program));
    assert!(!eligible(external.program));

    let (references, _) = source_program(
        "export * from './item.js';",
        &[(
            "/project/dep/tsconfig.json",
            r#"{"compilerOptions":{"composite":true},"files":["../item.ts"]}"#,
        )],
        r#"{"compilerOptions":{"types":[]},"files":["index.ts","item.ts"],"references":[{"path":"./dep"}]}"#,
        false,
    );
    assert_eq!(references.program.command_line().project_references().len(), 1);
    assert!(!eligible(references.program));
}
