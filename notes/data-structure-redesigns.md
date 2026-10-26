# Collection redesign trials (2026-10-26–27)

This continues the [collection audit](data-structure-review.md) and
[small-buffer follow-ups](data-structure-followups.md), starting from
`f9a7c8d6e14472ac55ff1b036f2ef3c4faa2d631`. That baseline already contains the accepted SmallVec and
SmallBitVec changes. The retained changes are lazy groups of concurrent-map shards, smaller generic link-store
chunks, and Fx hashing for ordered file events. No additional crate dependency is needed.

## Coverage and contracts

A source inventory covered 996 Rust files. Counts are textual references, including imports, declarations and
uses; they are not numbers of runtime allocations or independent containers.

| Family | References / files | Decision |
| --- | ---: | --- |
| FxHashMap / FxHashSet | 1,322 / 218; 488 / 114 | Keep. The earlier direct-hashbrown checker-cache trial remains rejected; a generic map substitution supplies no new evidence. |
| OrderedMap / OrderedSet | 260 / 65; 27 / 13 | Keep insertion order wherever it reaches diagnostics, emit or protocol replies. Use the existing Fx-hashed OrderedMap for watcher events. |
| HashTable | 33 / 9 | Keep existing cached-hash and custom-probe implementations. |
| Vec | 6,678 / 565 | Keep contiguous general-purpose storage; screen selected persistent fields, multimap values and an actual front-removal queue separately. |
| VecDeque / BinaryHeap | 26 / 10; 3 / 1 | Keep existing queues and priority scheduling. The declaration queue trial below regresses instructions. |
| BTreeMap / BTreeSet | 90 / 23; 18 / 9 | Keep sorted traversal, ranges and deterministic output. A hash table would not supply those contracts. |
| SyncMap / SyncSet | 126 / 28; 36 / 10 | Preserve 64 logical shards, single hashing, borrowed loads, detached range callbacks, owned results and lock poisoning. Change shard allocation; SyncSet inherits it. |
| MultiMap | 17 / 6 | Screen inline-one SmallVec and ThinVec values; retain Vec after mixed measurements. |
| CopyOnWriteMap | 12 / 3 | Screen an Rc-based persistent hash trie; retain the current scoped map. |
| SmallVec / SmallBitVec | 14 / 2; 1 / 1 | Keep the already measured temporary buffers. Do not repeat rejected inference capacities or boolean SmallVec. |
| PackedStr | 58 / 6 | Keep the packed representation. A general string crate would change the existing arena/handle layout. |

The review also covered project dirty-map snapshots, checker link stores, symbol tables, JSON property storage,
compressed handles and arena-backed arrays. Symbol/JSON maps have specialized ordering and compact layouts;
project snapshots have sharing and publication contracts. Persistent-map trials target those actual APIs rather
than replacing their surrounding synchronization. The dense-ID stores already have separate compact indexes;
this round changes only generic LinkStore chunk tails. Previously rejected global interning, sparse ID pages,
whole speculative-region rollback and broad checker-map swaps were not repeated.

The existing pinned Go implementation was read for the concurrent/scoped-map and declaration-queue contracts.
The repository's `ts-ref` HEAD differs from the Cargo pin, so it was preserved. Correctness runs instead use a
separate checkout at `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`; its tracked compiler source is unchanged.

## Census and layouts

A temporary instrumented dev build ran Compiler, Compiler-Unions, xstate-main, webpack, vscode, mikro-orm and
t3code-server in both fast/reference modes, each with one checker. The counters record constructions, operations,
final lengths and clone events. They do **not** count exact allocator calls, reallocations or simultaneously live
objects, and they do not estimate peak RSS savings by summing lifetime allocations.

For example, reference-mode vscode constructs 41,503 SyncMaps, makes 494,742 loads and 81,588 writes. Each old
empty map immediately allocates 3,072 bytes of locks/tables on this native target. Their cumulative construction
bytes are 127,497,216, not a live-memory measurement. xstate and webpack also construct thousands of these maps.
That evidence motivates delaying allocation rather than replacing a well-performing table or hash function.

| Native layout | Old | Candidate |
| --- | ---: | ---: |
| SyncMap inline header | 16 B | 128 B |
| Eager shard allocation | 3,072 B | 0 B |
| First initialized group | already allocated | 384 B for eight shards |
| Header plus all 64 initialized shards | 3,088 B | 3,200 B |
| NodeBuilderContext with two ThinVec fields, rejected | 440 B | 408 B |

