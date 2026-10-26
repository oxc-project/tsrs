# Further collection trials (2026-10-26)

Retained: three four-index SmallVec union-merge buffers and SmallBitVec inference-match flags. Ten other trials
were not retained. The combined change reduces median single-threaded instructions on 17/18 catalog projects in
each checker mode; wall time and peak RSS remain mixed.

This follows the [collection audit](data-structure-review.md), using `5b0ebd42` as the baseline. That baseline
already includes the four accepted SmallVec scratch buffers. The direct hashbrown checker-map replacement remains
rejected; these trials target individual allocation sites or a borrowed-key API instead.

## Allocation census and layout

A temporary instrumented dev build counted buffer lengths and final capacity on Compiler, Compiler-Unions,
xstate-main, webpack, vscode, mikro-orm and t3code-server, each single-threaded with `TSRS_FAST=0` and `1`.
Instrumentation was removed before building the release candidates. Counts below sum the seven projects.
“Allocated buffers” means a nonzero final heap capacity, not the number of allocator calls; a buffer may grow
several times. The final-length histogram estimates which buffers could fit inline.
An explicit capacity request can still force a spill even when the final length fits; in particular, the
next-round merge buffer sometimes reserves one spare entry. These counts are potential coverage, not measured
allocation reductions from the candidates.

| Site | Fast allocated buffers | Reference allocated buffers | Reference nonempty buffers fitting four entries | Largest length |
| --- | ---: | ---: | ---: | ---: |
| Inference matched sources | 127,665 | 191,696 | 190,983 | 107 |
| Inference matched targets | 127,665 | 191,696 | 190,970 | 134 |
| Inference unmatched sources | 34,235 | 34,734 | 34,457 | 50 |
| Inference matched flags | 181,174 | 227,082 | 225,802 | 616 |
| Union run starts | 505,965 | 1,956,552 | 1,930,583 | 236 |
| Union merge bounds | 174,793 | 282,209 | 208,157 | 238 |
| Union next-round bounds | 299,491 | 495,132 | 459,823 | 120 |
| Mixin flags | 38,282 | 38,324 | 37,606 | 17 |
| File-include reasons | 19,448 | 19,448 | 14,968 | 12,181 |
| Deferred type-argument checks | 21 | 0 | 0 | 52 |
| Diagnostic collision lists | 6 | 6 | 6 | 1 |

The resolver census found 115,821 module-cache hits and 75,803 misses, versus only 137 type-reference-cache hits
and 75 misses. These counts were identical between checker modes. Module lookups already avoid owned string
keys; their candidate replaces a trait-object probe with a concrete `hashbrown::Equivalent` probe. Type-reference
lookups additionally defer owned key construction until a miss. All key fields, sharding, locks, tracing bypass,
and the distinction between load-or-store and replacement are preserved.

Native layout probes using the actual `P<T>` type and SmallVec's `union` feature:

| Representation | `Vec<P<T>>` | Four inline handles | Eight inline handles |
| --- | ---: | ---: | ---: |
| Compressed, 4-byte handles | 24 B | 24 B | 40 B |
| Plain, 8-byte pointers | 24 B | 40 B | 72 B |

`SmallVec<[bool; 16]>` is 24 B; `SmallBitVec` is 8 B. For `usize` merge metadata, capacities 4, 8 and 16 occupy
40, 72 and 136 B respectively. This is a stack-size tradeoff for scratch buffers and a per-value memory tradeoff
for long-lived maps. The largest observed lists exercise heap spill paths.

