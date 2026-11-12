# Type check and type-aware lint in one run, without the second pass (2026-11-12)

Follow-up to [one lint pass instead of lint plus a separate typecheck](perf-tsgolint-check-program.md), the
[shared checker](perf-tsgolint-shared-checker.md) and the stealing experiment on branch `perf/lint-steal`
(`git show origin/perf/lint-steal:notes/perf-tsgolint-steal.md`, pull request #896).

`oxlint --type-aware --type-check` with tsrs-lint as the backend builds each tsconfig program once, then checks and
lints it, so one run can replace lint plus `tsrs --noEmit`. On main that run still cost far more than the check alone:
on vscode at 8 cores 1.83 times the check's wall time, on a large private monorepo the helper alone 1.34 times. This
records where the time went, what changed, what was measured and rejected, and what is left.

## Where the time went (main, 8 cores)

AWS m8g.4xlarge (16 Graviton4 vCPUs), every process pinned to the first 8 with `taskset`, `TSRS_FAST=1`, release
build of main `221ee7bf` (fat LTO, no PGO). vscode is `src/tsconfig.json` at 1.145.2 with Outline's 7 type-aware rules
over its 11,030 source files, as in [bench/lint-typecheck](../bench/lint-typecheck/RESULTS.md). Phases come from
`TSRS_LINT_TRACE` (below) on the helper alone, replaying the payload oxlint sent, and from timestamps around the helper
inside oxlint (1.93.0).

| vscode, 8 cores | check only | lint only | one run |
| --- | ---: | ---: | ---: |
| through oxlint | 2.93 s | 3.91 s | 5.36 s |
| oxlint before it starts the helper | | 0.25 s | 0.25 s |
| helper: program build | 0.43 s (parse) | 0.47 s | 0.48 s |
| helper: check pass | 2.37 s | | 2.97 s |
| helper: lint pass | | 1.76 s | 0.27 s |
| helper: freeing the program before exit | | 0.26 s | 0.32 s |
| oxlint after the helper exits (10,209 findings) | | 0.89 s | 0.89 s |

- **The two passes did not overlap and did not balance.** `run_program` type-checked every file, collected the
  global diagnostics, then linted every file, each pass on the static checker assignment
  (`for_each_checker_group`) with a barrier in between. The CLI's check steals files between checkers and starts
  each one on its heavy files; the lint path did neither. In vscode's check pass the checkers were busy 86% of the
  pass (the first was done 0.8 s before the last); in the lint-only pass 83%. On the private monorepo the busiest
  checker of the one run's check pass did 1.8 times the mean checker's work, and of its lint-only pass 2.2 times.
- **The helper freed everything before exiting.** Dropping the program, its checkers and the shared declaration
  files took 0.3 s on vscode. On the private monorepo the teardown and what the helper then ran on its own took 7% of
  its wall.
- **Per-checker regions cost a few percent.** Lint programs allocate each checker in a region of its own so that its
  memory goes back with the program; region chunks are not huge pages, the thread arenas' are. Turning them off took
  4-5% off the helper's wall on vscode and the private monorepo, mostly system time (page faults).
- **Oxlint's own work.** 0.25 s before the helper starts, then 0.89 s after it exits, reporting vscode's 10,209
  findings; that is the same in lint only and in the one run. Oxlint 1.93 starts the helper before its own pass and
  runs them side by side. The oxlint the private monorepo's lint script runs does its own non-type-aware rules first
  and starts the helper only after them, and that takes more than twice the helper's wall time there.
- **What the rules cost is small.** In a perf profile of the one run the type check is 78% of the helper's CPU on
  vscode and 73% on the private monorepo, the rule visits 7% on both (on vscode no-floating-promises the largest), creating
  rules and walking the tree too little to show.

Why lint only was slower than check only on vscode although it uses less CPU (18 s against 22 s): oxlint's fixed 1.1 s
around the helper, the unbalanced lint pass, and the teardown. The rules resolve types lazily, so lint only does about
half the check's type work; the one run does the whole check plus the rules on cached types, so it can approach the
check's wall time plus oxlint's own, but not lint only's.

The lint path already had `--fast`'s checker count, locality assignment, lazy members and canonical history. It
lacked stealing and heavy files first (above), lazy declaration-file members and leaf-file freeing (both memory
features of the CLI's `--noEmit` check; lazy members made no wall difference on the CLI here, and leaf freeing frees
trees the rules still need).

## What changed

- **One pass per program** (`Program::for_each_checker_check_then_visit`, checkerpool.rs). Each checker checks its
  files, takes its global diagnostics, then lints the files it checked while they are hot. A checker never runs a
  rule before one of its own checks, so rule type queries still cannot change what `tsrs --noEmit` would report (the
  reason the passes were split, #955): a query could report an error without a location, or cache a type whose error
  a later check would then miss. But no checker waits for the others: one that is done checking lints while the
  rest still check. Under `--fast` the checks steal as the CLI's type-check pass does (heavy files first, a limit of
  thieves per queue from 8 checkers, sticky thieves from 16). Rule visits stay on the checker that checked the file;
  a pass without checks (lint only) steals its visits instead, with the same queues. Default mode keeps the static
  assignment and file order, so its history and output are unchanged.
- **Stealing only while a program runs alone.** When tsconfig programs run concurrently (`program_threads`), a
  checker that runs out of work leaves its core to the other program instead of redoing another checker's
  neighbourhood.
- **The program expected to finish last is left to the process exit** (`Options::exit_after`, set only by
  `cli::main`): no free before exit, and its checkers allocate in the thread arenas instead of regions. Any other
  program is freed on a thread of its own while the request goes on. "Expected to finish last": no job starts after
  it and no other tsconfig program runs when it starts.
- **The inferred program is built first** (concurrently with the first tsconfig program when programs run two at a
  time), so that a tsconfig program is the one left to the exit; its output still comes last.
- `TSRS_LINT_TRACE=<file>` records the request's phases and a span per checked and per linted file;
  `bench/lint-typecheck/trace.mjs` summarizes it, `bench/lint-typecheck/frames.mjs` compares two helper outputs.

Nothing departs from tsgolint's protocol or from main's output (below).

## Output

The helper's frames were compared with main's byte for byte (`frames.mjs`) on oxlint's payloads with the 23
recommended-type-checked rules over every tracked TypeScript file of vscode, t3code-server, mikro-orm, webpack,
xstate-main, cal-diy, supabase-studio and mui-docs (many tsconfigs and loose files among them), lint only and one run,
with and without `TSRS_FAST`, and on the private monorepo's own payloads: the same frame sequence in every case.
`parallel_rules_and_type_checking_match_serial_for_independent_modules` now also runs the fast preset at 2, 4 and 8
checkers.

## Results

AWS m8g.4xlarge, one box per project set, both arms copied fresh before the runs (see the method note below), main
`221ee7bf` against this branch, `TSRS_FAST=1`. Wall time medians; changes are medians of per-pair ratios with ~95%
intervals, 12 pairs at 8 cores and 8 at 4 (private monorepo: 8 and 5). Oxlint 1.93.0 with Outline's 7 rules for vscode
and Outline, as in RESULTS.md; the helper rows replay oxlint's payload (23 recommended rules, every tracked
TypeScript file) without oxlint.

