# Type-aware lint bottleneck investigation (2026-10-15)

Follow-ups: [JSON lookup fix](perf-tsgolint-json-fix.md) and [parallel helper construction](perf-tsgolint-parallel-load.md).

Profiled Rust `29c59e7e` and Oxlint `9a424c209e`, the same public VS Code commit, 9,324 input paths and all
60 rules as the [release comparison](bench-tsgolint-vscode-rebased.md). Apple M2 Max, 12 logical CPUs, 96 GiB.
These are instrumented release observations with symbols retained (opt-level 3, fat LTO, one codegen unit,
no PGO/BOLT). Production code is unchanged. Builds and native sampling runs are separate from the timed controls.

## Largest end-to-end stage: final JSON reporting

The control helper consumes the real frontend request, then either returns no diagnostics, replays every tenth
cached diagnostic, replays the full cached stream, or runs the real instrumented helper. Cases rotate across
three measured rounds after one warm-up each. Full replay and real-helper diagnostic hashes match the
previous release exactly, including frontend suppression. Zero/tenth controls intentionally change output.

| Helper mode | Command wall | Final JSON render | Diagnostics |
| --- | ---: | ---: | ---: |
| none | 0.642 s | 0.000 s | 16 |
| tenth | 1.286 s | 0.529 s | 27606 |
| full | 6.134 s | 4.804 s | 275906 |
| real | 12.027 s | 4.822 s | 275906 |

The full cached run performs **no typechecking** yet still takes about six seconds. About 4.8 seconds is
the final, single-threaded JSON rendering pass, after linting has finished. The real-helper command spends
about 40% of its wall time there. Native sampling identifies `LineBreaks::next` and `PrefixScan::new` as the
dominant active leaf frames in this reporting-only run. Sleeping-thread samples are not counted as CPU work.

`JSONReportHandler::render_report` creates a fresh zero-context `SpanScanner` per diagnostic. Line information
is rebuilt by scanning the source prefix repeatedly. Across this output, the sum of each diagnostic’s furthest
label offset is **10.25 GiB**; this is an offset sum illustrating repeated scans, not measured memory traffic.
A shared per-source line index or a specialized bulk zero-context prefix scan is the first optimization to test.
The existing one-context-line path uses bulk LF counting for LF-only sources; JSON’s zero-context path takes the generic loop.

## Matched frontend critical-path observations

One matched run per variant, with stages measured within that invocation (not subtraction of separate medians):

| Stage | Lint only | Full checking |
| --- | ---: | ---: |
| Final JSON render | 4.809 s | 4.889 s |
| Program loading and binding | 2.363 s | 2.378 s |
| Worker execution and join | 2.931 s | 4.479 s |
| Diagnostic delivery | 0.818 s | 0.862 s |
| Remaining wall time | 1.106 s | 0.977 s |
| Command total | 12.027 s | 13.586 s |

## Helper critical path

A warm direct-helper observation separates non-overlapping engine stages below. These cells are observations,
not new headline benchmark medians. The first lint profile had slower loading (3.17 s); the warm observation
and real-frontend profiles put loading near 1.7–1.8 s. Full checking was profiled twice with consistent phases.

| Stage | Lint only | Full checking |
| --- | ---: | ---: |
| Configuration/discovery | 0.212 s | 0.226 s |
| Program loading | 1.808 s | 1.800 s |
| Binding | 0.651 s | 0.653 s |
| Workers, including join | 2.910 s | 4.479 s |
| Output writes to file | 0.020 s | 0.020 s |
| Engine total, including remaining overhead | 5.811 s | 7.563 s |

Program construction explicitly forces `single_threaded = true`; binding also runs serially. Together they
take about **2.45 seconds**, roughly 42% of the warm lint-only engine run. Native compiler parallel parsing/binding
already exists but is bypassed here. Reusing it requires preserving arena lifetime and diagnostic behavior.

The 9,281-file main program uses 12 static chunks of 767–774 files. Their measured thread CPU times are:

