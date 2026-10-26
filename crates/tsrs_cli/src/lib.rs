// The `tsrs` binary's API and LSP modes and its interrupt handling (src/main.rs dispatches to them), a library so the
// integration tests can call them.

pub mod api;
pub mod lsp;
pub mod signals;
