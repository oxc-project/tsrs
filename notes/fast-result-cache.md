# fast-result-cache: replay a `--fast --noEmit` check when nothing it read changed

Type checks are rerun constantly on trees where nothing that matters changed: git hooks, agents checking their own
work, editor tasks. Deno's `deno check` skips checking when a hash of its module graph matches the last run;
Turborepo, sccache and Bazel replay a task's outputs from recorded inputs. tsrs can do the same exactly and inside the
compiler, because every file system question a check asks goes through one VFS. In `--fast` mode, a check that writes
no files now records its output with every answer it got, and the next identical command replays the output when all
those answers are unchanged: tens of milliseconds instead of seconds.

Code: `crates/tsrs_execute/src/resultcache.rs` (the CLI's `main` calls `resultcache::command_line` instead of
`execute::command_line`; `perform_compilation` reports whether its options allow recording). Tests:
`crates/tsrs_cli/tests/integration/result_cache.rs`. User docs: [USAGE.md](../docs/USAGE.md#result-cache).

## Why the replay is exact

The compiler's output is a function of its arguments, environment, binary and the answers the file system gives it.
In fast mode it does not depend on the schedule (the determinism gate checks that). So if a run under schedule S asked
questions Q and got answers A, a run today under the same schedule asks the same first question, gets the same
answer if the file system still gives A for it, and by induction asks exactly Q and prints the same output; and any
other schedule prints the same output too. The recorded run's questions need not be the same set every run (parallel
parsing races on the resolver caches; observation counts differ by one or two between runs on vscode and cal-diy),
because the argument only replays the recorded run.

What that needs, and how it is enforced:

- **Every question goes through the recorder.** The recorder wraps the OS file system beneath the bundled-library layer
  (embedded libraries are part of the binary), and the CLI's system hands it to every consumer (config parsing, the
  compiler host, resolution). Code that reads files outside the VFS is reached only through switches that turn the
  cache off: `--checkerCostCache`, `--checkerAssignment file:`, the `TSRS_*` measurement variables, content mappers
  (external processes; `System::spawn` also spoils a recording).
- **The answers are compared exactly.** File existence, directory existence, `stat` (name, size, mode, mtime),
  directory listings (files, directories, symlinked names) and realpaths are asked again on validation and compared by
  a 128-bit xxh3 digest. File contents are vouched for by the file's device, inode, size, mode, and modification and
  change times, taken *before* the read: an edit after the stat changes the change time, so the next run cannot match.
  `touch -r` restores a modification time but not the change time. A failed read is retried on validation.
- **Racy files are hashed.** A file whose modification or change time is within two seconds of the recording run's
  start (or later) could be changed again within one time stamp tick without its stats changing (git's "racy git"
  rule). Its text is digested instead and read on validation. Two seconds covers FAT's granularity; APFS, ext4 and xfs
  use nanoseconds or the kernel tick.
- **A question answered twice must get one answer.** The recorder keeps the first answer per (kind, path); a later
  different answer (a file changed during the run) spoils the recording, as does any write through the VFS.
- **Everything else is in the key**: the binary (version string and the executable's device, inode, size and times,
  which covers the embedded libraries), the full argument list, the working directory, `TSRS_FAST`, `TSRS_LIB_PATH`,
  `FORCE_COLOR`, `NO_COLOR`, `TERM=dumb`, `RAYON_NUM_THREADS`, whether stdout is a terminal (the `--pretty` default),
  the available parallelism (the default checker count) and the file system's case sensitivity. The environment
  variables were found by auditing every `std::env::var` and `get_environment_variable` read on the CLI path; any
  `TSRS_*` variable other than `TSRS_FAST`, `TSRS_LIB_PATH` and the cache's own turns the cache off (they are
  experiment and measurement switches whose runs must happen, several write to stderr or to files).
- **Only side-effect-free, timing-free runs are recorded**: fast mode, `--noEmit`, not incremental (a build info file
  is a write), not `--build` or `--watch`, no `--diagnostics`/`--extendedDiagnostics`, `--generateTrace`,
  `--generateCpuProfile` or `--pprofDir`, no `--runExternalCode`, exit status 0-2, nothing written to stderr through
  the system. `--listFiles`, `--explainFiles`, `--traceResolution` and `--pretty` output are deterministic and replay.

