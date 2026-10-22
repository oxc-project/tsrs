# Front-end allocations, round two: one reparse cloner per file

Measured 2026-10-22 at `c6695157` (main after #681) on an 18-core Apple Silicon machine, following AGENTS.md
"Performance and memory changes" (profile first, attribute, smallest change, before/after numbers).

## Question

#681 (its pull request body holds that profile) cut the path, resolver-cache and JSDoc-indent
allocations of the front end and left two sites alone because they were outside the crates it touched: the
reparser's deep-clone visitor and the declaration transformer's boxed closure per visited node. Are they worth
taking, and what is the next front-end hotspot after them?

## Profile

The same instrument as #681, rebuilt for this round and not committed: a `GlobalAlloc` wrapper around mimalloc in
the CLI that counts every `alloc`, `alloc_zeroed` and `realloc` call exactly and samples the call stack of one in
four (`backtrace::trace_unsynchronized`, 14 frames, inlining chains resolved at exit). Single-threaded
`--noEmit --incremental false --pretty false --singleThreaded`, `RAYON_NUM_THREADS=1`, with and without
`--noCheck`. The share is of all sampled allocations in the run, attributed to the first tsrs frame.

| site | webpack full check | webpack `--noCheck` | cal-diy `--noCheck` | typeorm full check |
| --- | ---: | ---: | ---: | ---: |
| `get_deep_clone_visitor` + `with_deep_clone_visitor` (per JSDoc clone) | 5.5% | 22.8% | 0 | 0 |
| `EntryVec::reserve_exact` + `push` (`SymbolMap::insert`, binder and checker tables) | 11.5% | 16.1% | 11.3% | 7.6% |
| `is_declaration_file_name` (owned extension string) | 0.2% | 0.8% | 2.1% | 0.7% |
| VFS probes: `root_and_path`, `split_path`, `file_exists`, `directory_exists`, `DirFS::file_type`, cached `file_exists` | 1.5% | 6.1% | 11.0% | 3.3% |
| `InfoCache::get` + `get_package_json_info` (package.json cache keys) | 0.4% | 1.9% | 7.7% | 3.0% |
| `parse_jsdoc_comment_worker` + `get_jsdoc_comment_ranges` (tag and range vectors) | 2.7% | 6.8% | 1.1% | 0.5% |
| `tsrs_declarations::*` | 0 | 0 | 0 | 0 |

Totals (exact counts, `alloc` + `realloc` calls): webpack 3,590,181 full and 857,726 `--noCheck`; typeorm
2,854,693 full and 5,009,227 `--noCheck`; cal-diy 10,669,956 full and 3,149,589 `--noCheck`.

Two things the table settles:

- The reparser builds a fresh visitor for every JSDoc clone (`NodeFactory::deep_clone_reparse`): three `Rc` hook
  closures and the `Rc<NodeVisitorData>`, four heap allocations per clone, 192,096 per webpack run. #681 put it at
  8.7% of webpack's front end; attributed to the first tsrs frame (the sampler then attributed to the caller's
  caller) it is 22.8% of the front end and 5.5% of the full check.
- The declaration transformer's boxed closure (`setup_diagnostic_context`) runs only under `--noCheck`: that mode
  still computes declaration diagnostics, which is why typeorm's `--noCheck` run allocates 1.75x what its full check
  does (55% of it in `visit_cjs_export_assignments`, `visit_nested_expression` and `setup_diagnostic_context`).
  The catalog measures `--noEmit` without `--noCheck`, and no `tsrs_declarations` frame appears in 713,672 samples
  of typeorm's full check. Nothing to take there for the catalog; it is on the do-not-redo list with that reason.

## Change

- `NodeFactory::reparse_cloner()` builds the deep-clone visitor once and `ReparseCloner::clone_node` /
  `clone_modifiers` reuse it. `Parser` keeps one per file, built on the file's first JSDoc clone
  (`Parser::reparse_cloner`), so the four allocations are paid per file with reparsed JSDoc instead of per clone.
  `deep_clone_reparse` and `deep_clone_reparse_modifiers` stay as one-shot conveniences (tests, future callers);
  `deep_clone_node` (synthetic locations, language-service code actions) is untouched. The visitor is a handle
  (`Rc<NodeVisitorData>`) and never modified after construction, so reuse changes no behaviour; the visitor holds
  a factory handle, not the parser, so there is no reference cycle.
- `get_declaration_file_extension` returns a slice of its argument instead of a `String` of a static suffix:
  every module-resolution probe asks `is_declaration_file_name`. The extension has no separator in it, so it is
  the tail of the path before its (at most one) trailing separator, which is what the base name was computed
  from. Two callers that fall back to an owned extension (`rename.rs`, `dynamic.rs`) keep a local `String`.

## Measurement