An initial lazy slice representation had a 192-byte header; fixed-length boxed arrays reduce it to 128 bytes.
Eight OnceLocks publish groups of eight ordinary RwLocks. Reads and deletes of uninitialized groups allocate
nothing. Cloning copies initialized groups and preserves shard order; range callbacks run after owned entries
have been detached, so callbacks can mutate the map. Fully populated maps have a 112-byte header cost, and first
writes have a group-initialization cost. These tradeoffs require whole-workload measurements.

The LinkStore candidate reduces the generic chunk size from 1,024 to 64 values, limiting unused last-chunk
capacity in short stores. Values remain in stable arena allocations; key hashing, nil-key support and slot
indexes are unchanged. The separate 4,096-entry ID-link chunks are unchanged.

The census also found 2,341 NodeBuilderContext constructions on reference vscode, and 5,956 scoped-map scope
entries, but no inherited scoped-map clone events on these CLI workloads. A native LSP workload with 20 edits,
diagnostics and generic-function hover found 28 dirty-snapshot clone events, totaling 20 entries with maximum
one entry per clone; scoped-map inherited clones were again absent. Persistent tries therefore have little
observed copying to eliminate in these workloads. These are workload-specific observations, not proofs that
larger project snapshots never occur.

A separate xctrace Time Profiler recording of the baseline fat-LTO vscode reference run captured 8,879 stack
samples. The allocator's allocation entry point appears in 124 inclusive samples and reallocation in 51.
Nested stack counts overlap and are not additive allocation costs. The saved trace and exported samples are
usable even though xctrace returned status 54 after recording. Instrumentation and profiling were removed from
all timed binaries.

## Measurement method

Host: Apple M3 Max, 14 cores, 36 GB RAM, aarch64-apple-darwin, rustc 1.99.1.

Short screens use matched release builds with full debug symbols, stripping disabled, LTO disabled and 16
codegen units. This reduces experiment rebuild time; those numbers are selection evidence, not the final
shipping-build claim. Each CLI screen uses seven projects, one diagnostic warmup per binary and three
interleaved pairs. Fast mode uses one checker for instructions; reference mode uses default checkers for RSS.
Hardware retired instructions, peak RSS and wall time come from Hyperfine 2.

Final confirmation uses the normal fat-LTO, one-codegen-unit release configuration for both binaries:

```sh
cargo build --release -p tsrs_cli --config profile.release.debug=true --config profile.release.strip=false
```

All 19 catalog entries are attempted in four modes: fast/reference, each single-threaded and with default
checkers. The configuration-only drizzle-orm entry is excluded from performance conclusions, leaving 18 projects.
Two batches each have five interleaved pairs and one warmup per binary; the second batch reverses the initial
order. Default reference checking uses four checkers; fast defaults use the machine/file-count rule. Catalog
checkout pins and suite pins are recorded. Existing catalog configuration overlays are preserved. Builds,
correctness runs, census and profiling do not overlap timed runs.

Each measured cell requires identical diagnostic bytes and exit status, and records binary and input-config
hashes. Tables report pooled medians of ten observations per binary. Repeatability also uses each batch's median
of paired percentage changes, since a ratio of independent medians can exaggerate a noisy outlier.

Raw source snapshots, patches, immutable binaries, census, profiles, output hashes and every timing/counter
sample are in the ignored local directory `bench/.work/collections-redesign-20261026/`. `select.mjs` guards
experimental source switching with fingerprints; the final source snapshot is `final-fat/source.json`.

## Isolated CLI screens

These percentages are the median across seven projects of changes between each binary's three-observation
medians. The lower-project counts use the same statistic. They are short screens, not repeatability claims.

