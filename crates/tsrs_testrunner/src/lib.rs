// The `tsrs-test` harness (src/main.rs is its command line), a library so the integration tests can call it.

pub mod baseline;
pub mod compile;
pub mod compiler_runner;
pub mod compiler_test_filter;
pub mod diagnosticwriter;
pub mod emit_harness;
pub mod harnessutil;
pub mod materialize;
pub mod options;
pub mod pool;
pub mod report;
pub mod sourcemap_baseline;
pub mod sourcemap_recorder;
pub mod test_case_parser;
pub mod tsbaseline;
pub mod type_symbol_baseline;
pub mod types_dump;
pub mod worker;

// Which extra baselines to generate and compare besides `.errors.txt` (bit set of EXTRA_TYPES / EXTRA_SYMBOLS).
// Asking for either runs both walks (Go runs the type walk before the symbol walk in the same program).
pub static EXTRA_BASELINES: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
pub const EXTRA_TYPES: u8 = 1;
pub const EXTRA_SYMBOLS: u8 = 2;
// `.js` emit baselines (docs/EMIT.md section 8). Asking for them switches the compilation to Go's pre-/post-emit
// programs, so the error and `.types`/`.symbols` baselines then come from the post-emit program as in Go.
pub const EXTRA_JS: u8 = 4;
// `.js.map` and `.sourcemap.txt` baselines (docs/EMIT.md section 8); they need the same emitting compilation.
pub const EXTRA_JSMAP: u8 = 8;
pub const EXTRA_SOURCEMAP: u8 = 16;

// Set before compiling, and forwarded to worker processes; unset keeps the upstream harness defaults.
pub static TEST_CHECKERS: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

pub fn test_checkers() -> Option<i64> {
    TEST_CHECKERS.get().copied()
}

pub fn extra_baselines() -> u8 {
    // Relaxed: main stores it before running a command, and threads spawned later see it through the spawn.
    EXTRA_BASELINES.load(std::sync::atomic::Ordering::Relaxed)
}