| 8 cores | one run | lint only | check only |
| --- | ---: | ---: | ---: |
| vscode | 5.45 -> 4.83 s, −11.4% [−11.8, −11.1] | 3.99 -> 3.53 s, −11.4% [−12.1, −10.8] | 2.98 -> 2.99 s, +0.3% [−0.3, +0.9] |
| Outline | 2.67 -> 1.87 s, −30.4% [−30.8, −29.5] | 1.82 -> 1.12 s, −38.3% [−39.0, −37.7] | 1.73 -> 1.74 s, +0.5% [−1.1, +1.8] |
| vscode, helper | 4.14 -> 3.50 s, −15.4% [−15.7, −14.9] | | |
| t3code-server, helper | 9.11 -> 8.26 s, −9.3% [−9.7, −8.6] | 10.17 -> 9.31 s, −8.4% [−10.0, −7.0] | |
| mikro-orm, helper | 3.32 -> 2.92 s, −11.9% [−12.6, −10.3] | 3.45 -> 3.40 s, −1.4% [−2.4, −0.9] | |
| private monorepo, its oxlint | −5.5% [−5.6, −4.6] | −5.6% [−5.9, −5.3] | +0.1% [−0.1, +0.4] |
| private monorepo, helper | −17.7% [−18.9, −17.1] | −19.7% [−20.2, −19.3] | |

