// Test-only subprocess entry point for tools/go-fourslash. LSP uses stdio; the Go harness owns the VFS.

#[cfg(unix)]
#[path = "fourslash_server/unix.rs"]
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
