// The `tsrs` binary's API and LSP modes, its interrupt handling and its exit helper (src/main.rs dispatches to them), a
// library so the integration tests can call them.

pub mod api;
pub mod exit_helper;
pub mod lsp;
pub mod signals;