| 4 cores | one run | lint only | check only |
| --- | ---: | ---: | ---: |
| vscode | 9.37 -> 7.90 s, −15.8% [−16.9, −15.0] | 6.27 -> 5.39 s, −13.9% [−14.2, −13.6] | 5.57 -> 5.58 s, +0.4% [+0.1, +0.7] |
| Outline | 4.31 -> 3.08 s, −28.6% [−28.8, −28.1] | 2.87 -> 1.58 s, −44.8% [−45.7, −44.2] | 2.27 -> 2.26 s, −0.6% [−1.0, +0.9] |
| vscode, helper | 7.77 -> 6.33 s, −18.4% [−18.8, −18.2] | | |
| t3code-server, helper | 15.02 -> 13.61 s, −9.2% [−10.0, −8.7] | 13.95 -> 12.34 s, −11.4% [−12.5, −11.1] | |
| mikro-orm, helper | 6.53 -> 5.28 s, −19.1% [−19.8, −18.7] | 5.72 -> 4.68 s, −18.4% [−19.0, −18.2] | |
| private monorepo, its oxlint | −2.9% [−3.6, −2.3] | −3.5% [−3.8, −2.6] | +0.1% [+0.0, +0.4] |
| private monorepo, helper | −11.9% [−12.3, −11.7] | −13.3% [−13.4, −12.4] | |

CPU time, instructions and peak memory (largest process), branch against main, 8 / 4 cores:

| | CPU | instructions | peak memory |
| --- | ---: | ---: | ---: |
| vscode one run | −2.3% / −3.1% | −0.1% / −0.5% | −2.4% / −2.9% |
| vscode lint only | −2.4% / −3.1% | −0.2% / −0.4% | −1.3% / −1.4% |
| Outline one run | −1.4% / +1.7% | +6.0% / +6.4% | +4.4% / +3.0% |
| Outline lint only | +3.8% / +3.1% | +14.0% / +12.7% | +8.6% / +3.9% |
| t3code-server one run, helper | +1.9% / +4.4% | −0.7% / +4.2% | +3.7% / +6.3% |
| t3code-server lint only, helper | +1.2% / +3.5% | −0.2% / +2.6% | +0.5% (noise) / +4.7% |
| mikro-orm one run, helper | +1.1% / +2.9% | +1.5% / +3.4% | +2.3% / +0.1% (noise) |
| mikro-orm lint only, helper | +0.2% / +3.1% | +0.0% / +2.9% | +1.3% / +0.6% (noise) |
| private monorepo one run, helper | −1.6% / −3.5% | +1.6% / +0.3% | +0.7% / −1.3% |
| private monorepo lint only, helper | −1.7% / −3.4% | +1.8% / +0.4% | −0.2% / −1.8% |
| private monorepo, through its oxlint | −0.4% / −0.5% (noise) | +0.6% / +0.1% | 0.0% (noise) |

The check-only rows run the same code in both arms and stay within noise, as they should.

How the one run compares now, branch at 8 cores: on vscode it takes 1.62 times the check alone (main 1.83) and 1.37
times lint alone (main 1.37: both got faster); on Outline 1.07 times the check alone (main 1.54). The helper alone on
vscode, 3.23 s against the CLI check's 2.85 s, is the check's cost plus 0.25 s of rule visits: inside oxlint the rest of
the one run is oxlint's 0.25 s before the helper, 0.89 s after it, and about 0.4 s of the helper sharing cores with
oxlint's own pass. On the private monorepo the helper's one run now takes 1.10 times the check alone at 8
cores and at 4 (main 1.34 and 1.26); through its oxlint, the one run takes 1.05 times lint alone in both arms, since
lint alone is mostly oxlint's own rules there.