| Trial | Fast single-checker instructions | Projects lower | Reference-default RSS | Projects lower | Disposition |
| --- | ---: | ---: | ---: | ---: | --- |
| Lazy shard groups, slice header | -0.108% | 6/7 | -0.200% | 6/7 | Refine header |
| parking_lot 0.12.5, eager shards with poisoning wrapper | -0.021% | 7/7 | -0.271% | 7/7 | Compare directly with lazy groups |
| papaya 0.2.5 | +0.148% | 2/7 | +0.926% | 2/7 | Reject this adapter |
| scc 3.8.8 | -0.039% | 5/7 | -0.318% | 5/7 | Reject contract mismatch |
| Single snapshot Vec for SyncMap range/keys | -0.002% | 5/7 | +0.141% | 1/7 | Keep existing code |
| Lazy slice groups plus snapshot Vec | -0.088% | 4/7 | -0.203% | 4/7 | No consistent incremental benefit |
| LinkStore chunks of 64 | -0.027% | 4/7 | -0.401% | 7/7 | Confirm |
| LinkStore chunks of 256 | +0.015% | 2/7 | -0.258% | 6/7 | Prefer 64 |
| ThinVec 0.2.21 in NodeBuilderContext | -0.013% | 5/7 | +0.066% | 3/7 | Check LSP, not retained |
| SmallVec inline-one multimap values | +0.002% | 3/7 | -0.016% | 4/7 | Not retained |
| ThinVec multimap values | -0.004% | 6/7 | -0.081% | 5/7 | Not retained; no broad memory gain |
| rpds 1.2.1 / archery 1.2.3 scoped map | -0.004% | 4/7 | -0.019% | 5/7 | Check LSP, not retained |
| Lazy shard groups, array header | -0.083% | 7/7 | -0.208% | 4/7 | Confirm |
| Lazy array groups plus parking_lot | -0.097% | 6/7 | -0.170% | 4/7 | Direct comparison below |

The papaya and scc adapters also fail the existing vacant-insertion Clone-panic/poisoning contract: an insertion
can become visible before the returned value is cloned, and subsequent access does not panic on a poisoned lock.
This does not mean those crates cannot support another API; these adapters do not preserve this one.
The parking_lot prototype emulates poisoning, including writes during unrelated unwinding, and passes the
focused contract checks. Against the lazy std-lock array candidate directly, its fast single-checker
instructions change by +0.002% (only 3/7 projects lower), and reference-default RSS by +0.070% (3/7 lower).
There is no consistent incremental benefit to justify the dependency and poisoning emulation.

The multimap screens also need care: the census records no multimap insertions on xstate or webpack, and only
eight on Compiler, six on Compiler-Unions and 18 on t3code-server. ThinVec's lower independent medians on 6/7
projects do not mean all pairs improve: only two projects are lower in all three pairs. Its apparent instruction
changes include workloads with no affected writes, and the larger outliers are on the variable mikro-orm and
t3code runs. This screen does not establish a repeatable affected-workflow benefit for another dependency.

## Affected workflows

The native LSP driver opens a public vscode source file, makes 20 full-text comment edits and asks for
diagnostics and hover on a generic function after each edit. All replies match after normalizing opaque
diagnostic result IDs. Counters measure the actual native server process, not its Node driver. The driver closes
stdin after shutdown/exit so the process exits and counters are collected.

Initial three-pair LSP screens suggested gains for scoped rpds and ThinVec context fields. A second five-pair
batch with reversed initial order did not confirm them: paired instruction-change medians were +0.043% for
scoped rpds and -0.211% for ThinVec, with mixed pair signs and baseline spreads up to 0.315% and 1.826%
respectively. Persistent dirty snapshots had -0.020% paired median against a 1.310% baseline spread.
Independent-median headlines disagree with paired medians on these noisy runs. None is retained.

A separate xstate incremental screen compared cold, no-edit warm, leaf-comment edit and export-hub-comment
edit runs. The initial lazy-slice prototype reduced instructions by 0.200%, 0.644%, 0.200% and 0.211%, with RSS down 0.350%,
0.726%, 0.488% and 0.377%; wall time was mixed. These three-pair screens support exercising the changed caches
but are not a general incremental-speed claim. Edited public sources were restored and checksummed.

The declaration queue experiment generates public synthetic namespace/import-alias fixtures with queues of
128, 512, 2,048 and 4,096 statements; the census confirms the queue reaches those lengths. Declaration output
matches the pinned Go compiler. Replacing the front-removing Vec with VecDeque increases instructions by
0.110%, 1.768% and 4.321% at the last three sizes. RSS is roughly flat and wall time mixed. The queue's other
operations matter; the isolated cause of this regression was not established. Keep Vec.

The watcher probe uses the actual public watcher API with a fake event backend and 4,096 real public vscode
source paths. Ten update batches deliver 40,960 callbacks, verifying event order and output hashes. The existing
75 ms debounce remains. Both binaries use the same non-LTO screen configuration and native process counters.
Replacing default-hasher IndexMap with the core Fx-hashed OrderedMap reduces instructions by 1.785% and 1.702%
in two five-pair batches with reversed initial order. RSS changes by +2.160% and +0.204%; wall time by -0.864%
and +0.924%. This is a repeatable instruction saving in an affected workflow, with no wall-time claim and mixed
small-process memory. The testrunner's cold default-hasher map was also reviewed but not changed without evidence.

