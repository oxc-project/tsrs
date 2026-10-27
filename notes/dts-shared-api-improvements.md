# Shared compiler and declaration-bundler improvements (2026-10-26)

Starting revision: `7a00a0d7fc11be1e557e06846d0302cb842691c3`, with existing working changes preserved.
Reference: microsoft/TypeScript `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.
The reference HEAD matches Cargo metadata and its tracked `tsc` sources remain unchanged.
All changes are local; no commit or publication is part of this experiment.

## Program-owned symlink cache

`Program::get_symlink_cache` previously selected an allocation region by looking up the owner of
`processed`. That object is boxed, outside registered arena regions. Once owner routing is active,
its lazy cache therefore falls back to the calling thread's never-freed arena. Entering an invocation
region does not intercept that explicit fallback.

The cache now enters the Program's shared `ProgramRegions` when present, including while constructing
its child symlink records and sets. Successful Program updates clone those regions with their options,
so a cache copied to another version remains alive. Programs without regions retain the previous path.
The focused regression retires the original Program, accesses the cache through the updated Program,
and verifies that the regions are released after the final Program is retired. It also passes with
`TSRS_ARENA_POISON=1`.

The affected callers include the declaration bundler, watch mode and project builds that free their
Programs. Ordinary one-shot `-p` compilation does not enable `ProgramRegions`; this is not a general
CLI speedup claim. The bundler's outer invocation region remains necessary for other escaping allocations.
Matched release measurements use Apple M4 Max arm64, the system allocator, fat LTO, one codegen unit,
full debug symbols and no stripping. Both frozen source trees contain the same concurrent changes;
only the cache ownership initializer differs. Seven alternating baseline/candidate pairs follow warmups
for each workload (42 measured processes). This directly loads, fills the cache and frees Programs;
it does not type-check or bundle. The original general full-DTS fixture lacked symlink packages and
did not exercise the discovered leak, so the experiment narrowed to exact cache-lifecycle retention.

| Symlink packages × lifetimes | Baseline → candidate peak RSS (bytes) | Baseline → candidate instructions | Baseline → candidate elapsed seconds |
| --- | ---: | ---: | ---: |
| 200 × 1 | 8,830,976 → 8,863,744 (+0.37%) | 151,539,535 → 152,911,368 (+0.91%) | 0.0490 → 0.0455 |
| 200 × 20 | 42,958,848 → 14,106,624 (−67.16%) | 2,332,680,593 → 2,338,239,552 (+0.24%) | 0.7383 → 0.7463 |
| 2,000 × 1 | 39,632,896 → 39,780,352 (+0.37%) | 7,915,936,678 → 7,929,615,928 (+0.17%) | 3.0498 → 2.8970 |

These are medians per variant. The repeated-lifecycle candidate uses less RSS in all seven pairs;
baseline RSS ranges 42,893,312–43,057,152 bytes, candidate 14,024,704–14,270,464. Exact calling-thread
arena retention is `160 * packages + 96` bytes per lifetime before the fix and zero afterwards in every
sample. At 200 packages × 20 lifetimes that removes 641,920 arena bytes; peak RSS additionally includes
heap data owned by those arena objects. Single-lifetime RSS is slightly higher and freeing the cache
adds work. This is a memory-lifetime correction with measured costs, not a speedup.

Elapsed time is unstable under heavy external host load: the repeated-lifecycle baseline ranges
0.434–1.268 seconds and candidate 0.345–1.946 seconds. Its median paired elapsed change is +18.02%,
while the ratio of separate medians is +1.09%; neither supports a reliable wall-time conclusion. Median
paired instruction change is +0.10%, versus +0.24% from separate medians. No catalog, full bundler,
ordinary CLI, or end-to-end build/watch performance improvement is established.

Raw data, complete stdout/stderr, hashes and trial decision:
`target/scratch/shared-dts-ownership-trace/release-measurement/{raw,summary,contract,integrity,trial-decision,trial-plan,environment}.json`.
Repeat timings with `node target/scratch/shared-dts-ownership-trace/measure-release.mjs measure` using the
frozen snapshots and binaries described there. Preserve the existing JSON before rerunning, since measurement
replaces it; `prepare` recaptures live sources and is not a replay. All 42 samples and summary medians were
independently checked.

## UMD invalidation follows the pinned reference

An unchanged UMD consumer has no reference edge to its external-module provider, and that provider does
not affect global scope. Changing a type re-exported by the provider can therefore leave cached
incremental diagnostics stale. The Rust implementation and pinned Go match on all 88 transitions in
each of three exploratory matrices:

| Matrix | Cached incremental versus fresh mismatches | Rust versus Go differences |
| --- | ---: | ---: |
| Initially valid consumer | 40 / 88 | 0 |
| Initially erroneous consumer | 72 / 88 | 0 |
| Explicit import of the provider | 0 / 88 | 0 |

The matrix covers script/module consumers, single/default checker counts, independent compiler
Programs with reused incremental state, and actual compiler Program reuse. These are exploratory
comparisons, not conformance-suite pass counts.

Minimal initially valid input, with `noLib: true`, minimal built-in interfaces and all files included:

```ts
// umd.d.ts
export as namespace UmdApi;
export type { Item } from './shape';
// shape.d.ts
export interface Item { value: number }
// client.ts
declare const item: UmdApi.Item;
const checked: number = item.value;
```

Warm the old incremental diagnostics, change `value: number` to `value: string`, then reuse the old
incremental snapshot. Cached checking returns no errors while fresh checking reports TS2322. Adding
`import './umd';` to the consumer supplies the missing dependency and refreshes diagnostics.

`umd_imported_type_changes_preserve_pinned_upstream_stale_diagnostics` records this pinned behavior.
The shared incremental algorithm remains reference-faithful. The declaration bundler retains its
conservative UMD exclusion; this cross-edit evidence does not independently prove that its narrower
source-to-declaration reuse path fails. Removing that gate needs direct parity evidence for that path.

Raw paired outputs and Go overlay:
`target/umd-investigation/{rust,go}-results{,-reverse,-control}.txt`, `umd_test.go`, `run-go.mjs`.
The overlay does not modify tracked reference sources.

## Independent mapped printer writes

`Printer::write_with_source_map` clears the most-recent source/index and JSON flag around one write,
then restores them. Each independent generator registers its own sources, while generated names and
other printer state remain shared. Existing `Printer::write` keeps its pinned behavior.

The bundler no longer seeds every fragment generator with all sources from preceding fragments.
Each fragment stores its local-to-global source-index mapping, which final assembly translates before
composing original declaration maps. A focused printer regression alternates fresh and pre-seeded
generators with inline source content and resumes an ordinary mapped write afterwards.

Matched fixture checking passes all 223 candidate cases. The baseline passed 212 initially; 11 failed
because the isolated source copy lacked the existing test-only npm dependencies, then passed after
linking those dependencies into the scratch copy. No dependencies were installed. All 139 freshly
captured artifact paths and contents are byte-identical between baseline and candidate, independently
compared after capture. These captured files are declaration artifacts; map correctness is additionally
covered by the pipeline map tests and focused printer regression.

Matched release measurements use the same Apple M4 Max, fat LTO, one codegen unit, full debug symbols
and no stripping. Frozen baseline and candidate contain identical concurrent changes; only the printer
API and bundler map integration differ at runtime. Both variants build at the same absolute source path.
Two warmups per variant/workload precede five alternating AB/BA pairs (40 measured processes).
Each process links captured declarations once, then hashes every output path and content. The bundler
forces one checker, so a requested-default repeat would use the same effective checker count.

The following values are independent medians before → after. Percentages in parentheses are median
paired changes, which can differ from the ratio of those independent medians.

| Synthetic workload | Retired instructions | Elapsed milliseconds | Peak RSS (MiB) |
| --- | ---: | ---: | ---: |
| Maps, 4 sources × 120 items | 1,105,217,005 → 1,015,012,769 (−8.189%) | 82.20 → 75.76 (−6.329%) | 47.02 → 46.88 (−0.267%) |
| Maps, 4 sources × 1,200 items | 4,034,987,892 → 3,128,309,180 (−22.470%) | 220.21 → 191.07 (−20.405%) | 78.27 → 79.64 (+1.262%) |
| Maps, 120 sources × 10 items | 10,358,041,302 → 1,390,579,089 (−86.575%) | 502.38 → 88.84 (−82.025%) | 52.56 → 52.59 (+0.179%) |
| No maps, 4 sources × 1,200 items | 1,525,313,204 → 1,522,618,455 (−0.097%) | 111.57 → 111.47 (−1.549%) | 76.64 → 75.70 (−0.247%) |

Each item retains two declarations (960, 9,600 and 2,400 declarations in the mapped cases). All mapped
cases reduce instructions and elapsed time in all five pairs. Paired instruction ranges are
−8.314% to −8.155%, −24.387% to −21.805%, and −86.622% to −86.523%, respectively. The many-source
case benefits most from removing repeated source-table seeding. The no-map control is approximately flat.

The large mapped case has an explicit memory tradeoff: RSS increases in four of five pairs, with paired
changes −0.933% to +2.824%. Its independent median RSS increases 1.757%, versus the paired median
1.262%. Wall measurements remain sensitive to external host load (one-minute load 20.43–24.96 on
16 logical CPUs): the large case's paired wall range is −64.273% to −13.233%, and its independent median
wall reduction is 13.233%, versus paired median 20.405%. The many-source wall range is −83.477% to
−80.815%. Control wall/RSS ranges cross zero. These results support retaining the mapped integration;
no compiler-wide, catalog, checker-scaling or general end-to-end speedup is established. The matrix was
reduced before measurement, as recorded in `measurement-plan.json`.

All 267 synthetic artifact paths and contents, including 131 maps, were independently byte-compared.
Every timed output digest agrees. Raw counters, all 40 samples, paired statistics, source manifests and
runtime parity with this checkout were independently verified. Later source changes were confined to tests.
Corpus logs and captures are under `target/scratch/printer-map-integration/`, including `corpus.json`,
`baseline-corpus/` and `candidate-corpus/`. Measurement evidence includes `measurements.json`,
`correctness.json`, `provenance.json`, `variant-source-manifests.json`, `source-verification.json`,
`candidate.patch`, `builds.json`, `measurement-plan.json`, `artifacts/` and `timings/`.
The independent verification is `target/scratch/shared-dts-improvements/printer-verification.json`.
Repeat timings with `REPETITIONS=5 node target/scratch/printer-map-integration/measure.mjs` using the
retained binaries; preserve existing measurement JSON, artifacts and timing files first because this
command overwrites them. `setup.mjs` captures new live sources and is not a historical replay.

Subsequent [public-project end-to-end measurements](perf-dts-public-e2e.md) cover complete builds with
maps enabled and disabled. Mapped Zod and six-entry XState builds reduce paired median wall time by
18.08% and 21.04%, with instructions down 21.68% and 27.83%; both improve in all seven pairs.
Immer's mapped instructions fall 1.16%, but wall time remains inconclusive. No-map controls and RSS are
approximately flat. All 27 output artifacts, including nine maps, are byte-identical. Those results use
warm filesystem caches, one checker and the standalone system allocator, with input adaptations recorded.

## Diagnostic fallback must use a fresh Program

For this source, with strict NodeNext, target ES2022 and only `lib: ["es2022"]`:

```ts
using resource = null;
export const wrong: string = 1;
```

Both Rust and pinned Go expose these raw collection results:

| Collection | Ordered diagnostic codes |
| --- | --- |
| Fresh ordinary Program | TS2322, TS2318 |
| Incremental collection | TS2318, TS2322 |
| Ordinary collection on that already-checked Program | TS2318 |

Incremental caches include deferred globals with file diagnostics. A subsequent ordinary check can
see the discovered global error early and skip semantic checking, omitting TS2322. Sorting incremental
results cannot reproduce ordinary phase ordering; routing incremental callbacks through the ordinary
wrapper can duplicate globals.

The bundler's error fallback now constructs a fresh private Program using its existing creation APIs,
collects ordinary diagnostics and copies the text before releasing the owner. Successful reuse creates
no extra Program. `deferred_global_errors_match_fresh_checking` asserts the exact TS2322 then TS2318 text;
`dependency_diagnostic_order_matches_fresh_checking` distinguishes semantic errors in two files.
The eight-test diagnostic reuse suite passes, including no-host-write checks. A ninth focused test,
`stripped_internal_declaration_errors_match_fresh_checking`, passes separately: pinned Go successfully
emits a reference to an internal type removed by `stripInternal`, then reports TS2304 when checking that
declaration. The bundler preserves that error with ordinary and reused checking, exercising the fresh
declaration-Program fallback. The deferred-global regression also passes with freed arenas poisoned.

Raw Rust/Go evidence and reproducible overlay drivers are under
`target/scratch/diagnostic-order/` (`cases.mjs`, `build.mjs`, `deferred/{Rust,Go}.txt`).

## Validation

- Full `tsrs_dts` library and integration test run: 263 passed across six suites. The subsequently added
  declaration-fallback regression passed separately (one test, eight filtered out), including a rerun after
  fixture-spacing cleanup. Affected-crate Clippy, formatting and whitespace checks also passed after that cleanup.
- Development compiler and conformance runner build passed.
- Compiler Program tests: 10 passed; printer tests: 16 passed; incremental unit tests: 21 passed.
- Strengthened cache-lifetime and printer-cache-restoration tests passed afterwards.
- CLI `build_free_programs`: 2 passed, with its existing poisoned-arena checks.
- Symlinked-directory edit tests in ordinary watch and build-watch modes: 2 passed.
- Workspace Clippy with warnings denied, source lint, formatting and whitespace checks passed.
- Focused compiler conformance selected 454 variants in both ordinary Go history and parallel fast mode:
  272 error/emit baselines passed, 182 skipped; 123 type and 123 symbol baselines passed;
  97 JavaScript maps and 98 source-map records passed. No failures, crashes or timeouts in either run.
  Skips reflect unsupported or absent baseline classes, not passing comparisons.
- Raw conformance reports: `target/scratch/shared-dts-improvements/conformance-{go,fast}.json`.

This is focused validation, not a rerun of all workspace tests or the complete conformance suite.
