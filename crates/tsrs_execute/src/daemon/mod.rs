//! tsrs-only: a resident check server for repeated `--fast` runs (`tsrs --daemon`, `TSRS_DAEMON=1`;
//! notes/perf-fast-daemon.md).
//!
//! The command goes to a server process that keeps the parsed files of its last run and runs the command again with
//! them, or replays its last output when nothing it read has changed. The output is byte-identical to a cold run of
//! the same binary with the same arguments on the same files:
//!
//! - One server per key (`protocol::key_material`): the binary's path and file identity, the working directory, the
//!   arguments and the environment variables tsrs reads. A new binary has another key and never talks to an old
//!   server; the server compares the whole key text of each request with its own.
//! - Each request runs the cold command line (`execute::command_line`) in the server with fresh checkers and the
//!   configuration parsed again. Two things differ: file system answers come from the validation below where it has
//!   them, and the compiler host hands out a file parsed earlier when its text is unchanged (`Resident::host`). A
//!   kept file was parsed and bound as a cold run parses it (same options, same text); checkers only add lazily
//!   filled data, as watch mode and the language server reuse files. Ids keep growing from request to request; a
//!   `--fast` run numbers them differently on every run already, and its output does not depend on them.
//! - Staleness, without a watcher: the server records every file system query a run makes with its answer
//!   (recordfs.rs) and asks all of them again on the next request. Same answers, same bytes read, same output:
//!   replayed. A read is validated by the file's stamp, or by the hash of its text when the stamp changed or was
//!   too recent to trust.
//! - A configuration file changing restarts the server (the configuration sets process-wide state, such as lazy
//!   declaration parsing). What the server cannot serve (`--build`, `--watch`, emitting runs, timings, content
//!   mappers, any other `TSRS_*` variable) runs in-process, and so does a request whose server dies or answers
//!   anything unexpected.
//!
//! Limits: one request at a time per server (others wait on the socket), an idle exit (`TSRS_DAEMON_IDLE`, in
//! seconds; 30 minutes), a memory limit (twice the process's memory after the first request, at least 1 GiB more)
//! and half of the 31-bit id space. `tsrs --daemon-stop` stops every server of the user. Sockets and logs are in
//! `TSRS_DAEMON_DIR`, else `$XDG_RUNTIME_DIR/tsrs-daemon`, else `$TMPDIR/tsrs-daemon-<uid>`. Unix only.

use std::sync::{Arc, OnceLock};

use tsrs_compiler::{CompilerHost, Program, ProgramOptions};
use tsrs_core::P;
use tsrs_tsoptions::ParsedCommandLine;

#[cfg(unix)]
mod client;
#[cfg(unix)]
mod protocol;
#[cfg(unix)]
mod recordfs;
#[cfg(unix)]
mod server;

/// What a resident server changes in `execute::perform_compilation`.
pub(crate) trait Resident: Sync {
    /// The host the program is built with, given the cold run's.
    fn host(&self, host: Arc<dyn CompilerHost>) -> Arc<dyn CompilerHost>;
    /// Adjusts the program's options (regions for what the server frees after the request).
    fn program_options(&self, options: &mut ProgramOptions);
    /// The program has been checked and its output written.
    fn checked(&self, program: &'static Program, config: P<ParsedCommandLine>);
}

static RESIDENT: OnceLock<&'static dyn Resident> = OnceLock::new();

/// The server's `Resident` in a server process, else `None`.
pub(crate) fn resident() -> Option<&'static dyn Resident> {
    RESIDENT.get().copied()
}

/// The daemon's flags, before any compiler setup.
///
/// `--daemon-serve <args>` runs the server for `args` (started by a client), `--daemon-stop` stops every server,
/// and `--daemon` or `TSRS_DAEMON=1` sends an eligible command to its server. Returns the exit status, or `None`
/// to run `args` in-process (`--daemon` removed).
pub fn main(args: &mut Vec<String>) -> Option<u8> {
    if args.first().map(String::as_str) == Some("--daemon-serve") {
        #[cfg(unix)]
        return Some(server::serve(args[1..].to_vec()));
    }
    if args.first().map(String::as_str) == Some("--daemon-stop") {
        #[cfg(unix)]
        return Some(client::stop_all());
    }
    let before = args.len();
    args.retain(|a| !a.eq_ignore_ascii_case("--daemon"));
    let wanted = args.len() != before || std::env::var("TSRS_DAEMON").is_ok_and(|v| v == "1");
    #[cfg(unix)]
    if wanted && client::eligible(args) {
        return client::run(args);
    }
    let _ = wanted;
    None
}