| Workload | Fastest worker | Mean worker | Slowest worker | Largest single file |
| --- | ---: | ---: | ---: | ---: |
| lint | 0.745 s | 1.836 s | 2.712 s | 0.223 s |
| lint-typecheck | 1.191 s | 2.787 s | 4.091 s | 0.329 s |

The slowest worker is about 47% above the mean; one giant file does not explain the tail. Thread CPU time
excludes descheduling but includes heterogeneous-core speed and first-touch/type-cache differences. It does not
establish a 47% recoverable wall-time gain. The native compiler already has tail stealing; this lint scheduler
does not use it. Stealing changes cache locality and must be benchmarked with complete diagnostic checks.

## Checking work and diagnostic delivery

The lint-only workers spend about 22.1 CPU-seconds in rule traversal/type queries versus 0.42 CPU-seconds
serializing diagnostics (about 2% of worker CPU). Full checking adds about 12.5 CPU-seconds in separate semantic
checkers, while lint work remains about 21 CPU-seconds. Native helper stacks concentrate in expression/call
checking, flow types, name resolution and symbol maps. Independent checker state limits sharing of repeated queries.

The separate all-rule timing run preserves every diagnostic and all 60 previous callback counts. Its largest
three rules account for about 51% of summed callback elapsed time; these figures include cache-dependent type
resolution and cannot be converted directly into end-to-end wall shares.

| Rule | Summed callback time | Share |
| --- | ---: | ---: |
| no-misused-promises | 5.281 s | 21.5% |
| no-floating-promises | 3.859 s | 15.7% |
| no-unsafe-assignment | 3.474 s | 14.2% |
| no-unsafe-argument | 1.369 s | 5.6% |
| no-deprecated | 1.283 s | 5.2% |
| no-unsafe-return | 1.068 s | 4.4% |

In the real frontend trace, the first helper output arrives about 5.2 s after the proxy starts. Output writes
take about **0.8 s** under pipe backpressure, versus about 0.02 s to a file. Streaming could overlap delivery
with checking, but would leave the larger final JSON render untouched. This evidence moves streaming below
JSON line lookup and serial loading/binding in priority. This is a measured stage comparison, not subtraction
of separate helper/frontend benchmark medians.

## Next experiments

1. Optimize zero-context JSON line/column lookup in Oxlint; verify complete JSON diagnostic hashes.
2. Reuse parallel compiler loading/binding in the helper; verify lifetime and diagnostic parity.
3. Test tail stealing against static chunks, tracking instructions, RSS and full diagnostic multisets.
4. Then optimize the concentrated rule/type-query paths. Avoid merging semantic/lint checker state without
   evidence that diagnostic history remains compatible.

No optimization or speedup is landed by this investigation. Each candidate still needs the repository’s
performance gate and review; phase shares are not promised gains.

## Reproduction and raw evidence

The temporary profiling patches are [helper phases](../bench/profiles/tsgolint-helper-phases.patch) and
[frontend JSON render](../bench/profiles/tsgolint-frontend-json.patch). Apply them to isolated checkouts of the
revisions above and build with `CARGO_PROFILE_RELEASE_STRIP=none cargo build --release --locked`.
The helper’s profiling CPU clock is macOS-specific (clock id 16). Diagnostic stdout remains unchanged;
phase records use stderr. Native samples use `/usr/bin/sample PID 10 1 -file OUTPUT`.

[Raw events, counters, hashes, all 60 timings and exact local proxy/control scripts](../bench/results/2026-10-15-tsgolint-vscode-bottlenecks.json).
The archived scripts contain the local checkout/corpus/config paths used here; adjust those when moving checkouts.
Cached replay uses the previous Rust headless frames, either complete or selecting every tenth diagnostic frame.
The JSON archive includes cache counts/hashes and all input/binary/source identities. Full local outputs remain
under `/private/tmp/tsrs-lint-bottleneck-*`; the temporary checkouts retain the instrumentation and build logs.
