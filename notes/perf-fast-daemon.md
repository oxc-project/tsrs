# perf-fast-daemon: a resident check server for repeated `--fast` runs (`--daemon`)

Git hooks and agents type-check the same project again and again, and every run rebuilds the front end: read, parse
and bind every file, resolve every import. `tsrs -p <dir> --noEmit --fast --daemon` (or `TSRS_DAEMON=1`) sends the
command to a resident server that keeps the parsed files of its last run; the design and the exactness argument are
in `crates/tsrs_execute/src/daemon/mod.rs`. Earlier notes ruled a daemon out without measuring it
(notes/perf-round3.md, "holds several GiB per checkout"); `--fast` now accepts memory for speed, so this measures it.

Verdict: a rerun with no change answers in 9-50 ms instead of 0.9-2.2 s (94-99% less wall), byte-identical. A rerun
after an edit is 4-10% faster than a cold run on vscode and supabase-studio and 2-5% slower on mui-docs and
t3code-server on Linux, 8-19% faster on three of the four on macOS. The first request costs what a cold run does.
The server holds 0.5-1.4 GB once idle, growing 25-60 MiB per rerun. The no-change case is also what a result cache
without a server gets (branch `feat/fast-result-cache`, about the same replay time), so the server earns its
process only on edits, where it is weak on Linux.

## Design decisions

- Staleness by asking every recorded file system query again (a stamp for reads and listings; a hash when the stamp
  changed or was taken within 2 s of the file's newest time, git's rule for coarse clocks) rather than the watcher
  (`tsrs_fswatch`): exact by construction, cheap enough (tables below), no thread, and none of a watcher's failure
  modes at request time. FSEvents delivers 11-13 ms after a write (notes/watch-macos.md), so a hook that runs right
  after a save could see a stale tree; events coalesce and overflow; inotify has watch limits; every failed module
  lookup would need a watch.
