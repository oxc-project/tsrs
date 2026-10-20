unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

/// Resident memory after returning free heap pages to the OS (test binaries use the system allocator, whose
/// free lists would otherwise look like growth).
pub(super) fn rss_kib() -> u64 {
    // SAFETY: glibc `malloc_trim` has no preconditions.
    unsafe { malloc_trim(0) };
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}
