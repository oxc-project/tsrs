// tsrs-only: the server side of a resident check server (daemon/mod.rs). One process per key (protocol.rs); it
// answers one request at a time on the thread below, and the others wait in the socket's queue.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rustc_hash::{FxHashMap, FxHashSet};
use tsrs_ast::{SourceFile, SourceFileParseOptions};
use tsrs_compiler::{CompilerHost, Program, ProgramOptions, ProgramRegions};
use tsrs_contentmapper as contentmapper;
use tsrs_core::P;
use tsrs_core::collections::SyncMap;
use tsrs_core::tspath::{self, Path};
use tsrs_tsoptions::{Mapper, ParsedCommandLine};
use tsrs_vfs::{FS, bundled};

use super::protocol::{self, Op, Paths, Response};
use super::recordfs::{self, Answers, Contents, ReadAnswer, RecordFs};
use crate::tsc::{ReadWriteCloser, System};
use crate::watchmem::Freer;

/// How long a server waits for a request before it exits (`TSRS_DAEMON_IDLE`, in seconds).
const DEFAULT_IDLE: Duration = Duration::from_mins(30);
/// Ids are 31-bit and only grow (`tsrs_ast::ids_handed_out`); a server stops well before they run out.
const ID_LIMIT: u64 = 1 << 30;

/// A line of the server's log (standard error), written with one system call: other servers of the key and the
/// free thread write to the same file, and `eprintln!` writes a formatted line in pieces.
macro_rules! log {
    ($($arg:tt)*) => {{
        let line = format!("tsrs daemon: {}\n", format_args!($($arg)*));
        let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
    }};
}

// ---- The system a request runs on ----

/// `sys::OsSys` with the output kept for the client and the request's file system.
struct DaemonSys {
    fs: Arc<CurrentFs>,
    out: Mutex<Vec<u8>>,
    tty: AtomicBool,
    cwd: String,
    default_library_path: String,
    start: Instant,
}

impl System for DaemonSys {
    fn fs(&self) -> Arc<dyn FS> {
        Arc::clone(&self.fs) as Arc<dyn FS>
    }

    fn default_library_path(&self) -> &str {
        &self.default_library_path
    }

    fn get_current_directory(&self) -> &str {
        &self.cwd
    }

    fn write(&self, text: &str) {
        self.out.lock().unwrap().extend_from_slice(text.as_bytes());
    }

    fn flush(&self) {}

    fn write_output_is_tty(&self) -> bool {
        // Relaxed: set by the request thread before the run that reads it.
        self.tty.load(Ordering::Relaxed)
    }

    fn get_environment_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn Write + Send>>,
    ) -> Result<ReadWriteCloser, String> {
        tsrs_contentmapper::spawn_process(command, dir, stderr)
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn since_start(&self) -> Duration {
        self.start.elapsed()
    }
}

/// The file system `DaemonSys::fs` hands out: the current request's (bundled libraries over `RecordFs`). A run
/// keeps what it is handed past its end (the configuration parse host is leaked, as in a cold run); keeping this
/// instead of the request's file system lets each request's recorded answers go once the next has replaced them.
#[derive(Default)]
struct CurrentFs(std::sync::RwLock<Option<Arc<dyn FS>>>);

impl CurrentFs {
    fn with<T>(&self, f: impl FnOnce(&dyn FS) -> T) -> T {
        f(&**self.0.read().unwrap().as_ref().expect("a request's file system"))
    }
}

