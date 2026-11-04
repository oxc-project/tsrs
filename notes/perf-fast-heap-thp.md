# perf-fast-heap-thp: transparent huge pages for mimalloc's heap under `--fast`

The build keeps transparent huge pages off mimalloc's heap (`no_thp`, notes/mem-no-thp.md): every partly filled
thread-local mimalloc page was resident as a whole 2 MiB page, 25-33% of peak memory on 64 vCPUs, and that note also
measured the price, 1-5% check time and 28-39% parse time. `--fast` already spends memory on more checkers and
caches, so under `--fast` the CLI gives the heap huge pages again. The arena's own `MADV_HUGEPAGE` advice is unchanged.

## Mechanism

mimalloc fixes the heap's policy in its start-up constructor (`mi_process_attach`, priority 101): with `allow_thp = 0`
it records that the OS has no THP and nothing set later turns that back on, and `main` runs too late to read the
command line first. So `crates/tsrs_cli/src/main.rs` registers a priority-100 constructor that reads argv and envp
(glibc passes them to constructors) the way the option parser decides `--fast` and calls
`mi_option_set(mi_option_allow_thp, 1)` before mimalloc starts. It allocates nothing: the first allocation would
start mimalloc with the default policy.

- An explicit `MIMALLOC_ALLOW_THP` in the environment wins, as before.
- `--lsp`, `--api` and `--lint` keep the build's policy.
- A `--fast` inside a response file is not seen; that run keeps the build's heap policy. A miss only changes memory
  use and speed, never output.
- musl builds keep the build's policy: musl calls constructors without arguments.
- The integration test `fast_heap_huge_pages` (tsrs_cli, Linux glibc) reads the value mimalloc starts with from its
  `MIMALLOC_VERBOSE=1` report, which also pins the option number: 43 in mimalloc v3's `mimalloc.h`;
  `libmimalloc-sys2`'s Rust constant, 37, is v2's.

`allow_thp = 2` (mimalloc's "full" mode, which also raises the minimum purge size to 2 MiB) measured the same as 1
(second table), so the change uses 1 and leaves purging as it is in the default build.

## Measurements

One AWS `m8g.8xlarge` (Graviton4, 32 vCPU, 128 GiB, Ubuntu 24.04, kernel 7.0, THP `enabled` and `defrag` both
`madvise`). Base = main 48cf809ef, candidate = this branch, both `cargo build --release --locked -p tsrs_cli` in the
same checkout. 15 alternating pairs per project (`runner.mjs`, order flipped each pair), `-p <project> --noEmit
--incremental false --pretty false --fast` at the default checker count (16 on 32 vCPUs). Cells: median of the
per-pair ratios candidate / base - 1 with a ~95% interval; CPU is user + sys of the process; peak RSS from
`/usr/bin/time`.

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

`--fast` instruction counts move by up to 1% from run to run (work stealing); single-threaded default-mode
instructions, where the change does nothing, are identical (all eight within 0.0%).

The same setting through the environment on the base binary (`MIMALLOC_ALLOW_THP=1` against unset, same box, 15
pairs) gave the same numbers, and `MIMALLOC_ALLOW_THP=2` the same again:

| project | wall, `=1` | wall, `=2` | peak RSS, `=1` | peak RSS, `=2` |
| --- | ---: | ---: | ---: | ---: |
| vscode | -9.5% | -9.7% | +39.4% | +39.9% |
| t3code-server | -6.2% | -7.7% | +34.0% | +34.0% |
| mui-docs | -10.3% | -10.3% | +32.5% | +32.1% |
| mikro-orm | -11.3% | -10.9% | +24.1% | +23.7% |
| supabase-studio | -11.3% | -11.7% | +42.4% | +42.8% |
| cal-diy | -12.6% | -13.9% | +35.2% | +35.8% |
| webpack | -7.3% | -9.5% | +95.4% | +95.0% |
| xstate-main | -4.4% | -5.8% | +120.3% | +121.0% |

Read:

- Wall time falls 4-14% on every project. CPU falls less (3-6% on the six large projects, flat on the two small
  ones), so most of the gain is the parallel phases waiting less on page faults: 32 parse threads and 16 checkers
  faulting 4 KiB pages at once contend in the kernel.
- The memory cost is the one notes/mem-no-thp.md describes: about 0.5-0.8 GiB on the large projects (+23-42%), and a
  fixed +0.3-0.5 GiB that doubles the small ones, from the partly used 2 MiB pages each of the ~64 parse and checker
  threads holds.
- Diagnostics are byte-identical to base in default mode and `--fast` on all eight projects, and the default-mode
  `--extendedDiagnostics` counters are unchanged.
