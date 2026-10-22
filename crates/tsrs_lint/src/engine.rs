//! Program construction and headless lint execution.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use tsrs_ast::{Diagnostic as TsDiagnostic, SourceFile};
use tsrs_compiler::{Context, Program, ProgramOptions};
use tsrs_core::json::Value;
use tsrs_core::tspath;
use tsrs_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, P, ScriptTarget, Tristate,
};
use tsrs_tsoptions::{ParseConfigHost, ParsedCommandLine};
use tsrs_vfs::{FS, bundled, cachedvfs, osvfs, wrapvfs};

use crate::payload::{ConfiguredRule, Payload};
use crate::protocol::{self, Diagnostic, MessageType, Range, RuleMessage};
use crate::rule::{RuleContext, visit_lint_nodes};

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub fix: bool,
    pub fix_suggestions: bool,
    pub debug_timings: bool,
    pub suppress_program_diagnostics: bool,
}

#[derive(Default)]
struct RequestRegions {
    workers: Vec<tsrs_core::arena::Region>,
    loaders: Arc<tsrs_compiler::ProgramRegions>,
}

struct ConfigHost {
    fs: Arc<dyn FS>,
    cwd: String,
}

impl ParseConfigHost for ConfigHost {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &str {
        &self.cwd
    }
}

pub fn run(
    payload: &Payload,
    options: Options,
    cwd: &str,
    writer: &mut dyn Write,
) -> Result<(), String> {
    let overrides = Arc::new(payload.source_overrides.clone());
    let read_overrides = Arc::clone(&overrides);
    let directory_overrides = Arc::clone(&overrides);
    let entries_overrides = Arc::clone(&overrides);
    let stat_overrides = Arc::clone(&overrides);
    let realpath_overrides = Arc::clone(&overrides);
    let fs = wrapvfs::wrap(
        osvfs::fs(),
        wrapvfs::Replacements {
            file_exists: Some(Box::new(move |path| {
                overrides.contains_key(path) || osvfs::fs().file_exists(path)
            })),
            read_file: Some(Box::new(move |path| {
                read_overrides.get(path).cloned().or_else(|| osvfs::fs().read_file(path))
            })),
            directory_exists: Some(Box::new(move |path| {
                if !directory_overrides.is_empty() {
                    let prefix = format!("{}/", tspath::normalize_path(path).trim_end_matches('/'));
                    if directory_overrides.keys().any(|file| file.starts_with(&prefix)) {
                        return true;
                    }
                }
                osvfs::fs().directory_exists(path)
            })),
            get_accessible_entries: Some(Box::new(move |path| {
                let mut entries = osvfs::fs().get_accessible_entries(path);
                if !entries_overrides.is_empty() {
                    let prefix = format!("{}/", tspath::normalize_path(path).trim_end_matches('/'));
                    for file in entries_overrides.keys() {
                        if let Some(relative) = file.strip_prefix(&prefix) {
                            if let Some((directory, _)) = relative.split_once('/') {
                                entries.directories.push(directory.into());
                            } else {
                                entries.files.push(relative.into());
                            }
                        }
                    }
                    entries.files.sort_unstable();
                    entries.files.dedup();
                    entries.directories.sort_unstable();
                    entries.directories.dedup();
                }
                entries
            })),
            stat: Some(Box::new(move |path| {
                stat_overrides.get(path).map_or_else(
                    || osvfs::fs().stat(path),
                    |source| {
                        Some(tsrs_vfs::FileInfo {
                            name: path.into(),
                            size: i64::try_from(source.len()).unwrap(),
                            mode: tsrs_vfs::FileMode::None,
                            mod_time: None,
                        })
                    },
                )
            })),
            realpath: Some(Box::new(move |path| {
                if realpath_overrides.contains_key(path) {
                    path.into()
                } else {
                    osvfs::fs().realpath(path)
                }
            })),
            ..Default::default()
        },
    );
    let fs: Arc<dyn FS> = Arc::new(bundled::wrap_fs(cachedvfs::from(Arc::new(fs))));
    let host =
        ConfigHostOwner(Box::leak(Box::new(ConfigHost { fs: Arc::clone(&fs), cwd: cwd.into() })));
    let region = tsrs_core::arena::Region::new_scratch(1 << 20);
    // Shared caches can receive worker allocations, so every worker region outlives every request program.
    let mut regions = RequestRegions::default();
    let _scope = region.enter();
    run_in_region(payload, options, cwd, writer, fs, host.0, &mut regions)
}

