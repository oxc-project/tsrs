# perf-incremental-pass-stealing: work stealing in the incremental check pass under `--fast`

Every benchmark in this repository passes `--incremental false`. A project whose tsconfig sets `"incremental": true`
goes through the incremental pass on every run, the first one included (a CI job on a fresh checkout without its build
info). Under `--fast` the full type-check pass moves files between checkers (notes/perf-checker-stealing.md); the
incremental pass did not, so the checkers that finished early sat idle while the slowest one finished. This note lists
why the pass was static, makes it steal under `--fast` with output and build info identical to the static pass, and
measures it.

## The gap

On a large private monorepo, main at 221ee7bf, an m8g.4xlarge pinned to 8 cores, `--noEmit --fast` with the
tsconfig's `incremental: true` and no build info on disk took 17.5% more wall time than `--incremental false` (check
phase +15%), with 3% less CPU time and 4.5% more peak RSS. A sampling profile bucketed by time showed 8 busy threads for
the first two thirds of the check, then 7, then 4, 3, 2 and 1: the static assignment's checkers finished at different
times and nothing rebalanced.

The public projects show the same gap. Same box (below), main against main: `--incremental false` as A, a cold
incremental run as B, 10 alternating pairs, median of the per-pair changes with a ~95% interval:

| project | cores | wall A → B (s) | check A → B (s) | wall | CPU | check | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| vscode | 8 | 2.05 → 2.39 | 1.60 → 1.81 | +15.9% [+15.6, +16.5] | −0.3% [−0.8, +0.8] | +12.4% [+11.7, +13.2] | +2.5% [+2.4, +2.5] | +17.3% [+16.9, +17.8] |
| | 4 | 3.67 → 4.08 | 2.97 → 3.23 | +11.4% [+9.8, +12.8] | +0.7% [+0.0, +2.0] | +8.3% [+6.8, +10.3] | +2.8% [+2.8, +2.8] | +19.0% [+18.7, +19.5] |
| mikro-orm | 8 | 1.18 → 1.59 | 1.00 → 1.19 | +34.3% [+31.3, +35.4] | −1.6% [−2.3, −0.4] | +19.0% [+16.3, +20.7] | −1.5% [−1.9, −1.4] | −1.9% [−2.6, −1.5] |
| | 4 | 2.00 → 2.36 | 1.77 → 1.95 | +17.8% [+16.9, +18.8] | +0.1% [−1.2, +0.9] | +10.2% [+9.0, +11.4] | +0.4% [+0.3, +0.4] | −0.2% [−1.1, +0.3] |
| mui-docs | 8 | 1.46 → 1.71 | 1.15 → 1.31 | +17.3% [+13.6, +19.4] | −12.4% [−17.0, −11.5] | +13.5% [+8.8, +15.9] | −14.6% [−19.3, −14.3] | −6.2% [−8.5, −4.9] |
| | 4 | 1.70 → 2.11 | 1.37 → 1.65 | +23.9% [+22.6, +24.9] | −8.2% [−8.8, −7.1] | +20.2% [+19.9, +21.4] | −10.3% [−10.3, −10.2] | −4.5% [−4.7, −3.6] |
| supabase-studio | 8 | 1.30 → 1.54 | 0.97 → 1.03 | +17.8% [+17.4, +18.2] | +2.9% [+2.3, +3.5] | +6.0% [+5.5, +6.4] | +3.6% [+3.5, +3.6] | +4.4% [+4.1, +4.9] |
| | 4 | 2.16 → 2.47 | 1.68 → 1.79 | +15.1% [+14.2, +16.2] | +4.2% [+3.8, +4.6] | +6.9% [+5.3, +7.3] | +4.9% [+4.9, +5.0] | +5.8% [+5.6, +6.1] |

