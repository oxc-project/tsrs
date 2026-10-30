#![expect(
    clippy::multiple_crate_versions,
    reason = "regress 0.11 uses hashbrown 0.16; the compiler uses hashbrown 0.17"
)]

#[cfg(not(miri))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

fn main() {
    #[cfg(all(target_os = "linux", not(miri)))]
    tsrs_core::allocator::init_mimalloc();
    std::process::exit(tsrs_lint::cli::main(std::env::args().skip(1).collect()));
}
