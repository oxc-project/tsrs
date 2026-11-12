//! Startup policy shared by the binaries that use mimalloc as their global allocator.

/// Restores transparent huge pages for the arena after mimalloc's startup constructor disables them.
/// Call at the start of each mimalloc binary's `main`, before spawning worker threads.
#[cfg(all(target_os = "linux", not(miri)))]
pub fn init_mimalloc() {
    // libmimalloc-sys2's `no_thp` feature sets `allow_thp = 0`, which disables THP for the whole process in
    // `_mi_prim_mem_init`. Keep that allocator option to avoid huge-page amplification on its heap, but
    // restore THP for the arena's `MADV_HUGEPAGE` chunks (notes/mem-no-thp.md). An explicit environment
    // override keeps mimalloc's choice.
    if std::env::var_os("MIMALLOC_ALLOW_THP").is_some() {
        return;
    }
    let zero: libc::c_ulong = 0;
    // SAFETY: PR_SET_THP_DISABLE takes integer arguments only and changes nothing but this process's THP flag.
    unsafe { libc::prctl(libc::PR_SET_THP_DISABLE, zero, zero, zero, zero) };
}

/// mimalloc as a `GlobalAlloc`, through its plain entry points where they already give the alignment.
///
/// `mimalloc_safe::MiMalloc` sends every request to `mi_malloc_aligned` / `mi_zalloc_aligned` /
/// `mi_realloc_aligned`, whose small-block path tests the alignment and the free block's address before it reaches
/// the same page allocation that `mi_malloc` inlines. Rust std's unix `System` allocator and Bun's allocator shim call
/// the plain function when `align <= MIN_ALIGN && align <= size`; this does the same with mimalloc's own guarantee.
/// mimalloc v3 rounds every size class of at least 9 bytes to a multiple of `MI_MAX_ALIGN_SIZE` (16) on 64-bit and
/// wasm32 (`mi_bin` in `page-queue.c`, two or four machine words) and starts each page's blocks 16-aligned, so a block
/// of `size >= align` is `align`-aligned for any `align <= 16`; smaller sizes get a word-aligned block of at least
/// `size` bytes (`alloc.c` asserts it in `_mi_page_malloc_zero`). Same blocks, same size classes: only the path to
/// them changes (notes/perf-small-exact-wins-20261112.md).
#[cfg(feature = "mimalloc")]
pub struct MiMalloc;

/// mimalloc's `MI_MAX_ALIGN_SIZE`: every block of at least this many bytes is aligned to it.
#[cfg(feature = "mimalloc")]
const MI_MAX_ALIGN_SIZE: usize = 16;

#[cfg(feature = "mimalloc")]
impl MiMalloc {
    #[inline]
    fn plain(align: usize, size: usize) -> bool {
        align <= MI_MAX_ALIGN_SIZE && align <= size
    }
}

#[cfg(feature = "mimalloc")]
// SAFETY: every path returns a block from mimalloc of at least `size` bytes aligned to `align` (the plain functions
// only when `plain` holds, see the type's comment), or null on failure; `mi_free` and `mi_realloc` accept any block
// from either family.
unsafe impl std::alloc::GlobalAlloc for MiMalloc {
    #[inline]
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        use libmimalloc_sys2 as ffi;
        if Self::plain(layout.align(), layout.size()) {
            // SAFETY: mimalloc's malloc takes any size; `plain` guarantees the block is aligned enough.
            unsafe { ffi::mi_malloc(layout.size()).cast() }
        } else {
            // SAFETY: `layout.align()` is a power of two, as mimalloc requires.
            unsafe { ffi::mi_malloc_aligned(layout.size(), layout.align()).cast() }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        use libmimalloc_sys2 as ffi;
        if Self::plain(layout.align(), layout.size()) {
            // SAFETY: as in `alloc`.
            unsafe { ffi::mi_zalloc(layout.size()).cast() }
        } else {
            // SAFETY: as in `alloc`.
            unsafe { ffi::mi_zalloc_aligned(layout.size(), layout.align()).cast() }
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
        // SAFETY: the caller passes a block this allocator returned; `mi_free` takes blocks from every mimalloc entry.
        unsafe { libmimalloc_sys2::mi_free(ptr.cast()) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        use libmimalloc_sys2 as ffi;
        if Self::plain(layout.align(), new_size) {
            // SAFETY: the caller passes a live block of this allocator; `plain` guarantees the new block's alignment,
            // and `mi_realloc` copies `min(old, new)` bytes as `GlobalAlloc::realloc` requires.
            unsafe { ffi::mi_realloc(ptr.cast(), new_size).cast() }
        } else {
            // SAFETY: as above, with `layout.align()` a power of two.
            unsafe { ffi::mi_realloc_aligned(ptr.cast(), new_size, layout.align()).cast() }
        }
    }
}
