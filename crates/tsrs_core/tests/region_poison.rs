//! Early owner drop under `TSRS_ARENA_POISON=1` (docs/DEBUGGING.md "Arena recycling checks"): a freed region's memory
//! stays mapped and is filled with `POISON`, so a `P` or slice that safe code kept past the region's owner reads as
//! poison instead of stale or reused data. The mode is read once per process, so this binary holds the one test that
//! needs it (the arena's unit tests run with recycling on).
//!
//! Runs natively in both pointer modes. Not under Miri: the stale read below is what Miri is for, and it reports it
//! (Stacked Borrows: the poison fill through an exposed-provenance pointer retags the block, and the handle's tag is
//! gone), as `arena::tests::miri_rejects_reading_a_handle_of_a_freed_region` shows for the unpoisoned free
//! (notes/miri-coverage.md).

use tsrs_core::P;
use tsrs_core::arena::{POISON, Region};

#[test]
#[cfg_attr(miri, ignore = "a deliberate stale read: Miri reports it (see the module comment)")]
fn a_handle_kept_past_its_regions_owner_reads_poison() {
    // SAFETY: the first thing this process does, before any other thread exists.
    unsafe { std::env::set_var("TSRS_ARENA_POISON", "1") };
    let region = Region::new(4096);
    let (p, s) = {
        let _s = region.enter();
        (P::new(0x1122_3344_5566_7788u64), tsrs_core::alloc_slice(&[1u8, 2, 3, 4, 5, 6, 7, 8]))
    };
    let addr = p.addr();
    drop(region);
    assert!(Region::containing(addr).is_none());
    // Safe code can still read through the handle: the memory model has no owner in the types (tsrs issue #646).
    // Poison mode makes that visible instead of returning stale or reused data.
    assert_eq!(*p, u64::from_ne_bytes([POISON; 8]));
    assert_eq!(s, &[POISON; 8]);
}
