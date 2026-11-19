// Go turns the `file:` URI of a request into a normalized rooted path with `DocumentUri.FileName`
// (`RootedPathFromAbsolute`): it drops a trailing `/` and removes `.` and `..` segments. tsrs returns the decoded
// path as it is. A directory URI with a trailing `/` then does not match the directory (`willRenameFiles` from
// `file://<dir>/src/` returns `null`, and a delete event for `file://<dir>/src/sub/` leaves the deleted files
// in the program). A URI with a `.` segment (`file://<dir>/./a.ts`) makes the server panic and exit.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MODULE: &str = "document_uri_trailing_slash_not_normalized";

const A_TS: &str = "import { b } from \"./sub/b\";\nexport const n: number = b;\n";
const B_TS: &str = "export const b = 1;\n";
const C_TS: &str = "export const c = 1;\n";
const CAPABILITIES: &str = r#"{"textDocument":{"diagnostic":{}}}"#;

#[test]
fn will_rename_files_from_directory_uri_with_trailing_slash_updates_root_dirs() {
    let project = Project::new(
        "rename-root-dirs",
        &[("tsconfig.json", r#"{"compilerOptions": {"rootDirs": ["src"]}}"#), ("src/c.ts", C_TS)],
    );
    // Go changes `"src"` in `rootDirs` to `"lib"`.
    let result = will_rename_files(&project, "src/", "lib");
    assert_eq!(string_values(&result, "newText"), ["lib"], "edits: {result}");
    assert!(result.contains("\"file://<dir>/tsconfig.json\""), "edits: {result}");
}

#[test]
fn will_rename_files_from_directory_uri_with_trailing_slash_updates_include() {
    let project = Project::new(
        "rename-include",
        &[("tsconfig.json", r#"{"include": ["src"]}"#), ("src/c.ts", C_TS)],
    );
    // The new URI has a trailing `/` too. Go changes `"src"` in `include` to `"lib"`.
    let result = will_rename_files(&project, "src/", "lib/");
    assert_eq!(string_values(&result, "newText"), ["lib"], "edits: {result}");
    assert!(result.contains("\"file://<dir>/tsconfig.json\""), "edits: {result}");
}

#[test]
fn watched_file_delete_event_for_directory_uri_with_trailing_slash_updates_the_program() {
    let project = Project::new(
        "delete-directory",
        &[("tsconfig.json", r#"{"include":["src"]}"#), ("src/a.ts", A_TS), ("src/sub/b.ts", B_TS)],
    );
    let mut server = Server::start(&project, CAPABILITIES);
    let a = project.uri("src/a.ts");
    server.open(&a, "typescript", A_TS);
    let params = format!(r#"{{"textDocument":{{"uri":"{a}"}}}}"#);
    let before = server.request("textDocument/diagnostic", &params);
    assert!(!before.contains("\"code\""), "unexpected diagnostics at the start: {before}");

    // The client reports that the directory `src/sub/` was deleted. The URI ends with `/`.
    std::fs::remove_dir_all(project.dir.join("src/sub")).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        &format!(r#"{{"changes":[{{"uri":"{}","type":3}}]}}"#, project.uri("src/sub/")),
    );

    // Go removes the files of the directory from the program, so `./sub/b` is TS2307. Wait up to 5 seconds.
    let mut after = String::new();
    for _ in 0..25 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        after = server.request("textDocument/diagnostic", &params);
        if after.contains("\"code\":2307") {
            break;
        }
    }
    assert!(
        after.contains("\"code\":2307"),
        "the deleted directory is still in the program: {after}"
    );
}

#[test]
fn document_uri_with_dot_segment_is_normalized() {
    let text = "export const a = 1;\n";
    let project = Project::new("dot-segment", &[("tsconfig.json", "{}"), ("a.ts", text)]);
    let mut server = Server::start(&project, "{}");
    // The URI is `file://<dir>/./a.ts`. tsrs panics with `fileName should be normalized and absolute`.
    let uri = project.uri("./a.ts");
    server.open(&uri, "typescript", text);
    let response = server.request(
        "textDocument/hover",
        &format!(r#"{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":0,"character":13}}}}"#),
    );
    assert_eq!(string_values(&response, "value"), ["const a: 1"], "hover: {response}");
}

/// Asks for the edits of `workspace/willRenameFiles` and returns the `result` text.
fn will_rename_files(project: &Project, old_path: &str, new_path: &str) -> String {
    let mut server = Server::start(project, "{}");
    server.open(&project.uri("src/c.ts"), "typescript", C_TS);
    let response = server.request(
        "workspace/willRenameFiles",
        &format!(
            r#"{{"files":[{{"oldUri":"{}","newUri":"{}"}}]}}"#,
            project.uri(old_path),
            project.uri(new_path)
        ),
    );
    top_level(&response, "result").expect("result").to_string()
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

/// The decoded values of all string properties named `key`, anywhere in `json`, in order.
fn string_values(json: &str, key: &str) -> Vec<String> {
    let pattern = format!("\"{key}\":\"");
    let mut values = Vec::new();
    let mut from = 0;
    while let Some(found) = json[from..].find(&pattern) {
        let start = from + found + pattern.len() - 1;
        let end = string_end(json.as_bytes(), start);
        values.push(json_unescape(&json[start + 1..end - 1]));
        from = end;
    }
    values
}

fn json_unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                out.push(
                    u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap_or('\u{FFFD}'),
                );
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
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
