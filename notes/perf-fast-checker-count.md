# perf-fast-checker-count: one `--fast` checker per core up to 32

`--fast` picked `clamp(max(cores / 2, min(cores, 16)), 4, 32)` checkers (notes/perf-default-checkers-16.md): every core
up to 16, then half the cores, so a 17-33 core machine stayed at 16 and a 64-core one got 32. The halving above 16
survived from the small-machine rule; notes/mem-round4.md section 3 measured 16 -> 32 on 64 vCPUs at -6..-32% wall
for +24..+34% peak memory and left the count as an open trade against `bun check`'s memory. `--fast` trades memory
for speed by design, so the rule is now `clamp(cores, 4, 32)`: up to 16 cores and from 64 cores on nothing changes,
17-63 cores get up to 32. Unchanged: the floor of one checker per 32 type-checked files beyond Go's 4, `-b`'s 4 per
project, explicit `--checkers` and the default mode's 4. Above 16 checkers leaf freeing is off by default
(fileregions.rs `MAX_DEFAULT_CHECKERS`, notes/mem-leaf-regions-32.md), which is part of the memory below.

## Measurements

One AWS `m8g.8xlarge` (Graviton4, 32 vCPU, 128 GiB, Ubuntu 24.04, THP `madvise`), release builds at main 48cf809ef
built in one checkout, 15 alternating pairs per cell (`runner.mjs`), `-p <project> --noEmit --incremental false
--pretty false --fast`. Cells: median of the per-pair ratios B / A - 1 with a ~95% interval.

### The sweep: the base binary at its default (16 here) against `--checkers N`

Wall time:

| project | 20 | 24 | 28 | 32 |
| --- | ---: | ---: | ---: | ---: |
| vscode | -14.5% [-15.4, -13.9] | -21.8% [-24.0, -21.1] | -29.0% [-29.6, -28.0] | -32.3% [-32.7, -31.7] |
| t3code-server | -4.9% [-7.8, -3.2] | -6.5% [-9.4, -3.5] | -9.2% [-11.6, -6.6] | -7.4% [-10.0, -6.1] |
| mui-docs | -2.0% [-3.4, -0.7] | -6.1% [-7.2, -5.6] | -9.2% [-10.0, -8.1] | -9.1% [-10.5, -8.1] |
| mikro-orm | -4.9% [-9.4, -2.9] | -9.6% [-11.4, -9.0] | -16.9% [-19.9, -15.4] | -12.7% [-17.5, -11.9] |
| supabase-studio | -3.6% [-4.6, -3.2] | -9.9% [-10.6, -9.4] | -16.2% [-17.0, -16.0] | -18.1% [-19.6, -17.4] |
| cal-diy | -1.7% [-2.1, -0.1] | -8.8% [-10.4, -6.7] | -14.2% [-15.9, -10.5] | -13.6% [-14.0, -11.8] |
| webpack | -9.4% [-11.2, -5.8] | -13.4% [-15.4, -11.6] | -14.8% [-16.4, -13.7] | -18.4% [-20.2, -15.2] |
| xstate-main | -22.6% [-24.4, -19.6] | -20.9% [-22.0, -20.6] | -21.1% [-24.8, -19.2] | -19.1% [-20.1, -18.4] |

CPU time (user + sys):

| project | 20 | 24 | 28 | 32 |
| --- | ---: | ---: | ---: | ---: |
| vscode | +0.6% | +2.7% | +3.8% | +5.5% |
| t3code-server | +4.6% | +8.1% | +13.0% | +15.9% |
| mui-docs | +7.6% | +10.8% | +14.6% | +17.5% |
| mikro-orm | +7.3% | +16.4% | +18.7% | +30.4% |
| supabase-studio | +6.2% | +13.9% | +17.8% | +23.9% |
| cal-diy | +9.8% | +12.7% | +21.0% | +25.2% |
| webpack | +2.6% | +10.2% | +14.2% | +20.1% |
| xstate-main | +23.3% | +31.8% | +38.1% | +46.9% |

Peak RSS:

| project | 20 | 24 | 28 | 32 |
| --- | ---: | ---: | ---: | ---: |
| vscode | +16.4% | +19.2% | +22.8% | +24.5% |
| t3code-server | +8.3% | +13.1% | +19.7% | +24.0% |
| mui-docs | +8.6% | +15.0% | +20.2% | +25.1% |
| mikro-orm | +9.1% | +19.3% | +21.1% | +34.0% |
| supabase-studio | +7.8% | +16.7% | +21.9% | +27.7% |
| cal-diy | +10.3% | +14.9% | +22.8% | +31.2% |
| webpack | +5.6% | +13.1% | +15.4% | +22.3% |
| xstate-main | +29.9% | +37.7% | +44.9% | +52.3% |

Wall time falls at every step up to 28 on all large projects; 32 is best or within the interval of 28 everywhere
except mikro-orm (-12.7% against -16.9%), and it is the count a per-core rule gives. The CPU growth is each checker
resolving the library types again, on cores the old rule left idle. xstate-main (248 type-checked files, automatic
count 7) and webpack (909, 28) run above their floor here because an explicit `--checkers` bypasses it; the rule
below keeps the floor.

### The rule: base against this branch, both at their automatic count (16 -> 32 checkers here)

| project | wall | CPU | peak RSS |
| --- | ---: | ---: | ---: |
| vscode | -31.8% [-32.4, -30.7] (1.13 -> 0.77 s) | +5.9% | +24.7% (2,010 -> 2,508 MiB) |
| t3code-server | -8.9% [-12.8, -5.0] (2.16 -> 1.97 s) | +16.3% | +24.0% (2,040 -> 2,531 MiB) |
| mui-docs | -9.3% [-9.9, -7.5] (1.24 -> 1.12 s) | +18.3% | +24.4% (1,535 -> 1,913 MiB) |
| mikro-orm | -17.2% [-19.3, -11.9] (1.05 -> 0.87 s) | +30.8% | +33.1% (2,007 -> 2,691 MiB) |
| supabase-studio | -18.3% [-18.5, -17.5] (0.93 -> 0.76 s) | +23.5% | +27.6% (1,626 -> 2,076 MiB) |
| cal-diy | -12.4% [-13.0, -11.4] (0.92 -> 0.81 s) | +26.3% | +31.0% (1,806 -> 2,361 MiB) |
| webpack (16 -> 28) | -17.3% [-19.9, -14.7] (0.21 -> 0.18 s) | +19.7% | +22.0% (499 -> 612 MiB) |
| xstate-main (7 -> 7) | -0.1% [-1.3, +1.6] | -0.9% | +2.4% (267 -> 270 MiB; leaf freeing off) |

Diagnostics are byte-identical to base in default mode and in `--fast` (16 against 32 checkers) on all eight
projects; default-mode `--extendedDiagnostics` counters are unchanged, and single-threaded and default-mode
instructions are within 0.1%. Machines with 33-63 cores also move to 32 (24 on 48 cores before);
notes/perf-checker-64.md measured 32 at or below 24 on 64 vCPUs, but 33-63 cores were not measured here.

Not measured as a rule: a lower file floor under `--fast`. xstate-main runs at 7 checkers by it, and the sweep's
explicit 20 checkers took -22.6% wall for +29.9% peak RSS (265 -> 345 MiB).
