use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsrs_ast::Diagnostic;
use tsrs_core::P;
use tsrs_execute::tsc::System;
use tsrs_vfs::{FS, bundled, osvfs};

use crate::CheckResult;

#[derive(Default)]
struct Output {
    diagnostics: Vec<serde_json::Value>,
    stdout: String,
    stderr: String,
}

struct NativeSystem {
    cwd: String,
    fs: Arc<dyn FS>,
    lib_path: String,
    start: Instant,
    output: Arc<Mutex<Output>>,
    sink: Box<dyn Fn(P<Diagnostic>) + Send + Sync>,
}

pub(crate) fn check(cwd: &str, project: &str) -> Result<CheckResult, String> {
    let output = Arc::new(Mutex::new(Output::default()));
    let diagnostics = Arc::clone(&output);
    let system = NativeSystem {
        cwd: tsrs_core::tspath::normalize_path(cwd),
        fs: Arc::new(bundled::wrap_fs(osvfs::fs())),
        lib_path: bundled::default_lib_path(),
        start: Instant::now(),
        output: Arc::clone(&output),
        sink: Box::new(move |diagnostic| {
            // Convert while the source file and checker regions are alive. Only owned JSON crosses
            // the invocation boundary, including source lines and related diagnostics.
            let value = tsrs_api::diagnostics::diagnostic_response(&diagnostic);
            let json = tsrs_core::json::marshal(&value).expect("diagnostic is JSON");
            diagnostics
                .lock()
                .unwrap()
                .diagnostics
                .push(serde_json::from_str(&json).expect("serialized diagnostic is JSON"));
        }),
    };
    let exit_code = tsrs_execute::embedded::check_project(system, project) as i32;
    let output = Arc::try_unwrap(output)
        .map_err(|_| "compiler retained its output after checking".to_string())?
        .into_inner()
        .map_err(|error| error.to_string())?;
    Ok(CheckResult {
        exit_code,
        diagnostics: output.diagnostics,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

impl System for NativeSystem {
    fn fs(&self) -> Arc<dyn FS> {
        Arc::clone(&self.fs)
    }
    fn default_library_path(&self) -> &str {
        &self.lib_path
    }
    fn get_current_directory(&self) -> &str {
        &self.cwd
    }
    fn write(&self, text: &str) {
        self.output.lock().unwrap().stdout.push_str(text);
    }
    fn write_error(&self, text: &str) {
        self.output.lock().unwrap().stderr.push_str(text);
    }
    fn flush(&self) {}
    fn write_output_is_tty(&self) -> bool {
        false
    }
    fn get_environment_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn Write + Send>>,
    ) -> Result<tsrs_execute::tsc::ReadWriteCloser, String> {
        tsrs_contentmapper::spawn_process(command, dir, stderr)
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn since_start(&self) -> Duration {
        self.start.elapsed()
    }
    fn diagnostic_sink(&self) -> Option<&(dyn Fn(P<Diagnostic>) + Sync)> {
        Some(&*self.sink)
    }
}
