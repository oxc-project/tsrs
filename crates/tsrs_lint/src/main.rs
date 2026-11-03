#[cfg(not(miri))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

fn main() -> std::process::ExitCode {
    #[cfg(all(target_os = "linux", not(miri)))]
    tsrs_core::allocator::init_mimalloc();
    let code = tsrs_lint::cli::main(std::env::args().skip(1).collect());
    // Return rather than call `process::exit`: on Windows that skips the exit handlers, so a PGO-instrumented build
    // would write no profile.
    std::process::ExitCode::from(code as u8)
}
