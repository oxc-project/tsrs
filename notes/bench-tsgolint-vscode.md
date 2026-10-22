# All-rule lint benchmark on VS Code (2026-10-15)

Historical baseline at `ab2eb669`, before the lint infrastructure performance fixes.

With all 60 rules enabled, the Rust helper takes **14.17×** the Go helper's wall time
at their default settings on this machine. It uses **71.2% less** peak RSS.
This measures the complete helper lifecycle, including project discovery, parsing, binding,
type queries, rule execution, and diagnostic serialization.

## Corpus and binaries

- VS Code `b029fb142d7a1c7a656d8be926525d4e44c6d635`, existing installed dependencies,
  `../../microsoft/vscode/src`; tracked source changes were absent.
- Oxlint `9a424c209e`, version 1.88.0, `cargo build --release --locked -p oxlint`.
- Rust `ab2eb669`, `cargo build --release -p tsrs_lint --bin tsrs-lint`;
  Rust 1.99.0, fat LTO, one codegen unit, no PGO/BOLT.
- Installed Go `oxlint-tsgolint` 7.0.2004, module revision
  `a61289117febcb1a60cf3bc75ef2b6bbccc4b665+dirty`, Go 1.26.8.
  The exact binary hashes are in the [raw results](../bench/results/2026-10-15-tsgolint-vscode-local.json).
- Apple M2 Max, 12 logical CPUs, 96 GiB RAM, macOS 27.0.1.

Oxlint received an explicit configuration enabling only the 60 implemented type-aware rules,
with default rule options, `--type-aware --no-ignore --format json src`.
Its captured V2 payload contains **9,324 paths**: 9,281 `.ts`, 2 `.tsx`, 38 `.js`, and 3 `.cjs`.
There are no source overrides, fixes, or syntactic/semantic compiler diagnostic reports.
No source files were modified. This includes intentional invalid JavaScript fixtures;
Oxlint's own parser reports them during the end-to-end run.

## Default settings: complete helper

Each helper receives the same captured payload through stdin. There is one warm-up for each,
then three fresh-process measurements in alternating order (Rust/Go, Go/Rust, Rust/Go).
The filesystem cache is warm; compiler state is rebuilt each time.
The table reports independent medians. No benchmark builds or tests overlapped the measurements.

| Metric | Rust | Go | Rust / Go |
| --- | ---: | ---: | ---: |
| Wall time | 153.908 s | 10.860 s | 14.17× |
| Wall range | 153.585–155.385 s | 10.837–10.931 s | |
| Peak RSS | 2.263 GiB | 7.855 GiB | 0.288× |
| Instructions retired | 2800.061 G | 483.442 G | 5.79× |
| User CPU | 217.06 s | 54.85 s | |
| System CPU | 159.42 s | 3.34 s | |
| Diagnostics | 275890 | 275832/275834 | |

Defaults leave `GOMAXPROCS` and `RAYON_NUM_THREADS` unset. Rust's lint callbacks run serially;
configuration and filesystem operations may use Rayon. Go uses its default concurrency.
RSS is the helper process's own high-water mark, with all of its threads included.
`/usr/bin/time -l` supplies CPU, RSS, and instruction counters; the Python runner measures
wall time around the subprocess, excluding Python's output decoding.
These are local macOS screening numbers on a shared desktop, not deterministic Linux
`bench/count.py` results or a two-publish performance landing claim.

## Full Oxlint command

One additional sample per helper uses the same optimized Oxlint binary and configuration,
changing only `OXLINT_TSGOLINT_PATH`. These samples validate the complete frontend/helper path;
they are individual observations, not repeated medians.

| Metric | Rust helper | Go helper |
| --- | ---: | ---: |
| Wall time | 159.390 s | 15.339 s |
| Diagnostics, including frontend errors | 275905 | 275847 |

The command exits with status 1 because the corpus contains intentionally invalid JavaScript fixtures.
The direct helper runs exit successfully and contain no protocol error records.
End-to-end RSS is not used for the helper memory comparison: process-tree resource accounting
is different from measuring each helper directly.

## Per-rule callback profiles: all 60 rules

One separate instrumented run per helper uses `headless -debug timings`,
`GOMAXPROCS=1`, and `RAYON_NUM_THREADS=1`. Both implementations time rule initialization
and registered callbacks. Values below are accumulated callback milliseconds within one
all-rule run; they include type resolution triggered by a rule and depend on rule/file order
and shared caches. They are not independent cold per-rule process benchmarks. Instrumented
wall time is excluded from the headline medians; timing overhead also differs by implementation.
The raw artifact includes callback counts as well as durations.

- Rust: 168.273 s instrumented wall, 18.087 s summed callbacks; 60 timing records.
- Go: 48.879 s instrumented wall, 39.635 s summed callbacks; 60 timing records.

