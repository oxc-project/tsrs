use std::collections::BTreeMap;
use std::sync::{Arc, atomic::Ordering};

use tsrs_compiler::Context;
use tsrs_core::Tristate;
use tsrs_dts::{
    CapturedDeclarations, DeclarationContext, DeclarationInputs, LinkOptions, link_declarations,
};
use tsrs_vfs::FS;

use crate::filesystem;
use crate::program;

fn options() -> LinkOptions {
    LinkOptions { entries: BTreeMap::from([("index.d.ts".into(), "/project/index.ts".into())]) }
}
fn settings() -> DeclarationContext {
    DeclarationContext {
        compiler_options: tsrs_core::CompilerOptions::default(),
        project_references: Vec::new(),
        current_directory: "/project".into(),
        default_library_path: tsrs_vfs::bundled::lib_path(),
    }
}
fn capture(text: &str) -> CapturedDeclarations {
    CapturedDeclarations {
        files: BTreeMap::from([("/project/index.d.ts".into(), text.into())]),
        source_to_declaration: BTreeMap::from([(
            "/project/index.ts".into(),
            "/project/index.d.ts".into(),
        )]),
        ..Default::default()
    }
}
fn link(captured: CapturedDeclarations) -> tsrs_dts::Result<tsrs_dts::BuildResult> {
    let (fs, writes) = filesystem::memory_fs(&BTreeMap::new());
    let result =
        link_declarations(DeclarationInputs { captured, context: settings(), fs }, &options());
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    result
}

#[test]
fn captures_do_not_need_source_or_config_files() {
    let result = link(capture("export interface API { value: string }")).unwrap();
    assert!(result.files["index.d.ts"].contains("value: string"));
}

