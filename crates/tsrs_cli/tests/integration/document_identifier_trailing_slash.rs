// Go converts each API `DocumentIdentifier` with `ToFileName(cwd)`. For a file name it calls `ToRootedFilePath`, which
// normalizes the path and removes a trailing `/`, so `<dir>/a.ts/` names the source file `<dir>/a.ts`. tsrs kept the
// trailing separator, found no source file, and answered `null` (`getSourceFileMetadata`) or `source file not found`
// (`getSymbolAtPosition`). The test starts `tsrs --api --async` as a child process. The expected text comes from the
// same requests to `tsgo --api --async`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tsrs-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    dir
}

/// A `tsrs --api --async` child process. The protocol is JSON-RPC 2.0 with `Content-Length` framing.
struct ApiServer {
    child: Child,
    stdin: ChildStdin,
    frames: Receiver<String>,
    next_id: u64,
}

impl ApiServer {
    fn start(dir: &Path, case_sensitive: bool) -> ApiServer {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tsrs"))
            .args(["--api", "--async", "--cwd"])
            .arg(dir)
            .arg(format!("--useCaseSensitiveFileNames={case_sensitive}"))
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, frames) = channel();
        std::thread::spawn(move || {
            loop {
                let mut length = None;
                loop {
                    let mut line = String::new();
                    if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:") {
                        length = value.trim().parse::<usize>().ok();
                    }
                }
                let mut body = vec![0u8; length.unwrap()];
                if stdout.read_exact(&mut body).is_err() {
                    return;
                }
                if tx.send(String::from_utf8_lossy(&body).into_owned()).is_err() {
                    return;
                }
            }
        });
        ApiServer { child, stdin, frames, next_id: 0 }
    }

    /// Sends one request and returns the response text after `{"jsonrpc":"2.0","id":N,` without
    /// the closing brace, that is `"result":...` or `"error":...`. `params` is raw JSON text, or
    /// `None` to leave the `params` member out of the request.
    fn call(&mut self, method: &str, params: Option<&str>) -> String {
        self.next_id += 1;
        let id = self.next_id;
        let params = params.map_or_else(String::new, |p| format!(r#","params":{p}"#));
        let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"{method}"{params}}}"#);
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
        let prefix = format!(r#"{{"jsonrpc":"2.0","id":{id},"#);
        loop {
            let frame = self
                .frames
                .recv_timeout(Duration::from_secs(120))
                .unwrap_or_else(|_| panic!("no response to `{method}`"));
            if let Some(rest) = frame.strip_prefix(&prefix) {
                return rest.strip_suffix('}').unwrap_or(rest).to_string();
            }
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn path_text(dir: &Path) -> String {
    dir.to_string_lossy().replace('\\', "/")
}

const METADATA: &str = r#""result":{"isDefaultLibrary":false,"isFromExternalLibrary":false,"packageJsonType":"","packageJsonDirectory":"","impliedNodeFormat":1}"#;

/// Starts a server on a one-file project and returns it with the project directory and the snapshot parameters.
fn open_project(name: &str) -> (ApiServer, PathBuf, String) {
    let dir = scratch(
        name,
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"strict":true,"noEmit":true,"moduleResolution":"bundler","module":"esnext"},"include":["*.ts"]}"#,
            ),
            ("a.ts", "export const a: number = 1;\n"),
        ],
    );
    let root = path_text(&dir);
    let mut server = ApiServer::start(&dir, true);
    server.call("initialize", None);
    let config = format!(r#"{{"openProjects":["{root}/tsconfig.json"]}}"#);
    let snapshot = server.call("createSnapshot", Some(&config));
    assert!(snapshot.contains(r#""snapshot":1"#), "{snapshot}");
    let base = format!(r#""snapshot":1,"project":"{root}/tsconfig.json""#);
    (server, dir, base)
}

#[test]
fn file_name_with_trailing_slash() {
    let (mut server, dir, base) = open_project("document-identifier-trailing-slash-name");
    let root = path_text(&dir);

    // The plain name works in both servers.
    let plain = format!(r#"{{{base},"file":"{root}/a.ts"}}"#);
    assert_eq!(server.call("getSourceFileMetadata", Some(&plain)), METADATA);

    // The name with a trailing slash names the same file in tsgo.
    let slash = format!(r#"{{{base},"file":"{root}/a.ts/"}}"#);
    let metadata = server.call("getSourceFileMetadata", Some(&slash));
    let symbol_params = format!(r#"{{{base},"file":"{root}/a.ts/","position":13}}"#);
    let symbol = server.call("getSymbolAtPosition", Some(&symbol_params));
    let diagnostics_params = format!(r#"{{{base},"files":["{root}/a.ts/"]}}"#);
    let diagnostics = server.call("getSemanticDiagnostics", Some(&diagnostics_params));
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(metadata, METADATA, "getSourceFileMetadata");
    assert!(
        symbol.starts_with(r#""result":{"reference""#) && symbol.contains(r#""name":"a""#),
        "getSymbolAtPosition: {symbol}"
    );
    assert_eq!(diagnostics, r#""result":[]"#, "getSemanticDiagnostics");
}