About 150.2 seconds of Rust's instrumented run falls outside timed callbacks. This includes
project discovery, program construction/binding, AST traversal/dispatch, and output work;
the profile does not distinguish those phases. The large overall gap is therefore not
explained by the summed rule callback times. Among individual callback profiles,
`no-unnecessary-type-parameters` stands out at 3,786.826 ms versus 178.815 ms, subject to
the cache and ordering limitations above.

| Rule | Rust callback ms | Go callback ms | Rust / Go | Rust diagnostics | Go diagnostics |
| --- | ---: | ---: | ---: | ---: | ---: |
| await-thenable | 104.735 | 194.952 | 0.54× | 307 | 307 |
| consistent-return | 219.234 | 66.906 | 3.28× | 965 | 965 |
| consistent-type-exports | 2.506 | 21.005 | 0.12× | 20 | 20 |
| dot-notation | 62.677 | 75.894 | 0.83× | 3034 | 3034 |
| no-array-delete | 0.952 | 2.918 | 0.33× | 7 | 7 |
| no-base-to-string | 130.575 | 305.174 | 0.43× | 200 | 200 |
| no-confusing-void-expression | 429.737 | 728.961 | 0.59× | 9693 | 9693 |
| no-deprecated | 876.030 | 1918.586 | 0.46× | 2217 | 2287 |
| no-duplicate-type-constituents | 19.243 | 90.412 | 0.21× | 195 | 192 |
| no-floating-promises | 2105.485 | 8848.686 | 0.24× | 6416 | 6416 |
| no-for-in-array | 1.112 | 3.643 | 0.31× | 6 | 6 |
| no-generated-empty-object-type | 51.942 | 149.151 | 0.35× | 10 | 10 |
| no-implied-eval | 60.690 | 41.924 | 1.45× | 4 | 4 |
| no-meaningless-void-operator | 6.571 | 29.880 | 0.22× | 10 | 10 |
| no-misused-promises | 2934.257 | 8969.824 | 0.33× | 2032 | 2032 |
| no-misused-spread | 66.218 | 89.481 | 0.74× | 61 | 61 |
| no-mixed-enums | 1.290 | 4.068 | 0.32× | 3 | 3 |
| no-redundant-type-constituents | 14.366 | 21.457 | 0.67× | 401 | 401 |
| no-unnecessary-boolean-literal-compare | 17.934 | 36.675 | 0.49× | 480 | 480 |
| no-unnecessary-condition | 331.399 | 681.068 | 0.49× | 9554 | 9540 |
| no-unnecessary-qualifier | 236.847 | 89.241 | 2.65× | 64 | 64 |
| no-unnecessary-template-expression | 5.659 | 34.131 | 0.17× | 120 | 120 |
| no-unnecessary-type-arguments | 224.353 | 68.581 | 3.27× | 274 | 274 |
| no-unnecessary-type-assertion | 116.584 | 425.401 | 0.27× | 3334 | 3277 |
| no-unnecessary-type-conversion | 62.908 | 53.511 | 1.18× | 494 | 494 |
| no-unnecessary-type-parameters | 3786.826 | 178.815 | 21.18× | 330 | 331 |
| no-unsafe-argument | 880.722 | 1803.553 | 0.49× | 2507 | 2507 |
| no-unsafe-assignment | 1724.381 | 6036.494 | 0.29× | 5417 | 5417 |
| no-unsafe-call | 171.569 | 372.225 | 0.46× | 2100 | 2100 |
| no-unsafe-enum-comparison | 38.686 | 89.147 | 0.43× | 1690 | 1690 |
| no-unsafe-member-access | 562.087 | 1783.702 | 0.32× | 6634 | 6634 |
| no-unsafe-return | 568.395 | 1813.292 | 0.31× | 1037 | 1031 |
| no-unsafe-type-assertion | 80.026 | 227.503 | 0.35× | 13014 | 13013/13015 |
| no-unsafe-unary-minus | 5.108 | 3.720 | 1.37× | 0 | 0 |
| no-useless-default-assignment | 12.629 | 13.923 | 0.91× | 83 | 83 |
| non-nullable-type-assertion-style | 9.470 | 77.385 | 0.12× | 96 | 96 |
| only-throw-error | 20.684 | 67.050 | 0.31× | 65 | 65 |
| prefer-find | 60.708 | 78.000 | 0.78× | 65 | 65 |
| prefer-includes | 59.005 | 64.116 | 0.92× | 403 | 403 |
| prefer-nullish-coalescing | 45.542 | 124.082 | 0.37× | 4136 | 4136 |
| prefer-optional-chain | 48.997 | 132.723 | 0.37× | 1277 | 1230 |
| prefer-promise-reject-errors | 92.542 | 175.818 | 0.53× | 181 | 181 |
| prefer-readonly | 129.389 | 180.468 | 0.72× | 3369 | 3369 |
| prefer-readonly-parameter-types | 157.022 | 561.966 | 0.28× | 140291 | 140291 |
| prefer-reduce-type-parameter | 46.560 | 21.788 | 2.14× | 20 | 20 |
| prefer-regexp-exec | 44.536 | 57.638 | 0.77× | 301 | 301 |
| prefer-return-this-type | 14.369 | 68.831 | 0.21× | 56 | 56 |
| prefer-string-starts-ends-with | 72.533 | 77.675 | 0.93× | 223 | 223 |
| promise-function-async | 33.516 | 387.410 | 0.09× | 10090 | 10090 |
| related-getter-setter-pairs | 5.250 | 7.453 | 0.70× | 6 | 6 |
| require-array-sort-compare | 43.492 | 66.912 | 0.65× | 37 | 37 |
| require-await | 68.815 | 61.122 | 1.13× | 8366 | 8366 |
| restrict-plus-operands | 35.492 | 51.904 | 0.68× | 42 | 42 |
| restrict-template-expressions | 10.375 | 84.052 | 0.12× | 643 | 643 |
| return-await | 44.741 | 60.910 | 0.73× | 292 | 292 |
| strict-boolean-expressions | 198.593 | 382.238 | 0.52× | 26745 | 26745 |
| strict-void-return | 638.543 | 1226.380 | 0.52× | 4220 | 4220 |
| switch-exhaustiveness-check | 14.307 | 59.449 | 0.24× | 617 | 617 |
| unbound-method | 214.342 | 243.290 | 0.88× | 940 | 939 |
| use-unknown-in-catch-callback-variable | 64.917 | 42.863 | 1.51× | 696 | 696 |

