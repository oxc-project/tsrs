# Rebased Rust versus Go lint benchmark on VS Code (2026-10-15)

For phase attribution and the next optimization targets, see the [bottleneck investigation](perf-tsgolint-bottlenecks.md).

Rust `1bc46d55`, rebased onto main `7cccff0b`. All 60 rules,
9,324 public VS Code source paths, default options, no fixes. The same Oxlint 1.88.0 release frontend
uses either the new Rust release helper or installed `oxlint-tsgolint` 7.0.2004 (Go 1.26.8).
VS Code commit, dependencies, payload values, rule configuration and Go binary are unchanged
from the [historical comparison](bench-tsgolint-vscode.md#corpus-and-binaries). The frontend was rebuilt
from the same Oxlint commit `9a424c209e` after its original target directory was removed during the
helper measurements. Every frontend sample uses one immutable copy of that new release build.

Apple M2 Max, 12 logical CPUs, 96 GiB RAM, macOS 27.0.1. Rust is opt-level 3, fat LTO, one codegen unit,
without PGO/BOLT. Thread overrides are unset: Rust defaults to available parallelism (12 here); Go uses
its normal default. One warm-up per helper/variant, then alternating fresh-process measurements.
No benchmark builds or tests overlap timings. Filesystem caches are warm; compiler state starts fresh.
Wall times are slower than the previous session: compare the interleaved implementations within
this run, rather than mixing absolute times across publishes. Values are independent medians.
Helper and frontend series ran separately; their times are not additive phase measurements.

## Full Oxlint commands

| Workload | Rust helper | Go helper | Go / Rust |
| --- | ---: | ---: | ---: |
| lint | 12.355 s | 20.214 s | 1.64× |
| lint-typecheck | 14.424 s | 27.242 s | 1.89× |

Medians of 5 measured runs per cell. `lint-typecheck` adds `--type-check`; `lint` still
resolves types on demand for lint rules. Commands retain intentional invalid JavaScript parser fixtures
and exit with status 1. The helpers themselves exit successfully with no protocol error records.

## Complete helper lifecycle

| Workload | Rust | Go | Go / Rust | Rust peak RSS | Go peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| lint | 9.551 s | 22.085 s | 2.31× | 2.695 GiB | 7.835 GiB |
| lint-typecheck | 10.648 s | 25.452 s | 2.39× | 3.776 GiB | 11.531 GiB |

Medians of 5 measured runs per cell. Each helper receives the identical captured V2 request.
Helper RSS includes all its threads. End-to-end RSS is excluded from this comparison because
`time -l` frontend accounting is not a simultaneous process-tree peak.

| Path | Workload | Helper | Instructions | Wall range |
| --- | --- | --- | ---: | ---: |
| helper | lint | rust | 165.670 G | 8.135–11.350 s |
| helper | lint | go | 480.570 G | 18.702–23.775 s |
| helper | lint-typecheck | rust | 230.936 G | 8.430–11.114 s |
| helper | lint-typecheck | go | 611.693 G | 22.095–30.051 s |
| frontend | lint | rust | 60.331 G | 12.337–12.385 s |
| frontend | lint | go | 60.088 G | 20.031–21.214 s |
| frontend | lint-typecheck | rust | 60.777 G | 14.065–15.370 s |
| frontend | lint-typecheck | go | 61.131 G | 26.186–28.120 s |

Instructions are local macOS hardware counters, not deterministic Linux instruction measurements.
Frontend resource counters use OS frontend accounting; use isolated-helper counters to compare helper work.

## Diagnostics and reproduction

Every Rust diagnostic multiset matches across all its repetitions, including warm-ups. Headless Rust
hashes also match the pre-rebase release exactly for both variants. Rust and installed Go retain
known rule/compiler-version differences; equal workloads do not imply diagnostic parity.
Go diagnostic counts and hashes also vary between repetitions, affecting several rules when full
checking is enabled. Every observed count and per-rule breakdown is retained in the raw report.

| Path | Workload | Rust diagnostics | Go diagnostics |
| --- | --- | ---: | ---: |
| helper | lint | 275891 | 275832/275834 |
| helper | lint-typecheck | 276925 | 276867/276869/276876/276883/276885/276917 |
| frontend | lint | 275906 | 275847/275849 |
| frontend | lint-typecheck | 276940 | 276882/276893/276900/276901/276912/276933 |

Relative to the Rust helper, the frontend adds 16 parser errors and suppresses one `prefer-includes`
diagnostic through an existing ESLint comment, giving a net increase of 15 diagnostics.

Capture the payload/config using the [original instructions](bench-tsgolint-vscode.md#reproduction), then:

```sh
cargo build --release --locked -p tsrs_lint --bin tsrs-lint
python3 bench/tsgolint.py --payload /path/to/payload.json --cwd /path/to/vscode \
  --helper rust="$PWD/target/release/tsrs-lint" --helper go=/path/to/tsgolint \
  --modes default --runs 5 --output /private/tmp/helpers
python3 bench/tsgolint_frontend.py --frontend /path/to/release/oxlint \
  --config /path/to/all60.json --cwd /path/to/vscode \
  --helper rust="$PWD/target/release/tsrs-lint" --helper go=/path/to/tsgolint \
  --runs 5 --output /private/tmp/frontends
```

[Raw samples, counters, binary/source hashes and diagnostic counts](../bench/results/2026-10-15-tsgolint-vscode-rebased.json).
The first typecheck warm-up returned an empty Go timing file; no measured typecheck sample was lost.
The variant was restarted after old benchmark outputs were losslessly compressed to relieve disk pressure.
Complete local outputs remain under `/private/tmp/tsrs-vscode-lint-rebased-{helper,typecheck,frontend}/`.
