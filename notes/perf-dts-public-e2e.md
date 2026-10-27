# End-to-end declaration bundler benchmarks on public projects

Starting revision: `7a00a0d7fc11be1e557e06846d0302cb842691c3`. TypeScript reference: `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.
All work is local. Production code and original public-project source files were not edited for this benchmark.

## Comparison and scope

The baseline uses the previous fragment generator source-table seeding. The candidate uses
`Printer::write_with_source_map` and fragment-local source-index translation. These are the only two
runtime source differences. Both variants retain the same concurrent compiler, AST, graph, generated-name,
source-map serialization, allocation-ownership and diagnostic-fallback changes. This measures the printer
integration, not the aggregate effect of the entire uncommitted working tree.

Each fresh process calls the public `tsrs_dts::build` or `build_entries` API once. The timed API includes
configuration parsing, source loading/parsing/binding/checking, declaration emission, declaration
reparsing/checking, linking, printing, map composition and internal Program teardown. OS file caches are warm;
no BuildSession result cache is used. Capturing outputs is outside the build timer and absent from measured
runs. Sorted-output hashing is outside the build timer but included in process counters and process wall time.
This measures the in-memory library build API; package-manager/plugin startup and output disk writes are absent.

## Public inputs

- **immer 10.1.1:** Locally installed npm immer 10.1.1, repository https://github.com/immerjs/immer.git. Published src/immer.ts entry closure; strict ES2022/ESNext/Bundler, ES2024 library, installed @types/node 22.20.5 for process.env. Source and package.json copied unchanged outside node_modules.
- **zod 4.4.3:** Locally installed npm zod 4.4.3, repository https://github.com/colinhacks/zod.git. Published src/index.ts entry closure; strict ES2022/NodeNext, ES2024 and DOM libraries, no automatic ambient types. Source and package.json copied unchanged outside node_modules.
- **xstate 5.33.2:** Existing public catalog checkout https://github.com/statelyai/xstate.git at fbee62e7c1586315ed478c2fedf530d7e0ff5a3e. Six package preconstruct entrypoints; narrow source config retains strict, isolatedModules, NodeNext, ESNext, ES2024/DOM, allowImportingTsExtensions, resolveJsonModule and stripInternal. Explicit installed Node25 types replace vitest globals and provide global for the dev entry. skipLibCheck is false; diagnostic reuse remains subject to its normal eligibility gates.

The original package placement rejected Immer/Zod entry re-exports because the source was under
`node_modules`. Byte-identical copies outside that directory compile and bundle successfully. Immer initially
conflicted with Node25 disposable iterator declarations; existing Node22 types compile cleanly. XState initially
lacked Node global declarations. Initial and corrected preflight/smoke logs are retained. No source stubs,
dependency installs, disabled checking, or compiler fixes were used.

Source inventories in `projects.json` hash whole packaged source directories, including any tests;
the configured explicit entry files select transitive production source closures. Emitted declaration counts
below describe what each build actually processed. Installed package versions/tarball integrity records are
retained in `package-provenance.json`; npm package source Git commits are unavailable locally. Input hashes
and installed Node/undici type package hashes were verified before and after measurement.

## Method

Apple M4 Max arm64, 16 logical CPUs, 64 GiB; matched offline release builds with opt-level 3, fat LTO,
one codegen unit, full debug symbols and stripping disabled. Same absolute dependency source/build paths,
system allocator, and 64 MiB worker stack in both variants. This matches the preceding standalone probe;
it does not benchmark the CLI mimalloc allocator. The bundler forces one checker, so requesting a default
checker count would not exercise a different setup.

Each case has two warmups per variant followed by seven alternating AB/BA pairs. `/usr/bin/time -l`
supplies retired instructions and process peak RSS; an internal Instant measures the full API call,
and the JavaScript driver records process wall time. Installed Hyperfine is 1.18 without hardware counters,
so native macOS counters are used. No own builds/tests overlap final timing. External load is recorded per run.

## Results

Independent medians before → after. Percentages are median paired changes.

| Workload | Build API milliseconds | Instructions (billions) | Peak RSS MiB | Process milliseconds |
| --- | ---: | ---: | ---: | ---: |
| immer-no-maps | 88.15 → 88.29 (+0.21%) | 1.389 → 1.389 (+0.02%) | 65.25 → 65.41 (+0.29%) | 95.12 → 94.05 (-0.56%) |
| immer-maps | 91.70 → 91.20 (-0.49%) | 1.463 → 1.446 (-1.16%) | 65.45 → 65.61 (+0.24%) | 97.58 → 97.31 (-0.79%) |
| zod-no-maps | 276.20 → 272.58 (-1.28%) | 4.738 → 4.737 (-0.01%) | 143.97 → 143.94 (-0.02%) | 282.45 → 278.59 (-1.19%) |
| zod-maps | 408.84 → 336.46 (-18.08%) | 7.975 → 6.247 (-21.68%) | 147.02 → 146.98 (-0.06%) | 414.94 → 343.46 (-17.82%) |
| xstate-no-maps | 259.85 → 260.20 (-0.02%) | 4.195 → 4.194 (-0.02%) | 145.95 → 145.89 (+0.06%) | 266.01 → 266.49 (+0.22%) |
| xstate-maps | 378.25 → 298.68 (-21.04%) | 7.001 → 5.052 (-27.83%) | 147.88 → 147.98 (+0.06%) | 384.47 → 304.79 (-20.73%) |

Paired ranges and counts of decreases:

- immer-no-maps: buildWall -2.66%..+3.88%, lower 3/7; processWall -2.70%..+5.77%, lower 4/7; instructions -0.91%..+1.09%, lower 2/7; rss -0.07%..+0.67%, lower 2/7
- immer-maps: buildWall -2.07%..+0.47%, lower 5/7; processWall -4.18%..+0.91%, lower 6/7; instructions -1.91%..-0.44%, lower 7/7; rss -0.24%..+0.48%, lower 1/7
- zod-no-maps: buildWall -15.87%..+2.06%, lower 5/7; processWall -16.02%..+1.94%, lower 6/7; instructions -0.26%..+0.03%, lower 4/7; rss -0.75%..+0.35%, lower 4/7
- zod-maps: buildWall -18.69%..-16.75%, lower 7/7; processWall -18.42%..-16.32%, lower 7/7; instructions -22.40%..-21.62%, lower 7/7; rss -0.94%..+0.05%, lower 5/7
- xstate-no-maps: buildWall -1.00%..+2.44%, lower 4/7; processWall -0.92%..+1.97%, lower 3/7; instructions -0.08%..+0.01%, lower 5/7; rss -0.09%..+0.33%, lower 3/7
- xstate-maps: buildWall -21.93%..-15.87%, lower 7/7; processWall -21.51%..-15.32%, lower 7/7; instructions -27.93%..-27.59%, lower 7/7; rss -0.10%..+0.48%, lower 3/7

Verified 84 measured processes and 24 warmups. 27 artifacts including 9 maps are byte-identical.

This is a warm-filesystem-cache fresh-process build benchmark; each process parses/checks/emits/rebinds/links/prints once. The current bundler forces one checker. No BuildSession result cache is used.


## Correctness and workload size

| Workload | Emitted declarations | Retained statements | Removed statements | Output files | Output bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| immer-no-maps | 15 | 46 | 47 | 1 | 11900 |
| immer-maps | 15 | 46 | 47 | 2 | 20307 |
| zod-no-maps | 80 | 1416 | 45 | 1 | 306868 |
| zod-maps | 80 | 1416 | 45 | 2 | 511158 |
| xstate-no-maps | 58 | 373 | 68 | 7 | 203726 |
| xstate-maps | 58 | 373 | 68 | 14 | 324526 |

All selected builds succeeded with matching counters and no diagnostics. 27 captured
artifacts, including 9 maps, have identical paths and bytes. All 84
measured outputs and 24 warmups match their expected output digest. Raw counters and paired
statistics were independently recomputed. This is output parity for these workloads, not consumer validation
against a separate bundler or a rerun of the full conformance suite.

Final one-minute host load ranges 6.26–9.12. Report paired
ranges and direction counts alongside medians; small wall-time changes within these ranges are inconclusive.
An instruction reduction is evidence of less work even when wall-time noise masks it. Memory differences are
reported independently, including regressions; do not infer general CLI, cold-disk or checker-scaling gains.

## Practical conclusion

Retain the mapped printer integration. Zod and six-entry XState both reduce complete build wall time in
all seven pairs: Zod paired median −18.08% (range −18.69% to −16.75%), XState −21.04% (range −21.93%
to −15.87%). Independent median build times are 408.84 → 336.46 ms for Zod (−17.70%) and
378.25 → 298.68 ms for XState (−21.04%). Instructions fall in all seven pairs by median 21.68% and
27.83%, respectively. This establishes a practical end-to-end benefit on these larger mapped workloads.

Immer with maps uses 1.16% fewer instructions in all seven pairs, but its build-time range crosses zero
(median −0.49%, range −2.07% to +0.47%), so a wall-time benefit is not established. No-map controls are
approximately flat in instructions (−0.02% to +0.02% paired medians); their wall ranges cross zero.
There is no established no-map speedup. Peak RSS is approximately flat: paired medians range −0.06%
to +0.29% across all six cases. Small increases are retained in the table rather than counted as savings.

No further implementation was tuned on these projects; no holdout was used. Earlier synthetic gains
were larger because those measurements concentrated on linking, while these include the full compiler
pipeline. These results do not measure the cache ownership fix separately.

## Evidence and reproduction

Raw data and byte captures: `target/scratch/dts-e2e-public/final-run/`.
The experiment contract, source and build manifests, binary hashes, package provenance, generated configs,
preflight/smoke logs, scripts and chronological decisions are retained in `target/scratch/dts-e2e-public/`.
Preflight failures and config/environment adaptations are retained rather than counted as successful builds.

Use retained binaries and inputs to repeat into a new directory, preserving the historical result:

```bash
node target/scratch/dts-e2e-public/verify-inputs.mjs
```

```bash
node target/scratch/dts-e2e-public/measure.mjs repeat-run
```

```bash
node target/scratch/dts-e2e-public/summarize.mjs repeat-run
```

`prepare-projects.mjs` recaptures live inputs/configs and is not a replay of the historical snapshot.
`build-probe.mjs` refuses to overwrite existing evidence; source trees/binaries from both variants remain available.
