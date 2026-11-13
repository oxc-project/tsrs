# perf-deep-files: the owner keeps the files at the top of the import graph

Question (2026-11-04): under `--fast` the check phase of t3code-server and mui-docs ends long after most checkers are
idle. Is that placement or stealing, and can the scheduler shorten it without sharing the checkers' type graphs
(notes/spike-shared-graph.md and notes/perf-checker-processes.md, both rejected for wall time)?

Answer: yes for t3code-server, partly for mui-docs. The files that cost a checker a whole library graph are the files
at the top of the import graph, and thieves take exactly those. Keeping them with their owner, which starts on them,
at 16 checkers: t3code-server -15.1% wall, -34.5% CPU, -38.5% instructions, -23.7% peak RSS; mui-docs -6.6%, -8.9%,
-10.0%, -3.6%. The other six projects are within the noise except cal-diy, whose check phase is 4-6% longer (wall
+2.8%, interval [-0.2%, +4.4%]) for the same CPU, instructions and memory.

Setup: one m8g.4xlarge (Graviton4, 16 vCPU, 64 GiB, Ubuntu 24.04), `cargo build --release` of base 48cf809ef and of
the branch in the same directory, `-p <project> --noEmit --incremental false --pretty false --fast`, 16 checkers (the
default on 16 vCPUs) unless stated.

## 1. The timeline

`TSRS_FILE_TIMES` now ends each line with the file's start (seconds from the start of the pass), the queue it was taken
from and its scheduling weight. Base t3code-server, one run (check 2.10 s; runs varied 1.90-2.10 s):

| file | weight | queue | checker | start s | seconds |
| --- | ---: | --- | --- | ---: | ---: |
| `apps/server/src/binCli.ts` (398 nodes) | 28,678 | 14 | 14 (owner) | 0.245 | 1.410 |
| `apps/server/src/ws.ts` | 204,970 | 6 | 6 (owner) | 0.179 | 0.813 |
| `apps/server/src/cli/project.test.ts` | 40,804 | 6 | 4 (thief) | 0.486 | 1.306 |
| `apps/server/src/cli/server.ts` | 11,632 | 14 | 9 (thief) | 0.549 | 1.247 |
| `apps/server/src/cli/pair.test.ts` | 24,560 | 14 | 10 (thief) | 0.626 | 1.346 |
| `apps/server/src/server.ts` | 171,556 | 6 | 13 (thief) | 0.628 | 1.233 |
| `apps/server/src/cli/theme.test.ts` | 25,108 | 14 | 8 (thief) | 0.629 | 1.472 |
| `apps/server/src/cli/app.test.ts` | 28,434 | 14 | 15 (thief) | 0.635 | 1.283 |

Nine checkers end at 0.64-0.68 s with nothing left to steal (stealing moves unstarted files only); the pass ends at
0.629 + 1.472. Single-threaded the same files cost `ws.ts` 0.58 s and then 0.001-0.023 s each: all of them need the
types of the server's composed Effect layers, which the first of them builds. With all eight on one checker (a static
`--checkerAssignment file:` run, no stealing): `ws.ts` 0.76 s, `server.ts` 0.54 s, the other six 0.001-0.046 s. With
stealing, six checkers built that graph again, each for one file.

They are stolen because a checker visits its queue in program order (dependencies first) and thieves take from the
back, where the files that import the most of the program sit: entry points, composition roots, the tests of whole
services.

mui-docs has the same shape with a smaller graph: nine checkers each spend 0.5-0.6 s on a demo that builds the MUI
theme types (`createTheme({...theme, ...})`); single-threaded only the first such file costs 0.5 s
(`CoreShowcase.tsx`), the others 0.04-0.07 s. The pass ends with `AppTheme.tsx`, stolen at 0.368 s from the queue
whose owner was building that graph for `dashboard-template-theme.tsx` (0.55 s on the thief).

The other projects have no such tail: the busiest checker ends 0-18% after the mean (vscode 1.00, xstate-main 1.05,
supabase-studio 1.10, mikro-orm 1.11, webpack 1.13, cal-diy 1.18), against 1.74 on t3code-server and 1.49 on mui-docs.

## 2. The rule

Static weight misses these files (`binCli.ts` has 398 nodes). Transitive import closure finds them on t3code-server
(ranks 0-7 of 1,518 checked files) but costs O(files x closure), says nothing on mui-docs (the theme demos sit at the
median) and ties hundreds of files at the maximum under large import cycles (cal-diy 268 within 90% of the largest,
webpack 695); it was already rejected as a cost model (notes/perf-balance.md).

