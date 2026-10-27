use std::sync::Arc;

use tsrs_ast::Diagnostic;
use tsrs_compiler::diagnosticwriter::{FormattingOptions, write_format_diagnostic};
use tsrs_compiler::{
    Context, Program, ProgramOptions, ProgramRegions, get_diagnostics_of_any_program,
    new_compiler_host, new_program,
};
use tsrs_core::{CompilerOptions, P, Tristate, tspath};
use tsrs_tsoptions::{self as tsoptions, ParseConfigHost};
use tsrs_vfs::FS;

use crate::{BuildOptions, CapturedDeclarations, DeclarationContext, Error, Result, host};

// No arena pointers, type handles or diagnostics may escape this private owner.
// Public results consist exclusively of owned strings and counters.
pub struct ProgramOwner {
    program: &'static Program,
    _mapper: MapperOwner,
    // Parsed configs/options are allocated before the file-loader regions exist.
    // Keep their separate arena alive until after the Program is released.
    _config_region: tsrs_core::arena::Region,
}
impl ProgramOwner {
    pub fn program(&self) -> &'static Program {
        self.program
    }
}
impl Drop for ProgramOwner {
    fn drop(&mut self) {
        // SAFETY: only our private pipeline owns this new_program result; it was
        // never updated/shared. Graph/checker handles are dropped before owner.
        // Callers receive no reference to the declaration/source Program.
        unsafe { tsrs_compiler::free_unshared_program(self.program) };
    }
}

#[derive(Default)]
struct MapperOwner {
    project: Option<Arc<dyn tsrs_contentmapper::Project>>,
    host: Option<Arc<dyn tsrs_contentmapper::Host>>,
}
impl Drop for MapperOwner {
    fn drop(&mut self) {
        if let Some(project) = &self.project {
            let _ = project.close();
        }
        if let Some(host) = &self.host {
            let _ = host.close();
        }
    }
}

struct ConfigHost {
    fs: Arc<dyn FS>,
}
impl ParseConfigHost for ConfigHost {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

/// Fail with tsc-style lines: `file(line,column): error TSxxxx: message`, with file
/// names relative to `current_directory`.
pub fn diagnostics(items: &[P<Diagnostic>], current_directory: &str) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let options = FormattingOptions {
        compare_paths_options: tspath::ComparePathsOptions {
            current_directory: current_directory.to_string(),
            use_case_sensitive_file_names: true,
        },
        new_line: "\n".into(),
    };
    // Copy while the owning Program/diagnostic region is still alive.
    let mut text = Vec::new();
    for &item in items {
        write_format_diagnostic(&mut text, item, &options);
    }
    Err(Error(String::from_utf8_lossy(&text).trim_end().to_string()))
}

pub fn check(program: &'static Program, context: &Context) -> Result<()> {
    diagnostics(
        &get_diagnostics_of_any_program(
            context,
            program,
            None,
            false,
            &mut |ctx, file| program.get_bind_diagnostics(ctx, file),
            &mut |ctx, file| program.get_semantic_diagnostics(ctx, file),
        ),
        program.host().get_current_directory(),
    )
}

fn create(
    config: P<tsoptions::ParsedCommandLine>,
    fs: Arc<dyn FS>,
    cwd: &str,
    default_library_path: &str,
    config_region: tsrs_core::arena::Region,
) -> Result<ProgramOwner> {
    let mut mapper = MapperOwner::default();
    if !config.content_mappers().is_empty() {
        let options =
            config.compiler_options().ok_or_else(|| Error("missing compiler options".into()))?;
        if !options.run_external_code.is_true() {
            return Err(Error("content mappers require run_external_code".into()));
        }
        mapper.host = Some(tsrs_contentmapper::new_host(
            Arc::new(tsrs_contentmapper::ProcessSpawner),
            tsrs_core::Locale(options.locale.clone()),
        ));
        mapper.project = mapper.host.as_ref().unwrap().project(tsrs_contentmapper::ProjectSpec {
            config_file_name: config.config_name().into(),
            mappers: config.get().content_mappers().iter().collect(),
            compiler_options: Some(options),
        });
        if mapper.project.is_none() {
            return Err(Error("could not create content mapper project".into()));
        }
    }
    let mut program_options = ProgramOptions::new(
        config,
        new_compiler_host(cwd, fs, default_library_path, None, None, mapper.project.clone()),
    );
    // One declaration checker gives a single consistent symbol identity domain
    // for this initial linker.

    // TODO: Support symbol identities across checkers, then enable parallel checking
    // after output/diagnostic parity checks. See docs/DTS.md.
    program_options.single_threaded = Tristate::True;
    program_options.canonical_diagnostics = false;
    program_options.regions = Some(Arc::new(ProgramRegions::default()));
    program_options.checker_regions = true;
    Ok(ProgramOwner {
        program: new_program(program_options),
        _mapper: mapper,
        _config_region: config_region,
    })
}