struct ConfigHostOwner(&'static ConfigHost);

impl Drop for ConfigHostOwner {
    fn drop(&mut self) {
        // SAFETY: run's programs and request region have been released before this owned host is dropped.
        unsafe {
            drop(Box::from_raw(std::ptr::from_ref(self.0).cast_mut()));
        }
    }
}

struct ProgramOwner(&'static Program);

impl Drop for ProgramOwner {
    fn drop(&mut self) {
        // SAFETY: each program is created once here, is never shared with another version, and all checker handles have been dropped.
        unsafe {
            tsrs_compiler::free_unshared_program(self.0);
        }
    }
}

fn run_in_region(
    payload: &Payload,
    options: Options,
    cwd: &str,
    writer: &mut dyn Write,
    fs: Arc<dyn FS>,
    host: &'static ConfigHost,
    regions: &mut RequestRegions,
) -> Result<(), String> {
    let mut programs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unmatched = Vec::new();
    let mut file_configs = BTreeMap::new();
    let mut timings: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for config in &payload.configs {
        for path in &config.file_paths {
            file_configs.insert(tspath::normalize_slashes(path), config.rules.as_slice());
        }
    }
    for rules in file_configs.values() {
        for rule in *rules {
            crate::registry::require_implemented(&rule.name)?;
        }
    }
    let mut config_cache = FxHashMap::default();
    for path in file_configs.keys() {
        let resolved = tspath::get_normalized_absolute_path(path, cwd);
        if let Some(config_path) = find_tsconfig(&resolved, host, &mut config_cache) {
            programs.entry(config_path).or_default().push(path.clone());
        } else {
            unmatched.push(path.clone());
        }
    }
    drop(config_cache);
    for (config_path, files) in programs {
        let (config, diagnostics) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
            &config_path,
            None,
            None,
            host,
            None,
        );
        if !diagnostics.is_empty() {
            for diagnostic in diagnostics {
                report_config_error(diagnostic, &config_path, false, writer)?;
            }
            continue;
        }
        let config = config.ok_or_else(|| format!("couldn't read tsconfig at {config_path}"))?;
        if !config.errors.is_empty() && !options.suppress_program_diagnostics {
            for diagnostic in &config.errors {
                report_config_error(*diagnostic, &config_path, false, writer)?;
            }
            continue;
        }
        let program = create_program(
            config,
            Arc::clone(&fs),
            &tspath::get_directory_path(&config_path),
            true,
            Arc::clone(&regions.loaders),
        );
        let program_ref = program.0;
        let diagnostics = program_ref.get_program_diagnostics();
        if !diagnostics.is_empty() && !options.suppress_program_diagnostics {
            for diagnostic in diagnostics {
                report_config_error(diagnostic, &config_path, true, writer)?;
            }
            continue;
        }
        run_program(
            program_ref,
            &files,
            payload,
            options,
            writer,
            &file_configs,
            &mut timings,
            &mut regions.workers,
        )?;
    }
    if !unmatched.is_empty() {
        let config = tsrs_tsoptions::new_parsed_command_line(
            P::new(CompilerOptions {
                allow_js: Tristate::True,
                module: ModuleKind::ESNext,
                module_resolution: ModuleResolutionKind::Bundler,
                target: ScriptTarget::ES2022,
                jsx: JsxEmit::ReactJSX,
                allow_importing_ts_extensions: Tristate::True,
                strict_null_checks: Tristate::True,
                strict_function_types: Tristate::True,
                source_map: Tristate::True,
                es_module_interop: Tristate::True,
                allow_non_ts_extensions: Tristate::True,
                resolve_json_module: Tristate::True,
                ..Default::default()
            }),
            unmatched.clone(),
            Vec::new(),
            tspath::ComparePathsOptions {
                current_directory: cwd.into(),
                use_case_sensitive_file_names: fs.use_case_sensitive_file_names(),
            },
        );
        let program = create_program(config, fs, cwd, false, Arc::clone(&regions.loaders));
        run_program(
            program.0,
            &unmatched,
            payload,
            options,
            writer,
            &file_configs,
            &mut timings,
            &mut regions.workers,
        )?;
    }
    if options.debug_timings {
        let mut records = timings.into_iter().collect::<Vec<_>>();
        records.sort_by(|(left_name, (left_duration, _)), (right_name, (right_duration, _))| {
            right_duration.cmp(left_duration).then_with(|| left_name.cmp(right_name))
        });
        protocol::write_message(
            writer,
            MessageType::Timing,
            &protocol::object([(
                "rules",
                Value::Array(
                    records
                        .into_iter()
                        .map(|(name, (duration, calls))| {
                            protocol::object([
                                ("rule_name", Value::String(name)),
                                ("duration", Value::Number(duration as f64)),
                                ("calls", Value::Number(calls as f64)),
                            ])
                        })
                        .collect(),
                ),
            )]),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn create_program(
    mut config: ParsedCommandLine,
    fs: Arc<dyn FS>,
    cwd: &str,
    references: bool,
    loader_regions: Arc<tsrs_compiler::ProgramRegions>,
) -> ProgramOwner {
    config.parsed_config.compiler_options = Some(P::new(CompilerOptions {
        checkers: Some(1),
        ..(*config.compiler_options().unwrap()).clone()
    }));
    let host = tsrs_compiler::new_compiler_host(cwd, fs, &bundled::lib_path(), None, None, None);
    let host = Arc::new(crate::compiler_host::LintCompilerHost(host));
    let mut options = ProgramOptions::new(P::new(config), host);
    options.use_source_of_project_reference = references;
    options.regions = Some(loader_regions);
    ProgramOwner(tsrs_compiler::new_program(options))
}

fn run_program(
    program: &'static Program,
    files: &[String],
    payload: &Payload,
    options: Options,
    writer: &mut dyn Write,
    file_configs: &BTreeMap<String, &[ConfiguredRule]>,
    timings: &mut BTreeMap<String, (u64, u64)>,
    worker_regions: &mut Vec<tsrs_core::arena::Region>,
) -> Result<(), String> {
    program.bind_source_files();
    let lint_files = files
        .iter()
        .filter(|name| {
            payload.report_semantic
                || file_configs.get(*name).is_some_and(|rules| !rules.is_empty())
        })
        .count();
    let workers = if lint_files > 1 && !tsrs_core::NO_THREADS {
        let configured =
            std::env::var("RAYON_NUM_THREADS").ok().and_then(|value| value.parse::<usize>().ok());
        lint_worker_count(
            lint_files,
            configured,
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        )
    } else {
        1
    };
    if workers > 1 {
        return run_program_parallel(
            program,
            files,
            payload,
            options,
            writer,
            file_configs,
            timings,
            worker_regions,
            workers,
        );
    }
    let context = Context::default();
    for name in files {
        let file = program
            .get_source_file(name)
            .ok_or_else(|| format!("Expected file '{name}' to be in program"))?;
        if payload.report_syntactic {
            for diagnostic in program.get_syntactic_diagnostics(&context, Some(file)) {
                report_type_error(diagnostic, file, writer)?;
            }
        }
        if payload.report_semantic {
            for diagnostic in program.get_semantic_diagnostics(&context, Some(file)) {
                report_type_error(diagnostic, file, writer)?;
            }
        }
        let configured = file_configs.get(name).copied().unwrap_or_default();
        if configured.is_empty() {
            continue;
        }
        let mut checker = program.get_type_checker_for_file_exclusive(&context, file);
        let diagnostics = lint_file(program, file, configured, options, &mut checker, timings)?;
        drop(checker);
        for diagnostic in &diagnostics {
            emit(diagnostic, writer)?;
        }
    }
    Ok(())
}

fn lint_worker_count(files: usize, configured: Option<usize>, parallelism: usize) -> usize {
    configured.filter(|&count| count > 0).unwrap_or(parallelism).clamp(1, 256).min(files)
}

fn lint_file(
    program: &'static Program,
    file: P<SourceFile>,
    configured: &[ConfiguredRule],
    options: Options,
    checker: &mut tsrs_checker::Checker,
    timings: &mut BTreeMap<String, (u64, u64)>,
) -> Result<Vec<Diagnostic>, String> {
    let mut diagnostics = Vec::new();
    let mut rules = Vec::with_capacity(configured.len());
    for config in configured {
        let start = options.debug_timings.then(Instant::now);
        let mut rule = crate::registry::create_rule(config)?;
        rule.initialize(&mut RuleContext {
            source_file: file,
            program,
            checker,
            rule_name: &config.name,
            options,
            diagnostics: &mut diagnostics,
        });
        if let Some(start) = start {
            let entry = timings.entry(config.name.clone()).or_default();
            entry.0 = entry
                .0
                .saturating_add(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
            entry.1 += 1;
        }
        rules.push((config.name.as_str(), rule));
    }
    let mut listeners = FxHashMap::default();
    visit_lint_nodes(file, &mut |listener, node| {
        let active: &mut Vec<usize> = listeners.entry(listener).or_insert_with(|| {
            rules
                .iter()
                .enumerate()
                .filter_map(|(index, (_, rule))| rule.listens_to(listener).then_some(index))
                .collect()
        });
        for &index in active.iter() {
            let (name, rule) = &mut rules[index];
            let start = options.debug_timings.then(Instant::now);
            let mut context = RuleContext {
                source_file: file,
                program,
                checker,
                rule_name: name,
                options,
                diagnostics: &mut diagnostics,
            };
            rule.visit(listener, node, &mut context);
            if let Some(start) = start {
                let entry = timings.entry((*name).into()).or_default();
                entry.0 = entry
                    .0
                    .saturating_add(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
                entry.1 += 1;
            }
        }
    });
    Ok(diagnostics)
}

fn run_program_parallel(
    program: &'static Program,
    files: &[String],
    payload: &Payload,
    options: Options,
    writer: &mut dyn Write,
    file_configs: &BTreeMap<String, &[ConfiguredRule]>,
    timings: &mut BTreeMap<String, (u64, u64)>,
    worker_regions: &mut Vec<tsrs_core::arena::Region>,
    workers: usize,
) -> Result<(), String> {
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for chunk in files.chunks(files.len().div_ceil(workers)) {
            let region = tsrs_core::arena::Region::new_scratch(1 << 20);
            worker_regions.push(region.clone());
            handles.push(
                std::thread::Builder::new()
                    .name("tsrs-lint-worker".into())
                    .stack_size(512 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        let _scope = region.enter();
                        let context = Context::default();
                        let mut checker = None;
                        let mut output = Vec::new();
                        let mut timings = BTreeMap::new();
                        for name in chunk {
                            let file = program.get_source_file(name).ok_or_else(|| {
                                format!("Expected file '{name}' to be in program")
                            })?;
                            if payload.report_syntactic {
                                for diagnostic in
                                    program.get_syntactic_diagnostics(&context, Some(file))
                                {
                                    report_type_error(diagnostic, file, &mut output)?;
                                }
                            }
                            if payload.report_semantic {
                                let checker = checker
                                    .get_or_insert_with(|| tsrs_checker::new_checker(program));
                                let diagnostics = program
                                    .get_semantic_diagnostics_with_checker(&context, checker, file);
                                for diagnostic in
                                    tsrs_compiler::sort_and_deduplicate_diagnostics(&diagnostics)
                                {
                                    report_type_error(diagnostic, file, &mut output)?;
                                }
                            }
                            let configured = file_configs.get(name).copied().unwrap_or_default();
                            if configured.is_empty() {
                                continue;
                            }
                            let checker =
                                checker.get_or_insert_with(|| tsrs_checker::new_checker(program));
                            for diagnostic in lint_file(
                                program,
                                file,
                                configured,
                                options,
                                checker,
                                &mut timings,
                            )? {
                                emit(&diagnostic, &mut output)?;
                            }
                        }
                        Ok::<_, String>((output, timings))
                    })
                    .map_err(|error| format!("could not start lint worker: {error}"))?,
            );
        }
        handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Result<Vec<_>, String>>()
    })?;
    for (output, local_timings) in results {
        writer.write_all(&output).map_err(|error| error.to_string())?;
        for (name, (duration, calls)) in local_timings {
            let entry = timings.entry(name).or_default();
            entry.0 = entry.0.saturating_add(duration);
            entry.1 += calls;
        }
    }
    Ok(())
}

fn report_type_error(
    diagnostic: P<TsDiagnostic>,
    file: P<SourceFile>,
    writer: &mut dyn Write,
) -> Result<(), String> {
    if diagnostic.file().is_none_or(|f| f.file_name() != file.file_name()) {
        return Ok(());
    }
    emit(
        &Diagnostic {
            range: diagnostic_range(diagnostic),
            message: RuleMessage {
                id: format!("TS{}", diagnostic.code()),
                description: diagnostic.localize(),
                help: String::new(),
            },
            file_path: Some(file.file_name().into()),
            ..Default::default()
        },
        writer,
    )
}

fn report_config_error(
    diagnostic: P<TsDiagnostic>,
    config_path: &str,
    program_diagnostic: bool,
    writer: &mut dyn Write,
) -> Result<(), String> {
    let mut help = diagnostic.localize();
    if program_diagnostic && help.contains("Please remove it from your configuration.") {
        help.push_str(
            "\nSee https://github.com/oxc-project/tsgolint/issues/351 for more information.",
        );
    }
    emit(
        &Diagnostic {
            range: diagnostic_range(diagnostic),
            message: RuleMessage {
                id: "tsconfig-error".into(),
                description: "Invalid tsconfig".into(),
                help,
            },
            file_path: Some(
                diagnostic
                    .file()
                    .map_or_else(|| config_path.to_owned(), |f| f.file_name().to_owned()),
            ),
            ..Default::default()
        },
        writer,
    )
}

fn diagnostic_range(diagnostic: P<TsDiagnostic>) -> Option<Range> {
    Some(Range { pos: diagnostic.pos()?, end: diagnostic.end()? })
}

pub(crate) fn emit(diagnostic: &Diagnostic, writer: &mut dyn Write) -> Result<(), String> {
    protocol::write_message(writer, MessageType::Diagnostic, &diagnostic.to_json())
        .map_err(|e| e.to_string())
}

fn find_tsconfig(
    file: &str,
    host: &'static ConfigHost,
    configs: &mut FxHashMap<tspath::Path, Option<ParsedCommandLine>>,
) -> Option<String> {
    let mut next = compute_config_file_name(file, false, host);
    let mut visited = FxHashSet::default();
    let file_path = tspath::to_path(file, &host.cwd, host.fs.use_case_sensitive_file_names());
    while let Some(config_path) = next {
        let mut queue = VecDeque::from([config_path.clone()]);
        let mut disable_solution_searching = false;
        while let Some(path) = queue.pop_front() {
            if !visited.insert(path.clone()) {
                continue;
            }
            let config_path_key =
                tspath::to_path(&path, &host.cwd, host.fs.use_case_sensitive_file_names());
            let config = configs.entry(config_path_key).or_insert_with(|| {
                let (config, diagnostics) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
                    &path, None, None, host, None,
                );
                if !diagnostics.is_empty() {
                    return None;
                }
                config.filter(|config| {
                    config.config_file.is_none_or(|file| file.source_file.diagnostics().is_empty())
                })
            });
            if let Some(config) = config {
                if path == config_path {
                    disable_solution_searching = config
                        .compiler_options()
                        .is_some_and(|options| options.disable_solution_searching.is_true());
                }
                if config.file_names().is_empty() {
                    queue.extend(config.resolved_project_reference_paths().iter().cloned());
                    continue;
                }
                if config.compiler_options().is_some_and(|options| options.composite.is_true())
                    && !config.possibly_matches_file_name(file)
                {
                    continue;
                }
                if config.file_names_by_path().contains_key(&file_path) {
                    return Some(path);
                }
                queue.extend(config.resolved_project_reference_paths().iter().cloned());
            }
        }
        if disable_solution_searching {
            return None;
        }
        next = compute_config_file_name(&config_path, true, host);
    }
    None
}

fn compute_config_file_name(file: &str, skip_directory: bool, host: &ConfigHost) -> Option<String> {
    let mut directory = tspath::get_directory_path(file);
    let mut skip_tsconfig = skip_directory;
    let mut skip_jsconfig = skip_directory && !file.ends_with("/tsconfig.json");
    loop {
        for (name, skip) in [("tsconfig.json", skip_tsconfig), ("jsconfig.json", skip_jsconfig)] {
            if !skip {
                let path = tspath::combine_paths(&directory, &[name]);
                if host.fs.file_exists(&path) {
                    return Some(path);
                }
            }
        }
        if directory.ends_with("/node_modules") {
            return None;
        }
        let parent = tspath::get_directory_path(&directory);
        if parent == directory {
            return None;
        }
        directory = parent;
        skip_tsconfig = false;
        skip_jsconfig = false;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn worker_counts_follow_available_parallelism_unless_overridden() {
        for count in [1, 4, 8, 12] {
            assert_eq!(super::lint_worker_count(100, Some(count), 12), count);
        }
        assert_eq!(super::lint_worker_count(100, None, 12), 12);
        assert_eq!(super::lint_worker_count(100, Some(0), 12), 12);
        assert_eq!(super::lint_worker_count(100, None, 2), 2);
        assert_eq!(super::lint_worker_count(3, None, 12), 3);
        assert_eq!(super::lint_worker_count(1000, None, 512), 256);
        assert_eq!(super::lint_worker_count(3, Some(12), 12), 3);
        assert_eq!(super::lint_worker_count(1000, Some(usize::MAX), 12), 256);
    }

    use std::sync::Mutex;

    use tsrs_vfs::vfstest;

    use super::*;

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
                files.push((
                    format!("/project/{directory}/file{index}.ts"),
                    "export {};".to_owned(),
                ));
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
        let host = ConfigHostOwner(Box::leak(Box::new(ConfigHost {
            fs: Arc::new(fs),
            cwd: "/project".into(),
        })));
        let mut configs = FxHashMap::default();
        for directory in ["a", "b"] {
            for index in 0..100 {
                assert_eq!(
                    find_tsconfig(
                        &format!("/project/{directory}/file{index}.ts"),
                        host.0,
                        &mut configs,
                    ),
                    Some(format!("/project/{directory}/tsconfig.json")),
                );
            }
        }
        assert_eq!(find_tsconfig("/project/loose.ts", host.0, &mut configs), None);
        assert_eq!(find_tsconfig("/project/loose.ts", host.0, &mut configs), None);
        for name in ["first", "second"] {
            assert_eq!(
                find_tsconfig(&format!("/project/invalid/{name}.ts"), host.0, &mut configs),
                None,
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
        let host = ConfigHostOwner(Box::leak(Box::new(ConfigHost {
            fs: Arc::new(fs),
            cwd: "/project".into(),
        })));
        let mut configs = FxHashMap::default();
        for directory in ["/PROJECT", "/project"] {
            assert_eq!(
                find_tsconfig(&format!("{directory}/src/A.ts"), host.0, &mut configs),
                Some(format!("{directory}/tsconfig.json")),
            );
        }
    }
}
