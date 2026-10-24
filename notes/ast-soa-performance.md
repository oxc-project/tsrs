# AST migration performance checks

2026-10-23. Acceptance requires safe Rust with no repeatable performance or memory regression. These are
preparation measurements; the production compiler still allocates AST nodes in arenas.

## Exclusive legacy factory, before shared finalization

Baseline: `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1`, extracted with Git archive into
`/private/tmp/tsrs-ast-soa-base-19ba886`. Candidate: the uncommitted owned-storage foundation, exclusive legacy
`ParserFactory` and oracle repairs, before `construction.rs` was introduced. Both use the release profile with
`profile.release.debug=true` and `profile.release.strip=false` on the same Darwin arm64 host (12 logical CPUs).

| Binary | SHA-256 |
| --- | --- |
| `target/ast-soa-baseline/release/tsrs` | `0f1c00c32b2942a3a5adf8f6b14680771b5502dc976c967b46f06262d960299e` |
| `target/ast-soa-bench/preparation/tsrs` (retained candidate) | `863e0a80a78d9f260369d6290da47a8587705a67ca4c9327748b3958a87caef6` |

All 19 public catalog projects were available at their pinned commits. Runs used their cached working directories
and existing benchmark overlays, with `--noEmit --incremental false --pretty false`. No dependency installation,
benchmark preparation or project-file modification was performed. Baseline and candidate warmup output and exit
status matched for every project/mode. The old conformance rerun had completed before measurement started.

Hyperfine 2.0.0 collected wall time, peak RSS and retired instructions. Each project/mode had one warmup per
binary, then three pairs in baseline/candidate, candidate/baseline, baseline/candidate order. Instructions below
are single-threaded; RSS and wall time use the default checker count. Each value is the median paired percentage
change `(candidate / baseline - 1) * 100`, rather than a ratio of aggregate medians.

| Project | Single instructions | Default peak RSS | Default wall time |
| --- | ---: | ---: | ---: |
| Compiler-Unions | 0.007% | -0.136% | -0.39% |
| Compiler | -0.027% | 0.103% | 0.75% |
| cal-diy | -0.236% | -0.490% | 0.06% |
| date-fns | -0.270% | 0.160% | -15.36% |
| drizzle-orm | -0.009% | -0.119% | -2.45% |
| formbricks-web | -0.015% | -0.023% | -3.03% |
| mikro-orm | -0.025% | 0.337% | 1.21% |
| mui-docs | 0.014% | 0.270% | -1.20% |
| next-packages-next | 0.042% | -0.256% | 8.46% |
| next-root | 0.200% | 0.633% | 2.37% |
| nuxt | 0.100% | -2.484% | -3.66% |
| playwright | 0.010% | -0.129% | -0.23% |
| storybook | -0.023% | 0.296% | 0.16% |
| supabase-studio | -0.189% | -0.467% | -0.39% |
| t3code-server | -0.112% | 0.517% | -2.90% |
| typeorm | -0.453% | 1.695% | 24.31% |
| vscode | 0.002% | -1.574% | -2.39% |
| webpack | -0.022% | 0.449% | -3.01% |
| xstate-main | -0.108% | 0.006% | -0.93% |

**This screening does not pass the no-regression gate.** Several medians increased and three pairs do not
establish whether these are stable effects. In particular, next-packages-next instructions increased in all
three pairs (+0.005% to +0.095%), while typeorm default RSS increased in all three (+0.954% to +2.342%). Those
effects require further investigation. Default instructions vary with scheduling and are not the single-threaded
instruction gate. Wall times, including the large typeorm increase, also require repetition before attribution.
Improvements elsewhere cannot compensate for a repeatable regression.

Raw Hyperfine JSON, warmup output and host/binary metadata are under `target/ast-soa-bench/preparation/` (ignored
local artifacts). Reproduction scripts are `target/ast-soa-bench/compare.mjs` and `summarize.mjs`. A single sample
uses, from the catalog project's working directory:

```sh
hyperfine --runs 1 --metrics time_wall_clock,memory_peak_resident,instructions \
  --ignore-failure=1,2 --export-json /private/tmp/ast-sample.json \
  '/absolute/path/to/tsrs -p /absolute/path/to/config --noEmit --incremental false --pretty false --singleThreaded'
```

Repeat with alternating matched binaries after warmups; omit `--singleThreaded` for default-checker RSS. Keep
the binary hashes and workload pins with the results. The newly shared finalization routine changes production
code and requires its own rebuilt candidate and measurements before acceptance.

## Shared parser finalization

The next candidate includes `construction.rs`, production parser finalization through its mutable writer, and
the ten-input deferred binder contract. The release build was verified fresh after all generators completed.
It is retained at `target/ast-soa-bench/finalization/tsrs`, SHA-256
`d52d2f2ddf0f12831014105b4217f65c6fd6fe4bf4d3b1a69bbfc5f67320182c`. The baseline hash/configuration remain the same.
Changed and untracked sources are saved in `target/ast-soa-bench/finalization-source.tar.gz`, with the base commit
and per-file hashes in `finalization-source.json`; the benchmark metadata embeds that manifest.

Both checker histories passed all 15,448 variants (13,700 diagnostic passes, 1,748 skips) at a 600-second test
limit, and the baseline passed the known slow case at that limit too. Focused parser/binder tests, the two owned
finalization Miri cases, workspace Clippy/source lint, generation and a fresh 2,000-case differential fuzz run
passed. The extended test timeout is separate from runtime benchmark acceptance.

The balanced run completed after all compiler/test jobs ended: six pairs per project/mode, with three pairs in
each binary order, after warmups. All 19 projects' warmup output and exit status matched. The local report
script validated all 456 measurements and both retained binary hashes. Only research/progress notes changed
after the candidate source snapshot; its Rust and generator sources still match that snapshot.

