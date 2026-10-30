// The `tsrs` binary's API and LSP modes and its interrupt handling (src/main.rs dispatches to them), a library so the
// integration tests can call them.
#![expect(
    clippy::multiple_crate_versions,
    reason = "the linter's regress 0.11 uses hashbrown 0.16; the compiler uses hashbrown 0.17"
)]

pub mod api;
pub mod lsp;
pub mod signals;
