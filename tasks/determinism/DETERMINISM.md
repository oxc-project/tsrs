# Determinism and diagnostic snapshots

This Rust task checks that tsrs reports the same ordered errors and exit status as
the matching npm Go compiler, across checker assignments, checker counts, and
repeated runs. Every project in `bench/projects.json` is covered: currently 19
projects and 456 comparisons. Compiler errors are valid results; compiler crashes
and incomplete runs always fail.

A separate regression sweep checks fixture expectations across default and seeded
random schedules, so focused reproductions retain multithreaded coverage.

The expectation is one committed JSON snapshot per project in `snapshots/`.
Snapshots come from the upstream native Go compiler with `--singleThreaded`.
They are independent of the tsrs binary being checked. Updating snapshots does
not bless differences between tsrs configurations.

## Setup and commands

Run from the repository root:

```sh
pnpm install --frozen-lockfile
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner -p tsrs_determinism
target/debug/tsrs_determinism plan
target/debug/tsrs_determinism check
target/debug/tsrs_determinism check --projects webpack --tsrs target/debug/tsrs
target/debug/tsrs_determinism regressions
```

The task prepares the selected benchmark checkouts using the existing benchmark
setup command, including pinned revisions, dependency installation and overlays.
Preparation requires network access on a cache miss. The Go compiler is the root
pnpm `typescript` dev dependency, pinned exactly to
`[workspace.metadata.typescript].nightly` in `Cargo.toml`. pnpm installs its native
platform package and records package integrity in the root lockfile. The task
resolves that package through the root dependency, so a project's own TypeScript
installation cannot shadow the reference. There is no task-specific compiler
installation or compiler build.

## Snapshot updates

Generate snapshots explicitly when the TypeScript pin or benchmark inputs change:

```sh
target/debug/tsrs_determinism snapshot
target/debug/tsrs_determinism snapshot --projects webpack
git diff -- tasks/determinism/snapshots
target/debug/tsrs_determinism check
```

When upgrading TypeScript, update Cargo's metadata, the exact root dev dependency
and the pnpm lockfile together before regenerating snapshots. Review the complete
diagnostic changes. A normal check never writes snapshots. Missing snapshots or
snapshots for different source pins, project recipes or overlays fail the check.
An abnormal reference run never replaces an existing snapshot.

Each reference run uses the same project configuration as tsrs:

```sh
<root-native-TypeScript> -p <project> --noEmit --incremental false --pretty false --singleThreaded
```

Go supplies its own assignment behavior; it does not accept tsrs's
`--checkerAssignment` option. Snapshot provenance records the npm release, native
package, binary SHA-256, and Cargo source pin.

## Comparison matrix

Each tsrs invocation uses `-p <project> --noEmit --incremental false --pretty false`
and one row of the following matrix, in table order:

| `--checkerAssignment` | `--checkers` | Repetitions |
| --- | --- | --- |
| `go` | 4 | 3 |
| `go` | max | 3 |
| `locality` | 4 | 3 |
| `locality` | max | 3 |
| omitted (default scheduler) | 4 | 3 |
| omitted (default scheduler) | max | 3 |
| `random:1`, `random:2`, `random:3` | 4 | 1 per seed |
| `random:1`, `random:2`, `random:3` | max | 1 per seed |

`max` means the available CPU parallelism, capped at the compiler limit of 256;
the task passes its numeric value and logs it. Both labels run even if max is 4.
The compiler may further cap the actual checker count to the number of files.
Projects and compiler invocations run sequentially in catalog and matrix order.
CI checks the full catalog in one task invocation, sharing one tooling install
and benchmark cache, and uploads one combined results artifact.

Named assignments disable work stealing. The default rows omit the assignment
flag, exercising the production scheduler and timing-dependent work stealing.
Each random seed varies both checker ownership and file visit order. Fixed seeds
make that permutation reproducible; repeating default runs samples the scheduler's
timing. Seeds do not make dynamic thread scheduling reproducible or enumerate all
possible schedules. The random run's label and recorded command include the seed
(for example, `random-max-2` uses `--checkerAssignment random:2`). All variants
compare against the same upstream snapshot.

## Regression scheduling

The `regressions` command retains the previous gate's scheduling coverage for every
`testdata/regressions/*/tsconfig.json` fixture, without benchmark preparation:

```sh
target/debug/tsrs_determinism regressions
target/debug/tsrs_determinism regressions --projects union-too-complex-cross-product
```

First it runs `-p <fixture> --pretty false --singleThreaded` and compares the full
combined output with the fixture's committed `expected.txt`, normalizing CRLF and
the absolute fixture directory to `<dir>`. As in `tools/regressions.sh`, the
`expected.txt` comparison ignores trailing newlines; schedule comparisons retain
them. It then compares the full ordered output and exit status with that checked
single-threaded run under 2 and 4 checkers:
three default scheduler runs and one run each for `random:1` through `random:6`.
That is 19 invocations per fixture. Crashes, timeouts and incomplete runs fail.

These fixtures retain their existing expectations because some exercise intentional
canonical-history differences from Go. Go history is tested in the benchmark
snapshot matrix. The regression sweep uses the same CLI invocation as
`tools/regressions.sh`; it includes resolution traces and never rewrites expectations.
Results default to `target/determinism/regressions/`, with a separate summary and
per-fixture logs, JSON and diffs. CI runs the sweep in the same determinism job,
including when a benchmark comparison fails, and uploads it with the benchmark results.

## Comparisons and failures

Snapshots retain diagnostic lines and their indented continuation lines in order,
including duplicates, codes, locations and message text. Only CRLF line endings
and absolute benchmark work/repository prefixes are normalized, to `<work>` and
`<repo>`. Compiler progress or timing output is not part of an error snapshot.
Combined stdout/stderr is retained in full for investigation.

Normal exit codes are 0, 1 and 2 and must match the snapshot. Nonzero exits without
recognizable TypeScript errors fail, as do panic markers, signals, launch failures
and timeouts. The default timeout is 900 seconds per compiler invocation. Ambient
compiler experiments, allocator overrides and thread settings are cleared; worker
parallelism is set to the detected available CPU count.

The task continues after compiler failures and diagnostic mismatches, and exits
unsuccessfully if any case failed. Results default to `target/determinism/`:

- `summary.json` lists every attempted case and failure.
- `<project>/<assignment>-<count>-<repetition>.log` holds raw combined output.
- The adjacent JSON contains the command, requested checker count, exit status,
  failure and normalized diagnostics; `.diff` files compare expected and actual
  diagnostic lines.

CI uploads these results even on failure. Investigate the raw log and snapshot
diff before changing expectations. A compiler mismatch is a failing parity case.

## Options

All commands accept `--projects all|name,...` (default: all). Benchmark names run
in catalog order; `regressions` selects fixture names in alphabetical order.
`snapshot`, `check` and `regressions` also accept:

| Option | Default |
| --- | --- |
| `--tsrs BIN` | `target/debug/tsrs` (used by `check` and `regressions`) |
| `--work-dir DIR` | `bench/.work` |
| `--snapshots-dir DIR` | `tasks/determinism/snapshots` |
| `--out-dir DIR` | `target/determinism` (`target/determinism/regressions` for the regression sweep) |
| `--timeout SECONDS` | `900` |

Relative paths are resolved from the repository root. Snapshot generation requires
the root pnpm dependencies; `check` uses the committed reference provenance.