| Project | Single instructions | Default peak RSS | Default wall time |
| --- | ---: | ---: | ---: |
| vscode | -0.412% | 0.983% | -1.28% |
| xstate-main | -0.097% | -0.047% | -0.30% |
| webpack | -0.062% | 0.590% | -1.00% |
| mui-docs | 0.505% | -0.734% | -0.33% |
| Compiler | -0.041% | 0.248% | -1.42% |
| Compiler-Unions | -0.014% | -0.155% | -0.14% |
| cal-diy | -0.055% | 0.947% | 0.03% |
| formbricks-web | -0.130% | 0.070% | 0.37% |
| supabase-studio | 0.298% | -0.084% | 0.27% |
| t3code-server | -0.052% | 0.338% | 1.90% |
| mikro-orm | -0.039% | 0.207% | 2.70% |
| next-packages-next | -0.095% | -0.790% | 3.63% |
| next-root | 0.485% | -1.649% | 1.61% |
| storybook | -0.245% | -0.335% | 0.52% |
| nuxt | -0.101% | -0.008% | 2.41% |
| playwright | -0.155% | -0.467% | -3.49% |
| drizzle-orm | -0.214% | -0.042% | 0.12% |
| typeorm | 0.987% | 0.022% | -0.64% |
| date-fns | 0.491% | -0.204% | -4.29% |

Values use the same median paired percentage change as above. Most increases changed sign across the six
pairs. The exception among the gated metrics was Compiler default RSS: every pair increased (+0.062% to
+0.642%, median +0.248%). The focused repeat and identical-binary control below do not yet attribute this
small increase. Other positive medians, especially instructions and wall time, remain unresolved; six noisy
pairs do not establish their absence or cause. Do not use improvements on other workloads to clear them.

**The no-regression gate remains unpassed.** These are Darwin arm64 preparation measurements. They do not
measure an owned production parser/binder or repeated LSP/watch retention. Production AST allocation remains
the legacy arena model, and no production cutover is accepted by these results.

Raw files are under `target/ast-soa-bench/finalization/`, with the validated table and per-metric ranges in
`report.json`. Reproduce the summary with `node target/ast-soa-bench/report.mjs`; the earlier `summarize.mjs`
also prints all pairs' ranges and can inspect incomplete runs. These are ignored local artifacts.

### Compiler repeat and measurement control

After the catalog completed, Compiler ran another 12 alternating pairs per mode against the same retained
candidate. Then an identical-binary control ran 12 pairs per mode, using the baseline binary in both positions.
Both runs matched warmup output/status and every pair's exit status. No compiler builds or tests overlapped.

| Comparison | Single instructions | Single peak RSS | Default peak RSS | Default wall time |
| --- | ---: | ---: | ---: | ---: |
| Baseline → candidate | -0.040% | 0.097% | 0.062% | 0.47% |
| Baseline → same baseline | 0.028% | 0.028% | -0.124% | -1.43% |

Default candidate RSS increased in 8/12 pairs, with a median absolute increase of 48 KiB and a range from
-512 KiB to +1,680 KiB. The control increased in 5/12 pairs, with a median -96 KiB and a range from -704 KiB to
+848 KiB. The initial default-RSS result is therefore not sufficient to identify a repeatable candidate effect.
Single-threaded candidate RSS increased in 11/12 pairs (median +56 KiB), against 7/12 (+16 KiB) in the control;
that small possible cost also remains open. These repetitions do not turn the no-regression gate into a pass.

The control illustrates why isolated positive medians cannot establish causation. It also prevents treating
small changes in hardware instructions or wall time as established improvements. Remaining instruction/wall
increases from the catalog need focused follow-up under stable conditions before production acceptance.
The owned compiler and retained-memory lifecycle measurements are still absent.

Raw measurements and metadata are in `target/ast-soa-bench/finalization-compiler-repeat/` and
`finalization-compiler-control/`. Inspect them with `TSRS_SOA_STAGE=<stage> node
target/ast-soa-bench/summarize.mjs`. Both used `TSRS_SOA_PAIRS=12` with `compare.mjs Compiler`; the repeat selected
the retained finalization candidate, while the control set `TSRS_SOA_CANDIDATE` to the baseline path.

## Shared scanner: rejected first code generation

The shared scanner/regexp text-policy candidate, before shared lexical grammar, is retained at
`target/ast-soa-bench/scanner/tsrs`; its sources and hashes are in
`target/ast-soa-scanner/scanner-source.{tar.gz,json}`. It is compared with the retained finalization binary,
using the same release/debug/strip configuration and Hyperfine metrics above. Both checker histories passed
full conformance with the actual 600-second argument; scanner streams matched pinned Go over 13,081 inputs.

Six balanced pairs on Compiler and Compiler-Unions showed instruction increases in every single-checker pair.
An independent twelve-pair repeat confirmed the pattern; an identical-finalization-binary control did not.
Values below are median paired changes. These are two-workload checks, not the full catalog gate.

| Comparison | Workload | Single instructions | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: |
| Scanner repeat | Compiler | +0.516% | -0.103% | +2.66% |
| Identical-binary control | Compiler | +0.042% | -0.185% | -4.28% |
| Scanner repeat | Compiler-Unions | +0.333% | -0.252% | +2.68% |
| Identical-binary control | Compiler-Unions | -0.035% | +0.068% | -0.84% |

All twelve repeat instruction pairs increased: Compiler +0.159% to +1.046%, Compiler-Unions +0.094% to
+0.691%. Control ranges were -0.093% to +0.458% and -0.105% to +0.122%, respectively. RSS/wall findings
remain noisy; the instruction result is enough to reject this first production preparation candidate.
No unrelated improvement can compensate for it.

Release disassembly shows that the generic associated byte-handler table is read directly from constant data;
it is not rebuilt on the stack. The changed inlining boundary is a concrete suspect: the original scanner has
an out-of-line `Scanner::scan`, while the generic loop is embedded into parser token helpers, changing their
frames and handler instantiations. Assembly artifacts are in `target/ast-soa-scanner/*assembly*.txt`.
An isolated candidate changes only `ScannerImpl::scan` to `#[inline(never)]` on top of the scanner snapshot;
its provenance is in `outline-source.json`. The following measurements reject that correction too.

