# mem-program-owner: `ProgramBox`, the owner of a compiler program

Stage one of [#646](https://github.com/oxc-project/tsrs/issues/646) (sound lifetimes). Before this, `new_program`
leaked the `Program` and returned `&'static Program`; five call sites freed one with `unsafe { free_program(p) }` or
`free_unshared_program(p)`, each carrying its own "nothing refers to it any more" argument, and the processed-file
data a full build's versions share (`processedFiles`, the project reference file mapper and its resolution hosts) was
freed by a third `unsafe fn`, `SharedProgramData::free`, which its callers had to order after the last program by
hand (the watch's `free` closure, the build task's `watchFullBuild`, the language server's `base.on_free` hook).

## The owner

`tsrs_compiler::ProgramBox` is the only thing `new_program`, `update_program` and `reuse_program` hand out. It is a
newtype over the leaked `&'static Program`:

| | |
| --- | --- |
| `get(&self) -> &'static Program` (also `Deref`) | the view the rest of the compiler uses, unchanged |
| `leak(self) -> &'static Program` | gives the program to the process: the CLI's one-shot programs and the test harness, which never freed one and whose teardown at exit would only cost time |
| `Drop` | what `free_program` did: the `Program` (its checker pool's `poolState::drop` frees the checkers' slice), then its resolution host |

The shared data is an `Arc<sharedProgramData>` field of `Program`: `new_program` makes it, `reuse_program` clones
it into the derived version, and the last `ProgramBox` of a full build to drop frees it. `free_unshared_program` is
gone: a program that shared nothing has the only `Arc`, so dropping its owner frees everything, and a program that
did share frees the data exactly when the last sharer goes. Callers no longer order that free, which is what the
`free` closures and the `on_free` hook were doing by hand. The `Arc` is touched once per program creation and once
per free; the view is still a plain reference, so the hot path is unchanged.

## Holders and drop order

| holder | before | now |
| --- | --- | --- |
| CLI (`tsrs_execute` execute.rs) | leaked | `new_program(..).leak()` |
| watch (`Watcher::install_program`) | `free_program(old)` then `shared.free()` after the snapshot diagnostics, before the dead file regions | `compiler_program: Option<ProgramBox>`; the `free` closure drops the old owner first (the shared data goes with the last one), then the snapshot diagnostics, the dead file regions, the full build's regions |
| build task, watch mode (`replace_watch_program`) | `watch_program: Option<&'static Program>`, `watchFullBuild::shared` | `watch_program: Option<ProgramBox>`; the freer drops the previous owner, then the old snapshot diagnostics, then the old full build's regions |
| build task, batch and API (`free_built_program`, the orchestrator's report step) | `free_unshared_program(program.get_program())` | `taskResult::compiler_program: Option<ProgramBox>`, taken with the incremental program; dropped on the free thread (batch) or inline (API); a CLI build that keeps its programs leaks it as before |
| language server (`tsrs_project` memregions.rs `programOwner`) | `free_program` in `drop`, `shared.free()` in the base region's `on_free` hook | `program: ProgramBox` is the first field: `drop` frees the checkers, then the fields drop in order: program (and with the last version of a full build, the shared data), own region, base region, file regions |
| `transpileModule` (`tsrs_api` transpile.rs) | `free_unshared_program` | `drop(owned_program)` |

One order changed: the language server used to free the shared data when the base region was freed, after the
version's own region; it now goes with the last `Program`, before the regions. Nothing in a region's drop list reads
the processed-file data (the drop lists hold heap containers of handles), and the processed-file data's own
destructor only frees heap (`processedFiles`, the mapper's maps, the hosts' `Arc`s); the arena recycling checks and
the language-server lifetime regressions pass in normal and `TSRS_ARENA_POISON=1` mode.

## What stays unsound, and stage two

The view is still `&'static Program`: safe code can copy it out of a `ProgramBox` and read it after the owner is
dropped, and `tsrs_incremental::Program` keeps one (`compiler_program()`), as do checkers, snapshots and language
service values. The holders above keep the owner alive for them by construction, which is the same convention as
before, now with one owner per program instead of a free call per site.

Stage two is to make the view a borrow: `ProgramBox` derefs to a `Program` whose methods take `&self`, and
`&'static Program` leaves the signatures of what reads a program. At this revision, non-test code names
`&'static Program` (or `&'static tsrs_compiler::Program` / `&'static CompilerProgram`) in 187 places, 152 as the
bare spelling; the biggest groups are the language service (string_completions.rs 14, callhierarchy.rs 12,
findallreferences.rs 10), `tsrs_incremental` (program.rs 9, snapshot.rs 7, programtosnapshot.rs 6) and the checker
pool (7). The `&'static self` methods on `Program` (`update_program`, `reuse_program`, the checker pool's creation)
and the pool's `&'static Program` back-reference are the places where a lifetime parameter would have to appear,
which is the migration the memory model in docs/PORTING.md currently rules out; the shape of the handle after that
(a lifetime-carrying view, or an `Arc`-like owner that the checker pool and incremental program clone) is the
design question to settle before touching those sites.
