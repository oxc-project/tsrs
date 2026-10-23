// Not a Go test (Go has no language-server test for `custom/initializeAPISession`). An editor opens a file with
// unsaved text, asks the server for an API session, and a JSON-RPC client on the returned socket sees that text
// through `getCurrentLanguageServerSnapshot`. Once the client disconnects, the session is removed and the files it
// opened are closed in the language server.

use std::io::{BufRead, BufReader, Read, Write};
use std::time::{Duration, Instant};

use tsrs_core::json::{self, Value};
use tsrs_lsproto as lsproto;

use crate::lsptestutil::{acknowledge_registrations, new_lsp_client, test_server_options};

const CONFIG_FILE_NAME: &str = "/home/projects/p/tsconfig.json";
const FILE_NAME: &str = "/home/projects/p/index.ts";
const LOOSE_FILE_NAME: &str = "/home/projects/loose.ts";

struct ApiClient {
    reader: BufReader<std::os::unix::net::UnixStream>,
    writer: std::os::unix::net::UnixStream,
    next_id: u64,
}

impl ApiClient {
    fn connect(pipe: &str) -> ApiClient {
        let writer = std::os::unix::net::UnixStream::connect(pipe).unwrap();
        writer.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        writer.set_write_timeout(Some(Duration::from_secs(30))).unwrap();
        ApiClient { reader: BufReader::new(writer.try_clone().unwrap()), writer, next_id: 0 }
    }

    // Async API protocol: JSON-RPC 2.0 with Content-Length framing.
    fn request(&mut self, method: &str, params: &str) -> Value {
        self.next_id += 1;
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"{method}","params":{params}}}"#,
            self.next_id
        );
        write!(self.writer, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.writer.flush().unwrap();

        let mut length = 0;
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.reader.read_exact(&mut body).unwrap();
        let response = json::unmarshal(std::str::from_utf8(&body).unwrap()).unwrap();
        let Value::Object(members) = &response else { panic!("not an object: {response:?}") };
        if let Some(error) = members.get("error") {
            panic!("{method} failed: {error:?}");
        }
        members.get("result").cloned().unwrap()
    }
}

fn get<'a>(v: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(v, |cur, key| match cur {
        Value::Object(o) => o.get(key).unwrap_or_else(|| panic!("missing {key} in {cur:?}")),
        Value::Array(a) => &a[key.parse::<usize>().unwrap()],
        _ => panic!("cannot index {key} in {cur:?}"),
    })
}

fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        _ => panic!("not a number: {v:?}"),
    }
}

