# perf-exit-teardown: free the memory after the caller has seen the exit (Linux, `--fast`)

When the CLI finishes a check, the kernel tears the process's address space down inside `exit_group`, on one core,
before whoever started the process sees it exit. On large programs that is 2-5% of the wall time. Since this change,
a fast-mode batch compile on Linux hands its address space to a helper process just before it exits: the process's
own exit drops only a reference to the memory and returns at once, and the kernel frees the memory in the helper's
exit, after the caller's `wait` has returned. The code is `crates/tsrs_cli/src/exit_helper.rs`;
`TSRS_EXIT_HELPER=0` turns it off.

## The cost

Measured on main 221ee7bf, m8g.4xlarge (Graviton4, 16 vCPUs, kernel 7.0) pinned to 8 cores
(`taskset -c 0-7`), `-p <project> --noEmit --incremental false --pretty false --fast`, with
`strace -f -ttt -e trace=exit_group` (the time from the main process's `exit_group` to its `+++ exited`), median of
three:

| project | teardown |
| --- | ---: |
| a large private monorepo | about 2% of wall time |
| vscode | 71 ms |
| mui-docs | 75 ms |
| supabase-studio | 71 ms |
| cal-diy | 68 ms |
| mikro-orm | 62 ms |
| t3code-server | 60 ms |
| webpack | 16 ms |
| xstate-main | 9 ms |

The arenas are on huge pages and free quickly; the cost is the mimalloc heap's 4 KiB pages. With
`MIMALLOC_ALLOW_THP=1` the private monorepo's teardown drops to about a ninth, but huge pages on the heap cost
25-33% peak RSS (`notes/mem-no-thp.md`), so that is not an option.

An earlier look (`notes/perf-front-end-fixed-costs.md`, "Tried, not landed") measured about 15 ms on a small
project and tried to free the memory faster before the exit: an arena `munmap` cost 3 ms and moved the teardown
within noise, and `madvise` from 16 threads was slower than one `munmap`. Two things are different here: the
teardown was never measured on large programs, where it is 2-5% of wall time, and this change does not free memory
faster; it takes the freeing off the path the caller waits on.

## Mechanism

`main` calls `exit_helper::at_exit` after the compile thread has joined and stdout and stderr are flushed, unless a
watch loop ran (`tsrs_execute::watchmanager::ran_watch_loop`). `--lsp`, `--api` and `--lint` return earlier and
never reach it; the Node API and wasm do not use this binary. In fast mode, above 32 MiB resident, not as PID 1 and
not with `TSRS_EXIT_HELPER=0`, `hand_off`:

1. Creates two `O_CLOEXEC` pipes, `wake` and `ready`.
2. Blocks every signal on the calling (main) thread with the raw `rt_sigprocmask` (libc's `sigfillset` and
   `pthread_sigmask` leave glibc's internal signals out), so the helper starts with all of them blocked.
3. Calls `clone(CLONE_VM | SIGCHLD)` in inline assembly, with a 16 KiB static stack. The child starts on that stack
   and calls `helper(wake[0])`, which never returns. The parent restores its signal mask and closes its copies of
   `wake[0]` and `ready[1]`.
4. Waits (`poll`, 1 s at most) until `ready[0]` reports end of file: the helper has closed all its descriptors.
5. Returns; `main` returns its exit code as before. `wake[1]` stays open.

The helper (`helper`) makes only raw system calls: `close_range` below and above `wake[0]`, which closes its copies
of stdout, stderr, `ready[1]` and everything else, then `read(wake[0])` until it returns anything but `EINTR`, then
`exit_group(0)`. The only write end of `wake` is in the main process's descriptor table. In `do_exit` the kernel runs
`exit_mm` before `exit_files`, and the table is released when the last thread passes `exit_files`, so the helper
wakes only after every thread of the main process has dropped its reference to the memory. Its `exit_group` then
drops the last one and the kernel tears the memory down there. The main process is reaped by its caller as usual and
the helper by init.

Seen from the outside (strace on vscode at 8 cores): the main process's `exit_group` to its exit takes 0.2 ms
instead of 71 ms, and the helper's own exit takes 76 ms. In the traces read line by line (a debug build on
webpack, the musl build below) the helper's `exit_group` follows the main process's `+++ exited` by 4 µs.

### Why each piece is there

- **Inline assembly, not libc's `clone`.** The helper shares the address space and, without `CLONE_SETTLS`, the
  calling thread's thread pointer. Any libc call in it can write `errno` or other thread-locals of the main thread,
  and glibc before 2.25 had its `clone` wrapper write the cached pid into the caller's thread control block in a
  `CLONE_VM` child. A raw `syscall`/`svc` in the child path touches nothing but registers and the new stack. The
  helper's code is a Rust function that uses only stack locals and raw system calls, and nothing in it can panic (the
  fd arithmetic is `wrapping_*`), so no panic machinery or allocation can run in it. It never runs atexit handlers or
  flushes stdio: it ends with a raw `exit_group`. x86_64 and aarch64 are implemented, which covers the four Linux
  release targets (glibc and musl); other architectures compile the no-op.
