// `tsrs --daemon` (tsrs_execute daemon/mod.rs): every answer of a resident check server is the output and exit
// status of a cold run of the same command on the same files, across edits, new and deleted files, renames,
// configuration changes, concurrent clients, a server killed mid-request and stale servers.

#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const TSCONFIG: &str = r#"{ "compilerOptions": { "strict": true, "noEmit": true, "skipLibCheck": true }, "include": ["src"] }"#;

struct Project {
    dir: PathBuf,
    sockets: PathBuf,
    binary: PathBuf,
}

impl Project {
    fn new(name: &str) -> Project {
        let root = std::env::temp_dir().join(format!("tsrs-dmn-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("p");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();
        std::fs::write(dir.join("src/a.ts"), "export const a: number = \"x\";\n").unwrap();
        std::fs::write(
            dir.join("src/b.ts"),
            "import { a } from \"./a\";\nexport const b: string = a;\n",
        )
        .unwrap();
        Project { dir, sockets: root.join("s"), binary: PathBuf::from(env!("CARGO_BIN_EXE_tsrs")) }
    }

    fn write(&self, file: &str, text: &str) {
        let path = self.dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn command(&self, binary: &Path, daemon: bool) -> Command {
        let mut command = Command::new(binary);
        command.args(["-p", ".", "--fast", "--pretty", "false"]).current_dir(&self.dir);
        for (name, _) in std::env::vars_os() {
            if name.to_str().is_some_and(|n| n.starts_with("TSRS_")) {
                command.env_remove(name);
            }
        }
        command.env("TSRS_DAEMON_DIR", &self.sockets);
        if daemon {
            command.arg("--daemon");
        }
        command
    }

    fn run(&self, daemon: bool) -> Output {
        self.command(&self.binary, daemon).output().unwrap()
    }

    /// Runs the command cold and through the server, asserts both print the same and exit the same, and returns
    /// the output.
    fn check(&self) -> String {
        let cold = self.run(false);
        let served = self.run(true);
        assert_eq!(
            String::from_utf8_lossy(&served.stdout),
            String::from_utf8_lossy(&cold.stdout),
            "served vs cold output; server log:\n{}",
            self.log()
        );
        assert_eq!(served.status.code(), cold.status.code(), "exit status; log:\n{}", self.log());
        assert!(served.stderr.is_empty(), "{}", String::from_utf8_lossy(&served.stderr));
        String::from_utf8(cold.stdout).unwrap()
    }

    fn log(&self) -> String {
        let mut text = String::new();
        for entry in std::fs::read_dir(&self.sockets).into_iter().flatten().flatten() {
            if entry.path().extension().is_some_and(|x| x == "log") {
                text.push_str(&std::fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }
        text
    }

    fn requests(&self) -> Vec<String> {
        self.log().lines().filter(|l| l.contains("; answered after")).map(str::to_string).collect()
    }

    fn server_pids(&self) -> Vec<u32> {
        self.log()
            .lines()
            .filter_map(|l| l.strip_prefix("tsrs daemon: pid ")?.split(',').next()?.parse().ok())
            .collect()
    }

    fn sockets(&self) -> Vec<PathBuf> {
        let mut sockets: Vec<PathBuf> = std::fs::read_dir(&self.sockets)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "sock"))
            .collect();
        sockets.sort();
        sockets
    }

    fn stop(&self) -> String {
        let out = Command::new(&self.binary)
            .arg("--daemon-stop")
            .env("TSRS_DAEMON_DIR", &self.sockets)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// A killed server stays a zombie until the client that started it exits.
fn alive(pid: u32) -> bool {
    let out = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
    let stat = String::from_utf8_lossy(&out.stdout);
    !stat.trim().is_empty() && !stat.trim().starts_with('Z')
}

#[test]
fn edits_between_requests_match_cold_runs() {
    let p = Project::new("edits");
    let first = p.check();
    assert!(first.contains("src/a.ts(1,14): error TS2322"), "{first}");
    // Nothing changed: the server replays.
    p.check();
    // A same-size edit right after the run that read the file (its stamp is too recent to vouch for it).
    p.write("src/a.ts", "export const a: number = \"y\";\n");
    p.check();
    p.write("src/a.ts", "export const a: number = 1;\n");
    assert!(!p.check().contains("src/a.ts"));
    // A new file under the include directory, then its deletion.
    p.write("src/c.ts", "export const c: boolean = 0;\n");
    assert!(p.check().contains("src/c.ts"));
    std::fs::remove_file(p.dir.join("src/c.ts")).unwrap();
    p.check();
    // A rename, and an editor's save by writing a temporary file and renaming it over the original.
    std::fs::rename(p.dir.join("src/b.ts"), p.dir.join("src/d.ts")).unwrap();
    assert!(p.check().contains("src/d.ts"));
    p.write("src/a.tmp", "export const a: string = \"s\";\n");
    std::fs::rename(p.dir.join("src/a.tmp"), p.dir.join("src/a.ts")).unwrap();
    assert!(!p.check().contains("src/d.ts"));
    // The same text written again: unchanged, replayed.
    p.write("src/a.ts", "export const a: string = \"s\";\n");
    p.check();
    // A module that did not resolve appears, in a new node_modules directory.
    p.write("src/e.ts", "import { e } from \"pkg\";\nexport const f: number = e;\n");
    assert!(p.check().contains("TS2307"));
    p.write("node_modules/pkg/package.json", r#"{ "name": "pkg", "types": "index.d.ts" }"#);
    p.write("node_modules/pkg/index.d.ts", "export declare const e: string;\n");
    let resolved = p.check();
    assert!(!resolved.contains("TS2307") && resolved.contains("src/e.ts(2,14)"), "{resolved}");
    // Its package.json now points elsewhere.
    p.write("node_modules/pkg/other.d.ts", "export declare const e: number;\n");
    p.write("node_modules/pkg/package.json", r#"{ "name": "pkg", "types": "other.d.ts" }"#);
    assert!(!p.check().contains("src/e.ts"));
    // A dangling symlink in an included directory (left out of its listing) starts resolving when its target
    // appears, though the directory itself does not change.
    std::os::unix::fs::symlink("../outside.ts", p.dir.join("src/link.ts")).unwrap();
    assert!(!p.check().contains("link.ts"));
    p.write("outside.ts", "export const o: number = \"o\";\n");
    assert!(p.check().contains("src/link.ts"));

    let pids = p.server_pids();
    assert_eq!(pids.len(), 1, "one server for the whole session:\n{}", p.log());
    let requests = p.requests();
    assert_eq!(requests.len(), 14, "{}", p.log());
    let replays = requests.iter().filter(|r| r.contains("replay")).count();
    assert_eq!(replays, 3, "{}", p.log());
}

#[test]
fn configuration_change_restarts_the_server() {
    let p = Project::new("config");
    p.check();
    p.write("tsconfig.json", &TSCONFIG.replace(r#""strict": true"#, r#""strict": false"#));
    p.write("src/a.ts", "export const a = (x) => x;\n");
    p.check();
    p.check();
    assert_eq!(p.server_pids().len(), 2, "{}", p.log());
    assert!(
        p.log().contains("restart (") && p.log().contains("tsconfig.json changed"),
        "{}",
        p.log()
    );
}

#[test]
fn concurrent_clients_share_one_server() {
    let p = Project::new("concurrent");
    let cold = p.run(false);
    let clients: Vec<_> = std::iter::repeat_with(|| {
        p.command(&p.binary, true).stdout(Stdio::piped()).spawn().unwrap()
    })
    .take(6)
    .collect();
    for client in clients {
        let out = client.wait_with_output().unwrap();
        assert_eq!(out.stdout, cold.stdout, "{}", p.log());
        assert_eq!(out.status.code(), cold.status.code());
    }
    assert_eq!(p.server_pids().len(), 1, "{}", p.log());
    assert_eq!(p.requests().len(), 6, "{}", p.log());
}

#[test]
fn a_server_killed_mid_request_leaves_the_client_to_run_the_command() {
    let p = Project::new("killed");
    p.write(
        "tsconfig.json",
        r#"{ "compilerOptions": { "strict": true, "noEmit": true, "skipLibCheck": true }, "files": ["src/a.ts", "src/fifo.ts"] }"#,
    );
    // The server blocks reading a named pipe until it is written to.
    let fifo = p.dir.join("src/fifo.ts");
    let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: mkfifo with a valid path and mode.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);
    let client = p.command(&p.binary, true).stdout(Stdio::piped()).spawn().unwrap();
    wait_until("the server to start", || !p.server_pids().is_empty());
    let pid = p.server_pids()[0];
    std::thread::sleep(Duration::from_millis(300));
    // SAFETY: kills the server process this test started through the client.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    wait_until("the server to die", || !alive(pid));
    // The client now runs the command itself and opens the pipe in turn.
    let text = "export const f: number = \"z\";\n";
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fifo)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let served = client.wait_with_output().unwrap();
    std::fs::remove_file(&fifo).unwrap();
    p.write("src/fifo.ts", text);
    let cold = p.run(false);
    assert_eq!(String::from_utf8_lossy(&served.stdout), String::from_utf8_lossy(&cold.stdout));
    assert!(String::from_utf8_lossy(&cold.stdout).contains("src/fifo.ts(1,14)"));
    assert_eq!(served.status.code(), cold.status.code());
    // A later request starts a new server.
    p.check();
    assert_eq!(p.server_pids().len(), 2, "{}", p.log());
}

#[test]
fn another_binary_or_protocol_never_talks_to_the_server() {
    let p = Project::new("version");
    p.check();
    let socket = p.sockets()[0].clone();
    // A request whose key is not the server's (another version's client) is answered with "run it yourself".
    let mut stream = UnixStream::connect(&socket).unwrap();
    let material = "tsrs-daemon 0\nanother binary\n";
    let mut request = b"TSRD".to_vec();
    request.extend_from_slice(&(material.len() as u32).to_le_bytes());
    request.extend_from_slice(material.as_bytes());
    request.extend_from_slice(&[0, 0]);
    stream.write_all(&request).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    assert_eq!(response, b"TSRD\x01");
    // A copy of the binary (another file, as an upgrade would be) has a server of its own.
    let copy = p.sockets.parent().unwrap().join("tsrs-copy");
    std::fs::copy(&p.binary, &copy).unwrap();
    let cold = p.run(false);
    let other = p.command(&copy, true).output().unwrap();
    assert_eq!(other.stdout, cold.stdout);
    assert_eq!(p.sockets().len(), 2);
    assert_eq!(p.server_pids().len(), 2, "{}", p.log());
}

#[test]
fn an_idle_server_exits_and_stop_stops_servers() {
    let p = Project::new("idle");
    let cold = p.run(false);
    let served = p.command(&p.binary, true).env("TSRS_DAEMON_IDLE", "0.5").output().unwrap();
    assert_eq!(served.stdout, cold.stdout);
    let pid = p.server_pids()[0];
    wait_until("the idle server to exit", || !alive(pid));
    assert_eq!(p.sockets(), Vec::<PathBuf>::new());
    assert!(p.log().contains("idle for"), "{}", p.log());
    p.check();
    assert_eq!(p.stop(), "tsrs: stopped 1 check server(s)\n");
    assert_eq!(p.sockets(), Vec::<PathBuf>::new());
}

#[test]
fn a_server_over_its_memory_limit_exits() {
    let p = Project::new("memory");
    let cold = p.run(false);
    let served = p.command(&p.binary, true).env("TSRS_DAEMON_MAX_MEMORY", "1M").output().unwrap();
    assert_eq!(served.stdout, cold.stdout);
    let pid = p.server_pids()[0];
    wait_until("the server over its limit to exit", || !alive(pid));
    assert!(p.log().contains("over 1048576: exiting"), "{}", p.log());
    // The next request starts another server.
    p.check();
    assert_eq!(p.server_pids().len(), 2, "{}", p.log());
}

#[test]
fn ineligible_commands_run_in_process() {
    let p = Project::new("ineligible");
    // Without --fast, and with a tsrs debugging variable, no server starts.
    let mut plain = Command::new(&p.binary);
    plain
        .args(["-p", ".", "--pretty", "false", "--daemon"])
        .current_dir(&p.dir)
        .env("TSRS_DAEMON_DIR", &p.sockets);
    let out = plain.output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("TS2322"));
    let debug = p.command(&p.binary, true).env("TSRS_FILE_TIMES", "/dev/null").output().unwrap();
    assert_eq!(debug.status.code(), Some(2));
    assert!(p.sockets().is_empty() && p.server_pids().is_empty(), "{}", p.log());
    // An emitting run is served once, and the server keeps nothing.
    p.write("tsconfig.json", r#"{ "compilerOptions": { "outDir": "out" }, "include": ["src"] }"#);
    p.check();
    assert!(p.dir.join("out/src/a.js").exists());
    assert!(p.log().contains("not resident"), "{}", p.log());
}
