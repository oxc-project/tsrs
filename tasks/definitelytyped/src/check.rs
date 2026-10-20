use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tsrs_ast::{
    Kind, Node, NodeFactory, NodeVisitorHooks, SourceFileParseOptions, TokenFlags, new_node_visitor,
};
use tsrs_core::{P, ScriptKind, tspath::Path as TsPath};

use crate::api::{Api, Result, SourceAst};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub kind: String,
    pub config: String,
    pub file: String,
    pub start: i64,
    pub end: i64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub config: String,
    pub file: String,
    pub line: usize,
    pub expected: String,
    pub actual: String,
    pub matched: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilerOutcome {
    pub issues: Vec<Issue>,
    pub assertions: Vec<Assertion>,
    pub source_files: Vec<String>,
    pub loaded_file_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageResult {
    pub package: String,
    pub tsgo: CompilerOutcome,
    pub tsrs: CompilerOutcome,
    pub package_issues: Vec<String>,
}

pub fn check_package(
    dt: &Path,
    package: &str,
    tsgo: &Path,
    tsrs: &Path,
    stage: &Path,
    reference_out: &Path,
) -> PackageResult {
    let dir = dt.join("types").join(package);
    let (configs, mut package_issues) = package_config(&dir);
    check_expected_files(&dir, &mut package_issues);
    let run = |binary: &Path| match run_compiler(dt, &dir, &configs, binary) {
        Ok(outcome) => outcome,
        Err(error) => {
            CompilerOutcome { error: Some(error.to_string()), ..CompilerOutcome::default() }
        }
    };
    let _ = std::fs::write(stage, "tsgo");
    let reference = run(tsgo);
    let _ = std::fs::write(reference_out, serde_json::to_vec(&reference).unwrap_or_default());
    let _ = std::fs::write(stage, "tsrs");
    let candidate = run(tsrs);
    PackageResult { package: package.to_string(), tsgo: reference, tsrs: candidate, package_issues }
}

fn package_config(dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut issues = Vec::new();
    if !dir.join("index.d.ts").is_file() {
        issues.push("missing index.d.ts".to_string());
    }
    let manifest = dir.join("package.json");
    let data = match std::fs::read_to_string(&manifest) {
        Ok(data) => data,
        Err(error) => {
            issues.push(format!("cannot read {}: {error}", manifest.display()));
            return (vec!["tsconfig.json".to_string()], issues);
        }
    };
    let package_json: Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(error) => {
            issues.push(format!("invalid package.json: {error}"));
            return (vec!["tsconfig.json".to_string()], issues);
        }
    };
    let configs = match package_json.get("tsconfigs") {
        Some(Value::Array(items)) if !items.is_empty() => {
            items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect::<Vec<_>>()
        }
        Some(_) => {
            issues.push("package.json tsconfigs must be a nonempty string array".to_string());
            vec!["tsconfig.json".to_string()]
        }
        None => vec!["tsconfig.json".to_string()],
    };
    for config in &configs {
        if !dir.join(config).is_file() {
            issues.push(format!("missing {config}"));
        }
    }
    (configs, issues)
}

fn check_expected_files(dir: &Path, issues: &mut Vec<String>) {
    if dir.join("tslint.json").exists() {
        issues.push("tslint.json is no longer permitted".to_string());
    }
    if dir.join("OTHER_FILES.txt").exists() {
        issues.push("OTHER_FILES.txt is no longer permitted".to_string());
    }
    let mut expected = vec![
        "*".to_string(),
        "!**/*.d.ts".to_string(),
        "!**/*.d.cts".to_string(),
        "!**/*.d.mts".to_string(),
        "!**/*.d.*.ts".to_string(),
    ];
    if dir.parent().and_then(|parent| parent.file_name()).is_some_and(|name| name == "types") {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut versions = entries
                .filter_map(std::result::Result::ok)
                .filter_map(|entry| {
                    entry
                        .file_type()
                        .ok()
                        .filter(std::fs::FileType::is_dir)
                        .map(|_| entry.file_name().to_string_lossy().into_owned())
                })
                .filter(|name| version_dir(name))
                .collect::<Vec<_>>();
            versions.sort();
            expected.extend(versions.into_iter().map(|name| format!("/{name}/")));
        }
    } else if let Some(name) = dir.file_name().and_then(|name| name.to_str()) {
        let parent = dir.parent().unwrap();
        match std::fs::read_to_string(parent.join(".npmignore")) {
            Ok(contents) if contents.lines().any(|line| line.trim() == format!("/{name}/")) => {}
            _ => issues.push(format!("parent .npmignore must contain /{name}/")),
        }
    }
    match std::fs::read_to_string(dir.join(".npmignore")) {
        Ok(contents)
            if contents.trim().lines().map(str::trim).eq(expected.iter().map(String::as_str)) => {}
        _ => {
            issues.push(".npmignore does not match the expected declaration-file list".to_string());
        }
    }
}