- **The kernel writes nothing into the shared memory for the helper.** Without `CLONE_CHILD_SETTID`,
  `CLONE_CHILD_CLEARTID` or `CLONE_SETTLS` there is no tid address; the child starts with no robust-futex list and,
  with `CLONE_VM`, no rseq registration and no alternate signal stack.
- **Signals blocked.** The helper gets a copy of the main process's handlers (no `CLONE_SIGHAND`). Blocked, none of
  them can run on its stack with the main thread's thread pointer. SIGKILL and SIGSTOP still work.
- **The pipe, not `PR_SET_PDEATHSIG`.** The parent-death signal follows the thread that called `clone` and fires in
  that thread's `exit_notify`, while other threads of the group may still be before their `exit_mm`; it also needs a
  `getppid` check for the race where the parent dies before the `prctl`. End of file on a pipe whose only write end is
  in the main process's descriptor table happens exactly once every thread is past `exit_mm`, whatever ends the
  process: `exit`, a fatal signal or SIGKILL (checked below).
- **The `ready` handshake.** Without it, a pipe on stdout (`tsrs | tee log`, Node's `spawnSync`) would stay open
  until the helper got to run its `close_range`. With it, the helper has closed every descriptor before `hand_off`
  returns, so the pipes close with the main process.
- **`SIGCHLD` as the exit signal.** init's `wait` reaps only children that report with SIGCHLD (`__WCLONE` children are
  skipped). A reparented child gets SIGCHLD anyway, and if the helper exits first the main process ignores it.
- **PID 1 skipped.** When the CLI is the init of a PID namespace (`docker run image tsrs ...` without `--init`), its
  exit waits until every process in the namespace has exited, the helper included: nothing to gain.

## Accounting

Every checker thread ran in the main process, so `wait4`, `/usr/bin/time` and hyperfine report its user time, its
system time up to the exit and its peak RSS as before. Peak RSS comes from the memory's high-water mark, taken when
the last thread exits, and is the same memory. What moves is the teardown's system time: it is now the helper's, and
init reaps the helper, so the caller's CPU time goes down by about the teardown. That is work moved, not saved; it
still takes a core for that long, after the caller has moved on. `perf stat` follows the helper (counters are
inherited across `clone`), but the teardown is kernel work, which `instructions:u` does not count either way; the
counts below are the same. mold gets the same wall-time effect by forking at start-up
and doing the work in the child, which hides the child's CPU time and memory from the caller; this does not.

## Results

One m8g.4xlarge (16 Graviton4 vCPUs, kernel 7.0), release builds of main 221ee7bf and of this change built the same
way, `-p <project> --noEmit --incremental false --pretty false --fast` under `taskset`, 15 alternating pairs per
project with the order flipped every pair (wall time from outside the process, CPU time as the caller's `times`
reports it, `perf stat -e instructions:u`, peak RSS from `/usr/bin/time`). Before every run an untimed wait until
no `tsrs` process is left, so a helper still freeing the previous run's memory never shares the cores with the next
run. Differences are medians of the paired differences; `*` marks a ~95% interval that excludes 0. The release
binary of the final commit has the same `.text` section as the measured one (the changes after it are comments, docs
and the sanitizer `cfg`).

