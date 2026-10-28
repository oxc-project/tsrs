//! `@tsrs/napi`: the `tsrs --api` session inside the Node process.
//!
//! [`SyncRpcChannel`] is a drop-in for the sync client's channel (npm/tsrs/src/api/syncChannel.ts). It takes the
//! arguments the client would spawn `tsrs --api` with and serves the same session (`tsrs_cli::api`) on a thread of
//! this process, over in-process pipes in the same `MessagePack` tuples. A request blocks the JavaScript thread, which
//! answers the session's callbacks (filesystem, module resolution) inline while it waits, as the subprocess channel
//! does. Async API sessions are not hosted yet.

use std::cell::{Cell, RefCell};
use std::io::{PipeReader, PipeWriter};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;

use napi::bindgen_prelude::{Buffer, Env, FnArgs, Function, FunctionRef, Uint8Array};
use napi::{Error, Result};
use napi_derive::napi;
use tsrs_api::adapter::{ClientConnAdapter, SessionHandler};
use tsrs_api_transport::msgpack::{MessagePackReader, MessagePackWriter, MessageType, Tuple};
use tsrs_api_transport::{Connection, ServeOptions, Stream, TransportError};
use tsrs_cli::api;

type Callback = FunctionRef<FnArgs<(String, String)>, String>;

/// The client's ends of the two pipes. Dropping them ends the session thread: its read sees EOF and a pending
/// write fails.
struct Pipes {
    reader: MessagePackReader<PipeReader>,
    writer: MessagePackWriter<PipeWriter>,
}

#[napi]
pub struct SyncRpcChannel {
    pipes: RefCell<Option<Pipes>>,
    server: RefCell<Option<JoinHandle<()>>>,
    callbacks: RefCell<rustc_hash::FxHashMap<String, Rc<Callback>>>,
    /// A failed callback leaves its request's response unread, so the channel rejects every later request with
    /// the same error (syncChannel.ts `callbackFailure`).
    failure: RefCell<Option<Error>>,
    last_bytes_sent: Cell<u32>,
    last_bytes_received: Cell<u32>,
}

#[napi]
#[expect(
    clippy::needless_pass_by_value,
    reason = "napi-rs passes JavaScript arguments as owned values"
)]
impl SyncRpcChannel {
    /// `args` are the `tsrs --api ...` arguments (npm/tsrs/src/api/options.ts `getAPIProcessArgs`).
    #[napi(constructor)]
    pub fn new(args: Vec<String>) -> Result<Self> {
        let flags = match args.split_first() {
            Some((first, rest)) if first == "--api" => api::parse_api_flags(rest).ok(),
            _ => None,
        }
        .filter(|flags| !flags.is_async && flags.pipe_path.is_empty())
        .ok_or_else(|| Error::from_reason(format!("unsupported API arguments: {args:?}")))?;
        let (session, callback_fs) = api::new_api_session(&flags).map_err(Error::from_reason)?;

        let (server_reader, client_writer) = std::io::pipe()?;
        let (client_reader, server_writer) = std::io::pipe()?;
        let stream = Stream {
            reader: Box::new(server_reader),
            writer: Box::new(server_writer),
            closer: None,
        };
        let mut options = ServeOptions::new(false);
        options.collect_timing = flags.timing;
        // Same wiring as `api::run_api`, which serves this session on stdio.
        let conn =
            Connection::new(stream, Arc::new(SessionHandler(Arc::clone(&session))), options, None);
        callback_fs.set_connection(conn.caller());
        session.set_connection(Arc::new(ClientConnAdapter(conn.caller())));
        // Checking recurses deeply: the CLI serves `--api` on a 512 MiB stack too (main.rs).
        let server = std::thread::Builder::new()
            .name("tsrs-api".to_string())
            .stack_size(512 << 20)
            .spawn(move || {
                // A transport error ends the session; the client sees it as EOF on its next read.
                let _ = conn.run();
            })?;

        Ok(SyncRpcChannel {
            pipes: RefCell::new(Some(Pipes {
                reader: MessagePackReader::new(client_reader),
                writer: MessagePackWriter::new(client_writer),
            })),
            server: RefCell::new(Some(server)),
            callbacks: RefCell::default(),
            failure: RefCell::new(None),
            last_bytes_sent: Cell::new(0),
            last_bytes_received: Cell::new(0),
        })
    }

