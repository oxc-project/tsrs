// Port of Go's tsoptions/showconfig.go and the generated serializeCompilerOptions / serializeCompilerOptionEnum of
// tsoptions/options_generated.go.

use rustc_hash::FxHashSet;
use tsrs_core::collections::OrderedMap;
use tsrs_core::json::{JsonMap, Value};
use tsrs_core::tspath;
use tsrs_core::{
    CompilerOptions, JsxEmit, ModuleDetectionKind, ModuleKind, ModuleResolutionKind, NewLineKind,
    ScriptTarget, Tristate,
};

use crate::commandlineoption::{CommandLineOption, CompilerOptionsValue};
use crate::enummaps::LIB_MAP;
use crate::gojson::GoJson;
use crate::parsedcommandline::ParsedCommandLine;
use crate::tsconfigparsing::{COMMAND_LINE_COMPILER_OPTIONS_MAP, DEFAULT_INCLUDE_SPEC};

// impliedOption: a compiler option whose effective value derives from other options. `name` and `dependencies` are
// the Go struct field names of core.CompilerOptions.
struct ImpliedOption {
    name: &'static str,
    dependencies: &'static [&'static str],
    compute: fn(&CompilerOptions) -> CompilerOptionsValue,
}

static IMPLIED_OPTIONS: &[ImpliedOption] = &[
    ImpliedOption {
        name: "Module",
        dependencies: &["Target"],
        compute: |options| CompilerOptionsValue::ModuleKind(options.get_emit_module_kind()),
    },
    ImpliedOption {
        name: "ModuleResolution",
        dependencies: &["Module", "Target"],
        compute: |options| {
            CompilerOptionsValue::ModuleResolutionKind(options.get_module_resolution_kind())
        },
    },
    ImpliedOption {
        name: "ModuleDetection",
        dependencies: &["Module", "Target"],
        compute: |options| {
            CompilerOptionsValue::ModuleDetectionKind(options.get_emit_module_detection_kind())
        },
    },
    ImpliedOption {
        name: "IsolatedModules",
        dependencies: &["VerbatimModuleSyntax"],
        compute: |options| CompilerOptionsValue::Bool(options.get_isolated_modules()),
    },
    ImpliedOption {
        name: "PreserveConstEnums",
        dependencies: &["IsolatedModules", "VerbatimModuleSyntax"],
        compute: |options| CompilerOptionsValue::Bool(options.should_preserve_const_enums()),
    },
    ImpliedOption {
        name: "Declaration",
        dependencies: &["Composite"],
        compute: |options| CompilerOptionsValue::Bool(options.get_emit_declarations()),
    },
    ImpliedOption {
        name: "DeclarationMap",
        dependencies: &["Declaration", "Composite"],
        compute: |options| CompilerOptionsValue::Bool(options.get_are_declaration_maps_enabled()),
    },
    ImpliedOption {
        name: "Incremental",
        dependencies: &["Composite"],
        compute: |options| CompilerOptionsValue::Bool(options.is_incremental()),
    },
    ImpliedOption {
        name: "UseDefineForClassFields",
        dependencies: &["Target", "Module"],
        compute: |options| CompilerOptionsValue::Bool(options.get_use_define_for_class_fields()),
    },
    ImpliedOption {
        name: "ResolvePackageJsonExports",
        dependencies: &["ModuleResolution", "Module", "Target"],
        compute: |options| CompilerOptionsValue::Bool(options.get_resolve_package_json_exports()),
    },
    ImpliedOption {
        name: "ResolvePackageJsonImports",
        dependencies: &["ModuleResolution", "ResolvePackageJsonExports", "Module", "Target"],
        compute: |options| CompilerOptionsValue::Bool(options.get_resolve_package_json_imports()),
    },
    ImpliedOption {
        name: "ResolveJsonModule",
        dependencies: &["ModuleResolution", "Module", "Target"],
        compute: |options| CompilerOptionsValue::Bool(options.get_resolve_json_module()),
    },
    ImpliedOption {
        name: "AllowJs",
        dependencies: &["CheckJs"],
        compute: |options| CompilerOptionsValue::Bool(options.get_allow_js()),
    },
    ImpliedOption {
        name: "AllowImportingTsExtensions",
        dependencies: &["RewriteRelativeImportExtensions"],
        compute: |options| CompilerOptionsValue::Bool(options.get_allow_importing_ts_extensions()),
    },
];

// TSConfig: the `--showConfig` output. Empty lists and an unset `compileOnSave` are omitted (`omitzero`).
pub struct TsConfig {
    pub compiler_options: JsonMap,
    pub references: Vec<Value>,
    pub files: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub compile_on_save: Option<bool>,
}

