// tsrs-only: regions for what building a program allocates, freed when nothing uses the program's data
// any more (tsrs_execute watcher.rs: with the programs updated from it). The file loader allocates on the calling
// thread and on the worker pool (`fileloader::process_all_program_files`, `filesParser`), which other programs share
// under `tsc -b --watch`, so each thread gets a region of its own and every loader job enters it.
//
// Unregistered regions prevent owner-based routing from locking another thread's loader region while its job
// waits on the worker pool.

use std::sync::Mutex;
use std::thread::ThreadId;

use rustc_hash::FxHashMap;
use tsrs_core::arena::{Region, RegionScope};

#[derive(Default)]
pub struct ProgramRegions(Mutex<FxHashMap<ThreadId, Region>>);

impl ProgramRegions {
    /// Makes this thread's region of the set the allocation target until the scope is dropped.
    pub(crate) fn enter(&self) -> RegionScope {
        let region = self
            .0
            .lock()
            .unwrap()
            .entry(std::thread::current().id())
            .or_insert_with(|| Region::new_scratch(1 << 20))
            .clone();
        region.enter()
    }
}

/// `ProgramRegions::enter` if the program has regions.
pub(crate) fn enter(regions: Option<&ProgramRegions>) -> Option<RegionScope> {
    regions.map(ProgramRegions::enter)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tsrs_core::{CompilerOptions, P, Tristate, tspath};
    use tsrs_tsoptions::new_parsed_command_line;
    use tsrs_vfs::vfstest;

    use crate::{ProgramOptions, new_compiler_host, new_program};

    use super::ProgramRegions;

    #[test]
    fn deferred_binding_uses_program_regions() {
        for single_threaded in [Tristate::True, Tristate::False] {
            let regions = Arc::new(ProgramRegions::default());
            let config = new_parsed_command_line(
                P::new(CompilerOptions {
                    no_lib: Tristate::True,
                    types: Some(Vec::new()),
                    ..Default::default()
                }),
                vec!["/index.ts".into()],
                Vec::new(),
                tspath::ComparePathsOptions::default(),
            );
            let host = new_compiler_host(
                "/",
                Arc::new(vfstest::from_map([("/index.ts", "export const value = 1;")], true)),
                "",
                None,
                None,
                None,
            );
            let mut options = ProgramOptions::new(P::new(config), host);
            options.single_threaded = single_threaded;
            options.regions = Some(Arc::clone(&regions));
            let program = new_program(options);
            let file = program.get_source_file("/index.ts").unwrap();
            assert!(!file.is_bound());
            program.bind_source_files();
            let locals = file.locals().unwrap().addr();
            assert!(regions.0.lock().unwrap().values().any(|region| {
                region.chunks().iter().any(|&(start, size)| (start..start + size).contains(&locals))
            }));
            // SAFETY: this program has no shared versions or live checker handles; regions remain owned here.
            unsafe { crate::free_unshared_program(program) };
        }
    }
}
