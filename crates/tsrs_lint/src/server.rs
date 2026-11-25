//! `tsrs-lint server`: a long-lived linter for oxlint's language server.
//!
//! The headless mode builds every program from scratch for each request. The server keeps a project session instead
//! (`tsrs_project::Session`, the one `tsrs --lsp` uses): it holds the editor's unsaved text of open files, reuses
//! programs across edits and keeps checkers warm, so a lint request after an edit only pays for what changed.
//!
//! Frames use the headless framing in both directions (`protocol.rs`): a little-endian `u32` JSON length, a `u8`
//! message type, then the UTF-8 JSON. The client sends:
//!
//! | type | name | JSON |
//! | --- | --- | --- |
//! | 16 | `DidOpen` | `{"path", "version", "text"}` |
//! | 17 | `DidChange` | `{"path", "version", "text"}` (the whole text) |
//! | 18 | `DidClose` | `{"path"}` |
//! | 19 | `FilesChanged` | `{"changes": [{"path", "kind"}]}` (LSP `FileChangeType`: 1 created, 2 changed, 3 deleted) |
//! | 20 | `Lint` | `{"id", "configs": [{"file_paths", "rules"}], "fix", "fix_suggestions"}` |
//! | 21 | `Shutdown` | `{}` |
//! | 22 | `Hello` | `{"protocol_version"}` |
//! | 23 | `Cancel` | `{"id"}` |
//!
//! The server sends:
//!
//! | type | name | JSON |
//! | --- | --- | --- |
//! | 0 | `Error` | `{"id", "error"}`, plus `"canceled": true` for a canceled request (see below) |
//! | 1 | `Diagnostic` | the headless diagnostic with the request's `"id"` first |
//! | 3 | `Done` | `{"id"}` |
//! | 4 | `Hello` | `{"protocol_version", "version"}` (the tsrs-lint version) |
//!
//! A fatal error has a null `id` and is the last frame: the server writes nothing after it, not even the terminal
//! frames of queued or running requests, and exits with status 1. A canceled request's error is
//! `{"id", "error": "request canceled", "canceled": true}`.
//!
//! The first frame must be the client's `Hello`. The server answers with its own, then exits with a fatal error if the
//! client's `protocol_version` is not [`PROTOCOL_VERSION`]; any other first frame is a fatal error. After the
//! handshake, frames of unknown types (and another `Hello`) are skipped, so a newer client can add notifications.
//! A malformed notification is a fatal error, as the client's state and the server's would diverge.
//!
//! Notifications are applied in order, on the thread that reads frames, before any later lint request takes a
//! snapshot. A lint request lints every file in `configs`, as the headless mode does; compiler diagnostics are not
//! reported. Each request gets exactly one terminal frame, unless a fatal error comes first: its diagnostic frames,
//! each carrying its `id`, then `Done {"id"}`; or an error frame carrying its `id` (a request that cannot be parsed,
//! a panic, or a cancellation), in which case its diagnostics are dropped. A request's frames are written at once,
//! so requests never interleave.
//!
//! Lint requests wait in a queue for a fixed pool of worker threads, first in, first out. A running request loads
//! its files' projects, then takes one snapshot for all of its files. There are as many workers as the available
//! parallelism, at most 4, unless `TSRS_LINT_SERVER_WORKERS` sets a positive number. `Cancel {"id"}` answers a queued
//! request as canceled at once; a running request stops before it loads its next file's project or lints its next
//! file, and is answered as canceled; a finished or unknown `id` is ignored. `Shutdown` or the end of the input
//! answers queued requests as canceled, lets running requests finish, then exits with status 0. `docs/LINT_SERVER.md`
//! documents the protocol.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use rustc_hash::{FxHashMap, FxHashSet};