fn version_dir(name: &str) -> bool {
    let Some(version) = name.strip_prefix('v') else { return false };
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or("");
    let minor = parts.next();
    !major.is_empty()
        && major.bytes().all(|b| b.is_ascii_digit())
        && minor.is_none_or(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
}

fn run_compiler(
    dt: &Path,
    dir: &Path,
    configs: &[String],
    binary: &Path,
) -> Result<CompilerOutcome> {
    let mut api = Api::start(binary, dir)?;
    let config_paths = configs
        .iter()
        .map(|config| dir.join(config).to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let (snapshot, projects) = api.snapshot(&config_paths)?;
    let mut outcome = CompilerOutcome::default();
    for (config, path) in configs.iter().zip(&config_paths) {
        let project = projects.iter().find(|p| p["configFileName"].as_str() == Some(path));
        let Some(project) = project else {
            outcome.issues.push(Issue {
                kind: "config".to_string(),
                config: config.clone(),
                file: config.clone(),
                start: -1,
                end: -1,
                message: "could not open configured project".to_string(),
            });
            continue;
        };
        let project_id = project["id"].as_str().ok_or("project response has no ID")?;
        validate_tsconfig(dt, config, &project["parsedCommandLine"]["raw"], &mut outcome);
        let params = json!({ "snapshot": snapshot, "project": project_id });
        let mut diagnostics = Vec::new();
        for method in
            ["getConfigFileParsingDiagnostics", "getProgramDiagnostics", "getGlobalDiagnostics"]
        {
            diagnostics.extend(
                api.request(method, &params)?
                    .as_array()
                    .ok_or("invalid diagnostics response")?
                    .clone(),
            );
        }
        let file_names = api.request("getSourceFileNames", &params)?;
        let file_names = file_names.as_array().ok_or("invalid source file names")?;
        outcome.loaded_file_count += file_names.len();
        let loaded_names = file_names
            .iter()
            .filter_map(Value::as_str)
            .map(|name| name.replace('\\', "/"))
            .collect::<BTreeSet<_>>();
        if let Some(roots) = project["parsedCommandLine"]["fileNames"].as_array() {
            for root in roots.iter().filter_map(Value::as_str) {
                if !loaded_names.contains(&root.replace('\\', "/")) {
                    outcome.issues.push(Issue {
                        kind: "coverage".to_string(),
                        config: config.clone(),
                        file: rel_path(dt, root),
                        start: 0,
                        end: 0,
                        message: "could not find a tsconfig that includes this file".to_string(),
                    });
                }
            }
        }
        let declaration = project["compilerOptions"]["declaration"].as_bool().unwrap_or(false)
            || project["compilerOptions"]["composite"].as_bool().unwrap_or(false);
        for file_name in file_names.iter().filter_map(Value::as_str) {
            let metadata = api.request(
                "getSourceFileMetadata",
                &json!({
                    "snapshot": snapshot, "project": project_id, "file": file_name,
                }),
            )?;
            if metadata["isDefaultLibrary"].as_bool() == Some(true)
                || metadata["isFromExternalLibrary"].as_bool() == Some(true)
                || !inside(file_name, dir)
                || is_types_version_path(file_name, dir)
            {
                continue;
            }
            outcome.source_files.push(rel_path(dt, file_name));
            for method in ["getSyntacticDiagnostics", "getSemanticDiagnostics"] {
                diagnostics.extend(
                    api.request(
                        method,
                        &json!({
                            "snapshot": snapshot, "project": project_id, "files": [file_name],
                        }),
                    )?
                    .as_array()
                    .ok_or("invalid file diagnostics response")?
                    .clone(),
                );
            }
            if declaration {
                diagnostics.extend(
                    api.request(
                        "getDeclarationDiagnostics",
                        &json!({
                            "snapshot": snapshot, "project": project_id, "files": [file_name],
                        }),
                    )?
                    .as_array()
                    .ok_or("invalid declaration diagnostics response")?
                    .clone(),
                );
            }
            let text = std::fs::read_to_string(file_name)?;
            if let Some(position) = lint_disable(&text) {
                outcome.issues.push(Issue {
                    kind: "lint-disable".to_string(),
                    config: config.clone(),
                    file: rel_path(dt, file_name),
                    start: position as i64,
                    end: (position + "eslint-disable".len()) as i64,
                    message: "'eslint-disable' is forbidden".to_string(),
                });
            }
            if text.contains("$ExpectType") {
                let ast = api.source_ast(snapshot, project_id, file_name)?;
                if !ast.file().is_declaration_file() {
                    check_assertions(
                        &mut api,
                        &ast,
                        snapshot,
                        project_id,
                        dt,
                        config,
                        file_name,
                        &text,
                        &mut outcome,
                    )?;
                }
            }
        }
        for diagnostic in diagnostics {
            if is_out_of_range_expect_error(&diagnostic) {
                continue;
            }
            outcome.issues.push(diagnostic_issue(dt, config, &diagnostic));
        }
    }
    let _ = api.request("release", &json!({ "snapshot": snapshot }));
    outcome.source_files.sort();
    Ok(outcome)
}

fn is_out_of_range_expect_error(diagnostic: &Value) -> bool {
    if diagnostic["code"].as_u64() != Some(2578) {
        return false;
    }
    let Some(file) = diagnostic["fileName"].as_str() else { return false };
    let Some(start) = diagnostic["pos"].as_u64() else { return false };
    let Some(end) = diagnostic["end"].as_u64() else { return false };
    if end < start {
        return false;
    }
    let Ok(source) = std::fs::read_to_string(file) else { return false };
    let from = utf16_to_byte(&source, start as u32);
    let to = utf16_to_byte(&source, end as u32);
    let Some(text) = source.get(from..to) else { return false };
    let mut comment = text.trim();
    comment = comment.trim_start_matches(['/', '*']).trim_start();
    let Some(range) = comment.strip_prefix("@ts-expect-error") else { return false };
    if !range.starts_with(char::is_whitespace) {
        return false;
    }
    let range = range.trim().trim_end_matches("*/").trim_end();
    if range.is_empty() {
        return false;
    }
    let Some(range) = tsrs_core::semver::try_parse_version_range(range) else { return false };
    let Some(version) = pinned_typescript_version() else { return false };
    let Ok(version) = tsrs_core::semver::try_parse_version(&version) else { return false };
    !range.test(&version)
}

fn pinned_typescript_version() -> Option<String> {
    let manifest = include_str!("../../../Cargo.toml");
    let section = manifest.split("[workspace.metadata.typescript]").nth(1)?;
    let line = section.lines().find(|line| line.trim_start().starts_with("version = "))?;
    Some(line.split('"').nth(1)?.split('-').next()?.to_string())
}

fn validate_tsconfig(dt: &Path, config: &str, raw: &Value, outcome: &mut CompilerOutcome) {
    let mut problems = Vec::new();
    let options = &raw["compilerOptions"];
    if raw.get("include").is_some() {
        problems.push("Use files instead of include".to_string());
    } else if raw.get("exclude").is_some() {
        problems.push("Use files instead of exclude".to_string());
    } else if let Some(files) = raw["files"].as_array() {
        if !files.iter().any(|file| matches!(file.as_str(), Some("index.d.ts" | "./index.d.ts"))) {
            problems.push("files must include index.d.ts".to_string());
        }
    } else {
        problems.push("Must specify files".to_string());
    }
    if options["noEmit"] != true {
        problems.push("compilerOptions.noEmit must be true".to_string());
    }
    if options["forceConsistentCasingInFileNames"] != true {
        problems.push("compilerOptions.forceConsistentCasingInFileNames must be true".to_string());
    }
    if options["types"] != json!([]) {
        problems.push("compilerOptions.types must be []".to_string());
    }
    if options.get("lib").is_none() {
        problems.push("compilerOptions.lib is required".to_string());
    }
    if !matches!(options["module"].as_str(), Some("commonjs" | "node16")) {
        problems.push("compilerOptions.module must be commonjs or node16".to_string());
    }
    for problem in problems {
        outcome.issues.push(Issue {
            kind: "tsconfig".to_string(),
            config: config.to_string(),
            file: rel_path(dt, config),
            start: -1,
            end: -1,
            message: problem,
        });
    }
}

fn lint_disable(text: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(relative) = text[from..].find("eslint-disable") {
        let start = from + relative;
        let suffix = &text[start + "eslint-disable".len()..];
        let mut chars = suffix.chars();
        let next = chars.next();
        let after_next = chars.next();
        if next != Some('-') && !(next == Some(' ') && after_next != Some('*')) {
            return Some(start);
        }
        from = start + "eslint-disable".len();
    }
    None
}

fn inside(file: &str, dir: &Path) -> bool {
    let dir = dir.to_string_lossy().replace('\\', "/");
    file.replace('\\', "/").starts_with(&format!("{}/", dir.trim_end_matches('/')))
}

fn is_types_version_path(file: &str, dir: &Path) -> bool {
    let dir = dir.to_string_lossy().replace('\\', "/");
    let normalized = file.replace('\\', "/");
    normalized
        .strip_prefix(&format!("{}/", dir.trim_end_matches('/')))
        .and_then(|s| s.split('/').next())
        .is_some_and(|first| {
            first.starts_with("ts") && first[2..].chars().next().is_some_and(|c| c.is_ascii_digit())
        })
}

fn rel_path(root: &Path, file: &str) -> String {
    PathBuf::from(file)
        .strip_prefix(root)
        .map_or_else(|_| file.to_string(), |path| path.to_string_lossy().replace('\\', "/"))
}

fn flatten_diagnostic(diagnostic: &Value, output: &mut Vec<String>) {
    if let Some(text) = diagnostic["text"].as_str() {
        output.push(text.to_string());
    }
    if let Some(children) = diagnostic["messageChain"].as_array() {
        for child in children {
            flatten_diagnostic(child, output);
        }
    }
}

fn diagnostic_issue(dt: &Path, config: &str, diagnostic: &Value) -> Issue {
    let mut parts = Vec::new();
    flatten_diagnostic(diagnostic, &mut parts);
    let file = diagnostic["fileName"].as_str().map_or_else(String::new, |file| rel_path(dt, file));
    Issue {
        kind: format!("TS{}", diagnostic["code"]),
        config: config.to_string(),
        file,
        start: diagnostic["pos"].as_i64().unwrap_or(-1),
        end: diagnostic["end"].as_i64().unwrap_or(-1),
        message: parts.join("\n"),
    }
}

fn check_assertions(
    api: &mut Api,
    ast: &SourceAst,
    snapshot: u64,
    project: &str,
    dt: &Path,
    config: &str,
    file_name: &str,
    text: &str,
    outcome: &mut CompilerOutcome,
) -> Result<()> {
    let relative = rel_path(dt, file_name);
    let (mut assertions, duplicates) = parse_assertions(text);
    let positions = TextIndex::new(text);
    for line in duplicates {
        outcome.issues.push(Issue {
            kind: "assertion".to_string(),
            config: config.to_string(),
            file: relative.clone(),
            start: line as i64,
            end: line as i64,
            message: "This line has 2 $ExpectType assertions.".to_string(),
        });
    }
    fn visit(
        node: P<Node>,
        api: &mut Api,
        ast: &SourceAst,
        snapshot: u64,
        project: &str,
        config: &str,
        relative: &str,
        text: &str,
        positions: &TextIndex,
        assertions: &mut BTreeMap<usize, String>,
        outcome: &mut CompilerOutcome,
    ) -> Result<()> {
        if node.kind() == Kind::EndOfFile {
            return Ok(());
        }
        let pos = node.pos().expect_source("decoded source node");
        let byte_pos = positions.byte_at(pos);
        let byte_start = tsrs_scanner::skip_trivia(text, byte_pos as u32) as usize;
        let start = positions.utf16_at(byte_start);
        let line = positions.line_at(byte_start);
        if let Some(expected) = assertions.remove(&line) {
            let target = match node.kind() {
                Kind::ExpressionStatement => node.as_expression_statement().expression(),
                Kind::VariableStatement => {
                    let declarations = node
                        .as_variable_statement()
                        .declaration_list()
                        .as_variable_declaration_list()
                        .declarations()
                        .nodes();
                    if declarations.len() == 1 {
                        declarations[0].as_variable_declaration().initializer().unwrap_or(node)
                    } else {
                        node
                    }
                }
                _ => node,
            };
            let type_response = api.request(
                "getTypeAtLocation",
                &json!({
                    "snapshot": snapshot, "project": project, "location": ast.handle(target),
                }),
            )?;
            let actual = if let Some(id) = type_response["id"].as_u64() {
                api.request(
                    "typeToString",
                    &json!({
                        "snapshot": snapshot, "project": project, "type": id, "flags": 1,
                    }),
                )?
                .as_str()
                .unwrap_or("")
                .to_string()
            } else {
                String::new()
            };
            let matched = expected.split("||").any(|candidate| {
                candidate.trim() == actual
                    || normalize_type(candidate.trim()) == normalize_type(&actual)
            });
            outcome.assertions.push(Assertion {
                config: config.to_string(),
                file: relative.to_string(),
                line: line + 1,
                expected: expected.clone(),
                actual: actual.clone(),
                matched,
            });
            if !matched {
                outcome.issues.push(Issue {
                    kind: "assertion".to_string(),
                    config: config.to_string(),
                    file: relative.to_string(),
                    start: i64::from(start),
                    end: node.end().expect_source("decoded source node") as i64,
                    message: format!("expected type to be:\n  {expected}\ngot:\n  {actual}"),
                });
            }
        }
        let mut children = Vec::new();
        node.for_each_child(&mut |child| {
            children.push(child);
            false
        });
        for child in children {
            visit(
                child, api, ast, snapshot, project, config, relative, text, positions, assertions,
                outcome,
            )?;
        }
        Ok(())
    }
    let mut roots = Vec::new();
    ast.file().as_node().for_each_child(&mut |child| {
        roots.push(child);
        false
    });
    for root in roots {
        visit(
            root,
            api,
            ast,
            snapshot,
            project,
            config,
            &relative,
            text,
            &positions,
            &mut assertions,
            outcome,
        )?;
    }
    for line in assertions.keys() {
        outcome.issues.push(Issue {
            kind: "assertion".to_string(),
            config: config.to_string(),
            file: relative.clone(),
            start: *line as i64,
            end: *line as i64,
            message: "Cannot match a node to this assertion.".to_string(),
        });
    }
    Ok(())
}

fn parse_assertions(text: &str) -> (BTreeMap<usize, String>, Vec<usize>) {
    let mut assertions = BTreeMap::new();
    let mut duplicates = Vec::new();
    for (line, content) in text.lines().enumerate() {
        let Some(marker) = content.rfind("$ExpectType") else { continue };
        let Some(comment) = content[..marker].rfind("//") else { continue };
        let Some(expected) = content[comment + 2..].trim().strip_prefix("$ExpectType") else {
            continue;
        };
        let target = if content[..comment].bytes().all(|c| c == b' ') { line + 1 } else { line };
        if assertions.remove(&target).is_some() {
            duplicates.push(target);
        } else {
            assertions.insert(target, expected.trim().to_string());
        }
    }
    (assertions, duplicates)
}

fn utf16_to_byte(text: &str, pos: u32) -> usize {
    let mut count = 0;
    for (byte, ch) in text.char_indices() {
        if count >= pos {
            return byte;
        }
        count += ch.len_utf16() as u32;
    }
    text.len()
}

#[cfg(test)]
fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].encode_utf16().count() as u32
}

