use std::collections::BTreeMap;
use std::sync::Arc;

use tsrs_compiler::{
    Context, Program, ProgramOptions, ProgramRegions, get_diagnostics_of_any_program,
    new_compiler_host, new_program,
};
use tsrs_core::json::{JsonMap, Value};
use tsrs_core::{CompilerOptions, Locale, P, Tristate, arena::Region};
use tsrs_tsoptions::ParseConfigHost;
use tsrs_vfs::FS;

struct ConfigHost(Arc<dyn FS>);
impl ParseConfigHost for ConfigHost {
    fn fs(&self) -> &dyn FS {
        &*self.0
    }
    fn get_current_directory(&self) -> &'static str {
        "/"
    }
}

struct HostProgram {
    program: &'static Program,
    project: Arc<dyn tsrs_contentmapper::Project>,
    host: Arc<dyn tsrs_contentmapper::Host>,
    _region: Region,
}
impl Drop for HostProgram {
    fn drop(&mut self) {
        // SAFETY: this test exclusively owns its Program, with no updates or shared Programs.
        unsafe { tsrs_compiler::free_unshared_program(self.program) };
        self.project.close().expect("close Vue mapper project");
        self.host.close().expect("close Vue mapper host");
    }
}

fn strings(files: &BTreeMap<String, String>) -> Value {
    Value::Object(
        files.iter().map(|(name, text)| (name.clone(), Value::String(text.clone()))).collect(),
    )
}

fn run(config_path: &str, entry: &str) -> Result<Value, String> {
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(tsrs_vfs::osvfs::fs()));
    let region = Region::new_scratch(1 << 20);
    let scope = region.enter_scratch();
    let (config, errors) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
        config_path,
        Some(&CompilerOptions { run_external_code: Tristate::True, ..Default::default() }),
        None,
        &ConfigHost(Arc::clone(&fs)),
        None,
    );
    if !errors.is_empty() {
        return Err(format!("Vue config diagnostics: {errors:?}"));
    }
    let config = P::new(config.ok_or("missing Vue config")?);
    let compiler_options = config.compiler_options().ok_or("missing compiler options")?;
    if !compiler_options.strict.is_true()
        || compiler_options.no_check.is_true()
        || compiler_options.skip_lib_check.is_true()
        || compiler_options.skip_default_lib_check.is_true()
    {
        return Err("Vue tests require strict checking and library checks".into());
    }
    if config.get().content_mappers().is_empty() {
        return Err("Vue test config must enable the mapper".into());
    }
    let host = tsrs_contentmapper::new_host_with_options(
        Arc::new(tsrs_contentmapper::ProcessSpawner),
        Locale::DEFAULT,
        tsrs_contentmapper::HostOptions { logger: Some(Arc::new(|line| eprintln!("{line}"))) },
    );
    let project = host
        .project(tsrs_contentmapper::ProjectSpec {
            config_file_name: config_path.into(),
            mappers: config.get().content_mappers().iter().collect(),
            compiler_options: Some(compiler_options),
        })
        .ok_or("missing Vue mapper project")?;
    let mut options = ProgramOptions::new(
        config,
        new_compiler_host(
            &tsrs_core::tspath::get_directory_path(config_path),
            Arc::clone(&fs),
            &tsrs_vfs::bundled::lib_path(),
            None,
            None,
            Some(Arc::clone(&project)),
        ),
    );
    options.single_threaded = Tristate::True;
    options.canonical_diagnostics = false;
    options.regions = Some(Arc::new(ProgramRegions::default()));
    options.checker_regions = true;
    drop(scope);
    let owner = HostProgram { program: new_program(options), project, host, _region: region };
    let context = Context::background();
    let program = owner.program;
    let diagnostics = get_diagnostics_of_any_program(
        &context,
        program,
        None,
        false,
        &mut |ctx, file| program.get_bind_diagnostics(ctx, file),
        &mut |ctx, file| program.get_semantic_diagnostics(ctx, file),
    );
    let mut result = JsonMap::new();
    result.insert(
        "codes",
        Value::Array(diagnostics.iter().map(|d| Value::Integer(i64::from(d.code()))).collect()),
    );
    result.insert(
        "diagnostics",
        Value::Array(
            diagnostics
                .iter()
                .map(|d| Value::String(format!("TS{}: {}", d.code(), d.get())))
                .collect(),
        ),
    );
    let options = tsrs_dts::BuildOptions {
        config_path: config_path.into(),
        entry: entry.into(),
        output: tsrs_dts::OutputMode::Bundle { file_name: "index.d.ts".into() },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: true,
        reuse_diagnostics: false,
    };
    let public = tsrs_dts::build(&options, Arc::clone(&fs));
    let capture = tsrs_dts::emit_declarations(program, &context);
    if !diagnostics.is_empty() {
        let error = capture.err().ok_or("invalid Vue project unexpectedly emitted")?;
        let public_error = public.err().ok_or("invalid Vue project passed public build")?;
        for diagnostic in &diagnostics {
            if !public_error.to_string().contains(&format!("TS{}:", diagnostic.code())) {
                return Err(format!("public build missed diagnostic {}", diagnostic.code()));
            }
        }
        result.insert("emitError", Value::String(error.to_string()));
        return Ok(Value::Object(result));
    }
    let captured = capture.map_err(|error| error.to_string())?;
    result.insert("declarations", strings(&captured.files));
    result.insert("provenance", strings(&captured.source_to_declaration));
    let output = tsrs_dts::link_declarations(
        tsrs_dts::DeclarationInputs {
            context: tsrs_dts::DeclarationContext::from_program(program),
            captured,
            fs,
        },
        &tsrs_dts::LinkOptions { entries: BTreeMap::from([("index.d.ts".into(), entry.into())]) },
    )
    .map_err(|error| error.to_string())?;
    let public = public.map_err(|error| error.to_string())?;
    if public.files != output.files {
        return Err("public Vue build differs from borrowed Program linking".into());
    }
    let mut denied = options.clone();
    denied.run_external_code = false;
    if !tsrs_dts::build(&denied, Arc::new(tsrs_vfs::bundled::wrap_fs(tsrs_vfs::osvfs::fs())))
        .err()
        .is_some_and(|error| error.to_string().contains("TS18068"))
    {
        return Err("Vue mapper ran without explicit opt-in".into());
    }
    let mut session = tsrs_dts::BuildSession::new();
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(tsrs_vfs::osvfs::fs()));
    let first = session.build(&options, Arc::clone(&fs)).map_err(|error| error.to_string())?;
    let second = session.build(&options, fs).map_err(|error| error.to_string())?;
    if first.files != public.files
        || second.files != public.files
        || Arc::ptr_eq(&first, &second)
        || session.cache_hits() != 0
        || session.link_cache_hits() != 0
    {
        return Err("mapper session reused untracked external inputs".into());
    }
    result.insert("files", strings(&output.files));
    Ok(Value::Object(result))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "vue-test-host <absolute config> <absolute entry>");
    match run(&args[1], &args[2]) {
        Ok(result) => println!("{}", tsrs_core::json::marshal(&result).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
