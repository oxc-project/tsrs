// Port of cmd/tsc/api.go (`tsrs --api`): flag parsing, the session filesystem (bundled libs over the OS
// filesystem, optionally behind the client callback filesystem), and the connection to the wire runtime.

use std::sync::Arc;

use tsrs_api::adapter::{ClientConnAdapter, SessionHandler};
use tsrs_api::{Session, SessionOptions};
use tsrs_api_transport as transport;
use tsrs_api_transport::{CallbackConfig, CallbackFs};
use tsrs_vfs::{FS, bundled, osvfs};

use crate::lsp::FlagSet;

/// Go `apiFlags`.
pub struct ApiFlags {
    pub cwd: String,
    pub pipe_path: String,
    pub callbacks: Vec<String>,
    pub case_sensitive: bool,
    pub is_async: bool,
    pub timing: bool,
    pub run_external_code: bool,
}

/// Go `parseAPIFlags`.
#[expect(
    clippy::result_unit_err,
    reason = "Public for the integration tests; the caller only needs to know parsing failed."
)]
pub fn parse_api_flags(args: &[String]) -> Result<ApiFlags, ()> {
    let mut flags = FlagSet::new("api");
    flags.string("cwd", "current working directory");
    flags.string(
        "pipe",
        "use a named pipe (Windows) or create a private Unix socket at <path>/socket, where <path> must not exist",
    );
    flags.string("callbacks", "comma-separated list of FS callbacks and defaults to enable");
    flags.bool("useCaseSensitiveFileNames", "treat filesystem paths as case-sensitive");
    flags.bool("async", "use JSON-RPC protocol instead of MessagePack (for async API)");
    flags.bool(
        "timing",
        "collect per-request server processing time, folded into the client's timing snapshot",
    );
    flags.bool("runExternalCode", "allow projects to execute configured external plugins");
    flags.parse(args)?;
    // Go defaults: cwd = os.Getwd(), useCaseSensitiveFileNames = the OS filesystem's sensitivity.
    let cwd = match flags.string_value("cwd") {
        c if !c.is_empty() => c,
        _ => std::env::current_dir()
            .map(|d| d.to_string_lossy().replace('\\', "/"))
            .map_err(|_| ())?,
    };
    let case_flag_given = args.iter().any(|a| {
        let a = a.trim_start_matches('-');
        a == "useCaseSensitiveFileNames" || a.starts_with("useCaseSensitiveFileNames=")
    });
    let case_sensitive = if case_flag_given {
        flags.bool_value("useCaseSensitiveFileNames")
    } else {
        osvfs::fs().use_case_sensitive_file_names()
    };
    let callbacks = flags.string_value("callbacks");
    Ok(ApiFlags {
        cwd,
        pipe_path: flags.string_value("pipe"),
        callbacks: if callbacks.is_empty() {
            Vec::new()
        } else {
            callbacks.split(',').map(str::to_string).collect()
        },
        case_sensitive,
        is_async: flags.bool_value("async"),
        timing: flags.bool_value("timing"),
        run_external_code: flags.bool_value("runExternalCode"),
    })
}

/// Go `StdioServer.Run` setup: the session over bundled libs + OS filesystem, wrapped by the client
/// callback filesystem (the CLI always passes a case-sensitivity setting, so Go always wraps).
pub fn new_api_session(flags: &ApiFlags) -> Result<(Arc<Session>, Arc<CallbackFs>), String> {
    let base: Arc<dyn FS> = Arc::new(bundled::wrap_fs(osvfs::fs()));
    let config = CallbackConfig::parse(&flags.callbacks)?;
    let callback_fs = Arc::new(CallbackFs::new(base, &config, Some(flags.case_sensitive)));
    let fs: Arc<dyn FS> = Arc::<CallbackFs>::clone(&callback_fs);
    let mut options =
        SessionOptions::new(flags.cwd.clone(), bundled::default_lib_path(), fs, !flags.is_async);
    options.run_external_code = flags.run_external_code;
    let session = Session::new(options);
    Ok((session, callback_fs))
}

/// `tsrs --api` (Go `runAPI` + `StdioServer.Run`). stdout carries only protocol bytes.
pub fn run_api(args: &[String]) -> i32 {
    let Ok(flags) = parse_api_flags(args) else {
        return 2;
    };
    if let Err(err) = CallbackConfig::parse(&flags.callbacks) {
        eprintln!("{err}");
        return 2;
    }
    let mut options = transport::ServeOptions::new(flags.is_async);
    options.collect_timing = flags.timing;
    // signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM); main started catching the signals.
    let shutdown = options.shutdown.clone();
    crate::signals::on_interrupt(move || shutdown.cancel());
    let pipe = (!flags.pipe_path.is_empty()).then(|| std::path::PathBuf::from(&flags.pipe_path));
    let result = transport::serve(pipe.as_deref(), options, |caller| {
        let (session, callback_fs) = new_api_session(&flags).expect("callbacks validated above");
        callback_fs.set_connection(Arc::clone(&caller));
        session.set_connection(Arc::new(ClientConnAdapter(caller)));
        Arc::new(SessionHandler(session))
    });
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}
