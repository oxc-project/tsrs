// tsrs-only (watch mode, `tsc -b` freeing each built program, and tsrs_dts's private programs): regions for what
// building a program allocates, freed when nothing uses the program's data any more (tsrs_execute watcher.rs: with the
// programs updated from it; buildtask.rs `free_built_program`; tsrs_dts project.rs `ProgramOwner`). The file loader
// allocates on the calling thread and on the worker pool (`fileloader::process_all_program_files`, `filesParser`),
// which other programs share under `tsc -b`, so each thread gets a region of its own and every loader job enters it.
//
// The loader regions are not registered (`Region::new_scratch`): lazily filled data of an object in one goes to the
// thread arena (`arena::enter_owner`), so another thread never waits for a region a loader job holds while that job
// waits on the pool.
//
// That would leave the files' lazily filled data (JSDoc, line maps, name tables) in the never-freed thread arena, so
// the files are parsed into registered regions instead, one per parsing thread, entered only while one file is parsed
// (`enter_file`), where the file is bound too (fileregions.rs `bind`). A parse never waits on the pool, so another
// thread filling a file's lazy data waits at most for that parse. Watch hosts parse every file into a region of its own inside it (tsrs_execute watchmem.rs `FileRegions`),
// which then stays empty.

use std::sync::Mutex;
use std::thread::ThreadId;

use rustc_hash::FxHashMap;
use tsrs_core::arena::{Region, RegionScope};

#[derive(Default)]
pub struct ProgramRegions {
    loader: Mutex<FxHashMap<ThreadId, Region>>,
    /// The regions the program's files are parsed into.
    files: Mutex<FxHashMap<ThreadId, Region>>,
}

impl ProgramRegions {
    /// Makes this thread's region of the set the allocation target until the scope is dropped: for the file loader,
    /// and for what is made for the program before it, such as its configuration.
    pub fn enter(&self) -> RegionScope {
        thread_region(&self.loader, || Region::new_scratch(1 << 20)).enter()
    }

    /// Makes this thread's file region the allocation target until the scope is dropped.
    ///
    /// The first chunk takes a slab of its own that is not the cached size, so freeing it gives its pages back. In
    /// a 1 MiB chunk, freed file regions went to the slab cache dirty and a build of 200 small projects peaked
    /// 6.5 MiB higher; in 64 KiB chunks carved from the threads' shared slabs, a live neighbour kept each freed
    /// region's slab, 25 MiB higher.
    fn enter_file(&self) -> RegionScope {
        thread_region(&self.files, || Region::new(512 << 10)).enter()
    }
}

fn thread_region(
    regions: &Mutex<FxHashMap<ThreadId, Region>>,
    new: impl FnOnce() -> Region,
) -> Region {
    regions.lock().unwrap().entry(std::thread::current().id()).or_insert_with(new).clone()
}

/// `ProgramRegions::enter` if the program has regions.
pub fn enter(regions: Option<&ProgramRegions>) -> Option<RegionScope> {
    regions.map(ProgramRegions::enter)
}

/// `ProgramRegions::enter_file` if the program has regions: around parsing one file.
pub fn enter_file(regions: Option<&ProgramRegions>) -> Option<RegionScope> {
    regions.map(ProgramRegions::enter_file)
}
