mod snapshot;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsrs_core::tspath;
use tsrs_execute::{
    execute,
    tsc::{ExitStatus, System},
};
use tsrs_vfs::{FS, bundled, osvfs};

use snapshot::RecordingFs;

pub const PROJECTS: &[&str] = &["Compiler-Unions", "xstate-main", "webpack", "drizzle-orm"];

pub fn project_config(name: &str) -> &'static str {
    match name {
        "Compiler-Unions" => "bench/.work/suite/cases/solutions/Compiler-Unions/tsconfig.json",
        "xstate-main" => "bench/.work/solutions/xstate-main/tsconfig.json",
        "webpack" => "bench/.work/solutions/webpack/tsconfig.json",
        "drizzle-orm" => "bench/.work/solutions/drizzle-orm/tsconfig.json",
        _ => panic!("unknown TSRS_BENCH_PROJECT {name:?}; expected one of {}", PROJECTS.join(", ")),
    }
}

fn workspace_directory() -> String {
    // cargo-codspeed runs from the package directory. The walltime binary is built on a different
    // runner, so CARGO_MANIFEST_DIR points to a path that does not exist when it executes.
    let cwd = std::env::current_dir().expect("current directory");
    let workspace = cwd
        .ancestors()
        .find(|path| path.join("bench/projects.json").is_file())
        .expect("workspace directory");
    tspath::normalize_slashes(&workspace.to_string_lossy())
}

#[derive(Debug, PartialEq, Eq)]
pub struct Compilation {
    status: ExitStatus,
    output: String,
}

impl Compilation {
    pub fn assert_completed(&self) {
        assert!(
            matches!(
                self.status,
                ExitStatus::Success
                    | ExitStatus::DiagnosticsPresentOutputsSkipped
                    | ExitStatus::DiagnosticsPresentOutputsGenerated
            ),
            "benchmark compilation failed: {self:?}"
        );
    }
}

pub struct PreparedProject {
    cwd: String,
    lib_path: String,
    args: Vec<String>,
    fs: Arc<dyn FS>,
    expected: Compilation,
}

impl PreparedProject {
    pub fn load(name: &str) -> Self {
        let cwd = workspace_directory();
        let config = tspath::combine_paths(&cwd, &[project_config(name)]);
        Self::capture(
            cwd,
            &config,
            bundled::default_lib_path(),
            Arc::new(bundled::wrap_fs(osvfs::fs())),
        )
    }

    fn capture(cwd: String, config: &str, lib_path: String, fs: Arc<dyn FS>) -> Self {
        assert!(
            fs.file_exists(config),
            "missing benchmark config {config:?}; run node bench/bench.mjs prepare first"
        );
        let recording = Arc::new(RecordingFs::new(fs));
        let args: Vec<String> = [
            "-p",
            config,
            "--noEmit",
            "--incremental",
            "false",
            "--singleThreaded",
            "--pretty",
            "false",
            // Statistics read /proc on Linux, and their timings would differ between capture and replay.
            "--diagnostics",
            "false",
            "--extendedDiagnostics",
            "false",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let sys = BenchmarkSystem::new(&cwd, &lib_path, Arc::clone(&recording) as Arc<dyn FS>);
        let expected = sys.compile(args.clone());
        expected.assert_completed();
        let fs = Arc::new(recording.freeze());
        Self { cwd, lib_path, args, fs, expected }
    }

    pub fn invocation(&self) -> (&'static BenchmarkSystem, Vec<String>) {
        (BenchmarkSystem::new(&self.cwd, &self.lib_path, Arc::clone(&self.fs)), self.args.clone())
    }

    pub fn verify(&self, result: &Compilation) {
        assert_eq!(
            result, &self.expected,
            "memory-only compilation differs from filesystem capture"
        );
    }
}

/// Disk-backed compilation with the CLI's default checker count, for walltime and peak memory.
/// Call once in a fresh process so earlier compilations cannot inflate its physical-memory peak.
pub fn default_invocation(name: &str) -> (&'static BenchmarkSystem, Vec<String>) {
    let cwd = workspace_directory();
    let config = tspath::combine_paths(&cwd, &[project_config(name)]);
    let fs = Arc::new(bundled::wrap_fs(osvfs::fs()));
    assert!(
        fs.file_exists(&config),
        "missing benchmark config {config:?}; run pnpm bench prepare first"
    );
    let args = [
        "-p",
        &config,
        "--noEmit",
        "--incremental",
        "false",
        "--pretty",
        "false",
        "--diagnostics",
        "false",
        "--extendedDiagnostics",
        "false",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let sys = BenchmarkSystem::new(&cwd, &bundled::default_lib_path(), fs);
    (sys, args)
}

pub struct BenchmarkSystem {
    fs: Arc<dyn FS>,
    cwd: String,
    lib_path: String,
    output: Mutex<String>,
    start: Instant,
}

impl BenchmarkSystem {
    // The command-line driver keeps its System and program objects for the process lifetime.
    fn new(cwd: &str, lib_path: &str, fs: Arc<dyn FS>) -> &'static Self {
        Box::leak(Box::new(Self {
            fs,
            cwd: cwd.to_string(),
            lib_path: lib_path.to_string(),
            output: Mutex::default(),
            start: Instant::now(),
        }))
    }

    pub fn compile(&'static self, args: Vec<String>) -> Compilation {
        let result = execute::command_line(self, args);
        assert!(result.watcher.is_none(), "watch mode is not a benchmark compilation");
        Compilation {
            status: result.status,
            output: std::mem::take(&mut *self.output.lock().unwrap()),
        }
    }
}

impl System for BenchmarkSystem {
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
        self.output.lock().unwrap().push_str(text);
    }

    fn write_error(&self, text: &str) {
        self.write(text);
    }

    fn flush(&self) {}

    fn write_output_is_tty(&self) -> bool {
        false
    }

    fn get_environment_variable(&self, _name: &str) -> Option<String> {
        None
    }

    fn spawn(
        &self,
        command: &[String],
        _dir: &str,
        _stderr: Option<Box<dyn std::io::Write + Send>>,
    ) -> Result<tsrs_execute::tsc::ReadWriteCloser, String> {
        panic!("benchmark cannot spawn external processes: {command:?}")
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn since_start(&self) -> Duration {
        self.start.elapsed()
    }
}
