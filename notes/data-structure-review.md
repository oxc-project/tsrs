# Data structure review (2026-10-26)

Base: `368aa219`. The review inventoried collection references across 868 Rust files in `crates/` and `tasks/`,
read the shared collection implementations and checker hot paths, and checked the earlier performance and memory
experiments. Counts below are textual occurrences, including imports, declarations, comments and tests.

## Collection choices

| Family | Inventory | Decision and reason |
| --- | --- | --- |
| Unordered maps and sets | 1,320 `FxHashMap` references in 216 files; 488 `FxHashSet` references in 113 files | Keep Fx hashing. These are already SwissTable-backed standard collections with a non-SipHash hasher. Replacing every import with hashbrown alone does not establish a gain. The resolver already uses hashbrown directly for its borrowed composite-key lookup. |
| Ordered maps and sets | 257 `OrderedMap` references in 61 files; 27 `OrderedSet` references in 12 files | Keep indexmap with `FxBuildHasher`: insertion order and order-preserving deletion are observable. The LSP watcher and emit harness also have direct default-hasher `IndexMap` uses; changing those needs a workload for those paths. |
| Specialized hash tables | 33 `HashTable` references in nine files, plus symbol tables, packed caches and link stores | Keep the existing hashbrown tables, packed slots, embedded keys and compact indexes. A generic map would give up existing layout savings. Symbol and JSON maps deliberately scan tiny tables and index larger ones. |
| Vectors and slices | 6,693 `Vec` references in 538 files | Retain ordinary growable buffers, pooled checker scratch and arena-backed stored slices. Trial `smallvec` only for measured short-lived buffers in `filter_type` and `find_applicable_index_info`. Inline storage in every long-lived object would increase object size. |
| Queues, stacks and timer heap | 26 `VecDeque` references in ten files; three `BinaryHeap` references in one file; `Stack` wraps `Vec` | Keep their current representations. FIFO worker queues use deque operations; the timer requires priority ordering. Front insertion/removal remains in a few emit transforms, including the late-marked declaration queue; the check-only catalog does not measure those paths. |
| Sorted trees | 89 `BTreeMap` references in 21 files; 18 `BTreeSet` references in nine files | Keep predecessor/range queries for arena spans and free ranges, and deterministic ordering in test/report/VFS maps. Hash-map substitution would change the required operations. |
| Concurrent collections | 126 `SyncMap` references in 27 files; 35 `SyncSet` references in nine files | Keep the measured 64-shard RwLock/hashbrown design. A DashMap migration would need contention and callback/reentrancy evidence; a new concurrent-map dependency alone does not remove that work. |
| Multimaps and copy-on-write collections | `MultiMap`, `CopyOnWriteMap`, `CopyOnWriteSet` | Keep per-key vectors and the lazy `Rc` backing map. Scope restoration and snapshot ownership are part of their contracts. |
| Identity, strings, flags and dense indexes | `P`, `PackedStr`, packed slices, link pages, bitflags and union-cache slots | Keep compressed identities, arena ownership, packed strings and fixed-width flags. Slot-map, interner, compact-string and dynamic-bitset crates are not drop-in replacements for these contracts. |

Prior work considered: `perf-round2-followups.md`, `mem-round4.md`, `mem-checker-heap.md`,
`perf-checker-cpu2.md`, `perf-checker-cpu3.md`, `perf-symbol-map-lookups.md`, `perf-lookup-insert.md`,
`perf-probes-2026-10-05.md`, and the techniques in `docs/RUST.md`. In particular, this does not retry global
identifier interning, sparse link-page redesigns, symbol-table lookup/insert fusion or generic scratch pooling.
The temporary-vector experiment uses a fresh size/allocation census, as required by the previously untried
inline-vector entry in `docs/RUST.md`.

## Temporary-vector census

An instrumented development binary counted final lengths and nonzero capacities of the original vectors,
single-threaded with the fast preset. Nonempty results of at most four elements can stay inline. The counts are
per vector construction, rather than counts of allocator calls including growth reallocations.

| Project | Filter allocations / nonempty results <= 4 | Origin-filter allocations / results <= 4 | Applicable-index allocations / results <= 4 |
| --- | ---: | ---: | ---: |
| Compiler-Unions | 7,777 / 3,865 | 1,155 / 655 | 6,518 / 6,518 |
| xstate-main | 7,910 / 7,805 | 536 / 533 | 19,538 / 19,538 |
| vscode | 287,723 / 271,844 | 13,492 / 12,593 | 484,529 / 484,529 |
| t3code-server | 104,604 / 95,192 | 7,058 / 5,410 | 102,053 / 102,053 |