Raw checks are in `target/ast-soa-bench/scanner-{screen,repeat,control}/`, using the local
`compare-scanner.mjs` baseline override. No compiler builds or tests overlapped the twelve-pair repeat/control.
The no-regression gate remains unpassed; the full owned compiler and retained-memory lifecycle are not measured.

### Outlined loop and shared lexical grammar screens

Both release builds completed with the same debug/strip configuration. Retained binaries:

| Candidate | SHA-256 |
| --- | --- |
| `target/ast-soa-bench/scanner-outline/tsrs` | `f40dd725523bf1ccce5c31f8edb5e71e028468fe3c4464fbd1200feec0b17e75` |
| `target/ast-soa-bench/lexical/tsrs` | `1ccb53b3f6bc54f8e45dfaf8a2831ff999a40542718d57d87202c7e2be4d016a` |

Twelve alternating pairs per mode compared the outlined scanner with the retained finalization predecessor,
then shared lexical grammar with that outlined scanner. Warmup output/status and measurement exit codes matched.
No compiler builds or tests overlapped these measurements. Paired median changes:

| Comparison | Workload | Single instructions | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: |
| Finalization → outlined scanner | Compiler | +0.506% | +0.083% | -13.22% |
| Finalization → outlined scanner | Compiler-Unions | +0.270% | -0.194% | -0.18% |
| Outlined scanner → shared lexical grammar | Compiler | +0.089% | -0.186% | -4.28% |
| Outlined scanner → shared lexical grammar | Compiler-Unions | +0.039% | -0.232% | +29.79% |

Compiler instructions increased in all twelve outlined-scanner pairs (+0.045% to +0.739%); Compiler-Unions
increased in ten (+0.270% median, -0.258% to +0.554%). Outlining the loop alone therefore does not resolve the
earlier scanner regression. The lexical-only instruction samples were mixed (8/12 and 6/12 positive), and
neither comparison clears the no-regression gate. Wall results varied substantially between samples; these
screens establish no speedup or accepted wall-time result. Focused repeats and controls remain necessary.

The outlined scan loop has the same 66 static instructions as the predecessor. Helper compilation still differs:
for example the predecessor string-byte handler calls `scan_string`, while the generic handler contains its
fast path. Static instruction counts describe code shape, not executed cost. An isolated concrete legacy
`Scanner` wrapper is being built to test keeping dispatch compilation inside the scanner crate. Its source
manifest is `target/ast-soa-scanner/facade-source.json`; no result is accepted for that experiment yet.

Raw screens are in `target/ast-soa-bench/scanner-outline-screen/` and `lexical-screen/`, with logs in
`target/ast-soa-scanner/benchmark-{outline,lexical}-screen.log`. Source provenance remains in
`outline-source.json` and `lexical-source.{tar.gz,json}`; the wrapper experiment does not alter the working tree.

### Concrete legacy scan wrapper: rejected

The isolated wrapper build completed. Retained `target/ast-soa-bench/scanner-facade/tsrs` has SHA-256
`160be63c87b557e567c7efe4182524a01a24e0a59b6d6ef2cc040e0ef5734cee`. It wraps the generic scanner in one
concrete `Scanner` type, with a non-inlined `scan` method. All other methods still resolve through the generic
implementation. This preserves the scan boundary, but does not preserve every scanner entry point's compilation
boundary. It is a scratch experiment; no wrapper is present in the working tree.

Twelve alternating pairs against the retained finalization predecessor, without overlapping builds/tests:

| Workload | Single instructions | Default peak RSS | Default wall |
| --- | ---: | ---: | ---: |
| Compiler | +0.495% | +0.062% | +1.21% |
| Compiler-Unions | +0.304% | +0.039% | -6.66% |

Single-checker instructions increased in 11/12 Compiler pairs and all 12 Compiler-Unions pairs. This wrapper
also fails the gate. No wall/RSS improvement is established. Its results are in
`target/ast-soa-bench/scanner-facade-screen/`, with `facade-source.json` recording exact sources.

The same twelve-pair check with `--noCheck --noEmit` localizes most of the absolute instruction increase to the
front end: +0.932% on Compiler and +0.862% on Compiler-Unions. The middle paired absolute increases are roughly
8.7M and 7.6M instructions, compared with 8.9M and 9.4M for full checking. `--noCheck` still includes Program
construction; it is not a scanner-only measurement. All warmup outputs/status and measurement exit codes match.
Raw results are in `scanner-facade-nocheck/`, using `compare-phase.mjs` with `TSRS_SOA_FLAGS='["--noCheck"]'`.

The source-manifest comparison also confirms that parser production files are identical between finalization
and the scanner snapshot. Other changes are in unused owned storage and its generator/tests, plus scanner
code. Those unused-source changes are still part of these complete-binary comparisons; the result does not
prove an isolated source-line cause. The next scratch candidate adds wrappers for the other six large public
scanner entry points. A separate common scanner-only harness is being built for further localization. Neither
has an accepted result yet.

### Snapshot copying: concrete code-generation cost

A common scanner-only harness (`target/ast-soa-scanner/scan-cost-source.rs`) makes saved-state round trips
observable with `black_box`, and also scans identifiers, strings, numbers, punctuation and public Compiler
source. Its baseline uses the original monomorphic scanner; its first candidate uses the isolated concrete
scan wrapper. Both have the same harness text and release/debug/strip settings. Their matching token counts
and checksums are checked before measurement.

The round-trip helper has 16 static instructions with the original scanner and 22 with generic fieldwise
`ScannerStateImpl::clone`. The latter separately loads/stores fields instead of copying the entire state.
`ScannerText::snapshot` now defaults to Clone and the arena text policy overrides it with direct Copy.
The owned policy still clones its owned text, preserving independent speculation snapshots. With direct Copy,
the same helper returns to 16 instructions. Saved disassemblies are `snapshot-{baseline,facade,copy}.asm`.

