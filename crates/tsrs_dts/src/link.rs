use std::collections::BTreeMap;
use std::sync::Arc;

use tsrs_compiler::{Context, Program};
use tsrs_core::{CompilerOptions, ProjectReference, tspath};
use tsrs_vfs::FS;

use crate::{BuildResult, CapturedDeclarations, Error, Result, graph, host, output, project};

/// Owned compilation settings used to resolve and check captured declarations.
///
/// The filesystem supplied alongside these settings must represent the same
/// dependency snapshot as the capture, including prepared project references.
#[derive(Clone, Debug)]
pub struct DeclarationContext {
    pub compiler_options: CompilerOptions,
    pub project_references: Vec<ProjectReference>,
    pub current_directory: String,
    pub default_library_path: String,
}

impl DeclarationContext {
    /// Copy settings without retaining any Program, AST or checker handles.
    pub fn from_program(program: &'static Program) -> Self {
        Self {
            compiler_options: program.options().get().clone(),
            project_references: program.command_line().project_references().to_vec(),
            current_directory: program.host().get_current_directory().to_string(),
            default_library_path: program.host().default_library_path().to_string(),
        }
    }
}

/// Output declaration basenames mapped to absolute source entry paths.
///
/// Entries are resolved through `CapturedDeclarations::source_to_declaration`.
/// One entry produces a bundle; multiple entries produce facades and a shared bundle.
#[derive(Clone, Debug)]
pub struct LinkOptions {
    pub entries: BTreeMap<String, String>,
}

/// Owned declaration text, provenance and maps with their resolution context.
///
/// The filesystem remains read-only and must stay stable throughout linking.
/// Source compilation and project-reference preparation belong to the caller.
pub struct DeclarationInputs {
    pub captured: CapturedDeclarations,
    pub context: DeclarationContext,
    pub fs: Arc<dyn FS>,
}

/// Bind and fully check captured declarations, then link their public entries.
///
/// This creates one fresh declaration Program, with no source compilation or
/// declaration emit. Only owned artifacts leave the call; write every result file.
pub fn link_declarations(inputs: DeclarationInputs, options: &LinkOptions) -> Result<BuildResult> {
    link(&inputs.captured, &inputs.context, options, inputs.fs, false, None)
}

/// Check and emit a caller-owned Program, then link through the captured-input pipeline.
///
/// Declaration emit must be enabled, `noEmit`/`noCheck` disabled, and `outFile` unset. The Program is never freed or updated by this crate. The host
/// must prepare referenced outputs and supply a filesystem matching the Program.
pub fn build_from_program(
    program: &'static Program,
    context: &Context,
    options: &LinkOptions,
    fs: Arc<dyn FS>,
) -> Result<BuildResult> {
    crate::validate_entries(&options.entries)?;
    let captured = crate::emit_declarations(program, context)?;
    link_declarations(
        DeclarationInputs { captured, context: DeclarationContext::from_program(program), fs },
        options,
    )
}

pub fn link(
    captured: &CapturedDeclarations,
    context: &DeclarationContext,
    options: &LinkOptions,
    fs: Arc<dyn FS>,
    facades: bool,
    diagnostics: Option<&crate::diagnostics::Diagnostics>,
) -> Result<BuildResult> {
    crate::validate_entries(&options.entries)?;
    if !tspath::is_rooted_disk_path(&context.current_directory)
        || host::normalize(&context.current_directory) != context.current_directory
    {
        return Err(Error("current_directory must be an absolute normalized path".into()));
    }
    let case_sensitive = fs.use_case_sensitive_file_names();
    captured.validate(case_sensitive)?;
    let provenance: BTreeMap<_, _> = captured
        .source_to_declaration
        .iter()
        .map(|(source, declaration)| {
            (tspath::get_canonical_file_name(source, case_sensitive), declaration)
        })
        .collect();
    crate::validate_output_names(&options.entries, case_sensitive)?;
    let mut entries = BTreeMap::new();
    for (name, source) in &options.entries {
        if !tspath::is_rooted_disk_path(source) {
            return Err(Error(format!("entry must be an absolute path: {source}")));
        }
        let declaration = provenance
            .get(&tspath::get_canonical_file_name(&host::normalize(source), case_sensitive))
            .ok_or_else(|| Error(format!("entry was not emitted: {source}")))?;
        entries.insert(name.clone(), *declaration);
    }
    let (first_name, entry) = entries.first_key_value().unwrap();
    let declarations = project::declaration_program(context, captured, Arc::clone(&fs))?;
    // Release the declaration snapshot before its Program, on success and error.
    let diagnostics = diagnostics.map(|state| state.declarations(declarations.program()));
    if let Some(diagnostics) = &diagnostics {
        diagnostics.check(declarations.program(), || {
            project::declaration_program(context, captured, fs)
        })?;
    } else {
        project::check(declarations.program(), &Context::background())?;
    }
    let mut graph = graph::Graph::analyze(declarations.program(), &captured.files, entry)?;
    let maps = context.compiler_options.declaration_map.is_true();
    let mut live = graph::Live::default();
    let files = if facades || entries.len() > 1 {
        let mut public = BTreeMap::new();
        for (name, declaration) in entries {
            let module = graph
                .find_module(declaration)
                .ok_or_else(|| Error(format!("entry missing from graph: {declaration}")))?;
            live.extend(graph.reachable(module)?);
            public.insert(name, module);
        }
        // Shared-bundle statements are attributed to the last entry.
        let source = public.values().last().copied().unwrap();
        let layout = output::Layout::Facades { entries: &public, source };
        output::generate(&mut graph, &live, &layout, &context.current_directory, captured, maps)?
    } else {
        live = graph.reachable(graph.entry)?;
        let layout = output::Layout::Bundle(first_name);
        output::generate(&mut graph, &live, &layout, &context.current_directory, captured, maps)?
    };
    let retained_statements = live.statements.len();
    Ok(BuildResult {
        files,
        emitted_declarations: captured.files.len(),
        retained_statements,
        removed_statements: graph.statements.len().saturating_sub(retained_statements),
    })
}
