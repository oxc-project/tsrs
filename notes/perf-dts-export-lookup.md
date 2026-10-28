# Declaration bundler direct export lookup (2026-10-26)

## Decision: rejected

The direct key lookup was withdrawn after an augmentation counterexample disproved
API equivalence. The original filtered symbol-name scan is restored. The five
previously evaluated FxHashMap graph tables remain applied. The measurements below
are historical results for the rejected candidate, not an accepted speedup.

## Experiment

Replace only the membership test in `graph::export_is_type_only`:
`get_exports_of_module_exported(target).iter().any(...)` becomes
`try_get_member_in_module_exports(name, target).is_some()`.

The former constructs a fresh Vec of non-reserved exports and scans symbol names
for each name followed through an export-star edge. The latter uses the existing
checker API to query its resolved export table. Both initialize exports through
`get_exports_of_module`; no checker algorithm or public API changes. The
path-sensitive traversal, cycle guard, direct-export precedence and ordinary-path
precedence remain unchanged. External-provenance scans are outside this trial.

Baseline: local HEAD `3231a0b00338c12d4d2eded59b0ef3b2dd2f9d9d` plus the already
evaluated five FxHashMap graph tables. The candidate differs by this lookup only.
The concurrent core path experiment was reverted by its session. Both core path
files now match HEAD. Both benchmark variants are rebuilt against that restored
core, with source snapshots verifying identical compiler dependencies. Earlier
measurements with the temporary core changes remain archived separately and are
superseded by the restored-core runs.

Primary metric: fewer retired instructions. Report wall time and peak RSS as
separate metrics; accept stable improvements without a fixed percentage minimum.
One candidate, six workload cells, one warmup and nine alternating measured pairs
per cell. No holdout is used; this is a bounded synthetic linking experiment.

## Correctness

The existing differential tests run the old scan and direct lookup in independent
Programs across thirteen constructed alias/star/cycle/merged-symbol graphs and
ten existing type-only fixture directories. They assert exact alias and
path-sensitive restriction results, including the known distinction from the
aggregate star metadata query. Additional cases cover default exports, renamed
exports, string names, double underscores, Unicode, CommonJS export assignment
and qualified aliases. Membership checks include missing and default names.
During the trial, an initialization test also queried direct lookup before a
diagnostic pass. That preliminary call was removed when restoring the original
metadata test's initialization order.

The pinned TypeScript checkout is verified at
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with clean tracked `tsc` sources. Its
Go compiler and the Rust compiler produce identical diagnostic text on all four
additional cases, including TS1203/TS2498 for the CommonJS case under these options.
The API port matches `TryGetMemberInModuleExports` in that reference.

The initial validation missed a diagnostic-free module augmentation case:

```ts
// dep.d.ts
interface Original { a: number }
export { Original as Renamed };
// aug.d.ts
import './dep';
declare module './dep' { interface Renamed { b: number } }
// index.d.ts
export * from './dep';
```

After checking diagnostics, query `get_merged_symbol(file.symbol())` for dep.d.ts.
The merged export table has key `Renamed` whose symbol is named `Original`.
Enumeration finds `Original`; key lookup for `Original` returns nothing, and key
lookup for `Renamed` returns that symbol. The new Rust regression
`augmentation_keeps_export_key_distinct_from_symbol_name` verifies this with a
single checker and no diagnostics. The raw source-file symbol does not expose the
same table in this test. Equivalent API observations were independently reproduced
through the pinned Go and Rust Node APIs. The saved
`augmentation-reference.mjs` also verifies that both CLI compilers return no
diagnostics for these exact files with `--singleThreaded`. This proves API
inequivalence, but a final bundle-output difference under production's exact
checker history has not been established. A competing type/value-star source example also changes the
traced traversal result, but has TS2308 and is not a diagnostic-free output
regression.

Enumeration additionally filters reserved table keys; direct lookup does not.
Public string export names alone do not justify ignoring this distinction. Both
APIs are faithful to their distinct reference semantics. Restoring the scan avoids
assuming key/name equivalence; it does not change the checker implementation.

Historical candidate validation before rejection: all 259 bundler tests passed,
and all 139 captured fixture artifact paths and bytes matched the FxHashMap
baseline. All four differential tests passed after the final initialization
assertion, including with plain pointers. Workspace Clippy with warnings denied,
source checks, formatting and dev CLI/test-runner builds passed. After the core
experiment was reverted, all 259 tests and all 139 artifact comparisons passed
again, as did workspace checks, dev builds and the four Go/Rust diagnostic
comparisons. The first Clippy attempt caught needless borrows in the temporary
core edit; no part of that core trial remains. Export-star compiler baselines
selected 16 variants: 11 passed errors/types/symbols/emit baselines, five skipped,
none failed; source-map baselines were absent for these variants. Full compiler
conformance was not run.

