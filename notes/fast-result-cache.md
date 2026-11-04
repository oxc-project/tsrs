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

MEASUREMENTS
