//! tsrs-only: a resident check server for repeated `--fast` runs (`tsrs --daemon`, `TSRS_DAEMON=1`;
//! notes/perf-fast-daemon.md).
//!
//! A git hook or an agent that type-checks after every edit pays the whole front end each time: read, parse and
//! bind every file, resolve every import. With `--daemon`, the command goes to a server process that keeps the parsed
//! files of its last run and runs the command again with them, or replays its last output when nothing it read has
//! changed. The output is byte-identical to a cold run of the same binary with the same arguments on the same files:
//!
//! - One server per key (`protocol::key_material`): this binary's path and file identity, the working directory,
//!   the arguments and the environment variables tsrs reads. A new binary has another key, so it never talks to an
//!   old server; the server compares the whole key text of each request with its own.
//! - Each request runs the cold command line (`execute::command_line`) on the server's process, with fresh checkers and
//!   the configuration parsed again. Two things differ from a cold run: the file system answers come from the
//!   validation below where it has them, and the compiler host hands out a file parsed earlier when its text is
//!   unchanged (`Resident::host`). The files of the last run are parsed and bound as a cold run would parse them
//!   (same options, same text); checkers only add lazily filled data to them, as in watch mode and the language
//!   server, which reuse files across programs the same way. Node and symbol ids keep growing from request to
//!   request; a `--fast` run already numbers them differently on every run (checkers take ids in parallel), and its
//!   output does not depend on them.
//! - Staleness: no file watcher. The server records every file system query a run makes with its answer
//!   (recordfs.rs) and asks all of them again on the next request. If every answer is unchanged, a cold run would
//!   read the same files, so it would print the same output, and the server replays it. A read is validated by the
//!   file's stamp (inode, size, modification and change times) and, when that changed or was too recent to trust, by
//!   the hash of its text. Event loss, coalescing and watch limits cannot make a result stale.
//! - Any configuration file changing restarts the server (the configuration sets process-wide state, such as lazy
//!   declaration parsing). Anything the server cannot serve (`--build`, `--watch`, emitting runs, timings, content
//!   mappers, `TSRS_*` debugging variables) runs in-process; so does a request whose server dies or answers
//!   anything unexpected.
//!
//! Limits: one request at a time per server (others wait on the socket), an idle timeout (`TSRS_DAEMON_IDLE`, in
//! seconds; 30 minutes), a memory limit (`TSRS_DAEMON_MAX_MEMORY`; by default twice the process's memory after the
//! first request, and at least 1 GiB more), and half of the 31-bit id space. `tsrs --daemon-stop` stops every
//! server of the user. The sockets are in `TSRS_DAEMON_DIR`, else `$XDG_RUNTIME_DIR/tsrs-daemon`, else
//! `$TMPDIR/tsrs-daemon-<uid>`, with each server's log beside its socket. Unix only.

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

/// The daemon's command-line flags, before any compiler setup.
///
/// `--daemon-serve <args>` runs the server for `args`
/// (started by a client), `--daemon-stop` stops every server, and `--daemon` or `TSRS_DAEMON=1` sends an eligible
/// command to its server. Returns the exit status, or `None` to run `args` in-process (`--daemon` removed).
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