impl FS for CurrentFs {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.with(|fs| fs.use_case_sensitive_file_names())
    }
    fn file_exists(&self, path: &str) -> bool {
        self.with(|fs| fs.file_exists(path))
    }
    fn read_file(&self, path: &str) -> Option<String> {
        self.with(|fs| fs.read_file(path))
    }
    fn write_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.with(|fs| fs.write_file(path, data))
    }
    fn append_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.with(|fs| fs.append_file(path, data))
    }
    fn remove(&self, path: &str) -> Result<(), String> {
        self.with(|fs| fs.remove(path))
    }
    fn chtimes(
        &self,
        path: &str,
        a_time: std::time::SystemTime,
        m_time: std::time::SystemTime,
    ) -> Result<(), String> {
        self.with(|fs| fs.chtimes(path, a_time, m_time))
    }
    fn directory_exists(&self, path: &str) -> bool {
        self.with(|fs| fs.directory_exists(path))
    }
    fn get_accessible_entries(&self, path: &str) -> tsrs_vfs::Entries {
        self.with(|fs| fs.get_accessible_entries(path))
    }
    fn stat(&self, path: &str) -> Option<tsrs_vfs::FileInfo> {
        self.with(|fs| fs.stat(path))
    }
    fn realpath(&self, path: &str) -> String {
        self.with(|fs| fs.realpath(path))
    }
}

fn daemon_sys() -> &'static DaemonSys {
    static SYS: OnceLock<DaemonSys> = OnceLock::new();
    SYS.get_or_init(|| DaemonSys {
        fs: Arc::default(),
        out: Mutex::new(Vec::new()),
        tty: AtomicBool::new(false),
        cwd: std::env::current_dir()
            .map(|cwd| tspath::normalize_path(&cwd.to_string_lossy()))
            .unwrap_or_default(),
        default_library_path: bundled::default_lib_path(),
        start: Instant::now(),
    })
}

// ---- What requests keep: the parsed files ----

#[derive(Clone)]
struct CachedFile {
    file: P<SourceFile>,
    opts: SourceFileParseOptions,
    // `None`: a library file embedded in the binary.
    read: Option<ReadAnswer>,
}

/// The `Resident` of the server process (`perform_compilation` asks for it through `daemon::resident`).
struct ResidentState {
    cache: SyncMap<Path, CachedFile>,
    run: Mutex<Run>,
}

/// What `perform_compilation` handed the server during the current request.
#[derive(Default)]
struct Run {
    fs: Option<Arc<RecordFs>>,
    regions: Option<Arc<ProgramRegions>>,
    checked: Option<(&'static Program, P<ParsedCommandLine>)>,
}

fn state() -> &'static ResidentState {
    static STATE: OnceLock<ResidentState> = OnceLock::new();
    STATE.get_or_init(|| ResidentState { cache: SyncMap::default(), run: Mutex::default() })
}

impl super::Resident for ResidentState {
    fn host(&self, host: Arc<dyn CompilerHost>) -> Arc<dyn CompilerHost> {
        let fs = self.run.lock().unwrap().fs.clone().expect("a request's file system");
        Arc::new(ResidentHost { inner: host, state: state(), fs })
    }

    fn program_options(&self, options: &mut ProgramOptions) {
        // As watch mode: the program's checkers, their diagnostics and the file loader's data live in regions that
        // are freed with the program once the request is answered (`Server::keep`).
        options.checker_regions = true;
        options.checker_diagnostic_regions = true;
        let regions = Arc::new(ProgramRegions::default());
        options.regions = Some(Arc::clone(&regions));
        self.run.lock().unwrap().regions = Some(regions);
    }

    fn checked(&self, program: &'static Program, config: P<ParsedCommandLine>) {
        self.run.lock().unwrap().checked = Some((program, config));
    }
}

/// The compiler host of a request: the cold run's, with the files parsed for earlier requests handed out again while
/// their text is unchanged (watch.rs `WatchCompilerHost`, with the request's validated reads instead of a
/// modification time). A new file is parsed and bound in the thread's arena, as in a cold run: in per-file regions
/// (watchmem.rs) the checks were slower, and the arena's huge pages are what a cold run's checkers read. The old
/// version of an edited file is not freed; the memory limit bounds what accumulates (`Server::over_limit`).
struct ResidentHost {
    inner: Arc<dyn CompilerHost>,
    state: &'static ResidentState,
    fs: Arc<RecordFs>,
}

impl CompilerHost for ResidentHost {
    fn fs(&self) -> &dyn FS {
        self.inner.fs()
    }

    fn default_library_path(&self) -> &str {
        self.inner.default_library_path()
    }