Twelve alternating pairs compare the **same wrapper scanner before and after only that snapshot change**:
the explicit snapshot workload reduces instructions by 2.856%, with every pair between -2.929% and -2.640%.
Median RSS is unchanged. The scanner workloads without snapshots have mixed instruction differences around
zero; they establish no separate improvement. This is a localized probe, not the compiler no-regression gate.
Results are under `target/ast-soa-bench/scanner-copy-cost/`; exact candidate changes are recorded in
`target/ast-soa-scanner/copy-snapshot-source.json`.

The first `scanner-cost/` comparison has a material context limitation: the original scanner loop is inlined
into the tiny standalone harness, whereas the wrapper's attribute forces an out-of-line loop. The production
predecessor already has an out-of-line scanner loop. Do not interpret that probe's larger scanner instruction
increases as a measured cause or size of the complete compiler regression. The snapshot-only sibling comparison
above holds this compilation boundary fixed.

The working tree now uses the snapshot policy and removes the unsuccessful loop-inlining override. It retains
the generic legacy scanner alias; neither scratch wrapper is adopted. Current parser/scanner library tests,
workspace Clippy and source lint pass. Generator regeneration changes no Rust/TS/TOML source hashes from the
new `lexical-copy-source.{tar.gz,json}` snapshot. Fresh complete compiler measurement remains pending.

The six-entry-point wrapper also finished building, retained as
`target/ast-soa-bench/scanner-entries/tsrs` with SHA-256
`c9be9044118b5146b5eb5d95e5dc779f0077e75c8636f791351ea47fafdeee58`. It is unmeasured and unaccepted; work
shifted to the concrete snapshot-copy finding. Its manifest is `facade-entries-source.json`. The earlier
facade scratch root has since been reused for the copy experiment; its pre-copy scanner source is retained at
`facade-scanner-before-snapshot.rs`, and each manifest records the relevant hashes.

### Generic production lowering with Copy snapshots: rejected

The full lexical/Copy candidate is retained at `target/ast-soa-bench/lexical-copy/tsrs`, SHA-256
`6d50c6ffe451b3855c6aec89dcde3ff70db545f877f5706202319246344b5d3e`. Twelve alternating pairs per mode against
the finalization predecessor still increase single-checker instructions in every pair on both workloads:

| Workload | Single instructions | Default peak RSS | Default wall |
| --- | ---: | ---: | ---: |
| Compiler | +0.485% | -0.041% | -13.40% |
| Compiler-Unions | +0.270% | +0.254% | +25.63% |

Compiler instruction ranges are +0.193% to +0.895%; Compiler-Unions +0.082% to +0.924%. Snapshot Copy does not
resolve the compiler regression. Wall results remain highly variable and establish no speedup. No build/test
work overlapped this screen. Raw results are `target/ast-soa-bench/lexical-copy-screen/` with the source manifest
already described above. Default full conformance passed after Copy; canonical was not rerun for that candidate.

### Separate concrete production lowering: experimental

The next candidate keeps one algorithm source in `scanner_impl.rs` / `regexp_impl.rs` and generates concrete
legacy methods with `tools/gen-scanner.mjs`, now registered in the regeneration check. Production has concrete
`Scanner`, `ScannerState` and regexp types. The owned implementation still compiles the generic text policy.
This tests all production method compilation boundaries, rather than wrapping selected generic entry points.

After formatting, the generated concrete regexp algorithm is byte-for-byte identical to Git HEAD except its
generated-file header. The scanner implementation differs only by that header, two unused text-observation
helpers and `slice` visibility. Its dispatch table and state declarations retain their original concrete form.
The shared parser grammar reads token observations through an associated scanner type without a state snapshot.
The scanner oracle now explicitly runs actual production `Scanner`, rather than a generic arena-policy scanner.

Both scanner implementations match pinned Go over 13,081 public files / four passes each, zero differences;
eight invalid-UTF-8 inputs are excluded as before. The 27 parser/scanner tests, workspace Clippy with warnings
denied, source lint and registered generator regeneration pass. Full compiler conformance and matched release
measurements are in progress. No performance or memory acceptance is claimed for this experimental split.

The retained release binary is `target/ast-soa-bench/scanner-concrete/tsrs`, SHA-256
`7cfcb1dcfca190bec518d6c5e9216213a2ee3e3313bc0e5a5aedf0f51d244e37`. Build settings match the predecessor.
Its changed/untracked inputs are archived in `target/ast-soa-scanner/concrete-source.{tar.gz,json}`. A subsequent
generator-only dispatch-consistency check changes no Rust output; all compiled Rust source hashes still match
the archived snapshot. Registered regeneration and formatting changed none of 51 source/generator hashes.
The default-history full run passed all 15,448 variants with zero differences/crashes/timeouts in 306.5 seconds
(while the release build was active, not a performance result). Diagnostic passes remain 13,700, with no lost
entries. Per-category pass lists are retained in `concrete-default-pass-sets/`; the earlier saved before/after
pass-list comparisons cover diagnostics, not independently archived sets for every baseline category.

Canonical conformance also passed all 15,448 variants, zero failures/crashes/timeouts, in 308.9 seconds. It lost
no diagnostic passes against the saved canonical run. The new default/canonical pass sets match in each supported
category: diagnostics 13,700; types/symbols 12,998 each; JS 13,626; JS maps 149; source-map records 156. Logs and
per-category sets are under `target/ast-soa-scanner/concrete-*`. All 82 AST tests, 13 parser tests, 14 scanner
tests, the ten-input deferred binder contract and 32-client borrowing rejection check pass. The isolated-name
oracle also matches Go on all 2,163 inputs for both contexts.