Import depth, the longest chain of imports below a file over the strongly connected components (Tarjan,
O(files + imports)), puts the seven expensive t3code-server files other than `ws.ts` at depth 60-63 of 63 (with
`bin.ts` and five cheap MCP tests). On mui-docs it finds `dashboard-template-theme.tsx`, `Dashboard.tsx` and
`CrudDashboard.tsx` (57 of 58) but not `AppTheme.tsx`, `Locales.tsx`, `UsingAugmentColor.tsx` or
`ThemeNestingExtend.tsx` (32-35). On the other projects the deepest files are cheap (0.000-0.05 s).

What landed (`checkerpool.rs`: `import_depths`, `deep_files`, the pinned prefix of `FileQueue`): with the locality
assignment, so under `--fast` only, a deep file is a checked file at depth >= 95% of the deepest checked file's, plus
the checked files of the same checker it imports directly. In the type-check pass with stealing each owner's queue
starts with its deep files and thieves do not take them, up to half an average share of pinned weight per queue.
`TSRS_DEEP_FILES=0` turns it off.

Files at depth >= 95%, per project at 16 checkers: t3code-server 13 (9.5 of 19.3 s of checker time), mui-docs 9 (1.6
of 9.9 s), vscode 12 (0.05 s), cal-diy 31 (0.26 s), supabase-studio 97 (0.13 s), mikro-orm 30 (0.04 s), webpack 3,
xstate-main 19. A fixed number of levels below the maximum marks too much on shallow programs (3 levels: webpack 712
files of 909, mikro-orm 225). The pinned weight per queue, with the imports, is 15-52% of an average share on
t3code-server's busiest queues, under 11% on mui-docs, up to 20% on cal-diy, 29% on supabase-studio (its 97 cheap
route files and their imports), 15% on vscode and 48% on one mikro-orm queue (its 28 deepest CLI tests); the cap of
half a share binds only on t3code-server's queue 6.

## 3. Variants measured and rejected

Prototype knobs (since removed), 3-5 interleaved runs per cell, medians of `Check time` (`--extendedDiagnostics`),
CPU (user + sys) and peak RSS against base in the same batch:

| variant | t3code-server check / CPU / RSS | mui-docs check / CPU | cal-diy check / CPU | others |
| --- | --- | --- | --- | --- |
| front: deep files first in their owner's queue, stealable | +5.2% / +2.1% / +1.1% | +4.1% / +1.5% | | |
| pin: first and kept from thieves | -21% / -30% / -20% | -1% / -5% | +2 to +8% / +1 to +2% | within 2-5% |
| spread: one deep file per checker, first | -21% / +6% / +7% | +11% / 0% | | |
| group: all deep files first on one checker, kept | -17% / -38% / -25% | -0.5% / -9% | -2.5% / +2.6% | mikro-orm +146%, webpack +400% |
| keep: in their place, thieves skip them, the owner checks them | -13% / -34% / -23% | 0% / -4% | +5% / 0% | webpack +4.6% |
| **pin, plus the deep files' direct imports in the same queue (landed)** | **-16% / -34% / -24%** | **-8% / -9%** | +2.3 to +6.1% / +0.3 to +1.3% | vscode 0 to -1.7%, others within 2.5% |

- front: worse than base, because thieves still reach the deep files from the back half a second in.
- spread: the same wall as pin, but every checker pays a cold start (CPU +6%).
- group: pays the cold start once, but that checker becomes the tail; with the fixed-level rule used then it pinned
  hundreds of files and serialized webpack and mikro-orm.
- keep: keeps cold work off t=0 but loses a third of t3code-server's gain (`binCli.ts` and `server.ts` start later)
  and does not help cal-diy.
- the direct imports: give mui-docs its gain (`AppTheme.tsx` stays with the checker that has the theme graph) for a
  few points on t3code-server (the CLI modules are pinned too).
- depth 0.9, or 1, 3, 6 or 10 levels below the maximum: 3-6 levels match 0.95 on t3code-server, 1 level gets -9%,
  10 levels and 0.9 pin hundreds of files on the other projects.
- the depth-picked files ordered before the files they import: within 1.1% of the landed order on five projects
  (7 runs each).

cal-diy: its deep files cost 0.27 s in total and are cheap where base checks them (late, on warm checkers); at t=0
they cost 0.6-0.7 s (`EventAvailabilityTab.tsx` 0.234 s against 0.029 s), and the pass's tail, a late steal of a
0.17-0.21 s file from its busiest queue, starts 20-50 ms later. Total CPU is unchanged; at 4 and 12 checkers the same
change is 3% faster there.

## 4. Results

Paired A/B, base against the branch, 15 alternating pairs per project: median of paired ratios with a ~95% interval.
The check-phase column is a separate interleaved batch with `--extendedDiagnostics`, 7 runs per binary.