The bit-vector trial uses [smallbitvec 2.6.1](https://docs.rs/smallbitvec/2.6.1/smallbitvec/struct.SmallBitVec.html),
whose native inline capacity is 62 flags (30 on wasm32). Its safe indexing and `set` retain bounds checking.
An isolated dependency check passed on wasm32-wasip1. The resolver uses the existing
[hashbrown 0.17.1 Equivalent contract](https://docs.rs/hashbrown/0.17.1/hashbrown/trait.Equivalent.html): borrowed
and owned keys must hash identically; ordinary `Borrow` lookups remain supported through its blanket implementation.

A separate xctrace Time Profiler recording of the baseline on vscode, reference mode and one checker, captured
8,090 stack samples. `infer_to_multiple_types` appeared in 169 samples, `infer_from_matching_types` in 60 and
`add_types_to_union` in 36. These inclusive samples include their callees and are not allocation costs; the census
and paired end-to-end measurements determine whether the storage changes are useful.

## Measurement method

Host: Apple M3 Max, 14 cores and 36 GB RAM; rustc 1.99.1 on aarch64-apple-darwin.

Baseline and candidates use identical release configuration, full debug symbols and no stripping:

```sh
cargo build --release -p tsrs_cli --config profile.release.debug=true --config profile.release.strip=false
```

Each isolated screen compares seven projects with three interleaved pairs, after a diagnostic-output warmup of
each binary. Fast mode uses one checker for instructions; reference mode uses the default checker count for peak
RSS and wall time. Reference defaults use four checkers; fast defaults use the machine/file-count rule (four to
eight on this host). Hyperfine 2 records hardware instructions, peak RSS and wall time. Timed measurements do not
overlap builds, profiling, census collection or correctness runs. Diagnostic bytes and exit status must match.

Local source snapshots, candidate patches, binaries, census, layouts, profile and raw measurement JSON live in
`bench/.work/data-structures-round3-20261026/`. `select.mjs` refuses to switch variants over an unrecorded edit.
`paired.mjs` records binary hashes, configuration hashes, machine details and every sample. The second
confirmation batch reverses which binary runs first.

## Isolated screens

The instruction column is the median across projects of each project's median paired percentage change.
RSS is the range of changes between the baseline and candidate medians with reference-default checkers.
These short screens select candidates for confirmation; they do not establish the final performance claim.

| Trial | Median paired instructions | Projects lower in all three instruction pairs | Default peak RSS range | Disposition after screening |
| --- | ---: | ---: | ---: | --- |
| Inference, four handles | -0.0001% | 1/7 | -0.441% to +0.233% | Confirm |
| Inference, eight handles | -0.017% | 2/7 | -0.134% to +0.602% | Confirm |
| Merge metadata, four indexes | -0.042% | 4/7 | -0.452% to +0.101% | Confirm |
| Merge metadata, eight indexes | -0.018% | 3/7 | -0.834% to +0.147% | Prefer four; mixed compiler-workload results and larger stack buffers |
| Merge metadata, sixteen indexes | -0.015% | 1/7 | -0.440% to +1.007% | Prefer four; mixed compiler-workload results and larger stack buffers |
| Boolean SmallVec, sixteen flags | -0.025% | 2/7 | -0.364% to +0.831% | Confirm |
| SmallBitVec flags | -0.013% | 1/7 | -0.554% to +0.307% | Confirm |
| Mixin flags | -0.005% | 0/7 | -0.161% to +0.649% | Not retained; no consistent paired improvement |
| Borrowed resolver keys | -0.031% | 1/7 | -0.562% to +0.256% | Not retained; mixed results and large baseline variation on apparent outliers |
| File-include reasons | +0.006% | 1/7 | -0.455% to +0.523% | Not retained; mixed instructions and memory |
| Deferred type-argument lists | +0.003% | 1/7 | -0.540% to +0.100% | Not retained; cold target and mixed measurements |
| Diagnostic collision lists | -0.001% | 1/7 | -0.802% to +0.376% | Not retained; cold target and mixed measurements |

Some runs have noticeably higher instruction counts even with the same binary. For example, the resolver
screen's mikro-orm baseline samples are 47.050, 47.377 and 47.362 G, while the candidate samples are 47.044,
47.363 and 47.038 G. The ratio of medians suggests -0.672%, but the median paired change is only -0.031%.
Neither that headline nor the analogous t3code result is treated as an established gain. A boolean SmallVec
xstate outlier similarly changes the sign of the ratio of medians relative to the paired median. Both boolean
representations and both inference capacities therefore proceed to the larger comparison.

Confirmation uses five rounds per batch with a common baseline observation in each round, alternating the full
binary order and reversing the first order in the second batch. Each candidate is paired with the
baseline from its own round. All 19 catalog entries are attempted in four modes: fast/reference, each with one
checker and default checkers. The configuration-only drizzle-orm entry is excluded from performance conclusions.
An initial partial four-candidate batch was superseded before conclusions were drawn so that both inference
capacities could be measured together; its observations are not included in the final comparison.

## Full-catalog confirmation

The instruction changes below compare pooled medians of ten observations per binary. “Both paired medians lower” counts projects whose median paired change is negative in each five-round batch. Baseline observations are shared within a round, not independent observations for each candidate.

| Trial | Fast instructions, median change | Fast: both paired medians lower | Reference instructions, median change | Reference: both paired medians lower |
| --- | ---: | ---: | ---: | ---: |
| Inference, four handles | +0.004% | 7/18 | +0.003% | 6/18 |
| Inference, eight handles | -0.003% | 8/18 | +0.005% | 4/18 |
| Merge metadata, four indexes | -0.004% | 9/18 | -0.038% | 13/18 |
| Boolean SmallVec | +0.007% | 5/18 | +0.011% | 4/18 |
| SmallBitVec flags | -0.024% | 11/18 | -0.016% | 12/18 |

### Fast single-threaded instructions

| Project | Inference 4 | Inference 8 | Merge 4 | Boolean SmallVec | SmallBitVec |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | +0.012% | -0.019% | -0.036% | -0.023% | -0.034% |
| xstate-main | -0.012% | -0.007% | -0.023% | -0.008% | -0.049% |
| webpack | -0.020% | -0.017% | -0.017% | -0.009% | -0.032% |
| mui-docs | +0.029% | +0.028% | +0.025% | +0.031% | -0.013% |
| Compiler | +0.006% | -0.002% | -0.006% | +0.011% | -0.001% |
| Compiler-Unions | -0.002% | -0.003% | -0.049% | +0.002% | -0.005% |
| cal-diy | -0.012% | -0.025% | -0.003% | -0.024% | -0.065% |
| formbricks-web | -0.020% | -0.021% | +0.001% | -0.012% | -0.060% |
| supabase-studio | +0.002% | +0.007% | +0.029% | +0.035% | -0.031% |
| t3code-server | -0.014% | -0.012% | -0.016% | -0.004% | +0.001% |
| mikro-orm | -0.022% | -0.018% | -0.024% | +0.008% | -0.017% |
| next-packages-next | +0.009% | +0.009% | -0.009% | +0.007% | -0.012% |
| next-root | +0.013% | +0.028% | +0.026% | +0.016% | +0.016% |
| storybook | +0.049% | +0.031% | +0.060% | +0.066% | -0.011% |
| nuxt | +0.017% | +0.018% | +0.014% | +0.024% | +0.022% |
| playwright | -0.033% | -0.032% | -0.024% | -0.019% | -0.059% |
| typeorm | +0.118% | +0.115% | +0.109% | +0.074% | -0.037% |
| date-fns | +0.120% | +0.120% | +0.129% | +0.096% | -0.056% |

### Reference single-threaded instructions

| Project | Inference 4 | Inference 8 | Merge 4 | Boolean SmallVec | SmallBitVec |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | -0.005% | -0.002% | -0.102% | +0.041% | +0.024% |
| xstate-main | -0.002% | +0.001% | -0.064% | -0.006% | -0.054% |
| webpack | -0.004% | +0.004% | -0.092% | -0.001% | -0.019% |
| mui-docs | +0.024% | +0.027% | +0.044% | +0.074% | -0.013% |
| Compiler | +0.004% | +0.008% | -0.002% | +0.002% | +0.001% |
| Compiler-Unions | -0.007% | -0.005% | -0.067% | -0.003% | -0.007% |
| cal-diy | +0.012% | +0.068% | +0.020% | +0.035% | -0.041% |
| formbricks-web | +0.012% | +0.011% | +0.036% | +0.026% | -0.007% |
| supabase-studio | -0.010% | -0.004% | -0.009% | +0.007% | -0.010% |
| t3code-server | -0.323% | -0.384% | -0.471% | -0.396% | -0.413% |
| mikro-orm | -0.017% | -0.030% | -0.087% | -0.011% | -0.021% |
| next-packages-next | +0.006% | +0.005% | -0.032% | +0.007% | -0.012% |
| next-root | +0.013% | +0.008% | -0.002% | +0.006% | -0.085% |
| storybook | +0.012% | +0.011% | -0.044% | +0.060% | -0.009% |
| nuxt | -0.006% | -0.015% | -0.050% | +0.016% | -0.050% |
| playwright | +0.002% | +0.003% | -0.046% | +0.023% | -0.006% |
| typeorm | +0.089% | +0.093% | -0.003% | +0.082% | -0.048% |
| date-fns | +0.071% | +0.100% | +0.103% | +0.139% | -0.019% |

### Default-checker tradeoffs

Ranges across the 18 usable projects, comparing pooled medians.

| Trial | Fast RSS | Fast wall | Reference RSS | Reference wall |
| --- | ---: | ---: | ---: | ---: |
| Inference, four handles | -0.493% to +1.593% | -0.834% to +2.597% | -0.179% to +0.237% | -1.456% to +3.208% |
| Inference, eight handles | -0.527% to +1.418% | -2.038% to +4.815% | -0.371% to +0.161% | -2.400% to +2.140% |
| Merge metadata, four indexes | -0.836% to +1.004% | -2.143% to +3.019% | -0.361% to +0.418% | -1.570% to +2.220% |
| Boolean SmallVec | -0.580% to +1.166% | -1.278% to +3.202% | -0.325% to +0.386% | -1.561% to +2.293% |
| SmallBitVec flags | -0.603% to +1.735% | -6.536% to +2.405% | -0.401% to +0.205% | -1.352% to +1.437% |

The retained candidates are four-index merge metadata and SmallBitVec
inference flags. The merge trial reduces the median paired reference-mode instruction count in both batches on
13/18 projects. SmallBitVec does so on 11/18 fast-mode and 12/18 reference-mode projects. The inference-handle
capacities and boolean SmallVec do not show that broad, repeatable improvement and are not retained.

This is an instruction-count result, with mixed wall time and peak RSS. The merge trial regresses date-fns
instructions in both modes; SmallBitVec regresses reference-mode vscode. The per-project tables preserve those
tradeoffs. The shared t3code-server reference baseline varies enough that all five trials have a large pooled
reduction there; that common headline is not treated as a standalone gain. Both batches contain 4,320 unique
timed observations in total. Every timed exit status matches its warmup, every baseline/candidate diagnostic
hash matches, and configuration and binary hashes remain fixed.

PR #794 was merged during the investigation. The final follow-up therefore starts from current main
`d197eeea709616d61930c2bd5eac048471e404b8`, with fresh baseline and candidate builds. Its comparison below
measures only the additional changes; the isolated trials above remain relative to `5b0ebd42`.

## Final comparison on current main

Baseline: `d197eeea709616d61930c2bd5eac048471e404b8`. Five interleaved pairs per mode, after one warmup per binary;
the baseline already includes PR #794. The final candidate contains only the retained follow-up changes.

### Fast mode

| Project | Single-threaded instructions, G | Default-checker peak RSS, MiB | Default-checker wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 80.26749 → 80.26598 (-0.002%) | 2037.92 → 2034.47 (-0.169%) | 947.149 → 947.446 (+0.031%) |
| xstate-main | 5.85920 → 5.85529 (-0.067%) | 248.95 → 249.00 (+0.019%) | 122.912 → 122.174 (-0.601%) |
| webpack | 10.67842 → 10.67604 (-0.022%) | 482.69 → 478.28 (-0.913%) | 147.256 → 146.148 (-0.753%) |
| mui-docs | 36.82109 → 36.80392 (-0.047%) | 1506.06 → 1498.69 (-0.490%) | 1098.041 → 1109.188 (+1.015%) |
| Compiler | 1.68308 → 1.68294 (-0.009%) | 76.81 → 76.73 (-0.102%) | 62.093 → 63.622 (+2.462%) |
| Compiler-Unions | 2.85054 → 2.84906 (-0.052%) | 81.81 → 81.80 (-0.019%) | 94.995 → 96.409 (+1.489%) |
| cal-diy | 31.07648 → 31.07308 (-0.011%) | 1759.13 → 1760.97 (+0.105%) | 726.183 → 743.483 (+2.382%) |
| formbricks-web | 39.57973 → 39.56985 (-0.025%) | 1878.41 → 1881.28 (+0.153%) | 772.444 → 768.160 (-0.555%) |
| supabase-studio | 43.60488 → 43.60002 (-0.011%) | 1582.23 → 1578.95 (-0.207%) | 805.007 → 799.427 (-0.693%) |
| t3code-server | 39.11075 → 39.10353 (-0.018%) | 1990.86 → 1998.73 (+0.396%) | 1297.698 → 1359.261 (+4.744%) |
| mikro-orm | 46.90837 → 46.88344 (-0.053%) | 1846.77 → 1826.75 (-1.084%) | 749.577 → 748.813 (-0.102%) |
| next-packages-next | 16.73063 → 16.72522 (-0.032%) | 620.80 → 619.89 (-0.146%) | 249.872 → 246.948 (-1.170%) |
| next-root | 16.25512 → 16.25284 (-0.014%) | 465.55 → 461.22 (-0.930%) | 384.366 → 385.453 (+0.283%) |
| storybook | 9.74381 → 9.74046 (-0.034%) | 472.92 → 469.48 (-0.727%) | 186.065 → 185.104 (-0.517%) |
| nuxt | 9.86273 → 9.85954 (-0.032%) | 493.91 → 494.34 (+0.089%) | 230.619 → 229.478 (-0.495%) |
| playwright | 7.44381 → 7.44405 (+0.003%) | 443.77 → 446.56 (+0.630%) | 137.890 → 139.084 (+0.866%) |
| drizzle-orm | Configuration rejected | — | — |
| typeorm | 7.54348 → 7.54085 (-0.035%) | 250.91 → 251.06 (+0.062%) | 176.110 → 177.552 (+0.818%) |
| date-fns | 2.84808 → 2.84735 (-0.026%) | 146.20 → 146.27 (+0.043%) | 130.627 → 128.904 (-1.319%) |

### Reference mode

| Project | Single-threaded instructions, G | Default-checker peak RSS, MiB | Default-checker wall, ms |
| --- | ---: | ---: | ---: |
| vscode | 90.55914 → 90.47022 (-0.098%) | 2409.34 → 2403.92 (-0.225%) | 2374.391 → 2378.183 (+0.160%) |
| xstate-main | 6.26258 → 6.25547 (-0.113%) | 266.47 → 266.97 (+0.188%) | 188.587 → 189.513 (+0.491%) |
| webpack | 11.09621 → 11.08548 (-0.097%) | 404.89 → 404.86 (-0.008%) | 301.841 → 302.506 (+0.220%) |
| mui-docs | 40.29912 → 40.28932 (-0.024%) | 1328.27 → 1323.33 (-0.372%) | 1542.278 → 1540.470 (-0.117%) |
| Compiler | 1.71700 → 1.71693 (-0.004%) | 77.95 → 77.75 (-0.261%) | 62.047 → 61.524 (-0.843%) |
| Compiler-Unions | 2.94866 → 2.94675 (-0.065%) | 81.77 → 81.56 (-0.248%) | 94.273 → 95.309 (+1.100%) |
| cal-diy | 33.00482 → 32.99302 (-0.036%) | 1344.98 → 1345.38 (+0.029%) | 1106.479 → 1105.412 (-0.096%) |
| formbricks-web | 44.36842 → 44.34939 (-0.043%) | 1988.95 → 1986.52 (-0.123%) | 1348.137 → 1340.980 (-0.531%) |
| supabase-studio | 46.11672 → 46.09753 (-0.042%) | 1407.06 → 1409.91 (+0.202%) | 1601.075 → 1603.703 (+0.164%) |
| t3code-server | 48.25429 → 48.22011 (-0.071%) | 1864.13 → 1860.03 (-0.220%) | 2016.377 → 2004.601 (-0.584%) |
| mikro-orm | 53.42503 → 53.36571 (-0.111%) | 1631.70 → 1621.16 (-0.646%) | 1573.304 → 1566.960 (-0.403%) |
| next-packages-next | 17.73836 → 17.72977 (-0.048%) | 603.00 → 602.39 (-0.101%) | 666.732 → 665.911 (-0.123%) |
| next-root | 16.59804 → 16.59905 (+0.006%) | 469.77 → 469.42 (-0.073%) | 523.274 → 524.013 (+0.141%) |
| storybook | 10.13765 → 10.12919 (-0.083%) | 447.06 → 447.50 (+0.098%) | 298.144 → 294.343 (-1.275%) |
| nuxt | 10.35649 → 10.34811 (-0.081%) | 428.39 → 427.86 (-0.124%) | 323.324 → 324.656 (+0.412%) |
| playwright | 7.67839 → 7.67176 (-0.086%) | 367.58 → 367.39 (-0.051%) | 250.432 → 248.668 (-0.704%) |
| drizzle-orm | Configuration rejected | — | — |
| typeorm | 7.62280 → 7.61463 (-0.107%) | 234.69 → 233.83 (-0.366%) | 258.760 → 255.251 (-1.356%) |
| date-fns | 2.90450 → 2.90402 (-0.016%) | 146.05 → 146.34 (+0.203%) | 128.317 → 128.292 (-0.019%) |

The combined change lowers median single-threaded instructions on 17/18 projects in each mode. Across projects,
the median changes are -0.025% in fast mode and -0.068% in reference mode. Median paired changes are negative on
16/18 and 17/18 projects respectively; every pair is lower on 3/18 and 7/18. Individual-run variation still exceeds
some of these small effects. Together with the two isolated confirmation batches, this supports retaining both
changes under the current policy, which has no fixed percentage threshold.

The exceptions are fast-mode playwright (+0.003%) and reference-mode next-root (+0.006%). Default-checker peak RSS
ranges from -1.084% to +0.630% in fast mode (median -0.060%) and -0.646% to +0.203% in reference mode
(median -0.112%). Default wall time ranges from -1.319% to +4.744% and -1.356% to +1.100%, respectively.
Single-threaded wall time is also mixed: -2.029% to +2.371% in fast mode and -1.194% to +2.066% in reference mode.
There is no general wall-time speedup or peak-memory reduction claim.

The t3code-server fast-default timing was repeated with five more pairs starting in the reverse order. Its first
batch's +4.744% ratio of medians has a median paired change of only +0.200%; the second batch is -1.076% by the
ratio of medians and -2.474% by paired medians. Pooled ten-run wall medians are 1302.244 → 1296.415 ms (-0.448%),
and peak RSS is 1994.23 → 1994.63 MiB (+0.020%). The original catalog row above is kept unchanged: the apparent
wall-time regression did not repeat, and these mixed batches establish neither a general speedup nor a stable
parallel timing regression.

All 720 timed catalog observations and the ten targeted repeat observations have matching warmup exit statuses.
Baseline/candidate diagnostic hashes match in every cell. The final audit verifies equal Rust/compiler build
settings, unchanged binary and configuration hashes, and a source diff matching the saved release build. Raw records are
in `pr-combined/summary.json` and `pr-t3-confirm/summary.json` under the local work directory.

## Focused correctness coverage

A generated local fixture checks matched inference buffers with 24 entries, unmatched inference with 80 entries,
boolean scratch with lengths 1, 2, 16, 17, 62, 63 and above 64, union metadata reaching 32/34/18 entries, and
20 mixin flags. Temporary census output confirms that the intended paths execute. The baseline and all twelve
candidate binaries match the pinned Go compiler's diagnostic bytes and exit status in both checker
modes (26 comparisons); the fixture is in the local `spill/` directory. These checks include inline capacity
boundaries and heap spills, not only the common short-buffer cases.

The resolver candidate also passes 146 core and 39 module unit checks (one ignored in each crate), including
borrowed string lookup, every composite-key discriminator, module-cache first-value retention, and
type-reference-cache replacement. The existing `ts-ref` checkout remains untouched; reference checks use the
clean isolated checkout at the exact Cargo pin, as described in the preceding audit.

## Validation on current main

Fresh development builds of `d197eeea` and the combined candidate preserve exactly the same passing lists in both
checker modes: 13,699 error baselines, 12,997 each of types and symbols, 13,625 JavaScript/declaration outputs,
149 JavaScript maps and 156 source-map records. Each mode covers 15,447 variants, with no baseline mismatch,
crash or timeout in that selection. Reference mode uses one checker; fast mode exercises four checkers.

The separately retried `compiler/intersectionConstructorReductionCrash` times out in both builds at 30 seconds;
the preceding audit also reproduced it on both earlier builds at 180 seconds. It is the one omitted case above.
The isolated reference checkout still matches Cargo's `3e9236361fa97a9e6c685f4833ac457ae2d8c50a` pin and has a clean
`tsc` tree. The original reference checkout is preserved.

All 50 checked-in regression fixtures pass. Formatting, Clippy with warnings denied, source lint and
`cargo check -p tsrs_wasm --target wasm32-wasip1` pass.

Fresh baseline and candidate workspace runs use
`cargo test --offline --locked --no-fail-fast -- --skip generated_code_is_fresh`, outside the sandbox for local
socket checks. Their failure lists are identical: the same 20 missing/mismatched reference-fixture cases in
`tsrs_ls`, `tsrs_lsp` and `tsrs_tsoptions` documented in the preceding audit. The workspace suite is not green in
this checkout; no candidate-only failure was found. Generator freshness is skipped because those checks use the
original reference checkout; the preceding audit records its isolated verification at the correct pin.

The candidate also builds with `tsrs_core/plain-ptrs` in a separate target directory. The fresh-main release and
development baselines, compressed-pointer development candidate and plain-pointer candidate all match the pinned
Go compiler on the focused spill fixture in both modes (eight comparisons).
The final compressed-pointer release candidate also matches in both modes, for ten comparisons on current main.