Two independent twelve-pair screens against the finalization predecessor, followed by an identical-binary
control, give these median paired changes. No builds/tests overlapped any of these measurements.

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Concrete screen | Compiler | +0.063% | -0.028% | -0.258% | +1.56% |
| Concrete repeat | Compiler | +0.049% | -0.056% | -0.062% | -0.06% |
| Identical-binary control | Compiler | -0.078% | 0.000% | -0.082% | +0.85% |
| Concrete screen | Compiler-Unions | +0.025% | +0.312% | +0.165% | +0.35% |
| Concrete repeat | Compiler-Unions | -0.002% | +0.286% | -0.029% | -0.27% |
| Identical-binary control | Compiler-Unions | +0.029% | -0.013% | -0.362% | +8.37% |

The earlier generic scanner's large instruction increase is absent from these screens, but **this candidate
still fails the memory gate**. Compiler-Unions single-checker RSS increases in all 24 candidate pairs, with
median increases of 192 KiB then 176 KiB; the control has 3 positive, 6 negative and 3 equal pairs (median -8 KiB).
Compiler instructions remain slightly positive in both screens (10/12 then 8/12 positive); attribution is also
unresolved. There is no accepted wall-time improvement. Improvements elsewhere cannot offset the RSS increase.

Mach-O segment sizes are identical between these binaries, and candidate `__text` is 1,408 bytes smaller.
This rules out a simple increase in total segment size, not changed resident code pages or stack/heap effects.
One extended-diagnostics run has identical semantic counters and slightly lower final candidate RSS; it is not
a peak-memory measurement or a contradiction of the repeated peak result. Memory profiling is the next step.
Raw measurements are `scanner-concrete-{screen,repeat,control}/`, with logs in `target/ast-soa-scanner/`.

The twelve-pair `--noCheck` probe (`scanner-concrete-nocheck/`) gives +0.060% / +0.053% single instructions on
Compiler / Compiler-Unions. Single RSS medians are 0.000% / +0.072%, with mixed signs on Compiler-Unions. This
restricted run has a much smaller memory difference; `--noCheck` also changes compiler policy, so it does not
by itself locate the full run's peak or prove that checking allocates extra data.

Profiling used separate copies with the `com.apple.security.get-task-allow` entitlement; the retained measurement
binaries were not modified. Instruments could not attach to the original linker-signed binary. The debug copies
recorded successfully (expected compiler exit 2). The VM Tracker export selected a startup map, not the measured
peak. A subsequent LLDB pair stopped both debug copies at `statistics_from_program` with extended diagnostics:
both maps report 368 KiB stack residency and 49.4 MiB rounded private footprint. This is one instrumented,
post-check pair with extra statistics enabled, not evidence that the peak RSS increase is absent. Raw maps,
debugger logs and traces are `target/ast-soa-scanner/concrete-*`.

Disassembly of the original/concrete binaries finds matching static instruction counts and stack frames for
identifier creation (350, 112 bytes), literal parsing (236, 128 bytes), and entity names (194, 112 bytes). The
shared token/keyword/next-token routines have four fewer static instructions each, with unchanged stack frames.
This does not establish executed cost; it does refute a simple larger text-descriptor frame in those routines.

A matched sibling experiment is building both variants in the same scratch checkout/build directory, changing
only the ten production lexical entry points and their two cold helpers back to the retained finalization
implementation. The owned grammar and concrete scanner remain present in both. Source paths and release options
are held fixed between the two builds. This isolates the production lexical integration without editing the
current workspace. Its source/three-file hashes and retained binary provenance are recorded in
`target/ast-soa-scanner/lexical-isolation-build.json`; no result or adoption is claimed yet.

### Matched production lexical isolation: no resolution

Both sibling builds completed in the same scratch checkout/target directory. The shared-routine binary has
SHA-256 `993ead63eb6336ae3030b65d5f1238da2b8feae3f443982e0751e2d6f0fb156e`; the restored-routine binary has
`f9ed7db1fd04a350155045d31e2b9c06d4b8bb2baa34906d789f45b020228f50`. These retain the archived concrete
snapshot; the later declaration-snapshot prototype is not present in either. Twelve alternating pairs per
workload/mode, with no build or test overlap, give the following median changes:

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Shared → restored lexical routines | Compiler | -0.032% | +0.014% | -0.021% | +0.11% |
| Shared → restored lexical routines | Compiler-Unions | -0.019% | 0.000% | -0.029% | -1.64% |
| Finalization → restored lexical routines | Compiler | +0.024% | +0.014% | 0.000% | +0.19% |
| Finalization → restored lexical routines | Compiler-Unions | +0.002% | +0.416% | -0.019% | +5.68% |

The direct sibling comparison has mixed single-RSS signs: Compiler 6 positive / 4 negative / 2 equal;
Compiler-Unions 5 positive / 3 negative / 4 equal. Restoring these routines does not establish a repeatable
memory reduction. Against the finalization predecessor, restored-routine Compiler-Unions RSS is higher in all
12 single-checker pairs (+0.052% to +0.678%). The preparation still fails the memory gate. Neither set establishes
a wall-time improvement or clears the instruction gate. No production parser methods were reverted.

Raw results are `target/ast-soa-bench/lexical-isolation-screen/` and `lexical-original-predecessor/`; build/source
provenance remains in `lexical-isolation-build.json`. The next scratch experiment changes only source organization:
it inserts the exact generated concrete scanner method text into `scanner.rs`, replacing the include. It keeps
the restored production parser routines and every scanner algorithm fixed. This tests whether that organization
changes emitted code/residency, not whether an assumed runtime allocation is present. Its first script attempt
failed before compilation because JavaScript string replacement interpreted a literal replacement marker in
scanner text; callback replacement fixed it. Source/parent hashes are in `scanner-organization-build.json`.
No result or adoption is claimed for that in-progress build.

### Scanner source organization: no resolution

