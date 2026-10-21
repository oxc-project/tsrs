# lifecycle-leaks: memory across repeated program lifecycles

Issue #646 asks for observed leaks to be tracked across repeated build and language-server cycles, not inferred from
the unsafe sites. This note adds `tools/lsp-mem/lifecycle_leaks.py`, which drives one long-lived `tsrs` process
through N identical cycles that create and release a program (a watch rebuild, an edit, a snapshot) and fits the
growth of its resident memory per cycle, and records what it found at main 357d1d3 (2026-10-21) plus this change:
release build, macOS on Apple Silicon (18 cores, 128 GiB), a synthetic project of 40 chained modules (105 source
files per program with the bundled libs).

## In plain words

Six of the eight scenarios are steady: resident memory stays flat or settles, nothing stays used in the thread
arenas or live in the heap from one cycle to the next. Two grow by a constant amount per cycle for as long as the
run lasts, and they are the same defect:

- `api-snapshot` (`tsrs --api`: createSnapshot, diagnostics, releaseSnapshot): 339 KiB of resident memory per
  cycle over 400 cycles, of which 52 KiB stays *used* in a thread arena and 12 KiB *live* in the heap at exit.
- `lsp-reload` (`tsrs --lsp`: tsconfig.json changes, workspace/didChangeWatchedFiles, diagnostics): 66 KiB per
  cycle (r2 1.00), of which 46 KiB stays used in a thread arena and 17 KiB live in the heap.

Both load a config file: the API from scratch on every createSnapshot (a root snapshot has no config registry), the
language server whenever the tsconfig's text changed. The load parses tsconfig.json into the *current thread's*
leak arena, not into a region: the file's text, its JSON syntax tree, the `CompilerOptions`, the include match
(the file list) and the `ParsedCommandLine` that owns them. The config registry entry holds that command line by
`P`; when the snapshot that made it is released the entry goes, the memory does not, and values in a thread arena
are never dropped, so the heap-allocated members of the command line (the file names, the lazily built maps) are
kept too. The programs themselves are fine: the region log shows one program created and freed per cycle, and
`api-update`, which keeps a base snapshot and therefore its config registry, is flat at 0.0 MiB used after 400
cycles.

Nothing crashed and no answer changed: 0 wrong answers in 2,520 normal and 1,260 poison cycles, with the per-cycle
response contents compared.

## Method

`tools/lsp-mem/lifecycle_leaks.py` (docstring: scenarios, options). Per cycle it samples `ps -o rss` of the server
after the cycle's last response. The first `--warmup` cycles are discarded; over the rest a least-squares line gives
bytes per cycle with its r2, the median of the per-cycle differences gives a second estimate that ignores the
allocator's occasional big steps, and the slope of the last half of the tail separates one-time growth from growth
that continues. The reading column is a first classification; the sections below refine it.

Resident memory counts freed memory the allocator has not returned, so a slope alone does not say what grows. Two
readings separate retained data from retention:

- `--split` runs each scenario twice, for a quarter of the cycles and for all of them, with `TSRS_MEM_SPLIT=1`, and
  divides the difference of the two exit reports (`tsrs_core::memsplit`, docs/DEBUGGING.md "the memory split") by
  the extra cycles: bytes per cycle that stay *used in the thread arenas* (an allocation outside any region; nothing
  frees it before the process ends) and *live in the heap* (reachable malloc blocks), against what is merely
  resident in the heap's pages. The `--lsp` and `--api` servers did not print the exit split; this change adds the
  `exit` report to both (`crates/tsrs_cli/src/main.rs`, the same call the compile path makes).