- Every request rebuilds the program with fresh checkers rather than reusing the last one (`Program::reuse_program`,
  watch mode's fast path). Reuse would skip module resolution (50-170 ms, ~5-15% of an edit), but a reused program
  inherits the old pool's checker assignment (`inherited_associations`) and fast mode is not assignment-independent
  everywhere (notes/open-history-dependence.md), so the output would no longer be the cold run's in those cases.
- The client waits for the server it starts. Running the command in-process beside a warming server costs the first
  run +31% (t3code-server, 2.34 -> 3.07 s), +49% (mui-docs), +84% (supabase-studio) and +96% (vscode, 1.19 ->
  2.33 s) on 16 vCPUs.

## Results

### Linux (m8g.4xlarge, Graviton4, 16 vCPU)

`runner.mjs` (aws-bench-box), 10 alternating pairs per row on one box: arm A is a cold `--fast` run of the candidate,
arm B the `--daemon` request on the same tree; wall time of the client process, flags `-p <dir> --noEmit
--incremental false --pretty false --fast`. "First" stops the server before each B run; "no change" reruns; "leaf"
and "hub" edit a file nothing imports and a file many files import (an editor-style save: write a temporary file,
rename it over) before each run of either arm.

| project | scenario | cold run (A) | `--daemon` (B) | B vs A, median of pairs [95%] |
|---|---|---:|---:|---:|
| vscode | first request | 1.21 s | 1.19 s | −1.7% [−2.8%, −0.7%] |
| vscode | no change | 1.21 s | 0.04 s | −96.6% [−97.1%, −96.5%] |
| vscode | leaf edit | 1.21 s | 1.09 s | −9.2% [−10.7%, −8.4%] |
| vscode | hub edit | 1.21 s | 1.10 s | −9.6% [−10.3%, −9.3%] |
| mui-docs | first request | 1.26 s | 1.25 s | −1.1% [−2.6%, +0.4%] |
| mui-docs | no change | 1.26 s | 0.05 s | −95.7% [−96.1%, −95.2%] |
| mui-docs | leaf edit | 1.26 s | 1.29 s | +2.3% [+1.2%, +4.6%] |
| mui-docs | hub edit | 1.25 s | 1.30 s | +4.0% [+2.8%, +4.5%] |
| t3code-server | first request | 2.24 s | 2.23 s | −1.3% [−5.0%, +5.3%] |
| t3code-server | no change | 2.14 s | 0.03 s | −98.6% [−98.6%, −98.4%] |
| t3code-server | leaf edit | 2.10 s | 2.20 s | +5.4% [+1.0%, +8.3%] |
| t3code-server | hub edit | 2.06 s | 2.16 s | +4.1% [+1.0%, +7.3%] |
| supabase-studio | first request | 0.94 s | 0.91 s | −2.6% [−3.8%, −1.2%] |
| supabase-studio | no change | 0.93 s | 0.05 s | −94.4% [−95.0%, −94.2%] |
| supabase-studio | leaf edit | 0.92 s | 0.88 s | −5.2% [−6.4%, −3.7%] |
| supabase-studio | hub edit | 0.92 s | 0.88 s | −4.2% [−6.2%, −2.2%] |

The runner wraps each run in `perf stat` and `/usr/bin/time` (~20 ms); a no-change request timed bare is 9-33 ms
(table below). The first request includes starting the server; it is no slower than a cold run, as the server's run
frees no leaf files and the client's own start is small.

Without `--daemon` nothing changes: candidate against base `48cf809ef`, the same pairs:

| project | CPU | wall | instructions | peak RSS | single-threaded instructions (3 pairs) |
|---|---:|---:|---:|---:|---:|
| vscode | +0.0% [−0.2%, +0.4%] | −0.2% [−0.3%, +0.2%] | +0.1% [−0.2%, +0.1%] | +0.1% [−0.1%, +0.2%] | −0.0% (74.57 G) |
| mui-docs | −0.3% [−0.8%, +0.3%] | +0.1% [−0.6%, +1.8%] | +0.1% [−0.2%, +0.5%] | −0.0% [−0.2%, +0.6%] | −0.0% (29.85 G) |
| t3code-server | −0.6% [−1.7%, +0.1%] | −1.1% [−5.4%, +1.7%] | −0.3% [−0.8%, +0.3%] | −0.4% [−1.1%, +0.5%] | +0.0% (37.14 G) |
| supabase-studio | −0.0% [−0.4%, +0.6%] | −0.4% [−1.1%, +0.3%] | +0.1% [−0.1%, +0.3%] | +0.1% [−0.2%, +0.2%] | +0.0% (37.07 G) |

The server's memory over a session on the same box (`/proc/<pid>/status`; resident once the request's program is
freed, and the peak so far), with the request's wall time:

