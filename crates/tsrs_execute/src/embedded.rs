//! Scoped ownership for a one-shot CLI check inside a long-lived process.
//!
//! The compiler keeps Go-shaped static references. They stay within this invocation: all worker jobs
//! join before the driver returns, diagnostics must be copied by the sink, and programs are destroyed
//! before their regions. The ordinary CLI keeps its process-lifetime allocation policy.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsrs_ast::{Diagnostic, SourceFile, SourceFileParseOptions};
use tsrs_compiler::{CompilerHost, Program, ProgramOptions, ProgramRegions};
use tsrs_contentmapper::{Mapper, Project, SourceFiles};
use tsrs_core::{P, tspath};
use tsrs_diagnostics::Message;
use tsrs_tsoptions::ParsedCommandLine;
use tsrs_vfs::FS;

use crate::tsc::{ExitStatus, System};
use crate::watchmem::FileRegions;

/// Compiler state retained by one embedded invocation. Only `check_project` creates this scope.
#[derive(Default)]
pub struct CompilationScope {
    programs: Mutex<Vec<&'static Program>>,
    pub(crate) regions: Arc<ProgramRegions>,
    files: Arc<FileRegions>,
}

impl CompilationScope {
    pub(crate) fn options(&self, options: &mut ProgramOptions) {
        options.checker_regions = true;
        options.checker_diagnostic_regions = true;
        options.regions = Some(Arc::clone(&self.regions));
    }

    pub(crate) fn retain(&self, program: &'static Program) {
        self.programs.lock().unwrap().push(program);
    }

    pub(crate) fn host(&self, inner: Arc<dyn CompilerHost>) -> Arc<dyn CompilerHost> {
        Arc::new(ScopedHost { inner, files: Arc::clone(&self.files) })
    }
}

impl Drop for CompilationScope {
    fn drop(&mut self) {
        for program in
            self.programs.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).drain(..)
        {
            // SAFETY: the synchronous driver and its scoped workers have returned or unwound. These
            // are full builds, never reused programs; the sink copied every result before returning.
            unsafe { tsrs_compiler::free_unshared_program(program) };
        }
    }
}

static CHECK: Mutex<()> = Mutex::new(());

/// Runs exactly `--noEmit -p project`. The sink must retain owned diagnostics, never arena pointers.
/// Calls are serialized because the CLI's diagnostic counters and compiler configuration are global.
pub fn check_project<S: System + 'static>(system: S, project: &str) -> ExitStatus {
    let _serial = CHECK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    tsrs_core::phases::reset();
    let scope = Box::new(CompilationScope::default());
    // SAFETY: `scope` stays at this address until the driver and all workers have finished. No
    // reference escapes this function; its destructor frees programs before their allocation regions.
    let scope_ref: &'static CompilationScope = unsafe { &*std::ptr::from_ref(&*scope) };
    let allocation = scope.regions.enter();
    let sys = tsrs_core::alloc(ScopedSystem { inner: system, scope: scope_ref });
    tsrs_compiler::bind_in_file_regions();
    let result =
        crate::execute::command_line(sys, vec!["--noEmit".into(), "-p".into(), project.into()]);
    debug_assert!(result.watcher.is_none());
    drop(allocation);
    // `scope` drops even when compilation unwinds, after `allocation` releases its region lock.
    result.status
}

struct ScopedSystem<S> {
    inner: S,
    scope: &'static CompilationScope,
}

impl<S: System> System for ScopedSystem<S> {
    fn compilation_scope(&self) -> Option<&CompilationScope> {
        Some(self.scope)
    }
    fn fs(&self) -> Arc<dyn FS> {
        self.inner.fs()
    }
    fn default_library_path(&self) -> &str {
        self.inner.default_library_path()
    }
    fn get_current_directory(&self) -> &str {
        self.inner.get_current_directory()
    }
    fn write(&self, text: &str) {
        self.inner.write(text);
    }
    fn write_error(&self, text: &str) {
        self.inner.write_error(text);
    }
    fn flush(&self) {
        self.inner.flush();
    }
    fn write_output_is_tty(&self) -> bool {
        self.inner.write_output_is_tty()
    }
    fn get_environment_variable(&self, name: &str) -> Option<String> {
        self.inner.get_environment_variable(name)
    }
    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn Write + Send>>,
    ) -> Result<crate::tsc::ReadWriteCloser, String> {
        self.inner.spawn(command, dir, stderr)
    }
    fn now(&self) -> Instant {
        self.inner.now()
    }
    fn since_start(&self) -> Duration {
        self.inner.since_start()
    }
    fn diagnostic_sink(&self) -> Option<&(dyn Fn(P<Diagnostic>) + Sync)> {
        self.inner.diagnostic_sink()
    }
}

struct ScopedHost {
    inner: Arc<dyn CompilerHost>,
    files: Arc<FileRegions>,
}

impl CompilerHost for ScopedHost {
    fn fs(&self) -> &dyn FS {
        self.inner.fs()
    }
    fn default_library_path(&self) -> &str {
        self.inner.default_library_path()
    }
    fn get_current_directory(&self) -> &str {
        self.inner.get_current_directory()
    }
    fn trace(&self, message: &'static Message, args: &[&dyn std::fmt::Display]) {
        self.inner.trace(message, args);
    }
    fn get_source_file(&self, options: SourceFileParseOptions) -> Option<P<SourceFile>> {
        self.files.parse(0, || self.inner.get_source_file(options))
    }
    fn get_content_mapped_source_files(
        &self,
        options: SourceFileParseOptions,
        mapper: &Mapper,
    ) -> Result<SourceFiles, tsrs_contentmapper::Error> {
        self.files
            .parse_content_mapped(0, || self.inner.get_content_mapped_source_files(options, mapper))
    }
    fn content_mapper_project(&self) -> Option<Arc<dyn Project>> {
        self.inner.content_mapper_project()
    }
    fn get_resolved_project_reference(
        &self,
        file_name: &str,
        path: tspath::Path,
    ) -> Option<P<ParsedCommandLine>> {
        self.inner.get_resolved_project_reference(file_name, path)
    }
}