pub fn source_program(options: &BuildOptions, fs: Arc<dyn FS>) -> Result<ProgramOwner> {
    // Entries are validated when linking; `build_entries` ignores `options.entry`.
    if !tspath::is_rooted_disk_path(&options.config_path) {
        return Err(Error("config_path must be an absolute path".into()));
    }
    let config_host = ConfigHost { fs: Arc::clone(&fs) };
    let config_region = tsrs_core::arena::Region::new_scratch(1 << 20);
    let config_scope = config_region.enter_scratch();
    let (config, errors) = tsoptions::get_parsed_command_line_of_config_file(
        &options.config_path,
        Some(&CompilerOptions {
            run_external_code: tsrs_core::bool_to_tristate(options.run_external_code),
            ..Default::default()
        }),
        None,
        &config_host,
        None,
    );
    diagnostics(&errors, &tspath::get_directory_path(&options.config_path))?;
    let mut config =
        config.ok_or_else(|| Error("tsrs could not load project configuration".into()))?;
    let original = config
        .parsed_config
        .compiler_options
        .ok_or_else(|| Error("missing compiler options".into()))?;
    let compiler_options = CompilerOptions {
        declaration: Tristate::True,
        emit_declaration_only: Tristate::True,
        no_emit: Tristate::False,
        no_check: Tristate::False,
        no_emit_on_error: Tristate::True,
        declaration_map: original.declaration_map,
        incremental: Tristate::False,
        composite: Tristate::False,
        ts_build_info_file: String::new(),
        // Capture uses source provenance, so tsconfig output layout is irrelevant.
        // In particular, inherited outDir/declarationDir can trigger TS5011 even
        // though no compiler output path will ever be written.

        // TODO: Add a capture API that avoids irrelevant output-layout checks while
        // preserving resolution, provenance and maps, then remove these overrides.
        // See docs/DTS.md, "Private Program options and reference freshness".
        out_dir: String::new(),
        declaration_dir: String::new(),
        out_file: String::new(),
        ..original.get().clone()
    };
    config.parsed_config.compiler_options = Some(P::new(compiler_options));
    let config = P::new(config);
    drop(config_scope);
    create(
        config,
        fs,
        &tspath::get_directory_path(&options.config_path),
        &options.default_library_path,
        config_region,
    )
}

pub fn declaration_program(
    context: &DeclarationContext,
    captured: &CapturedDeclarations,
    fs: Arc<dyn FS>,
) -> Result<ProgramOwner> {
    // Emit arenas are temporary, and the source checker cannot query synthesized
    // declaration nodes. Rebind owned declaration text to obtain usable symbols.

    // TODO: Provide retained emit arenas and checker support for emitted nodes,
    // then replace rebinding after bundle/map parity checks. See docs/DTS.md.
    let overlay: Arc<dyn FS> = Arc::new(host::DeclarationFs {
        files: captured
            .files
            .iter()
            .map(|(path, text)| {
                (
                    tspath::get_canonical_file_name(path, fs.use_case_sensitive_file_names()),
                    text.clone(),
                )
            })
            .collect(),
        hidden: captured
            .source_to_declaration
            .keys()
            .filter(|p| !captured.files.contains_key(*p))
            .map(|path| tspath::get_canonical_file_name(path, fs.use_case_sensitive_file_names()))
            .collect(),
        base: fs,
    });
    let config_region = tsrs_core::arena::Region::new_scratch(1 << 20);
    let config_scope = config_region.enter_scratch();
    let compiler_options = CompilerOptions {
        no_emit: Tristate::True,
        no_check: Tristate::False,
        skip_lib_check: Tristate::False,
        skip_default_lib_check: Tristate::False,
        declaration: Tristate::False,
        emit_declaration_only: Tristate::False,
        composite: Tristate::False,
        incremental: Tristate::False,
        root_dir: String::new(),
        out_dir: String::new(),
        declaration_dir: String::new(),
        declaration_map: Tristate::False,
        out_file: String::new(),
        ts_build_info_file: String::new(),
        ..context.compiler_options.clone()
    };
    let config = tsoptions::new_parsed_command_line(
        P::new(compiler_options),
        captured.files.keys().cloned().collect(),
        context.project_references.clone(),
        tspath::ComparePathsOptions {
            current_directory: context.current_directory.clone(),
            use_case_sensitive_file_names: overlay.use_case_sensitive_file_names(),
        },
    );
    let config = P::new(config);
    drop(config_scope);
    create(
        config,
        overlay,
        &context.current_directory,
        &context.default_library_path,
        config_region,
    )
}
