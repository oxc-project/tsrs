# fast-result-cache: replay a `--fast --noEmit` check when nothing it read changed

Type checks are rerun constantly on trees where nothing that matters changed: git hooks, agents checking their own
work, editor tasks. `deno check` skips checking when a hash of its module graph matches; Turborepo, sccache and Bazel
replay a task's outputs from recorded inputs. tsrs can do the same exactly, inside the compiler, because every file
system question a check asks goes through one VFS: a `--fast` check that writes no files records its output with
every answer it got, and the next identical command replays the output when all those answers are unchanged.

Code: `crates/tsrs_execute/src/resultcache.rs` (the CLI's `main` calls `resultcache::command_line`;
`perform_compilation` reports whether its options allow recording). Tests:
`crates/tsrs_cli/tests/integration/result_cache.rs`. User docs: [USAGE.md](../docs/USAGE.md#result-cache).

## Why the replay is exact

The compiler's output is a function of its arguments, environment, binary and the answers the file system gives it,
and in fast mode not of the schedule (the determinism gate checks that). If a run asked questions Q and got answers
A, a run today asks the same first question, gets the same answer if the file system still gives A for it, and by
induction asks exactly Q and prints the same output. The recorded run's questions need not be the same set every run
(parallel parsing races on the resolver caches; observation counts differ by one or two between runs on vscode and
cal-diy): the argument only replays the recorded run.

- **Every question goes through the recorder.** It wraps the OS file system beneath the bundled-library layer
  (embedded libraries are part of the binary). Code that reads files outside the VFS is reached only through
  switches that turn the cache off: `--checkerCostCache`, `--checkerAssignment file:`, any `TSRS_*` variable other
  than `TSRS_FAST`, `TSRS_LIB_PATH` and the cache's own, content mappers (`System::spawn` also spoils a recording).
- **The answers are compared exactly.** File existence, directory existence, `stat`, directory listings (files,
  directories, symlinked names) and realpaths are asked again on validation and compared by a 128-bit xxh3 digest.
  File contents are vouched for by the file's device, inode, size, mode, and modification and change times, taken
  before the read: an edit after the stat changes the change time, so the next run cannot match, and `touch -r`
  restores a modification time but not the change time.
- **Racy files are hashed.** A file whose modification or change time is within two seconds of the recording run's
  start (or later) could change again within one time stamp tick without its stats changing (git's "racy git" rule),
  so its text is digested instead and read again on validation.
- **A question answered twice must get one answer.** The recorder keeps the first answer per (kind, path); a later
  different answer, or any write through the VFS, spoils the recording.
- **Everything else is in the key**: the binary (version string and the executable's device, inode, size and times,
  which covers the embedded libraries), the argument list, the working directory, `TSRS_FAST`, `TSRS_LIB_PATH`,
  `FORCE_COLOR`, `NO_COLOR`, `TERM=dumb`, `RAYON_NUM_THREADS` (every `std::env::var` and `get_environment_variable`
  read on the CLI path), whether stdout is a terminal (the `--pretty` default), the available parallelism (the
  default checker count) and the file system's case sensitivity.
- **Only side-effect-free, timing-free runs are recorded**: fast mode, `--noEmit`, not incremental (a build info
  file is a write), not `--build` or `--watch`, no `--diagnostics`/`--extendedDiagnostics`, `--generateTrace`,
  `--generateCpuProfile` or `--pprofDir`, no `--runExternalCode`, exit status 0-2, nothing written to stderr through
  the system. `--listFiles`, `--explainFiles`, `--traceResolution` and `--pretty` output are deterministic and replay.

Not covered, by design: a network file system whose server clock lags the client's by more than the racy window
(git has the same exposure); file contents changed without updating the change time. Windows has no change time in
`std` metadata, so the cache is Unix only.

## Storage

One file per key in the user cache directory (`$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS,
`/tsrs/result-cache`), or `TSRS_RESULT_CACHE_DIR`. The records are written project paths before `node_modules`
paths and file reads and listings first, so a miss is found sooner; an xxh3-128 of the whole file ends it, so a
corrupt file is a miss. Entries are written to a temporary file and renamed, so concurrent runs never read a
partial entry; the last writer wins. After each write the directory keeps the newest 64 entries within 256 MiB,
deleting only files named like entries or their temporary files. The bench, perf-probe and determinism tools and
`tools/regressions.sh` set `TSRS_RESULT_CACHE=0`, since they repeat one command on an unchanged tree.

## Measurements

One `m8g.4xlarge` (Graviton4, 16 vCPU, Ubuntu 24.04), both arms release builds from the same source directory:
A = main `48cf809ef`, B = this branch at `f1408acaf` unless a row says otherwise.
`-p <project> --noEmit --incremental false --pretty false --fast`, default checker count, 15 alternating pairs per
workload (the aws-bench-box runner: wall, CPU, `perf` instructions, peak RSS). Intervals are the 95% interval of the
median paired ratio; `~` marks an interval that contains 0.

**Correctness.** For every prepared project, with `--pretty false` and `--pretty true`: A's output, B with the cache
off, B's recording run and B's replay are byte-identical in stdout and exit status (checked on `71dd343`, `6a3c2d959`
and `f1408acaf`). The integration tests pass on Linux; all but the pruning test also passed on macOS (`71dd343`).

**Replay against a normal `--fast` run** (cache primed; medians in ms; "checked" is the cache's own time from opening
the entry to the end of validation, median of 5):

| project | normal wall | replay wall | normal CPU | replay CPU | replay instructions | replay peak RSS | observations | entry | checked |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 1,188 | 32.0 | 16,041 | 101 | 88 M | 15 MiB | 33,359 | 4.1 MB | 7.6 |
| t3code-server | 2,126 | 29.5 | 19,613 | 61 | 43 M | 11 MiB | 14,831 | 2.4 MB | 6.2 |
| mui-docs | 1,220 | 41.0 | 10,740 | 153 | 172 M | 23 MiB | 77,564 | 11.0 MB | 16.9 |
| mikro-orm | 1,034 | 25.7 | 13,009 | 42 | 24 M | 9 MiB | 10,536 | 1.1 MB | 2.5 |
| supabase-studio | 931 | 41.7 | 10,804 | 177 | 181 M | 23 MiB | 67,793 | 11.4 MB | 17.5 |
| cal-diy | 924 | 35.9 | 10,371 | 132 | 125 M | 18 MiB | 59,903 | 7.3 MB | 12.3 |
| webpack | 213 | 26.2 | 2,251 | 39 | 20 M | 8 MiB | 8,841 | 1.1 MB | 2.7 |
| xstate-main | 201 | 28.2 | 1,197 | 45 | 25 M | 10 MiB | 9,421 | 1.4 MB | 4.3 |

Paired wall ratios: -86.0% (xstate-main) to -98.6% (t3code-server), every interval under half a point wide. A
`tsrs --version` under the same runner takes 20.8 ms (process start plus the runner's `bash`, `perf stat` and `time`
wrappers), so a replay is 5-21 ms above that floor. Under `strace -c`, `statx` is 60-83% of a replay's system call
time (file fingerprints and existence probes, 76.7k calls on mui-docs), `readlinkat` from realpaths 12-17% on the
pnpm projects (t3code-server, supabase-studio, cal-diy), the directory listings' `getdents64` 11% on vscode (2,693
directories) and under 3% elsewhere.

| observations by kind | vscode | t3code-server | mui-docs | mikro-orm | supabase-studio | cal-diy | webpack | xstate-main |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| file exists | 15,207 | 8,394 | 52,584 | 6,514 | 40,034 | 34,882 | 5,910 | 5,490 |
| directory exists | 4,816 | 2,298 | 6,431 | 845 | 10,359 | 11,660 | 919 | 1,759 |
| file read (stats) | 10,511 | 3,184 | 17,680 | 2,870 | 14,474 | 10,849 | 1,656 | 1,791 |
| directory listing | 2,693 | 157 | 290 | 195 | 1,015 | 489 | 227 | 75 |
| realpath | 132 | 798 | 579 | 112 | 1,911 | 2,023 | 129 | 306 |

**Misses.** "Recording": B finds no entry, checks and writes one. "Stale": a file the program reads gets a fresh
comment line before every run, so B opens the entry, validates until it meets the edit, checks and records again.
"Off": `TSRS_RESULT_CACHE=0`. Wall time, then CPU, instructions and peak RSS:

| project | recording wall | stale wall | off wall | recording CPU | recording instructions | recording peak RSS |
| --- | --- | --- | --- | --- | --- | --- |
| vscode | +0.4% ~ | +2.5% ~ | -1.3% ~ | +0.3% ~ | +0.1% ~ | +0.6% [+0.4, +0.8] |
| t3code-server | -0.4% ~ | +0.8% ~ | +0.2% ~ | +0.1% ~ | +0.1% ~ | +0.4% ~ |
| mui-docs | +3.7% [+2.3, +5.7] | +5.0% [+3.7, +6.6] | +0.6% ~ | +0.8% ~ | +0.2% [+0.1, +0.3] | +1.2% [+0.8, +1.7] |
| mikro-orm | +1.2% ~ | +0.8% [+0.1, +2.3] | +0.1% ~ | +0.5% ~ | +0.8% ~ | +0.8% [+0.1, +1.6] |
| supabase-studio | +3.3% [+1.8, +4.1] | +4.7% [+3.0, +5.8] | -0.5% ~ | +0.9% ~ | +0.3% [+0.0, +0.7] | +1.1% [+0.7, +1.2] |
| cal-diy | +1.9% ~ | +3.5% [+1.1, +5.1] | -1.1% ~ | +0.7% ~ | -0.0% ~ | +0.8% [+0.1, +1.1] |
| webpack | +1.2% ~ | +2.9% [+0.6, +4.3] | +0.1% ~ | +0.6% ~ | +0.1% ~ | +0.1% ~ |
| xstate-main | +1.4% [+0.1, +3.4] | +2.8% [+1.9, +3.9] | +0.5% ~ | +1.0% [+0.6, +1.6] | +0.2% [+0.1, +0.5] | +0.7% [+0.0, +1.9] |

The "off" column is from `6a3c2d959` (its path does not differ); all its CPU, instruction and RSS intervals contain 0
too. CPU barely moves while wall time grows with the entry size: the cost is serial work after the output is flushed.
The entry write itself takes 2.5-3.2 ms on the small projects, 9.6 ms on vscode, 17.6 ms on cal-diy and 22-23 ms on
mui-docs and supabase-studio (11 MB). The rest of a stale miss is reading and decoding the old entry before the run.

**Single-threaded** (`--singleThreaded`, recording every run, 5 pairs, `6a3c2d959`): instructions +0.1% (vscode,
t3code-server, mikro-orm), +0.2% (webpack), +0.5% (supabase-studio, cal-diy, xstate-main), +0.6% (mui-docs), every
interval within 0.1 point; peak RSS -0.1% to +2.3% (the answers held in memory). **Default mode** (no `--fast`,
single-threaded, 5 pairs, `6a3c2d959`): instructions +0.0% on all eight, every interval [-0.0%, +0.0%]; the cache
never runs there.

**macOS** (Apple M5 Max, debug build, vscode, a loaded shared machine): a replay checks the 33,368 observations in
56-143 ms; 40 replays run four at a time all hit and printed the recorded output. Validation re-lists directories,
so it opens each listed directory once (2,693 on vscode); macOS caps `open` near 110k/s on that machine, about 25 ms
of the replay. Before those runs, ten consecutive runs in a bash loop each missed on one `file_exists` probe of a
file that had not changed in a month, then re-recorded; it did not happen again in 46 later runs and was not
explained. A miss only re-runs the check, so the output stayed correct.

## Measured and rejected, or not done

- **Building the entry in memory with sorted paths** (`71dd343`): recording cost up to +5.6% wall (supabase-studio)
  and +3.2% peak RSS (mui-docs), stale entries up to +7.4%. Streaming the entry from the recorder's maps with one
  write per record into a 1 MiB buffer gives the numbers above. Replays may have lost a little locality with the
  sorted order: `71dd343` checked mui-docs in 13.0 ms and supabase-studio in 14.6 ms, against 16.9 and 17.5 ms now
  (separate sessions, not a paired comparison).
- **Directory fingerprints instead of re-listing.** A directory's modification time changes when an entry is added,
  removed or renamed, so a stat could replace a listing on validation (no `open`, which matters on macOS). But a
  listing also classifies symlinked entries by their targets and drops dangling ones; a dangling link that starts
  resolving changes the listing without touching the directory. Doing it exactly needs the raw entries at recording
  time (a second listing per directory on the miss path). Not worth it on Linux, where the whole replay is 3-18 ms of
  checking.
- **Racy entries stay racy.** A file recorded by its text is read on every replay until the entry is recorded again
  (after the next miss). After a large checkout followed at once by a check, that can be many files.
- **Smaller entries.** Paths are stored whole (11 MB on mui-docs). Sorting and prefix-coding them would shrink
  entries about 4x but put a sort of 78k long strings back on the recording path.