The candidate uses `SmallVec<[P<T>; 4]>` for the two filter buffers, the applicable-index buffer and its temporary
value-type list. Capacity four fits two machine words with compressed handles. The stable `union` feature avoids
an extra discriminator word; larger lists retain heap growth. No arena object layout changes, new unsafe code,
predicate reordering or type-creation shortcuts are involved. The pinned Go `filterType` and
`findApplicableIndexInfo` were read before changing their Rust counterparts.

Crate documentation: [smallvec 1.16.3](https://docs.rs/smallvec/1.16.3/smallvec/),
[hashbrown 0.17.1](https://docs.rs/hashbrown/0.17.1/hashbrown/),
[indexmap](https://github.com/indexmap-rs/indexmap).

## Measurement and validation

Local evidence is in `bench/.work/data-structures-20261026/`: saved baseline/candidate binaries, `census.patch`,
`census.json`, `paired.mjs`, per-run Hyperfine JSON and compiler output. Both release binaries use full debug
information, fat LTO and no stripping:

```sh
cargo build --release -p tsrs_cli --config profile.release.debug=true --config profile.release.strip=false
node bench/.work/data-structures-20261026/paired.mjs all 5 candidate single,default catalog
```

The runner uses Hyperfine 2.0.0, a warmup per binary/cell, alternating baseline/candidate order, single-threaded
instructions and default-checker peak RSS. Both modes use `TSRS_FAST=1`; default mode uses the machine's worker
count. Catalog checkouts match their pinned commits. Config hashes and diagnostic hashes are recorded and checked.
No builds or correctness suites run concurrently with measurements.

Apple M3 Max, 14 cores, 36 GB RAM. Medians of five measurements per binary/cell; the initial five-project screen
used three measurements and independently agreed on the direction of single-threaded instructions. Every measured
project has byte-identical baseline/candidate diagnostics and the same exit status in both modes.

| Project | Single-thread instructions, G | Fast-default peak RSS, MiB | Fast-default wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 80.72322 → 80.63928 (-0.104%) | 2043.33 → 2032.80 (-0.515%) | 974.237 → 962.424 (-1.213%) |
| xstate-main | 6.08606 → 6.08396 (-0.034%) | 248.13 → 250.02 (+0.762%) | 127.109 → 129.110 (+1.574%) |
| webpack | 10.75390 → 10.74872 (-0.048%) | 482.36 → 486.00 (+0.755%) | 153.171 → 154.051 (+0.575%) |
| mui-docs | 37.73423 → 37.70834 (-0.069%) | 1504.52 → 1501.23 (-0.218%) | 1148.112 → 1148.338 (+0.020%) |
| Compiler | 1.68532 → 1.68432 (-0.059%) | 76.97 → 76.80 (-0.223%) | 62.141 → 63.423 (+2.064%) |
| Compiler-Unions | 2.85046 → 2.85010 (-0.013%) | 81.69 → 81.64 (-0.057%) | 95.072 → 95.799 (+0.764%) |
| cal-diy | 31.91385 → 31.90002 (-0.043%) | 1764.89 → 1761.95 (-0.166%) | 766.738 → 762.552 (-0.546%) |
| formbricks-web | 40.19142 → 40.17042 (-0.052%) | 1889.70 → 1881.00 (-0.461%) | 789.920 → 800.427 (+1.330%) |
| supabase-studio | 45.25184 → 45.22620 (-0.057%) | 1584.11 → 1581.47 (-0.167%) | 843.278 → 854.694 (+1.354%) |
| t3code-server | 39.69052 → 39.66072 (-0.075%) | 1992.55 → 1992.45 (-0.005%) | 1317.506 → 1347.251 (+2.258%) |
| mikro-orm | 47.03002 → 47.01033 (-0.042%) | 1847.63 → 1856.34 (+0.472%) | 755.771 → 758.430 (+0.352%) |
| next-packages-next | 17.03062 → 17.02007 (-0.062%) | 618.64 → 620.81 (+0.351%) | 251.843 → 253.712 (+0.742%) |
| next-root | 16.67025 → 16.66342 (-0.041%) | 466.33 → 455.80 (-2.258%) | 388.489 → 386.384 (-0.542%) |
| storybook | 9.98795 → 9.97443 (-0.135%) | 472.80 → 471.17 (-0.344%) | 195.035 → 193.152 (-0.965%) |
| nuxt | 10.65690 → 10.65383 (-0.029%) | 491.31 → 488.16 (-0.642%) | 240.603 → 244.974 (+1.816%) |
| playwright | 7.52772 → 7.52304 (-0.062%) | 435.52 → 437.91 (+0.549%) | 134.049 → 136.739 (+2.006%) |
| drizzle-orm | Configuration rejected | — | — |
| typeorm | 7.74437 → 7.74177 (-0.034%) | 251.02 → 251.83 (+0.324%) | 180.195 → 179.659 (-0.298%) |
| date-fns | 3.25671 → 3.25626 (-0.014%) | 146.16 → 146.03 (-0.086%) | 127.250 → 128.481 (+0.967%) |

All 18 measured projects reduce median single-threaded instructions by 0.013–0.135% (median 0.050%). Nine
projects have fewer instructions in every one of the five matched pairs; several other effects are smaller than
individual-run variation. This meets the current policy accepting repeatable instruction reductions on most
catalog projects, without a minimum percentage threshold.

Single-threaded wall changes range from -1.24% to +1.15%, with a median near zero. Fast-default wall changes range
from -1.21% to +2.26% (median +0.75%); peak RSS ranges from -2.26% to +0.76% (12/18 lower, median -0.13%).
Fast mode uses adaptive checker assignment, so those parallel changes include scheduling variation. The screen
and catalog runs disagree on the direction of several parallel timings. There is no general wall-time speedup
or memory reduction claim. The retained benefit is the small, repeatable single-threaded instruction reduction.

Drizzle is reported but excluded: the unmodified catalog configuration returns TS5090/TS5102 for its removed
`baseUrl` option and non-relative paths before checking. Both binaries produce the same errors. Its configuration
was left unchanged. Raw measurements, including single-threaded wall/RSS and parallel instruction counts, are in
`catalog-summary.json`; the two constituent runs are `catalog/` and `catalog-rest/`.

## Literal CLI defaults

A separate three-pair run sets `TSRS_FAST=0` and leaves checker count at the CLI default. The same binaries,
projects, warmup, alternating order and diagnostic comparisons apply. No builds or suites overlap this run.

| Project | Default peak RSS, MiB | Default wall, ms |
| --- | ---: | ---: |
| vscode | 2405.95 → 2406.89 (+0.039%) | 2373.640 → 2379.443 (+0.244%) |
| xstate-main | 266.77 → 266.50 (-0.100%) | 192.945 → 194.963 (+1.046%) |
| webpack | 405.25 → 405.36 (+0.027%) | 301.702 → 301.167 (-0.177%) |
| mui-docs | 1323.14 → 1323.22 (+0.006%) | 1555.298 → 1562.230 (+0.446%) |
| Compiler | 77.48 → 77.86 (+0.484%) | 63.015 → 65.091 (+3.294%) |
| Compiler-Unions | 81.58 → 81.77 (+0.230%) | 95.049 → 93.217 (-1.928%) |
| cal-diy | 1345.05 → 1345.03 (-0.001%) | 1119.010 → 1124.194 (+0.463%) |
| formbricks-web | 1989.30 → 1985.19 (-0.207%) | 1379.980 → 1365.228 (-1.069%) |
| supabase-studio | 1408.13 → 1414.69 (+0.466%) | 1614.518 → 1620.467 (+0.368%) |
| t3code-server | 1865.84 → 1864.02 (-0.098%) | 2009.198 → 2003.403 (-0.288%) |
| mikro-orm | 1629.89 → 1622.67 (-0.443%) | 1557.244 → 1563.194 (+0.382%) |
| next-packages-next | 603.13 → 604.34 (+0.202%) | 659.086 → 662.823 (+0.567%) |
| next-root | 469.09 → 468.92 (-0.037%) | 518.431 → 514.295 (-0.798%) |
| storybook | 446.11 → 447.70 (+0.357%) | 295.633 → 295.675 (+0.014%) |
| nuxt | 427.50 → 428.61 (+0.260%) | 349.571 → 336.968 (-3.605%) |
| playwright | 368.09 → 367.36 (-0.200%) | 248.252 → 252.225 (+1.601%) |
| drizzle-orm | Configuration rejected | — |
| typeorm | 236.55 → 235.64 (-0.383%) | 254.254 → 257.244 (+1.176%) |
| date-fns | 145.44 → 146.55 (+0.763%) | 126.159 → 123.142 (-2.391%) |

Default peak RSS: -0.443% to +0.763%, median +0.016%.

Default wall time: -3.605% to +3.294%, median +0.306%.

These results also support no general wall-time or peak-memory improvement claim. Raw output, exit statuses,
instructions and samples are in `cli-default/summary.json`.

## Correctness checks

The existing `ts-ref` checkout has a different HEAD and working sources. It was preserved. A separate checkout at
`/tmp/tsrs-collections-reference` was verified at the Cargo pin
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with a clean `tsc` tree; its Go compiler was built and its testdata
selected through `TSRS_TESTDATA`.

Development builds use `cargo build --profile dev -p tsrs_cli -p tsrs_testrunner`. Baseline and candidate retain
exactly the same passing lists: 13,699 error baselines, 12,997 each of types and symbols, 13,625 JavaScript/
declaration outputs, 149 JavaScript maps and 156 source-map records. Neither has a baseline mismatch or crash.
`compiler/intersectionConstructorReductionCrash` times out in both development builds with a 180-second limit.
The baseline's two other initial 20-second timeouts pass when retried at the longer limit.

Fast-mode conformance covers the other 15,447 variants with the same passing counts, no mismatches, crashes or
timeouts. All 50 checked-in regressions pass. A focused fixture with six simultaneously applicable template index
signatures, readonly index assignment and a filtered ten-element union exercises heap spill; its diagnostics and
exit status are byte-identical to the pinned Go compiler. The fixture and outputs are in the local `spill/` directory.

Clippy with warnings denied, source lint, formatting, and `cargo check -p tsrs_wasm --target wasm32-wasip1` pass.
The API codec freshness check fails when pointed at the existing mismatched `ts-ref`; the unchanged generator and
generated artifacts pass in an isolated copy pointing at the correct pin. No generated files were changed.

The remaining workspace run used `cargo test --locked --no-fail-fast -- --skip generated_code_is_fresh`, outside
the sandbox because local socket operations are denied inside it. Twenty checks fail across `tsrs_ls`, `tsrs_lsp`
and `tsrs_tsoptions`: missing AST-navigation/formatting/stack-sanitizer fixtures and mismatched command-line/config
baselines under the original `ts-ref`. All twenty failures reproduce with the exact baseline source restored for
a targeted run; that sandboxed baseline run also has five socket/watcher failures. Candidate sources were restored
afterward. The full workspace suite is therefore not green in this checkout; no candidate-only failure was found.

## Follow-up: direct hashbrown checker caches

A second review screened replacing 20 private checker caches in `checker.rs` with
`hashbrown::HashMap<K, V, FxBuildHasher>`, retaining the existing keys, values, hashing, lookup and insertion
logic. The comparison isolates the map implementation: baseline `0bc31e12` already includes the four
`SmallVec` buffers above. No new dependency or unsafe code was involved. This trial was not retained.

Both binaries used the release/debug-symbol configuration above on the same machine. All 19 catalog checkouts
were reverified at their pins. Each cell had a warmup and three alternating baseline/candidate pairs, with
`TSRS_FAST=1 --singleThreaded` for instructions and literal CLI defaults (`TSRS_FAST=0`) for default peak RSS
and wall time. No builds or correctness suites overlapped the measurements. Drizzle was again excluded for the
same configuration errors. Baseline and candidate diagnostics and exit statuses matched in every cell.

| Project | Single-thread instructions, G | Default peak RSS, MiB | Default wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 80.68965 → 80.67026 (-0.024%) | 2403.80 → 2411.23 (+0.309%) | 2459.098 → 2456.511 (-0.105%) |
| xstate-main | 6.08348 → 6.08422 (+0.012%) | 267.91 → 267.34 (-0.210%) | 193.558 → 192.939 (-0.319%) |
| webpack | 10.75128 → 10.74887 (-0.022%) | 404.98 → 405.41 (+0.104%) | 301.920 → 305.461 (+1.173%) |
| mui-docs | 37.72510 → 37.72715 (+0.005%) | 1325.45 → 1324.91 (-0.041%) | 1578.855 → 1590.724 (+0.752%) |
| Compiler | 1.68424 → 1.68425 (+0.001%) | 78.13 → 77.83 (-0.380%) | 64.486 → 64.636 (+0.233%) |
| Compiler-Unions | 2.85007 → 2.85016 (+0.003%) | 81.69 → 81.64 (-0.057%) | 93.130 → 96.454 (+3.569%) |
| cal-diy | 31.91295 → 31.92624 (+0.042%) | 1353.13 → 1342.20 (-0.807%) | 1133.137 → 1138.676 (+0.489%) |
| formbricks-web | 40.88797 → 41.21125 (+0.791%) | 1986.41 → 1987.52 (+0.056%) | 1361.055 → 1354.238 (-0.501%) |
| supabase-studio | 45.25005 → 45.25699 (+0.015%) | 1407.92 → 1407.81 (-0.008%) | 1648.037 → 1647.322 (-0.043%) |
| t3code-server | 39.67342 → 39.67039 (-0.008%) | 1864.53 → 1864.05 (-0.026%) | 2046.426 → 2057.305 (+0.532%) |
| mikro-orm | 47.00451 → 47.00800 (+0.007%) | 1628.67 → 1631.31 (+0.162%) | 1600.814 → 1615.993 (+0.948%) |
| next-packages-next | 17.02396 → 17.02281 (-0.007%) | 603.67 → 602.31 (-0.225%) | 673.173 → 672.633 (-0.080%) |
| next-root | 16.65489 → 16.65562 (+0.004%) | 469.52 → 469.36 (-0.033%) | 524.019 → 522.789 (-0.235%) |
| storybook | 9.98712 → 9.98724 (+0.001%) | 446.16 → 446.61 (+0.102%) | 299.203 → 298.742 (-0.154%) |
| nuxt | 10.64946 → 10.64916 (-0.003%) | 427.63 → 429.31 (+0.395%) | 340.705 → 337.796 (-0.854%) |
| playwright | 7.52142 → 7.52146 (+0.001%) | 368.53 → 367.80 (-0.199%) | 251.549 → 251.416 (-0.053%) |
| drizzle-orm | Configuration rejected | — | — |
| typeorm | 7.75008 → 7.74927 (-0.010%) | 237.03 → 236.47 (-0.237%) | 259.330 → 260.864 (+0.592%) |
| date-fns | 3.26109 → 3.26145 (+0.011%) | 146.22 → 146.53 (+0.214%) | 127.508 → 124.597 (-2.283%) |

Single-threaded median instructions increased on 12/18 measured projects. The range was -0.024% to +0.791%,
with a median change of +0.002%; no project reduced instructions in all three matched pairs. Formbricks had
substantial variation within each binary (baseline 40.239–41.259 G; candidate 40.175–41.239 G), so its +0.791%
median is not a repeatable regression claim. Single-threaded wall changes ranged from -2.890% to +6.943%
(median -0.206%), with only two projects improving in all three pairs.

Default peak RSS changed -0.807% to +0.395% (median -0.030%): 11/18 median values decreased, but only one
project decreased in every pair. Default wall changed -2.283% to +3.569% (median -0.048%), with none improving
in every pair. Single-threaded RSS was slightly lower on 17/18 (median -0.018%), but only seven did so in every
pair. These mixed results do not establish a repeatable speed or default-memory benefit, so the original Fx maps
were restored. This does not rule out a different cache-specific use of hashbrown's APIs; it rules out adopting
this group solely by replacing its map type.

The candidate passed `cargo check -p tsrs_checker` and the catalog diagnostic comparison. Because the trial
was discarded, it did not proceed to another full conformance run. The retained implementation and its correctness
results above are unchanged. Local evidence: `bench/.work/data-structures-hashbrown-20261026/`, including saved
binaries, `candidate.patch`, the 20 field names, `paired.mjs`, per-run counter output and `catalog-summary.json`.

Reproduce the two parts of the comparison from the saved artifacts:

```sh
node bench/.work/data-structures-hashbrown-20261026/paired.mjs Compiler,Compiler-Unions,xstate-main,vscode,t3code-server 3 candidate single,reference screen
node bench/.work/data-structures-hashbrown-20261026/paired.mjs webpack,mui-docs,cal-diy,formbricks-web,supabase-studio,mikro-orm,next-packages-next,next-root,storybook,nuxt,playwright,drizzle-orm,typeorm,date-fns 3 candidate single,reference catalog-rest
```