| Watcher batch | Instructions, M | Peak RSS, MiB | Wall, ms |
| --- | ---: | ---: | ---: |
| 1 | 680.418 → 668.276 | 15.188 → 15.516 | 956.24 → 947.98 |
| 2 | 680.135 → 668.560 | 15.313 → 15.344 | 945.77 → 954.51 |


## Final fat-LTO catalog comparison

The combined change is retained. The median paired single-checker instruction change across projects is
-0.103% / -0.140% in the two fast batches and -0.149% / -0.134% in the two reference batches.
Both batches have a lower paired instruction median on 16/18 fast-mode and 15/18 reference-mode projects.
Default-checker RSS has a lower paired median in both batches on 15/18 projects in each mode.
This meets the balanced policy through repeated instruction and memory reductions across most of the catalog.

| Metric | Median change across projects, pooled observations | Projects lower, pooled | Lower paired median in both batches |
| --- | ---: | ---: | ---: |
| Fast single-checker instructions | -0.139% | 16/18 | 16/18 |
| Reference single-checker instructions | -0.188% | 16/18 | 15/18 |
| Fast default-checker peak RSS | -1.160% | 18/18 | 15/18 |
| Reference default-checker peak RSS | -0.513% | 18/18 | 15/18 |
| Fast default-checker wall | -0.739% | 14/18 | 9/18 |
| Reference default-checker wall | -0.333% | 14/18 | 7/18 |

The pooled instruction regressions are t3code-server (+0.353%) and mikro-orm (+0.178%) in fast mode, and
next-root (+1.727%) and playwright (+1.164%) in reference mode. The paired table below preserves the differing
batch signs: for example, t3code-server's fast paired medians are negative in both batches despite its positive
pooled ratio. Large pooled reductions on next-root, storybook and nuxt similarly exceed their typical paired
changes and are not treated as standalone gains of that magnitude.

Run variation matters here. In the second fast-single Next package cell, the baseline spans about 16.728 to
17.111 G instructions and the candidate about 16.723 to 17.082 G. That batch's independent-median ratio is
+0.841%, while its median paired change is -0.033%. In the second fast-default vscode cell, the same baseline
binary spans 1.077 to 1.839 seconds. Wall time remains workload- and batch-dependent: the pooled default-wall
ranges are -3.086% to +0.401% in fast mode and -2.126% to +2.246% in reference mode. There is no general
wall-time speedup claim. The comparison describes these local release-profile builds on this host.

All 1,440 timed exit statuses match their warmups; diagnostic hashes match within and between batches. Binary
hashes, config hashes and catalog inputs remain unchanged. The following tables include every usable project,
with instructions retired and peak RSS before and after. Drizzle-orm remains recorded as configuration-only.

### Fast mode

