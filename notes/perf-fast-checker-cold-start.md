# Fast mode at 16 checkers: every checker pays the same cold start

Measured 2026-11-02 at `ce090c5a` on an Apple M5 Max (6 Super + 12 Performance cores), shared with other agents (load
19-125 during the runs, stated per table). Question: why does `--fast` retire more instructions than `bun check` at full
parallelism on some projects, and what sets the wall-time floor there? No code change; the evidence for a later one.

## 1. Against bun check

npm tsrs 0.20.1 and bun 1.4.4 (oven-sh/bun#46001 is in it), 2026-11-01, load 50-345 (instructions only),
`/usr/bin/time -l`, medians of 2 interleaved runs, billions of instructions (kernel included):

| project | tsrs, 1 checker | `--fast`, 1 checker | bun, 1 thread | `--fast`, 16 checkers | bun, 18 threads |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | 78.9 | 68.9 | 87.4 | 79.9 | 89.0 |
| mikro-orm | 47.8 | 42.2 | 51.5 | 82.6 | 55.6 |
| t3code-server | 42.7 | 35.1 | 44.8 | 112.9 | 77.6 |
| mui-docs | 38.7 | 33.9 | 38.7 | 87.3 | 45.4 |
| webpack | 9.8 | 9.2 | 11.2 | 13.2 | 13.9 |
| nuxt | 9.9 | 8.7 | 11.4 | 15.3 | 13.3 |
| xstate-main | 5.9 | 5.3 | 6.3 | 7.0 | 7.3 |

On one thread bun does 1.00-1.16x tsrs's work. On 2026-10-14 it did 1.5-2.1x (bun 1.4.3). From 1 to 16 checkers
`--fast` grows 1.2-3.2x and bun grows 1.0-1.7x. At full parallelism bun does less work on mikro-orm, t3code-server,
mui-docs and nuxt.

Phases, one run each (tsrs: the `ce090c5a` build below, `--extendedDiagnostics` "Parse time" / "Check time"; bun
1.4.4 `--timing` "loaded in" / "checked in"):

| project | `--fast`, 4 checkers | `--fast`, 16 checkers | bun, 4 threads | bun, 18 threads |
| --- | --- | --- | --- | --- |
| t3code-server | 0.07 / 1.05 s | 0.07 / 1.02 s | 0.21 / 0.81 s | 0.07 / 0.67 s |
| mui-docs | 0.32 / 0.67 s | 0.27 / 0.52 s | 0.42 / 0.54 s | 0.22 / 0.33 s |

The front ends are even. Bun's check phase is 35-37% shorter at full parallelism, and it still shrinks from 4 to 18
threads (-18%, -38%) where tsrs's barely moves on t3code-server (-2%).

## 2. The checker-count curve

`--fast --checkers N` with a fat-LTO `--release` build of `ce090c5a`, medians of 3 rounds with the counts rotated
each round. Load 19-125, so walls are indicative: the three walls of a cell span at most 10% in 24 of the 32 cells
and up to 19% in the rest. Instructions are the load-independent measure.

| N | mui-docs wall / instr G / peak MiB | t3code-server | mikro-orm | vscode |
| ---: | --- | --- | --- | --- |
| 1 | 1.78 / 36.5 / 743 | 2.25 / 39.5 / 732 | 3.01 / 46.8 / 958 | 5.11 / 81.0 / 1576 |
| 2 | 1.28 / 45.1 / 862 | 1.55 / 50.8 / 906 | 1.66 / 51.4 / 1086 | 2.73 / 83.0 / 1666 |
| 4 | 0.98 / 57.4 / 1007 | 1.12 / 65.4 / 1126 | 1.04 / 58.9 / 1258 | 1.61 / 85.2 / 1744 |
| 6 | 0.93 / 66.8 / 1149 | 1.23 / 90.4 / 1407 | 0.83 / 65.4 / 1414 | 1.27 / 86.4 / 1812 |
| 8 | 0.95 / 77.9 / 1257 | 1.20 / 89.2 / 1477 | 0.75 / 71.8 / 1545 | 1.02 / 88.3 / 1886 |
| 10 | 0.95 / 84.9 / 1373 | 1.22 / 94.6 / 1570 | 0.71 / 75.2 / 1632 | 0.91 / 89.8 / 1926 |
| 12 | 0.91 / 88.0 / 1428 | 1.13 / 121.2 / 1883 | 0.62 / 79.1 / 1744 | 0.86 / 91.2 / 1977 |
| 16 | 0.86 / 92.1 / 1546 | 1.09 / 128.1 / 2063 | 0.61 / 90.0 / 1991 | 0.76 / 92.2 / 2025 |

vscode and mikro-orm keep scaling to 12-16 checkers. On t3code-server 4 checkers are as fast as 16 for half the
instructions and 55% of the memory; on mui-docs 6 are within 8% of 16. This repeats
[perf-fast-audit-20261027.md](perf-fast-audit-20261027.md) on another machine.

## 3. What the extra checkers do

`TSRS_FILE_TIMES` at 16 checkers (one run each), each checker's total and its single most expensive file:

- t3code-server: seven checkers each spend 0.55-0.71 s on one small entry file into the same Effect-based server:
  `src/binCli.ts` (398 nodes, 0.71 s), `cli/server.ts` (386 nodes, 0.63 s), `cli/project.test.ts`, `cli/app.test.ts`,
  `cli/pair.test.ts`, `cli/theme.test.ts`, `src/server.ts`. Those seven are the slowest checkers (0.89-1.02 s); the
  other nine finish in 0.42-0.45 s.
- mui-docs: eight checkers each spend 0.28-0.38 s on one small file of 120-1,450 nodes that uses the theme
  (`theming/ThemeNestingExtend.tsx`, `palette/UsingAugmentColor.tsx`, `localization/Locales.tsx`,
  `shared-theme/AppTheme.tsx`, ...). Those eight are the slowest checkers (0.39-0.52 s); the other eight finish in
  0.28-0.37 s.

Summed checker time grows from 4.06 s at 4 checkers to 10.85 s at 16 on t3code-server (0.57 s per added checker), and
from 2.66 s to 5.94 s on mui-docs (0.27 s per added checker). The Types counter doubles (t3code-server 2.69M ->
5.80M, mui-docs 1.33M -> 2.36M). The files on the critical path are small, and `src/binCli.ts` took 0.43 s at 4
checkers against 0.71 s at 16. The reading that fits: each of those files is its checker's first contact with a
large type graph that every checker resolves again. Only part of that cost can be split over more checkers, which is
why the check phase stops shrinking.

## 4. The shared-graph seed trades wall time for memory

The frozen seed of [spike-shared-graph.md](spike-shared-graph.md) (`--features tsrs_core/shared-graph`,
`TSRS_SHARED_GRAPH=1`) at 16 checkers under `--fast`, 3 interleaved rounds against the plain build and the feature
build with the switch off. Diagnostics were byte-identical in every run.

| project | instructions, seed 1% / 5% vs plain | peak, 1% / 5% | median wall, plain -> 1% / 5% |
| --- | --- | --- | --- |
| t3code-server | +1.6% / -6.0% | -7.8% / -15.7% | 1.12 -> 1.21 / 1.23 s |
| mui-docs | +8.9% / +7.7% | -1.5% / -2.7% | 0.90 -> 0.91 / 0.94 s |
| mikro-orm | -8.9% / -9.2% | -16.4% / -14.1% | 0.59 -> 0.64 / 0.78 s |
| vscode | +1.4% / +0.1% | -3.5% / -4.2% | 0.73 -> 0.79 / 0.94 s |

The feature build costs 3.7-6.0% instructions with the switch off. The seed's sample (light files, evenly spaced) misses
mui-docs's theme files, and every checker waits for it, so it trades wall time for memory, as on the 16-vCPU runner
in October.

## 5. The same seed, aimed at the files that pull in the shared types

A probe switch on branch `probe/seed-named-files` (not for landing): `TSRS_SHARED_GRAPH_SEED_FILES=<suffix>,...` seeds
exactly the eligible files whose names end with a suffix, ignoring the budget. The seed may not take a leaf file, so
the files of section 3 cannot be seeded themselves; the probe seeds the modules they import. 16 checkers under
`--fast`, 3 interleaved rounds against the plain build, load 46-54. Diagnostics were byte-identical in every run.

| project | seed files | median wall | instructions | peak |
| --- | --- | --- | --- | --- |
| t3code-server | `src/cli/server.ts`, `src/server.ts` (the suffix also takes `packages/contracts/src/server.ts`) | 1.05 -> 0.98 s (-7%) | 127.0 -> 53.6 G (-57.8%) | 2051 -> 1167 MiB (-43.1%) |
| t3code-server | the same plus `src/cli/{app,project,pair,theme}.ts` | 1.05 -> 0.97 s (-8%) | 127.0 -> 53.2 G (-58.1%) | 2051 -> 1157 MiB (-43.6%) |
| mui-docs | `shared-theme/AppTheme.tsx` | 0.87 -> 0.93 s (+8%) | 92.4 -> 57.7 G (-37.6%) | 1548 -> 1124 MiB (-27.4%) |
| mui-docs | the same plus `shared-theme/themePrimitives.ts`, `branding/brandingTheme.ts` | 0.87 -> 1.00 s (+16%) | 92.4 -> 53.1 G (-42.6%) | 1548 -> 1028 MiB (-33.6%) |

Two or three files carry most of the duplicated work. On t3code-server the seeded run retires fewer instructions at
16 checkers than bun does at 18 threads (53.6 G against 77.6 G) and is also faster than the plain run. On mui-docs the
seed's serial time costs more wall time than the parallel phase saves.

## 6. What would remove it

Bun pays the cold start once. In its source at 1.4.3 (`src/sema/driver/lib.rs`), a warm-up of light files sampled
over the program runs as steps of 1, 8 and 64 tasks; each step publishes what it resolved at a barrier, and later
tasks read it. The serial part is the first step only. tsrs's seed is one serial step that every checker waits for,
with no parallel ramp, and its default sample does not aim at the files that pull in the shared types.

What section 5 leaves open:

- Choosing the seed files without naming them. The files are composition roots (a server's wiring, a theme), not
  hubs with many importers; neither the evenly spaced default nor the hub-first rules of an unmerged 2026-10-13 probe
  found them on every project. A per-file cost from an earlier run (`--checkerCostCache`) or the size of a module's
  import closure are the candidates to measure.
- Not waiting for the seed. Under `--maxMemory` the pool already starts plain checkers and swaps in forks after the
  freeze; with a targeted seed that would keep mui-docs's parallel phase running during the seed.
- A parallel ramp that publishes in turn, as bun does, is the shared type layer of
  [design-shared-type-layer.md](design-shared-type-layer.md); `--fast` no longer keeps Go's counters exactly, which
  removes one obstacle, but not the order-dependent diagnostics.

## Reproduce

```sh
tsrs -p <project> --noEmit --incremental false --pretty false --fast --checkers N    # the curve, /usr/bin/time -l
TSRS_FILE_TIMES=files.tsv tsrs -p <project> --noEmit --incremental false --pretty false --fast --checkers 16 \
  --extendedDiagnostics                                                            # per-file times and phases
bun check -p <project> --no-pretty --all --no-script --timing --threads=N          # bun's phases
cargo build --release -p tsrs_cli -p tsrs_core --features tsrs_core/shared-graph   # sections 4 and 5
TSRS_SHARED_GRAPH=1 [TSRS_SHARED_GRAPH_SEED_PERCENT=5] tsrs ... --fast --checkers 16
TSRS_SHARED_GRAPH=1 TSRS_SHARED_GRAPH_SEED_FILES=src/cli/server.ts,src/server.ts tsrs ... # probe branch only
```
