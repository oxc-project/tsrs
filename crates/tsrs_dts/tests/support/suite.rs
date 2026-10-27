use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::Ordering};

use tsrs_ast::Kind;
use tsrs_compiler::Context;
use tsrs_dts::{BuildOptions, Files, OutputMode, build};

pub struct Case<'a> {
    pub key: &'a str,
    pub entries: &'a [&'a str],
    pub declarations: Option<usize>,
    pub classification: &'a str,
    pub expected: &'a [&'a str],
    pub required_output: &'a [&'a str],
    pub forbidden_output: &'a [&'a str],
    pub occurrences: &'a [(&'a str, usize)],
    pub removed: bool,
    pub nodenext: bool,
    pub files: Option<usize>,
    pub each_contains_any: &'a [&'a str],
    pub repeat: bool,
    pub generated_consumer_only: bool,
    pub shared: bool,
    pub output: Option<&'a str>,
}

impl Case<'_> {
    pub const DEFAULT: Self = Self {
        key: "",
        entries: &["index.d.ts"],
        declarations: None,
        classification: "consumer-contract",
        expected: &[],
        required_output: &[],
        forbidden_output: &[],
        occurrences: &[],
        removed: false,
        nodenext: false,
        files: None,
        each_contains_any: &[],
        repeat: false,
        generated_consumer_only: false,
        shared: false,
        output: None,
    };
}

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

pub fn inventory(cases: &[Case]) {
    let expected: BTreeSet<_> = cases.iter().map(|case| case.key.to_string()).collect();
    assert_eq!(expected.len(), cases.len(), "duplicate rolldown-plugin-dts case");
    let actual: BTreeSet<_> = fs::read_dir(corpus().join("rolldown-plugin-dts"))
        .expect("missing rolldown-plugin-dts suite")
        .filter_map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                Some(format!("rolldown-plugin-dts/{}", entry.file_name().to_str().unwrap()))
            } else {
                assert_eq!(entry.file_name(), "tsconfig.json", "unexpected fixture file");
                None
            }
        })
        .collect();
    assert_eq!(expected, actual, "unregistered/missing rolldown-plugin-dts case");
    for case in cases {
        assert!(
            matches!(
                case.classification,
                "consumer-contract"
                    | "validated"
                    | "generated-invalid"
                    | "missing-environment"
                    | "invalid-input"
                    | "unsupported"
            ),
            "unknown classification: {}",
            case.key
        );
        assert!(!case.entries.is_empty(), "{} has no entries", case.key);
        for entry in case.entries {
            assert!(corpus().join(case.key).join(entry).is_file(), "missing {}/{entry}", case.key);
        }
        if !matches!(case.classification, "missing-environment" | "invalid-input") {
            assert!(
                consumer_path(case).is_file()
                    || !case.required_output.is_empty()
                    || !case.forbidden_output.is_empty()
                    || !case.occurrences.is_empty(),
                "active case has no output or consumer assertions: {}",
                case.key
            );
        }
        if !case.key.starts_with("rolldown-plugin-dts/") {
            assert!(
                case.declarations.is_some(),
                "active case has no pruning assertion: {}",
                case.key
            );
        }
    }
}

fn consumer_path(case: &Case) -> PathBuf {
    corpus().join(case.key).join("consumer.ts")
}

fn read_inputs(directory: &Path) -> Files {
    fn visit(root: &Path, directory: &Path, files: &mut Files) {
        let mut entries: Vec<_> = fs::read_dir(directory).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink(), "symlink fixture requires a dedicated adapter");
            if kind.is_dir() {
                visit(root, &entry.path(), files);
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            // Upstream goldens are retained as evidence, never compiler inputs.
            if name.starts_with("snapshot.")
                || name.starts_with("snapshot-")
                || (directory == root && matches!(name.as_str(), "consumer.ts" | "fixture.json"))
            {
                continue;
            }
            let relative =
                entry.path().strip_prefix(root).unwrap().to_str().unwrap().replace('\\', "/");
            files.insert(
                format!("/project/src/{relative}"),
                fs::read_to_string(entry.path()).unwrap(),
            );
        }
    }
    let mut files = Files::new();
    visit(directory, directory, &mut files);
    files
}

#[path = "dependencies.rs"]
mod dependencies;

fn environment(case: &Case) -> Files {
    match case.key {
        "external_namespace_merge" | "external_namespace_to_named" => {
            Files::from([
                (
                    "/project/node_modules/ext-pkg/package.json".into(),
                    r#"{"name":"ext-pkg","types":"index.d.ts"}"#.into(),
                ),
                (
                    "/project/node_modules/ext-pkg/index.d.ts".into(),
                    "export interface Node { kind: 'node'; value: string }\nexport interface Token { kind: 'token'; text: string }\n".into(),
                ),
            ])
        }
        _ => dependencies::environment(case.key),
    }
}

use crate::filesystem::memory_fs;

fn config(entry: &str, consumer: bool) -> String {
    // Generated checks use fresh roots, without the source fixture's config.
    format!(
        r#"{{"compilerOptions":{{"strict":true,"noEmit":true,"target":"es2022","module":"preserve","moduleResolution":"bundler","types":[]}},"files":["{entry}"{}]}}"#,
        if consumer { ",\"/project/consumer.ts\"" } else { "" }
    )
}