| Project | Single-checker instructions, G | Default-checker peak RSS, MiB | Default-checker wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 80.28546 → 80.21783 (-0.084%) | 2038.03 → 2035.00 (-0.149%) | 1266.95 → 1243.90 (-1.819%) |
| xstate-main | 5.89030 → 5.85703 (-0.565%) | 248.82 → 244.24 (-1.840%) | 132.66 → 132.04 (-0.473%) |
| webpack | 10.68356 → 10.67489 (-0.081%) | 480.75 → 477.95 (-0.582%) | 167.68 → 167.43 (-0.148%) |
| mui-docs | 38.07205 → 37.93974 (-0.348%) | 1505.26 → 1497.48 (-0.517%) | 1216.83 → 1189.30 (-2.263%) |
| Compiler | 1.68312 → 1.68253 (-0.035%) | 76.80 → 75.09 (-2.228%) | 64.65 → 64.68 (+0.050%) |
| Compiler-Unions | 2.84987 → 2.84883 (-0.036%) | 81.66 → 80.15 (-1.856%) | 98.16 → 96.91 (-1.278%) |
| cal-diy | 30.88591 → 30.80370 (-0.266%) | 1758.63 → 1744.77 (-0.789%) | 800.89 → 787.78 (-1.637%) |
| formbricks-web | 39.36170 → 39.30530 (-0.143%) | 1883.38 → 1881.55 (-0.097%) | 808.67 → 811.92 (+0.401%) |
| supabase-studio | 43.39371 → 43.36067 (-0.076%) | 1591.52 → 1578.72 (-0.804%) | 838.51 → 831.50 (-0.836%) |
| t3code-server | 39.13782 → 39.27614 (+0.353%) | 1982.41 → 1979.73 (-0.135%) | 1399.32 → 1378.17 (-1.512%) |
| mikro-orm | 47.12614 → 47.21002 (+0.178%) | 1861.48 → 1829.35 (-1.726%) | 853.28 → 826.95 (-3.086%) |
| next-packages-next | 16.73854 → 16.72181 (-0.100%) | 622.13 → 612.47 (-1.552%) | 273.38 → 271.46 (-0.704%) |
| next-root | 16.68278 → 16.29386 (-2.331%) | 457.31 → 454.37 (-0.644%) | 383.94 → 384.91 (+0.253%) |
| storybook | 9.95660 → 9.73543 (-2.221%) | 470.74 → 463.61 (-1.515%) | 196.43 → 197.11 (+0.346%) |
| nuxt | 9.87266 → 9.84916 (-0.238%) | 483.74 → 475.90 (-1.621%) | 239.72 → 239.10 (-0.261%) |
| playwright | 7.45107 → 7.44100 (-0.135%) | 441.26 → 433.16 (-1.836%) | 144.54 → 144.54 (-0.001%) |
| typeorm | 7.56673 → 7.54042 (-0.348%) | 251.23 → 245.73 (-2.186%) | 174.16 → 169.41 (-2.727%) |
| date-fns | 2.86010 → 2.84189 (-0.637%) | 146.91 → 146.50 (-0.277%) | 123.12 → 122.16 (-0.775%) |

### Reference mode

| Project | Single-checker instructions, G | Default-checker peak RSS, MiB | Default-checker wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 90.47476 → 90.43139 (-0.048%) | 2408.42 → 2406.67 (-0.073%) | 2462.26 → 2450.28 (-0.486%) |
| xstate-main | 6.26284 → 6.25238 (-0.167%) | 266.40 → 263.90 (-0.938%) | 196.90 → 196.88 (-0.011%) |
| webpack | 11.10570 → 11.08266 (-0.207%) | 404.77 → 402.95 (-0.452%) | 323.89 → 318.28 (-1.731%) |
| mui-docs | 40.33401 → 40.22660 (-0.266%) | 1323.10 → 1322.88 (-0.017%) | 1676.47 → 1676.40 (-0.005%) |
| Compiler | 1.71704 → 1.71641 (-0.037%) | 77.66 → 75.98 (-2.163%) | 64.93 → 64.43 (-0.772%) |
| Compiler-Unions | 2.94738 → 2.94662 (-0.026%) | 81.41 → 79.91 (-1.843%) | 98.28 → 98.10 (-0.184%) |
| cal-diy | 33.04271 → 32.96388 (-0.239%) | 1344.55 → 1339.86 (-0.349%) | 1126.75 → 1134.64 (+0.700%) |
| formbricks-web | 44.38140 → 44.31937 (-0.140%) | 1988.86 → 1984.92 (-0.198%) | 1392.68 → 1387.14 (-0.398%) |
| supabase-studio | 46.16012 → 46.08237 (-0.168%) | 1410.08 → 1402.70 (-0.523%) | 1638.42 → 1628.46 (-0.608%) |
| t3code-server | 48.24808 → 48.24538 (-0.006%) | 1865.30 → 1860.51 (-0.257%) | 2054.98 → 2055.27 (+0.014%) |
| mikro-orm | 53.54931 → 53.36260 (-0.349%) | 1632.05 → 1621.89 (-0.623%) | 1634.70 → 1654.67 (+1.222%) |
| next-packages-next | 17.93453 → 17.72394 (-1.174%) | 603.02 → 600.53 (-0.413%) | 702.39 → 700.50 (-0.268%) |
| next-root | 17.22484 → 17.52224 (+1.727%) | 469.66 → 467.20 (-0.526%) | 520.55 → 520.51 (-0.008%) |
| storybook | 10.23895 → 10.12303 (-1.132%) | 446.63 → 444.38 (-0.504%) | 304.02 → 310.84 (+2.246%) |
| nuxt | 10.58067 → 10.33705 (-2.303%) | 428.05 → 422.70 (-1.248%) | 331.52 → 329.29 (-0.673%) |
| playwright | 7.68131 → 7.77075 (+1.164%) | 367.41 → 365.47 (-0.529%) | 263.12 → 258.60 (-1.718%) |
| typeorm | 7.64006 → 7.61335 (-0.350%) | 235.87 → 233.09 (-1.179%) | 254.54 → 249.57 (-1.949%) |
| date-fns | 2.91646 → 2.89892 (-0.602%) | 146.34 → 146.22 (-0.080%) | 121.97 → 119.38 (-2.126%) |

