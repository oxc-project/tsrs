// Test-only subprocess entry point for tools/go-fourslash. The Go harness owns the VFS.

#[cfg(unix)]
mod unix;

fn main() {
    #[cfg(unix)]
    std::thread::Builder::new()
        .name("fourslash-server".into())
        .stack_size(512 << 20)
        .spawn(unix::run)
        .expect("spawn server")
        .join()
        .expect("server panicked");

    #[cfg(not(unix))]
    panic!("the Go fourslash subprocess prototype requires Unix sockets");
}
