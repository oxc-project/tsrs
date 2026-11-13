//! An allocator for the checker's largest hash tables (notes/perf-cache-line-tables.md).
//!
//! An allocation of 2 MiB or more, on Linux with compressed handles, is a chunk of the arena reservation whose whole
//! 2 MiB blocks are advised for transparent huge pages (`reserve::alloc_table_chunk`), given back when freed
//! (`reserve::release_chunk`). Everything else goes to the global allocator, whose heap stays on 4 KiB pages
//! (notes/mem-no-thp.md). Which way an allocation went follows from its size, which `deallocate` gets back.

use std::alloc::Layout;
use std::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator, Global};

/// The smallest allocation that gets huge pages.
pub const HUGE_BYTES: usize = 2 << 20;

#[derive(Clone, Copy, Default)]
pub struct HugeAlloc;

// Without compressed handles there is no arena reservation (Windows, wasm, `plain-ptrs`): everything is `Global`'s.
#[cfg(compressed_ptrs)]
impl HugeAlloc {
    #[inline]
    fn is_huge(layout: Layout) -> bool {
        cfg!(target_os = "linux") && layout.size() >= HUGE_BYTES && layout.align() <= HUGE_BYTES
    }
}

// SAFETY: a huge allocation is a chunk of its own, aligned to `HUGE_CHUNK` (at least `layout.align()`, checked in
// `is_huge`), at least `layout.size()` bytes and live until `deallocate` releases it; the rest is `Global`'s.
unsafe impl Allocator for HugeAlloc {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        #[cfg(compressed_ptrs)]
        if Self::is_huge(layout) {
            let p = crate::reserve::alloc_table_chunk(layout.size());
            return Ok(NonNull::slice_from_raw_parts(
                NonNull::new(p).ok_or(AllocError)?,
                layout.size(),
            ));
        }
        Global.allocate(layout)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        #[cfg(compressed_ptrs)]
        if Self::is_huge(layout) {
            // SAFETY: `allocate` took this chunk for the same size (the caller's contract).
            unsafe { crate::reserve::release_chunk(ptr.as_ptr(), layout.size()) };
            return;
        }
        // SAFETY: `Global` allocated it (the caller's contract and `is_huge`).
        unsafe { Global.deallocate(ptr, layout) };
    }
}