### The helper alone: one binary, `TSRS_EXIT_HELPER=0` against `1`

8 cores (`taskset -c 0-7`):

| project | wall, helper off | wall, helper on | difference (95% CI) | wall | CPU | peak RSS | instructions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 2444 ms | 2328 ms | -95 ms (-120 to -64) | -3.9%* | -1.2%* | 0.0% | -0.0% |
| mui-docs | 1673 ms | 1657 ms | -64 ms (-113 to -35) | -3.9%* | -1.0% | 0.1% | -0.1% |
| supabase-studio | 1511 ms | 1449 ms | -55 ms (-76 to -40) | -3.7%* | -0.0% | -0.0% | 0.0% |
| t3code-server | 2886 ms | 2824 ms | -93 ms (-157 to 64) | -3.1% | -0.7% | 0.7% | 0.0% |
| webpack | 339 ms | 327 ms | -13 ms (-30 to 2) | -4.1% | -0.7% | 0.0% | 0.0% |
| mikro-orm | 1429 ms | 1361 ms | -54 ms (-73 to -27) | -3.8%* | -0.7% | -0.4% | 0.0% |
| cal-diy | 1410 ms | 1330 ms | -88 ms (-111 to -67) | -6.2%* | -1.6%* | -0.1% | -0.2% |
| xstate-main | 248 ms | 230 ms | -14 ms (-24 to -6) | -5.7%* | -2.3% | -0.6% | 0.2% |
| vscode, stdout piped to `cat` | 2391 ms | 2313 ms | -51 ms (-101 to -16) | -2.4%* | 0.4% | -0.0% | -0.0% |

4 cores (`taskset -c 0-3`):

| project | wall, helper off | wall, helper on | difference (95% CI) | wall | CPU | peak RSS | instructions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 4393 ms | 4370 ms | -28 ms (-62 to -12) | -0.7%* | 0.2% | -0.0% | -0.0% |
| mui-docs | 2030 ms | 1994 ms | -49 ms (-100 to -39) | -2.8%* | -1.1%* | -0.1% | -0.0% |
| supabase-studio | 2493 ms | 2419 ms | -49 ms (-82 to -32) | -1.9%* | -0.8% | -0.2% | -0.0% |
| t3code-server | 3247 ms | 3153 ms | -46 ms (-87 to 30) | -1.7% | -0.1% | -0.1% | 0.0% |
| webpack | 541 ms | 545 ms | -6 ms (-28 to 4) | -1.0% | -0.9% | -0.2% | -0.0% |
| mikro-orm | 2381 ms | 2334 ms | -47 ms (-71 to 11) | -1.9% | -0.4% | 0.1% | 0.0% |
| cal-diy | 2039 ms | 1946 ms | -73 ms (-131 to -6) | -4.2%* | -1.1% | -0.4% | -0.8% |
| xstate-main | 341 ms | 342 ms | -6 ms (-11 to 3) | -1.8% | 0.2% | -0.5% | -0.1% |

16 cores:

| project | wall, helper off | wall, helper on | difference (95% CI) | wall | CPU | peak RSS | instructions |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 1409 ms | 1302 ms | -101 ms (-137 to -61) | -7.2%* | -1.8% | 0.0% | -0.0% |
| mui-docs | 1344 ms | 1245 ms | -81 ms (-110 to -54) | -6.8%* | -1.6% | 0.4%* | 0.1% |
| supabase-studio | 1093 ms | 1007 ms | -89 ms (-115 to -56) | -8.0%* | -0.1% | 0.1% | 0.1% |
| t3code-server | 2547 ms | 2485 ms | -113 ms (-200 to -5) | -4.5%* | -0.0% | -0.0% | 0.3% |
| webpack | 249 ms | 226 ms | -17 ms (-31 to -8) | -6.8%* | -1.3% | -0.4% | -0.3% |
| mikro-orm | 906 ms | 815 ms | -72 ms (-92 to -59) | -7.6%* | -0.2% | 0.8% | 0.7% |
| cal-diy | 1055 ms | 963 ms | -100 ms (-128 to -62) | -9.7%* | -1.3% | 0.2% | 0.5% |
| xstate-main | 230 ms | 222 ms | -9 ms (-12 to -6) | -3.8%* | 0.0% | -0.1% | 0.1% |