    fn get_current_directory(&self) -> &str {
        self.inner.get_current_directory()
    }

    fn trace(&self, msg: &'static tsrs_diagnostics::Message, args: &[&dyn std::fmt::Display]) {
        self.inner.trace(msg, args);
    }

    fn get_source_file(&self, opts: SourceFileParseOptions) -> Option<P<SourceFile>> {
        if let Some(cached) = self.state.cache.load(&opts.path)
            && cached.opts == opts
        {
            match cached.read {
                None => return Some(cached.file),
                Some(read) => {
                    if self.fs.current_read(&opts.file_name).is_some_and(|now| now.same_text(&read))
                    {
                        return Some(cached.file);
                    }
                }
            }
        }
        let path = opts.path.clone();
        let embedded = bundled::embedded_file(&opts.file_name).is_some();
        let file_name = opts.file_name.clone();
        let parse_opts = opts.clone();
        let file = {
            // Out of the file loader's region, which is freed with the program; bound here too, for the same reason.
            let _arena = tsrs_core::arena::enter_thread_arena();
            let file = if embedded {
                self.inner.get_source_file(parse_opts)
            } else {
                let fingerprint = recordfs::fingerprint(&file_name);
                recordfs::source_read(fingerprint, || self.inner.get_source_file(parse_opts))
            };
            if let Some(file) = file
                && !file.is_bound()
            {
                tsrs_binder::bind_source_file(file);
            }
            file
        };
        match file {
            Some(file) => {
                let read = if embedded { None } else { self.fs.current_read(&file_name) };
                if embedded || read.is_some() {
                    self.state.cache.store(path, CachedFile { file, opts, read });
                }
            }
            None => self.state.cache.delete(&path),
        }
        file
    }

    fn get_content_mapped_source_files(
        &self,
        parse_options: SourceFileParseOptions,
        mapper: &Mapper,
    ) -> Result<contentmapper::SourceFiles, contentmapper::Error> {
        // Not kept: a program with content mappers is not kept either (`Server::check`).
        self.inner.get_content_mapped_source_files(parse_options, mapper)
    }

    fn content_mapper_project(&self) -> Option<Arc<dyn contentmapper::Project>> {
        self.inner.content_mapper_project()
    }

    fn get_resolved_project_reference(
        &self,
        file_name: &str,
        path: Path,
    ) -> Option<P<ParsedCommandLine>> {
        self.inner.get_resolved_project_reference(file_name, path)
    }
}

// ---- The server ----

/// The last request that ran, for the next one to replay or to validate against.
struct Last {
    record: Arc<Answers>,
    stdout: Vec<u8>,
    status: u8,
    tty: bool,
    // The configuration files the run read (the project's and those it extends): a change restarts the server, as
    // the configuration sets process-wide state (`declaration_files_unchecked`).
    config_files: FxHashSet<String>,
}

/// The process's memory once a request's program is freed (`process_memory`), for the limit (`Server::over_limit`).
#[derive(Default)]
struct Memory {
    first: AtomicUsize,
    latest: AtomicUsize,
}

struct Server {
    args: Vec<String>,
    material: String,
    paths: Paths,
    lock: Option<std::fs::File>,
    listener: UnixListener,
    idle: Duration,
    max_memory: Option<usize>,
    freer: Freer,
    contents: Arc<Contents>,
    last: Option<Last>,
    // `declaration_files_unchecked` of the first kept run.
    declarations_unchecked: Option<bool>,
    memory: Arc<Memory>,
    requests: usize,
}

enum After {
    Keep,
    Stop,
}

