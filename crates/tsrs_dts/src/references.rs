use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tsrs_core::{P, Tristate};
use tsrs_execute::{build, tsc};
use tsrs_vfs::{Entries, FS, FileInfo};

use crate::{BuildOptions, Error, Files, Result, host, project};

struct Outputs {
    overlay: Mutex<host::DeclarationFs>,
    timestamp: SystemTime,
}
impl FS for Outputs {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.overlay.lock().unwrap().use_case_sensitive_file_names()
    }
    fn file_exists(&self, path: &str) -> bool {
        self.overlay.lock().unwrap().file_exists(path)
    }
    fn read_file(&self, path: &str) -> Option<String> {
        self.overlay.lock().unwrap().read_file(path)
    }
    fn directory_exists(&self, path: &str) -> bool {
        self.overlay.lock().unwrap().directory_exists(path)
    }
    fn get_accessible_entries(&self, path: &str) -> Entries {
        self.overlay.lock().unwrap().get_accessible_entries(path)
    }
    fn stat(&self, path: &str) -> Option<FileInfo> {
        self.overlay.lock().unwrap().stat(path).map(|mut info| {
            info.mod_time = Some(self.timestamp);
            info
        })
    }
    fn realpath(&self, path: &str) -> String {
        self.overlay.lock().unwrap().realpath(path)
    }
    fn write_file(&self, path: &str, text: &str) -> std::result::Result<(), String> {
        let mut overlay = self.overlay.lock().unwrap();
        let path = overlay.key(path);
        overlay.files.insert(path, text.into());
        Ok(())
    }
    fn append_file(&self, path: &str, text: &str) -> std::result::Result<(), String> {
        let mut overlay = self.overlay.lock().unwrap();
        let path = overlay.key(path);
        overlay.files.entry(path).or_default().push_str(text);
        Ok(())
    }
    fn remove(&self, _: &str) -> std::result::Result<(), String> {
        Err("reference build cannot remove caller files".into())
    }
    fn chtimes(&self, _: &str, _: SystemTime, _: SystemTime) -> std::result::Result<(), String> {
        Ok(())
    }
}
struct System {
    fs: Arc<Outputs>,
    directory: String,
    libraries: String,
    start: Instant,
    run_external_code: bool,
}
impl tsrs_tsoptions::ParseConfigHost for System {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &str {
        &self.directory
    }
}
impl tsc::System for System {
    fn fs(&self) -> Arc<dyn FS> {
        Arc::clone(&self.fs) as Arc<dyn FS>
    }
    fn default_library_path(&self) -> &str {
        &self.libraries
    }
    fn get_current_directory(&self) -> &str {
        &self.directory
    }
    fn write(&self, _: &str) {}
    fn flush(&self) {}
    fn write_output_is_tty(&self) -> bool {
        false
    }
    fn get_environment_variable(&self, _: &str) -> Option<String> {
        None
    }
    fn spawn(
        &self,
        command: &[String],
        directory: &str,
        stderr: Option<Box<dyn std::io::Write + Send>>,
    ) -> std::result::Result<tsc::ReadWriteCloser, String> {
        if !self.run_external_code {
            return Err("content mappers require run_external_code".into());
        }
        tsrs_contentmapper::spawn_process(command, directory, stderr)
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn since_start(&self) -> Duration {
        self.start.elapsed()
    }
}
struct Owner(&'static build::Orchestrator);
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: API mode was enabled before the only build; diagnostics have
        // been copied and no compiler or task references escape this owner.
        unsafe { build::free_api_orchestrator(self.0) };
    }
}

pub fn compile(options: &BuildOptions, base: Arc<dyn FS>) -> Result<Arc<dyn FS>> {
    let region = tsrs_core::arena::Region::new_scratch(1 << 20);
    let _scope = region.enter_scratch();
    let outputs = Arc::new(Outputs {
        overlay: Mutex::new(host::DeclarationFs {
            base,
            files: Files::new(),
            hidden: BTreeSet::default(),
        }),
        timestamp: SystemTime::now(),
    });
    let system = Box::new(System {
        fs: Arc::clone(&outputs),
        directory: tsrs_core::tspath::get_directory_path(&options.config_path),
        libraries: options.default_library_path.clone(),
        start: Instant::now(),
        run_external_code: options.run_external_code,
    });
    let (config, errors) = tsrs_tsoptions::get_parsed_command_line_of_config_file(
        &options.config_path,
        Some(&tsrs_core::CompilerOptions {
            run_external_code: tsrs_core::bool_to_tristate(options.run_external_code),
            ..Default::default()
        }),
        None,
        &*system,
        None,
    );
    let directory = &system.directory;
    project::diagnostics(&errors, directory)?;
    let config = config.ok_or_else(|| Error("could not load reference configuration".into()))?;
    project::diagnostics(&config.get_config_file_parsing_diagnostics(), directory)?;
    if config.project_references().is_empty() {
        return Ok(Arc::clone(&outputs.overlay.lock().unwrap().base));
    }
    let mut command = tsrs_tsoptions::parse_build_command_line(
        std::slice::from_ref(&options.config_path),
        &*system,
    );
    project::diagnostics(&command.errors, directory)?;
    command.compiler_options.run_external_code =
        tsrs_core::bool_to_tristate(options.run_external_code);
    command.compiler_options.declaration = Tristate::True;
    command.compiler_options.emit_declaration_only = Tristate::True;
    command.compiler_options.no_emit = Tristate::False;
    command.compiler_options.no_check = Tristate::False;
    command.compiler_options.single_threaded = Tristate::True;
    // Generic caller filesystems need not provide timestamps. Force dependency
    // compilation into a fresh overlay rather than trusting stale disk outputs.

    // TODO: Establish dependency/output freshness for caller filesystems, then
    // remove forced rebuilding. See docs/DTS.md.
    command.build_options.force = Tristate::True;
    let command = P::new(command);
    {
        let system_ptr = &raw const *system;
        // SAFETY: the CLI requires a static System. Its Box stays at a stable
        // address until after Owner drops, including during panic unwinding.
        let system: &'static System = unsafe { &*system_ptr };
        let owner =
            Owner(build::new_orchestrator(build::Options { sys: system, command, testing: None }));
        owner.0.enable_api_mode();
        let result = owner.0.build_for_api(&options.config_path, true);
        project::diagnostics(&result.errors.unwrap_or_default(), directory)?;
        if result.status.is_some_and(|status| status != tsc::ExitStatus::Success) {
            return Err(Error("tsrs reference build failed".into()));
        }
    }
    Ok(outputs)
}
