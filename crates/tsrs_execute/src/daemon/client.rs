// tsrs-only: the client side of a resident check server (daemon/mod.rs). It finds or starts the server for its
// command, sends the request, and writes what comes back. Anything unexpected returns `None`: the caller then runs
// the command in-process, as without `--daemon`.

use std::io::{IsTerminal, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::protocol::{self, Op, Paths, Request, Response};

/// How long a client waits for a server it started to accept connections.
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// Flags of commands the server does not serve (lowercase).
const REFUSED: &[&str] = &[
    "-w",
    "--watch",
    "--diagnostics",
    "--extendeddiagnostics",
    "--generatetrace",
    "--generatecpuprofile",
    "--checkercostcache",
    "--lsp",
    "--api",
    "--lint",
];

/// Whether a command can be served: a `--fast` compilation (not `--build`, `--watch`, a response file or one of the
/// commands that print timings or write a cache of their own), and no `TSRS_*` tuning or debugging variable in the environment
/// (those can print to standard error, which the server does not send back).
pub fn eligible(args: &[String]) -> bool {
    let lower: Vec<String> = args.iter().map(|a| a.to_ascii_lowercase()).collect();
    if lower.first().is_some_and(|a| matches!(a.as_str(), "-b" | "--b" | "-build" | "--build")) {
        return false;
    }
    // A response file (`@file`) may hold any of these; its text is not part of the key.
    if lower.iter().any(|a| REFUSED.contains(&a.as_str()) || a.starts_with('@')) {
        return false;
    }
    let fast_flag = lower
        .iter()
        .enumerate()
        .any(|(i, a)| a == "--fast" && lower.get(i + 1).is_none_or(|next| next != "false"));
    let mut fast_env = false;
    for (name, value) in std::env::vars_os() {
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("TSRS_") || name.starts_with("TSRS_DAEMON") {
            continue;
        }
        if name != "TSRS_FAST" {
            return false;
        }
        fast_env = value == "1";
    }
    fast_flag || (fast_env && !lower.iter().any(|a| a == "--fast"))
}

/// Runs `args` on its server and returns the exit status, or `None` to run in-process.
pub fn run(args: &[String]) -> Option<u8> {
    let material = protocol::key_material(args)?;
    let paths = Paths::new(&material)?;
    let request =
        Request { material, op: Op::Check, stdout_is_tty: std::io::stdout().is_terminal() };
    for _ in 0..3 {
        let mut stream = match UnixStream::connect(&paths.socket) {
            Ok(stream) => stream,
            Err(_) => start_server(args, &paths)?,
        };
        protocol::write_request(&mut stream, &request).ok()?;
        match protocol::read_response(&mut stream) {
            Ok(Response::Output { status, stdout }) => {
                let mut out = std::io::stdout().lock();
                crate::sys::exit_on_broken_pipe(&out.write_all(&stdout));
                crate::sys::exit_on_broken_pipe(&out.flush());
                return Some(status);
            }
            // The server stopped (its configuration changed, or it reached a limit): start another.
            Ok(Response::Restart) => {}
            // The server cannot serve this command, or it died: run it here.
            Ok(Response::Fallback | Response::Stopped) | Err(_) => return None,
        }
    }
    None
}

/// Starts a server for `args` and waits until it accepts connections. Another client may start one at the same
/// time; only one takes the lock (server.rs `serve`), and both clients connect to it.
fn start_server(args: &[String], paths: &Paths) -> Option<UnixStream> {
    // The log keeps the servers of this key one after another, up to 1 MiB.
    if paths.log.metadata().is_ok_and(|md| md.len() > 1 << 20) {
        let _ = std::fs::File::create(&paths.log);
    }
    let log = std::fs::OpenOptions::new().create(true).append(true).open(&paths.log).ok()?;
    let mut command = Command::new(std::env::current_exe().ok()?);
    command
        .arg("--daemon-serve")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        // Its own process group: a terminal's Ctrl-C, or a hook runner that kills the client's group, does not reach it.
        .process_group(0);
    let mut child: Child = command.spawn().ok()?;
    let start = Instant::now();
    let mut child_running = true;
    while start.elapsed() < START_TIMEOUT {
        if let Ok(stream) = UnixStream::connect(&paths.socket) {
            return Some(stream);
        }
        if child_running && let Ok(Some(status)) = child.try_wait() {
            // Exit 0: another server holds the lock and will bind the socket; anything else is an error.
            if !status.success() {
                return None;
            }
            child_running = false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    None
}

/// `tsrs --daemon-stop`: stops every server of this user, and prints how many there were.
pub fn stop_all() -> u8 {
    let Some(dir) = protocol::directory() else {
        return 0;
    };
    let mut stopped = 0;
    let mut sockets: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "sock"))
                .collect()
        })
        .unwrap_or_default();
    sockets.sort();
    for socket in sockets {
        let Ok(mut stream) = UnixStream::connect(&socket) else { continue };
        let request = Request { material: String::new(), op: Op::Stop, stdout_is_tty: false };
        if protocol::write_request(&mut stream, &request).is_ok()
            && matches!(protocol::read_response(&mut stream), Ok(Response::Stopped))
        {
            stopped += 1;
        }
    }
    println!("tsrs: stopped {stopped} check server(s)");
    0
}
