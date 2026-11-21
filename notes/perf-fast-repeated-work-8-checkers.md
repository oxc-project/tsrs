# perf-fast-repeated-work-8-checkers: what eight checkers repeat on a large monolith, and what the shared-graph seed buys there

Measured 2026-11-12 at main 221ee7bf on AWS `m8g.4xlarge` boxes (Graviton4, 16 vCPU, 64 GiB, Ubuntu 24.04), pinned to
8 cores with `taskset -c 0-7` unless a row says otherwise, on a large private monorepo: one application checked with
`-p tsconfig.json --noEmit --incremental false --pretty false --fast`. Every figure for that program is relative
(ratios and percent changes between runs on the same box). No code change; this is the evidence for a later one.

Question: under `--fast`, how much of the check phase at 8 and 4 cores is work another checker also did, where does
it sit, and does the frozen seed of [spike-shared-graph.md](spike-shared-graph.md) turn it into wall time?

Answer: 8 checkers do 1.69x the checking work of one, half of the excess in 0.1% of the files. The seed removes most
of it (CPU -19 to -27%, peak RSS -21 to -27%, output identical) but wall time gets worse at 8 and 4 cores, because the
shared part of this program takes one thread about half of the baseline's whole check phase to build and only 2-6%
of the checking is independent of it. On this program the seed is a memory and CPU lever, not a wall-time one, until
the shared part is built by several threads.

## 1. The check phase is CPU-bound at 8 and 4 cores

The check phase is 89% of wall time at 8 cores and at 4, and the checker threads are busy for about 95% of it, so
wall time is the checkers' CPU time divided by the core count. Against one checker on the same box and binary
(release build with frame pointers, under `perf record`; thread CPU summed from `TSRS_FILE_TIMES`):

| | 1 checker | 8 checkers | ratio |
| --- | --- | --- | --- |
| checker thread CPU | 1.00 | 1.69 | 1.69x |
| `Types` | 1.00 | 1.90 | 1.90x |
| `Instantiations` | 1.00 | 2.39 | 2.39x |
| `Symbols` | 1.00 | 1.61 | 1.61x |

With plain builds and no profiler, process CPU at 4 / 8 / 16 cores is 1.00 / 1.26 / 1.62 and peak RSS 1.00 / 1.26 /
1.62 (both relative to 4 cores). 41% of the checker CPU at 8 cores is work some other checker also did.

## 2. Where the repeated work is

Per file, the extra is its thread CPU at 8 checkers minus at 1 (same build). It is concentrated:

| files, by extra | share of the extra |
| --- | --- |
| top 0.1% | 50% |
| top 0.5% | 74% |
| top 1% | 81% |
| top 5% | 93% |
| top 20% | 99% |