Wall time falls on every project at every core count, significantly on 4, 6 and 8 of the 8 at 4, 8 and 16 cores. At 8
and 16 cores it falls by about the teardown or more (vscode at 8 cores: 95 ms against a 71-76 ms teardown; at 16
cores peak RSS, and likely the teardown, is 8-37% larger); at 4 cores by less, 0.7-4%. CPU time falls by up to 2%;
peak RSS and instructions do not move. The piped row is timed until the whole pipeline has ended: the pipe closes
with the main process.

What the caller's `wait4` reports, from GNU time (`%e %U %S %M`), 7 alternating runs, medians, 8 cores:

| project | arm | wall s | user s | system s | peak RSS MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| vscode | main | 2.35 | 16.87 | 0.68 | 1840 |
| vscode | helper off | 2.39 | 17.21 | 0.67 | 1836 |
| vscode | helper on | 2.31 | 17.17 | 0.61 | 1839 |
| supabase-studio | main | 1.49 | 9.99 | 0.56 | 1313 |
| supabase-studio | helper off | 1.48 | 9.96 | 0.55 | 1310 |
| supabase-studio | helper on | 1.42 | 10.05 | 0.48 | 1310 |
| webpack | main | 0.29 | 1.90 | 0.10 | 404 |
| webpack | helper off | 0.29 | 1.99 | 0.10 | 403 |
| webpack | helper on | 0.29 | 1.99 | 0.09 | 402 |

System time goes down by about the teardown in every pair (vscode 40-110 ms, supabase-studio 40-120 ms), user time
moves within its noise (±0.3 s on vscode) and peak RSS is the same: the threads' CPU time and the peak are still
reported, the teardown is not.

### Main against this change

What a user of `--fast` sees. Wall time, CPU time and peak RSS, branch against main:

| project | wall, 4 cores | wall, 8 cores | wall, 16 cores | CPU, 4 | CPU, 8 | CPU, 16 | peak RSS, 4 | peak RSS, 8 | peak RSS, 16 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | 0.8% | -0.7% | -4.0%* | 1.7%* | 2.7% | 2.3% | -0.2% | -0.2% | 0.0% |
| mui-docs | -2.8%* | -3.5%* | -6.9%* | -1.5% | 0.5% | -0.7% | -0.2% | 0.5% | -0.1% |
| supabase-studio | -0.5%* | -2.9%* | -7.7%* | 0.7% | 1.1%* | 0.5% | -0.1% | -0.1% | 0.0% |
| t3code-server | -0.7% | -0.4% | -3.2%* | -0.3% | 1.3% | 1.8% | -0.2% | -0.5% | -0.1% |
| webpack | -0.7% | -1.2% | -5.5%* | 0.3% | 0.8% | 1.1%* | -0.4%* | -0.3% | -0.6%* |
| mikro-orm | -0.8% | -2.5% | -6.3%* | 0.6% | 0.8% | 0.5% | -0.6% | -0.0% | 0.4% |
| cal-diy | -1.8%* | -3.2%* | -8.8%* | -0.1% | 0.5% | 0.2% | -0.4% | -0.6% | -0.4% |
| xstate-main | -1.5% | -3.2%* | -4.0%* | 0.7% | -0.8% | -0.7% | -1.0% | -1.1%* | -0.8%* |

Wall time falls on 23 of the 24 project and core-count cells (vscode at 4 cores: +0.8%, interval across 0),
significantly on 15, most at 16 cores (3-9%), where the teardown is the largest share of a shorter run. CPU time
comes out 0.5-2.7% higher for the branch on several projects, where the same-binary comparison above shows it
lower. The two binaries differ (new code in the CLI crate, fat LTO, no PGO in either build) while their instruction
counts are the same (within 0.3%, apart from work-stealing noise), so this is most likely code layout, not the
helper; it also takes part of the wall gain at 4 and 8 cores, where vscode does not improve. The release workflow's
PGO and BOLT builds lay the code out again, so this comparison does not predict them either way.

