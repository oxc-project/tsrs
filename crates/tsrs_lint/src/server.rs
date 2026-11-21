//! `tsrs-lint server`: a long-lived linter for oxlint's language server.
//!
//! The headless mode builds every program from scratch for each request. The server keeps a project session instead
//! (`tsrs_project::Session`, the one `tsrs --lsp` uses): it holds the editor's unsaved text of open files, reuses
//! programs across edits and keeps checkers warm, so a lint request after an edit only pays for what changed.
//!
//! Frames use the headless framing in both directions (`protocol.rs`). The client sends:
//!
//! | type | name | JSON |
//! | --- | --- | --- |
//! | 16 | `DidOpen` | `{"path", "version", "text"}` |
//! | 17 | `DidChange` | `{"path", "version", "text"}` (the whole text) |
//! | 18 | `DidClose` | `{"path"}` |
//! | 19 | `FilesChanged` | `{"changes": [{"path", "kind"}]}` (LSP `FileChangeType`: 1 created, 2 changed, 3 deleted) |
//! | 20 | `Lint` | `{"id", "configs": [{"file_paths", "rules"}], "fix", "fix_suggestions"}` |
//! | 21 | `Shutdown` | `{}` |
//!
//! Notifications are applied in order before any later lint request takes a snapshot. A lint request lints every
//! file in `configs`, as the headless mode does. It is answered with diagnostic frames carrying its `id`, then one
//! `Done {"id"}` frame, or an error frame carrying its `id`. Requests run concurrently. Compiler diagnostics are not
//! reported. `docs/LINT_SERVER.md` documents the protocol.

use std::io::{self, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rustc_hash::FxHashSet;

use tsrs_core::P;
use tsrs_core::context::{self, CheckerLifetime, Context};
use tsrs_core::jsnum;
use tsrs_core::json::{self, JsonMap, Value};
use tsrs_core::tspath;
use tsrs_lsproto as lsproto;
use tsrs_project::{Kind, Session, SessionInit, SessionOptions};
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

/// The stack of each request thread: the checker recurses as deeply as in the CLI.
const STACK_SIZE: usize = 512 << 20;

struct Server {
    session: Arc<Session>,
    cwd: String,
    options: Options,
    output: Mutex<Box<dyn Write + Send>>,
    /// Paths are normalized with [`tspath::normalize_slashes`].
    open_file_paths: Mutex<FxHashSet<String>>,
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
        output: Mutex::new(output),
        open_file_paths: Mutex::default(),
    });
    let code = server.serve(input);
    server.session.close();
    code
}

impl Server {
    fn serve(self: &Arc<Self>, input: &mut dyn Read) -> i32 {
        let mut requests: Vec<JoinHandle<()>> = Vec::new();
        let code = loop {
            let frame = match protocol::read_frame(input) {
                Ok(Some(frame)) => frame,
                Ok(None) => break 0,
                Err(error) => {
                    self.fatal(&format!("error reading frame: {error}"));
                    break 1;
                }
            };
            requests.retain(|request| !request.is_finished());
            match self.handle(frame, &mut requests) {
                Ok(true) => {}
                Ok(false) => break 0,
                Err(error) => {
                    self.fatal(&error);
                    break 1;
                }
            }
        };
        for request in requests {
            let _ = request.join();
        }
        code
    }

