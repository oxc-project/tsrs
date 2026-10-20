use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::Engine;
use serde_json::{Value, json};
use tsrs_api_codec::{DecodedNode, NodeIndexTable, build_node_index_table, decode_source_file};
use tsrs_ast::SourceFile;
use tsrs_core::P;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A small client for the pinned TypeScript 7 JSON-RPC API. Each package owns a
/// process, so a crashed checker cannot poison another package's result.
pub struct Api {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
}

pub struct SourceAst {
    decoded: DecodedNode,
    table: NodeIndexTable,
}

impl SourceAst {
    pub fn file(&self) -> P<SourceFile> {
        self.decoded.root().as_source_file_p()
    }

    pub fn handle(&self, node: P<tsrs_ast::Node>) -> String {
        tsrs_api_codec::node_handle(&self.table, self.file().get(), node)
    }
}

impl Api {
    pub fn start(binary: &Path, cwd: &Path) -> Result<Self> {
        let mut child = Command::new(binary)
            .args(["--api", "--async", "--cwd"])
            .arg(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or("API stdin unavailable")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("API stdout unavailable")?);
        Ok(Self { child, stdin, stdout, next_id: 1 })
    }

    pub fn request(&mut self, method: &str, params: &Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }))?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;

        let mut length = None;
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line)? == 0 {
                return Err(format!("{method}: API process closed stdout").into());
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some(value) = line.trim().strip_prefix("Content-Length: ") {
                length = Some(value.parse::<usize>()?);
            }
        }
        let length = length.ok_or("API response has no Content-Length")?;
        if length > 256 * 1024 * 1024 {
            return Err("API response exceeds 256 MiB".into());
        }
        let mut bytes = vec![0; length];
        self.stdout.read_exact(&mut bytes)?;
        let response: Value = serde_json::from_slice(&bytes)?;
        if response["id"].as_u64() != Some(u64::from(id)) {
            return Err(format!("{method}: unexpected API response ID: {}", response["id"]).into());
        }
        if !response["error"].is_null() {
            return Err(format!("{method}: {}", response["error"]["message"]).into());
        }
        Ok(response["result"].clone())
    }

    pub fn snapshot(&mut self, configs: &[String]) -> Result<(u64, Vec<Value>)> {
        let response = self.request("createSnapshot", &json!({ "openProjects": configs }))?;
        let snapshot = response["snapshot"].as_u64().ok_or("missing snapshot ID")?;
        let projects = response["projects"].as_array().ok_or("missing projects")?.clone();
        Ok((snapshot, projects))
    }

    pub fn source_ast(&mut self, snapshot: u64, project: &str, file: &str) -> Result<SourceAst> {
        let response = self.request(
            "getSourceFile",
            &json!({ "snapshot": snapshot, "project": project, "file": file }),
        )?;
        let data = response["data"].as_str().ok_or("source file has no AST data")?;
        let data = base64::engine::general_purpose::STANDARD.decode(data)?;
        let decoded = decode_source_file(&data)?;
        let table = build_node_index_table(decoded.root().as_source_file_p().get());
        Ok(SourceAst { decoded, table })
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