## Diagnostic differences

This is a comparison of the current Rust port and installed Go package, not a claim of full
VS Code diagnostic parity. Rust retains the original pinned rule semantics except for the
previously documented diagnostic refreshes and additional rule. The Go package is newer.
Equal diagnostic counts would also not establish equal diagnostic contents. Observed counts
in the default helper samples differ for the following rules:

| Rule | Rust | Go |
| --- | ---: | ---: |
| no-deprecated | 2217 | 2287 |
| no-duplicate-type-constituents | 195 | 192 |
| no-unnecessary-condition | 9554 | 9540 |
| no-unnecessary-type-parameters | 330 | 331 |
| no-unnecessary-type-assertion | 3334 | 3277 |
| no-unsafe-return | 1037 | 1031 |
| no-unsafe-type-assertion | 13014 | 13013/13015 |
| prefer-optional-chain | 1277 | 1230 |
| unbound-method | 940 | 939 |

The Go `no-unsafe-type-assertion` count varied by two between otherwise identical default
runs. All default Rust counts were stable. This benchmark does not diagnose those differences.

## Reproduction

Build the binaries above, then generate an Oxlint JSON configuration with
`plugins: ["typescript"]`, `categories: {"correctness": "off"}`, and each name in the raw
artifact's `metadata.rules` enabled as `"typescript/<name>": "warn"`.

Capture the frontend's request once by pointing `OXLINT_TSGOLINT_PATH` at an executable Python
wrapper that writes `sys.stdin.buffer.read()` to `payload.json` and exits without output.
Run it from the VS Code root:

```sh
OXLINT_TSGOLINT_PATH="$CAPTURE_WRAPPER" "$OXLINT"   --type-aware --no-ignore -c "$CONFIG" --format json src > capture-result.json
```

Replay that identical request with each helper, also from the VS Code root:

```sh
/usr/bin/time -l -o run.time "$HELPER" headless < payload.json > run.frames
# Separate instrumentation run:
GOMAXPROCS=1 RAYON_NUM_THREADS=1 /usr/bin/time -l -o profile.time   "$HELPER" headless -debug timings < payload.json > profile.frames
# Full frontend/helper validation:
OXLINT_TSGOLINT_PATH="$HELPER" /usr/bin/time -l -o oxlint.time   "$OXLINT" --type-aware --no-ignore -c "$CONFIG" --format json src > oxlint.json
```

Unset `GOMAXPROCS`, `RAYON_NUM_THREADS`, `TSRS_CHECKERS`, `TSRS_HISTORY`,
`TS_TEST_PROGRAM_SINGLE_THREADED`, and the dangerous program-diagnostic suppression flag
for the default measurements. Input/config hashes, binary hashes, raw counters, diagnostic
counts, callback timings/counts, and run ordering are in the JSON artifact.
The full local capture, process outputs, runner, and resource logs remain in
`/private/tmp/tsrs-vscode-lint-bench/`.
