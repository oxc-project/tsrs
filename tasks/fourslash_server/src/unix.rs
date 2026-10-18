use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tsrs_core::P;
use tsrs_core::context::Context;
use tsrs_core::json::{self, Value};
use tsrs_vfs::{Entries, FS, FileInfo, FileMode};

mod state;

struct RemoteFS {
    stream: Mutex<BufReader<UnixStream>>,
    case_sensitive: bool,
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    let Value::Object(object) = value else { panic!("expected an object") };
    object.get(key).unwrap_or(&Value::Null)
}

fn string(value: &Value) -> &str {
    let Value::String(value) = value else { panic!("expected a string: {value:?}") };
    value
}

fn integer(value: &Value) -> i64 {
    match value {
        Value::Number(value) => *value as i64,
        Value::Integer(value) => *value,
        _ => panic!("expected an integer: {value:?}"),
    }
}

fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(values) => values.iter().map(|v| string(v).to_string()).collect(),
        _ => panic!("expected an array: {value:?}"),
    }
}

impl RemoteFS {
    fn call(&self, method: &str, args: &[Value]) -> Value {
        let request = Value::Array(vec![Value::String(method.into()), Value::Array(args.to_vec())]);
        let mut stream = self.stream.lock().unwrap();
        writeln!(stream.get_mut(), "{}", json::marshal(&request).unwrap())
            .expect("write to Go VFS");
        let mut line = String::new();
        assert!(stream.read_line(&mut line).expect("read Go VFS") != 0, "Go VFS disconnected");
        let response = json::unmarshal(&line).expect("decode Go VFS response");
        assert_eq!(field(&response, "error"), &Value::Null, "Go VFS protocol error: {response:?}");
        let result = field(&response, "result");
        if method == "readFile" && result != &Value::Null {
            let length = usize::try_from(integer(result)).expect("nonnegative file length");
            let mut contents = vec![0; length];
            stream.read_exact(&mut contents).expect("read Go file contents");
            return Value::String(String::from_utf8(contents).expect("Go sends valid UTF-8"));
        }
        result.clone()
    }

    fn path_call(&self, method: &str, path: &str) -> Value {
        self.call(method, &[Value::String(path.into())])
    }
}

fn mutation_result(value: &Value) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        _ => Err(string(value).to_string()),
    }
}

fn timestamp(time: SystemTime) -> Value {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    Value::Number(seconds)
}

impl FS for RemoteFS {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.case_sensitive
    }

    fn file_exists(&self, path: &str) -> bool {
        self.path_call("fileExists", path) == Value::Bool(true)
    }

    fn read_file(&self, path: &str) -> Option<String> {
        match self.path_call("readFile", path) {
            Value::Null => None,
            Value::String(contents) => Some(contents),
            value => panic!("expected file contents: {value:?}"),
        }
    }

    fn write_file(&self, path: &str, data: &str) -> Result<(), String> {
        mutation_result(
            &self.call("writeFile", &[Value::String(path.into()), Value::String(data.into())]),
        )
    }

    fn append_file(&self, path: &str, data: &str) -> Result<(), String> {
        mutation_result(
            &self.call("appendFile", &[Value::String(path.into()), Value::String(data.into())]),
        )
    }

    fn remove(&self, path: &str) -> Result<(), String> {
        mutation_result(&self.path_call("remove", path))
    }

    fn chtimes(&self, path: &str, a_time: SystemTime, m_time: SystemTime) -> Result<(), String> {
        mutation_result(
            &self.call(
                "chtimes",
                &[Value::String(path.into()), timestamp(a_time), timestamp(m_time)],
            ),
        )
    }

    fn directory_exists(&self, path: &str) -> bool {
        self.path_call("directoryExists", path) == Value::Bool(true)
    }

    fn get_accessible_entries(&self, path: &str) -> Entries {
        let value = self.path_call("entries", path);
        Entries {
            files: strings(field(&value, "files")),
            directories: strings(field(&value, "directories")),
            symlinks: match field(&value, "symlinks") {
                Value::Null => None,
                names => Some(strings(names).into_iter().collect()),
            },
        }
    }

    fn stat(&self, path: &str) -> Option<FileInfo> {
        let value = self.path_call("stat", path);
        if value == Value::Null {
            return None;
        }
        let seconds = integer(field(&value, "seconds"));
        let nanos = u32::try_from(integer(field(&value, "nanos"))).expect("nanoseconds fit u32");
        let mod_time = if seconds < 0 {
            UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
        } else {
            UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
        }
        .and_then(|time| time.checked_add(Duration::from_nanos(u64::from(nanos))));
        Some(FileInfo {
            name: string(field(&value, "name")).into(),
            size: integer(field(&value, "size")),
            mode: FileMode::from_bits_retain(
                u32::try_from(integer(field(&value, "mode"))).expect("file mode fits u32"),
            ),
            mod_time,
        })
    }

    fn realpath(&self, path: &str) -> String {
        string(&self.path_call("realpath", path)).into()
    }
}