- `--vmmap` (macOS) attributes the growth between the end of the warmup and the last cycle to the arena reservation,
  other anonymous memory (mimalloc's segments), system malloc and stacks, and reports the physical footprint, which
  leaves out pages marked reusable. Its arena figure is the longest contiguous run of `VM_ALLOCATE` regions and is
  rough (the `api-snapshot` row below shows it shrinking while the exit split shows the arenas growing); the exit
  split is the number to trust.

`--poison` sets `TSRS_ARENA_POISON=1 TSRS_REGION_LOG=1`: freed arena memory stays mapped and poisoned, so resident
memory then grows by what each cycle frees, and that run is for crashes, wrong answers and the region log's
created/freed balance, not for the slope.

Runs: normal mode 400 cycles with 50 warmup (`--split --vmmap`), poison mode 200 cycles with 30 warmup; the watch
scenarios 60/15 and 30/8, because a cycle takes 15 s there (below). Server logs, region logs and raw `vmmap` output
are kept under `--work`.

## Results

Normal mode, 400 cycles (watch: 60), tail after 50 warmup cycles (watch: 15):

| scenario | start | end | RSS/cycle (tail) | last half | median diff | r2 | arena used/cycle | heap live/cycle | reading |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| watch | 60.7 MiB | 57.8 MiB | -201.7 KiB | 593.4 KiB | 160.0 KiB | 0.12 | - | - | noisy, no trend |
| watch-root | 60.5 MiB | 92.2 MiB | 82.7 KiB | 37.6 KiB | 752.0 KiB | 0.19 | - | - | noisy, no trend |
| lsp-edit | 45.9 MiB | 53.3 MiB | 10.6 KiB | 13.4 KiB | 0 B | 0.97 | -350 B | -2.0 KiB | steady |
| lsp-open-close | 9.6 MiB | 47.7 MiB | 2.1 KiB | 1.7 KiB | 0 B | 0.99 | 350 B | 2.0 KiB | steady |
| lsp-reload | 45.6 MiB | 76.3 MiB | 65.7 KiB | 69.5 KiB | 64.0 KiB | 1.00 | 46.4 KiB | 16.7 KiB | **leak** |
| api-snapshot | 8.3 MiB | 234.7 MiB | 338.9 KiB | 137.9 KiB | 80.0 KiB | 0.92 | 52.2 KiB | 11.6 KiB | **leak** |
| api-update | 8.3 MiB | 44.9 MiB | 196 B | 168 B | 0 B | 0.87 | 350 B | 0 B | steady |
| api-build | 8.3 MiB | 69.6 MiB | 46.0 KiB | 66.9 KiB | 0 B | 0.91 | 1.0 KiB | 1.0 KiB | steady (retention) |

Poison mode, 200 cycles (watch: 30), 30 warmup (watch: 8). The slope is what each cycle frees; `regions` is the
region log's count of programs created / freed over the run:

| scenario | start | end | RSS/cycle (tail) | median diff | r2 | wrong answers | regions | reading |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| watch | 60.6 MiB | 63.9 MiB | 364.1 KiB | 640.0 KiB | 0.00 | 0/30 | - | settles |
| watch-root | 60.4 MiB | 197.5 MiB | 2.1 MiB | 5.2 MiB | 0.61 | 0/30 | - | noisy (every module re-checked) |
| lsp-edit | 45.4 MiB | 139.5 MiB | 473.9 KiB | 464.0 KiB | 1.00 | 0/200 | 201 / 200 | one program version freed per edit |
| lsp-open-close | 9.6 MiB | 47.0 MiB | 1.9 KiB | 0 B | 0.96 | 0/200 | 1 / 0 | steady |
| lsp-reload | 45.9 MiB | 131.6 MiB | 419.9 KiB | 416.0 KiB | 1.00 | 0/200 | 201 / 200 | one program freed per reload |
| api-snapshot | 8.3 MiB | 2.2 GiB | 10.6 MiB | 10.3 MiB | 1.00 | 0/200 | 200 / 200 | one program and checker freed per snapshot |
| api-update | 8.3 MiB | 115.9 MiB | 368.4 KiB | 368.0 KiB | 1.00 | 0/200 | 200 / 200 | one program version freed per update |
| api-build | 8.3 MiB | 3.6 GiB | 18.4 MiB | 18.3 MiB | 1.00 | 0/200 | - | one build freed per cycle |

The poison slopes are the expected size of what a cycle frees (a whole program and its checker for a snapshot, a
file version for an edit) and show that freeing happens; a scenario whose normal slope were close to its poison
slope would free nothing. No run crashed, and every response matched the first cycle's.

## The two leaks

### `api-snapshot`: every createSnapshot loads the config into a thread arena

What the measurements say:

- 338.9 KiB/cycle of resident memory over 400 cycles (289-339 KiB across four 400-cycle runs; the median per-cycle
  step is 80-96 KiB). `--no-check` (createSnapshot and release only, no diagnostics) grows the same
  (300.6 KiB/cycle): checking is not involved.
- The region log balances: 200 programs created, 200 freed. The growth is outside the program regions.
- The exit split, by run length (`TSRS_MEM_SPLIT=1`, same binary): thread arenas used 2.6 MiB after 50 cycles,
  10.1 MiB after 200, 20.5 MiB after 400 (52 KiB per cycle, linear); heap live blocks 15.2, 17.6 and 20.3 MiB
  (72 KiB per cycle over the first 80 cycles, then 15 KiB per cycle); heap resident 70, 109 and 134 MiB. So of
  the 339 KiB resident per cycle about 64 KiB is data that is kept and the rest is mimalloc keeping freed pages
  (the physical footprint grows 283 KiB per cycle; `TSRS_MEM_SPLIT=purge` is the handle on that part, not this note).
- The retained arena bytes are the config file's text: with 100 KiB of comment prepended to the synthetic
  tsconfig.json the arena grows 151 KiB per cycle instead of 52 (7.4 MiB after 50 cycles, 29.5 MiB after 200): the
  extra 99 KiB per cycle is the padding, copied into the arena on every load.
- `api-update`, which creates one snapshot and then updates it 400 times (a new program version per cycle, 200
  created / 200 freed in poison mode), ends with 0.0 MiB used in the thread arenas and 2.4 MiB live in the heap at
  50 and at 400 cycles: a snapshot cloned from a base snapshot inherits its config registry and loads nothing.

The code path (`crates/tsrs_api/src/snapshots.rs` `handle_create_snapshot`, `crates/tsrs_project/src`):

1. `handle_create_snapshot` starts from `snapshot_host.new_root_snapshot()`, a snapshot with an empty config
   registry, and clones it with the request's open projects (`SnapshotHost::clone_snapshot`).
2. The clone's `ConfigFileRegistryBuilder` acquires each project's config file. A new entry has
   `pending_reload: PendingReload::Full` (`configfileregistry.rs:97`), so `reload_if_needed`
   (`configfileregistrybuilder.rs:180`) calls `tsoptions::get_parsed_command_line_of_config_file_path`
   (`crates/tsrs_tsoptions/src/tsconfigparsing.rs:3111`) and stores the result with `P::new`
   (`entry.command_line = command_line.map(P::new)`).
3. That function reads the file and parses it with `tsrs_parser::parse_source_file`
   (`new_tsconfig_source_file_from_file_path`, `tsconfigparsing.rs:421`): the text and the JSON syntax tree go to
   the current allocation target. Then `parse_json_source_file_config_file_content` builds the `CompilerOptions`
   (`P`), matches the include patterns against the directory (the file list) and the `ParsedCommandLine`.
4. No region is entered on this path. `Project::create_program` (`project.rs:643-646`) deliberately makes the
   command line *before* entering the program's region ("the command line above is memoized in the project and
   outlives the program, so it is made before"), and nothing else wraps it, so it lands in the thread's own leak
   arena, whose values are never freed or dropped (`crates/tsrs_core/src/arena.rs:150`).
5. releaseSnapshot drops `SnapshotData`, the snapshot's reference count reaches zero and the registry with its
   entries is dropped; the `P<ParsedCommandLine>` is a handle, so the `ParsedCommandLine` is not. Its heap-owned
   members (`parsedoptions.rs` `file_names: Vec<String>`, `parsedcommandline.rs` `wildcard_directories`,
   `include_globs`, `file_names_by_path`, `errors`) are the 12-17 KiB per cycle that stay live in the heap.

What is kept per createSnapshot, for this project: the tsconfig.json text (1:1 with the file), its syntax tree, the
compiler options, 105 file names (as the file list and, once `file_names_by_path` has been asked for, as a map) and
the command line struct: about 52 KiB in the arena and 12 KiB in the heap. A project with a large include match or
a long tsconfig keeps correspondingly more per snapshot.

### `lsp-reload`: a tsconfig change loads the config into a worker's arena

What the measurements say:

- 65.7 KiB/cycle, r2 1.00, median step 64 KiB (66-79 KiB across four runs); the exit split attributes 46.4 KiB per
  cycle to the thread arenas and 16.7 KiB to live heap blocks, 14.3 KiB to resident heap pages; `vmmap` sees the
  arena reservation's dirty pages grow 47.5 KiB per cycle and nothing else of note.
- The same server with the same edits but no config change (`lsp-edit`, one didChange per cycle, a program version
  created and freed per cycle) keeps nothing: arena used and heap live per cycle are both at the noise floor
  (-350 B, -2.0 KiB); `lsp-open-close` likewise. The region log balances in both (201 created / 200 freed, the
  extra one is the initial program).

The code path:

1. The cycle writes tsconfig.json (a comment toggles, so the text changes) and sends
   `workspace/didChangeWatchedFiles`. `ConfigFileRegistryBuilder::did_change_files` sees a changed config file
   and `handle_config_change` (`configfileregistrybuilder.rs:837`) marks its entry `PendingReload::Full`.
   (The excessive-changes path, `invalidate_cache`, compares the new text with the entry's
   `config_file.source_file` text first and keeps the load when it is unchanged.)
2. `Session::schedule_snapshot_update` (`session.rs:648`) enqueues the update on the background queue; the worker
   runs `update_snapshot`, which clones the snapshot, and the clone's builder runs `reload_if_needed` for the dirty
   entry: steps 2-4 above, in that worker thread's arena.
3. `reload_if_needed` keeps `old_command_line` only to update the extended-config bookkeeping
   (`update_extending_configs`); the old load's memory stays in the arena it was made in.

So the API and the language server share one defect with two triggers: the API triggers it on every createSnapshot
(each is a root clone), the language server on every tsconfig change (a plain edit reuses the entry). A long
editing session that touches tsconfig.json or a `files`/`include` list keeps every version of the config it has
seen; an API client that creates a snapshot per request keeps every request's.

### What is not a leak

- `api-build` (createBuildOrchestrator, build, dispose): 46 KiB/cycle of resident memory with r2 0.91 but a median
  step of 0 B, 1.0 KiB per cycle used in the arenas and 1.0 KiB live in the heap; a 600-cycle run earlier the same
  day fit 13.8 KiB/cycle with 4.3 KiB over its last half. Allocator retention that slows down, not data.
- `lsp-edit`: 10.6 KiB/cycle of resident memory, median 0 B, nothing used or live per cycle; `vmmap` puts it in
  the arena reservation's dirty pages (6.9 KiB per cycle): freed program regions' chunks the arena keeps for reuse.
- `watch` and `watch-root` (`tsrs --watch`): no trend in either mode. The normal `watch` run's slope is negative
  with r2 0.12 and its poison run settles; `watch-root`, which re-checks every module per cycle, swings by several
  MiB from cycle to cycle (median step 752 KiB, r2 0.19, 37.6 KiB/cycle over the last half), the allocator's
  reuse of what a full re-check frees. A caveat on the sample size: a cycle takes 13-15 s on this machine, from
  the edit to "Found 0 errors", although the debouncer's constants are 50/500 ms and its settle 1-2 s
  (`crates/tsrs_fswatch/src/debounce.rs`), so the watch runs are 60 and 30 cycles; the latency is not explained
  here and may be the FSEvents stream or the directory under `~/.t3`.

## Overlap with open work

- #638 (own non-arena runtime allocations) replaces `Box::leak` of hosts and the protocol strings; it does not
  touch where a loaded config lives. Its `main.rs` hunk (the compile path's system object) is next to but disjoint
  from this change's `exit` reports.
- #655 / #656: standalone `resolveModuleName` in the API server allocates a resolver and its caches in the request
  thread's arena on every call; #656 routes the request through a scratch region that is freed once the response
  is built. The config load is the same class of defect on another path, but a scratch region does not fit it: the
  command line outlives the request (programs hold it, registry clones share the entry), so it needs an owned region
  with the entry's lifetime, not the request's.

## Proposed fix (not in this change)

Give each config file entry's load a region of its own: enter a `Region` around the
`get_parsed_command_line_of_config_file_path` call in `reload_if_needed` (and the `FileNames` reload), keep the
region handle in `configFileEntry` next to `command_line`, and let the registry clones that share the entry share
the handle, so the last snapshot that refers to the entry frees the load, and the region's drop list releases the
command line's heap members with it. Two things to check: a program holds its command line (`project.rs:643`), so
the program's region must keep the config region alive the way `memregions.rs` keeps the regions of its files
(`Region::containing_all`), and the extended-config cache must not keep a `P` into a freed load. The tool's
`lsp-reload` and `api-snapshot` rows are the regression check: both should read `steady` with arena used per cycle
at the noise floor.

## Draft issue

One issue, since one fix covers both entry points.

**Title:** `--api` createSnapshot and `--lsp` tsconfig reloads keep every loaded config file in a thread arena

**Body:**

> ### Summary
>
> Loading a tsconfig.json for a snapshot allocates the parsed config (its text, JSON syntax tree, compiler options,
> file list and `ParsedCommandLine`) in the current thread's leak arena, outside any region, and nothing frees it
> when the snapshot that loaded it is released. The API server does this on every `createSnapshot` (a root
> snapshot has no config registry, so each one loads the config again); the language server does it on every
> change to a tsconfig.json's text. Each load keeps about 50 KiB in the arena plus about 15 KiB of heap for a
> 40-module project with a 10-line tsconfig, scaling with the config's text and the size of the include match.
> Programs are not affected (one created and freed per cycle under `TSRS_REGION_LOG=1`).
>
> ### Versions
>
> main 357d1d3 (2026-10-21), release build, macOS 26.6 on Apple Silicon.
>
> ### Reproduction
>
> ```sh
> cargo build --release -p tsrs_cli
> python3 tools/lsp-mem/lifecycle_leaks.py --scenario api-snapshot --scenario lsp-reload --scenario api-update \
>     --cycles 400 --warmup 50 --split
> ```
>
> | scenario | RSS/cycle | arena used/cycle | heap live/cycle |
> | --- | --- | --- | --- |
> | api-snapshot | 338.9 KiB | 52.2 KiB | 11.6 KiB |
> | lsp-reload | 65.7 KiB | 46.4 KiB | 16.7 KiB |
> | api-update (keeps a base snapshot) | 196 B | 350 B | 0 B |
>
> Prepending 100 KiB of comment to the tsconfig raises api-snapshot's arena growth from 52 to 151 KiB per cycle:
> the config text is copied into the arena on every load.
>
> ### Where
>
> `ConfigFileRegistryBuilder::reload_if_needed` (`crates/tsrs_project/src/configfileregistrybuilder.rs`) calls
> `tsoptions::get_parsed_command_line_of_config_file_path`, which parses the file with
> `tsrs_parser::parse_source_file` and builds the command line, and stores the result with `P::new`. No region is
> entered on that path (`Project::create_program` makes the command line before the program's region on purpose,
> since it outlives the program). The API reaches it from `handle_create_snapshot` through `new_root_snapshot` and
> `clone_snapshot`; the language server from `did_change_files` (`PendingReload::Full`) through the scheduled
> snapshot update. Values in a thread arena are never dropped, so the command line's `Vec<String>` file names and
> maps leak with it.
>
> ### Expected
>
> The Go server lets the GC collect a `ParsedCommandLine` once no snapshot refers to it. In tsrs the load should
> live in a region owned by the config file entry and shared by the registry clones that keep the entry, freed with
> the last of them. Same class as #655 (per-request arena allocations in the API), different lifetime: the config
> outlives the request, so a scratch region is not enough.
>
> Measurements and the tool: notes/lifecycle-leaks.md.

## Open questions

- The heap's live blocks grow 72 KiB per cycle over the first 80 `api-snapshot` cycles and 15 KiB per cycle from
  100 to 400. The steady part matches the command line's heap members; the early part (about 5 MiB) is not
  attributed and may be caches filling (the overlay file system's handles, the API session's maps).
- The 13-15 s watch latency on this machine (above). It bounds the watch sample size and is worth a look of its own.
- Linux was not measured. `vmmap` is macOS only; the exit split and the RSS slope work there unchanged.

## Reproduce

```sh
cargo build --release -p tsrs_cli
# the table above (about 3 minutes for the six fast scenarios; the watch scenarios take 15 s per cycle)
python3 tools/lsp-mem/lifecycle_leaks.py --scenario lsp-edit --scenario lsp-open-close --scenario lsp-reload \
    --scenario api-snapshot --scenario api-update --scenario api-build --cycles 400 --warmup 50 --split --vmmap
python3 tools/lsp-mem/lifecycle_leaks.py --scenario watch --scenario watch-root --cycles 60 --warmup 15
# poison: crashes, wrong answers, region balance
python3 tools/lsp-mem/lifecycle_leaks.py --cycles 200 --warmup 30 --poison
# the padding experiment: --project with a tsconfig.json that has 100 KiB of comment prepended, --env TSRS_MEM_SPLIT=1
```