use tsrs_core::P;
use tsrs_core::context::{self, CheckerLifetime, Context};
use tsrs_core::jsnum;
use tsrs_core::json::{self, JsonMap, Value};
use tsrs_core::tspath;
use tsrs_lsproto as lsproto;
use tsrs_project::{Kind, Session, SessionInit, SessionOptions, Snapshot};
use tsrs_vfs::{FS, bundled, osvfs};

use crate::engine::{self, Options};
use crate::payload::{self, ConfiguredRule};
use crate::protocol::{self, MessageType};

const DID_OPEN: u8 = 16;
const DID_CHANGE: u8 = 17;
const DID_CLOSE: u8 = 18;
const FILES_CHANGED: u8 = 19;
const LINT: u8 = 20;
const SHUTDOWN: u8 = 21;
const HELLO: u8 = 22;
const CANCEL: u8 = 23;

/// The version of the frames below. A client and the server must speak the same version: any incompatible change to
/// the protocol bumps it.
pub const PROTOCOL_VERSION: u32 = 1;

/// The stack of each worker thread: the checker recurses as deeply as in the CLI.
const STACK_SIZE: usize = 512 << 20;

/// Overrides the number of worker threads (a positive integer).
const WORKERS_ENV: &str = "TSRS_LINT_SERVER_WORKERS";
/// The most workers by default: each lint holds a checker, and an editor rarely has more requests in flight.
const MAX_DEFAULT_WORKERS: usize = 4;

/// The error text of a canceled request's error frame, which also carries `"canceled": true`.
const CANCELED: &str = "request canceled";

/// The language of an opened file: unknown, so the session takes its script kind from the file name, as it does for
/// a file on disk (any extension, in any case).
const UNKNOWN_LANGUAGE: lsproto::LanguageKind = lsproto::LanguageKind("");

struct Server {
    session: Arc<Session>,
    cwd: String,
    options: Options,
    output: Mutex<Output>,
    /// Paths are absolute and normalized ([`normalize`]).
    open_file_paths: Mutex<FxHashSet<String>>,
    queue: Mutex<Queue>,
    /// Signaled when a request is queued or the queue closes.
    queued: Condvar,
}

/// The server's output.
struct Output {
    writer: Box<dyn Write + Send>,
    /// Set by a fatal error, which is the last frame: the frames of requests that end after it are dropped.
    closed: bool,
}

/// Lint requests waiting for a worker, and the requests the workers are running.
#[derive(Default)]
struct Queue {
    /// First in, first out.
    pending: VecDeque<LintRequest>,
    /// The id and cancellation flag of each running request.
    running: Vec<(u64, Arc<AtomicBool>)>,
    /// Set at shutdown: workers exit once nothing is pending.
    closed: bool,
}

/// Runs the server on `input` and `output` until the input ends or a shutdown frame arrives. Returns the exit status.
pub fn run(base: Options, input: &mut dyn Read, output: Box<dyn Write + Send>) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => tspath::normalize_slashes(&cwd.to_string_lossy()),
        Err(error) => {
            let mut output = output;
            let _ = protocol::write_message(
                &mut output,
                MessageType::Error,
                &protocol::object([
                    ("id", Value::Null),
                    ("error", Value::String(format!("error getting current directory: {error}"))),
                ]),
            );
            let _ = output.flush();
            return 1;
        }
    };
    let fs: Arc<dyn FS> = Arc::new(bundled::wrap_fs(osvfs::fs()));
    let session = tsrs_project::new_session(&SessionInit {
        background_ctx: Context::background(),
        options: Arc::new(SessionOptions {
            current_directory: cwd.clone(),
            default_library_path: bundled::default_lib_path(),
            // File changes arrive as FilesChanged frames from oxlint's language server.
            watch_enabled: false,
            run_external_code: base.run_external_code,
            ..Default::default()
        }),
        fs,
        client: None,
        logger: None,
        npm_executor: None,
        spawner: base.run_external_code.then(|| {
            Arc::new(tsrs_contentmapper::ProcessSpawner) as Arc<dyn tsrs_contentmapper::Spawner>
        }),
        content_mapper_logger: None,
        parse_cache: None,
        content_mapped_parse_cache: None,
    });
    // Files outside every tsconfig are linted with the headless mode's options.
    session.did_change_compiler_options_for_inferred_projects(
        &Context::background(),
        Some(P::new(engine::inferred_compiler_options())),
    );
    let server = Arc::new(Server {
        session,
        cwd,
        options: base,
        output: Mutex::new(Output { writer: output, closed: false }),
        open_file_paths: Mutex::default(),
        queue: Mutex::default(),
        queued: Condvar::new(),
    });
    let code = server.serve(input);
    server.session.close();
    code
}

