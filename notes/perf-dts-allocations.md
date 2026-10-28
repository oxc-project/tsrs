# Declaration bundler allocation reductions (2026-10-26)

Baseline: `3fca398fb46a16e2f231041522bf2f70c7ad8001`.
Candidate: the local `crates/tsrs_dts` allocation changes described below.

## Changes

- Move the reserved identifier set and three visitor lookup tables out of the
  completed graph into the output callbacks. The graph is local to one link and
  those fields have no later reader; the compiler Program still owns its nodes
  throughout printing. No compiler handles enter returned outputs or caches.
- Borrow string sort keys when assigning multi-entry shared export names. The
  comparator preserves the previous tuple ordering and stable sort.
- Fill the graph work queue directly, preserving order while removing intermediate
  vectors and the small vector allocated for every top-level declaration.
- Keep one append-only source-name table for all printer fragments. Each generator
  is seeded with all previous names, preserving indices; fragments retain only
  mapping text rather than a source-name vector and a complete raw map. Append
  newly discovered names once instead of replacing the table on every statement.
- Borrow original source names during map composition and directive deduplication.
  Do not clone/decode captured maps when output maps are disabled.
- Skip the duplicate-path tree on case-sensitive capture validation: exact keys
  are already unique in the capture's BTreeMap. Case-insensitive validation still
  uses the same Unicode lowercasing and collision errors.
- Borrow VFS prefix range bounds and allocate a virtual child directory name once
  per distinct child, retaining final sorting/deduplication with base-FS entries.

Owned declaration text in the compiler filesystem and persistent cache snapshots
remain owned. The compiler's FS trait returns owned strings, and visitor/printer
callbacks require owned captures. Necessary AST construction vectors and cheap
NodeFactory/Rc handle clones remain.

## Measurements

macOS arm64; identical release configuration for both binaries: opt-level 3,
fat LTO, one codegen unit, debug symbols enabled and stripping disabled.
`/usr/bin/time -l` supplies retired instructions and peak RSS. A Time Profiler
recording of the baseline also contains graph construction, cloned BTreeMap
subtrees and the allocating multi-entry sort comparator.

The synthetic linking workload contains four captured declaration modules, each
with 1,200 interfaces and 1,200 exported const declarations in descending name
order (9,600 retained declarations total), and the bundled standard library.
Single-entry uses one barrel; multi-entry uses four facades plus a shared bundle;
maps uses the same multi-entry input with declarationMap enabled. Linking checks
all declarations. Each process links once and hashes every output path and byte.
Hashes match baseline/candidate in every measured run.

The bundler forces one checker in `project::create`; requested single/default
settings therefore exercise the same checker configuration, not parallel scaling.
The normal benchmark catalog runs compiler checking and does not invoke this
bundler, so these figures are bundler workload measurements, not catalog results.

Five measured alternating baseline/candidate pairs after one warmup pair, medians
from the second run after build activity had finished:

| Workload / requested mode | Instructions before → after | Peak RSS MiB before → after | Wall ms before → after |
| --- | ---: | ---: | ---: |
| Single entry / single | 20,992,427,193 → 20,986,152,371 (-0.03%) | 74.70 → 72.80 (-2.55%) | 1051.15 → 1051.86 (+0.07%) |
| Single entry / default | 20,989,347,394 → 20,986,301,061 (-0.01%) | 74.59 → 72.77 (-2.45%) | 1048.32 → 1044.94 (-0.32%) |
| Multiple entries / single | 1,648,981,916 → 1,606,170,485 (-2.60%) | 75.94 → 75.23 (-0.93%) | 101.30 → 100.98 (-0.31%) |
| Multiple entries / default | 1,650,025,640 → 1,605,882,603 (-2.68%) | 75.92 → 75.31 (-0.80%) | 101.18 → 99.85 (-1.31%) |
| Maps / single | 4,239,060,715 → 4,082,868,564 (-3.68%) | 88.31 → 78.39 (-11.23%) | 214.77 → 206.51 (-3.85%) |
| Maps / default | 4,244,280,285 → 4,076,063,993 (-3.96%) | 88.33 → 77.73 (-11.99%) | 214.32 → 204.79 (-4.45%) |

An earlier five-pair run overlapping other work found multi-entry instructions
-2.80%/-2.83% and maps -3.69%/-4.21%, with maps RSS -13.58%/-13.28%.
Single-entry wall time ranged from +3.36% to -2.58% in that run, so no single-entry
speedup is established. RSS and wall time vary with process/system conditions;
these are grouped changes and do not attribute individual savings to each edit.

Local reproduction/evidence is retained under `target/scratch/dts-alloc/`:
`Cargo.toml`, `src/main.rs`, baseline/candidate binaries, `measure.mjs`,
`measurements.json`, and `baseline-profile.trace`. Build both revisions with:

```sh
cargo build --release --manifest-path target/scratch/dts-alloc/Cargo.toml --offline --target-dir target/dts-alloc-release
```

Save each resulting probe binary as `baseline` or `candidate` in that directory,
then run `node target/scratch/dts-alloc/measure.mjs`. These local artifacts are
ignored, not a new public benchmark API.

## Correctness and checks

Before and after: 258 bundler tests pass across six suites. All 139 captured
fixture output paths and contents match byte for byte. Workspace Clippy with
warnings denied, formatting, source checks and the dev CLI/runner build pass.
No compiler algorithm or public bundler API changed. Full compiler conformance
and measurements on external real-world bundler inputs were not run.