### Back to back

Without the wait between runs, the next run starts while the previous run's helper is still freeing memory on one
of its cores. Main against this change at 8 cores, no wait:

| project | wall, main | wall, branch | difference (95% CI) |
| --- | ---: | ---: | ---: |
| vscode | 2357 ms | 2322 ms | -43 ms (-82 to -15)* |
| mui-docs | 1699 ms | 1631 ms | -57 ms (-120 to 1) |
| webpack | 335 ms | 321 ms | -13 ms (-21 to 2) |

About the same as with the wait: the overlap with the next run's start-up costs little at these sizes.

## Pitfalls checked

All on the m8g.4xlarge above, release builds of main (A) and this change (B), `--fast`, unless noted.

- **Output and exit status.** vscode and webpack (exit status 2), a one-file project (0) and an unknown option (1),
  each with stdout to a file, through `| cat`, through `| head -1` (141 in all three arms: SIGPIPE before the exit),
  under `setsid -w ... < /dev/null` with no terminal, and through Node's `spawnSync` with both pipes: A, B with
  `TSRS_EXIT_HELPER=0` and B with the helper gave the same status and byte-identical stdout and stderr in every case.
  The integration test `crates/tsrs_cli/tests/integration/exit_helper.rs` checks the same for pipes and a file.
- **Pipes close with the main process.** With strace, the helper's two `close_range` calls finish before the main
  process's `ppoll` on the ready pipe returns; the main process exits 0.2 ms after its `exit_group`, and `| cat`
  ends with it ("Results": the pipeline's wall time falls like the plain run's).
- **The helper goes away.** strace on all eight projects: the main process's exit (0.2 ms) plus the helper's teardown
  adds up to the time from the main process's `exit_group` to the helper's exit, within 0.3 ms; the helper is gone when
  its teardown ends (vscode 76 ms after its `exit_group`, xstate-main 10 ms); 20
  webpack runs left no tsrs process 50 ms after each exit. The integration test reaps it as a child subreaper within
  its deadline.
- **Main process killed.** An `LD_PRELOAD` destructor that sleeps 3 s keeps the main process in `exit()` after the
  hand-off. SIGKILL and SIGTERM there: the main process died 3 ms after the signal, the helper about 90 ms after
  it (its teardown). Without a signal the helper waits out the 3 s and then exits: it never holds the memory longer
  than the main process lives plus the teardown. (A first try with strace's `inject=...:delay_enter` is not a valid
  check: strace holds the killed tracee at its exit event until the delay ends.)
- **PID 1.** `unshare --pid --fork --mount-proc tsrs ...`, traced from outside: no helper `clone`. Under a shell as
  PID 1, one.
- **A PID 1 that never reaps.** Node as PID 1 of a new PID namespace, running tsrs three times with `spawnSync`:
  three `Z` tsrs processes until Node exits; none with `TSRS_EXIT_HELPER=0`.
- **Below the threshold, and the opt-out.** `--fast --version` (7.5 MiB) starts no helper; `TSRS_EXIT_HELPER=0`
  never does (integration test and strace).
- **Thread-local state.** In the disassembly of the aarch64 release binary and of an x86_64 build of the module, the
  helper is a 16-byte frame of register moves and `svc`/`syscall` (`close_range` twice, `read` until it returns
  anything but `-EINTR`, `exit_group`) with no access through the thread pointer (`tpidr_el0`, `%fs`). The clone
  site passes `0x111` (`CLONE_VM | SIGCHLD`) and the end of the 16 KiB stack, 16-byte aligned; the child path is
  `mov x29, xzr; mov x0, x9; bl helper; brk` and `xor ebp, ebp; mov rdi, r12; call helper; ud2`.