impl Server {
    fn serve(self: &Arc<Self>, input: &mut dyn Read) -> i32 {
        match self.handshake(input) {
            Ok(true) => {}
            // The client left before saying hello.
            Ok(false) => return 0,
            Err(error) => {
                self.fatal(&error);
                return 1;
            }
        }
        let workers = match self.start_workers(worker_count()) {
            Ok(workers) => workers,
            Err(error) => {
                self.fatal(&error);
                return 1;
            }
        };
        let mut skipped = FxHashSet::default();
        let code = loop {
            let frame = match protocol::read_frame(input) {
                Ok(Some(frame)) => frame,
                Ok(None) => break 0,
                Err(error) => {
                    self.fatal(&format!("error reading frame: {error}"));
                    break 1;
                }
            };
            match self.handle(frame, &mut skipped) {
                Ok(true) => {}
                Ok(false) => break 0,
                Err(error) => {
                    self.fatal(&error);
                    break 1;
                }
            }
        };
        // Queued requests are canceled; running ones finish.
        self.close_queue();
        for worker in workers {
            let _ = worker.join();
        }
        code
    }

    /// Starts `count` workers, which lint queued requests until the queue closes.
    fn start_workers(self: &Arc<Self>, count: usize) -> Result<Vec<JoinHandle<()>>, String> {
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            let server = Arc::clone(self);
            let worker = std::thread::Builder::new()
                .name(format!("tsrs-lint-worker-{index}"))
                .stack_size(STACK_SIZE)
                .spawn(move || server.work());
            match worker {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    self.close_queue();
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(format!("could not start a lint worker: {error}"));
                }
            }
        }
        Ok(workers)
    }

    /// A worker: lints requests in the order they were queued until the queue closes.
    fn work(&self) {
        while let Some(request) = self.next_request() {
            self.lint(&request);
            let mut queue = self.queue.lock().unwrap();
            queue.running.retain(|(_, canceled)| !Arc::ptr_eq(canceled, &request.canceled));
        }
    }

    /// Waits for the next queued request and marks it running, or returns `None` once the queue closes.
    fn next_request(&self) -> Option<LintRequest> {
        let mut queue = self.queue.lock().unwrap();
        loop {
            if let Some(request) = queue.pending.pop_front() {
                queue.running.push((request.id, Arc::clone(&request.canceled)));
                return Some(request);
            }
            if queue.closed {
                return None;
            }
            queue = self.queued.wait(queue).unwrap();
        }
    }

    /// Closes the queue and answers every queued request as canceled.
    fn close_queue(&self) {
        let pending = {
            let mut queue = self.queue.lock().unwrap();
            queue.closed = true;
            std::mem::take(&mut queue.pending)
        };
        self.queued.notify_all();
        for request in pending {
            self.write_canceled(request.id);
        }
    }

    /// Cancels request `id`: a queued request is answered at once, a running one stops before its next file. A
    /// finished or unknown request is ignored.
    fn cancel(&self, id: u64) {
        let canceled_queued = {
            let mut queue = self.queue.lock().unwrap();
            for (running, canceled) in &queue.running {
                if *running == id {
                    // Only the flag is shared: its worker reads it and nothing else depends on its ordering.
                    canceled.store(true, Ordering::Relaxed);
                }
            }
            let before = queue.pending.len();
            queue.pending.retain(|request| request.id != id);
            before - queue.pending.len()
        };
        for _ in 0..canceled_queued {
            self.write_canceled(id);
        }
    }

    /// Reads the client's `Hello` and answers with the server's. Returns `false` if the input ends first.
    fn handshake(&self, input: &mut dyn Read) -> Result<bool, String> {
        let Some((message_type, bytes)) =
            protocol::read_frame(input).map_err(|error| format!("error reading frame: {error}"))?
        else {
            return Ok(false);
        };
        if message_type != HELLO {
            return Err(format!(
                "expected a Hello frame (message type {HELLO}) first, got message type {message_type}"
            ));
        }
        // The client learns the server's versions even when it cannot go on.
        let mut output = Vec::new();
        protocol::write_message(
            &mut output,
            MessageType::ServerHello,
            &protocol::object([
                ("protocol_version", Value::Integer(PROTOCOL_VERSION.into())),
                ("version", Value::String(env!("CARGO_PKG_VERSION").into())),
            ]),
        )
        .map_err(|error| format!("could not encode Hello: {error}"))?;
        self.write(&output);
        let client = parse(&bytes)
            .and_then(|value| {
                let version = integer(payload::object(&value)?.get("protocol_version"))?;
                u32::try_from(version).map_err(|error| error.to_string())
            })
            .map_err(|error| format!("invalid Hello frame: {error}"))?;
        if client != PROTOCOL_VERSION {
            return Err(format!(
                "protocol version mismatch: client speaks {client}, tsrs-lint server speaks {PROTOCOL_VERSION}"
            ));
        }
        Ok(true)
    }

    /// Handles one frame after the handshake. Returns `false` on shutdown.
    ///
    /// Frames of unknown types, and a second `Hello`, are skipped: a newer client may send notifications this server
    /// does not know. `skipped` holds the types already logged.
    fn handle(
        &self,
        (message_type, bytes): (u8, Vec<u8>),
        skipped: &mut FxHashSet<u8>,
    ) -> Result<bool, String> {
        if !matches!(
            message_type,
            DID_OPEN | DID_CHANGE | DID_CLOSE | FILES_CHANGED | LINT | SHUTDOWN | CANCEL
        ) {
            if skipped.insert(message_type) {
                eprintln!("tsrs-lint server: skipping unknown message type {message_type}");
            }
            return Ok(true);
        }
        let value = parse(&bytes)?;
        let fields = payload::object(&value)?;
        let ctx = Context::background();
        match message_type {
            DID_OPEN | DID_CHANGE => {
                let name = file_name(fields, &self.cwd)?;
                let version = integer(fields.get("version"))?;
                let version = i32::try_from(version).map_err(|error| error.to_string())?;
                let text = payload::string(fields.get("text"))?;
                let uri = uri(&name);
                if self.open_file_paths.lock().unwrap().insert(name) {
                    self.session.did_open_file(&ctx, uri, version, text, UNKNOWN_LANGUAGE);
                } else {
                    self.session.did_change_file(
                        &ctx,
                        uri,
                        version,
                        vec![lsproto::TextDocumentContentChangePartialOrWholeDocument {
                            whole_document: Some(lsproto::TextDocumentContentChangeWholeDocument {
                                text,
                            }),
                            ..Default::default()
                        }],
                    );
                }
            }
            DID_CLOSE => {
                let name = file_name(fields, &self.cwd)?;
                if self.open_file_paths.lock().unwrap().remove(&name) {
                    self.session.did_close_file(&ctx, uri(&name));
                }
            }
            FILES_CHANGED => {
                let mut events = Vec::new();
                for change in payload::array(fields.get("changes"))? {
                    let change = payload::object(change)?;
                    let kind = integer(change.get("kind"))?;
                    events.push(lsproto::FileEvent {
                        uri: uri(&file_name(change, &self.cwd)?),
                        type_: lsproto::FileChangeType(
                            u32::try_from(kind).map_err(|e| e.to_string())?,
                        ),
                    });
                }
                self.session.did_change_watched_files(&ctx, &events);
            }
            LINT => {
                let id = request_id(fields)?;
                // A request that cannot be parsed (an unknown rule, for example) fails on its own.
                let request = match LintRequest::parse(fields, id, self.options, &self.cwd) {
                    Ok(request) => request,
                    Err(error) => {
                        self.write_request_error(id, error);
                        return Ok(true);
                    }
                };
                self.queue.lock().unwrap().pending.push_back(request);
                self.queued.notify_one();
            }
            CANCEL => self.cancel(request_id(fields)?),
            SHUTDOWN => return Ok(false),
            _ => unreachable!("message type {message_type} is skipped above"),
        }
        Ok(true)
    }

    fn lint(&self, request: &LintRequest) {
        let mut output = Vec::new();
        let result = catch_unwind(AssertUnwindSafe(|| self.lint_files(request, &mut output)))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(ToString::to_string)
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into());
                Err(format!("panic: {message}"))
            });
        let terminal = if request.is_canceled() {
            // A canceled request's diagnostics are dropped, even if it got to the end.
            output.clear();
            canceled_frame(&mut output, request.id)
        } else {
            match result {
                Ok(()) => protocol::write_message(
                    &mut output,
                    MessageType::ServerLintDone,
                    &protocol::object([("id", id_value(request.id))]),
                ),
                Err(error) => {
                    // Diagnostics of a failed request are dropped: the client keeps its previous results.
                    output.clear();
                    error_frame(&mut output, request.id, error)
                }
            }
        };
        if let Err(error) = terminal {
            eprintln!("tsrs-lint server: could not encode a response: {error}");
            return;
        }
        self.write(&output);
    }

    fn write_canceled(&self, id: u64) {
        let mut output = Vec::new();
        match canceled_frame(&mut output, id) {
            Ok(()) => self.write(&output),
            Err(error) => eprintln!("tsrs-lint server: could not encode a response: {error}"),
        }
    }

    fn write_request_error(&self, id: u64, error: String) {
        let mut output = Vec::new();
        match error_frame(&mut output, id, error) {
            Ok(()) => self.write(&output),
            Err(error) => eprintln!("tsrs-lint server: could not encode a response: {error}"),
        }
    }

    /// Writes a request's frames at once, so the frames of concurrent requests never interleave. Nothing is written
    /// after a fatal error.
    fn write(&self, bytes: &[u8]) {
        let mut output = self.output.lock().unwrap();
        if output.closed {
            return;
        }
        let writer = &mut output.writer;
        if let Err(error) = writer.write_all(bytes).and_then(|()| writer.flush()) {
            eprintln!("tsrs-lint server: could not write a response: {error}");
        }
    }

    /// Lints the request's files into `output`, all against one snapshot: a notification that arrives while the
    /// files are linted applies to later requests only.
    fn lint_files(&self, request: &LintRequest, output: &mut Vec<u8>) -> Result<(), String> {
        let ctx =
            context::with_checker_lifetime(&Context::background(), CheckerLifetime::Diagnostics);
        let uris: Vec<lsproto::DocumentUri> =
            request.files.iter().map(|file| uri(&file.name)).collect();
        // Building a program cannot be interrupted, so the request's projects are loaded one file at a time first: a
        // request canceled meanwhile stops before it builds the next project's program. A file whose project is
        // already loaded costs a lookup.
        for uri in &uris {
            if request.is_canceled() {
                return Ok(());
            }
            self.session.with_snapshot_for_document(&ctx, uri, |_| {});
        }
        let mut result = Ok(());
        // The programs are loaded: this snapshot only updates them for notifications that arrived meanwhile.
        self.session.with_snapshot_for_documents(&ctx, &uris, |snapshot| {
            // Projects whose tsconfig errors were reported: once per request, as the headless mode does per program.
            let mut reported_projects = FxHashSet::default();
            result = request.files.iter().zip(&uris).try_for_each(|(file, uri)| {
                if request.is_canceled() {
                    return Ok(());
                }
                self.lint_file(&ctx, snapshot, request, file, uri, &mut reported_projects, output)
            });
        });
        result
    }

    /// Lints one file of the request.
    #[expect(clippy::too_many_arguments, reason = "the request state each file reads")]
    fn lint_file(
        &self,
        ctx: &Context,
        snapshot: &Snapshot,
        request: &LintRequest,
        file: &RequestFile,
        uri: &lsproto::DocumentUri,
        reported_projects: &mut FxHashSet<String>,
        output: &mut Vec<u8>,
    ) -> Result<(), String> {
        // A file without a project (an unknown script kind, for example) has nothing to lint.
        let Some(project) = snapshot.get_default_project(uri) else {
            return Ok(());
        };
        let Some(program) = project.get_program() else {
            return Ok(());
        };
        let mut diagnostics = Vec::new();
        let config_errors = self.config_errors(&project, program, &mut diagnostics)?;
        if !reported_projects.insert(project.id().0) {
            diagnostics.clear();
        }
        if !config_errors
            && let Some(source_file) = program.get_source_file(&file.name)
            // Rules see a content-mapped file's virtual text, whose ranges do not apply on disk.
            && !source_file.is_content_mapped()
            && !file.rules.is_empty()
        {
            let mut checker = program.get_type_checker_for_file(ctx, source_file);
            let mut timings = std::collections::BTreeMap::new();
            diagnostics.extend(engine::lint_file(
                program,
                source_file,
                &file.rules,
                request.options,
                &mut checker,
                &mut timings,
            )?);
        }
        for diagnostic in &diagnostics {
            protocol::write_diagnostic_with_id(output, request.id, diagnostic)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Reports a configured project's tsconfig and program errors as the headless mode does, in which case its files
    /// are not linted. Returns whether there were any.
    fn config_errors(
        &self,
        project: &tsrs_project::Project,
        program: &'static tsrs_compiler::Program,
        diagnostics: &mut Vec<protocol::Diagnostic>,
    ) -> Result<bool, String> {
        if project.kind != Kind::Configured || self.options.suppress_program_diagnostics {
            return Ok(false);
        }
        let config_path = project.config_file_name().to_string();
        let mut errors = project.command_line.map_or_default(|config| config.errors.clone());
        let program_diagnostic = errors.is_empty();
        if program_diagnostic {
            errors = program.get_program_diagnostics();
        }
        for &diagnostic in &errors {
            diagnostics.push(engine::config_error(diagnostic, &config_path, program_diagnostic)?);
        }
        Ok(!errors.is_empty())
    }

    /// Writes a fatal error, the last frame the server writes.
    fn fatal(&self, error: &str) {
        let mut output = self.output.lock().unwrap();
        if output.closed {
            return;
        }
        output.closed = true;
        let _ = protocol::write_message(
            &mut output.writer,
            MessageType::Error,
            &protocol::object([("id", Value::Null), ("error", Value::String(error.into()))]),
        );
        let _ = output.writer.flush();
    }
}

struct LintRequest {
    id: u64,
    options: Options,
    /// Each file once, in the order the request first names it.
    files: Vec<RequestFile>,
    /// Set by a `Cancel` frame while the request runs.
    canceled: Arc<AtomicBool>,
}

struct RequestFile {
    /// Absolute, with forward slashes.
    name: String,
    rules: Vec<ConfiguredRule>,
}

impl LintRequest {
    fn parse(fields: &JsonMap, id: u64, base: Options, cwd: &str) -> Result<Self, String> {
        let options = Options {
            fix: payload::boolean(fields.get("fix"))?,
            fix_suggestions: payload::boolean(fields.get("fix_suggestions"))?,
            ..base
        };
        let mut files: Vec<RequestFile> = Vec::new();
        // Each file's index in `files`.
        let mut seen: FxHashMap<String, usize> = FxHashMap::default();
        for config in payload::deserialize_configs(fields.get("configs"))? {
            for rule in &config.rules {
                crate::registry::require_implemented(&rule.name)?;
            }
            for path in config.file_paths {
                let name = normalize(&path, cwd);
                // A file named by several configs gets the rules of the last one, as in the headless mode.
                if let Some(&index) = seen.get(&name) {
                    files[index].rules.clone_from(&config.rules);
                } else {
                    seen.insert(name.clone(), files.len());
                    files.push(RequestFile { name, rules: config.rules.clone() });
                }
            }
        }
        Ok(Self { id, options, files, canceled: Arc::default() })
    }

    fn is_canceled(&self) -> bool {
        // Only the flag is shared (see `Server::cancel`).
        self.canceled.load(Ordering::Relaxed)
    }
}

/// The number of worker threads: `TSRS_LINT_SERVER_WORKERS` if it is a positive integer, else the available
/// parallelism up to [`MAX_DEFAULT_WORKERS`].
fn worker_count() -> usize {
    std::env::var(WORKERS_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<NonZeroUsize>().ok())
        .map_or_else(
            || {
                std::thread::available_parallelism()
                    .map_or(1, NonZeroUsize::get)
                    .clamp(1, MAX_DEFAULT_WORKERS)
            },
            NonZeroUsize::get,
        )
}

fn request_id(fields: &JsonMap) -> Result<u64, String> {
    u64::try_from(integer(fields.get("id"))?).map_err(|error| error.to_string())
}

/// A frame's JSON.
fn parse(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| format!("invalid frame: {error}"))?;
    json::unmarshal(text).map_err(|error| format!("invalid frame: {error}"))
}

