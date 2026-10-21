// Memory regions of the language server (docs/LSP.md "Memory plan for a long-lived server"; not in Go, where the GC
// frees a program version and everything only it refers to).
//
// A program version owns:
// - its own region: what `CreateProgram` allocated (the cloned file list and resolution data of a reused program,
//   or everything a full build allocated on the calling thread);
// - the base region: the full build's region, shared by every version cloned from it (`UpdateProgram` hands the
//   shared processed-file data and the symlink cache to the next version);
// - the regions of its source files (each parsed file version has one, see parsecache.rs);
// - through its checker pool, the checkers' regions.
//
// `programOwner` is held by every `Project` value that refers to the program (projects are cloned per snapshot and
// live as long as a snapshot or a language service refers to them), so it is dropped exactly when nothing can reach
// the program any more. Dropping it frees the checkers and their regions, the `Program` itself (`ProgramBox`, and
// with the last version of a full build the processed-file data its versions share), then the regions.

use std::sync::Arc;

use tsrs_compiler::{Program, ProgramBox};
use tsrs_core::arena::Region;

use crate::checkerpool::checkerPool;

pub(crate) struct programOwner {
    // Fields drop in this order, after `drop` freed the checkers: the program, then the regions it points into.
    program: ProgramBox,
    checker_pool: Option<Arc<checkerPool>>,
    region: Region,
    pub(crate) base: Region,
    files: Vec<Region>,
}

impl programOwner {
    // `full_build`: the program does not share data with an older version; `base` is then its own region, which
    // also becomes the owner of the shared processed-file data (`Program::get_symlink_cache` routes there).
    pub(crate) fn new(
        program: ProgramBox,
        checker_pool: Option<Arc<checkerPool>>,
        region: Region,
        base: Region,
        full_build: bool,
    ) -> programOwner {
        if full_build {
            // The data shared by every version cloned from this build (its processed files) goes with the last
            // version's `ProgramBox`; each version's owner holds `base`, which it points into, so `base` outlives it.
            base.adopt_owner(processed_files_addr(program.get()));
        }
        let files = Region::containing_all(program.source_files().iter().map(|f| f.addr()))
            .into_iter()
            .flatten()
            .collect();
        log_region(|| {
            format!(
                "program {:p}: created (region {} KiB, base {} KiB)",
                program.get(),
                region.allocated_bytes() >> 10,
                base.allocated_bytes() >> 10
            )
        });
        programOwner { program, checker_pool, region, base, files }
    }
}

// The address `Program::get_symlink_cache` routes by (`Program` derefs to its shared `processedFiles`).
fn processed_files_addr(program: &'static Program) -> usize {
    std::ptr::from_ref(&**program).cast::<()>() as usize
}

impl Drop for programOwner {
    fn drop(&mut self) {
        log_region(|| {
            format!(
                "program {:p}: freed ({} files, region {} KiB)",
                self.program.get(),
                self.files.len(),
                self.region.allocated_bytes() >> 10
            )
        });
        if let Some(pool) = &self.checker_pool {
            pool.free_checkers();
        }
        // Then the fields: the last project value referring to the program is being dropped, so no snapshot, language
        // service or held checker can reach it (see the file comment), and `program` is freed before the regions.
    }
}

// `TSRS_REGION_LOG=1`: one stderr line per program created / freed and per disposed checker freed / parked.
pub(crate) fn log_region(msg: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("TSRS_REGION_LOG").is_some_and(|v| v == "1")) {
        eprintln!("regions: {}", msg());
    }
}