| project | wall | CPU (user + sys) | instructions | peak RSS | check phase |
| --- | --- | --- | --- | --- | --- |
| t3code-server | 2.19 -> 1.86 s, **-15.1%** [-18.3, -13.5] | **-34.5%** [-35.0, -33.7] | 123.6 -> 75.9 G, **-38.5%** [-39.0, -38.0] | 2012 -> 1526 MiB, **-23.7%** [-24.9, -22.9] | 2.019 -> 1.669 s, -17.3% |
| mui-docs | 1.25 -> 1.17 s, **-6.6%** [-8.0, -5.3] | **-8.9%** [-9.8, -7.7] | 84.2 -> 75.9 G, **-10.0%** [-10.2, -9.6] | 1505 -> 1451 MiB, **-3.6%** [-3.8, -3.1] | 0.940 -> 0.848 s, -9.8% |
| vscode | +0.2% [-1.2, +1.3] | +0.0% [-1.4, +1.4] | +0.0% [-0.1, +0.2] | -0.2% [-0.3, -0.1] | +1.2% |
| supabase-studio | -1.2% [-2.0, +1.1] | -0.5% [-1.3, +0.9] | +0.0% [-0.4, +0.4] | +0.0% [-0.2, +0.1] | +0.3% |
| mikro-orm | +0.4% [-0.9, +5.9] | -1.3% [-2.1, +0.6] | -1.6% [-1.8, +0.1] | -1.4% [-2.1, +0.1] | -1.0% |
| cal-diy | +2.8% [-0.2, +4.4] | -0.4% [-0.9, +1.5] | +0.4% [-0.4, +1.9] | +0.3% [-0.1, +1.6] | +4.0% (+6.1% in a second batch) |
| webpack | +1.2% [-1.0, +3.5] | +0.4% [-1.2, +1.6] | +0.8% [-1.0, +1.5] | -0.3% [-1.1, +1.2] | +2.3% (3 ms) |
| xstate-main | +0.1% [-0.4, +3.2] | +0.6% [+0.3, +1.4] | +0.1% [-0.0, +0.2] | +0.8% [-1.0, +1.6] | +2.2% (3 ms) |

Default mode (Go's assignment, 4 checkers), `--singleThreaded` and `--fast --singleThreaded`, 3 pairs each:
instructions within 0.1% on all eight projects.

Checker count sweep (prototype with the same rule, 4 interleaved runs per cell, check-phase medians):

| checkers | t3code-server | mui-docs | cal-diy | vscode | supabase-studio |
| --- | --- | --- | --- | --- | --- |
| 4 | -3.9% (CPU -1.4%) | -0.9% | -3.2% | 0.0% | +0.8% |
| 8 | -32% (CPU -15%, RSS -9%) | -5.9% (CPU -8%) | +1.5% | -1.9% | +0.4% |
| 12 | -23% (CPU -16%, RSS -10%) | +7.4% (CPU -6%) | -3.3% | +0.6% | -0.1% |

mui-docs at 12 is the tail lottery: in a timeline run both binaries ended at 1.01 s on the same late file
(`UsingAugmentColor.tsx`, owner, 0.515 s in), with 12% less mean checker time on the branch.

## 5. Output

`--pretty false` output and exit status, base against the branch, at 16, 8 and 4 checkers with `--fast`, with
`--fast --singleThreaded` and in the default mode, the branch three times per cell: byte-identical in all 40 cells
(vscode 448 lines, webpack 1,035, cal-diy 164, t3code-server 11, mikro-orm 11, supabase-studio 9; mui-docs and
xstate-main clean). The fast-mode conformance run has no failure or crash in any baseline kind; its one timeout,
`compiler/intersectionConstructorReductionCrash`, takes 160 s in the unoptimized build with Go history and with fast
mode alike (the limit is 120 s) and passes with a longer limit. The scheduling sweep (`testdata/checker-determinism.txt` at 4 and 8 checkers, ten scheduler runs and eight
seeded assignments each, 36 runs) and the regression sweep (every `testdata/regressions` fixture single-threaded, then
at 2 and 4 checkers under three scheduler runs and six seeded assignments, 950 invocations over 50 fixtures) pass.

## 6. Not done

- mui-docs' other theme-heavy demos (`UsingAugmentColor.tsx`, `ThemeNestingExtend.tsx`, `Locales.tsx`,
  `CoreShowcase.tsx`) are mid-depth and import nothing that marks them; 149 files call `createTheme` and a handful are
  expensive. Only a measured cost (the opt-in `--checkerCostCache`) would find them.
- cal-diy's tail (late steals of 0.17-0.21 s files from its busiest queue) is the stealing lottery, present in base too.
- A checker count chosen by the critical path: with the deep files pinned, t3code-server's pass is floored by one cold
  start of about 1.5 s at t=0, so fewer checkers would likely lose little wall time and save memory. Not measured.