impl TsConfig {
    pub fn to_json(&self) -> Value {
        let strings = |values: &[String]| {
            Value::Array(values.iter().map(|value| Value::String(value.clone())).collect())
        };
        let mut result = JsonMap::new();
        result.insert("compilerOptions", Value::Object(self.compiler_options.clone()));
        if !self.references.is_empty() {
            result.insert("references", Value::Array(self.references.clone()));
        }
        if !self.files.is_empty() {
            result.insert("files", strings(&self.files));
        }
        if !self.include.is_empty() {
            result.insert("include", strings(&self.include));
        }
        if !self.exclude.is_empty() {
            result.insert("exclude", strings(&self.exclude));
        }
        if let Some(compile_on_save) = self.compile_on_save {
            result.insert("compileOnSave", Value::Bool(compile_on_save));
        }
        Value::Object(result)
    }
}

// ConvertToTSConfig generates the tsconfig representation that `--showConfig` prints.
pub fn convert_to_tsconfig(
    config_parse_result: &ParsedCommandLine,
    config_file_name: &str,
) -> TsConfig {
    let config_file_name = if config_file_name.is_empty() {
        tspath::combine_paths(config_parse_result.get_current_directory(), &["tsconfig.json"])
    } else {
        config_file_name.to_string()
    };
    let use_case_sensitive_file_names = config_parse_result.use_case_sensitive_file_names();

    let mut files = Vec::with_capacity(config_parse_result.file_names().len());
    for file_name in config_parse_result.file_names() {
        let file_name = tspath::get_normalized_absolute_path(
            file_name,
            config_parse_result.get_current_directory(),
        );
        files.push(
            match tspath::relative_path_from_file_to_path(
                &config_file_name,
                &file_name,
                use_case_sensitive_file_names,
            ) {
                Some(relative_path) => tspath::ensure_path_is_non_module_name(&relative_path),
                None => file_name,
            },
        );
    }

    let options = config_parse_result.compiler_options().unwrap();
    let mut option_map =
        serialize_compiler_options(&options, &config_file_name, use_case_sensitive_file_names);

    for name in [
        "showConfig",
        "configFile",
        "configFilePath",
        "help",
        "init",
        "listFilesOnly",
        "listEmittedFiles",
        "project",
        "build",
        "version",
    ] {
        option_map.shift_remove(name);
    }

    add_implied_options(&mut option_map, &options);

    let mut config = TsConfig {
        compiler_options: option_map,
        references: Vec::new(),
        files: Vec::new(),
        include: Vec::new(),
        exclude: Vec::new(),
        compile_on_save: None,
    };

    for reference in config_parse_result.project_references() {
        let mut entry = JsonMap::new();
        entry.insert("path", Value::String(reference.original_path.clone()));
        if reference.circular {
            entry.insert("circular", Value::Bool(true));
        }
        config.references.push(Value::Object(entry));
    }

    config.files = files;

    if let Some(config_file) = config_parse_result.config_file {
        let specs = config_file.config_file_specs.lock().unwrap();
        if let Some(specs) = specs.as_ref() {
            config.include = filter_same_as_default_include(&specs.validated_include_specs);
            config.exclude.clone_from(&specs.validated_exclude_specs);
        }
    }

    if config_parse_result.compile_on_save == Some(true) {
        config.compile_on_save = Some(true);
    }

    config
}

// filterSameAsDefaultInclude: nothing when specs is the default include spec ["**/*"].
fn filter_same_as_default_include(specs: &[String]) -> Vec<String> {
    if specs.is_empty() {
        return Vec::new();
    }
    if specs.len() == 1 && specs[0] == DEFAULT_INCLUDE_SPEC {
        return Vec::new();
    }
    specs.to_vec()
}

// getNameOfCompilerOptionValue: the first key of the enum map whose value is `value`.
fn get_name_of_compiler_option_value(
    value: &str,
    enum_map: &OrderedMap<&'static str, CompilerOptionsValue>,
) -> &'static str {
    for (key, enum_value) in enum_map {
        if matches!(enum_value, CompilerOptionsValue::String(enum_value) if enum_value == value) {
            return key;
        }
    }
    ""
}

pub fn serialize_compiler_option_path(
    value: &str,
    config_file_path: &str,
    use_case_sensitive_file_names: bool,
) -> String {
    match tspath::relative_path_from_file_to_path(
        config_file_path,
        value,
        use_case_sensitive_file_names,
    ) {
        Some(relative_path) => tspath::ensure_path_is_non_module_name(&relative_path),
        None => value.to_string(),
    }
}

