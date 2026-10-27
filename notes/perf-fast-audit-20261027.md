Performance audit, 2026-10-27. The published chart measures npm 0.20.0, not the current checkout. The benchmark commands do enable fast mode, and an independent timer agrees with Hyperfine. The large difference between projects comes from checker scaling, duplicated checker work, heavy individual files, and front-end work shared by both modes.

The automatic fast checker count is too aggressive for t3code-server on this 14-core M3 Max (10 performance cores and 4 efficiency cores). In seven interleaved rounds, 10 checkers beat 14 in six rounds. Their median time is 2.6% lower, instructions about 25.5% lower, and peak RSS about 21.4% lower. Four checkers trade a little wall time for substantially lower memory. This is workload-specific: VS Code benefits substantially from 14 checkers.

**T3 checker-count sweep, published 0.20.0, seven runs per row.**

| Checkers | Wall, s | Instructions, billion (all threads) | Peak RSS, GiB |
| --- | ---: | ---: | ---: |
| 2 | 1.670 | 44.786 | 0.899 |
| 4 | 1.246 | 57.664 | 1.111 |
| 8 | 1.208 | 79.331 | 1.461 |
| 10 | 1.156 | 82.802 | 1.532 |
| 14 (automatic) | 1.187 | 111.175 | 1.950 |

**Mode controls, published 0.20.0, five interleaved runs per cell.**

| Project | Default, s | Fast with 4 checkers, s | Fast automatic, s | Fast 4 / automatic instructions, billion | Fast 4 / automatic RSS, GiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compiler | 0.094 | 0.092 | 0.089 | 1.694 / 1.694 | 0.075 / 0.075 |
| Compiler-Unions | 0.124 | 0.126 | 0.121 | 3.078 / 3.079 | 0.080 / 0.079 |
| mui-docs | 1.480 | 1.270 | 1.134 | 54.880 / 87.207 | 0.990 / 1.474 |
| t3code-server | 1.813 | 1.298 | 1.282 | 57.635 / 110.003 | 1.113 / 1.930 |
| vscode | 2.086 | 1.753 | 0.965 | 73.215 / 80.136 | 1.718 / 1.991 |
| date-fns | 0.177 | 0.170 | 0.155 | 4.413 / 4.741 | 0.155 / 0.188 |

The two Compiler cases use four checkers in both modes. Parallel parsing and the main control-flow memo already run in default mode. The extra preset optimizations save less than 1% of total instructions in these cases.

File timing instrumentation confirms four checkers for the two Compiler cases and 14 for the larger automatic-fast runs. On T3, total checker CPU rises from 4.303 s at four checkers to 9.743 s at 14; a single file takes 0.735 s of the slowest checker’s 1.015 s. On MUI, a single file takes 0.417 s of a 0.601 s checker critical path, while the front end takes about 0.42 s in either mode. VS Code’s heaviest fast-mode file takes only 0.083 s of a 0.554 s checker critical path. These are separate instrumented runs, not the benchmark medians. The data supports duplicated work and long individual files as scaling limits.

The automatic policy in crates/tsrs_compiler/src/checkerpool.rs uses CPU count and a floor of 32 checked files per additional checker. It does not estimate the critical path or duplicated type work. A global reduction would also affect projects such as VS Code that scale well.

**Revision comparison.** Release tag d197eeea709616d61930c2bd5eac048471e404b8 versus the compiler sources at main 1620b64f892a8fb19884af375862d31907df3cd0 (built from branch HEAD d52272409bcf6ab8b9d869d7cdd2cdada49249f8; compiler sources and Cargo files match main). Both use identical non-PGO release settings, full debug symbols, no stripping, no embedded libraries, and identical adjacent npm declaration files.

Instructions below are single-threaded. Wall time and peak RSS use automatic fast checkers. All projects had three interleaved pairs initially. The three initial instruction outliers received seven-pair confirmation and a separate seven-round isolated-attribution batch; Nuxt and date-fns received seven-pair wall confirmation; Playwright and Drizzle received ten-round final wall confirmation with the isolated correctness-revert build interleaved too. The table uses the latest applicable batch rather than pooling changing desktop load.