### Paired single-checker instruction changes

Each cell is that project's median of the five paired percentage changes in the indicated batch.

| Project | Fast A | Fast B | Reference A | Reference B |
| --- | ---: | ---: | ---: | ---: |
| vscode | -0.084% | -0.119% | -0.063% | -0.065% |
| xstate-main | -0.094% | -0.276% | -0.162% | -0.090% |
| webpack | -0.061% | -0.080% | -0.137% | -0.050% |
| mui-docs | -0.429% | -0.381% | -0.275% | -0.288% |
| Compiler | -0.035% | -0.315% | -0.039% | -0.047% |
| Compiler-Unions | -0.015% | -0.037% | -0.020% | -0.022% |
| cal-diy | -0.287% | -0.103% | -0.316% | -0.250% |
| formbricks-web | +0.048% | -0.160% | -0.160% | -0.120% |
| supabase-studio | -0.001% | -0.058% | -0.161% | -0.149% |
| t3code-server | -0.048% | -0.042% | +0.018% | -0.015% |
| mikro-orm | +0.557% | -0.072% | -0.074% | -0.071% |
| next-packages-next | -0.113% | -0.033% | -0.115% | -0.165% |
| next-root | -0.157% | -0.310% | +0.017% | -0.269% |
| storybook | -0.216% | -4.217% | -0.217% | -0.292% |
| nuxt | -0.273% | -0.114% | -0.254% | -0.223% |
| playwright | -0.115% | -0.171% | -0.032% | +0.027% |
| typeorm | -0.299% | -0.466% | -0.300% | -0.429% |
| date-fns | -0.464% | -0.827% | -0.489% | -0.790% |

## Correctness and tooling

Both baseline and candidate run against the verified Cargo-pinned Go source in the separate reference checkout.
An isolated workspace uses that checkout for crates whose integration checks load `ts-ref`; the user's original
checkout is preserved. The candidate's source/config snapshot is verified identical in the isolated workspace
and the final fat-LTO build.

- Workspace integration checks: baseline 1,242 passed, candidate 1,248 passed; four ignored in each. The six added
  concurrent-map cases cover racing insertion, callback mutation and early termination, empty/clone independence,
  poisoned writes and writes during unrelated unwinding.
- ThreadSanitizer with `plain-ptrs`: all nine SyncMap cases and ten checker link-store cases pass, including growth
  and stable-cell checks. The instrumented standard library is rebuilt on nightly-2026-10-04.
- Full compiler suites: 15,448 variants in each history mode, with identical per-case outcomes to baseline across
  diagnostics, types, symbols, JavaScript, JavaScript maps and source-map records. Reference mode initially has
  the same three timeout cases as baseline. The same two cases pass a 180-second retry on both binaries, leaving
  13,699 diagnostic passes and the pre-existing `intersectionConstructorReductionCrash` timeout. Fast mode also
  has 13,699 passes and only that timeout. There are no baseline mismatches or crashes; the full gate still exits
  nonzero for the existing timeout.
- All 36 focused scheduling sweeps pass: ten default-assignment repetitions and eight random seeds, each at four
  and eight checkers, with all output baseline classes enabled. All 50 regression fixtures pass.
- Go Fourslash: identical terminal outcomes for all 4,638 named test/subtest records. Top-level outcomes are
  4,093 pass / 469 skip; effective coverage is 4,092 pass / 470 skip, identical to baseline.
- Generated declaration stress fixtures at all four queue sizes match the freshly rebuilt pinned Go compiler
  byte for byte. Each watcher probe run preserves all 40,960 callbacks and their order.
- Clippy across all targets with warnings denied, source lint, formatting and the wasm32-wasip1 check pass.

Project-level determinism also passes all 96 comparisons on Compiler, Compiler-Unions, xstate-main and webpack:
Go, locality, default and random assignment, each at four and 14 checkers, with three repetitions.