fn serialize_compiler_option_enum_list(
    values: &[String],
    enum_map: &OrderedMap<&'static str, CompilerOptionsValue>,
) -> Vec<String> {
    values
        .iter()
        .map(|value| {
            let name = get_name_of_compiler_option_value(value, enum_map);
            if name.is_empty() { value.clone() } else { name.to_string() }
        })
        .collect()
}

// addImpliedOptions adds the options that explicitly set options imply (such as moduleResolution implied by module),
// as TypeScript's convertToTSConfig does for its computedOptions.
fn add_implied_options(option_map: &mut JsonMap, options: &CompilerOptions) {
    let provided: FxHashSet<String> = option_map.keys().map(str::to_string).collect();

    let default_options = CompilerOptions::default();

    for entry in IMPLIED_OPTIONS {
        let Some(option_decl) = COMMAND_LINE_COMPILER_OPTIONS_MAP.get(entry.name) else {
            continue;
        };

        if provided.contains(option_decl.name) {
            continue;
        }

        if !any_dependency_provided(entry.dependencies, &provided) {
            continue;
        }

        let implied = (entry.compute)(options);
        let default_value = (entry.compute)(&default_options);

        if implied == default_value {
            continue;
        }

        let Some(serialized) = serialize_implied_option_value(option_decl, &implied) else {
            continue;
        };
        option_map.insert(option_decl.name, serialized);
    }
}

fn any_dependency_provided(dependencies: &[&str], provided: &FxHashSet<String>) -> bool {
    dependencies.iter().any(|dependency| {
        COMMAND_LINE_COMPILER_OPTIONS_MAP
            .get(dependency)
            .is_some_and(|dependency_decl| provided.contains(dependency_decl.name))
    })
}

// serializeImpliedOptionValue: enum options by name, boolean options as JSON booleans. The implied options compute no
// other kind of value.
fn serialize_implied_option_value(
    option_decl: &CommandLineOption,
    value: &CompilerOptionsValue,
) -> Option<Value> {
    if option_decl.enum_map().is_some() {
        let name = serialize_compiler_option_enum(value);
        if !name.is_empty() {
            return Some(Value::String(name.to_string()));
        }
        return None;
    }
    match value {
        CompilerOptionsValue::Bool(value) => Some(Value::Bool(*value)),
        CompilerOptionsValue::Tristate(value) => {
            if value.is_true() {
                Some(Value::Bool(true))
            } else if value.is_false() {
                Some(Value::Bool(false))
            } else {
                None
            }
        }
        _ => None,
    }
}

