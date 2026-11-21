// Go builds two language-service paths with `RootedDirectoryPath.ResolveFile`, which normalizes the result
// (no `.` or `..` segment, no empty segment, no trailing separator). tsrs joins the strings without that step.
// A module specifier rename to `./b` sends the `RenameFile` operation with `newUri` `<dir>/./b.ts` instead of
// `<dir>/b.ts`, and a definition request on `import a from "./missing/"` answers `<dir>/missing/` instead of
// `<dir>/missing` for the module that does not exist.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MODULE: &str = "ls_resolve_file_not_normalized";

const MAIN_TS: &str = "import \"./a\";\n";
const RENAME_CAPABILITIES: &str = r#"{"textDocument":{"rename":{"prepareSupport":true}},"workspace":{"workspaceEdit":{"documentChanges":true,"resourceOperations":["rename"]}}}"#;

#[test]
fn module_specifier_rename_normalizes_the_new_uri() {
    let project = Project::new(
        "rename",
        &[("tsconfig.json", "{}"), ("a.ts", "export {};\n"), ("main.ts", MAIN_TS)],
    );
    let mut server = Server::start(&project, RENAME_CAPABILITIES);
    let main = project.uri("main.ts");
    server.open(&main, "typescript", MAIN_TS);

    // Line 0, character 10: inside `"./a"`. The new name is the text of the new specifier.
    let mut new_uris = Vec::new();
    for new_name in ["./b", "x/../b", "x//b", "."] {
        let response = server.request(
            "textDocument/rename",
            &format!(
                r#"{{"textDocument":{{"uri":"{main}"}},"position":{{"line":0,"character":10}},"newName":"{new_name}"}}"#
            ),
        );
        new_uris.push(string_values(&response, "newUri"));
    }
    // The project directory in the responses is `<dir>`. For `.`, Go resolves the path to the project directory
    // and adds the extension.
    assert_eq!(
        new_uris,
        [
            ["file://<dir>/b.ts"],
            ["file://<dir>/b.ts"],
            ["file://<dir>/x/b.ts"],
            ["file://<dir>.ts"],
        ]
    );
}

#[test]
fn definition_of_unresolved_import_has_no_trailing_separator() {
    let text = "import a from \"./missing/\";\n";
    let project = Project::new("definition", &[("tsconfig.json", "{}"), ("main.ts", text)]);
    let mut server = Server::start(&project, "{}");
    let main = project.uri("main.ts");
    server.open(&main, "typescript", text);

    // Line 0, character 17: inside `"./missing/"`.
    let response = server.request(
        "textDocument/definition",
        &format!(r#"{{"textDocument":{{"uri":"{main}"}},"position":{{"line":0,"character":17}}}}"#),
    );
    // The module does not exist, so the location is the path that the specifier names. Go removes the
    // trailing separator.
    assert_eq!(string_values(&response, "uri"), ["file://<dir>/missing"]);
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