Clean fat-LTO `--release` builds with debug info, `/usr/bin/time -l` instructions retired (kernel included; the
macOS screening instrument of notes/perf-icount-round4.md), `--noEmit --incremental false --pretty false
--singleThreaded`, `RAYON_NUM_THREADS=1`, one warm-up then interleaved base/candidate pairs, medians of the paired
changes. Peak RSS with the default checker count, same recipe. The machine was shared with two other agents' builds
during the series, so the pairs spread about +-1% on instructions and +-2% on default-checker RSS; the deep-clone
change can only show on webpack (JS files whose JSDoc the reparser clones), the extension change is a few tens of
thousands of allocations on each project.

| project | pairs | instructions base | candidate | change | pairs down | single-threaded RSS | default-checker peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| webpack | 11 | 11.254 G | 11.219 G | **-0.36%** | 8 of 11 | 309.2 -> 309.1 MiB (-0.02%) | 517 -> 513 MiB (-0.55%) |
| typeorm | 9 | 8.441 G | 8.451 G | +0.09% | 4 of 9 | 195.6 -> 195.6 MiB (0.00%) | 267 -> 266 MiB (+0.19%) |
| cal-diy | 7 | 33.501 G | 33.502 G | +0.04% | 3 of 7 | 792.4 -> 792.4 MiB (-0.01%) | 1929 -> 1933 MiB (+1.02%) |

A second series once the machine was quieter (my own builds done): webpack 15 pairs, 11.189 G -> 11.173 G,
**-0.21%**, 11 of 15 pairs down, the pairs within -0.52% to +0.10%; cal-diy 9 pairs, +0.03%, 4 of 9 down, default-checker
RSS -0.75% (series one said +1.02%: noise around zero).

Diagnostics (exit code and output bytes) identical in every run of every project, single-threaded and default.
The default-checker RSS medians are inside their pairs' spread (cal-diy's seven pairs run from -0.9% to +2.8%);
the change frees nothing that is retained, so no RSS change is expected.

Heap allocations per run, exact counts from the tracing build (`alloc` + `realloc` calls, single-threaded):

| project | full check, base | candidate | change | `--noCheck`, base | candidate | change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| webpack | 3,590,181 | 3,390,350 | **-5.6%** | 857,726 | 658,650 | **-23.2%** |
| typeorm | 2,854,693 | 2,834,726 | -0.7% | 5,009,227 | 4,989,258 | -0.4% |
| cal-diy | 10,669,956 | 10,604,615 | -0.6% | 3,149,589 | 3,084,349 | -2.1% |

The reparse cloner alone (an intermediate build) is webpack 3,590,181 -> 3,398,085; the other two projects were
unchanged by it to the allocation, so the rest is the borrowed extension. The counts are deterministic: every
run of a build repeats its count exactly.

Exactness: the parser oracle's AST hashes (`crates/tsrs_parser/examples/ast_oracle`, `hash` mode, base vs
candidate builds) are identical on all 279,430 `.ts`/`.tsx`/`.js`/`.jsx`/`.mts`/`.cts`/`.mjs`/`.cjs` files of
the three projects' trees, dependencies included (webpack 55,127, typeorm 30,923, cal-diy 193,380); no crashes.
`tools/regressions.sh` passes its 47 cases with the candidate binary.

The Linux `bench/count.py` gate on the pull request is the landing number; these are the screening numbers.

## Tried and dropped

- The declaration transformer's boxed closure (`setup_diagnostic_context`, #681's second site): absent from the
  catalog mode, see "Profile". Not changed.
- Caching the deep-clone visitor on the factory (the comment in deepclone.rs asked for it): the visitor holds a
  factory handle, so a cache in the factory's shared data would be an `Rc` cycle, and the factory outlives the
  parser in the language service. One cloner per parser has the same effect without the cycle.

## Next, from the same profile (not done here)

- VFS probes (`Common::file_exists`, `directory_exists`, `stat`): `split_path` normalizes the path into a new
  `String` and copies the root and the rest, `root_for` boxes a `DirFS` with its own copy of the root, and
  `DirFS::file_type` joins the path again before the syscall's `CString`: about five avoidable allocations per
  probe, 11.0% of cal-diy's front-end allocations (about 347,000 per run). Needs the `RootForFn` shape changed
  for its three implementors (os, iofs, wasm host). notes/perf-vfs-lightweight-metadata.md rejected a
  different probe change (lighter `FileInfo`) under the old thresholds.
- `InfoCache::get` keys every lookup with a fresh `to_path` (an `Arc` plus the lowercase copy on case-insensitive
  hosts) and `get_package_json_info` joins `package.json` onto the directory first: 7.7% of cal-diy's front end.
  A borrowed key view like #681's resolution-cache key would remove the hit path's allocations.
- JSDoc parsing grows a `Vec<P<Node>>` of tags per comment and a `Vec<CommentRange>` per node with JSDoc
  (6.8% of webpack's front end); the parser already has `*_space` buffers for similar lists. For TS files the
  lazy `parse_jsdoc_for_node` builds a whole `Parser` (scanner, factory with two `Rc` counters) per node whose
  JSDoc the checker asks for: 0.6% of webpack's full check in `new_node_factory` alone.
- `SymbolMap::insert` growth is memory-tuned and on the do-not-redo list (notes/perf-heap-churn.md).