// options_generated.go serializeCompilerOptions: the options with a declaration whose category is neither
// Command_line_Options nor Output_Formatting, in the field order of core.CompilerOptions.
pub fn serialize_compiler_options(
    options: &CompilerOptions,
    config_file_path: &str,
    use_case_sensitive_file_names: bool,
) -> JsonMap {
    let mut result = JsonMap::with_capacity(32);
    let path = |value: &str| {
        Value::String(serialize_compiler_option_path(
            value,
            config_file_path,
            use_case_sensitive_file_names,
        ))
    };
    let paths = |values: &[String]| {
        Value::Array(
            values
                .iter()
                .map(
                    |value| {
                        if value.is_empty() { Value::String(String::new()) } else { path(value) }
                    },
                )
                .collect(),
        )
    };
    macro_rules! tristate {
        ($($field:ident: $name:literal,)*) => {
            $(if options.$field == Tristate::True || options.$field == Tristate::False {
                result.insert($name, Value::Bool(options.$field == Tristate::True));
            })*
        };
    }
    macro_rules! string {
        ($field:ident: $name:literal) => {
            if !options.$field.is_empty() {
                result.insert($name, Value::String(options.$field.clone()));
            }
        };
    }
    macro_rules! path {
        ($field:ident: $name:literal) => {
            if !options.$field.is_empty() {
                result.insert($name, path(&options.$field));
            }
        };
    }
    macro_rules! paths {
        ($field:ident: $name:literal) => {
            if let Some(values) = &options.$field {
                result.insert($name, paths(values));
            }
        };
    }
    macro_rules! raw {
        ($field:ident: $name:literal) => {
            if let Some(value) = options.$field.to_go_json() {
                result.insert($name, value);
            }
        };
    }
    macro_rules! enumeration {
        ($field:ident: $name:literal, $variant:ident) => {
            if options.$field != Default::default() {
                let value =
                    serialize_compiler_option_enum(&CompilerOptionsValue::$variant(options.$field));
                if !value.is_empty() {
                    result.insert($name, Value::String(value.to_string()));
                }
            }
        };
    }
    tristate! {
        allow_js: "allowJs",
        allow_arbitrary_extensions: "allowArbitraryExtensions",
        allow_importing_ts_extensions: "allowImportingTsExtensions",
        allow_umd_global_access: "allowUmdGlobalAccess",
        allow_unreachable_code: "allowUnreachableCode",
        allow_unused_labels: "allowUnusedLabels",
        assume_changes_only_affect_direct_dependencies: "assumeChangesOnlyAffectDirectDependencies",
        check_js: "checkJs",
    }
    raw!(custom_conditions: "customConditions");
    tristate! {
        composite: "composite",
        emit_declaration_only: "emitDeclarationOnly",
        emit_bom: "emitBOM",
        emit_decorator_metadata: "emitDecoratorMetadata",
        declaration: "declaration",
    }
    path!(declaration_dir: "declarationDir");
    tristate! {
        declaration_map: "declarationMap",
        deduplicate_packages: "deduplicatePackages",
        disable_size_limit: "disableSizeLimit",
        disable_source_of_project_reference_redirect: "disableSourceOfProjectReferenceRedirect",
        disable_solution_searching: "disableSolutionSearching",
        disable_referenced_project_load: "disableReferencedProjectLoad",
        erasable_syntax_only: "erasableSyntaxOnly",
        exact_optional_property_types: "exactOptionalPropertyTypes",
        experimental_decorators: "experimentalDecorators",
        force_consistent_casing_in_file_names: "forceConsistentCasingInFileNames",
        isolated_modules: "isolatedModules",
        isolated_declarations: "isolatedDeclarations",
    }
    string!(ignore_deprecations: "ignoreDeprecations");
    tristate! {
        import_helpers: "importHelpers",
        inline_source_map: "inlineSourceMap",
        inline_sources: "inlineSources",
        incremental: "incremental",
    }
    enumeration!(jsx: "jsx", JsxEmit);
    string!(jsx_factory: "jsxFactory");
    string!(jsx_fragment_factory: "jsxFragmentFactory");
    string!(jsx_import_source: "jsxImportSource");
    if let Some(lib) = &options.lib {
        let lib = serialize_compiler_option_enum_list(lib, &LIB_MAP);
        result.insert("lib", Value::Array(lib.into_iter().map(Value::String).collect()));
    }
    tristate! {
        lib_replacement: "libReplacement",
    }
    string!(map_root: "mapRoot");
    enumeration!(module: "module", ModuleKind);
    enumeration!(module_resolution: "moduleResolution", ModuleResolutionKind);
    raw!(module_suffixes: "moduleSuffixes");
    enumeration!(module_detection: "moduleDetection", ModuleDetectionKind);
    enumeration!(new_line: "newLine", NewLineKind);
    tristate! {
        no_emit: "noEmit",
        no_check: "noCheck",
        no_fallthrough_cases_in_switch: "noFallthroughCasesInSwitch",
        no_implicit_any: "noImplicitAny",
        no_implicit_this: "noImplicitThis",
        no_implicit_returns: "noImplicitReturns",
        no_emit_helpers: "noEmitHelpers",
        no_lib: "noLib",
        no_property_access_from_index_signature: "noPropertyAccessFromIndexSignature",
        no_unchecked_indexed_access: "noUncheckedIndexedAccess",
        no_emit_on_error: "noEmitOnError",
        no_unused_locals: "noUnusedLocals",
        no_unused_parameters: "noUnusedParameters",
        no_resolve: "noResolve",
        no_implicit_override: "noImplicitOverride",
        no_unchecked_side_effect_imports: "noUncheckedSideEffectImports",
    }
    path!(out_dir: "outDir");
    raw!(paths: "paths");
    raw!(plugins: "plugins");
    tristate! {
        preserve_const_enums: "preserveConstEnums",
        preserve_symlinks: "preserveSymlinks",
        resolve_json_module: "resolveJsonModule",
        resolve_package_json_exports: "resolvePackageJsonExports",
        resolve_package_json_imports: "resolvePackageJsonImports",
        remove_comments: "removeComments",
        rewrite_relative_import_extensions: "rewriteRelativeImportExtensions",
    }
    string!(react_namespace: "reactNamespace");
    path!(root_dir: "rootDir");
    paths!(root_dirs: "rootDirs");
    tristate! {
        skip_lib_check: "skipLibCheck",
        stable_type_ordering: "stableTypeOrdering",
        strict: "strict",
        strict_bind_call_apply: "strictBindCallApply",
        strict_builtin_iterator_return: "strictBuiltinIteratorReturn",
        strict_function_types: "strictFunctionTypes",
        strict_null_checks: "strictNullChecks",
        strict_property_initialization: "strictPropertyInitialization",
        strip_internal: "stripInternal",
        skip_default_lib_check: "skipDefaultLibCheck",
        source_map: "sourceMap",
    }
    string!(source_root: "sourceRoot");
    enumeration!(target: "target", ScriptTarget);
    tristate! {
        trace_resolution: "traceResolution",
    }
    path!(ts_build_info_file: "tsBuildInfoFile");
    paths!(type_roots: "typeRoots");
    raw!(types: "types");
    tristate! {
        use_define_for_class_fields: "useDefineForClassFields",
        use_unknown_in_catch_variables: "useUnknownInCatchVariables",
        verbatim_module_syntax: "verbatimModuleSyntax",
    }
    raw!(max_node_module_js_depth: "maxNodeModuleJsDepth");
    tristate! {
        allow_synthetic_default_imports: "allowSyntheticDefaultImports",
        always_strict: "alwaysStrict",
    }
    path!(base_url: "baseUrl");
    tristate! {
        downlevel_iteration: "downlevelIteration",
        es_module_interop: "esModuleInterop",
    }
    path!(out_file: "outFile");
    tristate! {
        diagnostics: "diagnostics",
        extended_diagnostics: "extendedDiagnostics",
    }
    path!(generate_cpu_profile: "generateCpuProfile");
    path!(generate_trace: "generateTrace");
    tristate! {
        explain_files: "explainFiles",
    }
    result
}