- **Sanitizers and Miri.** Sanitizer builds (`system-alloc`, as CI's ThreadSanitizer job builds the CLI) would
  instrument `helper`, and the instrumentation keeps per-thread state behind the shared thread pointer, so those
  builds and Miri compile the no-op.
- **musl and x86_64.** The module, built alone for `aarch64-unknown-linux-musl` (static) and run in an arm64 Linux
  container, behaves as with glibc under strace: the main process exits 23 µs after its `exit_group` and the helper
  frees 1 GiB in 19 ms after it. The real `tsrs`, built for `aarch64-unknown-linux-musl` with cargo-zigbuild as the
  release workflow builds it (dev profile): on webpack the same status (2) and bytes with the helper off and on
  through `| cat`, the main process gone 0.4 ms after its `exit_group`, the helper 26 ms after that, nothing left.
  The x86_64 musl and glibc builds compile. An x86_64 build ran only under emulation (Rosetta in an amd64 container),
  where the emulated helper exits at once: the fallback path, with the exit status and output intact. CI's workspace
  tests run the integration test on x86_64 Linux.

## Zombies and the default

The orphaned helper is reaped by the nearest subreaper or by PID 1. Under a PID 1 that never reaps orphans (a
container whose command is Node or another program that is not an init, without `--init`), each run leaves one
zombie until the container exits: a pid and a task struct, no memory. A container with a pid limit that runs tsrs
thousands of times would run out of pids. `TSRS_EXIT_HELPER=0` turns the helper off. A caller that is itself a
child subreaper gets the helper as its child; one that waits for all its descendants (some sandboxes and process
supervisors do) waits for the teardown too, which is the behaviour without the helper.

The default: on in fast mode only, for now. It changes no output, exit status, user time or peak RSS, so it could
serve the reference-default mode too, which promises tsc's behaviour, not its exit timing. Before that I would add one
guard: skip the helper when PID 1 of the process's PID namespace is not a known init (`/proc/1/comm` one of systemd,
init, tini, docker-init, dumb-init, catatonit, s6-svscan, runit). That keeps the zombie out of the two common cases,
`docker run node ...` and GitHub Actions container jobs (whose PID 1 is `tail -f /dev/null`), for one small read at
exit. It is not done here.

## What was rejected or not done

- Freeing faster before the exit (`munmap`, `madvise` from many threads): `notes/perf-front-end-fixed-costs.md`.
- Huge pages on the mimalloc heap (`MIMALLOC_ALLOW_THP=1`): about a ninth of the teardown, 25-33% more peak RSS.
- mold's fork at start-up: hides CPU time and peak RSS from the caller.
- `CLONE_PARENT` (the helper as the caller's child, so its time is charged to the caller): the caller gets a child it
  did not start; one that waits only for its own child (Node, most shells' `$!` users) leaves it a zombie for as long as
  it lives, and `wait(-1)` loops would reap it in place of their own.
- A thread instead of a process: the group's exit waits for every thread, so the teardown stays on the waited path.

## macOS and Windows

Not done. Neither has a way to let a second process hold this one's address space: macOS has no `CLONE_VM` (`fork`
copies the map) and Windows creates every process with a map of its own. The first step on both is to measure whether
the teardown is on the waited path at all (on macOS the time from the last `write` to the parent's `wait4`
returning, with `dtruss` or `proc:::exit` probes; on Windows the time from `ExitProcess` to `WaitForSingleObject`
returning in the parent). If it is, what is left is giving memory back before the exit, which the earlier note found
no faster on Linux, or mold's fork at start-up, which macOS can do and Windows cannot, and which hides the work from
the caller.

## Reproduce

Release builds of both commits on one Linux machine, projects prepared with `node bench/bench.mjs prepare`. The
teardown, per process:

```sh
strace -f -ttt -e trace=exit_group -o /tmp/exit.txt taskset -c 0-7 target/release/tsrs -p src \
  --noEmit --incremental false --pretty false --fast
# the main process: its exit_group to its "+++ exited"; the helper: the other pid's exit_group to its "+++ exited"
```

Wall time from outside: alternate the two binaries (15 pairs, order flipped every pair) under `perf stat -e
instructions:u,cycles:u` and `/usr/bin/time -f %M`, with an untimed wait before each run until no `tsrs` process is
left (`while pgrep -x tsrs > /dev/null; do sleep 0.005; done`), so that a helper still freeing the previous run's
memory does not share the cores with the next run. `TSRS_EXIT_HELPER=0` on the candidate gives the baseline
behaviour in the same binary.
