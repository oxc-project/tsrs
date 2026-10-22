# Type-aware lint workers and type-check variants (2026-10-15)

For the subsequent explicit-count sweep, see [4/8/12 worker scaling](perf-tsgolint-worker-scaling.md).

The remaining gap came from concurrency: the Rust helper forced serial construction, binding,
compiler diagnostics and rule evaluation, while the Go helper already ran checker workers.
Both Rust snapshots below are optimized **release** builds. Fast-build screening timings are excluded.

## Changes

- Run file groups on up to four scoped workers; use positive `RAYON_NUM_THREADS` or available parallelism.
- Give each worker separate semantic and lint checkers, preserving the previous lint cache behavior.
- Serialize each group on its worker, then write groups in file order. Errors and panics join all workers.
- Keep unregistered request and worker regions alive until every request program has been released.
- Use the existing mimalloc v3 allocator with explicit heap `no_thp` and the CLI’s Linux arena-THP restoration.
- Skip checker acquisition and AST traversal when a file has no lint rules after requested compiler diagnostics.

These reuse techniques already documented in `docs/RUST.md`, `notes/mem-emit-regions.md`,
`notes/mem-no-thp.md` and `notes/perf-mimalloc-safe.md`. They were missing from this new helper;
no previously rejected checker algorithm was retried. Program construction and binding remain serial.

## Repeated default measurements

Apple M2 Max (12 logical CPUs, 96 GiB RAM), macOS 27.0.1, Rust 1.99.0. Same public VS Code
commit, 9,324 input paths, all 60 rules, options and installed Go release binary as the
[previous report](perf-tsgolint-terminal-references.md). The before helper is exactly its release binary
(SHA-256 verified). Rust uses fat LTO and one codegen unit, without PGO/BOLT. Go build information
shows its normal optimized compiler and no disabling `-gcflags`.

Each publish has three alternating repetitions per helper/variant. The first also has one warm-up;
the second reuses the warm filesystem cache. Every process starts with fresh compiler state.
No builds or tests overlap these measurements. Values are independent medians.
Absolute wall times shift between the two publishes; compare interleaved helpers within each
publish. The performance claim uses the improvement reproduced in both, not a cross-publish ratio.

| Publish | Workload | Rust before | Rust after | Go | Rust improvement |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | lint | 22.398 s | 9.214 s | 14.060 s | 58.9% |
| 1 | lint-typecheck | 33.053 s | 13.045 s | 20.907 s | 60.5% |
| 2 | lint | 15.246 s | 6.785 s | 11.005 s | 55.5% |
| 2 | lint-typecheck | 22.043 s | 9.589 s | 16.014 s | 56.5% |

`lint` resolves types on demand for the rules, with full compiler diagnostics disabled.
`lint-typecheck` additionally requests syntactic and semantic diagnostics (`--type-check` in Oxlint).

Instructions and peak RSS for publish 1:

| Workload | Helper | Instructions | Peak RSS |
| --- | --- | ---: | ---: |
| lint | before | 164.327 G | 2.110 GiB |
| lint | after | 158.608 G | 2.418 GiB |
| lint | go | 483.802 G | 7.848 GiB |
| lint-typecheck | before | 226.029 G | 2.862 GiB |
| lint-typecheck | after | 220.946 G | 3.332 GiB |
| lint-typecheck | go | 632.800 G | 10.980 GiB |

Parallel workers duplicate some checker state and increase Rust peak RSS; the table reports that cost.
The new threads clear the wall-time bar by a wide margin on both local publishes, recorded separately
in [`bench/RESULTS.md`](../bench/RESULTS.md). These macOS instruction counters are local measurements,
not the deterministic Linux `bench/count.py` gate. No Linux performance result is claimed.

## Concurrency and phase controls

The following are single observations per cell, not repeated medians. Single mode sets
`GOMAXPROCS=1 RAYON_NUM_THREADS=1`; Go can retain multiple checker states, so it is a concurrency
control rather than an assertion of identical cache layouts. `typecheck` removes every lint rule;
`setup` also disables full compiler diagnostics.

| Mode | Workload | Rust before | Rust after | Go |
| --- | --- | ---: | ---: | ---: |
| single | lint | 16.320 s | 14.746 s | 42.878 s |
| single | lint-typecheck | 21.781 s | 21.014 s | 63.451 s |
| default | typecheck | 9.525 s | 4.411 s | 7.360 s |
| default | setup | 2.189 s | 1.742 s | 1.027 s |

The empty-rule early exit reduces the setup/typecheck controls as well as avoiding unnecessary
lint work in real mixed configurations. These controls include loading, binding, serialization and teardown;
they are not pure isolated compiler phase timers.

## Full Oxlint observations

One optimized frontend/helper invocation each (single observations, not medians):

| Workload | Rust helper | Go helper |
| --- | ---: | ---: |
| lint | 11.202 s | 15.405 s |
| lint-typecheck | 13.963 s | 20.745 s |

Both retain the intentionally invalid JavaScript parser fixtures. The typed variant adds
`--type-check` to the identical all-rule `--type-aware` frontend invocation.

## Correctness and validation

Every before/after Rust diagnostic multiset is identical within its variant, including both publishes,
warm-ups and single controls: no diagnostic was removed, added or changed. The separate instrumented
run also matches the lint diagnostic multiset and preserves all 60 listener call counts.

This does not establish full Rust/installed-Go diagnostic parity. The existing version/history differences
are retained ([historical comparison](bench-tsgolint-vscode.md#diagnostic-differences)); enabling full
checking also exposes compiler-version differences. Per-rule counts and complete multiset hashes are
in the raw artifacts. Go output can vary with scheduling, including the previously observed two-count
variation in `no-unsafe-type-assertion`.

- 7,169 imported cases pass, with 36 upstream skips, on the frozen worker candidate.
- Final checks: 100 headless, two lifecycle, two unit and eight protocol tests pass.
- The new test compares all 60 rules at 1/2/4 workers, full checking off/on, in all four fix modes.
- Repeated multi-file library calls and writer failures pass, including arena-poison lifetime checks.
- Oxlint Rust suites and 642 JavaScript tests pass (one upstream JS skip).
- Required workspace Clippy and source checks pass.
- Three review workers audited semantics, lifetimes, runtime and the runner. Two resource findings
  (an unused Rayon pool and implicit Linux allocator features) were repaired and independently rechecked.

## Reproduce

Capture the same V2 frontend payload as described in the [historical report](bench-tsgolint-vscode.md#reproduction).
Build and retain immutable release helpers, then use the committed runner:

```sh
cargo build --release -p tsrs_lint --bin tsrs-lint
python3 bench/tsgolint.py --payload /path/to/payload.json --cwd /path/to/vscode \
  --helper before=/path/to/before-release --helper after=/path/to/after-release \
  --helper go=/path/to/go-release --output /private/tmp/lint-publish-1 --modes default
# Repeat into another new directory for a second publish.
# Add --modes single for concurrency controls; --variants typecheck setup for attribution.
```

Raw artifacts: [publish 1](../bench/results/2026-10-15-tsgolint-vscode-workers-1.json),
[publish 2](../bench/results/2026-10-15-tsgolint-vscode-workers-2.json),
[controls, frontend observations and callback counts](../bench/results/2026-10-15-tsgolint-vscode-workers-controls.json).
Full local frames, runner metadata and `time -l` counters remain under
`/private/tmp/tsrs-vscode-lint-gap/`.