Not all of it is scheduling. The cold incremental run also writes the build info (vscode's is 12.9 MB) and does
not free leaf files, which the `--noEmit` full pass does (most of vscode's +17-19% peak RSS, below). The check row
is the part this note is about.

## Why the incremental pass was static

1. **Global diagnostics are stored per file.** `get_semantic_diagnostics_for_incremental` collects each file with
   `include_deferred_globals`: a global diagnostic (one without a file, such as TS2318 "Cannot find global type") that
   the file's checker reports while checking the file is stored with that file's diagnostics. A checker reports a
   global once, at the first file that needs it, so which file stores it depends on which files share the checker and
   in what order they are checked. This is the reason notes/perf-checker-stealing.md gives.
2. **Those stored globals are all the incremental CLI prints.** `get_diagnostics_of_any_program_like` sweeps the
   checkers' globals after the semantic pass only for a plain program; for an incremental program the printed globals
   are the union of the ones stored with files. So the attribution decides the output of later warm runs, not only the
   build info bytes.
3. **The checkers have a history before the pass.** In a warm run after an edit, `collect_all_affected_files`
   computes declaration signatures on the files' static checkers before the check. A signature can report a global
   (an async generator's inferred return type looks up `AsyncIterableIterator` with error reporting); that checker
   then never attributes it to a file, and main prints nothing for it. Moving files to or from that checker changes
   which file stores it.
4. **Owners move.** Stealing rewrites `PoolState::owners`, so later passes over a stolen file (signature emit,
   declaration diagnostics, emit, the watch session's placeholder signatures) run on the checker that checked it. Exact
   only under the canonical-history claim the full pass already relies on: a file's diagnostics and declaration
   output do not depend on which checker produces them (notes/perf-order-independence.md, with the known exceptions
   of notes/open-history-dependence.md).
5. **The type-check pass's memory features assume nothing reads the checkers afterwards.** `allow_steal` also turned
   on checker retirement under `--maxMemory`, the shared-graph seed and forks, `TSRS_MEM_SPLIT` and arena tail
   trimming, all written for the `--noEmit` full pass. The incremental pass is followed by signature computation,
   emit and the build info write.
6. **Split files report globals outside the callback.** The pieces of a split declaration file run on other
   checkers before the owner's callback; a global one of them reports is attributed to no file.
7. **Leaf freeing frees trees a later pass may need.** It is never enabled for an incremental compilation
   (`perform_incremental_compilation` does not turn on file regions), and signatures, emit and later watch cycles read
   the trees. Left off.
8. **Go history.** In the default mode the checkers keep Go's history and assignment; stealing is off there
   (`stealing_enabled`), in every pass.

## The change

`for_each_checker_group_do_ex` takes a `GroupPass` instead of `allow_steal`: `Static`, `TypeCheck` (stealing, heavy
files first, file splitting and the memory features of reason 5) and `IncrementalCheck` (stealing, heavy files first
and file splitting only), and a `stop` flag that ends the pass before each checker's next file.
`Program::get_semantic_diagnostics_for_incremental` uses the stolen pass when stealing is on (`--fast`, no named
assignment, more than one checker, more than one file to check, `TSRS_INCREMENTAL_STEAL` not `0` or `off`):

1. Read every checker's global diagnostics. If they are not the same on every checker (reason 3), check statically,
   as main does.
2. Run the stolen pass. A file whose check grows its checker's global diagnostics sets `stop`.
3. If no checker has a global diagnostic it did not have before (counts compared after the pass, which also covers
   split pieces), keep the result.
4. Otherwise (`Checkers: incremental pass rechecked statically` in `--extendedDiagnostics`): replace every checker by
   a fresh one (`restart_static`, through `retire_checker`, which keeps the old checkers' globals and counters), give
   every file back its pre-pass owner, check the files again without stealing, and drop from each file's result the
   globals every checker had before the pass.

`TSRS_FILE_TIMES` now also records the incremental pass.

## Why it is exact

The claim is that every file's stored result is what main's static pass stores, so the build info bytes and the
printed output are main's. It rests on the canonical-history claim the full pass's stealing already makes (a file's
own diagnostics do not depend on the checker that checks it or what it checked before) and on how globals are
reported:

- A global diagnostic comes from a global lookup that reports (`get_global_type(.., report_errors = true)` and the
  like). Each lookup caches its result per checker, so it reports once per checker, at the first time a checker needs
  it. A lookup that does not report has its own cache (`get_global_async_iterable_iterator_type` and
  `get_global_async_iterable_iterator_type_checked` are separate), so a silent miss never suppresses a later report:
  every suppressed report left a global in the checker's list first.
- **No new globals (step 3).** All checkers started with the same globals G0. If the stolen pass reported nothing
  new, no checked file needs a reporting lookup that is missing outside G0: if one did, the checker that checked it
  (or the one that resolved what it needs before it) would have reported it. So main's static pass, starting from G0
  on every checker, reports nothing new either, attributes nothing, and stores each file's own diagnostics, which
  are the same on any checker.
- **The fallback (step 4).** A fresh checker visits the static owner's files in the same order as main's checker.
  For a global outside G0, the first file in that order that needs it reports it on both (main's checker had not
  needed it before the pass, or it would be in G0). A global in G0 is never attributed by main; the fallback drops it.
  So each file's stored globals are main's.
- **Warm runs where a signature reported a global (step 1).** The checkers' globals differ, so the pass is main's.
- **Everything else in the build info** is computed before the pass on the static owners (signatures of changed
  files), from the program (`tsrsCanonicalCheckSignature` hashes tsgo's virtual four-checker slots, not the real
  ones) or from the file text (`tsrsInterfaceFiles`).
- **Default mode** never reaches this code: `stealing_enabled` is false under Go history, so
  `incremental_check_steals` is false and the static code path is unchanged.

Not covered, as for the full pass: the open history-dependent cases (notes/open-history-dependence.md) and
diagnostics a checker files against another file than the one it checks.

### Main's own warm-run behaviour with global diagnostics (unchanged)

Main does not always print in a warm run what a cold run prints when a global diagnostic is involved, and the change
keeps that: it prints exactly main's bytes. With `lib: ["es2015"]` and an `async function*`:

- adding the generator to a file in a warm run: the file's new declaration signature reports TS2318 before the check,
  so no file stores it and the warm run prints nothing (cold prints it);
- two files with a generator, then removing it from the one that stored TS2318: the other file was checked after it
  on the same checker and stores nothing, so the warm run drops TS2318 (cold prints it).

Making those sound would take a per-file record of every missing global a file needs, which the per-checker lookup
caches do not keep; not attempted here.

## Results

m8g.4xlarge (Graviton4), one box, both arms alternating, `taskset -c 0-7` or `-c 0-3`, `--noEmit --pretty false
--fast --extendedDiagnostics`, build info in `/tmp` (`--incremental true --tsBuildInfoFile`), deleted before each cold
run. Median of per-pair ratios, ~95% interval in brackets; check time from the `Check time` row.

Two binaries are compared only as fresh copies: each arm's binary is `cp`'d to a new file and `sync`ed, and the
copies run. A binary run from the path cargo linked it to can take 0.5-2.4% more cycles (2.5-50 times the
instruction-TLB misses of a copy, by a different amount for each build: how the linker wrote the file decides how the
page cache maps its text), which is the size of the effects measured here. A same-binary comparison with an
environment switch is not affected. main = 221ee7bf. The main-against-branch tables use a copy of this branch's final
code; the same-binary tables (the switch, both gap tables) used its first revision, which differs only by moved code,
the `TSRS_FILE_TIMES` hook, the test and the docs, run from its link path, so their absolute seconds can be a few
percent high (their ratios are not affected).

**Cold incremental run** (no build info), the same binary with `TSRS_INCREMENTAL_STEAL=0` (A) and without (B), 10
pairs:

| project | cores | wall A → B (s) | check A → B (s) | wall | CPU | check | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| private monorepo | 8 | | | −10.7% [−11.0, −10.2] | +3.5% [+3.2, +3.6] | −12.4% [−12.9, −11.9] | +2.5% [+2.4, +2.8] | +4.1% [+3.6, +4.5] |
| | 4 | | | −6.3% [−6.8, −4.6] | +0.3% [−0.2, +2.0] | −7.3% [−7.5, −5.3] | +0.8% [+0.8, +0.9] | +1.4% [+1.2, +1.8] |
| vscode | 8 | 2.38 → 2.18 | 1.82 → 1.61 | −8.6% [−9.3, −8.0] | +1.5% [+0.7, +2.2] | −11.1% [−12.2, −10.4] | +0.2% [+0.2, +0.3] | +0.5% [+0.3, +0.9] |
| | 4 | 3.91 → 3.66 | 3.08 → 2.83 | −6.2% [−7.1, −4.8] | +0.6% [−0.1, +1.0] | −8.1% [−8.7, −7.4] | −0.0% [−0.0, +0.0] | +0.3% [−0.2, +0.6] |
| mikro-orm | 8 | 1.60 → 1.41 | 1.20 → 1.02 | −11.9% [−12.8, −10.3] | +1.9% [+0.9, +2.8] | −15.3% [−16.4, −14.0] | +2.2% [+2.1, +2.5] | +2.5% [+2.2, +3.2] |
| | 4 | 2.29 → 2.11 | 1.88 → 1.70 | −7.9% [−8.5, −7.3] | +0.0% [−0.4, +0.7] | −9.6% [−10.2, −8.9] | +0.2% [+0.2, +0.2] | +0.9% [+0.2, +1.4] |
| mui-docs | 8 | 1.75 → 1.61 | 1.33 → 1.20 | −6.9% [−10.1, −5.2] | +17.3% [+16.3, +23.1] | −9.0% [−12.7, −6.8] | +19.2% [+18.9, +25.9] | +9.1% [+8.4, +11.1] |
| | 4 | 2.13 → 1.87 | 1.68 → 1.41 | −12.6% [−13.2, −11.8] | +12.8% [+12.3, +13.5] | −16.0% [−16.9, −15.1] | +14.4% [+14.3, +14.4] | +7.3% [+6.4, +7.8] |
| supabase-studio | 8 | 1.53 → 1.49 | 1.04 → 0.99 | −3.1% [−3.8, −2.6] | +2.1% [+1.6, +2.5] | −4.8% [−5.3, −4.1] | +1.5% [+1.3, +1.6] | +1.0% [+0.8, +1.3] |
| | 4 | 2.40 → 2.29 | 1.74 → 1.63 | −4.6% [−5.4, −4.2] | +0.7% [+0.4, +1.6] | −6.5% [−7.6, −5.6] | +0.9% [+0.8, +0.9] | +0.4% [+0.2, +0.8] |

**Cold incremental run, main (A) against this branch (B)**, fresh copies, 15 pairs:

| project | cores | wall A → B (s) | wall | CPU | check | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| private monorepo | 8 | | −11.7% [−12.3, −11.3] | +3.4% [+3.1, +4.3] | −13.4% [−14.3, −13.1] | +2.8% [+2.7, +3.0] | +4.1% [+4.0, +4.3] |
| | 4 | | −5.2% [−6.4, −4.6] | +1.6% [−0.0, +1.8] | −5.9% [−7.4, −5.5] | +0.8% [+0.8, +0.9] | +1.4% [+1.3, +1.7] |
| vscode | 8 | 2.19 → 2.01 | −8.3% [−8.6, −7.8] | +1.3% [+1.2, +1.5] | −10.8% [−11.2, −10.6] | +0.2% [+0.2, +0.2] | +0.5% [+0.3, +0.7] |
| | 4 | 3.75 → 3.52 | −6.1% [−6.5, −5.9] | +0.8% [+0.6, +0.9] | −7.7% [−8.0, −7.5] | −0.0% [−0.0, +0.0] | +0.5% [+0.2, +0.7] |
| mikro-orm | 8 | 1.51 → 1.33 | −11.2% [−11.8, −10.4] | +2.8% [+2.5, +3.0] | −14.8% [−15.3, −13.3] | +2.2% [+2.2, +2.5] | +2.6% [+2.1, +3.4] |
| | 4 | 2.22 → 2.07 | −6.9% [−7.5, −6.5] | +0.7% [+0.6, +0.9] | −8.6% [−9.1, −8.2] | +0.2% [+0.2, +0.2] | +1.5% [+0.2, +2.0] |
| mui-docs | 8 | 1.67 → 1.57 | −6.7% [−9.7, −5.1] | +22.5% [+16.6, +23.3] | −9.3% [−12.9, −7.2] | +25.9% [+18.8, +26.0] | +11.1% [+7.3, +11.7] |
| | 4 | 2.05 → 1.79 | −12.6% [−13.2, −12.2] | +12.9% [+12.7, +13.1] | −16.1% [−16.6, −16.0] | +14.4% [+14.3, +14.4] | +7.0% [+6.4, +7.7] |
| supabase-studio | 8 | 1.44 → 1.40 | −2.6% [−2.8, −2.3] | +2.3% [+2.1, +2.5] | −3.6% [−4.2, −3.4] | +1.6% [+1.6, +1.6] | +1.0% [+0.7, +1.2] |
| | 4 | 2.26 → 2.16 | −4.4% [−4.7, −4.1] | +1.4% [+1.1, +1.6] | −6.3% [−7.2, −6.0] | +1.0% [+0.9, +1.1] | +0.5% [+0.1, +0.9] |

**Other runs, main (A) against this branch (B)**, fresh copies, 8 cores unless noted, 10 pairs (8 for the private
4-core rows). `--incremental false` is the full pass, which this change does not alter; warm no change reads the build
info and checks nothing; warm hub edit appends an export to the most imported file before each run, starting from a
cold build info of the unedited program.

| project | run | wall | CPU | check | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- | --- |
| private monorepo | `--incremental false` | +0.7% [−1.7, +2.3] | +0.5% [−1.0, +2.8] | +0.6% [−1.8, +2.6] | +0.0% [−0.7, +0.5] | −0.1% [−0.2, +0.4] |
| | `--incremental false`, 4 cores | +0.5% [−0.3, +0.6] | +0.4% [−0.2, +0.7] | +0.4% [−0.4, +0.6] | −0.0% [−0.0, −0.0] | −0.1% [−0.3, −0.1] |
| | warm no change | −0.7% [−1.1, +0.6] | −0.3% [−0.8, +0.3] | | −0.0% [−0.0, +0.0] | −0.1% [−0.4, +0.2] |
| | warm hub edit | −10.8% [−11.8, −9.7] | +2.4% [+1.3, +2.9] | −21.6% [−22.6, −20.7] | +2.4% [+2.1, +2.4] | +3.6% [+3.3, +4.0] |
| | warm hub edit, 4 cores | −4.6% [−4.7, −4.3] | +1.1% [+0.7, +1.3] | −9.3% [−9.7, −9.0] | +0.6% [+0.6, +0.6] | +1.1% [+1.0, +1.2] |
| vscode | `--incremental false` | −0.1% [−0.8, +0.4] | +0.0% [−0.1, +0.3] | +0.1% [−0.3, +0.7] | +0.0% [−0.0, +0.1] | −0.0% [−0.3, +0.5] |
| | warm no change | +0.1% [−0.4, +0.7] | +0.0% [−0.1, +0.8] | | +0.0% [−0.0, +0.0] | +0.1% [−0.2, +0.3] |
| | warm hub edit (2.83 → 2.63 s) | −7.1% [−8.2, −6.0] | +0.8% [−0.1, +1.6] | −11.2% [−11.6, −11.0] | +0.3% [+0.2, +0.3] | +0.8% [+0.4, +0.9] |
| mikro-orm | `--incremental false` | −0.3% [−1.3, +0.4] | +0.1% [−0.5, +0.4] | −0.5% [−1.9, +0.7] | −0.1% [−0.4, +0.1] | −0.1% [−0.9, +0.5] |
| | warm no change | +2.8% [−1.2, +4.4] | +0.0% [−0.5, +0.8] | | +0.0% [−0.1, +0.1] | −0.5% [−1.4, +1.5] |
| | warm hub edit (1.32 → 1.04 s) | −20.9% [−21.7, −20.2] | +4.4% [+4.1, +5.2] | −28.0% [−28.6, −27.9] | +3.8% [+3.8, +3.9] | +4.8% [+4.5, +5.1] |
| mui-docs | `--incremental false` | +1.5% [−4.7, +5.1] | +0.6% [−0.2, +6.0] | +0.9% [−5.7, +6.0] | +0.1% [−0.1, +6.6] | +0.6% [−0.4, +2.6] |
| | warm no change | −0.0% [−4.9, +3.9] | +0.4% [−0.9, +1.6] | | +0.0% [−0.0, +0.0] | +0.1% [−1.2, +1.6] |
| | warm hub edit (1.77 → 1.61 s) | −8.7% [−10.7, −6.1] | +16.9% [+16.4, +17.4] | −11.5% [−12.5, −7.3] | +18.5% [+18.3, +18.7] | +8.8% [+7.5, +10.1] |
| supabase-studio | `--incremental false` | +0.2% [−0.7, +0.8] | +0.3% [−0.2, +0.4] | +0.3% [−0.2, +0.7] | +0.0% [−0.2, +0.1] | −0.0% [−0.4, +0.2] |
| | warm no change | +0.1% [−1.3, +1.5] | +0.1% [−0.4, +0.8] | | −0.0% [−0.0, +0.0] | −0.2% [−1.6, +0.8] |
| | warm hub edit (1.59 → 1.58 s) | −1.1% [−1.7, −0.7] | +1.7% [+1.4, +1.9] | −1.9% [−2.5, −1.4] | +0.5% [+0.4, +1.1] | +0.5% [−0.2, +1.7] |

Reading them:

- The check phase of a cold incremental run is 3.6-16% shorter on every project at both core counts, the wall time
  2.6-13%, in the same-binary and in the main-against-branch comparison alike. The private monorepo gains more at 8
  cores than at 4, as expected from a tail that needs several idle checkers to show.
- The full pass (`--incremental false`) is the same on both binaries within the noise: instructions identical, wall
  and CPU intervals containing 0. A first run of these comparisons from the binaries' link paths under
  `target/release` had shown +0.1-3.4% cycles on that path (vscode +3.4%) with identical instructions, and main
  against the branch 1.5-3 points more CPU than the switch, which was first read as code layout. It was the link-path
  effect above: with fresh copies it is gone, and main against the branch agrees with the switch.
- The cost is the full pass's: a stolen file is checked on a checker that has not resolved its neighbourhood, so
  instructions and peak RSS grow with the share that moves. Small on most projects (0-3% instructions, 0-4% RSS at 8
  cores). mui-docs is the exception, +14-26% instructions and +7-11% RSS, with the wall time still down 7-13%: about
  30% of its check CPU moves (above). Its full pass pays the same: single runs of `--incremental false` at 8 cores
  did 19.5% more instructions with stealing than with `--checkerAssignment locality`, and on main the static
  incremental pass did 10-15% fewer instructions than the full pass with stealing (the gap table above). It did not
  take the fallback.
- Warm runs with no change check nothing and do not move. After a hub edit the incremental pass re-checks most of
  the program and gets the same kind of gain, larger in the check phase (−9% to −28%) because the signature step
  before it is unchanged. supabase-studio's hub edit re-checks little.
- Exit statuses and output were the same in every sample (the runner records the status; outputs are covered by the
  exactness runs).

**What is left of the gap.** The branch against itself, `--incremental false` as A and a cold incremental run as
B, 10 pairs (the same comparison as the gap table, which was main against itself):

| project | cores | wall A → B (s) | check A → B (s) | wall | CPU | check | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| private monorepo | 8 | | | +4.8% [+3.4, +5.4] | +1.1% [−0.1, +2.3] | +0.5% [−0.9, +1.2] | +1.4% [+0.8, +1.7] | +9.0% [+8.6, +9.0] |
| | 4 | | | +4.5% [+3.0, +5.7] | +2.3% [+0.4, +3.6] | +1.4% [−0.6, +3.0] | +1.4% [+1.4, +1.4] | +10.8% [+10.6, +11.1] |
| vscode | 8 | 2.03 → 2.16 | 1.59 → 1.59 | +6.0% [+5.3, +7.1] | +1.1% [+0.9, +2.4] | −0.1% [−0.6, +1.4] | +2.7% [+2.7, +2.8] | +18.2% [+17.9, +18.7] |
| | 4 | 3.75 → 3.88 | 3.04 → 3.02 | +3.7% [+3.5, +4.0] | +1.2% [+0.8, +1.4] | −0.7% [−1.1, −0.3] | +2.8% [+2.8, +2.8] | +19.6% [+19.3, +20.3] |
| mikro-orm | 8 | 1.21 → 1.43 | 1.02 → 1.03 | +17.5% [+16.9, +18.9] | +0.9% [+0.3, +1.4] | +0.3% [−0.8, +2.0] | +0.5% [+0.4, +0.9] | +0.9% [+0.4, +1.8] |
| | 4 | 2.03 → 2.21 | 1.79 → 1.79 | +9.0% [+7.8, +9.7] | +0.9% [−0.2, +1.3] | +0.2% [−0.6, +0.8] | +0.6% [+0.6, +0.6] | +1.0% [+0.5, +1.4] |
| mui-docs | 8 | 1.47 → 1.64 | 1.17 → 1.22 | +12.8% [+1.6, +14.4] | +3.0% [−6.9, +8.4] | +3.7% [−7.0, +6.9] | +2.0% [−9.6, +8.1] | +3.1% [−2.4, +5.1] |
| | 4 | 1.74 → 1.88 | 1.40 → 1.41 | +8.4% [+7.0, +9.4] | +3.4% [+2.7, +4.4] | +0.8% [−0.3, +1.2] | +2.6% [+2.6, +2.7] | +2.3% [+1.3, +3.0] |
| supabase-studio | 8 | 1.32 → 1.50 | 0.99 → 0.99 | +13.5% [+12.3, +14.5] | +4.8% [+4.4, +5.2] | +0.9% [−0.4, +1.2] | +5.2% [+5.1, +5.3] | +5.2% [+4.6, +5.5] |
| | 4 | 2.09 → 2.31 | 1.64 → 1.64 | +9.8% [+9.1, +10.9] | +4.8% [+4.5, +5.8] | −0.2% [−0.9, +0.6] | +5.9% [+5.8, +6.1] | +6.2% [+6.0, +6.7] |

The check phase of a cold incremental run now takes what the full pass takes, within the noise, on every project and
at both core counts (it was 6-20% longer on main). The rest of the wall-time gap is outside the check phase (the
incremental program's bookkeeping and the build info write; not broken down here), and the peak RSS gap is mostly
leaf freeing, which only the full pass does: vscode's `--incremental false` at 8 cores peaks at 1842-1852 MiB, 2121-2122
MiB with `TSRS_FREE_LEAVES=0`, and the cold incremental run at 2173 MiB. On mikro-orm the wall-time rest is about
0.2 s of a 1.4 s run at 8 cores: a next target.

## Exactness evidence

All comparisons below are byte comparisons of stdout (with the exit status) and of the build info file, written to
the same path by both binaries.

- **Public projects and the private monorepo, release builds on the bench box**, at 8 and at 4 cores (vscode,
  mikro-orm, mui-docs, supabase-studio and the private monorepo): a cold incremental run, a warm run with no change and a
  warm run after an export is appended to the program's most imported file (from `--explainFiles`). For each: main
  and the branch print the same bytes and write the same build info; the branch run twice from the same build info
  writes the same bytes; the branch with `TSRS_INCREMENTAL_STEAL=0` writes the same cold build info; and the warm run
  after the edit prints what a cold run of the edited state prints. All 30 project, core count and state cells
  identical. Files do move in these runs. Comparing `TSRS_FILE_TIMES` of a cold run with and without
  `TSRS_INCREMENTAL_STEAL=0`, the share of the incremental pass's files (and of its check CPU) that ran on another
  checker than the static one was, at 8 / 4 cores: vscode 2.8% (3.6%) / 2.2% (3.1%), mikro-orm 8.7% (10.3%) /
  2.8% (4.3%), mui-docs 29.9% (30.5%) / 38.4% (24.0%), supabase-studio 1.8% (4.1%) / 1.1% (3.4%), the private
  monorepo 5.2% (8.5%) / 2.8% (4.4%).
- **A generated project with global diagnostics** (`lib: ["es2015"]`, async generators), local dev builds, 2, 3, 4 and
  8 checkers, `lib` es2015 and es2020, and with `TSRS_SPLIT_FILES=force:3 --skipLibCheck false`: cold, no change, a
  leaf edit, an exported-shape edit of the shared file, an error added to and removed from a global script, a global
  diagnostic added, kept and removed, two files with one, the first removed, then the second. Main and the branch
  identical at every step (output and build info), the branch identical to itself run twice. The steps where main
  itself prints something else than a cold run are the two of the previous section.
- **`testdata/regressions`**, every case at 2 and 4 checkers, `--fast --incremental true`: cold and warm, main against
  the branch and the branch twice: 530 runs, all identical.
- **`crates/tsrs_cli/tests/integration/incremental_pass_stealing.rs`**: the stolen pass against
  `TSRS_INCREMENTAL_STEAL=0` over an edit sequence at 2 and 4 checkers, and that the cold run took the fallback. With
  the fallback disabled (the stolen result always kept) it fails in 3 of 3 runs.
- **Watch mode** (`--watch --fast`, whose checkers live in regions that `restart_static` replaces): a cold cycle that
  takes the fallback, then an edit; the second cycle prints what a cold run prints.
- **Conformance gate** (`tsrs_determinism conformance`, dev build): no failure, crash or code difference in either
  history, and every seeded and scheduler determinism run passes. One test, `compiler/intersectionConstructorReductionCrash`,
  times out in both histories; it needs more than 20 s of CPU in an unoptimized build and times out on main's dev
  build in fast mode too (60 s). Go history does not reach the changed code.

## The declaration-signature step after an edit to a shared file (measured, not changed)

After an edit that adds an export to a widely imported file, a warm run computes the declaration signatures of the
files that import it before the check (`get_files_affected_by`), and on the private monorepo that step ("Emit time")
used about 3 of the 8 cores. Measured with a temporary patch on main (not part of this change) that logs every
signature emit: its files, its wall time and, per checker thread, the files it emitted and the span from its first
to its last one. One hub edit per project at 8 cores, from a cold build info:

- **Nearly all of it is the batched level emits.** The walk computes each level of the referenced-by closure with one
  emit (`compute_dts_signatures`); single-file emits were under 1% of the time. On the private monorepo the level
  emits were 96% of "Emit time" (vscode 81%). So the step is not one emit per file any more; it is a few large emits
  with a barrier between levels.
- **The levels are unbalanced over the checkers.** Each level's files run on their static owners. A level is the set
  of files that import what changed, not the program, and the locality assignment that balances the whole program
  puts those files on a few checkers: in the private monorepo's four longest levels (62% of the step) the busiest
  checker held 3.7-4.9 times the mean number of files, and two of those levels kept only about 2 threads busy. The
  declaration transform's thread time was 3.4 times the level wall time on average (vscode 6.2, where the busiest
  checker held 1.5-1.7 times the mean on its two large levels).
- **What it would take.** Let the level emits move files between checkers, as the check passes do: a stealing
  `GroupPass` for the emit that `compute_dts_signatures` runs, with owners following the emitted files (the check
  that follows would then run on those checkers, which is fine under canonical history). Balanced to about 7 busy
  threads, the step would take about half as long, roughly a fifth of check plus emit in this case. Exactness is the
  open part: the stored signature hashes the printed declaration, so it needs printed declarations that do not depend
  on which checker prints them or what it resolved before. That holds for the emit oracle under random assignments
  (notes/perf-order-independence.md), but notes/perf-hub-edit-shortcut.md found 7-11 of 22k signatures that changed
  with the checker's state (a cause fixed since); it would need its own build-info byte comparison over many hub
  edits. Not built here.

## Not done

- **Reusing the full type-check pass for the cold run** and filling the per-file cache from it: it collects without
  attributing globals, so it needs the same fallback, and it would bring leaf freeing and checker retirement, which
  the later passes cannot have. The `IncrementalCheck` pass is the full pass's scheduler without those.
- **A timing-independent attribution rule** (first file in program order that needs the global, or every such file):
  a checker that already reported a global cannot tell which later files need it, so either rule needs a record of
  the missing globals each file needs, and both change the build info against main.
- **Keeping globals out of the per-file cache in fast mode**, with a separate stored set recomputed by a full check
  whenever it is non-empty and anything changes: a different build info format and different output from main.
- **Stealing in the signature and emit passes**: see the section above.

## Reproduce

```sh
# cold incremental, stolen and static (same binary)
rm -f /tmp/x.tsbuildinfo; tsrs -p . --noEmit --fast --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo --extendedDiagnostics
rm -f /tmp/x.tsbuildinfo; TSRS_INCREMENTAL_STEAL=0 tsrs -p . --noEmit --fast --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo --extendedDiagnostics
# where each file was checked
TSRS_FILE_TIMES=/tmp/times.tsv tsrs -p . --noEmit --fast --incremental true --tsBuildInfoFile /tmp/x.tsbuildinfo
cargo test -p tsrs_cli --test integration incremental_pass_stealing
```