| project | cold run (alone) | first request | resident after it | no-change rerun | hub edit 1 / 8 | resident after edit 1 / 8 | peak after 8 edits (cold run's peak) |
|---|---:|---:|---:|---:|---:|---:|---:|
| vscode | 1.19-1.21 s | 1.11 s | 1424 MiB | 13 ms | 1.02 / 1.08 s | 1566 / 1740 MiB | 2795 MiB (1988 MiB) |
| mui-docs | 1.24-1.26 s | 1.21 s | 514 MiB | 33 ms | 1.18 / 1.28 s | 648 / 886 MiB | 2148 MiB (1506 MiB) |
| t3code-server | 2.32-2.36 s | 2.16 s | 599 MiB | 9 ms | 2.28 / 2.34 s | 661 / 804 MiB | 3029 MiB (2005 MiB) |
| supabase-studio | 0.93-0.95 s | 0.90 s | 802 MiB | 32 ms | 0.87 / 0.91 s | 971 / 1253 MiB | 2263 MiB (1603 MiB) |

Timed with `date` around the client in a shell (no runner wrappers, so lower than the runner's times above). The
resident memory rises 25-60 MiB per rerun; the server's CPU per request rises with it for the first two or three edits
(mui-docs 11.1 -> 12.1 s, t3code-server 20.6 -> 22.3 s, vscode 14.7 -> 15.7 s) and then holds, so later edit
requests lose most of their margin over a cold run on Linux.

### macOS (M5 Max, 18 cores, shared machine, load 10-55)

A local script with the same scenarios, 2 x 10 interleaved pairs (the load varied a lot, so the ranges are wide; the
medians of the paired ratios are the numbers to read):

| project | scenario | cold median | daemon median | daemon vs cold (median of pairs) | [min, max] of pairs |
|---|---|---:|---:|---:|---:|
| vscode | first | 916 ms | 967 ms | -2.9% | [-32.7%, +45.6%] n=20 |
| vscode | nochange | 825 ms | 33 ms | -96.1% | [-97.2%, -93.5%] n=20 |
| vscode | leaf | 913 ms | 770 ms | -18.9% | [-44.0%, +60.1%] n=20 |
| vscode | hub | 932 ms | 762 ms | -19.5% | [-49.6%, +6.6%] n=20 |
| mui-docs | first | 1535 ms | 1378 ms | -4.9% | [-37.4%, +25.0%] n=20 |
| mui-docs | nochange | 1131 ms | 95 ms | -91.0% | [-93.9%, -81.5%] n=20 |
| mui-docs | leaf | 1191 ms | 963 ms | -16.1% | [-53.9%, +29.8%] n=20 |
| mui-docs | hub | 1114 ms | 1086 ms | -14.3% | [-41.8%, +20.0%] n=20 |
| t3code-server | first | 1729 ms | 1556 ms | +0.9% | [-30.5%, +84.9%] n=20 |
| t3code-server | nochange | 1815 ms | 26 ms | -98.0% | [-99.6%, -84.2%] n=20 |
| t3code-server | leaf | 1834 ms | 1701 ms | +2.7% | [-29.3%, +50.6%] n=20 |
| t3code-server | hub | 1558 ms | 1504 ms | -1.5% | [-24.5%, +52.2%] n=20 |
| supabase-studio | first | 921 ms | 930 ms | -3.9% | [-32.1%, +87.7%] n=20 |
| supabase-studio | nochange | 1105 ms | 75 ms | -90.7% | [-94.9%, -55.2%] n=20 |
| supabase-studio | leaf | 836 ms | 750 ms | -8.8% | [-35.2%, +12.6%] n=20 |
| supabase-studio | hub | 796 ms | 759 ms | -7.8% | [-54.9%, +56.6%] n=20 |

The server's footprint on macOS after 34 requests (24 of them reruns after an edit): vscode 1.45 -> 2.20 GB, mui-docs
0.53 -> 1.42 GB, t3code-server 0.63 -> 1.22 GB.

### Exactness

Output (stdout bytes) and exit status of the `--daemon` request equal a cold run of the same binary on all 17 prepared
benchmark projects, at each of: first request, rerun, after appending a type error to a source file of the program,
after reverting it, after adding a file next to it, after deleting that file (98 comparisons; webpack has no
TypeScript source to edit). Every paired run above compared its output too: all identical. The CLI tests are
`crates/tsrs_cli/tests/integration/daemon.rs`.

Node and symbol ids differ from a cold run's: ids of kept files were assigned by an earlier request, and new ones keep
growing. A `--fast` run already numbers them differently every time (checkers take ids in parallel), and the
determinism gate holds its output to the program alone. One way an id's value is visible: a unique symbol's internal
property name embeds its symbol id, and the node builder counts that name's length toward type truncation, so a type
printed right at the truncation limit could print differently once ids gain a digit. No comparison above hit it.

## Measured and rejected

- **Edits do not reliably beat a cold run on Linux.** vscode -9%, supabase-studio -5%, but mui-docs +2-4% and
  t3code-server +4-5% (table above). An edit still pays every module resolution (the loader walks the import graph
  again; with every file cached that is 50-170 ms of a vscode, mui-docs or supabase-studio request, CPU-bound and
  poorly parallel) and the whole check, 75-95% of a cold run, redone with fresh checkers. What the server skips is
  reading, parsing and binding, which is small on Linux. On macOS reading files is slower (`open` is capped near
  110k/s by the endpoint security client), and the server's edits were 8-19% faster on vscode, mui-docs and
  supabase-studio (t3code-server: within noise).
- **Later requests run slower than the first.** Over a session of hub edits on mui-docs the request's user cycles rose
  from 28.7 G to 31.1 G (+8%) for +1.5% instructions, with dTLB misses 352 M -> 425 M (+21%, system-wide `perf stat`
  around each request); it steps up at the third edit and then holds. The server's huge pages did not drop
  (AnonHugePages 311-326 MiB mid-request). Not found; the likeliest candidate is the checkers' id-indexed link stores
  (links.rs `IdLinkStore`): the kept files' ids were handed out by the first request and new ids keep growing, so each
  checker's dense index spans an ever wider id range, touched sparsely. This, not the first edit, is what the paired
  runs above measure (each pair follows a priming request).
- **Checker regions in 4 KiB pages cost 5-10% of the check.** Freeing each request's checkers through regions, as
  watch mode does, made edit runs slower than cold runs on mui-docs (+3%) and t3code-server (+7-10%): a region grew
  in 4 KiB-page chunks, where a cold run's checkers use the thread arenas' 2 MiB huge-page chunks. A t3code-server
  edit request took 405-480k minor faults and 0.7-0.8 s of system time with checker regions, 97-102k and 0.25 s
  without them (`/proc/<pid>/stat`); paired hub edits without checker regions were mui-docs -6.1%, t3code-server
  -2.8%, vscode -15.2%, supabase-studio -8.4% against cold runs. A region chunk of 2 MiB or more is now whole
  huge-page blocks on Linux, as a thread arena's are (tsrs_core arena.rs; the CLI's retired file regions excluded, so
  cold runs do not change). A mui-docs edit request then took 74-185k faults (137k without checker regions), and the
  paired hub edits went from +7.0% to +2.1% on t3code-server and from -10.1% to -12.2% on vscode. Watch mode, `-b`
  and `--maxMemory` use the same regions and get the same pages; not measured there.
- **Per-file regions for the kept files** (watch mode's watchmem.rs) changed nothing measurable against parsing into
  the thread arenas (hub edits within 1% on all four projects); the files go to the thread arenas, as in a cold run,
  and an edited file's old version is not freed.
- **Allocator retention.** Without `mi_collect` after each request, the server's resident memory on Linux rose
  ~100-150 MiB per request (freed checker memory kept by mimalloc), enough to trip the memory limit within a few
  edits. With it, 25-60 MiB per request remains (mostly the heap; the arena grows 4-17 MiB per request: old file
  versions and thread-arena data). Not found; the memory limit restarts the server when it doubles.
- **Not tried:** checking speculatively when a watcher sees a change, so that the request finds its answer ready
  (exact with the same validation, but it spends 16 cores on every save); a fork per request (the checkers die with
  the child, ids never grow), as the rayon pools do not survive `fork` (notes/perf-checker-processes.md forked from a
  checker thread with the pools idle and avoided them in the child; a whole check needs them).

## Overlap

The result cache on branch `feat/fast-result-cache` validates the same way and replays a no-change run from a file
without a server: its replays measured 26-42 ms on the same instance type, the server's 30-50 ms through the same
runner. If both land, the server is only worth its process and memory for edits. The source cache in `tsrs_vfs` on
branch `perf/fast-source-cache` would take the file reads out of a cold run, which is most of what the server saves
on macOS.