| Project | Single-thread instructions, billion: tag → main | Change | Fast wall, s: tag → main | Change | Fast peak RSS, GiB: tag → main |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | 80.338 → 80.341 | +0.00% | 1.075 → 1.068 | -0.64% | 1.987 → 1.975 |
| xstate-main | 5.895 → 5.884 | -0.18% | 0.166 → 0.160 | -3.98% | 0.243 → 0.238 |
| webpack | 10.712 → 10.701 | -0.10% | 0.214 → 0.200 | -6.68% | 0.467 → 0.463 |
| mui-docs | 36.979 → 36.868 | -0.30% | 1.127 → 1.146 | +1.68% | 1.475 → 1.470 |
| Compiler | 1.685 → 1.698 | +0.82% | 0.096 → 0.097 | +1.68% | 0.074 → 0.073 |
| Compiler-Unions | 2.853 → 2.896 | +1.49% | 0.130 → 0.127 | -2.34% | 0.080 → 0.078 |
| cal-diy | 30.867 → 30.852 | -0.05% | 0.882 → 0.831 | -5.74% | 1.720 → 1.717 |
| formbricks-web | 38.880 → 38.873 | -0.02% | 0.869 → 0.872 | +0.30% | 1.822 → 1.823 |
| supabase-studio | 43.401 → 43.403 | +0.01% | 0.908 → 0.895 | -1.46% | 1.548 → 1.542 |
| t3code-server | 39.154 → 39.139 | -0.04% | 1.316 → 1.333 | +1.31% | 1.947 → 1.939 |
| mikro-orm | 46.930 → 46.883 | -0.10% | 0.770 → 0.776 | +0.85% | 1.804 → 1.793 |
| next-packages-next | 17.424 → 17.408 | -0.09% | 0.277 → 0.270 | -2.42% | 0.626 → 0.619 |
| next-root | 18.060 → 18.117 | +0.31% | 0.425 → 0.426 | +0.28% | 0.486 → 0.484 |
| storybook | 9.793 → 9.783 | -0.10% | 0.219 → 0.214 | -2.19% | 0.461 → 0.454 |
| nuxt | 9.904 → 9.887 | -0.17% | 0.256 → 0.255 | -0.53% | 0.482 → 0.468 |
| playwright | 7.485 → 7.469 | -0.22% | 0.156 → 0.167 | +6.91% | 0.439 → 0.431 |
| drizzle-orm | 6.132 → 6.123 | -0.15% | 0.186 → 0.192 | +3.40% | 0.516 → 0.506 |
| typeorm | 7.596 → 7.589 | -0.09% | 0.203 → 0.207 | +1.97% | 0.245 → 0.238 |
| date-fns | 4.316 → 4.283 | -0.77% | 0.152 → 0.148 | -2.70% | 0.186 → 0.178 |

Playwright and Drizzle retain short-run wall-time outliers. In the final ten-round batch, tag versus main medians are 0.156 / 0.167 s and 0.186 / 0.192 s respectively. Main is slower in six of ten pairs for each, with wide paired variation. The isolated correctness revert is close to the tag on both. These timing differences are not attributed to that revert and remain candidates for confirmation on an idle host; lower single-threaded instructions do not rule out a wall-time regression. No broad wall-time improvement or absence of all regressions is claimed.

**Isolated attribution.** A third binary applies only commit 32031aefa992492e90566db36cc379bdae948481’s checker change to v0.20.0. This removes the union shortcut that mishandled NoInfer. It uses the same build configuration and declaration files as the other two binaries. Seven rotated, interleaved single-threaded rounds:

| Project | Tag instructions, billion | Tag + correctness revert | Change | Main instructions, billion | Change vs tag |
| --- | ---: | ---: | ---: | ---: | ---: |
| mui-docs | 36.979 | 36.976 | -0.01% | 36.868 | -0.30% |
| Compiler | 1.685 | 1.700 | +0.90% | 1.698 | +0.82% |
| Compiler-Unions | 2.853 | 2.900 | +1.62% | 2.896 | +1.49% |

The isolated revert adds 0.90% instructions on Compiler and 1.62% on Compiler-Unions; current main retains slightly smaller increases of 0.82% and 1.49%. This identifies the deliberate correctness revert as the source of these two small regressions. MUI initially showed +1.2% in two batches, but the independent attribution batch shows main at -0.30% and the isolated revert at -0.01%. Its file, symbol, type and instantiation counts are identical across all three builds and batches. MUI is therefore variable, not an established instruction regression.

The old optimization’s note at v0.20.0:notes/perf-narrowable-reference-union-memo.md reported -0.91% instructions on Compiler and -1.58% on Compiler-Unions. Its removal is a correctness change. The revert is absent from the published 0.20 benchmark binary, so it cannot explain the chart’s fast-versus-default gap.