pub fn run() {
    tsrs_core::compat::use_go_history_for_tsgo_baselines();
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        run_worker();
        return;
    }
    let socket =
        std::env::args().nth(1).expect("usage: fourslash_server <VFS socket> <state socket>");
    let state_socket = std::env::args().nth(2).expect("missing project-state socket");
    run_server(
        &socket,
        state_socket,
        tsrs_lsp::to_reader(std::io::stdin()),
        tsrs_lsp::to_writer(std::io::stdout()),
        None,
    );
}

fn run_worker() {
    let cache = Arc::new(tsrs_project::new_parse_cache(tsrs_project::RefCountCacheOptions {
        disable_deletion: true,
    }));
    for line in std::io::stdin().lock().lines() {
        let value =
            json::unmarshal(&line.expect("read worker command")).expect("decode worker command");
        let dir = string(&value).to_string();
        let cache = Arc::clone(&cache);
        std::thread::Builder::new()
            .name("fourslash-case".into())
            .stack_size(512 << 20)
            .spawn(move || {
                let lsp = UnixStream::connect(format!("{dir}/lsp.sock"))
                    .expect("connect to Go LSP client");
                run_server(
                    &format!("{dir}/vfs.sock"),
                    format!("{dir}/state.sock"),
                    tsrs_lsp::to_reader(lsp.try_clone().expect("clone LSP stream")),
                    tsrs_lsp::to_writer(lsp.try_clone().expect("clone LSP stream")),
                    Some(cache),
                );
                let _ = lsp.shutdown(Shutdown::Both);
            })
            .expect("spawn case")
            .join()
            .expect("case panicked");
        println!("true");
        std::io::stdout().flush().expect("finish worker case");
    }
}

fn run_server(
    socket: &str,
    state_socket: String,
    input: Box<dyn tsrs_lsp::Reader>,
    output: Box<dyn tsrs_lsp::Writer>,
    parse_cache: Option<Arc<tsrs_project::ParseCache>>,
) {
    let stream = UnixStream::connect(socket).expect("connect to Go VFS");
    let mut fs = RemoteFS { stream: Mutex::new(BufReader::new(stream)), case_sensitive: true };
    let config = fs.call("configure", &[]);
    fs.case_sensitive = field(&config, "caseSensitive") == &Value::Bool(true);
    let options =
        tsrs_tsoptions::gojson::compiler_options_from_go_json(field(&config, "compilerOptions"))
            .expect("decode inferred-project options");
    let fs = Arc::new(fs);
    let server = tsrs_lsp::new_server(tsrs_lsp::ServerOptions {
        in_: input,
        out: output,
        err: Box::new(std::io::stderr()),
        cwd: string(field(&config, "cwd")).into(),
        fs: Some(Arc::clone(&fs) as Arc<dyn FS>),
        default_library_path: string(field(&config, "defaultLibraryPath")).into(),
        typings_location: String::new(),
        parse_cache,
        npm_install: None,
        progress_delay: Duration::ZERO,
        set_parent_process_id: None,
    });
    let ctx = Context::background();
    server.set_compiler_options_for_inferred_projects(&ctx, Some(P::new(options)));
    let ready = server.init_complete();
    let state_server = Arc::clone(&server);
    let state_thread = std::thread::spawn(move || {
        ready.wait();
        let stream = UnixStream::connect(state_socket).expect("connect to Go state client");
        fs.call("ready", &[]);
        state::serve(stream, &state_server, fs.case_sensitive);
    });
    server.run(&ctx).expect("LSP server failed");
    state_thread.join().expect("state bridge panicked");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_payload_preserves_framing_and_missing_vs_empty() {
        let (client, peer) = UnixStream::pair().unwrap();
        let contents = "λ🦀\n\0\"\\".repeat(2048);
        let expected = contents.clone();
        let bridge = std::thread::spawn(move || {
            let mut peer = BufReader::new(peer);
            for payload in [None, Some(String::new()), Some(contents), Some("next file".into())] {
                let mut request = String::new();
                assert!(peer.read_line(&mut request).unwrap() > 0);
                match payload {
                    None => writeln!(peer.get_mut(), "{{\"result\":null}}").unwrap(),
                    Some(contents) => {
                        writeln!(peer.get_mut(), "{{\"result\":{}}}", contents.len()).unwrap();
                        peer.get_mut().write_all(contents.as_bytes()).unwrap();
                    }
                }
            }
        });
        let fs = RemoteFS { stream: Mutex::new(BufReader::new(client)), case_sensitive: true };
        assert_eq!(fs.read_file("/missing.ts"), None);
        assert_eq!(fs.read_file("/empty.ts"), Some(String::new()));
        assert_eq!(fs.read_file("/unicode.ts"), Some(expected));
        assert_eq!(fs.read_file("/next.ts"), Some("next file".into()));
        bridge.join().unwrap();
    }

    #[test]
    #[should_panic(expected = "read Go file contents")]
    fn truncated_file_payload_fails() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        peer.write_all(b"{\"result\":10}\nshort").unwrap();
        peer.shutdown(Shutdown::Write).unwrap();
        let fs = RemoteFS { stream: Mutex::new(BufReader::new(client)), case_sensitive: true };
        fs.read_file("/truncated.ts");
    }
}
