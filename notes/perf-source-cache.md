# perf-source-cache: serving `--fast` file reads from a stat-validated cache

2026-11-04, base main 48cf809ef. Code: `crates/tsrs_vfs/src/osvfs/sourcecache.rs`, wired in
`tsrs_execute::execute`; tests: `crates/tsrs_cli/tests/integration/source_cache.rs`.

## Why

On the development Mac (M5 Max, 18 cores) every `open()` goes through an Endpoint Security client (an MDM agent plus
XProtect) and the system tops out near 110k opens a second however many threads ask; `stat` scales to about 1.5M a
second, and a Linux VM on the same hardware opens about 1M files a second. The front end opens every source file each
run: on mui-docs (`--fast --noCheck`, 18 threads) 17.7k opens averaging 112 µs and 17.7k closes averaging 20 µs,
about 2.3 s of thread time, against 6 µs per `stat`. An `open`/`read` interposer serving every read from one
preloaded blob (stats unchanged) cut mui-docs' parallel parse median 0.384 -> 0.215 s and vscode's 0.145 -> 0.097 s
with identical diagnostics; that was the ceiling.

`notes/design-persisted-frontend.md` measured caching parsed and bound trees and named file system calls and module
resolution as the bulk of program construction; `notes/perf-vfs-lightweight-metadata.md` and
`notes/perf-vfs-probes.md` cut the work inside each `stat`. None touched the opens.

## Design

`--fast` command-line compilations (not watch, not `--build`, not the language server or the API) on Unix. One cache
file per project under the user cache directory, keyed by the config file (or the directory and root files without
one) and mapped once read-only: header | key | sorted index | paths | data, one xxh3 over key, index and paths and one
per entry's data. A read of a cached path `stat`s the file and serves the bytes when device, inode, size,
modification time and status-change time (nanoseconds) match and the data's checksum verifies; otherwise the file is
read as before.

