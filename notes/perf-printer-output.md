# Printer output allocations

This change appends indentation directly to the output buffer and moves completed output out of `TextWriter`
instead of cloning it immediately before clearing the writer. `EmitTextWriter::string()` remains a
non-consuming snapshot. The new `take_string()` operation resets the same state as `clear()`, preserves newline
and indentation configuration, and has a cloning default for other writer implementations. Boxed, borrowed and
trailing-semicolon writers forward the operation; taking output still discards a deferred final semicolon.

The pinned Go printer already returns `strings.Builder.String()` without copying and replaces its builder on
`Clear()`. The reference was built from `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with a clean `tsc/` tree.
The existing performance notes did not measure these two Rust allocation sites.

## Validation

- All 18 printer integration cases pass, including extraction/reuse, empty output, deep indentation, LF/CRLF,
  UTF-16 columns, comments, trailing whitespace, the default trait implementation and deferred semicolons.
- All 50 regression fixtures pass.
- The affected code-fix, refactor, formatting, quick-info, completion and signature-help fourslash selection
  passes 1,446 cases and skips 200, with no failures.
- Baseline and candidate have identical lists for all six conformance outputs: 13,699 diagnostics, 12,997 types,
  12,997 symbols, 13,625 JavaScript/declaration outputs, 149 JavaScript maps and 156 source-map records pass.
  There are no mismatches or crashes. `compiler/intersectionConstructorReductionCrash` exceeds 180 seconds in
  both builds; unsupported cases retain their existing skips. The initial full runs used the default 20-second
  timeout; their timed-out cases were rerun with two workers and a 180-second timeout.
- Workspace Clippy with warnings denied, source checks, formatting and whitespace checks pass.
- Single-threaded emit output and diagnostics match the pinned Go compiler byte for byte on vscode (37,612
  files), xstate (993), webpack (1,863) and mui-docs (52,687). Both Rust builds also match each other with
  default checkers.

## Measurement method

Baseline is `4a61321929d0d62b86c5fb35f15bfa1607e03b50`. Both release binaries use Rust 1.99.1,
`profile.release.debug=true` and `profile.release.strip=false`. Measurements use Hyperfine 2.0.0 with
`time_wall_clock:s,instructions:B,memory_peak_resident:GiB`. Each configuration receives a warmup per binary,
then five alternating baseline/candidate pairs, reversing the order every pair (six pairs for the first,
single-threaded vscode emit configuration, completed before reducing the repetition count). Single-threaded
instructions and default-checker peak RSS are reported separately; timing is end-to-end compiler wall time.
The machine is an Apple M3 Max with 14 cores and 36 GiB RAM, running macOS 27.0.1. Other work was running on
the machine; the recorded one-minute load average ranged from 12.7 to 74.8. These timings do not establish a
wall-time speedup. Measurements ran on October 28, 2026 (UTC+8).

The public projects and commits are the 19 entries in `bench/projects.json` at the baseline revision. The
checkouts' HEADs and configuration hashes are verified. `bench/lib.mjs` supplies `cleanEnvironment()` and
`projectPath()`; `bench/runner.mjs` supplies `measureOne()`. The clean environment uses `TSRS_FAST=1`, and
single-threaded runs add `--singleThreaded`. Both binaries, configuration hashes, raw Hyperfine samples,
diagnostics and emitted-file hashes are retained locally in `target/oxc-improvements/`.

Check-only runs use `-p <catalog config> --pretty false --incremental false --noEmit`. A separate
`--extendedDiagnostics` preflight verifies a nonempty source-file count. Emit runs use the same catalog
configuration with `--noEmit false --noEmitOnError false --emitDeclarationOnly false --sourceMap true
--allowImportingTsExtensions false --rootDir <checkout root> --outDir <scratch output>`. Declarations and
declaration maps are enabled, with `--declarationDir <scratch output>`, except for webpack, which measures
JavaScript and source maps. The output directory is removed before each run. Baseline and candidate
diagnostics and output bytes are compared before measuring each configuration.

## Results and tradeoffs

Vscode emit's single-threaded instructions fall from 207.143 G to 206.592 G in the independent medians.
The median paired change is -0.22%, with fewer instructions in five of six pairs. Xstate's paired median is
-0.12%, with four of five pairs improving. Webpack's independent instruction medians are slightly higher
despite a lower paired median; mui emit is effectively unchanged. These results support a small reduction
in CPU work on some emit workloads. Default-checker emit peak RSS is essentially unchanged.

Across the check-only catalog, 11 of 19 projects have lower median paired single-threaded instructions, and
11 have lower median paired default-checker peak RSS. The effects are mixed and mostly small. Mui's
single-threaded check is a regression: instructions increase in all five pairs, by 0.07% to 2.59% (median
paired change +0.36%). Its independent medians increase from 37.615 G to 38.589 G (+2.59%). Default-checker
RSS regressions include formbricks-web (1,857.00 to 1,863.66 MiB) and date-fns (173.47 to 174.47 MiB).
All workloads, including regressions, appear below; this is not a general wall-time or memory improvement.

Each cell below is **baseline / candidate**, using independent medians. `1T` means `--singleThreaded`;
`default` uses the compiler's default checker count. Independent medians and medians of paired percentage
changes can differ when samples vary. The raw samples are retained with the local measurement artifacts.

Check-only xstate, mui-docs, formbricks-web, playwright and date-fns exit successfully. The remaining catalog
projects return the same nonzero status and diagnostics in both builds; each preflight confirms that source
files are loaded. All four emit configurations likewise have identical diagnostics and exit status across
the two Rust builds and the single-threaded Go reference.

### Emit

| project | 1T instructions (G) | 1T wall (s) | default instructions (G) | default wall (s) | default peak RSS (MiB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | 207.143 / 206.592 | 30.199 / 23.833 | 223.666 / 223.222 | 5.392 / 5.361 | 2881.50 / 2882.05 |
| xstate-main | 8.646 / 8.637 | 0.908 / 0.897 | 10.570 / 10.567 | 0.258 / 0.269 | 277.61 / 276.28 |
| webpack | 22.066 / 22.070 | 2.066 / 2.107 | 26.523 / 26.669 | 1.407 / 1.515 | 580.33 / 579.50 |
| mui-docs | 126.892 / 126.839 | 11.247 / 11.942 | 188.515 / 188.500 | 4.041 / 4.191 | 1830.83 / 1828.83 |

### Check-only catalog

| project | 1T instructions (G) | 1T wall (s) | default instructions (G) | default wall (s) | default peak RSS (MiB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | 81.842 / 81.815 | 8.584 / 8.261 | 92.787 / 92.812 | 1.883 / 1.879 | 2017.91 / 2020.14 |
| xstate-main | 6.076 / 6.075 | 0.639 / 0.635 | 7.939 / 7.900 | 0.224 / 0.214 | 245.11 / 244.61 |
| webpack | 10.771 / 10.762 | 1.113 / 1.102 | 15.067 / 15.076 | 0.249 / 0.255 | 477.77 / 476.78 |
| mui-docs | 37.615 / 38.589 | 3.385 / 3.489 | 92.780 / 92.727 | 1.813 / 1.768 | 1500.30 / 1495.39 |
| Compiler | 1.704 / 1.703 | 0.162 / 0.165 | 2.014 / 2.015 | 0.081 / 0.080 | 75.20 / 74.84 |
| Compiler-Unions | 2.900 / 2.899 | 0.236 / 0.239 | 3.589 / 3.588 | 0.121 / 0.123 | 80.52 / 80.13 |
| cal-diy | 31.742 / 31.777 | 3.130 / 3.200 | 74.507 / 74.434 | 1.588 / 1.582 | 1770.41 / 1761.98 |
| formbricks-web | 39.718 / 39.690 | 4.646 / 4.602 | 69.579 / 69.652 | 1.664 / 1.678 | 1857.00 / 1863.66 |
| supabase-studio | 44.982 / 45.087 | 4.605 / 4.622 | 77.247 / 77.190 | 1.733 / 1.716 | 1580.44 / 1578.08 |
| t3code-server | 39.752 / 39.757 | 4.271 / 4.264 | 126.487 / 125.264 | 3.020 / 2.943 | 1994.72 / 1978.47 |
| mikro-orm | 47.380 / 47.200 | 9.435 / 7.543 | 83.682 / 83.810 | 2.557 / 2.718 | 1855.77 / 1854.58 |
| next-packages-next | 17.767 / 17.771 | 3.286 / 3.222 | 22.296 / 22.359 | 0.729 / 0.723 | 631.81 / 636.00 |
| next-root | 18.582 / 18.570 | 2.924 / 2.945 | 21.551 / 21.622 | 0.816 / 0.844 | 491.47 / 490.84 |
| storybook | 9.981 / 9.982 | 1.886 / 1.896 | 16.681 / 16.815 | 0.509 / 0.499 | 466.78 / 468.52 |
| nuxt | 10.649 / 10.659 | 2.083 / 2.103 | 17.867 / 17.906 | 0.581 / 0.592 | 480.25 / 481.95 |
| playwright | 7.556 / 7.561 | 1.524 / 1.493 | 13.246 / 12.925 | 0.396 / 0.398 | 448.33 / 439.86 |
| drizzle-orm | 6.294 / 6.290 | 1.201 / 1.218 | 16.528 / 16.500 | 0.494 / 0.482 | 529.67 / 529.58 |
| typeorm | 7.774 / 7.771 | 1.433 / 1.449 | 8.819 / 8.797 | 0.323 / 0.347 | 245.33 / 245.89 |
| date-fns | 4.413 / 4.407 | 0.746 / 0.731 | 5.104 / 5.115 | 0.325 / 0.318 | 173.47 / 174.47 |