/// `tsrs --daemon-serve <args>`: the server for `args`, started by a client (client.rs `start_server`) with the
/// client's working directory and environment and its log as standard error. Exits 0 when another server has the
/// key's lock.
pub fn serve(args: Vec<String>) -> u8 {
    close_inherited_fds();
    let Some(material) = protocol::key_material(&args) else { return 1 };
    let Some(paths) = Paths::new(&material) else { return 1 };
    let Some(lock) = take_lock(&paths) else { return 0 };
    let _ = std::fs::remove_file(&paths.socket);
    let Ok(listener) = UnixListener::bind(&paths.socket) else { return 1 };
    let idle = std::env::var("TSRS_DAEMON_IDLE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(DEFAULT_IDLE, Duration::from_secs_f64);
    let max_memory = std::env::var("TSRS_DAEMON_MAX_MEMORY")
        .ok()
        .and_then(|v| tsrs_compiler::parse_memory_size(&v));
    log!("pid {}, serving {:?}", std::process::id(), args);
    let _ = super::RESIDENT.set(state());
    // Lazily filled data of the kept files goes to their owner (the thread arenas), not to the region of the checker
    // that fills it, which is freed after the request.
    tsrs_core::arena::route_by_owner();
    // Checking recurses deeply: requests run on a thread with a large stack, like the command line's.
    std::thread::Builder::new()
        .name("tsrs".to_string())
        .stack_size(512 << 20)
        .spawn(move || {
            let mut server = Server {
                args,
                material,
                paths,
                lock: Some(lock),
                listener,
                idle,
                max_memory,
                freer: Freer::new(false),
                contents: Arc::default(),
                last: None,
                declarations_unchecked: None,
                memory: Arc::default(),
                requests: 0,
            };
            server.run();
            0
        })
        .expect("failed to spawn a thread")
        .join()
        .unwrap_or(1)
}

/// The key's lock, held for the server's life. Waits while a stopping server still holds it; `None` once a live server
/// answers on the socket (it got there first).
fn take_lock(paths: &Paths) -> Option<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&paths.lock)
        .ok()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if file.try_lock().is_ok() {
            return Some(file);
        }
        if UnixStream::connect(&paths.socket).is_ok() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

/// Closes the descriptors the client inherited from its parent: a server that kept, say, a hook runner's pipe open
/// would keep the runner waiting for it to close. Runs first, before this process opens anything.
fn close_inherited_fds() {
    let dir = if cfg!(target_os = "linux") { "/proc/self/fd" } else { "/dev/fd" };
    let fds: Vec<i32> = std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default();
    for fd in fds {
        if fd > 2 {
            // SAFETY: closes a descriptor no Rust object owns yet (or the listing's own, already closed: EBADF).
            unsafe { libc::close(fd) };
        }
    }
}

impl Server {
    fn run(&mut self) {
        let mut deadline = Instant::now() + self.idle;
        let mut identity_checked = Instant::now();
        loop {
            if let Some(stream) = self.wait(Duration::from_secs(1)) {
                if matches!(self.serve_connection(stream), After::Stop) {
                    break;
                }
                deadline = Instant::now() + self.idle;
                continue;
            }
            if Instant::now() >= deadline {
                log!("idle for {:?}, exiting", self.idle);
                break;
            }
            if self.over_limit() {
                break;
            }
            // A rebuilt or upgraded binary has another key; its server is a new process, and this one would only
            // hold memory until its idle timeout.
            if identity_checked.elapsed() >= Duration::from_secs(60) {
                identity_checked = Instant::now();
                if protocol::key_material(&self.args).as_deref() != Some(self.material.as_str()) {
                    log!("the binary changed, exiting");
                    break;
                }
            }
        }
        self.shut_down();
    }

