# mem-build-free-programs: `tsc -b` frees each project's program once it is built

Issue #597: `tsrs -b` on a graph of about 7,500 composite projects (150,000 files) aborted with "the arena address
range is exhausted" after about 4,800 projects at `--builders 64 --checkers 1`, and peaked at about 108 GB with the
default builders. tsgo completed.

Measured on a Boxy dev container (8 vCPUs, 90 GiB cgroup memory limit), release builds of the PR's base and head,
on synthetic graphs: N composite projects of 20 files each, each project referencing up to 4 earlier ones and
importing from 3 of them (generator in the PR description). No private code.

## The leak

A CLI `-b` that is not a watch session never freed a program. Every project's files, binder data, checkers, types and
incremental snapshot went to the builders' and checkers' thread arenas, which live to process exit, so arena use grew
with the number of projects built. With compressed pointers every arena shares one 32 GiB range, so a large enough
graph aborts:

    tsrs: the arena address range is exhausted (compressed pointers: one 32 GiB address range for every arena; 32712 MiB of it in use).

7,500 projects at `--builders 64 --checkers 1 --force`: aborted after 5,331 projects (66 s, 51.9 GB RSS). Go's GC
frees each program once nothing refers to it; only the declaration and JSON files of the build's parse cache, which
every project shares, stay for the build.

## The fix

The pieces watch mode already uses to free a program (notes/watch-memory.md), applied to every project of a build
(`Orchestrator::frees_built_programs`: not a watch session, not the API, not the test harness):

- The file loader allocates in program regions (`ProgramOptions::regions`) and each checker in a region of its own
  (`checker_regions`).
- The build host parses each cached declaration and JSON file (`host::get_source_file`) into a registered region of
  its own (watchmem.rs `FileRegions`), where it is bound (`bind_in_file_regions`) and where its lazily filled data goes
  (`arena::enter_owner`). These live for the build, as the cache does.
- Once a task is built, the free thread (`Freer`) drops its incremental program and snapshot, frees its program and
  checkers (`free_unshared_program`), and drops its program regions. A task that reported diagnostics keeps its
  program regions: the build's error summary reads the diagnostics' files, which are in them. The diagnostics
  themselves escape the checker regions when made (`escape_for_diagnostic`).
- `arena::route_by_owner` (called by `bind_in_file_regions`): before the first registered region existed,
  `enter_owner` left lazily filled data of an object outside every region in the current target. In an incremental
  build whose upstream projects were up to date, the first program built filled its references' command-line output
  maps (`ParsedCommandLine::parse_input_output_names`) before any declaration file was parsed, so the maps went to that
  program's loader region; the next downstream project read them after that region was freed (crash, heap
  use-after-free). `-b --watch` on the PR's base has the same order: with up-to-date outputs, two full builds of a
  middle project and an edit of a downstream one abort under `TSRS_ARENA_POISON=1`.

Freeing on the builder threads instead of the free thread cost 4% of wall time at 64 builders on 8 vCPUs (3,000
projects: 34.61 s against 33.44 s, median of 3), and nothing at the default builders.

## Numbers

Peak RSS from `/usr/bin/time`, one run unless noted. tsgo is built from the pinned commit.

| graph, flags | base | head | tsgo |
| --- | --- | --- | --- |
| 3,000 projects, `--builders 64 --checkers 1 --force` | 26.0 GB, 33.25 s (median of 3) | 2.2 GB, 33.57 s (median of 3) | 26.1 GB, 73.5 s |
| 3,000 projects, `--force` | 44.1 GB, 36.70 s (median of 3) | 1.3 GB, 33.44 s (median of 3) | 37.2 GB, 80.1 s |
| 7,500 projects, `--builders 64 --checkers 1 --force` | aborted after 5,331 projects (arena range), 51.9 GB, 66 s | 4.8 GB, 91.6 s | 69.5 GB, 197.6 s |
| 7,500 projects, `--force` | killed at the 90 GiB limit after 4,803 projects (42 min) | 3.3 GB, 93.7 s | killed at the 90 GiB limit after 6,258 projects (22 min) |

tsgo's peak on these graphs is its GC's choice (default `GOGC`), not what it keeps; the issue's report of about 30 GB
for tsgo is on the reporter's codebase. The arena reservation in use at the end of the 7,500-project build: 4.0 GiB (head, `TSRS_MEM_SPLIT=1` at exit)
against the full 32 GiB (base).

## Checks

- Outputs, `.tsbuildinfo` files and stdout identical to the base (with `TSRS_ARENA_POISON=1` for the head) on the
  synthetic graphs, on a 60-project graph with type errors, JSON imports and shared global declaration files, and on
  TypeScript 5.8's `src` (13 projects), at `--builders 1`, `16 --checkers 2` and `64 --checkers 1`, with `--verbose`,
  `--pretty false` and `--extendedDiagnostics` (timings aside).
- Incremental sequences (full build, no-op build, edit of one project, a fix of an error, `--force`) identical to the
  base, with every input's time set to the same instant first (otherwise the "newest input" a status names depends on
  copy order, base against base too).
- `tsctests` (the `tsc`/`tsbuild` and watch scenario baselines): the same 484 pass, 38 fail and 1 crash as the base.
  The harness keeps the programs (`Testing::on_program`), so it does not run the new free; the watch scenarios run
  `route_by_owner`.
- crates/tsrs_cli/tests/build_free_programs.rs runs the CLI in poison mode: the error summary after the programs are
  freed, and the incremental build above. Each test fails with its part of the fix removed (the kept program regions,
  `route_by_owner`, the declaration files' own regions).
- Instructions retired: not measured; `perf_event_open` is not permitted in the container. A `-p` build does not
  call any of the changed code.