The inline-method experiment completed; binary SHA-256 is
`ec028a65c892f37560d303350717e9a33b74e48c97812056228d20fa5f356abb`. Twelve alternating pairs per mode:

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Included → inline scanner methods | Compiler | +0.020% | -0.014% | -0.124% | +4.49% |
| Included → inline scanner methods | Compiler-Unions | +0.027% | 0.000% | -0.135% | +8.39% |
| Finalization → inline scanner methods | Compiler | +0.027% | 0.000% | -0.431% | -2.27% |
| Finalization → inline scanner methods | Compiler-Unions | +0.062% | +0.325% | -0.640% | +8.64% |

Sibling single-RSS signs are mixed: Compiler 4 positive / 6 negative / 2 equal; Compiler-Unions
4 positive / 5 negative / 3 equal. Against the predecessor, Compiler-Unions has 10 positive and 2 equal pairs,
with no reductions. Moving method text does not resolve the memory gate; the root generator/include is unchanged.
Wall variation does not establish a speedup. Raw stages are `scanner-inline-isolation/` and
`scanner-inline-predecessor/`. All builds/tests were terminal before each measurement.

### Broader parser inlining: experimental, no resolution

A whole-binary static instruction inventory found changed compilation outside the restored lexical entry points.
The predecessor outlines `parse_type_arguments` and inlines `parse_type_parameters`; the restored-routine
candidate instead outlines `parse_type_parameters` and a `parse_bracketed_list::<..., parse_type>` specialization.
Caller code changes too: `parse_type_reference` grows from 98 to 236 static instructions, with stack allocation
0x50 → 0x60; `parse_expression_with_type_arguments` grows from 119 to 256, 0x40 → 0x70. This is evidence of changed
inlining, not proof of executed cost or peak RSS attribution. The inventory and selected frames are in
`code-inventory-*.json`, `code-differences-*.json` and `code-inventory.log`. The demangler's numbered duplicate
suffixes create unmatched entries; do not interpret those as newly added runtime algorithms.

A scratch candidate adds `#[inline(never)]` to `parse_type_arguments` and `#[inline(always)]` to
`parse_type_parameters`, keeping all other inline-method experiment sources fixed. Its SHA-256 is
`39e94336612e70594b89168ac6be8cc1c55a73436169e5ecd316f8454dab4760`; provenance is `parser-boundaries-build.json`.
Again twelve alternating pairs, with no build/test overlap:

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Inline scanner → parser attributes | Compiler | -0.005% | -0.097% | -0.175% | +1.68% |
| Inline scanner → parser attributes | Compiler-Unions | +0.045% | -0.117% | -0.136% | +0.12% |
| Finalization → parser attributes | Compiler | -0.046% | -0.111% | -0.165% | +0.39% |
| Finalization → parser attributes | Compiler-Unions | +0.001% | +0.260% | -0.291% | -0.21% |

The sibling Compiler single-RSS decreases in all twelve pairs, but Compiler-Unions remains above the predecessor
at the median. No overall gate passes and the attributes are not adopted. Raw stages are
`parser-boundaries-isolation/` and `parser-boundaries-predecessor/`. This does not establish a wall-time improvement.

The next scratch experiment removes only the unused legacy `LexicalParser` implementation from the restored
production-routine variant, while keeping the owned grammar present. The parser attributes are first removed.
This tests whether unused generic preparation changes LTO's choices; it is not an assumed runtime allocation.
The root production/parser modules remain unchanged. Provenance is `unshared-legacy-build.json`; the build is
in progress, without a measurement or adoption yet. Later root declaration snapshots and owned token handoff
are absent from all these retained experiment binaries.

### Unused legacy lexical implementation: no resolution

The unused-implementation candidate completed with SHA-256
`dfffb7dd13662a2933590720ebf2f88d9e0dc87b653bdc7602719afeece0cf50`. Twelve alternating pairs per workload/mode,
with all builds/tests terminal, give these median changes:

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Inline scanner → unused adapter removed | Compiler | +0.027% | -0.042% | -0.175% | +6.30% |
| Inline scanner → unused adapter removed | Compiler-Unions | -0.028% | 0.000% | +0.068% | +0.66% |
| Finalization → unused adapter removed | Compiler | -0.018% | -0.042% | -0.093% | +0.43% |
| Finalization → unused adapter removed | Compiler-Unions | +0.028% | +0.403% | +0.068% | +0.13% |

Compiler single-RSS decreases in eleven sibling pairs and is equal in one. Compiler-Unions sibling signs remain
mixed (4 positive / 5 negative / 3 equal), and the direct predecessor comparison increases in all twelve pairs,
from +0.312% to +0.652%. Removing the unused legacy implementation does not resolve the memory gate. The root
implementation is unchanged; no candidate is accepted and no wall-time improvement is claimed. Raw stages are
`unshared-legacy-isolation/` and `unshared-legacy-predecessor/`, with source provenance in
`target/ast-soa-scanner/unshared-legacy-build.json`.

### Excluding owned modules and restoring the Scanner API: no resolution

Two more scratch variants retain the original production lexical routines and exact inline concrete scanner
methods. The first omits all experimental owned parser/scanner modules from legacy CLI compilation. The second
also changes the scratch generator to omit the owned-only `token_value_ref` / `token_value_source_range` methods
from concrete Scanner output. Regexp output remains unchanged. Neither includes the later root declaration/table
snapshots, token handoff or binding accounting, and neither is an owned compiler.

| Artifact | SHA-256 |
| --- | --- |
| `target/ast-soa-bench/legacy-modules-only/tsrs` | `cdba95e73e9984db532c7f872bfe7916259ab4c32522ffddd2efffee0986fd1d` |
| `target/ast-soa-bench/concrete-original-api/tsrs` | `cd015a98244e3eaec68cbf61c8335f3a64b531ce4b94839c2c2cfdd9ff576b5f` |

Both release builds and all 48 regressions completed before measurement. The root tests/Miri/Clippy and
conformance recovery were also terminal before any benchmark began. Each comparison uses twelve alternating
pairs for both checker modes, with matching compiler output/status and retained binary hashes:

| Comparison | Workload | Single instructions | Single peak RSS | Default peak RSS | Default wall |
| --- | --- | ---: | ---: | ---: | ---: |
| Unused-adapter removal → legacy modules only | Compiler | +0.002% | +0.083% | +0.320% | −1.71% |
| Same | Compiler-Unions | +0.002% | +0.052% | +0.116% | +0.30% |
| Finalization → legacy modules only | Compiler | −0.004% | +0.028% | +0.124% | +1.42% |
| Same | Compiler-Unions | −0.009% | +0.416% | +0.068% | −1.28% |
| Legacy modules only → original Scanner API | Compiler | −0.001% | 0.000% | −0.051% | +0.58% |
| Same | Compiler-Unions | +0.004% | 0.000% | +0.058% | +0.46% |
| Finalization → original Scanner API | Compiler | −0.006% | +0.042% | +0.155% | −1.58% |
| Same | Compiler-Unions | −0.004% | +0.416% | −0.165% | +2.30% |

Both direct predecessor comparisons increase Compiler-Unions single-checker RSS in all twelve pairs. Ranges
are +0.339% to +0.704% for module exclusion, and +0.026% to +0.704% for the original API. Module exclusion also
increases that workload's RSS in every sibling pair (+0.026% to +0.416%). Removing the two unused observations
has mixed sibling signs (4 positive / 2 negative / 6 equal). These changes do not resolve the memory gate.
No wall-time improvement is established, and no scratch change is adopted. Default-RSS reductions elsewhere
cannot compensate for the repeatable single-checker increase. A neutral comparison against finalization would
still not establish acceptance against the original repository baseline, whose own preparation gate is open.

Raw stages are `legacy-modules-only-{isolation,predecessor}/` and
`concrete-original-api-{isolation,predecessor}/` under `target/ast-soa-bench/`. Build/source provenance is in the
corresponding `target/ast-soa-scanner/*-build.json` files. Do not repeat source organization, unused-module or
unused-method removal without new evidence about actual executed code/residency. Continue the real owned
parser/binder and ownership feasibility work; these experiments do not prove that its eventual layout must
regress, nor do they authorize landing the current preparatory code.

### Template and recursive type grammar preparation: unmeasured

The later template-token integration and 23 recursive type routines are archived in
`target/ast-soa-scanner/type-grammar-source.tar.gz`, with hashes and correctness results in
`type-grammar-validation.json`. The template-token routines execute on both backends; only the complete
legacy parser currently implements the recursive `TypeParser` trait. No new node/store/parser fields were
added. Production grammar lowering changed, so unchanged algorithms and passing correctness tests do not
establish unchanged instructions, wall time or RSS. No release measurements of this slice or accepted owned
compiler comparison exist. The previously observed preparation regressions and the full catalog gate remain
open; this source is not authorized to land under the no-regression requirement yet.

### Type operands and signatures: unmeasured

The next preparation moves 18 more bodies into `TypeParser`, bringing its shared default-body count to 41.
Mapped/tuple/type literals, type parameters, function/constructor types and template-type spans now use explicit
backend operations. Optional-tuple flags/range/reparenting and lazy type-literal members preserve their existing
order and behavior. Only the legacy backend implements this recursive trait; real parameter/member/modifier,
JSDoc, import and expression dependencies remain. No node/store/parser fields were added.

Exact source and development binaries are retained in `target/ast-soa-scanner/type-operands-source.tar.gz` and
`type-operands-binaries/`, with validation provenance in `type-operands-validation.json`. No release instruction,
wall-time or RSS measurement of this slice exists. Generic extraction and different callback boundaries can
change generated code even with identical algorithms. Correctness passes establish no performance acceptance;
the earlier unresolved preparation increases and complete owned-compiler catalog/retention gate remain open.
No regression is approved for landing, including a temporary preparation regression.

The following signature preparation shares another 21 routines (62 total), covering real parameter/member/
modifier parsing and accessor/function-body entry. It replaces direct modifier-range access with a copied range
and keeps modifier-list creation under exclusive access to the shared key stack. The hot modifier fast path
and out-of-line worker retain their attributes; no parser/node/store fields were added. This is still a legacy
production compiler with an incomplete owned parser. Source and development binaries are retained under the
`type-signatures` stem. There are no release instruction/RSS/wall measurements for this slice either. Full
correctness validation, source equality and unchanged representations do not clear the no-regression gate.

## Current type/import/expression preparation

2026-10-24. The current preparation includes 89 shared type/signature/expression bodies and the experimental
owned storage/scanner/parser foundations. Production still uses arena ASTs. Both full conformance modes pass
all 15,448 variants without differences, crashes, timeouts or lost predecessor passes; those correctness
results do not establish performance acceptance.

The original baseline remains `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1`, with release binary SHA-256
`0f1c00c32b2942a3a5adf8f6b14680771b5502dc976c967b46f06262d960299e`. The current release candidate is retained
at `target/ast-soa-bench/type-expressions/tsrs`, SHA-256
`f0f1c585eb0a5e4fb6383e358438b563568d21365ddb4298080a627fc6dbdc1f`. Both use the release profile with
`profile.release.debug=true` and `profile.release.strip=false`. The current build uses rustc 1.99.1; its
command, toolchain, build log and source/configuration hashes are in
`target/ast-soa-scanner/type-expressions-release.json`.

All builds and correctness runs completed before measurement. On the same Darwin arm64 host with 12 logical
CPUs, Hyperfine 2.0.0 collected six alternating baseline/candidate pairs per project and checker mode, after
one warmup for each binary. All 19 pinned public catalog projects completed: 456 measurements with matching
warmup output/exit status and revalidated binary hashes. The flags remain `--noEmit --incremental false
--pretty false`, adding `--singleThreaded` for the single-checker runs. These are median paired percentage
changes; peak RSS and instructions were collected for both modes.

