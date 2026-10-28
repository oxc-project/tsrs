# DHAT allocation workflow and copy-removal trials

Baseline: `bc67f5bfcbbb66b9c123013f05d0e683810d4a27`. The TypeScript reference checkout matches
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with no tracked changes under `tsc`; the reference binary was rebuilt.

## Method

The opt-in `tsrs_cli/dhat-heap` feature uses DHAT 0.3.3 and the system allocator, retaining normal compressed
pointers. `TSRS_DHAT_FILE` controls activation and output; `TSRS_DHAT_FRAMES` controls stack depth. Ordinary
builds retain mimalloc and contain neither the profiler nor its environment checks. `vp run perf:alloc` records
repeated single-checker counts, catalog revisions, binary hashes, ordered diagnostics and allocation stacks.
See [the debugging guide](../docs/DEBUGGING.md#heap-allocation-profiles-with-dhat) for commands.

The initial 32-frame webpack reference-mode profile recorded 3,251,261 allocation blocks and 322,578,594 total
allocated bytes. Its raw JSON was 197 MiB and collecting it took several minutes. Repeated comparisons therefore
use four physical frames; deeper profiles remain available for attribution. Inline frames do not count against
DHAT's physical-frame limit. Total blocks include reallocations. Neither total bytes nor bytes at DHAT's global
heap peak measures process RSS, and direct arena mappings are outside the profiler's coverage.

Both instrumented and ordinary binaries use release optimization, full debug information and no stripping.
macOS dSYM bundles are saved beside each binary before rebuilding, so saved profiles retain their source
locations. Ordinary mimalloc binaries supply the separate instruction, wall-time and peak-RSS measurements.
Builds and profiles must finish before timed comparisons begin. Measurements use an Apple M3 Max, 14 cores,
36 GiB RAM, Darwin arm64, Rust 1.99.1 (`58cf8f9c3`), and Hyperfine 2.0.0.

Prior work consulted: `docs/RUST.md` Techniques, `notes/perf-round2-followups.md` (including rejected trials),
`notes/mem-round4.md`, the recent data-structure rounds, and the resolver, VFS, JSDoc and reparse-cloner notes.
This work targets remaining copy boundaries, without repeating their interning, map replacement or pooling trials.

## Initial sites

The first webpack profile attributes the following allocations to the specific source lines, excluding other
allocations made by the same functions:

| Candidate | Allocation blocks | Allocated bytes | Proposed change |
| --- | ---: | ---: | --- |
| Declaration snapshot in `check_function_or_constructor_symbol_worker` | 21,518 | 106,432 | Iterate the already stored declaration slice |
| Lazy `parse_jsdoc_for_node` result vector | 8,557 | 34,360 | Allocate the final slice in the source-file owner region before recycling the parser |
| Temporary duplicate-member name strings | 130 | 1,254 | Use existing arena-backed names as map keys |

The overload checker's separate growable function-declaration accumulator remains necessary. Its 20,333
allocations are not the snapshot above. The name strings are cold on webpack; allocation counts alone do not
establish an instruction or RSS benefit.

Attribution uses the first frame defined in a tsrs source file. A generic standard-library method such as
`<P<Node> as ConvertVec>::to_vec` has a tsrs receiver but is not itself the allocating tsrs caller; matching only
the symbol's prefix would merge unrelated copies into a misleading bucket.

## Isolated allocation trials

Each isolated variant was profiled once per mode on these three projects. Reference and fast modes remove
exactly the same number of blocks, and ordered diagnostics and exit status match the baseline:

| Candidate | webpack blocks removed | typeorm blocks removed | Compiler-Unions blocks removed |
| --- | ---: | ---: | ---: |
| Borrow declaration slice | 21,518 | 6,049 | 5,932 |
| Borrow duplicate-member names | 130 | 9 | 0 |
| Return final JSDoc slice | 8,557 | 2,995 | 882 |

Borrowing map keys also reduces the size of their buckets. Totals include process startup: the saved executable
paths have different lengths, accounting for a few extra bytes outside the changed compiler sites (for example,
the name-only Compiler-Unions trial has unchanged allocation counts and four extra startup bytes). The raw
profiles retain these costs; they are not subtracted from the results.

## Combined allocation trial

Baseline and combined binaries were each profiled three times per project and mode. Counts and bytes repeat
exactly, and all 30 candidate runs match the baseline's ordered diagnostics and exit status.

| Project | Reference blocks, baseline → candidate | Change | Fast blocks, baseline → candidate | Change | Allocated bytes removed in either mode |
| --- | ---: | ---: | ---: | ---: | ---: |
| webpack | 3,251,261 → 3,221,056 | -0.929% | 3,031,226 → 3,001,021 | -0.996% | 145,582 |
| typeorm | 2,614,619 → 2,605,566 | -0.346% | 2,549,784 → 2,540,731 | -0.355% | 38,224 |
| xstate-main | 2,442,759 → 2,441,854 | -0.037% | 2,229,369 → 2,228,464 | -0.041% | 4,016 |
| Compiler-Unions | 384,836 → 378,022 | -1.771% | 367,202 → 360,388 | -1.856% | 28,080 |
| vscode | 26,107,036 → 25,963,275 | -0.551% | 24,611,279 → 24,467,518 | -0.584% | 604,262 |

DHAT's global heap peak is unchanged in every cell. These are temporary allocations; reduced allocation traffic
does not establish a reduction in process RSS.

## Uninstrumented runtime measurements

Retained the three changes together. The retention basis is the repeatable reduction in single-checker instructions
on Compiler-Unions, alongside the verified allocation reductions. All 15 pairs across the main and confirmation
runs retire fewer instructions in both modes. The reference-mode paired reduction is 0.021% in the main run and
0.028% in the confirmation; fast mode is 0.030% and 0.033%. These are small improvements, with no percentage floor
under the current acceptance policy.

The full catalog has lower paired-median single-checker instructions on 19/19 projects in reference mode and
18/19 in fast mode. The medians of those project changes are -0.041% and -0.038%. Most individual differences are
smaller than the variation between pairs; the sign of a small median alone is not strong evidence for a project.

There is no overall speed or RSS improvement claim. Median project changes in default-checker peak RSS are
-0.014% (reference) and +0.103% (fast); default-checker wall time is -0.098% and -0.102%. Single-checker wall time
is +0.101% and +0.256%. Some regressions repeat: VS Code fast single-checker wall time increases 0.521% in the
main run and 0.800% in confirmation; webpack fast single-checker wall time increases 0.862% and 0.672%.
cal-diy fast single-checker instructions increase 0.097% and 0.078%. The tables retain all gains and regressions.

### Procedure and short screens

The completed isolated screens use two warmups per binary and three interleaved pairs on webpack, typeorm and
Compiler-Unions, in both modes and checker configurations. An earlier screen interrupted by system sleep is
excluded. The completed comparisons use temporary idle-sleep prevention, with no concurrent builds or profiling.

Single-checker instruction changes from the short screens (reference / fast):

| Candidate | webpack | typeorm | Compiler-Unions |
| --- | ---: | ---: | ---: |
| Declaration slice | -0.031% / -0.027% | -0.001% / +0.003% | -0.021% / -0.022% |
| Borrowed names | -0.031% / -0.022% | -0.005% / +0.006% | -0.005% / approximately zero |
| JSDoc slice | +0.362% / -0.012% | -0.020% / -0.011% | +0.002% / -0.003% |
| Combined | -0.043% / -0.043% | +0.004% / -0.009% | -0.023% / -0.024% |

The JSDoc-only screen increases webpack reference-mode instructions; the combined binary is measured separately.
The name-only effects are too small for a standalone runtime conclusion.

The full catalog comparison uses three warmups per binary and ten alternating baseline/candidate pairs per
project, mode and checker configuration. Every warmup compares ordered diagnostics and exit status; all agree.
Timed invocations use `-p <catalog config> --noEmit --incremental false --pretty false`. Reference mode sets
`TSRS_FAST=0`, fast mode sets `TSRS_FAST=1`. Single-checker measurements add `--singleThreaded` and
`RAYON_NUM_THREADS=1` and `GOMAXPROCS=1`; default-checker measurements leave the flag and worker overrides unset.
Other tsrs, Rayon and allocator overrides are cleared. Hyperfine 2 measures each invocation separately. The
follow-up uses two warmups and five pairs, starting with the opposite binary, on Compiler-Unions, webpack, vscode
and cal-diy with one checker.

### Confirmation

The instruction columns include the number of pairs in which the candidate is lower. Wall-time columns report
paired-median changes for the same single-checker runs.

| Project | Mode | Main instructions (10 pairs) | Confirmation instructions (5 pairs) | Main wall time | Confirmation wall time |
| --- | --- | ---: | ---: | ---: | ---: |
| Compiler-Unions | reference | -0.021% (10/10) | -0.028% (5/5) | +0.541% | +0.065% |
| Compiler-Unions | fast | -0.030% (10/10) | -0.033% (5/5) | +0.256% | +0.239% |
| webpack | reference | -0.048% (8/10) | -0.077% (4/5) | +0.332% | +0.370% |
| webpack | fast | -0.055% (8/10) | -0.014% (3/5) | +0.862% | +0.672% |
| vscode | reference | -0.064% (8/10) | -0.039% (3/5) | +0.448% | +0.674% |
| vscode | fast | -0.011% (6/10) | -0.010% (3/5) | +0.521% | +0.800% |
| cal-diy | reference | -0.050% (6/10) | -0.075% (3/5) | -0.662% | +0.344% |
| cal-diy | fast | +0.097% (2/10) | +0.078% (2/5) | -0.062% | +0.413% |

### Full catalog

#### Reference mode

Baseline → candidate medians; parenthesized changes are medians of paired percentage differences, which need not equal the ratio of the two medians.

| Project | Single-checker instructions (G) | Single-checker wall time (s) | Default-checker peak RSS (MiB) | Default-checker wall time (s) |
| --- | ---: | ---: | ---: | ---: |
| vscode | 91.308393 → 90.755525 (-0.064%) | 8.102 → 8.134 (+0.448%) | 2413.00 → 2412.66 (+0.013%) | 2.442 → 2.442 (+0.089%) |
| xstate-main | 6.462047 → 6.459730 (-0.033%) | 0.526 → 0.523 (-0.493%) | 264.69 → 264.12 (-0.174%) | 0.196 → 0.196 (+0.022%) |
| webpack | 11.123630 → 11.118342 (-0.048%) | 0.880 → 0.885 (+0.332%) | 405.00 → 404.77 (-0.133%) | 0.306 → 0.306 (+0.061%) |
| mui-docs | 41.059216 → 41.048662 (-0.013%) | 3.015 → 3.007 (-0.161%) | 1316.93 → 1320.36 (+0.087%) | 1.588 → 1.585 (-0.633%) |
| Compiler | 1.733799 → 1.733141 (-0.041%) | 0.138 → 0.139 (-0.014%) | 76.03 → 76.03 (-0.051%) | 0.064 → 0.064 (+0.682%) |
| Compiler-Unions | 2.991520 → 2.990931 (-0.021%) | 0.204 → 0.205 (+0.541%) | 80.23 → 80.47 (+0.078%) | 0.096 → 0.096 (-0.208%) |
| cal-diy | 33.627570 → 33.615886 (-0.050%) | 2.715 → 2.705 (-0.662%) | 1343.39 → 1341.16 (-0.306%) | 1.136 → 1.135 (-0.398%) |
| formbricks-web | 44.404397 → 44.400594 (-0.003%) | 3.533 → 3.535 (-0.328%) | 1962.62 → 1966.84 (+0.220%) | 1.355 → 1.355 (+0.230%) |
| supabase-studio | 47.579388 → 47.833768 (-0.001%) | 3.812 → 3.824 (+0.101%) | 1405.00 → 1404.86 (+0.034%) | 1.654 → 1.660 (+1.234%) |
| t3code-server | 48.730193 → 48.728309 (-0.007%) | 3.703 → 3.708 (-0.119%) | 1861.16 → 1861.34 (+0.031%) | 2.023 → 2.024 (+0.005%) |
| mikro-orm | 53.389643 → 53.388227 (-0.002%) | 4.152 → 4.158 (+0.207%) | 1631.09 → 1628.97 (+0.046%) | 1.584 → 1.587 (-0.098%) |
| next-packages-next | 18.708496 → 18.697109 (-0.073%) | 1.440 → 1.439 (-0.256%) | 621.25 → 621.09 (-0.020%) | 0.609 → 0.607 (-0.130%) |
| next-root | 18.734333 → 18.715809 (-0.106%) | 1.447 → 1.453 (+0.199%) | 504.07 → 504.37 (-0.014%) | 0.597 → 0.588 (-1.819%) |
| storybook | 10.278271 → 10.273621 (-0.046%) | 0.800 → 0.801 (+0.658%) | 445.49 → 444.77 (-0.153%) | 0.300 → 0.299 (+0.047%) |
| nuxt | 11.064469 → 11.058677 (-0.051%) | 0.894 → 0.895 (+0.293%) | 422.34 → 423.09 (+0.137%) | 0.341 → 0.337 (-1.666%) |
| playwright | 7.728997 → 7.726027 (-0.040%) | 0.638 → 0.640 (+0.038%) | 359.95 → 359.93 (-0.035%) | 0.245 → 0.244 (-0.138%) |
| drizzle-orm | 6.356552 → 6.354539 (-0.029%) | 0.500 → 0.502 (+0.885%) | 413.23 → 413.58 (-0.117%) | 0.262 → 0.262 (+0.063%) |
| typeorm | 7.721636 → 7.714136 (-0.096%) | 0.638 → 0.640 (+0.621%) | 231.89 → 231.05 (-0.371%) | 0.261 → 0.258 (-1.827%) |
| date-fns | 4.673734 → 4.669869 (-0.117%) | 0.348 → 0.344 (-0.547%) | 174.54 → 174.98 (+0.188%) | 0.169 → 0.166 (-1.936%) |

#### Fast mode

Baseline → candidate medians; parenthesized changes are medians of paired percentage differences, which need not equal the ratio of the two medians.

| Project | Single-checker instructions (G) | Single-checker wall time (s) | Default-checker peak RSS (MiB) | Default-checker wall time (s) |
| --- | ---: | ---: | ---: | ---: |
| vscode | 80.522741 → 80.499579 (-0.011%) | 7.550 → 7.592 (+0.521%) | 2037.99 → 2040.00 (+0.235%) | 0.999 → 1.003 (+0.045%) |
| xstate-main | 6.062452 → 6.060125 (-0.038%) | 0.504 → 0.502 (-0.206%) | 245.10 → 244.75 (-0.102%) | 0.128 → 0.127 (-0.871%) |
| webpack | 10.717659 → 10.711862 (-0.055%) | 0.871 → 0.880 (+0.862%) | 478.46 → 480.99 (+0.521%) | 0.157 → 0.157 (+1.450%) |
| mui-docs | 37.580475 → 37.568662 (-0.026%) | 2.796 → 2.799 (+0.087%) | 1493.29 → 1500.29 (+0.701%) | 1.164 → 1.164 (-0.054%) |
| Compiler | 1.697158 → 1.696476 (-0.040%) | 0.137 → 0.137 (-0.203%) | 75.09 → 75.05 (+0.083%) | 0.064 → 0.064 (+0.292%) |
| Compiler-Unions | 2.894655 → 2.893797 (-0.030%) | 0.201 → 0.201 (+0.256%) | 80.30 → 80.28 (-0.117%) | 0.096 → 0.096 (+0.766%) |
| cal-diy | 31.433620 → 31.477433 (+0.097%) | 2.584 → 2.580 (-0.062%) | 1762.23 → 1758.95 (+0.103%) | 0.760 → 0.754 (-1.146%) |
| formbricks-web | 39.996028 → 39.409359 (-0.526%) | 3.240 → 3.243 (+0.337%) | 1864.91 → 1862.26 (-0.111%) | 0.773 → 0.774 (-0.102%) |
| supabase-studio | 44.817863 → 44.781436 (-0.074%) | 3.643 → 3.663 (-0.365%) | 1575.63 → 1571.55 (-0.224%) | 0.845 → 0.838 (-0.971%) |
| t3code-server | 39.598434 → 39.597324 (-0.003%) | 3.196 → 3.205 (+0.161%) | 1992.26 → 2004.02 (+0.453%) | 1.319 → 1.333 (+1.382%) |
| mikro-orm | 46.913591 → 46.909377 (-0.004%) | 3.771 → 3.774 (+0.306%) | 1857.25 → 1857.82 (+0.039%) | 0.756 → 0.760 (+0.434%) |
| next-packages-next | 17.685026 → 17.676932 (-0.020%) | 1.396 → 1.403 (+0.123%) | 635.51 → 634.35 (-0.112%) | 0.257 → 0.255 (-1.113%) |
| next-root | 18.359010 → 18.338673 (-0.111%) | 1.430 → 1.437 (+0.365%) | 496.32 → 490.39 (-0.681%) | 0.412 → 0.407 (-1.188%) |
| storybook | 9.891997 → 9.891521 (-0.054%) | 0.786 → 0.794 (+0.835%) | 467.21 → 470.13 (+0.436%) | 0.192 → 0.188 (-0.293%) |
| nuxt | 10.573529 → 10.567726 (-0.065%) | 0.872 → 0.874 (+0.375%) | 482.30 → 483.33 (+0.277%) | 0.242 → 0.246 (+1.955%) |
| playwright | 7.501055 → 7.498434 (-0.028%) | 0.626 → 0.632 (+0.620%) | 450.26 → 446.49 (-0.522%) | 0.140 → 0.139 (-0.349%) |
| drizzle-orm | 6.235333 → 6.234252 (-0.008%) | 0.497 → 0.498 (+0.332%) | 523.59 → 528.22 (+0.975%) | 0.165 → 0.166 (+1.079%) |
| typeorm | 7.654102 → 7.647554 (-0.078%) | 0.638 → 0.638 (+0.096%) | 244.48 → 245.21 (+0.229%) | 0.182 → 0.178 (-1.982%) |
| date-fns | 4.351037 → 4.346963 (-0.131%) | 0.330 → 0.329 (-0.682%) | 180.81 → 181.33 (+0.277%) | 0.127 → 0.125 (-0.841%) |


## Validation and artifacts

Both reference and fast-mode baselines have the same passing lists: 13,700 diagnostic, 12,998 type and symbol,
13,626 JavaScript, 149 JavaScript-map and 156 source-map record passes, with no mismatches or crashes. Four reference
stress cases exceeded the initial development-build timeout; rerunning at lower concurrency with up to 600 seconds
leaves no timeouts. Fast mode completes the full suite with that timeout. The combined candidate preserves every
passing list in both modes, with no mismatches, crashes or timeouts.

A parser integration case checks that lazy JSDoc slices belong to the source-file region, remain valid after
request-region release and parser reuse, and are reused by concurrent cache readers. It runs alone in a child
process because registered regions enable process-wide owner routing. The case passes on the baseline and candidate.
The full workspace suite passes 1,252 cases, with four ignored; its initial sandbox run could not create local Unix
sockets, and the same offline command passes with socket access. All 50 regression fixtures pass. Ordinary and
DHAT-feature Clippy, Rust formatting, source lint and script-manifest checks pass.
The complete conformance gate passes all 38 runs: both full suites and 36 focused scheduling runs across
4 and 8 checkers, ten default fast-mode runs and eight seeded assignments per checker count.

Success and diagnostic smoke compilations match the pinned Go compiler and repeat identical allocation counts
in three runs. A normal binary ignores `TSRS_DHAT_FILE`; a DHAT binary without it writes no profile. The runner
rejects missing instrumentation and invalid stack-depth settings. Its baseline-versus-baseline smoke check
alternates execution order and records identical counts and ordered diagnostics in four Compiler-Unions runs.
Tooling is validated by direct runs, not new script tests.

Local artifacts are under `target/dhat-work/`: `allocation-comparison.json`, `conformance-comparison.json`,
`candidate-validation.json`, `perf-combined/results.json` and `perf-confirmation/results.json`, with the raw
profiles, compiler logs, saved binaries and dSYM bundles. `base-build.json` and `combined-build.json` record
binary hashes and exact release build commands; `combined.patch` records the measured compiler edits.
The final compiler sources match `combined.patch`; those are the three retained changes.