**Timing-harness and package checks.** Seven interleaved Hyperfine/direct-timer runs returned 0.093 / 0.090 s for Compiler and 1.136 / 1.120 s for MUI. The direct timer was /usr/bin/time -l, with an additional process.hrtime.bigint measurement around spawn; its displayed elapsed time has centisecond precision. TSRS_FAST=1 and --fast produced the same workload signatures and similar instruction counts. No override disabled fast mode. Every matched revision pair retained the same file count and file/location/diagnostic-code signature.

The published package reduces instructions in both modes versus a plain release build from the same tag. Five interleaved rounds per cell:

| Project | Default instructions, billion: plain → npm | Fast instructions, billion: plain → npm | Default wall, s: plain → npm | Fast wall, s: plain → npm |
| --- | ---: | ---: | ---: | ---: |
| Compiler | 2.030 → 1.707 | 2.007 → 1.694 | 0.094 → 0.093 | 0.100 → 0.085 |
| mui-docs | 63.143 → 58.766 | 91.993 → 87.388 | 1.563 → 1.435 | 1.136 → 1.105 |
| t3code-server | 87.189 → 75.622 | 125.958 → 111.296 | 2.029 → 1.688 | 1.310 → 1.151 |
| vscode | 97.975 → 83.944 | 92.697 → 80.151 | 2.398 → 1.988 | 0.977 → 0.873 |

**Method and retained evidence.**

The audit contains 754 timed samples, plus warmup/diagnostic runs and 18 file-timing runs. The desktop had background activity; the audit waited below three busy cores and recorded load for every sample (average 1.31 cores). Small wall-time changes are less conclusive than matched single-threaded instruction counts. The published chart retains its earlier 1.5-core quiet threshold and original raw data. The preceding chart was made on an M4 Pro using the identical npm compiler hash; comparing those charts cannot establish a code regression.

All commands used -p <catalog config> --noEmit --incremental false --pretty false. Fast runs add --fast; explicit-count controls add --checkers N. Single-threaded runs add --singleThreaded and set RAYON_NUM_THREADS=1 and GOMAXPROCS=1. Default-count runs leave both unset. TSRS_*, allocator and thread overrides were cleared, except the deliberate TSRS_FAST=1 control. Compilers were warmed before each group and order rotated each round. No build ran concurrently with measurements.

Build command: cargo build --release --locked -p tsrs_cli --features noembed --config profile.release.debug=true --config profile.release.strip=false --target-dir bench/.work/fast-audit-20261027/target. RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS were cleared. The isolated sources and three compiler binaries are retained below bench/.work/fast-audit-20261027; the correctness-revert patch is noninfer-revert.patch.

Published binary SHA-256: d6d149d8cce39e69e46d9e4c4a76fc4c2f5fd5785d856a0cf286971c12861767.
Isolated correctness-revert binary SHA-256: 99eb3ab5fb6dff017a58f78bbebddef1b2873257742ce1b3b2a16c01595373f1.

Raw audit directory: bench/.work/fast-audit-20261027/. It contains builds.json, attribute-build.json, per-sample Hyperfine JSON, diagnostic logs, per-file TSVs, and the JavaScript drivers. Each driver saves commands, environment, build metadata, diagnostic signatures, background load and individual samples. Aggregate bundles:

| Bundle | Samples | SHA-256 |
| --- | ---: | --- |
| audit.json | 376 | 80893419a86fe8d156415bb0eb94ce9d3e4736413eead2526edaec13a2579a9c |
| pgo.json | 80 | 0bf488e820a23584e128b4c2087674b88b2a8c116fd6dd98431b93439979487d |
| scaling.json | 35 | c8f1dd1eb13310e448ef8527d5e875771611b646a55508e3fa780f1d0a36e76b |
| confirmation.json | 84 | 2093dfe2062bd3f13631e3e047a300da280633909614ae6ddd0f7be10f5e2108 |
| wall-confirmation.json | 56 | 2dff861c0dc35a7199b540a3cfcb1423efb5467e54beefe16d555543c14de573 |
| attribution.json | 63 | 4fcd6ce6743621b6252fe14e68b62ff9739f8b1a0dd3d1363bcf94cfdecf3216 |
| latency.json | 60 | 2a6d6e4e236301eef7b54a366cfc89accfd10ba444ee1e3a61e1b87e50482dab |

At the time of this audit, the chart was sourced from bench/results/20261027T073353047Z/run.json. The subsequent requested rerun uses bench/results/20261027T093401744Z/run.json; its current chart data is bench/publication.json. The audit measurements above retain their original inputs and timestamps.