#[test]
fn declaration_errors_are_checked_even_with_host_skip_flags() {
    let (fs, _) = filesystem::memory_fs(&BTreeMap::new());
    let mut context = settings();
    context.compiler_options.no_check = Tristate::True;
    context.compiler_options.skip_lib_check = Tristate::True;
    context.compiler_options.skip_default_lib_check = Tristate::True;
    let error = link_declarations(
        DeclarationInputs {
            captured: capture("export interface API extends Missing {}"),
            context,
            fs,
        },
        &options(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("TS2304"), "{error}");
}

#[test]
fn default_library_diagnostics_cannot_be_skipped_by_capture_options() {
    let library = format!(
        "/custom/{}",
        tsrs_tsoptions::get_default_lib_file_name(&settings().compiler_options)
    );
    let files = BTreeMap::from([(
        library,
        "interface Array<T> {} interface Boolean {} interface CallableFunction {} interface Function {} interface IArguments {} interface NewableFunction {} interface Number {} interface Object {} interface RegExp {} interface String {} interface Broken extends MissingInLibrary {}".into(),
    )]);
    let (fs, _) = filesystem::memory_fs(&files);
    let mut context = settings();
    context.default_library_path = "/custom".into();
    context.compiler_options.skip_default_lib_check = Tristate::True;
    context.compiler_options.skip_lib_check = Tristate::True;
    let error = link_declarations(
        DeclarationInputs { captured: capture("export {};"), context, fs },
        &options(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("MissingInLibrary"), "{error}");
}

#[test]
fn malformed_capture_metadata_returns_errors() {
    let mut cases = Vec::new();
    let mut missing = capture("export {};");
    missing.files.clear();
    cases.push((missing, "missing captured declaration"));
    let mut relative = capture("export {};");
    relative.files.insert("relative.d.ts".into(), "export {};".into());
    cases.push((relative, "absolute and normalized"));
    let mut implementation = capture("export {};");
    implementation.files.insert("/project/other.ts".into(), "export {};".into());
    cases.push((implementation, "not a declaration file"));
    let mut map = capture("export {};");
    map.declaration_maps.insert(
        "/project/index.d.ts".into(),
        tsrs_compiler::SourceMapEmitResult {
            input_source_file_names: vec![],
            source_map: tsrs_sourcemap::RawSourceMap { mappings: "!".into(), ..Default::default() },
            generated_file: "/project/index.d.ts".into(),
        },
    );
    cases.push((map, "invalid declaration map"));
    for (sources, mappings, names) in [
        (vec![], "AAAA", vec![]),
        (vec!["index.ts"], "ACAA", vec![]),
        (vec!["index.ts"], "AAAAC", vec!["API"]),
    ] {
        let mut captured = capture("export {};");
        let raw = tsrs_sourcemap::RawSourceMap::from_json(
            &tsrs_core::json::unmarshal(&format!(
                r#"{{"version":3,"sources":{sources:?},"names":{names:?},"mappings":"{mappings}"}}"#
            ))
            .unwrap(),
        )
        .unwrap();
        captured.declaration_maps.insert(
            "/project/index.d.ts".into(),
            tsrs_compiler::SourceMapEmitResult {
                input_source_file_names: vec!["/project/index.ts".into()],
                source_map: raw,
                generated_file: "/project/index.d.ts".into(),
            },
        );
        cases.push((captured, "invalid declaration map"));
    }
    for (capture, message) in cases {
        let error = link(capture).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    for name in ["../index.d.ts", "dir\\index.d.ts", "__shared.d.ts", "index.ts"] {
        let (fs, _) = filesystem::memory_fs(&BTreeMap::new());
        let error = link_declarations(
            DeclarationInputs { captured: capture("export {};"), context: settings(), fs },
            &LinkOptions { entries: BTreeMap::from([(name.into(), "/project/index.ts".into())]) },
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid or reserved"));
    }
}

fn host(source: &str, extra_options: &str) -> (program::HostProgram, Arc<dyn FS>) {
    let files = BTreeMap::from([
        ("/project/index.ts".into(), source.into()),
        (
            "/project/tsconfig.json".into(),
            format!(
                r#"{{"compilerOptions":{{"strict":true,"types":[],"declaration":true{extra_options}}},"files":["index.ts"]}}"#
            ),
        ),
    ]);
    let (fs, _) = filesystem::memory_fs(&files);
    (program::HostProgram::new(Arc::clone(&fs), "/project/tsconfig.json"), fs)
}

#[test]
fn borrowed_program_stays_usable_and_options_are_unchanged() {
    let (host, fs) = host("export const answer: number = 42;", "");
    let original = host.program.options().get().clone();
    let context = Context::background();
    let first =
        tsrs_dts::build_from_program(host.program, &context, &options(), Arc::clone(&fs)).unwrap();
    let second = tsrs_dts::build_from_program(host.program, &context, &options(), fs).unwrap();
    assert_eq!(first.files, second.files);
    assert_eq!(host.program.options().get(), &original);
    assert_eq!(host.diagnostics(), [] as [String; 0]);
    let file = host.program.get_source_file("/project/index.ts").unwrap();
    let mut checker = host.program.get_type_checker_for_file_exclusive(&context, file);
    assert_eq!(checker.get_exports_of_module_exported(file.symbol().unwrap()).len(), 1);
}

#[test]
fn declaration_maps_match_full_build_with_host_output_directories() {
    let (host, fs) = host(
        "export interface API { value: string }",
        ",\"declarationMap\":true,\"outDir\":\"dist\",\"rootDir\":\".\"",
    );
    let full = tsrs_dts::build(
        &tsrs_dts::BuildOptions {
            config_path: "/project/tsconfig.json".into(),
            entry: "/project/index.ts".into(),
            output: tsrs_dts::OutputMode::Bundle { file_name: "index.d.ts".into() },
            default_library_path: tsrs_vfs::bundled::lib_path(),
            run_external_code: false,
            reuse_diagnostics: false,
        },
        Arc::clone(&fs),
    )
    .unwrap();
    let borrowed =
        tsrs_dts::build_from_program(host.program, &Context::background(), &options(), fs).unwrap();
    assert!(borrowed.files.contains_key("index.d.ts.map"));
    assert_eq!(full.files, borrowed.files);
}

#[test]
fn borrowed_program_rejects_source_errors_and_incompatible_options() {
    for (source, extra, expected) in [
        ("export const value: string = 1;", "", "TS2322"),
        ("export {};", ",\"noEmit\":true", "disable noEmit"),
        ("export const value: string = 1;", ",\"noCheck\":true", "disable noCheck"),
        ("export {};", ",\"declaration\":false", "enable declaration"),
        ("export {};", ",\"outFile\":\"bundle.js\"", "outFile"),
    ] {
        let (host, fs) = host(source, extra);
        let error =
            tsrs_dts::build_from_program(host.program, &Context::background(), &options(), fs)
                .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn prepared_project_references_match_full_build_without_rebuilding() {
    let config = r#"{"compilerOptions":{"strict":true,"types":[],"declaration":true},"files":["index.ts"],"references":[{"path":"./dep"}]}"#;
    let files = BTreeMap::from([
        ("/project/tsconfig.json".into(), config.into()),
        ("/project/index.ts".into(), "export type { Item } from './dep/src/index.js';".into()),
        ("/project/dep/tsconfig.json".into(), r#"{"compilerOptions":{"composite":true,"rootDir":"src","outDir":"dist","types":[]},"files":["src/index.ts"]}"#.into()),
        ("/project/dep/src/index.ts".into(), "export interface Item { value: string }".into()),
        ("/project/dep/dist/index.d.ts".into(), "export interface Item { value: string }".into()),
    ]);
    let (fs, writes) = filesystem::memory_fs(&files);
    let expected = tsrs_dts::build(
        &tsrs_dts::BuildOptions {
            config_path: "/project/tsconfig.json".into(),
            entry: "/project/index.ts".into(),
            output: tsrs_dts::OutputMode::Bundle { file_name: "index.d.ts".into() },
            default_library_path: tsrs_vfs::bundled::lib_path(),
            run_external_code: false,
            reuse_diagnostics: false,
        },
        Arc::clone(&fs),
    )
    .unwrap();
    let host = program::HostProgram::new(Arc::clone(&fs), "/project/tsconfig.json");
    assert_eq!(host.program.command_line().project_references().len(), 1);
    let actual =
        tsrs_dts::build_from_program(host.program, &Context::background(), &options(), fs).unwrap();
    assert_eq!(expected.files, actual.files);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

#[test]
fn resolution_uses_new_dependency_snapshot_on_each_link() {
    let mut errors = Vec::new();
    for declaration in ["export interface Item {}", "export interface Other {}"] {
        let files = BTreeMap::from([
            (
                "/project/node_modules/external/package.json".into(),
                r#"{"types":"index.d.ts"}"#.into(),
            ),
            ("/project/node_modules/external/index.d.ts".into(), declaration.into()),
        ]);
        let (fs, _) = filesystem::memory_fs(&files);
        let result = link_declarations(
            DeclarationInputs {
                captured: capture("export type API = import('external').Item;"),
                context: settings(),
                fs,
            },
            &options(),
        );
        errors.push(result.err().map(|error| error.to_string()));
    }
    assert!(errors[0].is_none());
    assert!(errors[1].as_ref().unwrap().contains("TS2694"));
}
