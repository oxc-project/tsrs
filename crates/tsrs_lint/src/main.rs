#[cfg(not(miri))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

fn main() -> std::process::ExitCode {
    #[cfg(all(target_os = "linux", not(miri)))]
    tsrs_core::allocator::init_mimalloc();
    // `std::env::args()` panics on an argument that isn't UTF-8; convert as `tsrs --lint` does.
    let code = tsrs_lint::cli::main(
        std::env::args_os()
            .skip(1)
            .map(|arg| tsrs_core::gocompat::to_valid_utf8(arg.as_encoded_bytes()))
            .collect(),
    );
    // Return rather than call `process::exit`: on Windows that skips the exit handlers, so a PGO-instrumented build
    // would write no profile.
    std::process::ExitCode::from(code as u8)
}
