# perf-fast-heap-thp: transparent huge pages for mimalloc's heap under `--fast`, after the parse phase

The build keeps transparent huge pages off mimalloc's heap (`no_thp`, notes/mem-no-thp.md): every partly filled
thread-local mimalloc page would be resident as a whole 2 MiB page. `--fast` may spend memory, so under `--fast` the
CLI gives the checkers' heap huge pages; the parse phase's heap stays without them. The arena's own `MADV_HUGEPAGE`
advice is unchanged.

## Mechanism

mimalloc decides once, in its start-up constructor (`mi_process_attach`, priority 101), whether the OS has THP: with
`allow_thp = 0` it records that it has none and nothing set later turns huge pages back on, and `main` runs too late
to read the command line first. So `crates/tsrs_cli/src/main.rs` registers a priority-100 constructor that reads argv
and envp (glibc passes them to constructors) the way the option parser decides `--fast`, and sets two options before
mimalloc starts:

- `allow_thp = 1`, so mimalloc advises the memory it maps for huge pages;
- `arena_reserve = 64 MiB` instead of 1 GiB, so the checkers' memory is mapped after the parse phase instead of carved
  out of the arena the parse threads filled.

`main` then sets `allow_thp = 0`, and the checker pool sets it back to 1 once the program is parsed, before any
checker thread starts (`tsrs_core::allocator::parse_ended`). The constructor allocates nothing: the first allocation
would start mimalloc with the default policy.

- An explicit `MIMALLOC_ALLOW_THP` or `MIMALLOC_ARENA_RESERVE` in the environment wins.
- `--lsp`, `--api` and `--lint` keep the build's policy.
- A `--fast` inside a response file is not seen; that run keeps the build's policy. A miss only changes memory use and
  speed, never output.
- musl builds keep the build's policy: musl calls constructors without arguments.
- The `fast_heap_huge_pages` integration tests (tsrs_cli, Linux glibc) read the values mimalloc starts with from its
  `MIMALLOC_VERBOSE=1` report, which pins the option numbers: 43 and 23 in mimalloc v3's `mimalloc.h`
  (`libmimalloc-sys2`'s constants are v2's).

## Why the gate

`TSRS_MEM_SPLIT=1` on vscode showed that with huge pages for the whole run, the heap at parse end holds about 470 MiB
more resident memory than without (the same block, not one that scales with the program, on every project measured):
free space inside the partly used pages of 32 parse threads, now 2 MiB each. That fixed block is why the first
version of this change doubled the small projects' peak. With the gate, vscode's heap at parse end has 60 MiB on huge
pages (the first 64 MiB arena, mapped at start-up) instead of 762 MiB, and nearly all of the checkers' heap is on huge
pages.

## Measurements

One AWS `m8g.8xlarge` per comparison (Graviton4, 32 vCPU, 128 GiB, Ubuntu 24.04, kernel 7.0, THP `enabled` and
`defrag` both `madvise`), base and candidate built with `cargo build --release --locked -p tsrs_cli` in the same
checkout. 15 alternating pairs per project (`runner.mjs`, order flipped each pair), `-p <project> --noEmit
--incremental false --pretty false --fast` at the default checker count (16 on 32 vCPUs). Cells: median of the
per-pair ratios candidate / base - 1 with a ~95% interval; CPU is user + sys of the process; peak RSS from
`/usr/bin/time`. Diagnostics are byte-identical to base in default mode and `--fast` on every project, and the
default-mode `--extendedDiagnostics` counters are unchanged.

This branch against main 48cf809ef:

| project | wall | CPU | peak RSS | instructions |
| --- | ---: | ---: | ---: | ---: |
| vscode | -5.4% [-5.9, -4.6] (1.14 -> 1.08 s) | -2.6% [-3.3, -2.4] | +15.2% [14.4, 16.0] (2,013 -> 2,322 MiB) | +0.0% (noise) |
| t3code-server | -3.9% [-7.5, -1.5] (2.13 -> 2.03 s) | -2.5% [-4.3, -2.2] | +14.2% [13.8, 15.3] (2,028 -> 2,317 MiB) | +0.5% (noise) |
| mui-docs | -6.2% [-6.6, -5.7] (1.23 -> 1.15 s) | -3.2% [-3.6, -2.8] | +19.4% [19.2, 20.2] (1,530 -> 1,828 MiB) | +0.1% (noise) |
| mikro-orm | -9.0% [-9.7, -8.0] (1.06 -> 0.96 s) | -5.0% [-5.6, -4.7] | +12.9% [12.6, 13.8] (2,019 -> 2,276 MiB) | -0.8% |
| supabase-studio | -6.1% [-6.7, -5.3] (0.92 -> 0.86 s) | -3.0% [-3.1, -1.8] | +18.3% [18.1, 19.0] (1,628 -> 1,926 MiB) | +0.2% (noise) |
| cal-diy | -9.4% [-10.0, -7.7] (0.92 -> 0.83 s) | -3.3% [-4.5, -2.4] | +17.7% [17.4, 19.1] (1,803 -> 2,119 MiB) | -0.2% (noise) |
| webpack | -4.6% [-6.7, -3.6] (0.21 -> 0.20 s) | -0.5% [-1.0, 0.0] | +50.6% [50.1, 51.4] (498 -> 750 MiB) | -0.0% (noise) |
| xstate-main | -2.5% [-3.6, -0.7] (0.21 -> 0.20 s) | -0.1% [-0.8, 0.5] | +60.4% [58.9, 63.4] (266 -> 428 MiB) | +0.2% (noise) |
| a large private monorepo | -9.9% [-13.1, -6.2] | -6.3% [-6.6, -5.4] | +2.6% [2.0, 3.2] | -0.2% (noise) |

