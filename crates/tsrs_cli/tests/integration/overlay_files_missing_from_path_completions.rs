// Go resolves the directory fragment of a path completion with `ResolveDirectory`, which drops the trailing
// separator (`<dir>`, not `<dir>/`). The overlay file system keys its directories by that form, so it merges the
// open documents that are not on disk (unsaved or virtual files) into the listing. tsrs keeps the trailing
// separator with `resolve_path`, so the lookup misses the overlay. In `import {} from "./"` and in
// `/// <reference path="./" />` tsrs lists only the files on disk, and a directory that exists only as the
// parent of an open document is empty.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const MODULE: &str = "overlay_files_missing_from_path_completions";

const CAPABILITIES: &str =
    r#"{"textDocument":{"completion":{"completionItem":{"snippetSupport":true}}}}"#;

#[test]
fn import_path_completion_lists_open_document_that_is_not_on_disk() {
    // `virt.ts` and `main.ts` exist only in the editor.
    let labels = completion_labels(
        "import",
        &[("a.ts", "export const a = 1;\n")],
        &[("virt.ts", "export const v = 1;\n")],
        "import {} from \"./\";\n",
        17,
    );
    assert_eq!(labels, ["a", "virt"]);
}

#[test]
fn import_path_completion_lists_directory_that_exists_only_in_the_editor() {
    // `sub/` is not on disk: it is the parent of the open document `sub/virt.ts`.
    let labels = completion_labels(
        "overlay-directory",
        &[("tsconfig.json", "{}"), ("a.ts", "export const a = 1;\n")],
        &[("sub/virt.ts", "export const v = 1;\n")],
        "import { b } from \"./sub/\";\n",
        25,
    );
    assert_eq!(labels, ["virt"]);
}

#[test]
fn reference_path_completion_lists_open_document_that_is_not_on_disk() {
    let labels = completion_labels(
        "reference-path",
        &[("tsconfig.json", "{}"), ("a.d.ts", "declare const a: number;\n")],
        &[("virt.d.ts", "declare const v: number;\n")],
        "/// <reference path=\"./\" />\nexport {};\n",
        23,
    );
    assert_eq!(labels, ["a.d.ts", "tsconfig.json", "virt.d.ts"]);
}

#[test]
fn package_path_completion_with_types_versions_lists_open_document() {
    // The fragment `pkg/` names the package directory, so the path in the package is empty. No `typesVersions`
    // pattern matches it, and the completion then lists the package directory.
    let labels = completion_labels(
        "types-versions",
        &[
            (
                "node_modules/pkg/package.json",
                r#"{"name":"pkg","version":"1.0.0","typesVersions":{"*":{"nomatch/*":["ts3.1/*"]}}}"#,
            ),
            ("node_modules/pkg/index.d.ts", "export {};\n"),
        ],
        &[("node_modules/pkg/virt.d.ts", "export {};\n")],
        "import {} from \"pkg/\";\n",
        20,
    );
    assert_eq!(labels, ["index", "nomatch", "package.json", "virt"]);
}

/// Writes `files` to a project, opens the documents `open` (not on disk) and then `main.ts` (not on disk) with
/// `main_text`, requests completion at `character` in line 0 of `main.ts`, and returns the sorted labels.
fn completion_labels(
    name: &str,
    files: &[(&str, &str)],
    open: &[(&str, &str)],
    main_text: &str,
    character: u32,
) -> Vec<String> {
    let project = Project::new(name, files);
    let mut server = Server::start(&project, CAPABILITIES);
    for (path, text) in open {
        server.open(&project.uri(path), "typescript", text);
    }
    let main = project.uri("main.ts");
    server.open(&main, "typescript", main_text);
    let response = server.request(
        "textDocument/completion",
        &format!(r#"{{"textDocument":{{"uri":"{main}"}},"position":{{"line":0,"character":{character}}}}}"#),
    );
    let mut labels = string_values(&response, "label");
    labels.sort();
    labels
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
