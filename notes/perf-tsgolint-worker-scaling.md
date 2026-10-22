# Rust lint worker scaling: 4 / 8 / 12 (2026-10-15)

All 60 rules on the same 9,324-path public VS Code payload as the [previous comparison](perf-tsgolint-workers.md).
Apple M2 Max, 12 logical CPUs, 96 GiB RAM, macOS 27.0.1. One identical optimized **release** binary
for every setting: opt-level 3, fat LTO, one codegen unit, no PGO/BOLT. No builds or tests overlap timed runs.

One warm-up per count/variant, followed by five measured rounds. Worker-count order rotates each round:
4/8/12, 8/12/4, 12/4/8, then repeats. Every process starts with fresh compiler state and a warm filesystem cache.
Values below are independent medians. The limits apply per program; small groups can use fewer workers.

| Worker limit | Full checking off | Peak RSS | Full checking on | Peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 9.141 s | 2.419 GiB | 12.972 s | 3.329 GiB |
| 8 | 6.508 s | 2.637 GiB | 8.598 s | 3.644 GiB |
| 12 | 6.093 s | 2.737 GiB | 7.830 s | 3.847 GiB |

`Full checking off` still resolves types on demand for the rules. `On` additionally collects syntactic
and semantic compiler diagnostics, equivalent to adding `--type-check` to type-aware linting.

| Workload | Worker limit | Instructions | Wall range | Speedup over 4 |
| --- | ---: | ---: | ---: | ---: |
| lint | 4 | 158.368 G | 9.083–9.170 s | 1.00× |
| lint | 8 | 161.603 G | 6.466–6.543 s | 1.40× |
| lint | 12 | 163.568 G | 5.916–6.198 s | 1.50× |
| lint-typecheck | 4 | 220.672 G | 12.559–13.076 s | 1.00× |
| lint-typecheck | 8 | 226.863 G | 8.524–9.394 s | 1.51× |
| lint-typecheck | 12 | 230.880 G | 7.750–8.307 s | 1.66× |

More workers duplicate checker state and type queries; the instruction and RSS columns expose that cost.
These are local macOS instruction counters, not deterministic Linux instruction measurements.
The compiler-scaling notes (`notes/perf-checker-scaling.md`, `notes/perf-round2-followups.md`) already
describe this tradeoff. This sweep measures the newly ported lint workload and its separate semantic/lint checkers.

## Configuration and correctness

The old helper silently capped positive `RAYON_NUM_THREADS` settings at four. Explicit settings now
override that default, capped at 256 and the active file count. This first sweep retained the four-worker
default. The follow-up below changes the automatic default to available threads. Threadless targets retain serial execution.
The runner’s new `--threads` option interleaves counts and sets `RAYON_NUM_THREADS` and `GOMAXPROCS`.
`--threads` and `--modes` are mutually exclusive.

All 36 diagnostic multiset hashes match within each variant, including warm-ups. They also match the
previous four-worker release: no added, removed or changed Rust diagnostic records at 4, 8 or 12 workers.
100 headless, three unit, two lifecycle and eight protocol tests pass. The expanded parity test uses
24 source files, so 8 and 12 both create the requested number of chunks; all 60 rules, full checking off/on,
and every fix mode match serial output. Required workspace Clippy and source checks pass.

## Reproduce

Capture the V2 payload using the [existing instructions](bench-tsgolint-vscode.md#reproduction), then:

```sh
cargo build --release -p tsrs_lint --bin tsrs-lint
python3 bench/tsgolint.py --payload /path/to/payload.json --cwd /path/to/vscode \
  --helper rust="$PWD/target/release/tsrs-lint" --threads 4 8 12 --runs 5 \
  --output /private/tmp/lint-scaling
```

[Raw samples, hashes and counters](../bench/results/2026-10-15-tsgolint-vscode-worker-scaling.json).
Complete local framed outputs and `time -l` counters remain in `/private/tmp/tsrs-vscode-lint-scaling/`.

## Available-thread default follow-up

The automatic default now uses `available_parallelism()` (12 on this machine), with the same
256-worker and active-file caps. Positive `RAYON_NUM_THREADS` still overrides it; zero or malformed
values use the automatic default. Threadless targets remain serial.

A second release comparison alternates the previous four-worker binary and the new automatic-default
binary, with thread overrides unset. Same machine, corpus and payload as above; one warm-up and five
measured runs per helper and variant, with no builds or tests overlapping timings.

| Workload | Default | Wall | Instructions | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| lint | four | 9.463 s | 158.686 G | 2.425 GiB |
| lint | available | 6.151 s | 163.909 G | 2.749 GiB |
| lint-typecheck | four | 13.208 s | 221.092 G | 3.326 GiB |
| lint-typecheck | available | 7.928 s | 231.202 G | 3.849 GiB |

All 24 diagnostic multiset hashes match the first sweep for their variant. Together with the first
publish above, this supplies two same-hardware publishes for the default change; extra workers trade
additional instructions and memory for lower wall time. These remain macOS hardware instruction
counts, rather than deterministic Linux counts.

[Follow-up raw samples and binary/source hashes](../bench/results/2026-10-15-tsgolint-vscode-worker-default.json).
Reproduce with `bench/tsgolint.py --modes default --runs 5`, using separate `--helper` arguments for
the old and new release binaries and the same payload/cwd/output options above.