#[cfg(test)]
fn line_of_utf16(text: &str, pos: u32) -> usize {
    text[..utf16_to_byte(text, pos)].bytes().filter(|&c| c == b'\n').count()
}

struct TextIndex {
    boundaries: Vec<(u32, usize)>,
    lines: Vec<usize>,
}

impl TextIndex {
    fn new(text: &str) -> Self {
        let mut boundaries = Vec::with_capacity(text.len() / 2);
        let mut utf16 = 0;
        let mut lines = vec![0];
        for (byte, ch) in text.char_indices() {
            boundaries.push((utf16, byte));
            utf16 += ch.len_utf16() as u32;
            if ch == '\n' {
                lines.push(byte + 1);
            }
        }
        boundaries.push((utf16, text.len()));
        Self { boundaries, lines }
    }

    fn byte_at(&self, position: u32) -> usize {
        let index = self.boundaries.partition_point(|&(utf16, _)| utf16 <= position);
        self.boundaries[index.saturating_sub(1)].1
    }

    fn utf16_at(&self, byte: usize) -> u32 {
        let index = self.boundaries.partition_point(|&(_, boundary)| boundary <= byte);
        self.boundaries[index.saturating_sub(1)].0
    }

    fn line_at(&self, byte: usize) -> usize {
        self.lines.partition_point(|&start| start <= byte).saturating_sub(1)
    }
}

