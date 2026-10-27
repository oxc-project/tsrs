# Declaration bundler name reservations (2026-10-26)

## Question and variants

Replacing the linker's identifier-only `BTreeSet<String>` with repeated calls to
`SourceFile::has_identifier` removes a duplicate AST collector, but each candidate
name then scans source modules. This experiment checks that tradeoff before keeping
the replacement.

The pinned reference checkout matches Cargo's TypeScript commit
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`. Its
`tsc/internal/ast/ast.go::HasIdentifier` lazily collects identifiers, private
identifiers, strings, numbers, bigints and no-substitution template literals.
Literal inclusion is preserved; this experiment does not narrow the compiler query.

- **Baseline:** identifier-only collection during graph traversal, one aggregate
  `BTreeSet<String>` used by anonymous-name allocation and the printer callback.
- **Scan:** linker-synthesized names in a small BTreeSet, with every candidate
  checked through `has_identifier` across source modules.
- **Aggregate:** expose `SourceFile::identifier_names`, the existing immutable lazy
  cache used by `has_identifier`; union it once into a membership-only
  `FxHashSet<String>`, including linker-synthesized names. Hash iteration never
  determines naming or output order. Names are owned by the linker; AST strings
  remain owned by the source files.

The isolated bundler copy preserves other working-tree changes equally. Baseline
and scan snapshots, the exact reverse patch, source fingerprints, build configuration,
Rust probe, JavaScript runner and raw measurements are retained locally under
`target/name-reservation-bench/`. No experiment changed git branches or committed.
Concurrent documentation commits advanced HEAD while work ran; the starting revision
and diff are captured in `environment.json`. Dependency source fingerprints were
unchanged across the initial baseline/scan builds. The first final rebuild was
stopped after concurrent compiler/printer production edits appeared while it waited
on a shared build lock. Its binaries are excluded from the final comparison. Final
variants use frozen copies of the compiler sources and a dedicated target directory;
all three therefore use identical dependencies despite further workspace edits.

## Protocol and coverage

Apple M4 Max, macOS arm64, Rust 1.99.1. Identical release settings: optimization 3,
fat LTO, one codegen unit, full debug symbols and no stripping. Each process builds
an in-memory declaration Program, checks declarations, links one public barrel and
hashes every output path and byte. The standard library is included.

Inputs vary module count, interface count per module, repeated local names and
maps. Each interface has a nested type parameter. Distinct cases use unique names;
collision cases reuse each interface's local name in every module and re-export
under distinct public names. The 64/128 pairs double module count with the same
32 interfaces per module. Small is 4 × 16; large-few is 4 × 1,200.

For the initial comparison, one warmup pair precedes seven measured pairs in
alternating order. The final comparison interleaves baseline, aggregate and scan
against the same updated compiler dependency, reversing the three-variant order
on alternating rounds; one warmup round precedes seven measured rounds. macOS
`/usr/bin/time -l` supplies retired instructions and maximum RSS; Node's monotonic
clock supplies process wall time including launch and timing overhead. Hyperfine
1.18 is installed, so the hardware-counter-capable Hyperfine 2 workflow is unavailable.
No benchmark-owned build or test runs during measurements. Other desktop activity
can affect wall time; raw min/max and quartiles are saved. Instructions are the
primary signal; RSS and wall time are reported separately.

The linker forces one checker, so requested single/default settings exercise the
same checker configuration. `bench/projects.json` runs compiler checking without
this bundler; these are synthetic bundler results, not catalog-wide measurements.
No external real-world bundler input or private code was used. Maps cases exercise
bundled map generation without preexisting captured declaration maps; fixture byte
comparison separately covers captured-map composition.

## Rejected: scanning modules for every candidate

Seven-pair medians, original baseline versus scan:

| Input | Instructions G before → after | RSS MiB before → after | Wall ms before → after |
| --- | ---: | ---: | ---: |
| small | 0.79 → 0.79 (-0.05%) | 43.45 → 43.44 (-0.04%) | 52.59 → 50.98 (-3.05%) |
| large-few | 6.66 → 6.64 (-0.40%) | 65.58 → 65.31 (-0.41%) | 312.86 → 316.63 (+1.21%) |
| many-64 | 1.85 → 1.85 (+0.08%) | 53.44 → 53.50 (+0.12%) | 169.37 → 165.17 (-2.48%) |
| many-128 | 4.41 → 4.44 (+0.57%) | 63.58 → 63.19 (-0.61%) | 218.16 → 230.01 (+5.43%) |
| collision-64 | 2.00 → 2.37 (+18.40%) | 53.20 → 53.08 (-0.23%) | 112.92 → 127.72 (+13.11%) |
| collision-128 | 5.07 → 8.11 (+60.19%) | 62.78 → 62.92 (+0.22%) | 246.25 → 368.89 (+49.81%) |
| maps-64 | 4.93 → 5.30 (+7.45%) | 53.78 → 53.73 (-0.09%) | 230.58 → 244.14 (+5.88%) |
| maps-128 | 15.86 → 18.93 (+19.35%) | 64.31 → 64.38 (+0.10%) | 695.55 → 823.29 (+18.36%) |

All output hashes match. The collision regression is far outside instruction-count
variation (instruction interquartile ranges below 0.1% for those four baseline/scan
cells). At 128 modules every no-map measured pair regressed by 60.12–60.41% in
instructions and 40.27–66.07% in wall time; every maps pair also regressed in both.
Small wall differences in other rows are inconclusive: for example, the small
case's ratio of medians is -3.05%, while its median paired change is +0.40%. Doubling modules increases collision-case instructions 2.53× in the baseline
and 3.42× in scan. Existing generated-name searching also scales superlinearly; these
whole-link figures do not establish a formal complexity bound for either version.
The printer checks source reservations before its already-generated-name set, so
failed candidates repeat module scans too. Reject this variant. Revisit only with a
query structure that avoids repeatedly traversing the module list, or evidence that
the target workload cannot trigger that cost.

## Aggregate finalist

Frozen-source seven-round comparison is complete. All eight input cases produce
matching output hashes across baseline, aggregate and scan (168 measured rows).
Baseline versus aggregate medians:

| Input | Instructions G before → after | Peak RSS MiB before → after |
| --- | ---: | ---: |
| small | 0.80 → 0.79 (-0.14%) | 43.58 → 43.56 (-0.04%) |
| large-few | 6.69 → 6.67 (-0.24%) | 65.97 → 66.80 (+1.26%) |
| many-64 | 1.86 → 1.85 (-0.41%) | 53.25 → 53.78 (+1.00%) |
| many-128 | 4.44 → 4.42 (-0.36%) | 63.19 → 63.70 (+0.82%) |
| collision-64 | 2.02 → 1.98 (-1.53%) | 52.98 → 53.47 (+0.91%) |
| collision-128 | 5.10 → 4.98 (-2.21%) | 62.78 → 63.80 (+1.62%) |
| maps-64 | 4.95 → 4.92 (-0.59%) | 53.70 → 54.20 (+0.93%) |
| maps-128 | 15.94 → 15.84 (-0.65%) | 64.27 → 65.11 (+1.31%) |

The aggregate removes the scan regression and uses fewer median instructions than
the original collector in all eight cases. It costs about 0.5–1 MiB additional peak
RSS on the larger cases because it retains both compiler per-file caches and the
owned union. This is a measured CPU/memory tradeoff, not a memory improvement.
Collision-128 paired instruction changes range -3.02% to -1.47%, consistently below
baseline. Small instruction changes are less decisive.

Wall time is unsuitable for speed claims in this run: baseline medians ranged from
0.47 s (small) to 8.75 s (maps-128), far above the earlier low-contention run, and
paired differences change sign widely. Raw wall metrics and dispersion are retained
in `results-final.json`; no wall-time speedup is established. The dedicated build
snapshot controls code differences but cannot eliminate other machine activity.

Correctness and checks on the aggregate implementation:

- 261 bundler tests pass across six suites.
- All 139 captured fixture output paths and contents match the original baseline
  byte for byte, including declaration maps.
- Workspace Clippy with warnings denied, source checks, formatting, and the dev
  CLI/testrunner build pass.
- Focused pinned-reference collision baselines pass for
  `importHelpersWithOriginalSourceFileCollision` and the CommonJS/ES2015 variants
  of `importHelpersWithLocalCollisions`. The System-module variant remains skipped.
  No type/symbol/map baselines are present for these selected cases.
- Scoped static review found no concrete lifetime, ownership or output-order issue.
  The cache retains owner-region initialization; copied spellings and the moved
  callback set are owned. Hash iteration only populates a membership set.
- Full compiler conformance was not rerun. The shared collector and reservation
  policy are unchanged; the new accessor exposes the existing lazy cache.

These checks ran before subsequent concurrent compiler/printer/bundler edits in
this checkout. They establish the tested aggregate snapshot, not validation of
those later unrelated changes.

## Reproduction

The ignored scratch directory retains the complete harness. Initial setup copies the
current bundler source, so preserve the saved variant sources when replaying an older
experiment. `freeze.mjs` freezes compiler dependencies in a new directory and builds
saved baseline/scan/aggregate bundler snapshots with a dedicated target directory.
It refuses to overwrite an existing frozen snapshot. The resulting binaries are
`baseline-shared`, `candidate-shared` and `aggregate`.

```sh
node target/name-reservation-bench/freeze.mjs
```

```sh
VARIANTS=baseline-shared,aggregate,candidate-shared TAG=final node target/name-reservation-bench/measure.mjs
```

Raw rows and min/max/quartile summaries are written to `results-<TAG>.json`.
These scratch files are local evidence, not a published benchmark interface.
