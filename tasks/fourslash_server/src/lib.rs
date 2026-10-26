// The fourslash subprocess server (src/main.rs runs it), a library so the integration tests can call it.

#[cfg(unix)]
pub mod unix;