#[derive(Default)]
struct Facts {
    exports: BTreeSet<String>,
    declarations: usize,
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "TypeScript recognizes module extensions with case-sensitive spellings"
)]
fn runtime_specifier(relative: &str) -> String {
    let extension = if relative.ends_with(".mts") {
        ".mjs"
    } else if relative.ends_with(".cts") {
        ".cjs"
    } else {
        ".js"
    };
    format!("./{}", tsrs_core::tspath::change_extension(relative, extension))
}

fn inspect_with_config(
    mut files: Files,
    environment: &Files,
    entry: &str,
    consumer: Option<&str>,
    compiler_config: Option<&str>,
) -> Result<Facts, Vec<String>> {
    // Package declarations support checking but never contribute to the emitted
    // declaration count. Track the exact input paths before adding that context.
    let counted_files: BTreeSet<_> = files.keys().cloned().collect();
    assert!(environment.keys().all(|name| !files.contains_key(name)));
    files.extend(environment.clone());
    let relative = entry.strip_prefix("/project/").unwrap();
    if let Some(consumer) = consumer {
        let specifier = runtime_specifier(relative);
        files.insert("/project/consumer.ts".into(), consumer.replace("__ENTRY__", &specifier));
    }
    let check_config =
        compiler_config.map_or_else(|| config(entry, consumer.is_some()), str::to_owned);
    let mut check_config = tsrs_core::json::unmarshal(&check_config).unwrap();
    let tsrs_core::json::Value::Object(ref mut config) = check_config else {
        panic!("expected config object")
    };
    let Some(tsrs_core::json::Value::Object(options)) = config.get_mut("compilerOptions") else {
        panic!("expected compilerOptions")
    };
    options.insert("noEmit", tsrs_core::json::Value::Bool(true));
    // Match declaration builds when validating emit-only options such as declarationDir.
    options.insert("declaration", tsrs_core::json::Value::Bool(true));
    files.insert("/project/check.json".into(), tsrs_core::json::marshal(&check_config).unwrap());
    let (fs, writes) = memory_fs(&files);
    let host = crate::program::HostProgram::with_config(fs, "/project/check.json", |parsed| {
        // Check/count every emitted declaration, including orphaned outputs.
        parsed.parsed_config.file_names.extend(
            counted_files
                .iter()
                .filter(|name| tsrs_core::tspath::is_declaration_file_name(name))
                .cloned(),
        );
        if consumer.is_some() {
            parsed.parsed_config.file_names.push("/project/consumer.ts".into());
        }
        parsed.parsed_config.file_names.sort();
        parsed.parsed_config.file_names.dedup();
    });
    let program = host.program;
    let context = Context::background();
    let messages = host.diagnostics();
    let mut facts = Facts::default();
    if messages.is_empty() {
        let file = program.get_source_file(entry).expect("missing corpus entry");
        if let Some(symbol) = file.symbol() {
            let mut checker = program.get_type_checker_for_file_exclusive(&context, file);
            facts.exports = checker
                .get_exports_of_module_exported(symbol)
                .iter()
                .map(|s| s.name().to_string())
                .collect();
        }
        for &file in program.source_files() {
            if !counted_files.contains(file.file_name()) || !file.is_declaration_file() {
                continue;
            }
            for &node in file.statements().nodes() {
                facts.declarations += match node.kind() {
                    Kind::InterfaceDeclaration
                    | Kind::TypeAliasDeclaration
                    | Kind::ClassDeclaration
                    | Kind::FunctionDeclaration
                    | Kind::EnumDeclaration
                    | Kind::ModuleDeclaration => 1,
                    Kind::VariableStatement => node
                        .as_variable_statement()
                        .declaration_list()
                        .as_variable_declaration_list()
                        .declarations()
                        .nodes()
                        .len(),
                    _ => 0,
                };
            }
        }
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0, "inspection attempted filesystem writes");
    if messages.is_empty() { Ok(facts) } else { Err(messages) }
}

#[path = "fixture.rs"]
mod fixture;

pub fn run(case: &Case) {
    fixture::run(case, &corpus().join(case.key), consumer_path(case));
}

#[path = "local.rs"]
mod local;

pub fn local_inventory(names: &[&str]) {
    local::inventory(names);
}

pub fn run_local(name: &str) {
    local::run(name);
}

fn check_maps(files: &Files) {
    let map = tsrs_sourcemap::RawSourceMap::from_json(
        &tsrs_core::json::unmarshal(&files["index.d.ts.map"]).unwrap(),
    )
    .unwrap();
    let mappings: Vec<_> = tsrs_sourcemap::decode_mappings(&map.mappings).values().collect();
    for source in ["index.ts", "mod.ts"] {
        assert!(
            mappings.iter().any(|mapping| mapping
                .source_index
                .get()
                .is_some_and(|index| map.sources[index as usize].ends_with(source))),
            "{source}: {map:?}"
        );
    }
    assert!(
        map.sources_content.as_ref().is_some_and(|contents| contents.iter().any(Option::is_some))
    );
    assert!(map.sources.iter().all(|source| !source.ends_with(".d.ts")));
}