Each checker carries about the same amount: 34-47% of every checker's CPU is extra. The files at the top are small
and cost next to nothing with one checker: they are the first place where their checker meets a large type that
another checker already built. They are not at the start of the queues (the largest sits in the middle of its
checker's), so it is not only a cold start at the head of a queue (the cold-start note of #869); it is first
contact, wherever it happens.

By function, inclusive samples at 1 and 8 checkers (`perf record -F 499`, frame pointers; stacks the unwinder lost
make the profile's total ratio lower than the timers' 1.69x):

| function | 8 checkers / 1 checker |
| --- | --- |
| `get_diagnostics` (everything a checker does) | 1.52x |
| `get_type_of_variable_or_parameter_or_property` | 2.58x |
| `check_declaration_initializer` | 2.39x |
| `get_type_of_alias` | 8.2x |
| `get_narrowed_type_of_symbol` | 5.8x |
| `instantiate_type_with_alias_worker` | 2.49x |
| `get_conditional_type` | 2.84x |
| `recursive_type_related_to` | 2.25x |

The growth under `get_type_of_variable_or_parameter_or_property` is as large as the growth of `get_diagnostics`
itself: essentially all of the excess is below it. The repeated work is a checker inferring the type of an exported `const` from its initializer (call chains, overload
resolution, inference, the relation checks and conditional types under them) for a declaration another checker owns
and has already inferred. Declared types of annotated exports are cheap to resolve again; inferred ones are not.

## 3. The frozen seed on this program

Feature build (`--features tsrs_core/shared-graph`) of the same commit with the probe switch of branch
`probe/seed-named-files` applied; single runs, 8 cores, against the plain build on the same box (two plain runs
differed by 1.4% in wall and 1.6% in CPU). Diagnostics identical in every run. "Top N" seeds the N non-leaf files with
the largest extra of section 2.

| run | CPU | peak RSS | wall | seed time, share of the plain check phase |
| --- | --- | --- | --- | --- |
| feature compiled in, switch off | +2.6% | 0% | +1.1% | |
| `TSRS_SHARED_GRAPH=1`, default 1% seed | -19% | -21% | +19% | 50% |
| 5% seed | -25% | -24% | +41% | 83% |
| top 5 files | -16% | -19% | +21% | 46% |
| top 10 | -18% | -21% | +19% | 47% |
| top 25 | -22% | -21% | +26% | 60% |
| top 50 | -24% | -23% | +30% | 69% |
| top 100 | -25% | -25% | +37% | 77% |
| top 200 | -27% | -27% | +48% | 94% |

The parallel part after the seed shrinks as predicted (to 72% of the plain check phase with the 1% seed, 61% with
the top 200), and with the top 200 a fork does about what a checker does when there is only one. But the pool waits
for the seed, and no seed is shorter than 46% of the plain check phase: five files are enough to pull in the core of
what the checkers share, and building it once on one thread takes that long.

Not waiting does not help either. With `--maxMemory 60G` (no retirement for memory, but the pool starts plain
checkers and replaces each by a fork at its first file boundary after the freeze), against the plain build with the
same `--maxMemory` (which alone costs +10% wall here):

| run | CPU | peak RSS | wall |
| --- | --- | --- | --- |
| 1% seed, pool does not wait | +7% | -24% | +2% |
| 5% seed | +12% | -16% | +6% |
| top 10 | +10% | -28% | +6% |
| top 50 | +9% | -23% | +5% |

The plain checkers spend the seed's time meeting the same shared types, and that state is dropped when they become
forks.

At 4 cores (`taskset -c 0-3`), 1% seed with the pool waiting: CPU -8%, peak RSS -12%, wall +14%.

## 4. Why wall time cannot follow on this program

A waiting pool wins only if the seed is shorter than the repeated work each checker would otherwise do in parallel,
and a pool that does not wait wins only if the other threads have work that needs nothing from the seed. Neither
holds here:

- The smallest useful seed takes one thread 46% of the plain check phase; each checker's own repeated work is
  34-47% of its time and already runs in parallel.
- From `--explainFiles` (who imports whom) and the per-file CPU of the one-checker run: the files that cannot reach
  any module imported by the top first-contact files hold 2-6% of the one-checker CPU. There is nothing for seven
  threads to do for the length of the seed.
- A profile of the seed thread alone is the ordinary inference path (97% under `check_expression`, 85% under
  `check_call_expression`, 57% under relation checks, 45% under conditional types). No single function to fix.

So the remaining wall time is in building the shared part with several threads, which the frozen seed cannot do: it
is one checker on one thread, and two seeds cannot be merged because both would create the same declared types.
That is the live shared layer of [design-shared-type-layer.md](design-shared-type-layer.md), not a scheduling change.

## 5. What the seed is good for today

Memory, and through it wall time where memory sets the checker count. On another box, the same 8 cores, plain
`--checkers k` against the feature build with the 1% seed and `--checkers k` forks (3 runs each, medians; the pool
waits for the seed):

| peak RSS, relative to 8 plain checkers | plain | seeded |
| --- | --- | --- |
| 1.00 | 8 checkers, wall 1.00 | |
| 0.90 | 6 checkers, wall 1.21 | |
| 0.85 | 5 checkers, wall 1.31 | |
| 0.80 | 4 checkers, wall 1.50 | 8 forks (0.79), wall 1.32 |
| 0.74 | 3 checkers, wall 1.87 | 6 forks (0.75), wall 1.43 |
| 0.67-0.71 | 2 checkers (0.67), wall 2.62 | 4 forks (0.71), wall 1.80 |

At the memory of four plain checkers a seeded pool of eight is 12% faster; at three's, a seeded six is 24% faster;
a seeded four takes 6% more memory than two plain checkers and is 31% faster. So where a machine's memory, not its
cores, sets the checker count, the seed already pays in wall time. It needs the feature build (+2.6% CPU with the
switch off), which is why it is not in the default binary.

## 6. How Bun shares, read again

Earlier notes describe `bun check` as paying the cold start once through a warm-up in steps of 1, 8 and 64 tasks
that publish at barriers. That is bun-v1.4.3 (`warm_up_files: 1 + 8 + 64` in `src/sema/driver/lib.rs`). It is gone
in bun-v1.4.4 (2026-10-22), the version later notes benchmarked, and at oven-sh/bun 43850629 (2026-11-11) there is
no warm-up at all. What that source does instead (`src/sema/check/commit.rs`, `src/sema/types.rs`):

- Types, signatures and mappers are hash-consed in one program-wide store ("Equal ones have equal ids"), so two
  threads that build the same instantiation get the same record. That covers library generics instantiated with
  project types, which no seed can hold.
- A task commits what it computed after every file, under one mutex; the first entry for a key wins, a commit that
  conflicts with an earlier one is discarded and its file checked again, and a last round runs on one thread.
- Because ids depend on timing, nothing may depend on id order: types are ordered by content
  (`Types::content_order`).

So Bun builds the shared part with all its threads, which is what section 4 says this program needs, and it gets
there with content-addressed identity, records that never change after creation and recheck rules. Content order is
not tsgo's union order, so that design cannot print byte-identical diagnostics; it is a different checker, not a
scheduling change to this one.

## Reproduce

```sh
# sections 1 and 2: per-file thread CPU and profiles at 1 and 8 checkers
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release --locked -p tsrs_cli \
  --config 'profile.release.debug="line-tables-only"' --config profile.release.strip=false
TSRS_FILE_TIMES=files-N.tsv perf record -F 499 -g --call-graph fp -o perfN.data \
  taskset -c 0-7 tsrs -p tsconfig.json --noEmit --incremental false --pretty false --fast --checkers N
# section 3: the seed (probe/seed-named-files adds TSRS_SHARED_GRAPH_SEED_FILES=<suffix>,...)
cargo build --release --locked -p tsrs_cli -p tsrs_core --features tsrs_core/shared-graph
TSRS_SHARED_GRAPH=1 [TSRS_SHARED_GRAPH_SEED_PERCENT=5] taskset -c 0-7 tsrs ... --fast [--maxMemory 60G]
# section 4: tsrs ... --explainFiles gives the import edges
```

The join of the two `TSRS_FILE_TIMES` tables, the profile diff and the reachability walk were ad hoc JavaScript and
are not committed.