    /// Handles one frame. Returns `false` on shutdown.
    fn handle(
        self: &Arc<Self>,
        (message_type, bytes): (u8, Vec<u8>),
        requests: &mut Vec<JoinHandle<()>>,
    ) -> Result<bool, String> {
        let text =
            std::str::from_utf8(&bytes).map_err(|error| format!("invalid frame: {error}"))?;
        let value = json::unmarshal(text).map_err(|error| format!("invalid frame: {error}"))?;
        let fields = payload::object(&value)?;
        let ctx = Context::background();
        match message_type {
            DID_OPEN | DID_CHANGE => {
                let name = file_name(fields)?;
                let version = integer(fields.get("version"))?;
                let version = i32::try_from(version).map_err(|error| error.to_string())?;
                let text = payload::string(fields.get("text"))?;
                let uri = uri(&name);
                if self.open_file_paths.lock().unwrap().insert(name.clone()) {
                    self.session.did_open_file(&ctx, uri, version, text, language_kind(&name));
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
                let name = file_name(fields)?;
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
                        uri: uri(&file_name(change)?),
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
                let server = Arc::clone(self);
                let handle = std::thread::Builder::new()
                    .name(format!("lint-request-{}", request.id))
                    .stack_size(STACK_SIZE)
                    .spawn(move || server.lint(&request))
                    .map_err(|error| format!("could not start a lint request: {error}"))?;
                requests.push(handle);
            }
            SHUTDOWN => return Ok(false),
            other => return Err(format!("unknown message type: {other}")),
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
        let terminal = match result {
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
        };
        if let Err(error) = terminal {
            eprintln!("tsrs-lint server: could not encode a response: {error}");
            return;
        }
        self.write(&output);
    }

    fn write_request_error(&self, id: u64, error: String) {
        let mut output = Vec::new();
        match error_frame(&mut output, id, error) {
            Ok(()) => self.write(&output),
            Err(error) => eprintln!("tsrs-lint server: could not encode a response: {error}"),
        }
    }

    /// Writes a request's frames at once, so the frames of concurrent requests never interleave.
    fn write(&self, bytes: &[u8]) {
        let mut writer = self.output.lock().unwrap();
        if let Err(error) = writer.write_all(bytes).and_then(|()| writer.flush()) {
            eprintln!("tsrs-lint server: could not write a response: {error}");
        }
    }

    /// Lints the request's files into `output`.
    fn lint_files(&self, request: &LintRequest, output: &mut Vec<u8>) -> Result<(), String> {
        let ctx =
            context::with_checker_lifetime(&Context::background(), CheckerLifetime::Diagnostics);
        // Projects whose tsconfig errors were reported: once per request, as the headless mode does per program.
        let mut reported_projects = FxHashSet::default();
        for file in &request.files {
            self.lint_file(&ctx, request, file, &mut reported_projects, output)?;
        }
        Ok(())
    }

    /// Lints one file of the request.
    fn lint_file(
        &self,
        ctx: &Context,
        request: &LintRequest,
        file: &RequestFile,
        reported_projects: &mut FxHashSet<String>,
        output: &mut Vec<u8>,
    ) -> Result<(), String> {
        let uri = uri(&file.name);
        let mut result = Ok(());
        self.session.with_snapshot_for_document(ctx, &uri, |snapshot| {
            result = (|| {
                // A file without a project (an unknown script kind, for example) has nothing to lint.
                let Some(project) = snapshot.get_default_project(&uri) else {
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
            })();
        });
        result
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

    fn fatal(&self, error: &str) {
        let mut writer = self.output.lock().unwrap();
        let _ = protocol::write_message(
            &mut *writer,
            MessageType::Error,
            &protocol::object([("id", Value::Null), ("error", Value::String(error.into()))]),
        );
        let _ = writer.flush();
    }
}

struct LintRequest {
    id: u64,
    options: Options,
    /// Each file once.
    files: Vec<RequestFile>,
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
        let mut seen: FxHashSet<String> = FxHashSet::default();
        for config in payload::deserialize_configs(fields.get("configs"))? {
            for rule in &config.rules {
                crate::registry::require_implemented(&rule.name)?;
            }
            for path in config.file_paths {
                let name = normalize(&path, cwd);
                if seen.insert(name.clone()) {
                    files.push(RequestFile { name, rules: config.rules.clone() });
                }
            }
        }
        Ok(Self { id, options, files })
    }
}

fn request_id(fields: &JsonMap) -> Result<u64, String> {
    u64::try_from(integer(fields.get("id"))?).map_err(|error| error.to_string())
}

fn normalize(path: &str, cwd: &str) -> String {
    tspath::get_normalized_absolute_path(&tspath::normalize_slashes(path), cwd)
}

fn file_name(fields: &JsonMap) -> Result<String, String> {
    let path = payload::string(fields.get("path"))?;
    if path.is_empty() {
        return Err("expected a path".into());
    }
    Ok(tspath::normalize_slashes(&path))
}

fn uri(file_name: &str) -> lsproto::DocumentUri {
    tsrs_ls::lsconv::file_name_to_document_uri(file_name)
}

fn language_kind(file_name: &str) -> lsproto::LanguageKind {
    let extension = file_name.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension {
        "tsx" => lsproto::LanguageKind::TYPE_SCRIPT_REACT,
        "jsx" => lsproto::LanguageKind::JAVA_SCRIPT_REACT,
        "js" | "mjs" | "cjs" => lsproto::LanguageKind::JAVA_SCRIPT,
        _ => lsproto::LanguageKind::TYPE_SCRIPT,
    }
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

fn id_value(id: u64) -> Value {
    Value::Integer(i64::try_from(id).unwrap_or(i64::MAX))
}

/// Shared by `cli.rs`: stdout as the server's output.
pub(crate) fn stdout() -> Box<dyn Write + Send> {
    Box::new(io::BufWriter::new(io::stdout()))
}
