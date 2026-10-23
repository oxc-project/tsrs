// Go internal/lsp: the language server (docs/LSP.md).

mod dynamic_queue;
mod logger;
mod lsconsts;
pub mod lsptestutil;
pub mod lspwatcher;
mod progress;
mod server;
mod stack_sanitizer;
mod workerpool;

pub use server::{
    Reader, Server, ServerOptions, Writer, new_server, npmInstallFunc, to_reader, to_writer,
};

#[cfg(test)]
mod dynamic_queue_test;
#[cfg(test)]
mod lspwatcher_test;
#[cfg(test)]
mod progress_test;
#[cfg(all(test, unix))]
mod server_apisession_test;
#[cfg(test)]
mod server_contentmapper_internal_test;
#[cfg(test)]
mod server_flakydiagnostics_test;
#[cfg(test)]
mod server_progress_test;
#[cfg(test)]
mod server_projectinfo_test;
#[cfg(test)]
mod server_test;
#[cfg(test)]
mod stack_sanitizer_test;
#[cfg(test)]
mod workerpool_test;
