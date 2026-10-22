# Parallel program construction for the lint helper (2026-10-15)

Follow-up to the [direct-helper bottleneck investigation](perf-tsgolint-bottlenecks.md).

The helper forced `ProgramOptions::single_threaded = true`, bypassing the compiler's existing parallel
root lookup, parse/bind and module-resolution prefetch path. It now leaves that option at its default,
so program construction uses the native compiler pool and honors configuration `singleThreaded`.
The default compiler pool uses available parallelism up to 32 threads; `RAYON_NUM_THREADS` overrides it.

One `ProgramRegions` set per request owns loader/binder allocations on every compiler worker. It is retained
until all request programs have been freed, alongside the existing lint-worker regions. Sharing the set
across programs avoids a new arena tail per worker per tsconfig. Deferred binding also enters these regions;
a regression test verifies its symbol table is allocated there in both serial and parallel modes.
The native checker pool remains at one checker, preserving the serial fallback history; lint-worker counts are unchanged.
The existing all-rule concurrency test now compares explicit serial construction against parallel construction
at 1/2/4/8/12 workers, with lint/full diagnostics and all fix combinations.

This reuses the compiler's existing pool, loader and binder.
The prior serial-step experiments rejected millisecond gains from new overlap/threads; this is the independently
profiled multi-second serial loader/binder in the newly added lint helper. No rejected cost model is retried.

## Direct-helper release measurements

Same public VSCode commit, 9,324 paths and all 60 rules as the [previous comparison](bench-tsgolint-vscode-rebased.md).
Apple M2 Max, 12 logical CPUs, 96 GiB RAM. Optimized release, fat LTO, one codegen unit, no PGO/BOLT.
Two consecutive publishes, each with one warm-up and three alternating measured samples per cell.
Thread overrides are unset at default. Each process starts fresh with warm filesystem caches.
**Oxlint and its final JSON formatter are excluded.** Values are independent medians.

| Publish | Workload | Rust before | Rust after | Go | Rust reduction | Go / Rust after |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | lint | 4.237 s | 2.869 s | 10.795 s | 32.3% | 3.76× |
| 1 | lint-typecheck | 5.542 s | 4.212 s | 15.810 s | 24.0% | 3.75× |
| 1 | setup | 1.741 s | 0.378 s | 1.057 s | 78.3% | 2.80× |
| 2 | lint | 4.325 s | 2.921 s | 10.674 s | 32.5% | 3.65× |
| 2 | lint-typecheck | 5.531 s | 4.216 s | 15.515 s | 23.8% | 3.68× |
| 2 | setup | 1.836 s | 0.376 s | 1.049 s | 79.5% | 2.79× |

`lint` resolves types on demand for rules. `lint-typecheck` also requests full syntactic/semantic diagnostics.
`setup` removes all rules and compiler diagnostics; it includes discovery, construction, binding and teardown,
rather than being an isolated compiler phase timer. Earlier profiling attributed about 2.45 seconds to serial
loading/binding, but those observations must not be subtracted from these separate-run medians.

| Publish | Workload | Helper | Instructions | Peak RSS | Wall range |
| --- | --- | --- | ---: | ---: | ---: |
| 1 | lint | after | 163.409 G | 2.758 GiB | 2.756–3.043 s |
| 1 | lint | before | 163.155 G | 2.738 GiB | 4.170–4.288 s |
| 1 | lint | go | 481.293 G | 7.842 GiB | 10.764–10.833 s |
| 1 | lint-typecheck | after | 230.421 G | 3.869 GiB | 4.210–4.233 s |
| 1 | lint-typecheck | before | 230.203 G | 3.852 GiB | 5.527–5.586 s |
| 1 | lint-typecheck | go | 643.224 G | 11.558 GiB | 15.449–15.956 s |
| 1 | setup | after | 21.164 G | 1.127 GiB | 0.367–0.440 s |
| 1 | setup | before | 20.982 G | 1.082 GiB | 1.722–1.889 s |
| 1 | setup | go | 69.684 G | 2.861 GiB | 0.987–1.062 s |
| 2 | lint | after | 163.355 G | 2.767 GiB | 2.861–2.968 s |
| 2 | lint | before | 163.163 G | 2.747 GiB | 4.160–4.337 s |
| 2 | lint | go | 471.974 G | 7.949 GiB | 10.499–10.745 s |
| 2 | lint-typecheck | after | 230.497 G | 3.866 GiB | 4.187–4.375 s |
| 2 | lint-typecheck | before | 230.203 G | 3.845 GiB | 5.489–5.535 s |
| 2 | lint-typecheck | go | 611.364 G | 11.565 GiB | 14.982–15.942 s |
| 2 | setup | after | 21.174 G | 1.125 GiB | 0.370–0.376 s |
| 2 | setup | before | 21.855 G | 1.090 GiB | 1.738–1.890 s |
| 2 | setup | go | 69.546 G | 2.866 GiB | 1.047–1.080 s |

Instructions are local macOS hardware counters, not deterministic Linux instruction measurements.
RSS covers the complete helper process and all its threads. The tables retain allocation/parallelism costs;
no memory reduction is assumed from the concurrency change.

## One-worker controls

One observation per cell, `GOMAXPROCS=1 RAYON_NUM_THREADS=1`. This limits concurrency;
the new helper still uses the parallel-loader algorithm on its one-worker pool. It does not force the
configuration `singleThreaded` flag. The integration test separately exercises that flag.

| Workload | Helper | Wall | Instructions | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| lint | after | 14.469 s | 153.784 G | 2.106 GiB |
| lint | before | 14.351 s | 152.553 G | 2.143 GiB |
| lint-typecheck | after | 22.090 s | 211.653 G | 2.842 GiB |
| lint-typecheck | before | 20.708 s | 211.394 G | 2.847 GiB |

The one-worker full-checking observation is slower (20.708 → 22.090 s), while its instruction count
increases by about 0.12%. These controls were not repeated and do not establish a steady-state wall-time effect.
The repeated performance claim above applies to default-thread runs.

## Correctness and reproduction

Every before/after Rust diagnostic multiset matches across both publishes, warm-ups and controls,
and matches the prior release exactly. The separate pre-benchmark correctness run also preserved both
VSCode multisets. Go retains existing version/history differences; equal workloads do not imply Rust/Go
diagnostic parity. Per-rule counts and complete hashes are retained in the raw report.

- All 7,169 active imported rule baselines pass; 36 upstream skips remain.
- Compiler unit tests (24), lint unit tests (3), headless (100), lifecycle (2), protocol (8): passed.
- Compiler/conformance: 13,458 passes, the documented two codes/two fail cases, no crashes/timeouts.
- Workspace all-target Clippy with warnings denied, `tools/lint/source.py`, and all eight generated-code checks: passed.
- Existing native pool bounds and threadless behavior are retained; worker scheduling is unchanged.

[Raw samples, counters, binary/source hashes, diagnostic counts and exact benchmark wrapper](../bench/results/2026-10-15-tsgolint-parallel-load.json).
Runner: `bench/tsgolint.py`, direct `headless` invocation. Build with
`cargo build --release --locked -p tsrs_lint --bin tsrs-lint`; benchmark before/after/Go using the
same captured V2 payload and corpus cwd, `--modes default --variants lint lint-typecheck setup --runs 3`.
Run twice with separate output directories and `--warmups 1`. No builds/tests overlap these measurements.
Local raw outputs: `/private/tmp/tsrs-lint-parallel-load-final-{1,2,controls}`.
