// Go `ParseCommandLine` maps every parsed file name through `tspath.ToRootedFilePath(name, currentDirectory)`, so
// the API request `parseCommandLine {"commandLine": ["a.ts"]}` returns `fileNames: ["<cwd>/a.ts"]`, where the
// current directory is the `--cwd` of the server. tsrs does not root the names and returns `["a.ts"]`, and
// `["src/b.ts", "../c.ts"]` for relative names with directories. The test starts `tsrs --api --async` as a child
// process. The expected text comes from the same requests to `tsgo --api --async`.

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

#[test]
fn relative_file_names_are_rooted_at_the_server_cwd() {
    let dir = scratch("parse-command-line-file-names-not-rooted", &[("a.ts", "export {};\n")]);
    let root = path_text(&dir);
    let parent = path_text(dir.parent().unwrap());
    let mut server = ApiServer::start(&dir, true);
    server.call("initialize", None);

    // An absolute name stays as it is in both servers.
    let absolute = format!(r#"{{"commandLine":["{root}/d.ts"]}}"#);
    assert_eq!(
        server.call("parseCommandLine", Some(&absolute)),
        format!(r#""result":{{"fileNames":["{root}/d.ts"],"options":{{}},"errors":[]}}"#)
    );

    // A relative name is joined with the server cwd in tsgo.
    let single = server.call("parseCommandLine", Some(r#"{"commandLine":["a.ts"]}"#));
    let nested = server
        .call("parseCommandLine", Some(r#"{"commandLine":["src/b.ts","../c.ts","--strict"]}"#));
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        single,
        format!(r#""result":{{"fileNames":["{root}/a.ts"],"options":{{}},"errors":[]}}"#)
    );
    assert_eq!(
        nested,
        format!(
            r#""result":{{"fileNames":["{root}/src/b.ts","{parent}/c.ts"],"options":{{"strict":true}},"raw":{{"strict":true}},"errors":[]}}"#
        )
    );
}
