unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

/// Resident memory after returning free heap pages to the OS (test binaries use the system allocator, whose
/// free lists would otherwise look like growth).
pub fn rss_kib() -> u64 {
    // SAFETY: glibc `malloc_trim` has no preconditions.
    unsafe { malloc_trim(0) };
    // `status`'s VmRSS uses approximate counters; walk the mappings for an accurate snapshot.
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .expect("read process memory mappings")
        .lines()
        .find(|l| l.starts_with("Rss:"))
        .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        .expect("read resident memory from smaps_rollup")
}
