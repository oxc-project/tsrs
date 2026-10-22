# unsafe-audit: every unsafe site and lifetime escape, classified

The inventory the soundness tracker (issue #646) asks for: each `unsafe` block, `unsafe fn`, `unsafe impl`,
`unsafe extern`, `Box::leak`, `Box::from_raw`, `mem::forget`, `transmute` and `assume_static` in
`{crates,tasks}/*/src` (test files excluded; inline `#[cfg(test)]` modules kept and classed `test`), with a class,
a verdict and a note. The table is `tools/lint/unsafe_sites.tsv`; `tools/lint/source.mjs` keeps it in sync (check 4
in its docstring; docs/RUST.md "Source checks"). Taken at commit 357d1d3 on 2026-10-21, before #638 and #645 land
(#645 adds a `reason` to every `Box::leak`, which becomes those rows' safety text; one `--update` after it merges).

## Totals

498 sites in 666 source files (572k lines). By class and verdict:

| class | what it covers | keep | safe-api-possible | defect-risk | total |
| --- | --- | ---: | ---: | ---: | ---: |
| logic | unsafe inside compiler logic: tagged words, `repr(C)` payloads, arena recycling, link stores, checker handles | 137 | 14 | 0 | 151 |
| core-allocator | `tsrs_core` ptr, arena, reserve, memsplit, sharedgraph, frozen | 127 | 0 | 16 | 143 |
| ownership | manual leak and free of programs, mappers, hosts, checkers; lifetime extension | 17 | 70 | 11 | 98 |
| boundary | OS, FFI, allocator, signal, wasm | 86 | 0 | 0 | 86 |
| test | inline test modules | 20 | 0 | 0 | 20 |
| | | 387 | 84 | 27 | 498 |

Verdicts: `keep` is a justified site (a boundary, or an unsafe fn / block whose contract is stated and local);
`safe-api-possible` is a site a safe wrapper or an owner type could enforce; `defect-risk` is a site where *safe*
code can break the invariant and nothing in the type system or a release build stops it. `defect-risk` does not mean
a defect exists: every one of the 27 is sound today by convention and by the checks in docs/DEBUGGING.md.

By crate (sites): tsrs_core 173, tsrs_checker 77, tsrs_fswatch 51, tsrs_ast 50, tsrs_compiler 48, tsrs_execute 26,
tsrs_cli 16, tsrs_ls 12, tsrs_project 10, tsrs_api 7, the rest 28. By kind: 345 blocks, 46 `Box::leak`, 39 unsafe
fns, 34 `unsafe impl` (the Send/Sync inventory), 13 `Box::from_raw`, 11 `unsafe extern`, 5 lifetime extensions,
4 transmutes, 1 `mem::forget`.

## The defect-risk sites

These are the places where the `'static` convention of docs/PORTING.md ("Memory model") is minted or shared, so they
are where a lifetime fix starts. Each is one safe API, not many unsafe blocks.

| site | why safe code can misuse it | #646 work package |
| --- | --- | --- |
| `P::get` (`tsrs_core/src/ptr.rs`, both the pointer and the compressed form) | a safe fn returns `&'static T` for a handle whose region or recycled block can be freed; every other `'static` view of arena data derives from it | the root; stays until regions or handles carry an owner (after milestone 1) |
| `new_program`, `Program::reuse_program` (`tsrs_compiler/src/program.rs`) | a safe fn returns `&'static Program` that `free_program` later frees; 152 signatures carry that view | program ownership (milestone 1): an owner type with `Drop` |
| `assume_static` (`tsrs_project/src/extendedconfigcache.rs`) and its three call sites in `configfileregistrybuilder.rs` | removed: `tsrs_module::DefaultResolver<'h>` borrows its host and config parsing takes `&dyn ParseConfigHost`, so the project system passes its builder directly; the same change removed the per-call host leaks in `tsrs_compiler/src/host.rs`, `tsrs_api/src/session.rs` and the leak / `from_raw` pair in `tsrs_api/src/module_resolution.rs` | done |
| `assume_static` (`tsrs_ls/src/autoimport/registry.rs`) and `clone_registry_in_scratch` | a lifetime transmute of the clone host; sound while every derived value dies in the update's scratch region | blocked on program/checker ownership: the alias resolver built from the host is handed to `new_checker(&'static dyn Program)`, and `wrapvfs::wrap` / `get_package_realpath_funcs` take `&'static dyn FS` |
| `FrozenCell::borrow` / `borrow_mut` and its `Send` / `Sync` (`tsrs_core/src/frozen.rs`) | a `RefCell` API without a borrow flag in release builds; the owner rule is checked only with `checked-cells` | thread-sharing invariants (after milestone 1); Miri cannot see this, the checked build can |
| `Send` / `Sync` for `P<T>`, `SP<T>`, `ThinSlice<T>`, `StaticSlicePtr<T>`, `SlicePair<A, B>` (`tsrs_core/src/ptr.rs`) | by decree: safe code can hand a checker-owned object to another thread | same |

## The safe-api-possible sites

- **Program and mapper ownership (35 in `tsrs_compiler`, 19 in `tsrs_execute`, 2 in `tsrs_project`, 3 in
  `tsrs_cli`, 1 in `tsrs_api`).** `free_program`, `free_unshared_program`, `SharedProgramData::free`,
  `free_project_reference_file_mapper`, `free_api_orchestrator`, `Program::drop_replaced` / `free_replaced` and every
  caller (the watcher, the build task, the orchestrator, `programOwner`, `transpile_in_current_region`, the CLI API
  handle), plus the `Box::leak`s they reclaim (the processed files, the file loader's mapper and resolution hosts,
  the dts-faking host, the checker slice). One owner type with `Drop` replaces all of them; the program ownership
  package of #646 is exactly this set.
- **Checker handles (`CheckerHandle::from_raw` and its derefs, `checkerPool::create_release`,
  `get_persistent_checker`, `View::checker`, the API's `node_index_table` / `encode_source_file`).** The lifetime is
  a pool lease or a held checker, enforced at run time (docs/LSP.md "Memory management"), not by the type.
- **Allocations #638 already owns** (`format/rule.rs`, `rulecontext.rs`, `lsproto::intern`, the background
  worker pool and timer, `includeProcessor::get_reference_location`): the rows change to `keep` or disappear when it
  merges.
- **`new_ad_hoc_message`** leaks one `Message` per call. Bounded in `tsc`; unbounded if a long-lived server calls it
  per request. Worth a count in the repeated-lifecycle leak run.
- **Enum-from-integer transmutes** (`Kind`, `NodeDataTag`, the codec's `kind_from_u32`): a checked table costs a
  bounds check per decode, unmeasured.

## Kept as they are

- **Boundary (86):** fswatch's kqueue, inotify and FSEvents FFI (51), libc calls with plain-data arguments
  (`sysconf`, `clock_gettime`, `localtime_r`, `proc_pidinfo`, `kill`, `setrlimit`, `malloc_trim`), mimalloc, the
  frozen seed's SIGSEGV handler, wasm's staged-buffer calls, `from_utf8_unchecked` after validation.
- **Core allocator (127):** arena chunks, free lists, slabs and regions, handle packing, `PackedStr` / `ThinSlice`
  views, the reservation. The invariants are stated per site; Miri runs the arena tests only
  (`.github/workflows/miri.yml`); the ptr, region and frozen tests are the next Miri target (#379).
- **Logic (137):** tagged-word decodes whose tag only this type's constructors write (`Node::payload`, `Symbol`,
  `Type`, `TypeMapper`, `ValueSymbolLinks`, `FlowNode`), `repr(C)` payload casts, link-store index arithmetic
  measured in notes/lint-paydown-compiler.md, the checker's arena recycling of blocks the same call made (guarded
  by the reference-parameter lint and the `panic = "abort"` conformance gate, notes/fix-arena-recycle-uaf.md),
  `copy_cells_bitwise` from the frozen seed, the printer's scoped writer borrow.
- **Ownership, process lifetime (17):** the CLI's hosts, `tsc`'s parse config host, the frozen seed checker and its
  region (`mem::forget`), the thread arenas, the test runner's, bench's and wasm one-shot's hosts, `ExtraSlot`'s
  `Box::into_raw` / `from_raw` pair.

## How the ratchet works

`tools/lint/source.mjs` lists the sites keyed by file, enclosing item, kind and the first line of the site's `SAFETY`
comment (or `# Safety` doc section, or `#[expect(..., reason = "...")]`), never by line number, so moving code does
not churn the table. A site that appears or disappears fails the `lint` job until `--update` records it; a new row is
written with a class derived from its crate and the verdict `unknown`, and the check names it until someone sets a
class and verdict. `--update` keeps the class, verdict and note of every row whose key still exists. Classes:
`boundary`, `core-allocator`, `ownership`, `logic`, `test`. Verdicts: `keep`, `safe-api-possible`, `defect-risk`,
`unknown`. The scan adds about 0.4 s to the lint (1.2 s before, 1.6 s with it; 83 of 666 files contain a site).