#[test]
fn api_session_sees_the_language_servers_open_files() {
    let (client, close_client) = new_lsp_client(
        test_server_options(
            "/home/projects",
            &[
                (CONFIG_FILE_NAME, "{}"),
                (FILE_NAME, "export const x: number = 1;"),
                (LOOSE_FILE_NAME, "export const loose = 1;"),
            ],
        ),
        Some(acknowledge_registrations()),
    );
    let (init_msg, _) =
        client.send_request(lsproto::INITIALIZE_INFO, lsproto::InitializeParams::default());
    assert!(init_msg.error.is_none(), "Initialize failed");
    client.send_notification(lsproto::INITIALIZED_INFO, lsproto::InitializedParams::default());
    client.server.init_complete().wait();
    client.send_notification(
        lsproto::TEXT_DOCUMENT_DID_OPEN_INFO,
        lsproto::DidOpenTextDocumentParams {
            text_document: lsproto::TextDocumentItem {
                uri: lsproto::DocumentUri::from(format!("file://{FILE_NAME}").as_str()),
                language_id: lsproto::LanguageKind::TYPE_SCRIPT,
                version: 1,
                text: r#"export const x: number = "unsaved";"#.to_string(),
            },
        },
    );

    let (msg, result) = client.send_request(
        lsproto::CUSTOM_INITIALIZE_API_SESSION_INFO,
        lsproto::InitializeAPISessionParams::default(),
    );
    assert!(msg.error.is_none(), "{:?}", msg.error);
    let result = result.unwrap();
    assert!(result.pipe.contains("tsgo-api-"), "{}", result.pipe);
    assert!(client.server.api_sessions.lock().unwrap().contains_key(&result.session_id));

    let mut api = ApiClient::connect(&result.pipe);
    let initialized = api.request("initialize", "null");
    assert_eq!(get(&initialized, "currentDirectory"), &Value::String("/home/projects".into()));
    let snapshot = api.request(
        "getCurrentLanguageServerSnapshot",
        &format!(r#"{{"changes":{{"openFiles":["{FILE_NAME}","{LOOSE_FILE_NAME}"]}}}}"#),
    );
    assert_eq!(
        get(&snapshot, "operation.openedFiles.0.project"),
        &Value::String(CONFIG_FILE_NAME.into())
    );
    assert!(client.server.session().snapshot().project_collection.inferred_project().is_some());
    let diagnostics = api.request(
        "getSemanticDiagnostics",
        &format!(
            r#"{{"snapshot":{},"project":"{CONFIG_FILE_NAME}","files":["{FILE_NAME}"]}}"#,
            number(get(&snapshot, "snapshot"))
        ),
    );
    // TS2322: the unsaved text, not the file on disk.
    assert_eq!(number(get(&diagnostics, "0.code")), 2322.0);

    drop(api);
    let deadline = Instant::now() + Duration::from_secs(30);
    while client.server.api_sessions.lock().unwrap().contains_key(&result.session_id) {
        assert!(Instant::now() < deadline, "the API session was not removed after disconnect");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Only the API session had the loose file open; closing the session closed it in the language server.
    assert!(client.server.session().snapshot().project_collection.inferred_project().is_none());
    let _ = close_client.close();
}

#[test]
fn api_session_builds_project_references_and_cleans_outputs() {
    let (client, close_client) = new_lsp_client(
        test_server_options(
            "/home/projects",
            &[
                (
                    "/home/projects/core/tsconfig.json",
                    r#"{"compilerOptions":{"composite":true,"outDir":"out"}}"#,
                ),
                ("/home/projects/core/index.ts", "export const one = 1;\n"),
                (
                    "/home/projects/app/tsconfig.json",
                    r#"{"compilerOptions":{"composite":true,"outDir":"out"},"references":[{"path":"../core"}]}"#,
                ),
                (
                    "/home/projects/app/main.ts",
                    "import { one } from '../core/index';\nexport const two: number = one + 1;\n",
                ),
            ],
        ),
        Some(acknowledge_registrations()),
    );
    let (initialize, _) =
        client.send_request(lsproto::INITIALIZE_INFO, lsproto::InitializeParams::default());
    assert!(initialize.error.is_none(), "{:?}", initialize.error);
    client.send_notification(lsproto::INITIALIZED_INFO, lsproto::InitializedParams::default());
    client.server.init_complete().wait();
    let (response, session) = client.send_request(
        lsproto::CUSTOM_INITIALIZE_API_SESSION_INFO,
        lsproto::InitializeAPISessionParams::default(),
    );
    assert!(response.error.is_none(), "{:?}", response.error);
    let mut api = ApiClient::connect(&session.unwrap().pipe);
    api.request("initialize", "null");
    let created = api.request("createBuildOrchestrator", r#"{"rootNames":["app"]}"#);
    let id = number(get(&created, "buildOrchestratorID"));
    let params = format!(r#"{{"buildOrchestratorID":{id},"project":"app"}}"#);
    let fs = client.server.session().snapshot_host().fs();

    let references = api.request("buildReferences", &params);
    assert_eq!(number(get(&references, "status")), 0.0, "{references:?}");
    assert_eq!(number(get(&references, "statistics.Projects")), 1.0);
    assert!(fs.file_exists("/home/projects/core/out/index.js"));
    assert!(!fs.file_exists("/home/projects/app/out/main.js"));

    let built = api.request("build", &params);
    assert_eq!(number(get(&built, "status")), 0.0, "{built:?}");
    assert_eq!(number(get(&built, "statistics.Projects")), 2.0);
    assert!(fs.file_exists("/home/projects/app/out/main.js"));

    let references = api.request("cleanReferences", &params);
    assert_eq!(number(get(&references, "status")), 0.0);
    assert!(!fs.file_exists("/home/projects/core/out/index.js"));
    assert!(fs.file_exists("/home/projects/app/out/main.js"));
    let cleaned = api.request("cleanBuild", &params);
    assert_eq!(number(get(&cleaned, "status")), 0.0);
    assert!(!fs.file_exists("/home/projects/app/out/main.js"));
    assert_eq!(api.request("disposeBuildOrchestrator", &params), Value::Bool(true));
    drop(api);
    let _ = close_client.close();
}

// Not a Go test. Shutdown stops every API session (server.go:1832 closeAPISessions), whether a client connected
// or not.
#[test]
fn close_api_sessions_stops_pending_and_connected_sessions() {
    let (client, close_client) = new_lsp_client(
        test_server_options("/home/projects", &[(CONFIG_FILE_NAME, "{}"), (FILE_NAME, "")]),
        Some(acknowledge_registrations()),
    );
    let (init_msg, _) =
        client.send_request(lsproto::INITIALIZE_INFO, lsproto::InitializeParams::default());
    assert!(init_msg.error.is_none(), "Initialize failed");
    client.send_notification(lsproto::INITIALIZED_INFO, lsproto::InitializedParams::default());
    client.server.init_complete().wait();

    let start_session = || {
        let (msg, result) = client.send_request(
            lsproto::CUSTOM_INITIALIZE_API_SESSION_INFO,
            lsproto::InitializeAPISessionParams::default(),
        );
        assert!(msg.error.is_none(), "{:?}", msg.error);
        result.unwrap()
    };
    let pending = start_session();
    let connected = start_session();
    let mut api = ApiClient::connect(&connected.pipe);
    api.request("initialize", "null");
    assert_eq!(client.server.api_sessions.lock().unwrap().len(), 2);

    // The typed test client emits `{}` for NoParams, so exercise shutdown's cleanup directly here.
    client.server.close_api_sessions();
    assert!(client.server.api_sessions.lock().unwrap().is_empty());
    // The pending session's socket is gone and the connected one is closed.
    assert!(std::os::unix::net::UnixStream::connect(&pending.pipe).is_err());
    let mut buf = [0; 1];
    assert_eq!(api.reader.get_mut().read(&mut buf).unwrap(), 0);
    let _ = close_client.close();
}
