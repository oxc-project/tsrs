//! Opt-in performance defaults shared by the CLI and compiler harnesses.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Set before creating programs or checker threads. Explicit feature options take precedence.
pub fn set_enabled(enabled: bool) {
    // Relaxed: this is a configuration flag, not a publication barrier.
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    // Relaxed: see `set_enabled`; workers start after configuration.
    ENABLED.load(Ordering::Relaxed)
}