// options_generated.go serializeCompilerOptionEnum: the first name of an enum value in its option's enum map, or ""
// for a value with no name.
pub fn serialize_compiler_option_enum(value: &CompilerOptionsValue) -> &'static str {
    match *value {
        CompilerOptionsValue::JsxEmit(value) => match value {
            JsxEmit::PRESERVE => "preserve",
            JsxEmit::REACT_NATIVE => "react-native",
            JsxEmit::REACT_JSX => "react-jsx",
            JsxEmit::REACT_JSX_DEV => "react-jsxdev",
            JsxEmit::REACT => "react",
            _ => "",
        },
        CompilerOptionsValue::ModuleKind(value) => match value {
            ModuleKind::COMMON_JS => "commonjs",
            ModuleKind::AMD => "amd",
            ModuleKind::SYSTEM => "system",
            ModuleKind::UMD => "umd",
            ModuleKind::ES2015 => "es6",
            ModuleKind::ES2020 => "es2020",
            ModuleKind::ES2022 => "es2022",
            ModuleKind::ES_NEXT => "esnext",
            ModuleKind::NODE16 => "node16",
            ModuleKind::NODE18 => "node18",
            ModuleKind::NODE20 => "node20",
            ModuleKind::NODE_NEXT => "nodenext",
            ModuleKind::PRESERVE => "preserve",
            _ => "",
        },
        CompilerOptionsValue::ModuleResolutionKind(value) => match value {
            ModuleResolutionKind::NODE16 => "node16",
            ModuleResolutionKind::NODE_NEXT => "nodenext",
            ModuleResolutionKind::BUNDLER => "bundler",
            ModuleResolutionKind::CLASSIC => "classic",
            ModuleResolutionKind::NODE10 => "node",
            _ => "",
        },
        CompilerOptionsValue::ModuleDetectionKind(value) => match value {
            ModuleDetectionKind::Auto => "auto",
            ModuleDetectionKind::Legacy => "legacy",
            ModuleDetectionKind::Force => "force",
            ModuleDetectionKind::None => "",
        },
        CompilerOptionsValue::NewLineKind(value) => match value {
            NewLineKind::CRLF => "crlf",
            NewLineKind::LF => "lf",
            NewLineKind::None => "",
        },
        CompilerOptionsValue::ScriptTarget(value) => match value {
            ScriptTarget::ES5 => "es5",
            ScriptTarget::ES2015 => "es6",
            ScriptTarget::ES2016 => "es2016",
            ScriptTarget::ES2017 => "es2017",
            ScriptTarget::ES2018 => "es2018",
            ScriptTarget::ES2019 => "es2019",
            ScriptTarget::ES2020 => "es2020",
            ScriptTarget::ES2021 => "es2021",
            ScriptTarget::ES2022 => "es2022",
            ScriptTarget::ES2023 => "es2023",
            ScriptTarget::ES2024 => "es2024",
            ScriptTarget::ES2025 => "es2025",
            ScriptTarget::ES2026 => "es2026",
            ScriptTarget::ES_NEXT => "esnext",
            _ => "",
        },
        _ => "",
    }
}