    /// The next connection, or `None` after `timeout` without one.
    fn wait(&self, timeout: Duration) -> Option<UnixStream> {
        let mut fds =
            libc::pollfd { fd: self.listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: one valid pollfd for the listener this server owns.
        if unsafe { libc::poll(&raw mut fds, 1, ms) } <= 0 {
            return None;
        }
        self.listener.accept().ok().map(|(stream, _)| stream)
    }

    /// Stops accepting: removes the socket and gives up the lock, so that a new server can start while this one
    /// exits, then answers the connections already queued with `Restart`.
    fn shut_down(&mut self) {
        let _ = std::fs::remove_file(&self.paths.socket);
        self.lock = None;
        let _ = self.listener.set_nonblocking(true);
        while let Ok((mut stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            if protocol::read_request(&mut stream).is_ok() {
                let _ = protocol::write_response(&mut stream, &Response::Restart);
            }
        }
    }

    fn serve_connection(&mut self, mut stream: UnixStream) -> After {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let Ok(request) = protocol::read_request(&mut stream) else { return After::Keep };
        if request.op == Op::Stop {
            self.shut_down();
            let _ = protocol::write_response(&mut stream, &Response::Stopped);
            log!("stopped");
            return After::Stop;
        }
        if request.material != self.material {
            // Not this server's command (or an older client's protocol): the client runs it itself.
            let _ = protocol::write_response(&mut stream, &Response::Fallback);
            return After::Keep;
        }
        self.requests += 1;
        let start = Instant::now();
        let (response, after, what) = self.check(request.stdout_is_tty);
        if matches!(response, Response::Restart) {
            self.shut_down();
        }
        let _ = protocol::write_response(&mut stream, &response);
        drop(stream);
        log!("request {}: {what}; answered after {:.1?}", self.requests, start.elapsed());
        after
    }

    /// Answers one request: replays the last output when no answer the last run got from the file system changed,
    /// otherwise runs the command again on the validated answers, reusing the parsed files whose text is unchanged.
    fn check(&mut self, tty: bool) -> (Response, After, String) {
        let mut known = Arc::new(Answers::default());
        let mut changes = String::new();
        if self.last.is_some() && self.over_limit() {
            return (Response::Restart, After::Stop, "restart (limit)".to_string());
        }
        if let Some(last) = &mut self.last {
            let validated = Instant::now();
            let validation = recordfs::validate(&last.record, &self.contents);
            let validate_time = validated.elapsed();
            if let Some(config) = validation.changed.iter().find(|p| last.config_files.contains(*p))
            {
                return (Response::Restart, After::Stop, format!("restart ({config} changed)"));
            }
            if validation.changed.is_empty() && last.tty == tty {
                let response =
                    Response::Output { status: last.status, stdout: last.stdout.clone() };
                let what = format!(
                    "replay ({} queries validated in {validate_time:.1?})",
                    last.record.len()
                );
                // The same answers with newer stamps: a file that was too recently changed to trust may be now.
                last.record = Arc::new(validation.known);
                return (response, After::Keep, what);
            }
            changes = format!(
                "{} changed (first: {}), {} queries validated in {validate_time:.1?}, ",
                validation.changed.len(),
                validation.changed.first().map_or("none", String::as_str),
                last.record.len(),
            );
            known = Arc::new(validation.known);
        }
        let sys = daemon_sys();
        let fs = Arc::new(RecordFs::new(known, Arc::clone(&self.contents)));
        *sys.fs.0.write().unwrap() = Some(Arc::new(bundled::wrap_fs(Arc::clone(&fs))));
        sys.out.lock().unwrap().clear();
        // Relaxed: read by this run only, which starts below on this thread.
        sys.tty.store(tty, Ordering::Relaxed);
        let st = state();
        *st.run.lock().unwrap() = Run { fs: Some(Arc::clone(&fs)), ..Run::default() };
        let ran = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::execute::command_line(sys, self.args.clone())
        }));
        let run_time = ran.elapsed();
        let stdout = std::mem::take(&mut *sys.out.lock().unwrap());
        *sys.fs.0.write().unwrap() = None;
        let run = std::mem::take(&mut *st.run.lock().unwrap());
        let Ok(result) = result else {
            // A panic: the client runs the command itself and reports it as a cold run would.
            return (Response::Fallback, After::Stop, "panicked".to_string());
        };
        let status = result.status as u8;
        let Some((program, config)) = run.checked else {
            // Not a `--noEmit` compilation of one project (an incremental build, `--version`, a config error):
            // answered, but there is nothing to keep.
            let response = Response::Output { status, stdout };
            return (response, After::Stop, "ran, not resident (not a single check)".to_string());
        };
        let options = config.compiler_options().unwrap();
        let unchecked = crate::execute::declaration_files_unchecked(config);
        if *self.declarations_unchecked.get_or_insert(unchecked) != unchecked {
            // Lazy declaration parsing (`enable_lazy_dts`) is process-wide and was set for another configuration.
            return (
                Response::Fallback,
                After::Stop,
                "fallback (configuration changed)".to_string(),
            );
        }
        let keep = options.no_emit.is_true()
            && !options.diagnostics_enabled()
            && config.content_mappers().is_empty();
        if !keep {
            let response = Response::Output { status, stdout };
            return (
                response,
                After::Stop,
                "ran, not resident (emits or reports timings)".to_string(),
            );
        }
        let record = fs.record();
        self.prune_contents(&record);
        let config_files = config
            .config_file
            .map(|f| f.source_file.file_name().to_string())
            .into_iter()
            .chain(config.extended_source_files())
            .collect();
        let what = format!(
            "{changes}ran in {run_time:.1?} ({} files, {} queries)",
            program.get_source_files().len(),
            record.len()
        );
        self.last = Some(Last { record, stdout: stdout.clone(), status, tty, config_files });
        self.keep(program, run.regions);
        (Response::Output { status, stdout }, After::Keep, what)
    }

    /// Drops the kept text of the files the last run did not read.
    fn prune_contents(&self, record: &Answers) {
        let mut stale = Vec::new();
        self.contents.range(|path, _| {
            if !record.has_read(path) {
                stale.push(path.clone());
            }
            true
        });
        for path in &stale {
            self.contents.delete(path);
        }
    }

    /// After a kept run: keeps the program's files, and frees the rest on the free thread (watchmem.rs `Freer`) once
    /// the answer is out: the program and its checkers, and the file loader's regions.
    fn keep(&self, program: &'static Program, regions: Option<Arc<ProgramRegions>>) {
        let st = state();
        let files: &FxHashMap<Path, P<SourceFile>> = program.files_by_path();
        let mut gone = Vec::new();
        st.cache.range(|path, _| {
            if !files.contains_key(path) {
                gone.push(path.clone());
            }
            true
        });
        for path in &gone {
            st.cache.delete(path);
        }
        let memory = Arc::clone(&self.memory);
        let request = self.requests;
        self.freer.run(move || {
            // SAFETY: nothing refers to the program any more: the request that used it has been answered, its
            // checkers are idle, and it shares nothing with another program (each request builds its own).
            unsafe { tsrs_compiler::free_unshared_program(program) };
            // The program pointed into them; the kept files do not (watch mode frees the same way).
            drop(regions);
            // The allocator gives back what the request freed, here and on the worker pool's threads (as watch mode
            // does on macOS, `Freer::after_cycle`): otherwise it stays resident, about 100 MiB more after each request,
            // and the memory limit would stop the server for memory it does not use.
            tsrs_compiler::worker_pool().broadcast(|_| tsrs_core::memsplit::heap_collect(true));
            tsrs_core::memsplit::heap_collect(true);
        });
        self.freer.run(move || {
            let now = tsrs_core::memsplit::process_memory();
            // Relaxed (both): readings for a limit, published to the request thread by nothing but themselves.
            let _ = memory.first.compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
            memory.latest.store(now, Ordering::Relaxed);
            log!("request {request}: {} MiB once freed", now >> 20);
        });
    }

    /// Whether the server should stop: the process's memory after its last free passed the limit
    /// (`TSRS_DAEMON_MAX_MEMORY`, else twice what it was after the first request, and at least 1 GiB more), or the
    /// ids are half used.
    fn over_limit(&self) -> bool {
        // Relaxed (both): see `keep`.
        let first = self.memory.first.load(Ordering::Relaxed);
        let latest = self.memory.latest.load(Ordering::Relaxed);
        let limit = self.max_memory.unwrap_or_else(|| (2 * first).max(first + (1 << 30)));
        if first > 0 && latest > limit {
            log!("{latest} bytes after the last request, over {limit}: exiting");
            return true;
        }
        if tsrs_ast::ids_handed_out() > ID_LIMIT {
            log!("half of the id space used: exiting");
            return true;
        }
        false
    }
}