Not covered, by design: a network file system whose server clock lags the client's by more than the racy window
(git has the same exposure); file contents changed without updating the change time (writes to the block device).
Windows has no change time in `std` metadata, so the cache is Unix only.

## Storage

One file per key in the user cache directory (`$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS,
`/tsrs/result-cache`), or `TSRS_RESULT_CACHE_DIR`. Layout: magic, key, exit status, stdout bytes, the records (kind,
path, digest), project paths before `node_modules` paths (a miss is found sooner), and an xxh3-128 of the whole file;
a torn or corrupt file fails the checksum and is a miss. Entries are written to a temporary file and renamed, so
concurrent runs never read a partial entry; the last writer wins. After each write the directory keeps the newest
64 entries within 256 MiB; it only ever deletes files named like entries (32 hex digits) or their temporary files.

`TSRS_RESULT_CACHE=0` turns the cache off and `TSRS_RESULT_CACHE=log` prints one line per run (hit with the number of
observations and the time to check them, miss with the first changed answer, recorded with size and write time).
The bench, perf-probe and determinism tools and `tools/regressions.sh` set `TSRS_RESULT_CACHE=0`, since they repeat
one command on an unchanged tree; so do three CLI tests that rerun regression fixtures.

## Measurements

One `m8g.4xlarge` (Graviton4, 16 vCPU, Ubuntu 24.04), both arms release builds from the same source directory:
A = main `48cf809ef`, B = this branch. `-p <project> --noEmit --incremental false --pretty false --fast`, default
checker count, 15 alternating pairs per workload (the aws-bench-box runner: wall, CPU, `perf` instructions, peak RSS).
Intervals are the 95% interval of the median paired ratio. The timing stages below ran on `71dd343`, whose only
difference from the final code is how a recording run writes its entry (it cloned and sorted every path and built the
entry in memory; the final code streams it from the recorder's maps).

**Correctness.** For every prepared project, with `--pretty false` and `--pretty true`: A's output, B with the cache
off, B's recording run and B's replay are byte-identical in stdout and exit status (both on `71dd343` and on the final
`6a3c2d959`), and the replays printed only the cache's log line on stderr. The 11 integration tests pass on Linux
(`ed9081af2`); the first 10 also passed on macOS (`71dd343`; the pruning test came later).

**Replay against a normal `--fast` run** (cache primed; median ms; "checked" is the cache's own time from opening the
entry to the end of validation, final binary):

| project | normal wall | replay wall | normal CPU | replay CPU | replay instructions | replay peak RSS | observations | entry | checked |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 1,179 | 31.2 | 16,029 | 89 | 89 M | 15 MiB | 33,359 | 4.1 MB | 8.6 |
| t3code-server | 2,125 | 27.7 | 19,647 | 58 | 43 M | 11 MiB | 14,831 | 2.4 MB | 6.4 |
| mui-docs | 1,213 | 38.0 | 10,819 | 136 | 174 M | 23 MiB | 77,564 | 11.0 MB | 17.2 |
| mikro-orm | 1,049 | 25.8 | 13,220 | 42 | 25 M | 10 MiB | 10,536 | 1.1 MB | 3.2 |
| supabase-studio | 951 | 40.3 | 11,087 | 155 | 182 M | 23 MiB | 67,793 | 11.4 MB | 17.7 |
| cal-diy | 925 | 35.0 | 10,446 | 119 | 126 M | 18 MiB | 59,903 | 7.3 MB | 12.1 |
| webpack | 215 | 25.1 | 2,260 | 39 | 21 M | 9 MiB | 8,841 | 1.1 MB | 3.0 |
| xstate-main | 201 | 26.0 | 1,202 | 42 | 25 M | 9 MiB | 9,421 | 1.4 MB | 4.7 |

Paired wall time ratios: -87.0% (xstate-main) to -98.7% (t3code-server), every interval under a point wide. About
22 ms of each replay is process start and the runner's `bash`/`perf stat`/`time` wrappers: the smallest projects
replay in 25-26 ms with 3-5 ms of checking. The rest scales with the observations (mostly `statx`; 2,693 directory
listings on vscode, 132 realpaths).

**Recording cost** (B finds no entry, checks and writes one; `71dd343`):

| project | wall | CPU | instructions | peak RSS |
| --- | --- | --- | --- | --- |
| vscode | +1.7% [+1.0, +2.3] | +0.0% [-0.7, +1.7] | +0.2% [+0.1, +0.3] | +0.6% [+0.3, +0.8] |
| t3code-server | +1.0% [-0.7, +2.7] | +0.1% [-1.1, +1.4] | -0.0% [-0.5, +0.8] | +0.1% [-0.8, +1.1] |
| mui-docs | +5.4% [+3.0, +6.6] | +1.1% [-0.1, +2.4] | +0.3% [+0.2, +0.5] | +3.2% [+2.5, +3.4] |
| mikro-orm | +0.4% [+0.0, +1.1] | +0.1% [-0.9, +0.7] | +0.0% [-0.8, +0.7] | +0.5% [-0.4, +0.8] |
| supabase-studio | +5.6% [+4.7, +6.5] | +0.5% [+0.1, +1.6] | +0.4% [+0.3, +0.6] | +1.8% [+1.6, +1.9] |
| cal-diy | +3.5% [+2.5, +4.6] | +1.1% [-0.5, +2.9] | +1.1% [+0.2, +1.7] | +2.0% [+1.3, +2.6] |
| webpack | +1.4% [-1.4, +3.0] | +0.3% [-0.6, +1.6] | -0.1% [-1.0, +1.7] | +0.7% [+0.4, +1.0] |
| xstate-main | +2.1% [+0.3, +4.9] | +1.2% [+0.8, +1.6] | +0.5% [+0.3, +0.7] | +0.8% [-0.1, +2.1] |

The CPU barely moves while wall time grows on the projects with the largest entries: the cost is serial work after
the output is flushed (cloning and sorting 60-78k paths and building an 11 MB entry). The final code streams the entry
from the maps instead; its own timer puts the write at 25-27 ms on mui-docs and supabase-studio, 20 ms on cal-diy,
11 ms on vscode and 2.5-6 ms on the rest. Its paired recording numbers are listed under "Pending" below.

**Stale entry** (a project file the program reads gets a new comment line before every run, so B opens the entry,
validates until it meets the edit, checks and records again; `71dd343`): wall +0.6% (mikro-orm) to +7.4%
(supabase-studio), CPU -0.3% to +1.5%, instructions +0.0% to +0.7%, peak RSS +0.3% to +3.4%. Per project, wall:
vscode +1.9%, t3code-server +1.6% (interval crosses 0), mui-docs +6.5%, mikro-orm +0.6% (crosses 0),
supabase-studio +7.4%, cal-diy +5.0%, webpack +3.3% (crosses 0), xstate-main +4.3%.

**Cache off** (`TSRS_RESULT_CACHE=0`): every wall, CPU, instruction and RSS interval contains 0.

**macOS** (Apple M5 Max, debug build, vscode, a loaded shared machine): a replay checks the 33,368 observations in
56-143 ms; 40 replays run four at a time all hit and printed the recorded output. Validation re-lists directories,
so it opens each listed directory once (2,693 on vscode); macOS caps `open` near 110k/s on that machine, about 25 ms of
the replay. Before those runs, ten consecutive runs in a bash loop each missed on one `file_exists` probe of a file
that had not changed in a month, then re-recorded; it did not happen again in 46 later runs and was not explained.
A miss only re-runs the check, so the output stayed correct.

**Pending** (on the box when its SSH access was lost to a client IP change): the final binary's paired recording,
stale, single-threaded (`--singleThreaded`, recording) and default-mode instruction counts, and the `tsrs --version`
floor.

## Not done, and why

- **Directory fingerprints instead of re-listing.** A directory's modification time changes when an entry is added,
  removed or renamed, so a stat could replace a listing on validation (no `open`, which matters on macOS). But a
  listing also classifies symlinked entries by their targets and drops dangling ones; a dangling link that starts
  resolving changes the listing without touching the directory. Doing it exactly needs the raw entries at recording
  time (a second listing per directory on the miss path). Not worth it on Linux, where the whole replay is 3-18 ms of
  checking.
- **Racy entries stay racy.** A file recorded by its text is read on every replay until the entry is recorded again
  (after the next miss). After a large checkout followed at once by a check, that can be many files.
- **Smaller entries.** Paths are stored whole (11 MB on mui-docs). Sorting and prefix-coding them would shrink entries
  about 4x but put a sort of 78k long strings back on the recording path.