After restoration, the augmentation counterexample passed with compressed and
plain pointer handles. All five metadata tests passed with plain pointers. After
removing the trial's preliminary direct lookup from the initialization test, that
focused test passed again, preserving its original metadata-query call order.
Final restoration validation passed: all 260 bundler tests across seven suites,
workspace Clippy with warnings denied, source lint, formatting, patch whitespace,
and the dev CLI/test-runner build. The other session's concurrent sourcemap edits
were preserved; these checks do not attribute performance to those edits. No new
performance measurements were run after restoration.

## Measurement

Apple M4 Max, 64 GiB, macOS 27.0 (26A428), rustc 1.99.1. Both release builds use
opt-level 3, fat LTO, one codegen unit, full debug symbols and no stripping.
`/usr/bin/time -l` supplies instructions retired and maximum RSS; wall time uses
high-resolution elapsed time around process launch and teardown.

Reuse the captured-declaration probe from `perf-dts-allocations.md`: four modules,
120 or 1,200 interfaces plus const declarations per module (960 or 9,600 retained
declarations), single barrel, four entries with shared output, and the latter with
declaration maps. The linker uses one checker internally. Each process hashes all
output paths and text; baseline/candidate hashes must match in every run.

Final repeat: medians of nine alternating baseline/candidate pairs per workload.
Instructions are billions; RSS is MiB. Negative percentages mean a reduction.

| Declarations / workload | Instructions before → after | Wall ms before → after | Peak MiB before → after |
| --- | ---: | ---: | ---: |
| 960 / single entry | 1.088 → 1.034 (-4.97%) | 71.75 → 69.33 (-3.37%) | 46.41 → 46.09 (-0.67%) |
| 960 / four entries | 0.857 → 0.857 (+0.03%) | 57.81 → 57.26 (-0.95%) | 46.50 → 46.31 (-0.40%) |
| 960 / four entries with maps | 1.101 → 1.102 (+0.11%) | 65.61 → 66.26 (+1.00%) | 46.81 → 46.67 (-0.30%) |
| 9,600 / single entry | 20.951 → 16.660 (-20.48%) | 1161.78 → 919.88 (-20.82%) | 73.34 → 73.28 (-0.09%) |
| 9,600 / four entries | 1.545 → 1.545 (+0.05%) | 96.99 → 99.58 (+2.67%) | 75.13 → 75.47 (+0.46%) |
| 9,600 / four entries with maps | 4.020 → 4.023 (+0.06%) | 199.92 → 200.29 (+0.19%) | 78.11 → 78.05 (-0.08%) |

Both single-barrel sizes retire fewer instructions in all nine pairs in both
restored-core runs. The first run reports -4.93% / -20.43% instructions and
-1.84% / -14.56% wall (small / large); the repeat reports -4.97% / -20.48%
instructions and -3.37% / -20.82% wall. Large-barrel repeat instruction ranges:
baseline 20.921–21.038 G, candidate 16.656–16.693 G. Wall ranges are much noisier:
baseline 1.079–2.459 s, candidate 0.834–1.792 s. The consistent instruction
reduction is stronger evidence than the precise wall percentage. The small-barrel
wall ranges also overlap (68.8–77.0 vs 66.7–74.1 ms).

The other workloads have mixed instruction wins (2–6 of nine pairs on repeat)
and no established benefit. Their instruction medians increase by +0.02% to
+0.50% across both runs; the largest is the first large-map measurement and falls
to +0.06% on repeat. Their worst repeat wall median is +2.67% on the large
four-entry case, versus -1.54% in the first run. The code path under test is not
exercised by those direct-entry modules, so these differences do not demonstrate
work saved there. Report them rather than claiming a universal speedup. There is
no established memory improvement: large-barrel RSS medians change by -0.17% and
-0.09%, while the paired medians are +0.47% and +0.30%; the other cells range from
-0.44% to +0.46% across the two runs.

Decision: reject the direct key lookup despite the measured large single-barrel
improvement. The earlier fixture comparisons did not establish equivalent
semantics, as the augmentation regression demonstrates. The original scan is
restored; measurements, binaries and source fingerprints remain available as
historical evidence. A future optimization must preserve filtered symbol-name
membership, including key/name mismatches. No performance holdout or external
real-world corpus was used.

## Reproduction and evidence

Local scripts, source hashes, binaries and raw results are retained in
`target/scratch/dts-export-lookup/`. `build-pair.mjs` checks that the graph change
is exactly the lookup replacement, restores the candidate even on baseline build
failure, and rejects other source changes during either build or between them.
`measure.mjs` writes every sample, median, range and paired percentage into
`measurements.json`. `compare.mjs` checks captured fixture paths and bytes.
`reference-cases.mjs` records pinned Go/Rust diagnostics in `reference-results.json`.
`augmentation-reference.mjs` records the diagnostic-free augmentation case in
`augmentation-reference.json`. `experiment.json` and its summary generator mark
the lookup trial rejected, preserving all original measurements.

These are bundler linking results, not compiler check-only catalog measurements
or real-world end-to-end declaration-build speedups. The compiler benchmark
catalog does not exercise this crate.
