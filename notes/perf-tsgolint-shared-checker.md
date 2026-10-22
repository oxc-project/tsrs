# Shared semantic and lint checker (2026-10-17)

The user requested removing semantic/lint checker isolation and explicitly accepted
history-dependent diagnostics. The prior isolation avoided the shared compiler's
context-sensitive signature-cache behavior at the cost of rebuilding checker state.

Serial execution now reuses the program checker. Parallel execution retains one
lazy checker per worker for both semantic reporting and linting. The extra serial
lint allocation region is removed; request and worker regions retain their existing
ownership and destruction order. Lint diagnostics and fixes can change with semantic
reporting, file order and worker assignment. The compiler cache algorithm is unchanged.

This revisits the earlier isolation decision with a changed requirement: diagnostic
independence is no longer required. It adds no checker, cache or scheduling mechanism.

## Release measurements

Before: clean branch HEAD `3336318e`. After: the shared-checker engine change, same
compiler, rules and build configuration. Both binaries were rebuilt with
`cargo build --release --locked -p tsrs_lint --bin tsrs-lint` (fat LTO).
Public VSCode `b029fb142d7a1c7a656d8be926525d4e44c6d635`, captured V2 payload,
9,324 paths and all 60 rules. Full syntactic and semantic reporting enabled.

One warm-up plus three alternating samples per helper and mode on the same macOS
host. Single mode sets `RAYON_NUM_THREADS=1 GOMAXPROCS=1`; default mode leaves both
unset. The fixture runner was paused during measurement and resumed afterward.
Instruction counts and peak RSS come from `/usr/bin/time -l`; elapsed time is the
benchmark runner's wall clock. Values below are medians of the three measured samples.

| Workers | Wall seconds | Instructions, billions | Peak RSS, MiB |
| --- | --- | --- | --- |
| default | 5.937 → 4.909 | 220.63 → 166.57 (-24.5%) | 3938.4 → 2853.8 (-27.5%) |
| single | 32.992 → 24.007 | 201.89 → 156.42 (-22.5%) | 2910.7 → 2181.1 (-25.1%) |

Default peak RSS falls more than 5%, clearing the repository's memory threshold.
The wall-time figures are one publish, rather than the two required for a wall-time-only claim.

The accepted diagnostic change is visible in every sample: 276,868 records before,
276,953 after. Compiler records remain 1,034. Per-rule count deltas are:
`no-unnecessary-type-assertion` +188, `non-nullable-type-assertion-style` +91,
`no-unsafe-type-assertion` -193, and `no-unsafe-return` -1. Full record digests and
raw frames are retained; the comparison does not normalize these differences away.

Artifacts: `/private/tmp/tsrs-shared-checker-20261017/bench`, including source/helper
hashes in `metadata.json`, samples, counters, frames and `summary.json`.

```sh
python3 bench/tsgolint.py --payload /path/to/captured-v2-payload.json \
  --cwd /path/to/vscode --helper before=/path/to/before --helper after=/path/to/after \
  --output /private/tmp/shared-checker-bench --variants lint-typecheck --runs 3
```

## Validation

The headless regression covers the accepted semantic warm-up effect in both the
single-worker and two-worker paths, with all four fix modes. It preserves explicit
range, fix and compiler-error assertions. The all-rule independent-module fixture
retains its bounded serial/parallel equivalence check. Documentation no longer
promises diagnostic independence across semantic-reporting or thread configurations.

Required checks: `cargo test --locked -p tsrs_lint`, the dev CLI/testrunner build,
workspace Clippy with warnings denied, and `tools/lint/source.py`.
