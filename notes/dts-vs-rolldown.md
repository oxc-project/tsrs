# tsrs_dts against rolldown + rolldown-plugin-dts on published packages

Benchmark: [`bench/dts/`](../bench/dts), described in [docs/DTS.md](../docs/DTS.md#benchmark). Inputs are five
published npm packages that ship their TypeScript sources, pinned by tarball integrity in
[`projects.json`](../bench/dts/projects.json), each bundled from the same entries and tsconfig by every tool.

## Summary

- **Same work (check, then bundle):** tsrs_dts takes 1.7–3.4× less wall time than `tsgo --noEmit` followed by
  rolldown with the tsgo generator, and 7.6–12.4× less than the TypeScript 6 equivalent, on all five packages.
- **Against bundling alone:** tsrs_dts still type-checks, and rolldown-plugin-dts does not. Even so, tsrs_dts is
  faster than rolldown's tsgo generator on immer (1.49×), trpc-server (1.19×) and rxjs (2.06×), level on zod
  (1.05×, overlapping ranges) and slower on query-core (0.81×). It is 2.6–5.0× faster than the tsc generator.
- **CPU and memory:** tsrs_dts runs one checker, so its CPU time is about its wall time. The tsgo pipelines use
  1.7–2.6× more CPU than wall time on four cores. tsrs_dts peaks at 55–150 MiB, against 113–298 MiB for the tsgo
  generator and 246–521 MiB for the tsc generator.
- **Diagnostic reuse** cuts immer by 21% and query-core by 40%. On trpc-server, zod and rxjs it is within noise
  (+3–5%, overlapping ranges). Output bytes are identical with reuse on and off.
- **Output correctness:** every tsrs_dts bundle type-checks as a consumer and keeps every entry export
  with its meaning, apart from one harmless widening every tool shares (below). On zod,
  every rolldown variant writes a chunk that references an undeclared `__exportAll` (TS2304 at
  `index-*.d.ts(2847,8193)`), and its `JSONSchema` export (`export * as JSONSchema`) loses its value meaning.
- **Output size:** tsrs_dts bundles are 4–61% larger on four packages and 9% smaller on immer. Most of zod's
  difference is synthesized `declare namespace module_N { export { X_1 as X }; … }` blocks, one specifier per
  line: 236 KB of `export {` lines against 67 KB for rolldown. With whitespace removed, the tsrs_dts output is
  still 56% larger, so indentation (4 spaces against 2) accounts for little of the gap.

## Method

Linux x64 cloud container, Intel Xeon @ 2.80 GHz, 4 logical CPUs, 16 GiB; Node 22.22.0. tsrs at `fcf19ce`
(release profile, `bundle` example: mimalloc and a 512 MiB stack like the CLI). rolldown 1.2.15,
rolldown-plugin-dts 0.28.8, TypeScript 6.0.3 for the tsc generator and checks, TypeScript 7.1.0-beta (the version
tsrs ports) for the tsgo generator, checks and output validation.

Two warmup rounds, then ten measured rounds. Each round runs every tool on every package in a fresh process,
rotating tool order; each sample writes its outputs to disk. Wall time, CPU (user + sys) and peak RSS come from GNU
`time`. `perf` was unavailable, so there are no instruction counts. The host is a shared VM: compare ranges, not
just medians, and treat differences within about 10% as inconclusive.

After timing, each tool's first output is type-checked as a consumer (all output declarations as roots, strict,
`skipLibCheck: false`). Each entry's exports and their value/type/namespace meanings are then compared with the
source entry's, through the TypeScript 6 API. The source entries have 27 (immer), 151 (query-core), 376
(trpc-server), 1231 (zod) and 362 (rxjs) exports.

## Results

Machine: Intel(R) Xeon(R) Processor @ 2.80GHz, 4 logical CPUs, 16 GiB, linux-x64. rolldown 1.2.15, rolldown-plugin-dts 0.28.8, typescript 6.0.3, typescript 7 7.1.0-beta, Node v22.22.0. tsrs commit fcf19ce6b94d. 10 measured runs after 2 warmup(s); medians, with min–max for wall time.

### immer (immer@10.1.1, 1 entry)

| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | yes | 308 (273–406) | 1.00× | 0.29 | 66 | 1 | 11900 | pass | yes |
| tsrs_dts (reuse diagnostics) | yes | 242 (204–292) | 0.79× | 0.22 | 55 | 1 | 11900 | pass | yes |
| rolldown + plugin-dts (tsc 6) | no | 1542 (1446–1713) | 5.01× | 2.67 | 246 | 1 | 13065 | pass | yes |
| rolldown + plugin-dts (tsgo 7) | no | 457 (388–494) | 1.49× | 0.78 | 113 | 1 | 13065 | pass | yes |
| tsc 6 --noEmit && rolldown (tsc) | yes | 3792 (3440–4112) | 12.31× | 7.82 | 247 | 1 | 13065 | pass | yes |
| tsgo 7 --noEmit && rolldown (tsgo) | yes | 714 (594–769) | 2.32× | 1.78 | 113 | 1 | 13065 | pass | yes |

### query-core (@tanstack/query-core@5.105.0, 1 entry)

| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | yes | 809 (671–860) | 1.00× | 0.78 | 124 | 1 | 216869 | pass | yes |
| tsrs_dts (reuse diagnostics) | yes | 481 (435–589) | 0.60× | 0.46 | 99 | 1 | 216869 | pass | yes |
| rolldown + plugin-dts (tsc 6) | no | 2226 (1970–2369) | 2.75× | 4.36 | 348 | 1 | 202678 | pass | yes |
| rolldown + plugin-dts (tsgo 7) | no | 653 (565–925) | 0.81× | 1.38 | 173 | 1 | 202871 | pass | yes |
| tsc 6 --noEmit && rolldown (tsc) | yes | 6124 (5682–6846) | 7.57× | 13.57 | 341 | 1 | 202678 | pass | yes |
| tsgo 7 --noEmit && rolldown (tsgo) | yes | 1385 (1282–1447) | 1.71× | 3.30 | 174 | 1 | 202871 | pass | yes |

### trpc-server (@trpc/server@11.19.0, 6 entries)

| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | yes | 710 (652–914) | 1.00× | 0.68 | 127 | 7 | 123353 | pass | yes |
| tsrs_dts (reuse diagnostics) | yes | 744 (667–846) | 1.05× | 0.72 | 127 | 7 | 123353 | pass | yes |
| rolldown + plugin-dts (tsc 6) | no | 2457 (2273–2625) | 3.46× | 4.71 | 374 | 8 | 104086 | pass | yes |
| rolldown + plugin-dts (tsgo 7) | no | 843 (608–1015) | 1.19× | 1.69 | 182 | 8 | 104082 | pass | yes |
| tsc 6 --noEmit && rolldown (tsc) | yes | 7333 (6603–7866) | 10.33× | 15.39 | 448 | 8 | 104086 | pass | yes |
| tsgo 7 --noEmit && rolldown (tsgo) | yes | 1544 (1391–1934) | 2.17× | 3.60 | 177 | 8 | 104082 | pass | yes |

### zod (zod@4.4.3, 3 entries)

| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | yes | 1122 (1062–1258) | 1.00× | 1.08 | 150 | 4 | 484497 | pass | 1 differ |
| tsrs_dts (reuse diagnostics) | yes | 1180 (997–1293) | 1.05× | 1.14 | 150 | 4 | 484497 | pass | 1 differ |
| rolldown + plugin-dts (tsc 6) | no | 2875 (2742–3310) | 2.56× | 6.38 | 483 | 4 | 300637 | 1 errors | 2 differ |
| rolldown + plugin-dts (tsgo 7) | no | 1174 (943–1372) | 1.05× | 2.58 | 298 | 4 | 300641 | 1 errors | 2 differ |
| tsc 6 --noEmit && rolldown (tsc) | yes | 10010 (9411–10496) | 8.92× | 21.03 | 521 | 4 | 300637 | 1 errors | 2 differ |
| tsgo 7 --noEmit && rolldown (tsgo) | yes | 2444 (2044–2555) | 2.18× | 6.33 | 292 | 4 | 300641 | 1 errors | 2 differ |

### rxjs (rxjs@7.8.2, 5 entries)

| Tool | Type checks | Wall ms | vs tsrs_dts | CPU s | Peak RSS MiB | Files | Bytes | Bundle checks | Exports match source |
| --- | :---: | ---: | ---: | ---: | ---: | ---: | ---: | :---: | :---: |
| tsrs_dts | yes | 513 (480–752) | 1.00× | 0.49 | 87 | 6 | 338234 | pass | yes |
| tsrs_dts (reuse diagnostics) | yes | 530 (459–563) | 1.03× | 0.50 | 87 | 6 | 338234 | pass | yes |
| rolldown + plugin-dts (tsc 6) | no | 2565 (2334–3601) | 5.00× | 5.06 | 393 | 9 | 326757 | pass | yes |
| rolldown + plugin-dts (tsgo 7) | no | 1057 (968–1177) | 2.06× | 2.17 | 246 | 9 | 326745 | pass | yes |
| tsc 6 --noEmit && rolldown (tsc) | yes | 6354 (5742–6799) | 12.38× | 12.99 | 436 | 9 | 326757 | pass | yes |
| tsgo 7 --noEmit && rolldown (tsgo) | yes | 1723 (1586–1826) | 3.36× | 3.65 | 245 | 9 | 326745 | pass | yes |

"Type checks" means the row reports type errors in the sources. rolldown-plugin-dts never does: its tsc generator
only emits and its tsgo generator passes `--noCheck`. The `--noEmit &&` rows add that check, matching tsrs_dts's
work. Byte counts include all output files; tsrs_dts writes one facade per entry plus a shared bundle, while
rolldown writes entries plus hashed chunks.

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

- tsrs_dts errors print `TS<code>: <message>` without a file or position (`project::diagnostics`). Locating the
  effect failure needed a separate `tsrs --noEmit` run.
- Linker output size on namespace-heavy packages (zod): emitting synthesized namespace export lists more
  compactly would close most of the gap with rolldown.
- Re-measure on dedicated hardware with `perf` for instruction counts before citing small differences.