// Port of lintCorsa's normalizedCorsaTypeToString: transform the parsed type
// before printing, including nested unions and type literals.
fn normalize_type(input: &str) -> String {
    let region = tsrs_core::arena::Region::new(64 << 10);
    let _scope = region.enter();
    let source = format!("declare var x: {input};");
    let file = tsrs_parser::parse_source_file(
        SourceFileParseOptions {
            file_name: "/expect-type.ts".to_string(),
            path: TsPath::new("/expect-type.ts".to_string()),
            external_module_indicator_options:
                tsrs_ast::parseoptions::ExternalModuleIndicatorOptions::default(),
        },
        &source,
        ScriptKind::TS,
    );
    let Some(statement) = file.statements().nodes().first().copied() else {
        return input.to_string();
    };
    if statement.kind() != Kind::VariableStatement {
        return input.to_string();
    }
    let Some(declaration) = statement
        .as_variable_statement()
        .declaration_list()
        .as_variable_declaration_list()
        .declarations()
        .nodes()
        .first()
        .copied()
    else {
        return input.to_string();
    };
    let Some(type_node) = declaration.as_variable_declaration().type_() else {
        return input.to_string();
    };
    let printer = Rc::new(RefCell::new(tsrs_printer::new_printer(
        tsrs_printer::PrinterOptions::default(),
        tsrs_printer::PrintHandlers::default(),
        None,
    )));
    let factory = NodeFactory::default();
    let compare_printer = Rc::clone(&printer);
    let transform_factory = factory.clone();
    let transform = Rc::new(move |visitor: &mut tsrs_ast::NodeVisitor, node: P<Node>| {
        let node = visitor.visit_each_child(Some(node))?;
        let print = |item| compare_printer.borrow_mut().emit(item, Some(file));
        Some(match node.kind() {
            Kind::LiteralType
                if node.as_literal_type_node().literal().kind() == Kind::StringLiteral =>
            {
                let literal = node.as_literal_type_node().literal();
                transform_factory.new_literal_type_node(
                    transform_factory
                        .new_string_literal(literal.as_string_literal().text(), TokenFlags::None),
                )
            }
            Kind::UnionType => {
                let mut parts = node.as_union_type_node().types().nodes().to_vec();
                parts.sort_by_cached_key(|&item| print(item));
                transform_factory
                    .update_union_type_node(node, transform_factory.new_node_list(parts))
            }
            Kind::TypeLiteral => {
                let members = node.as_type_literal_node().members().nodes();
                if members.iter().all(|member| {
                    matches!(member.kind(), Kind::PropertySignature | Kind::IndexSignature)
                }) {
                    let mut parts = members.to_vec();
                    parts.sort_by_cached_key(|&item| print(item));
                    transform_factory
                        .update_type_literal_node(node, transform_factory.new_node_list(parts))
                } else {
                    node
                }
            }
            Kind::TypeOperator
                if node.as_type_operator_node().operator() == Kind::ReadonlyKeyword =>
            {
                let mut inner = node.as_type_operator_node().type_();
                if inner.kind() == Kind::ArrayType {
                    inner = inner.as_array_type_node().element_type();
                    while inner.kind() == Kind::ParenthesizedType {
                        inner = inner.as_parenthesized_type_node().type_();
                    }
                    transform_factory.new_type_reference_node(
                        transform_factory.new_identifier("ReadonlyArray"),
                        Some(transform_factory.new_node_list(vec![inner])),
                    )
                } else {
                    node
                }
            }
            _ => node,
        })
    });
    let mut visitor = new_node_visitor(Some(transform), Some(factory), NodeVisitorHooks::default());
    let transformed = visitor.visit_node(Some(type_node)).unwrap();
    printer.borrow_mut().emit(transformed, Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assertions_follow_upstream_line_rules() {
        let (assertions, duplicates) = parse_assertions(
            "// $ExpectType number\nfoo; // $ExpectType string\n  // $ExpectType boolean\nbar; // $ExpectType null\n",
        );
        assert!(assertions.is_empty());
        assert_eq!(duplicates, vec![1, 3]);
    }

    #[test]
    fn assertion_after_url_uses_the_comment_delimiter() {
        let (assertions, duplicates) =
            parse_assertions("const url = \"https://example.test\"; // $ExpectType string\n");
        assert_eq!(assertions.get(&0).map(String::as_str), Some("string"));
        assert_eq!(duplicates, Vec::<usize>::new());
    }

    #[test]
    fn utf16_positions_account_for_astral_text() {
        let text = "a😀b\n";
        assert_eq!(utf16_to_byte(text, 3), 5);
        assert_eq!(byte_to_utf16(text, 5), 3);
        assert_eq!(line_of_utf16(text, 4), 0);
        let index = TextIndex::new(text);
        assert_eq!(index.byte_at(3), 5);
        assert_eq!(index.utf16_at(5), 3);
        assert_eq!(index.line_at(6), 0);
    }

    #[test]
    fn expected_type_normalizes_unions_and_readonly_arrays() {
        assert_eq!(normalize_type("string | number"), normalize_type("number | string"));
        assert_eq!(normalize_type("readonly string[]"), normalize_type("ReadonlyArray<string>"));
        assert_eq!(
            normalize_type("Array<string | number>"),
            normalize_type("Array<number | string>")
        );
        assert_eq!(
            normalize_type("{ b: string; a: number }"),
            normalize_type("{ a: number; b: string }")
        );
        assert_eq!(normalize_type("'hello'"), normalize_type("\"hello\""));
    }

    #[test]
    fn lint_disable_allows_line_and_rule_scopes() {
        assert_eq!(lint_disable("/* eslint-disable */"), Some(3));
        assert_eq!(lint_disable("// eslint-disable-next-line"), None);
        assert_eq!(lint_disable("// eslint-disable rule"), None);
    }

    #[test]
    fn typescript_version_ranges_parse_like_expect_error_rules() {
        let version =
            tsrs_core::semver::try_parse_version(&pinned_typescript_version().unwrap()).unwrap();
        assert!(tsrs_core::semver::try_parse_version_range(">=7.0").unwrap().test(&version));
        assert!(!tsrs_core::semver::try_parse_version_range(">=7.2").unwrap().test(&version));
    }
}