    #[napi]
    pub fn request_sync(&self, env: Env, method: String, payload: String) -> Result<String> {
        let response = self.request(env, &method, payload.as_bytes())?;
        Ok(String::from_utf8(response)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }

    #[napi]
    pub fn request_binary_sync(
        &self,
        env: Env,
        method: String,
        payload: Uint8Array,
    ) -> Result<Buffer> {
        Ok(self.request(env, &method, &payload)?.into())
    }

    #[napi]
    pub fn register_callback(
        &self,
        name: String,
        callback: Function<FnArgs<(String, String)>, String>,
    ) -> Result<()> {
        self.callbacks.borrow_mut().insert(name, Rc::new(callback.create_ref()?));
        Ok(())
    }

    #[napi]
    pub fn unregister_callback(&self, name: String) {
        self.callbacks.borrow_mut().remove(&name);
    }

    /// Ends the session and waits for its thread, which frees the session's snapshots and programs.
    #[napi]
    pub fn close(&self) {
        drop(self.pipes.borrow_mut().take());
        if let Some(server) = self.server.borrow_mut().take() {
            let _ = server.join();
        }
    }

    #[napi(getter)]
    pub fn last_bytes_sent(&self) -> u32 {
        self.last_bytes_sent.get()
    }

    #[napi(getter)]
    pub fn last_bytes_received(&self) -> u32 {
        self.last_bytes_received.get()
    }

    /// syncChannel.ts `requestBytesSync`. No borrow is held while a callback runs, so a callback may make
    /// requests of its own; the session handles them as nested requests.
    fn request(&self, env: Env, method: &str, payload: &[u8]) -> Result<Vec<u8>> {
        if self.pipes.borrow().is_none() {
            return Err(closed());
        }
        if let Some(failure) = &*self.failure.borrow() {
            return Err(failure.try_clone()?);
        }
        self.last_bytes_sent.set(payload.len() as u32);
        self.last_bytes_received.set(0);
        self.write(MessageType::Request, method.as_bytes(), payload)?;
        loop {
            let Tuple { msg_type, method: name, payload } = self.read()?;
            match msg_type {
                MessageType::Response | MessageType::Error if name != method.as_bytes() => {
                    return Err(Error::from_reason(format!(
                        "name mismatch for response: expected `{method}`, got `{}`",
                        String::from_utf8_lossy(&name)
                    )));
                }
                MessageType::Response => {
                    self.last_bytes_received.set(payload.len() as u32);
                    return Ok(payload);
                }
                MessageType::Error => {
                    return Err(Error::from_reason(String::from_utf8_lossy(&payload)));
                }
                MessageType::Call => {
                    if let Err(error) = self.call(env, &String::from_utf8_lossy(&name), &payload) {
                        *self.failure.borrow_mut() = Some(error.try_clone()?);
                        return Err(error);
                    }
                }
                other => {
                    return Err(Error::from_reason(format!(
                        "Invalid message type from the API session: {}",
                        other as u8
                    )));
                }
            }
        }
    }

    /// syncChannel.ts `handleCall`: answers the session, then fails the request if the callback did.
    fn call(&self, env: Env, name: &str, payload: &[u8]) -> Result<()> {
        let callback = self.callbacks.borrow().get(name).cloned();
        let Some(callback) = callback else {
            let message = format!(
                "unknown callback: `{name}`. Please make sure to register it on the JavaScript side before invoking it."
            );
            self.write(MessageType::CallError, name.as_bytes(), message.as_bytes())?;
            return Err(Error::from_reason(format!("no callback named `{name}` found")));
        };
        let arg = String::from_utf8_lossy(payload).into_owned();
        match callback.borrow_back(&env)?.call(FnArgs::from((name.to_string(), arg))) {
            Ok(result) => self.write(MessageType::CallResponse, name.as_bytes(), result.as_bytes()),
            // tsrs-only: the request rethrows the callback's own exception instead of wrapping its message.
            Err(error) => {
                self.write(
                    MessageType::CallError,
                    name.as_bytes(),
                    error.reason.trim().as_bytes(),
                )?;
                Err(error)
            }
        }
    }

    fn write(&self, msg_type: MessageType, name: &[u8], payload: &[u8]) -> Result<()> {
        let mut pipes = self.pipes.borrow_mut();
        let pipes = pipes.as_mut().ok_or_else(closed)?;
        pipes.writer.write_tuple(msg_type, name, payload).map_err(|e| transport_error(&e))
    }

    fn read(&self) -> Result<Tuple> {
        let mut pipes = self.pipes.borrow_mut();
        let pipes = pipes.as_mut().ok_or_else(closed)?;
        pipes.reader.read_tuple().map_err(|e| transport_error(&e))
    }
}

fn closed() -> Error {
    Error::from_reason("SyncRpcChannel is closed")
}

fn transport_error(error: &TransportError) -> Error {
    if error.is_eof() {
        return Error::from_reason("Unexpected EOF while reading from the API session");
    }
    Error::from_reason(error.to_string())
}
