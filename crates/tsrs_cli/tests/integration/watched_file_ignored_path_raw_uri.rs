// Go converts the URI of each watched-file event to a path first (`uri.FileName()`) and then calls
// `ContainsIgnoredPath` on the decoded path, so the event for `file://<dir>/.%23b.ts` (the file `.#b.ts`, an
// Emacs lock-file name) is ignored. tsrs calls `contains_ignored_path` on the URI text, where `#` is `%23`, so it
// does not ignore the event. If `include` in `tsconfig.json` lists `.#b.ts`, tsrs adds the file to the
// project and the diagnostics of `a.ts` change (TS2304 for `bGlobal` goes away).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MODULE: &str = "watched_file_ignored_path_raw_uri";

const A_TS: &str = "export const n: number = bGlobal;\n";
const B_TS: &str = "declare var bGlobal: number;\n";
const CAPABILITIES: &str = r#"{"textDocument":{"diagnostic":{}}}"#;

#[test]
fn created_lock_file_event_is_ignored() {
    let project = Project::new(
        "lock-file",
        &[("tsconfig.json", r#"{"include":["a.ts",".#b.ts"]}"#), ("a.ts", A_TS)],
    );
    let mut server = Server::start(&project, CAPABILITIES);
    let a = project.uri("a.ts");
    server.open(&a, "typescript", A_TS);
    let params = format!(r#"{{"textDocument":{{"uri":"{a}"}}}}"#);

    let before = server.request("textDocument/diagnostic", &params);
    assert!(before.contains("\"code\":2304"), "bGlobal must be unknown at the start: {before}");

    // The editor creates the lock file `.#b.ts` and the client reports it as created. The URI has `%23`.
    std::fs::write(project.dir.join(".#b.ts"), B_TS).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        &format!(r#"{{"changes":[{{"uri":"{}","type":1}}]}}"#, project.uri(".%23b.ts")),
    );
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Go ignores the event, so the program does not change and `bGlobal` is still unknown.
    let after = server.request("textDocument/diagnostic", &params);
    assert!(after.contains("\"code\":2304"), "the lock file was added to the program: {after}");
}

/// A project directory in the temporary directory. The directory is removed when the value is dropped.
struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(name: &str, files: &[(&str, &str)]) -> Project {
        let dir =
            std::env::temp_dir().join(format!("tsrs-lsp-{MODULE}-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (path, text) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        Project { dir }
    }

    /// The `file:` URI of `relative` in the project directory.
    fn uri(&self, relative: &str) -> String {
        format!("file://{}/{relative}", self.dir.display())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A running `tsrs --lsp -stdio` process. Dropping the value stops the process.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
    dir: String,
}

impl Server {
    /// Starts the server and runs `initialize` and `initialized` with the project directory as the root.
    fn start(project: &Project, capabilities: &str) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
            .args(["--lsp", "-stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("run tsrs --lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let dir = project.dir.display().to_string();
        let mut server = Server { child, stdin, stdout, next_id: 1, dir: dir.clone() };
        server.request(
            "initialize",
            &format!(
                r#"{{"processId":null,"rootUri":"file://{dir}","capabilities":{capabilities}}}"#
            ),
        );
        server.notify("initialized", "{}");
        server
    }

    fn send(&mut self, body: &str) {
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())
            .and_then(|()| self.stdin.flush())
            .expect("write to tsrs --lsp (crashed?)");
    }

    fn notify(&mut self, method: &str, params: &str) {
        self.send(&format!(r#"{{"jsonrpc":"2.0","method":"{method}","params":{params}}}"#));
    }

    /// Opens a document with `textDocument/didOpen`. The file does not need to exist on disk.
    fn open(&mut self, uri: &str, language_id: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            &format!(
                r#"{{"textDocument":{{"uri":"{uri}","languageId":"{language_id}","version":1,"text":{}}}}}"#,
                json_string(text)
            ),
        );
    }

    /// Sends a request and returns the response message. The server's own requests (configuration,
    /// registrations) get the answer `null`. The project directory in the message is replaced by `<dir>`.
    fn request(&mut self, method: &str, params: &str) -> String {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}","params":{params}}}"#
        ));
        loop {
            let message = self.read_message();
            if top_level(&message, "method").is_some() {
                if let Some(request_id) = top_level(&message, "id") {
                    self.send(&format!(r#"{{"jsonrpc":"2.0","id":{request_id},"result":null}}"#));
                }
                continue;
            }
            if top_level(&message, "id") == Some(id.to_string().as_str()) {
                return message.replace(&self.dir, "<dir>");
            }
        }
    }

    fn read_message(&mut self) -> String {
        let mut length = None;
        loop {
            let mut line = String::new();
            assert!(self.stdout.read_line(&mut line).unwrap() > 0, "tsrs --lsp exited (crashed?)");
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                length = Some(n.parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; length.expect("Content-Length")];
        self.stdout.read_exact(&mut body).unwrap();
        String::from_utf8(body).unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The raw text of the value of the top-level property `key` of the JSON object `json`.
fn top_level<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let bytes = json.as_bytes();
    let (mut depth, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = string_end(bytes, i);
                if depth == 1 && json[i + 1..end - 1] == *key && bytes.get(end) == Some(&b':') {
                    return Some(&json[end + 1..value_end(bytes, end + 1)]);
                }
                i = end;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The index after the string that starts at `start` (a `"`).
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while bytes[i] != b'"' {
        i += if bytes[i] == b'\\' { 2 } else { 1 };
    }
    i + 1
}

/// The index after the JSON value that starts at `start`.
fn value_end(bytes: &[u8], start: usize) -> usize {
    match bytes[start] {
        b'"' => string_end(bytes, start),
        b'{' | b'[' => {
            let (mut depth, mut i) = (0usize, start);
            loop {
                match bytes[i] {
                    b'"' => {
                        i = string_end(bytes, i);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        _ => bytes[start..]
            .iter()
            .position(|b| matches!(b, b',' | b'}' | b']'))
            .map_or(bytes.len(), |n| start + n),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