**A measurement trap.** The first full A/B ran each arm's binary where cargo had linked it. The branch's ran the
unchanged CLI check 3.1% slower with the same instructions and 16 times the iTLB misses; a `cp` of the same binary ran
it as fast as main (and a `cp` of main slightly faster than main's own). How the linker wrote the file decides how
the page cache, and so the instruction-side TLB, maps its text. Copy both arms to fresh files before an A/B.

## Measured and rejected

- **Rules fused into each file's check** (check a file, lint it, next file): what this code did before #955. A
  rule's type query can then cache a type or report an error that a later file's check needed (#941); keeping every
  check of a checker ahead of its rules keeps the guarantee and nearly all of the benefit.
- **Stealing rule visits in the one run.** On outline the one run's wall was the same with and without it
  (1.73 s), for 8% more instructions and 5% more peak memory; on vscode the same wall; on t3code-server it cost wall.
  A thief that lints a file it did not check redoes most of that check. Kept only for lint only, where it is the only
  balancing (outline lint only 1.70 -> 1.24 s, vscode 2.47 -> 2.20 s, for 17% more instructions on outline).
- **Stealing while programs run concurrently.** t3code-server (about 40 tsconfigs, two programs at a time on 8 cores):
  8.32 s without stealing, 9.39 s with, and 18% more instructions.
- **Leaving the last-started program to the exit** regardless of what else runs: on mikro-orm the root tsconfig
  program was the last to start but finished long before `tests/tsconfig.json`, and keeping its memory raised the peak
  14%.
- **Freeing programs synchronously** instead of on a thread of their own: within 1% on memory, slower.
- **Not sharing declaration files between programs** (parsing them into each program's own regions, which are huge
  pages): no difference.
- **Streaming frames to oxlint as files finish.** Oxlint's unix reporter prints in arrival order (the default reporter
  sorts), so it would reorder output, and in the one run the findings come from the rule visits at the end of the
  pass anyway.
- **A direct mode without oxlint.** The protocol is not the cost: writing the frames takes 4 ms; oxlint's time is its
  own pass and its reporting.

## What is left

- **Oxlint's own time around the helper**, the same in lint only and the one run: 0.25 s before it starts the helper
  and 0.9 s after it exits on vscode, most of it reporting the 10,209 findings one at a time
  ([bottleneck investigation](perf-tsgolint-bottlenecks.md): line and column lookup per diagnostic). Without it the
  vscode one run would be within about 0.6 s of the check alone. That fix belongs in oxlint.
- **Sharing the cores with oxlint.** Oxlint 1.93 runs its own pass while the helper builds its program; now that the
  helper's pass keeps every core busy, the helper takes about 0.3 s longer inside oxlint than alone on vscode.
- **The private monorepo's oxlint** runs its own rules before it starts the helper, so its lint and one run gain
  much less than its helper does. An oxlint that starts the helper first (1.93 does) would overlap the two.
- **The floor of the one run** is the check plus the rules on cached types: on vscode the helper's pass is the
  check's time plus 0.24 s of rule visits, and its program build matches the CLI's parse.
- **Memory.** Where checks are stolen, a thief builds the types around the files it takes again: outline's one run
  peaks 3% higher, lint only's visit stealing 4-9% (`TSRS_STEAL_VISITS=0` turns that off). Lazy declaration-file
  members and leaf-file freeing, the CLI's memory features, are not used by the lint path; leaf freeing would need
  to free a file after its rules ran, not after its check.
- **Programs that share files** (repositories with many tsconfigs) still build the shared files' types once per
  program; only their type errors are reported once.

## Reproduce

```sh
bench/lint-typecheck/setup.sh                    # tools and projects, as for RESULTS.md
cargo build --release --locked -p tsrs_cli        # once per arm
# Capture oxlint's payload with a launcher that tees stdin, then replay the helper alone:
TSRS_FAST=1 TSRS_LINT_TRACE=/tmp/trace.tsv taskset -c 0-7 target/release/tsrs --lint headless < payload.json > out.bin
node bench/lint-typecheck/trace.mjs /tmp/trace.tsv 8
node bench/lint-typecheck/frames.mjs main-out.bin out.bin
```

Timings are paired A/B runs (main, branch) on one box with the aws-bench-box runner: each workload alternates the arms
for 12 pairs at 8 cores and 8 at 4 (8 and 5 on the private monorepo), order flipped every pair; changes are medians of
per-pair ratios with ~95% intervals. CPU time is user plus system of the whole process tree, instructions are
`perf stat` over the tree, peak memory is the largest process's.