- Racy-write rule (git's "racy git"): a file read from disk is stored only when its modification and status-change
  times are more than 2 s older than the moment its read began, so a later write always gives it a newer
  status-change time, even one that keeps the size and restores the modification time (`touch -r`, ruff's
  [#17619](https://github.com/astral-sh/ruff/issues/17619) class of bug). A file system reporting no status-change
  time is never trusted.
- The cache is rewritten only when at least 1/32 of the run's reads came from disk (any, with no cache yet) or a
  quarter of the cached entries went unused, so an edit loop touching a few files reads them from disk instead of
  rewriting the cache each run. The rewrite keeps the entries used plus the new ones (at most 1 GiB), runs on its own
  thread during the check, writes a temporary file and renames it over the old one, and is joined after the output
  is flushed. Otherwise the mapping is dropped once the program is built. A cache directory over 4 GiB loses its
  least recently written files.
- `--extendedDiagnostics` rows `Program:   source cache hits` / `misses`; `TSRS_SOURCE_CACHE=0` turns it off. Bundled
  libraries are in the binary and never go through it.

## Measured and rejected

- **`pread` from one descriptor** instead of a mapping (no `unsafe`, no resident pages). On macOS, preads of one
  file from many threads serialize on the vnode: 17k 2 KiB preads took 14 ms on one thread and 60 ms on 18 (64 ms
  with one descriptor per thread), against 4.8 ms for `memcpy` from a mapping on 18 threads. mui-docs' parallel parse
  got slower than the base (0.80 s against 0.37 s in one pair).
- **Writing after the output.** A cold run paid the synchronous rewrite before exiting: vscode +7.5% wall on Linux
  (145 MB written), +10% on macOS. On its own thread it hides in the check pass.

## Results

### macOS

Release builds of base and candidate built into one target directory. `/usr/bin/time -l` (CPU = user + sys;
instructions include the kernel), `--extendedDiagnostics` for the front-end row, `-p <project> --noEmit --incremental
false --pretty false --fast`, 20 interleaved pairs after one priming run per side, medians of per-pair ratios with
~95% intervals (`*`: excludes 0). The Mac was shared with other agents (load 35-230), so wall times are noisy; CPU
time and instructions are the steadier numbers. Diagnostics were byte-identical in every run.

Warm cache:

| project | front end (parallel parse + resolve) | wall | CPU | instructions | peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| mui-docs | 0.57 -> 0.40 s, −34% [−38, −28] * | −11.3% [−15.3, −6.6] * | −18.1% [−19.2, −16.6] * | −5.0% * | +0.2% |
| vscode | 0.28 -> 0.14 s, −38% [−44, −34] * | −6.6% [−11.7, −1.8] * | −6.8% [−7.8, −6.0] * | −2.8% * | +0.1% |
| supabase-studio | 0.19 -> 0.08 s, −55% [−61, −50] * | −10.9% [−18.8, −3.9] * | −17.6% [−19.4, −15.6] * | −4.9% * | −0.4% * |
| cal-diy | 0.17 -> 0.10 s, −41% [−56, −29] * | −10.9% [−20.1, −3.9] * | −12.9% [−15.5, −10.2] * | −4.1% * | +0.1% |
| t3code-server | 0.08 -> 0.06 s, −21% [−33, −13] * | −7.0% [−15.5, +2.4] | −1.8% [−3.9, +0.8] | −1.0% * | +0.0% |
| a large private monorepo (15 pairs) | −61% [−72, −50] * | −9.2% [−16.4, +7.4] | −5.9% [−8.8, −3.7] * | −2.4% * | +0.3% * |

Thread time in reads (`TSRS_FRONTEND_STATS=1`, mui-docs `--noCheck`, three runs each): 3.7-5.7 s -> 0.37-0.93 s.

Cold cache (B's cache directory removed before each of its runs; 12 pairs): every file read from disk, copied for
the cache and written during the check.

| project | wall | CPU | instructions | peak RSS |
| --- | ---: | ---: | ---: | ---: |
| mui-docs | +3.6% [−6.7, +11.8] | +1.4% [−2.8, +7.6] | +1.0% * | +2.2% * |
| vscode | +0.7% [−1.9, +8.5] | +1.9% [+0.5, +5.2] * | +1.0% * | +6.3% * |
| supabase-studio | +2.8% [+0.5, +6.4] * | +5.2% [+0.5, +7.5] * | +0.9% * | +3.7% * |
| cal-diy | −5.2% [−20.2, +10.3] | +4.8% [−1.6, +5.5] | +1.2% | +2.4% * |
| t3code-server | −11.2% [−26.3, +8.7] | −1.9% [−5.7, +1.3] | −0.2% | +1.6% |
| a large private monorepo (15 pairs) | +0.4% [−7.5, +6.5] | −0.3% [−1.8, +1.5] | +0.2% | +2.2% * |

The cold run's extra memory is the copies held until the write ends; cache files are 35 MB (mui-docs), 64 MB
(cal-diy) and 145 MB (vscode).

### Linux

One m8g.4xlarge (Graviton4, 16 vCPUs, Ubuntu 24.04), both arms `cargo build --release --locked -p tsrs_cli` in one
directory, `runner.mjs` alternating pairs (`perf stat` user-space instructions, `/usr/bin/time` peak RSS, CPU = user
+ sys), 15 pairs at the default 16 checkers. Warm: B's cache primed once. Cold: B's cache directory removed before
each of its runs.

| project | warm wall | warm CPU | warm instructions | warm peak RSS | cold wall | cold CPU | cold peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | +0.1% | −0.2% | −0.0% | +0.1% | +4.2% * | +2.2% * | +5.9% * |
| mui-docs | +0.0% | +0.0% | +0.2% | +0.3% | +1.2% * | +1.5% * | +2.0% * |
| supabase-studio | −0.3% | −0.8% * | +0.0% | +0.2% | +4.1% * | +2.2% * | +3.9% * |
| cal-diy | −0.3% | −0.2% | +0.3% | +0.4% | +2.4% * | +1.2% * | +2.0% * |
| t3code-server | +0.5% | +0.2% | +0.3% | +0.4% | +0.8% | +0.4% | +2.0% * |
| webpack | −1.3% | −0.5% | +0.2% | +0.2% | +3.4% * | +1.2% | +3.7% * |
| mikro-orm | −0.1% | −0.5% | −0.0% | +0.3% | +1.0% * | +1.0% * | +1.3% * |
| xstate-main | −0.5% | −0.5% | +0.1% | −0.2% | +1.4% * | +2.7% * | +5.8% * |

Warm runs are neutral: an open costs about 1 µs there. Cold runs pay for the copies and the write; with 16 checkers
on 16 vCPUs the writer thread has no idle core (vscode: +7.5% wall with the write after the output, +4.2% on its own
thread).

Single-threaded (`--singleThreaded`, 5 pairs; user-space instructions repeat to 0.0%):

| project | fast: instructions | fast: wall | fast: peak RSS | default mode: instructions | default mode: peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| vscode | +0.1% * | +0.5% | −0.0% | +0.0% | +0.1% |
| mui-docs | +0.1% * | −1.0% * | −0.0% | −0.0% | +0.0% |
| supabase-studio | +0.2% * | −0.3% | +0.1% | −0.0% | +0.0% |
| cal-diy | +0.2% * | −1.2% | −0.4% * | +0.0% | −0.6% |
| t3code-server | +0.1% * | −0.9% * | +0.3% | −0.0% | +0.0% |
| webpack | +0.1% * | −0.1% | +0.3% * | −0.0% | +0.2% * |
| mikro-orm | +0.0% * | −1.4% | −0.3% | +0.0% | −0.5% |
| xstate-main | +0.2% * | −1.2% | +0.4% | +0.0% | +0.7% * |

Default mode does not use the cache. In fast mode the user-space count rises 0.0-0.2%: a hit checksums the bytes and
binary-searches the index, while the `open`/`read` it replaces is mostly kernel time, which `instructions:u` does not
count (on macOS, where the count includes the kernel, it falls 1-5%).

## Correctness

- `source_cache.rs` runs `tsrs --fast` processes and compares every cached run with the cache off: a same-size edit
  with the modification time restored, a file read within 2 s of its change (not stored), delete, new file, rename
  over, pnpm-style symlinked directories repointed (with and without `preserveSymlinks`), case variants and case-only
  renames, a truncated, bit-flipped, garbage and empty cache file, four concurrent cold runs three times over, and
  the cache off without `--fast` or with `TSRS_SOURCE_CACHE=0`. Ignoring the status-change time makes the first test
  fail (checked by mutating the identity).
- Byte-identical diagnostics and exit status, base against candidate, on the eight prepared catalog projects above on
  the Linux box: default mode, `--fast` with the cache cold, warm and off. Every warm run served every read from the
  cache.
- In fast mode through the cache: `tools/regressions.sh` (50 of 50, cold then warm) and `tsrs_determinism
  regressions` (950 runs at 2 and 4 checkers, none failed).

## Limits

- A program that rewrites a cache file in place while tsrs has it mapped would make reads of the changed pages fault
  or see new bytes; tsrs never writes one in place.
- The identity rule trusts the file system's status-change time. File systems that fake it (some network and FUSE
  mounts) or a clock stepped back into the same timestamp tick could hide an in-place edit of the same size; git's
  index has the same exposure.
- Not on Windows (no inode or status-change time in `std`'s metadata there).
- The hit path still `stat`s each file, which module resolution has just done (`file_exists`); reusing that stat
  would save about 17k `stat`s on mui-docs (~0.1 s of thread time, a few ms of wall). Not done.
