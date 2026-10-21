// tsrs-only: regions for what building a program allocates, freed when nothing uses the program's data any more
// (tsrs_execute watcher.rs: with the programs updated from it; tsrs_project memregions.rs: with the program owner).
// The file loader allocates on the calling thread and on the worker pool (`fileloader::process_all_program_files`,
// `filesParser`), which other programs share under `tsc -b --watch` and in the language server, so each thread gets
// a region of its own and every loader job enters it. Without them a loader job's resolutions, include reasons and
// metadata stay in the worker's own arena for the life of the process (notes/lifecycle-leaks.md).
//
// The regions are not registered (`Region::new_scratch`): lazily filled data of an object in one goes to the thread
// arena (`arena::enter_owner`), so another thread never waits for a region a loader job holds while that job waits on
// the pool.

use std::sync::Mutex;
use std::thread::ThreadId;

use rustc_hash::FxHashMap;
use tsrs_core::arena::{Region, RegionScope};

pub struct ProgramRegions {
    regions: Mutex<FxHashMap<ThreadId, Region>>,
    /// The first chunk of each thread's region; a region grows by chunks as needed.
    first_chunk: usize,
}

impl Default for ProgramRegions {
    /// A full build under `tsc --watch` or `tsc -b`: a 1 MiB first chunk per thread.
    fn default() -> ProgramRegions {
        ProgramRegions::new(1 << 20)
    }
}

impl ProgramRegions {
    /// Regions whose first chunk is `first_chunk` bytes (the language server keeps several programs alive and
    /// most of its projects are small, so it starts smaller than a watch build).
    pub fn new(first_chunk: usize) -> ProgramRegions {
        ProgramRegions { regions: Mutex::new(FxHashMap::default()), first_chunk }
    }

    /// Makes this thread's region of the set the allocation target until the scope is dropped.
    pub(crate) fn enter(&self) -> RegionScope {
        let region = self
            .regions
            .lock()
            .unwrap()
            .entry(std::thread::current().id())
            .or_insert_with(|| Region::new_scratch(self.first_chunk))
            .clone();
        region.enter()
    }

    /// Bytes allocated in the regions so far (`TSRS_REGION_LOG=1`).
    pub fn allocated_bytes(&self) -> usize {
        self.regions.lock().unwrap().values().map(Region::allocated_bytes).sum()
    }
}

/// `ProgramRegions::enter` if the program has regions.
pub(crate) fn enter(regions: Option<&ProgramRegions>) -> Option<RegionScope> {
    regions.map(ProgramRegions::enter)
}