| Project | Single instructions | Default peak RSS | Default wall time |
| --- | ---: | ---: | ---: |
| vscode | +0.809% | -0.434% | +0.03% |
| xstate-main | -0.049% | +0.100% | -0.27% |
| webpack | +0.006% | -0.207% | +1.69% |
| mui-docs | -0.039% | +0.097% | +0.31% |
| Compiler | +0.040% | +0.052% | -0.94% |
| Compiler-Unions | +0.049% | +0.048% | -0.73% |
| cal-diy | +0.080% | -0.237% | -0.09% |
| formbricks-web | -0.018% | +0.040% | +0.15% |
| supabase-studio | +0.022% | -0.176% | +0.24% |
| t3code-server | +0.004% | +0.706% | -12.70% |
| mikro-orm | +0.008% | +0.558% | +6.42% |
| next-packages-next | +0.011% | +0.198% | -2.08% |
| next-root | -0.115% | -0.077% | +0.59% |
| storybook | -0.065% | +0.853% | -0.48% |
| nuxt | +0.037% | -1.657% | +1.10% |
| playwright | -0.029% | +0.346% | -2.20% |
| drizzle-orm | +0.201% | +0.150% | -0.51% |
| typeorm | -0.097% | -0.080% | -2.89% |
| date-fns | -0.209% | -0.427% | +2.37% |

VS Code single-checker instructions increase in every pair (+0.003% to +1.239%). Compiler-Unions
single-checker instructions also increase in every pair (+0.010% to +0.075%), as does that workload's
single-checker peak RSS (+0.391% to +0.679%, median +0.443%). The latter is not visible in the default-RSS
column above; reductions in another mode or project cannot compensate for it. Other positive medians remain
open as well. The substantial run-to-run variation prevents attributing every positive median to this change,
or claiming a speedup from the negative wall-time medians.

**The preparation fails the memory gate.** The focused follow-up completed twelve single-checker pairs on
VS Code and Compiler-Unions, followed by twelve identical-baseline control pairs on those same projects.
All 96 follow-up measurements passed output/status and binary-identity checks, without overlapping builds or
tests. Final verification also rechecks the 71 source files, three configuration files, development binaries
and retained release binaries against their recorded hashes.

| Comparison | Workload | Single instructions | Single peak RSS | RSS increase pairs | Median RSS bytes | Single wall |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Original baseline → candidate | vscode | -0.006% | -0.071% | 5/12 | -1,171,456 | -0.13% |
| Identical baseline control | vscode | -0.306% | -0.153% | 5/12 | -2,523,136 | -0.63% |
| Original baseline → candidate | Compiler-Unions | +0.030% | +0.430% | 12/12 | +270,336 | +0.06% |
| Identical baseline control | Compiler-Unions | -0.003% | 0.000% | 5/12 | 0 | -0.08% |

VS Code's initial instruction increase does not repeat consistently: the candidate repeat has six positive
and six negative pairs, ranging from -1.047% to +0.984%. The identical-binary control ranges from -1.122%
to +1.132%. These results establish neither a repeatable VS Code instruction regression nor an improvement.
Compiler-Unions instruction signs are also mixed (8/12 positive versus 5/12 for the control); the small
positive median remains unresolved.

Compiler-Unions memory is different: the candidate repeat increases in every pair, by 32 to 416 KiB, with
a median of 264 KiB. The control has five positive and seven equal pairs, ranging from 0 to 144 KiB with a
zero median. Together with the six-pair screen, this is a persistent preparation cost that must be resolved
before landing. The measurements do not identify a responsible field, allocation or code-generation choice.
The earlier source-organization, unused-module and inlining experiments above remain rejected; this result
does not justify repeating them without new causal evidence. No regression is approved as temporary overhead.

The complete raw catalog data, warmup outputs, metadata and six-pair report are under
`target/ast-soa-bench/type-expressions-screen/`; the direct report is
`target/ast-soa-scanner/type-expressions-benchmark-report.log`. Follow-up data are in
`type-expressions-repeat/` and `type-expressions-control/` under the same benchmark directory. The checked
follow-up results and raw measurement hashes are in `target/ast-soa-scanner/type-expressions-followups.json`.
This measures legacy preparation; the whole-compiler owned backend and retained-memory lifecycle remain
unmeasured, and all other open no-regression obligations remain in force.

## Subsequent expression/arrow/JSX extraction

The next source change moves another 61 real routines behind copied-handle reads and exclusive writers,
bringing `TypeParser` to 150 default bodies (including the extracted arrow-return-type error helper).
It adds no persistent node/store/parser field and preserves the existing rare JSX replacement-list allocation.
This substantial grammar/call-graph change is not the 89-body binary measured above. It has no matched release
instructions, wall-time or peak-RSS result yet. Its parity evidence uses the `expression-completion` stem under
`target/ast-soa-scanner/`; that does not resolve or transfer the prior binary's repeatable RSS cost. No landing
or owned-compiler no-regression acceptance is claimed. Continue real owned grammar/binding integration; do not
repeat rejected inlining/source-organization probes merely because this new preparation exists.

## Subsequent statement/declaration extraction

Another 82 real grammar routines now share copied handles and explicit writers: statements, binding patterns,
initializers, complete classes/declarations, imports/exports and decorator expressions. Four modifier/heritage
helpers bring `TypeParser` to 236 default bodies; its implementation remains legacy-only. Heritage conversion
and ambient mutations copy keys before construction/writes, without adding normal-path list copies or persistent
parser/node/store fields. The existing diagnostic suggestion allocations remain. Invalid-name diagnostics keep
the cooked scanner value intact instead of transferring it away before recovery.

This source differs from both the measured 89-body binary and the subsequently validated 150-body source. It
has no matched release instruction, wall-time or RSS measurement. Neither structural similarities nor passing
legacy parity checks establish the no-regression requirement. The preceding repeatable memory increase remains
unresolved, and the actual owned compiler and its retention costs remain unmeasured. This is experimental
integration work; no production landing or performance/memory acceptance is claimed.