Against the first version of this change (huge pages for the whole run, 2c67f0fe7), on another box:

| project | wall | peak RSS |
| --- | ---: | ---: |
| vscode | +4.2% [3.5, 4.7] | -17.4% [-18.4, -17.1] (2,809 -> 2,320 MiB) |
| t3code-server | +0.6% [-0.1, 2.2] | -14.0% [-14.7, -13.5] (2,710 -> 2,332 MiB) |
| mui-docs | +3.3% [1.8, 3.5] | -9.9% [-10.1, -9.0] (2,025 -> 1,830 MiB) |
| mikro-orm | +1.2% [0.6, 1.3] | -8.5% [-9.5, -7.7] (2,503 -> 2,288 MiB) |
| supabase-studio | +3.5% [3.3, 3.9] | -16.2% [-16.9, -15.9] (2,309 -> 1,937 MiB) |
| cal-diy | +3.1% [2.3, 3.8] | -13.5% [-14.4, -12.7] (2,452 -> 2,126 MiB) |
| webpack | +2.4% [1.9, 3.8] | -22.4% [-23.4, -21.4] (974 -> 750 MiB) |
| xstate-main | +1.9% [1.1, 2.8] | -28.2% [-28.8, -26.4] (587 -> 425 MiB) |
| a large private monorepo | +3.2% [1.7, 5.6] | -4.7% [-5.3, -4.5] |

The first version against main 48cf809ef:

| project | wall | CPU | peak RSS | instructions |
| --- | ---: | ---: | ---: | ---: |
| vscode | -9.4% [-11.3, -7.5] (1.13 -> 1.03 s) | -2.9% [-4.3, -1.8] | +39.4% (2,012 -> 2,811 MiB) | -0.0% |
| t3code-server | -7.8% [-8.6, -6.0] (2.18 -> 2.01 s) | -3.3% [-4.3, -3.1] | +33.6% (2,033 -> 2,716 MiB) | -0.4% (noise) |
| mui-docs | -11.2% [-12.7, -10.2] (1.25 -> 1.10 s) | -4.4% [-5.3, -3.0] | +33.3% (1,527 -> 2,035 MiB) | +0.1% |
| mikro-orm | -10.8% [-12.3, -9.9] (1.07 -> 0.96 s) | -5.8% [-7.1, -4.9] | +23.2% (2,022 -> 2,481 MiB) | -0.9% (noise) |
| supabase-studio | -11.0% [-12.1, -10.2] (0.93 -> 0.83 s) | -3.0% [-3.9, -2.4] | +41.8% (1,628 -> 2,309 MiB) | +0.2% |
| cal-diy | -13.6% [-14.6, -12.1] (0.91 -> 0.80 s) | -4.2% [-5.3, -3.2] | +35.6% (1,802 -> 2,455 MiB) | -0.1% |
| webpack | -6.6% [-10.0, -3.8] (0.21 -> 0.20 s) | +0.5% [-2.0, +2.9] | +95.4% (499 -> 977 MiB) | +1.0% (noise) |
| xstate-main | -4.2% [-5.5, -3.2] (0.20 -> 0.19 s) | +1.4% [+0.9, +2.2] | +121.0% (264 -> 583 MiB) | +0.2% |

Read:

- The gain is fewer page faults and TLB misses (vscode: faults 269k -> 182k, dTLB misses 306M -> 189M; the private
  monorepo: faults -67%, dTLB misses -43%). Wall falls more than CPU, so part of it is the checkers waiting less on
  page faults.
- The gate gives back about a third of the first version's wall gain (the parse phase's share) for most of its
  memory: peak +13-19% on the large catalog projects instead of +23-42%, +51-60% on the two small ones instead of
  +95-121%, and +2.6% on the private monorepo instead of +8.8%.

Measured and rejected (same box type, 15 pairs, `--fast`):

- Never giving freed heap memory back (`MIMALLOC_PURGE_DELAY=-1`): no change in wall or memory.
- Huge pages only for allocations of 2 MiB or more: no gain (wall and CPU +0.5-1.5%).
- 16 parse threads with huge pages for the whole run: a quarter less memory than the first version, but vscode lost
  half its gain (-9.3% -> -4.4%).
- The gate with mimalloc's default 1 GiB arenas: little memory saved (vscode +22%, webpack +94%), because the
  checkers keep filling the arena the parse phase reserved. 64 MiB arenas alone, without huge pages, change nothing.
