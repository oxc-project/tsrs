# tsrs_dts against rolldown + rolldown-plugin-dts on published packages

Benchmark: [`bench/dts/`](../bench/dts), described in [docs/DTS.md](../docs/DTS.md#benchmark). Inputs are five
published npm packages that ship their TypeScript sources, pinned by tarball integrity in
[`projects.json`](../bench/dts/projects.json), each bundled from the same entries and tsconfig by every tool.

## What is compared

tsrs_dts type-checks, emits and bundles in one pass and fails on any error. rolldown-plugin-dts never type-checks
by default: its tsgo generator passes `--noCheck`, and its tsc generator only emits. The like-for-like rows
therefore run the plugin's tsgo generator through a wrapper that drops `--noCheck`. tsgo then checks while it
emits, in the same single pass, and a type error fails the rolldown build. This was verified by injecting a
TS2322 into immer: both rolldown checking rows and tsrs_dts fail, and the `--noCheck` row succeeds.

tsrs_dts runs its whole Program single-threaded. One rolldown row matches that with tsgo `--singleThreaded`,
and it and both tsrs_dts rows run under `taskset` on one core, so Node, rolldown's threads and Go's runtime
threads get the same single CPU. A second rolldown row keeps tsgo's default parallelism on all four cores.
tsrs_dts also parses, binds and checks its emitted declarations, which no rolldown row does; that extra work
counts against tsrs_dts.

Both sides write their outputs to disk. "In-process ms" covers the build through the writes and excludes process
and runtime startup (Node and module loading for rolldown), which process wall time includes.

An earlier version of this note compared tsrs_dts with `tsgo --noEmit` followed by the plugin's `--noCheck`
build. That parsed and bound everything twice and let tsgo check on four threads. It overstated the gap
(1.7–3.4×), and those rows are gone.

## Summary

Paired ratios: tools run interleaved, so each round gives one rolldown/tsrs_dts ratio under the same host
conditions. The median and range are over 10 rounds.

| Package | Same work, 1 CPU each | Same work, rolldown on 4 CPUs, tsrs_dts on 1 | rolldown `--noCheck` (reference) |
| --- | ---: | ---: | ---: |
| immer | 2.35× (1.80–3.21) | 1.72× (1.03–2.04) | 1.30× (0.69–1.54) |
| query-core | 3.10× (2.30–3.38) | 1.64× (1.47–2.17) | 0.93× (0.70–1.20) |
| trpc-server | 2.86× (2.42–3.98) | 1.83× (1.41–2.15) | 0.99× (0.79–1.14) |
| zod | 3.37× (2.16–4.03) | 1.92× (1.32–2.14) | 1.01× (0.65–1.45) |
| rxjs | 4.72× (3.26–5.57) | 2.86× (1.60–3.52) | 1.94× (1.32–2.52) |

- **Same work:** tsrs_dts was faster in all 10 rounds on every package, in both rows. With one CPU each,
  rolldown takes 2.4–4.7× the wall time. Given all four cores against tsrs_dts's one, it still takes 1.6–2.9×.
- **Startup excluded:** in-process time gives 2.0–4.3× (one CPU) and 1.4–2.4× (four CPUs), so the gap is not
  Node startup.
- **CPU and memory:** tsrs_dts uses 0.35–1.30 CPU-seconds; rolldown with checking uses 0.87–4.23 on one core and
  1.45–6.71 on four. Peak RSS is 65–152 MiB for tsrs_dts against 106–297 MiB.
- **Reference without checking:** the plugin's default tsgo `--noCheck` build does less work than tsrs_dts, yet
  takes about the same time on query-core, trpc-server and zod (0.93–1.01×, ranges straddle 1), and longer on
  immer (1.30×) and rxjs (1.94×). The tsc 6 generator takes 2.4–4.7×.
- **Diagnostic reuse** runs at 0.74× and 0.72× of the default on immer and query-core, faster in 9 of 10 rounds.
  It makes no clear difference on trpc-server or zod, and a small, uncertain gain on rxjs (0.88×, faster in 7
  of 10 rounds). Output bytes are identical with it on and off.
- **Output correctness:** every tsrs_dts bundle type-checks as a consumer and keeps every entry export with its
  meaning, apart from one harmless widening every tool shares (below). On zod, every rolldown variant writes a
  chunk that references an undeclared `__exportAll` (TS2304 at `index-*.d.ts(2847,8193)`), and drops the value
  meaning of `export * as JSONSchema`.
- **Output size:** tsrs_dts bundles are 4–61% larger on four packages and 9% smaller on immer. Most of zod's
  difference is synthesized `declare namespace module_N { export { X_1 as X }; … }` blocks, one specifier per
  line: 236 KB of `export {` lines against 67 KB for rolldown. With whitespace removed, the tsrs_dts output is
  still 56% larger.

## Method

Linux x64 cloud VM, Intel Xeon @ 2.80 GHz, 4 logical CPUs, 16 GiB; Node 22.22.0. tsrs at `fd5befa` (release
profile; the `bundle` example uses the CLI's mimalloc and 512 MiB stack). rolldown 1.2.15, rolldown-plugin-dts
0.28.8, TypeScript 7.1.0-beta (the npm build of the commit tsrs ports) for tsgo and validation, TypeScript 6.0.3
for the tsc generator and the export comparison. Single-CPU rows are pinned to CPU 3.

Two warmup rounds, then ten measured rounds. Each round runs every tool on every package in a fresh process,
rotating the tool order. Wall time, CPU (user + sys) and peak RSS come from GNU `time`. `perf` was unavailable,
so there are no instruction counts. The host is shared and noisy (tsrs_dts on immer ranged 301–630 ms), which is
why the summary uses paired ratios.

After timing, each tool's first output is type-checked as a consumer: all output declarations as roots, strict,
`skipLibCheck: false`. Each entry's exports and their value/type/namespace meanings are then compared with the
source entry's, through the TypeScript 6 API. The source entries have 27 (immer), 151 (query-core), 376
(trpc-server), 1231 (zod) and 362 (rxjs) exports.

## Results

Machine: Intel(R) Xeon(R) Processor @ 2.80GHz, 4 logical CPUs, 16 GiB, linux-x64. rolldown 1.2.15, rolldown-plugin-dts 0.28.8, typescript 6.0.3, typescript 7 7.1.0-beta, Node v22.22.0. tsrs commit fd5befae87b2. 10 measured runs after 2 warmup(s); medians, with min–max for wall time. Wall / tsrs_dts is the median per-round ratio (min–max), then the rounds in which tsrs_dts was faster. CPUs "1" rows run under taskset on CPU 3. In-process ms spans the build and output writes, excluding process and runtime startup.

### immer (immer@10.1.1, 1 entry)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 375 (301–630) |  | 360 | 0.35 | 65 | 1 | 11900 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 282 (230–353) | 0.74× (0.53–1.06), 1/10 | 269 | 0.27 | 55 | 1 | 11900 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 896 (819–1131) | 2.35× (1.80–3.21), 10/10 | 716 | 0.87 | 106 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 663 (582–744) | 1.72× (1.03–2.04), 10/10 | 512 | 1.45 | 113 | 1 | 13065 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsgo --noCheck | all | 493 (401–599) | 1.30× (0.69–1.54), 9/10 | 333 | 0.80 | 112 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 1859 (1538–2107) | 4.62× (2.99–6.31), 10/10 | 1682 | 3.13 | 246 | 1 | 13065 | pass | yes |

### query-core (@tanstack/query-core@5.105.0, 1 entry)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 814 (706–928) |  | 797 | 0.79 | 125 | 1 | 216869 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 578 (508–933) | 0.72× (0.62–1.11), 1/10 | 564 | 0.55 | 99 | 1 | 216869 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 2349 (1875–3139) | 3.10× (2.30–3.38), 10/10 | 2110 | 2.31 | 171 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1338 (1171–1615) | 1.64× (1.47–2.17), 10/10 | 1135 | 3.06 | 175 | 1 | 202871 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsgo --noCheck | all | 730 (585–947) | 0.93× (0.70–1.20), 4/10 | 524 | 1.53 | 175 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 2739 (2260–3238) | 3.41× (2.78–3.89), 10/10 | 2476 | 5.13 | 344 | 1 | 202678 | pass | yes |

### trpc-server (@trpc/server@11.19.0, 6 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 861 (688–1059) |  | 843 | 0.83 | 128 | 7 | 123353 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 827 (721–1005) | 1.01× (0.79–1.19), 5/10 | 806 | 0.80 | 128 | 7 | 123353 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 2566 (2170–3213) | 2.86× (2.42–3.98), 10/10 | 2260 | 2.51 | 170 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1579 (1289–1776) | 1.83× (1.41–2.15), 10/10 | 1280 | 3.40 | 180 | 8 | 104082 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsgo --noCheck | all | 813 (668–1128) | 0.99× (0.79–1.14), 4/10 | 604 | 1.72 | 176 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 2952 (2592–4012) | 3.35× (2.89–4.86), 10/10 | 2651 | 5.56 | 376 | 8 | 104086 | pass | yes |

### zod (zod@4.4.3, 3 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 1326 (1162–1944) |  | 1309 | 1.30 | 152 | 4 | 484497 | pass | 1 differ |
| tsrs_dts, reuse diagnostics | 1 | 1211 (1142–2102) | 0.99× (0.63–1.14), 5/10 | 1190 | 1.19 | 152 | 4 | 484497 | pass | 1 differ |
| rolldown + plugin-dts, tsgo checking | 1 | 4318 (3937–5368) | 3.37× (2.16–4.03), 10/10 | 4153 | 4.23 | 270 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsgo checking | all | 2602 (2265–2774) | 1.92× (1.32–2.14), 10/10 | 2423 | 6.71 | 297 | 4 | 300641 | 1 errors | 2 differ |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsgo --noCheck | all | 1326 (1120–1707) | 1.01× (0.65–1.45), 5/10 | 1097 | 2.92 | 294 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsc 6 | all | 3383 (2819–4429) | 2.40× (1.90–3.76), 10/10 | 3146 | 7.38 | 469 | 4 | 300637 | 1 errors | 2 differ |

### rxjs (rxjs@7.8.2, 5 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 672 (528–1050) |  | 656 | 0.65 | 88 | 6 | 338234 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 619 (508–718) | 0.88× (0.61–1.12), 3/10 | 603 | 0.59 | 88 | 6 | 338234 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 3085 (2590–3920) | 4.72× (3.26–5.57), 10/10 | 2824 | 3.03 | 240 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1846 (1551–2106) | 2.86× (1.60–3.52), 10/10 | 1569 | 3.83 | 245 | 9 | 326745 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsgo --noCheck | all | 1281 (1115–1640) | 1.94× (1.32–2.52), 10/10 | 1018 | 2.62 | 245 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 3241 (2452–3842) | 4.69× (2.95–5.85), 10/10 | 2970 | 6.24 | 391 | 9 | 326757 | pass | yes |

Byte counts include all output files. tsrs_dts writes one facade per entry plus a shared bundle; rolldown writes
entries plus hashed chunks.

## Export differences

- `$ZodRealError` (zod `core`), all tools: the source pairs a local `interface $ZodRealError` with an exported
  `const $ZodRealError`. Every bundle exports both meanings, so the type is exported too. This widening is
  harmless.
- `JSONSchema` (zod `core`), rolldown only: `export * as JSONSchema from "./json-schema.js"` becomes a
  type-only namespace, so `typeof JSONSchema` no longer resolves. tsrs_dts keeps the value.

## Inputs left out

tsrs_dts rejects any diagnostic, so inputs must check cleanly under TypeScript 7.1.0-beta; the tsrs CLI and tsgo
7.1.0-beta report the same errors on these:

- effect 4.0.6: TS2345 at `src/SchemaAST.ts(3118,51)`. It checks cleanly under 7.0.2.
- effect 3.22.4: TS2719 and TS2769 (2) in `internal/doNotation.ts`, `internal/groupBy.ts` and
  `internal/stream.ts`. It checks cleanly under TypeScript 6.0.3.
- The rxjs `webSocket` entry: TS2345 from the TypeScript 5.7 `ArrayBuffer` lib change in `WebSocketSubject.ts`.

## Follow-ups

- Linker output size on namespace-heavy packages (zod): emitting synthesized namespace export lists more
  compactly would close most of the gap with rolldown.
- A no-check mode in tsrs_dts would allow a like-for-like comparison with the plugin's default `--noCheck` build.
- Re-measure on dedicated hardware with `perf` for instruction counts before citing small differences.
