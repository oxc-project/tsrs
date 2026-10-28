# tsrs_dts against rolldown + rolldown-plugin-dts on published packages

Benchmark: [`bench/dts/`](../bench/dts), described in [docs/DTS.md](../docs/DTS.md#benchmark). Inputs are five
published npm packages that ship their TypeScript sources, pinned by tarball integrity in
[`projects.json`](../bench/dts/projects.json), each bundled from the same entries and tsconfig by every tool.

## What is compared

tsrs_dts type-checks, emits and bundles in one pass and fails on any error. rolldown-plugin-dts's tsgo generator
runs any tsgo-compatible compiler. It is measured with two:

- **the tsrs CLI**, which shares tsrs_dts's checker and declaration emitter, so those rows differ from tsrs_dts
  only in how declarations are bundled (rolldown against the tsrs_dts linker). Its emitted declarations are
  byte-identical to tsgo's: rolldown names the zod chunk `index-DOuvpMbJ.d.ts` with either compiler.
- **TypeScript 7.1.0-beta (tsgo)**, which is what plugin users run today.

The plugin always passes `--noCheck`, so the like-for-like rows run the compiler through a wrapper that drops it.
The compiler then checks while it emits, in the same single pass, and a type error fails the rolldown build.
This was verified for both compilers by injecting a TS2322 into immer.

tsrs_dts runs its whole Program single-threaded. The "1 CPU" rows give the compiler `--singleThreaded`, and they
and the tsrs_dts rows run under `taskset` on one core, so Node, rolldown's threads and the compiler's runtime
threads share the single CPU. The "4 CPUs" rows leave rolldown and the compiler on every core, while tsrs_dts
stays on one. tsrs_dts also re-checks its emitted declarations, which no rolldown row does; that extra work
counts against tsrs_dts.

Both sides write their outputs to disk. "In-process ms" covers the build through the writes and excludes process
and runtime startup (Node and module loading for rolldown), which process wall time includes.

An earlier version of this note compared tsrs_dts with `tsgo --noEmit` followed by the plugin's `--noCheck`
build. That parsed and bound everything twice and let tsgo check on four threads, so it overstated the gap.

## Summary

Paired ratios: tools run interleaved, so each round gives one rolldown/tsrs_dts ratio under the same host
conditions. The median and range are over 10 rounds; above 1 means tsrs_dts was faster.

| Package | rolldown + tsrs CLI, 1 CPU each | rolldown + tsgo, 1 CPU each | rolldown + tsrs CLI on 4 CPUs | rolldown + tsgo on 4 CPUs |
| --- | ---: | ---: | ---: | ---: |
| immer | 1.31× (1.04–1.54) | 2.27× (1.48–3.00) | 1.10× (0.83–1.37) | 1.60× (1.30–2.00) |
| query-core | 1.56× (0.98–1.90) | 2.92× (1.57–3.35) | 0.88× (0.59–1.36) | 1.50× (1.19–1.92) |
| trpc-server | 1.62× (1.19–2.02) | 2.77× (1.90–3.11) | 0.97× (0.70–1.21) | 1.72× (1.32–2.19) |
| zod | 2.41× (2.01–2.95) | 3.62× (3.11–4.23) | 1.17× (1.03–1.36) | 1.90× (1.73–2.06) |
| rxjs | 3.66× (3.08–5.20) | 5.03× (4.11–6.69) | 2.07× (1.75–2.46) | 2.91× (2.43–3.87) |

- **Same compiler, one CPU each:** the tsrs_dts linker beats rolldown's bundling by 1.3–3.7×. tsrs_dts was faster
  in all 10 rounds on four packages, and in 9 of 10 on query-core. The gap grows with bundle size: 1.3× on
  immer, 3.7× on rxjs.
- **Startup excluded:** in-process time gives 0.83× on immer (rolldown + tsrs CLI is faster once Node startup is
  removed), 1.37× on query-core and trpc-server, 2.2× on zod and 3.5× on rxjs. On small packages, much of the
  wall-time gap is Node startup. On large ones it is bundling.
- **rolldown with 4 CPUs against tsrs_dts on one:** with the tsrs CLI, rolldown is level on immer, query-core and
  trpc-server (0.88–1.10×, ranges straddle 1), and slower on zod (1.17×) and rxjs (2.07×).
- **Against tsgo:** rolldown + tsgo takes 2.3–5.0× on one CPU each and 1.5–2.9× on four CPUs. The tsrs CLI is
  itself faster than tsgo here, which accounts for the difference between the first two columns.
- **Memory:** tsrs_dts peaks at 66–151 MiB; rolldown with either compiler at 106–301 MiB.
- **Diagnostic reuse** runs at 0.70× and 0.64× of the default on immer and query-core, faster in all 10 rounds,
  and at 0.90× on trpc-server. It is slower on zod and rxjs (1.11× each; slower in 7 and 6 of 10 rounds). Output
  bytes are identical with it on and off.
- **Output correctness:** every tsrs_dts bundle type-checks as a consumer and keeps every entry export with its
  meaning, apart from one harmless widening every tool shares (below). On zod, every rolldown variant, with
  either compiler, writes a chunk that references an undeclared `__exportAll` (TS2304 at
  `index-*.d.ts(2847,8193)`), and drops the value meaning of `export * as JSONSchema`. These are rolldown
  bundling bugs, not compiler bugs.
- **Output size:** tsrs_dts bundles are 4–61% larger on four packages and 9% smaller on immer. Most of zod's
  difference is synthesized `declare namespace module_N { export { X_1 as X }; … }` blocks, one specifier per
  line.

## Method

Linux x64 cloud VM, Intel Xeon @ 2.80 GHz, 4 logical CPUs, 16 GiB; Node 22.22.0. tsrs at `4210a2e` (release
profile, for both the `bundle` example and the `tsrs` CLI, without fast mode). rolldown 1.2.15,
rolldown-plugin-dts 0.28.8, TypeScript 7.1.0-beta for tsgo and validation, TypeScript 6.0.3 for the tsc generator
and the export comparison. Single-CPU rows are pinned to CPU 3.

Two warmup rounds, then ten measured rounds. Each round runs every tool on every package in a fresh process,
rotating the tool order. Wall time, CPU (user + sys) and peak RSS come from GNU `time`. `perf` was unavailable,
so there are no instruction counts. The host is shared and noisy, which is why the summary uses paired ratios.

After timing, each tool's first output is type-checked as a consumer: all output declarations as roots, strict,
`skipLibCheck: false`. Each entry's exports and their value/type/namespace meanings are then compared with the
source entry's, through the TypeScript 6 API. The source entries have 27 (immer), 151 (query-core), 376
(trpc-server), 1231 (zod) and 362 (rxjs) exports.

## Results

Machine: Intel(R) Xeon(R) Processor @ 2.80GHz, 4 logical CPUs, 16 GiB, linux-x64. rolldown 1.2.15, rolldown-plugin-dts 0.28.8, typescript 6.0.3, typescript 7 7.1.0-beta, Node v22.22.0. tsrs commit 4210a2e5d625. 10 measured runs after 2 warmup(s); medians, with min–max for wall time. Wall / tsrs_dts is the median per-round ratio (min–max), then the rounds in which tsrs_dts was faster. CPUs "1" rows run under taskset on CPU 3. In-process ms spans the build and output writes, excluding process and runtime startup.

### immer (immer@10.1.1, 1 entry)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 426 (309–498) |  | 411 | 0.40 | 66 | 1 | 11900 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 271 (201–373) | 0.70× (0.55–0.97), 0/10 | 257 | 0.24 | 55 | 1 | 11900 | pass | yes |
| rolldown + plugin-dts, tsrs checking | 1 | 526 (432–612) | 1.31× (1.04–1.54), 10/10 | 342 | 0.51 | 106 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsrs checking | all | 439 (413–569) | 1.10× (0.83–1.37), 7/10 | 283 | 0.63 | 113 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 932 (738–1068) | 2.27× (1.48–3.00), 10/10 | 737 | 0.91 | 106 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 669 (563–701) | 1.60× (1.30–2.00), 10/10 | 510 | 1.51 | 112 | 1 | 13065 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsrs --noCheck | all | 377 (305–424) | 0.94× (0.72–1.09), 3/10 | 218 | 0.46 | 111 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsgo --noCheck | all | 455 (423–536) | 1.14× (0.89–1.43), 9/10 | 300 | 0.79 | 111 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 1805 (1555–2121) | 4.20× (3.70–6.45), 10/10 | 1632 | 2.98 | 244 | 1 | 13065 | pass | yes |

### query-core (@tanstack/query-core@5.105.0, 1 entry)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 808 (721–1073) |  | 791 | 0.77 | 124 | 1 | 216869 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 515 (445–718) | 0.64× (0.42–0.79), 0/10 | 503 | 0.48 | 99 | 1 | 216869 | pass | yes |
| rolldown + plugin-dts, tsrs checking | 1 | 1248 (1055–1517) | 1.56× (0.98–1.90), 9/10 | 1085 | 1.21 | 158 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsrs checking | all | 748 (588–978) | 0.88× (0.59–1.36), 2/10 | 550 | 1.39 | 172 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 2295 (1680–2683) | 2.92× (1.57–3.35), 10/10 | 2099 | 2.25 | 168 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1275 (1058–1501) | 1.50× (1.19–1.92), 10/10 | 1083 | 2.94 | 176 | 1 | 202871 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsrs --noCheck | all | 601 (474–813) | 0.72× (0.52–1.00), 0/10 | 418 | 1.07 | 175 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsgo --noCheck | all | 775 (614–865) | 0.93× (0.67–1.11), 3/10 | 582 | 1.60 | 173 | 1 | 202871 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 2406 (2258–2956) | 3.03× (2.10–3.92), 10/10 | 2205 | 4.78 | 346 | 1 | 202678 | pass | yes |

### trpc-server (@trpc/server@11.19.0, 6 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 884 (720–1027) |  | 862 | 0.86 | 127 | 7 | 123353 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 825 (690–958) | 0.90× (0.73–1.21), 3/10 | 808 | 0.79 | 127 | 7 | 123353 | pass | yes |
| rolldown + plugin-dts, tsrs checking | 1 | 1362 (1218–1669) | 1.62× (1.19–2.02), 10/10 | 1185 | 1.33 | 160 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsrs checking | all | 876 (671–1051) | 0.97× (0.70–1.21), 4/10 | 654 | 1.54 | 177 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 2402 (1952–2874) | 2.77× (1.90–3.11), 10/10 | 2115 | 2.35 | 173 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1510 (1339–1780) | 1.72× (1.32–2.19), 10/10 | 1273 | 3.29 | 180 | 8 | 104082 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsrs --noCheck | all | 671 (495–837) | 0.74× (0.49–1.16), 2/10 | 476 | 1.16 | 172 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsgo --noCheck | all | 825 (738–950) | 0.94× (0.77–1.08), 4/10 | 597 | 1.69 | 177 | 8 | 104082 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 3021 (2665–3990) | 3.42× (2.67–5.37), 10/10 | 2724 | 5.65 | 377 | 8 | 104086 | pass | yes |

### zod (zod@4.4.3, 3 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 1270 (1067–1408) |  | 1249 | 1.23 | 151 | 4 | 484497 | pass | 1 differ |
| tsrs_dts, reuse diagnostics | 1 | 1408 (1042–2046) | 1.11× (0.83–1.45), 7/10 | 1385 | 1.40 | 151 | 4 | 484497 | pass | 1 differ |
| rolldown + plugin-dts, tsrs checking | 1 | 2961 (2548–3844) | 2.41× (2.01–2.95), 10/10 | 2747 | 2.91 | 268 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsrs checking | all | 1502 (1208–1584) | 1.17× (1.03–1.36), 10/10 | 1327 | 3.27 | 295 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsgo checking | 1 | 4746 (3796–5508) | 3.62× (3.11–4.23), 10/10 | 4544 | 4.68 | 267 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsgo checking | all | 2394 (2099–2721) | 1.90× (1.73–2.06), 10/10 | 2212 | 6.24 | 301 | 4 | 300641 | 1 errors | 2 differ |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsrs --noCheck | all | 1025 (941–1325) | 0.87× (0.69–1.04), 1/10 | 853 | 2.15 | 293 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsgo --noCheck | all | 1267 (939–1600) | 1.05× (0.67–1.26), 6/10 | 1093 | 2.84 | 298 | 4 | 300641 | 1 errors | 2 differ |
| rolldown + plugin-dts, tsc 6 | all | 3195 (2628–4138) | 2.60× (2.16–3.25), 10/10 | 2989 | 7.04 | 473 | 4 | 300637 | 1 errors | 2 differ |

### rxjs (rxjs@7.8.2, 5 entries)

Same work: type-check, emit and bundle in one pass; any error fails the build:

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | 1 | 618 (460–691) |  | 601 | 0.59 | 87 | 6 | 338234 | pass | yes |
| tsrs_dts, reuse diagnostics | 1 | 649 (498–830) | 1.11× (0.79–1.34), 6/10 | 634 | 0.62 | 87 | 6 | 338234 | pass | yes |
| rolldown + plugin-dts, tsrs checking | 1 | 2352 (1945–2539) | 3.66× (3.08–5.20), 10/10 | 2083 | 2.31 | 235 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsrs checking | all | 1224 (1129–1433) | 2.07× (1.75–2.46), 10/10 | 993 | 2.33 | 243 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsgo checking | 1 | 3076 (2570–3873) | 5.03× (4.11–6.69), 10/10 | 2820 | 3.01 | 237 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsgo checking | all | 1714 (1577–1858) | 2.91× (2.43–3.87), 10/10 | 1500 | 3.64 | 243 | 9 | 326745 | pass | yes |

Emit and bundle without type checking (plugin defaults; tsrs_dts has no such mode):

| Tool | CPUs | Wall ms | Wall / tsrs_dts | In-process ms | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| rolldown + plugin-dts, tsrs --noCheck | all | 1139 (880–1489) | 1.83× (1.58–2.42), 10/10 | 894 | 2.17 | 243 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsgo --noCheck | all | 1288 (1114–1432) | 2.05× (1.82–3.12), 10/10 | 1039 | 2.58 | 244 | 9 | 326745 | pass | yes |
| rolldown + plugin-dts, tsc 6 | all | 2985 (2351–3847) | 4.65× (4.29–7.59), 10/10 | 2694 | 5.80 | 394 | 9 | 326757 | pass | yes |

Byte counts include all output files. tsrs_dts writes one facade per entry plus a shared bundle; rolldown writes
entries plus hashed chunks.

## Without type checking

tsrs_dts has no mode without checking. An uncommitted experiment skipped both of its diagnostic passes and built
the source Program with `noCheck`, which is what `--noCheck` does in tsgo. Its outputs had the same public API as
the checked builds, and every bundle type-checked; zod's only difference was the numbering of synthesized
namespaces. In a separate interleaved run, on an earlier build of this branch that still had an arena-region fix since
dropped, it took 124–269 ms against 352–1239 ms with checking, so
checking is 65–78% of tsrs_dts's time. That run measured rolldown + tsgo `--noCheck` at 3.6–6.1× the time of
unchecked tsrs_dts on four CPUs, and 4.3–11.5× on one CPU each.

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

- On small packages, tsrs_dts's second check of its emitted declarations is a visible cost: rolldown + tsrs CLI
  is faster in-process on immer. Skipping or narrowing that check would help most there.
- A no-check option in tsrs_dts would allow a like-for-like comparison with the plugin's default `--noCheck`
  build. See "Without type checking" above.
- Linker output size on namespace-heavy packages (zod): emitting synthesized namespace export lists more
  compactly would close most of the gap with rolldown.
- Re-measure on dedicated hardware with `perf` for instruction counts before citing small differences.