fn normalize(path: &str, cwd: &str) -> String {
    tspath::get_normalized_absolute_path(&tspath::normalize_slashes(path), cwd)
}

/// A notification's `path`, normalized as a lint request's paths are, so that both name a file the same way.
fn file_name(fields: &JsonMap, cwd: &str) -> Result<String, String> {
    let path = payload::string(fields.get("path"))?;
    if path.is_empty() {
        return Err("expected a path".into());
    }
    Ok(normalize(&path, cwd))
}

fn uri(file_name: &str) -> lsproto::DocumentUri {
    tsrs_ls::lsconv::file_name_to_document_uri(file_name)
}

fn integer(value: Option<&Value>) -> Result<i64, String> {
    match value {
        Some(Value::Integer(n)) => Ok(*n),
        #[expect(clippy::cast_possible_truncation, reason = "checked to be integral and in range")]
        Some(Value::Number(n)) if n.fract() == 0.0 && n.abs() <= jsnum::MAX_SAFE_INTEGER.0 => {
            Ok(*n as i64)
        }
        _ => Err("expected an integer".into()),
    }
}

fn error_frame(output: &mut Vec<u8>, id: u64, error: String) -> io::Result<()> {
    protocol::write_message(
        output,
        MessageType::Error,
        &protocol::object([("id", id_value(id)), ("error", Value::String(error))]),
    )
}

fn canceled_frame(output: &mut Vec<u8>, id: u64) -> io::Result<()> {
    protocol::write_message(
        output,
        MessageType::Error,
        &protocol::object([
            ("id", id_value(id)),
            ("error", Value::String(CANCELED.into())),
            ("canceled", Value::Bool(true)),
        ]),
    )
}

fn id_value(id: u64) -> Value {
    Value::Integer(i64::try_from(id).unwrap_or(i64::MAX))
}

/// Shared by `cli.rs`: stdout as the server's output.
pub(crate) fn stdout() -> Box<dyn Write + Send> {
    Box::new(io::BufWriter::new(io::stdout()))
}
