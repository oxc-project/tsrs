use std::collections::BTreeMap;
use std::sync::Mutex;

use tsrs_compiler::{Context, EmitOnly, EmitOptions, Program, WriteFileData};

use crate::{Error, Files, Result, host, project};

/// Owned declaration texts keyed by normalized absolute virtual paths.
///
/// Provenance maps source paths to those keys; maps use the same declaration keys
/// and carry resolved absolute original source paths in `input_source_file_names`.
#[derive(Default)]
pub struct CapturedDeclarations {
    pub files: Files,
    pub source_to_declaration: BTreeMap<String, String>,
    pub declaration_maps: BTreeMap<String, tsrs_compiler::SourceMapEmitResult>,
}

impl CapturedDeclarations {
    pub fn validate(&self, case_sensitive: bool) -> Result<()> {
        let mut paths = std::collections::BTreeSet::new();
        for path in self.files.keys() {
            validate_path(path)?;
            if !tsrs_core::tspath::is_declaration_file_name(path) {
                return Err(Error(format!("not a declaration file: {path}")));
            }
            if !paths.insert(tsrs_core::tspath::get_canonical_file_name(path, case_sensitive)) {
                return Err(Error(format!("duplicate declaration path: {path}")));
            }
        }
        for (source, declaration) in &self.source_to_declaration {
            validate_path(source)?;
            if !self.files.contains_key(declaration) {
                return Err(Error(format!("missing captured declaration: {declaration}")));
            }
            if self.files.contains_key(source) && source != declaration {
                return Err(Error(format!("conflicting declaration provenance: {source}")));
            }
        }
        for (path, map) in &self.declaration_maps {
            if !self.files.contains_key(path) {
                return Err(Error(format!("map has no captured declaration: {path}")));
            }
            if map.input_source_file_names.len() != map.source_map.sources.len() {
                return Err(Error(format!("invalid declaration map sources: {path}")));
            }
            for source in &map.input_source_file_names {
                validate_path(source)?;
            }
            validate_map(&map.source_map, path)?;
        }
        Ok(())
    }
}

fn validate_path(path: &str) -> Result<()> {
    if !tsrs_core::tspath::is_rooted_disk_path(path) || host::normalize(path) != path {
        return Err(Error(format!("capture path must be absolute and normalized: {path}")));
    }
    Ok(())
}

fn validate_map(map: &tsrs_sourcemap::RawSourceMap, path: &str) -> Result<()> {
    let mut decoder = tsrs_sourcemap::decode_mappings(&map.mappings);
    let mut previous = (0, 0);
    for mapping in decoder.values() {
        let position = (mapping.generated_line, mapping.generated_character);
        if position < previous
            || mapping.source_index.get().is_some_and(|index| index as usize >= map.sources.len())
            || mapping.name_index.get().is_some_and(|index| index as usize >= map.names.len())
        {
            return Err(Error(format!("invalid declaration map: {path}")));
        }
        previous = position;
    }
    if let Some(error) = decoder.error() {
        return Err(Error(format!("invalid declaration map {path}: {error}")));
    }
    Ok(())
}

/// Reuse the existing tsrs declaration transformer and printer.
///
/// The compiler's Sync writer callback intercepts every artifact; no host writer
/// is called. Requires declaration emission, `noEmit`/`noCheck` disabled, and no
/// `outFile`.
pub fn emit_declarations(
    program: &'static Program,
    context: &Context,
) -> Result<CapturedDeclarations> {
    if !program.options().get_emit_declarations() || program.options().no_emit.is_true() {
        return Err(Error("Program must enable declaration emission and disable noEmit".into()));
    }
    if program.options().no_check.is_true() || !program.options().out_file.is_empty() {
        return Err(Error("Program must disable noCheck and leave outFile unset".into()));
    }
    project::check(program, context)?;
    emit_checked_declarations(program, context)
}

// Only the private build pipeline calls this after a successful full diagnostic
// pass. The public capture API above always checks its caller-owned Program.
pub fn emit_checked_declarations(
    program: &'static Program,
    context: &Context,
) -> Result<CapturedDeclarations> {
    let captured = Mutex::new(CapturedDeclarations::default());
    let writer =
        |path: &str, text: &str, data: &mut WriteFileData| -> std::result::Result<(), String> {
            if !tsrs_core::tspath::is_declaration_file_name(path) {
                return Ok(());
            }
            let source =
                data.source_file.ok_or_else(|| format!("missing source provenance: {path}"))?;
            let source_path = host::normalize(source.file_name());
            let virtual_path =
                tsrs_tsoptions::outputpaths::change_to_declaration_extension(&source_path, program);
            let mut output =
                captured.lock().map_err(|_| "poisoned declaration capture".to_string())?;
            if output.files.insert(virtual_path.clone(), text.to_string()).is_some() {
                return Err(format!("duplicate declaration output: {virtual_path}"));
            }
            output.source_to_declaration.insert(source_path, virtual_path);
            Ok(())
        };
    let result = program.emit(
        context,
        &EmitOptions {
            emit_only: EmitOnly::EmitOnlyDts,
            write_file: Some(&writer),
            ..EmitOptions::default()
        },
    );
    project::diagnostics(&result.diagnostics, program.host().get_current_directory())?;
    if result.emit_skipped {
        return Err(Error("tsrs skipped declaration emit".into()));
    }
    let mut captured =
        captured.into_inner().map_err(|_| Error("poisoned declaration capture".into()))?;
    for map in result.source_maps {
        if let Some(declaration) = map
            .input_source_file_names
            .first()
            .and_then(|source| captured.source_to_declaration.get(&host::normalize(source)))
        {
            captured.declaration_maps.insert(declaration.clone(), map);
        }
    }
    // Declaration inputs are never emitted again. Join their already-owned text
    // to the same virtual graph while keeping dependency libraries external.
    for &file in program.source_files() {
        if file.is_declaration_file()
            && !program.is_source_file_default_library(file.path())
            && !program.is_source_file_from_external_library(file)
        {
            let path = host::normalize(file.file_name());
            captured.files.entry(path.clone()).or_insert_with(|| file.text().to_string());
            if program.options().declaration_map.is_true() {
                let lines = tsrs_sourcemap::create_ecma_line_info(
                    file.text().to_string(),
                    file.ecma_line_map().to_vec(),
                );
                let url = tsrs_sourcemap::try_get_source_mapping_url(Some(&lines));
                let map_path = if url.is_empty() {
                    format!("{path}.map")
                } else {
                    tsrs_core::tspath::resolve_path(
                        &tsrs_core::tspath::get_directory_path(&path),
                        &[&url],
                    )
                };
                if let Some(text) = program.host().fs().read_file(&map_path) {
                    let value = tsrs_core::json::unmarshal(&text).map_err(Error)?;
                    let map = tsrs_sourcemap::RawSourceMap::from_json(&value).map_err(Error)?;
                    validate_map(&map, &map_path)?;
                    let directory = tsrs_core::tspath::get_directory_path(&map_path);
                    let sources = map
                        .sources
                        .iter()
                        .map(|source| {
                            tsrs_core::tspath::resolve_path(&directory, &[&map.source_root, source])
                        })
                        .collect();
                    captured.declaration_maps.insert(
                        path.clone(),
                        tsrs_compiler::SourceMapEmitResult {
                            input_source_file_names: sources,
                            source_map: map,
                            generated_file: path.clone(),
                        },
                    );
                }
            }
            captured.source_to_declaration.insert(path.clone(), path);
        }
    }
    Ok(captured)
}
