# perf-fast-memory-knobs: four memory-for-speed knobs for `--fast`, measured on Graviton4

`--fast` may spend memory to be faster. Four knobs that earlier notes had set for memory, or measured as small, were
measured again under `--fast`. Three landed as separate changes with their own notes; this note holds the rest: the
rejected plain-pointer build (section 1), the dropped table-sizing candidates (section 2) and the two landed `--fast`
changes measured together (section 3).

| knob | verdict |
| --- | --- |
| Transparent huge pages for mimalloc's heap under `--fast` | landed: notes/perf-fast-heap-thp.md |
| One `--fast` checker per core up to 32 | landed: notes/perf-fast-checker-count.md |
| Sizing symbol tables once before filling them | landed: notes/perf-presized-tables.md |
| Plain 64-bit pointers (`tsrs_core/plain-ptrs`) for `--fast` | rejected: no speed on Graviton4, +17-34% peak (section 1) |

Method: one AWS `m8g.8xlarge` (Graviton4, 32 vCPU, 128 GiB, Ubuntu 24.04, kernel 7.0, THP `enabled` and `defrag`
both `madvise`), base = main 48cf809ef, release builds from one checkout, 15 alternating pairs per project
(`runner.mjs`), `-p <project> --noEmit --incremental false --pretty false --fast` at the default 16 checkers;
single-threaded cells are default mode with `--singleThreaded`. Cells: median of per-pair ratios candidate / base - 1,
~95% interval.

## 1. Plain 64-bit pointers (not landed)

Would `--fast` users be faster with a second binary built with `tsrs_core/plain-ptrs` (no 32-bit handles)?
notes/mem-pointer-compression.md measured about 1% wall on x86-64 for +15% memory, and on arm64 one extra `add` per
pointer chase, so arm64 is where it should pay most.

As built the comparison is unfair: a `plain-ptrs` build takes its arena chunks from mimalloc (`os_chunk`), and the
`no_thp` mimalloc does not advise its heap, so the plain arena runs on 4 KiB pages while the compressed arena advises
its own chunks with `MADV_HUGEPAGE`. The fair comparison gives both builds huge pages everywhere
(`MIMALLOC_ALLOW_THP=1` on both sides, the setting notes/perf-fast-heap-thp.md applies under `--fast`).

| project | wall, both huge pages | CPU, both huge pages | instructions | peak RSS | wall, as built | wall, single-threaded as built |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | -0.7% [-2.0, +0.2] | -0.4% | -3.9% | +33.5% | +19.7% | +7.6% |
| t3code-server | -0.5% [-3.2, +1.9] | -0.2% | -4.6% | +30.7% | +11.1% | +8.9% |
| mui-docs | +0.3% [-1.7, +1.2] | +0.0% | -3.7% | +23.2% | +12.1% | +7.9% |
| mikro-orm | -0.3% [-1.4, +0.3] | +0.1% | -4.6% | +29.1% | +16.2% | +7.7% |
| supabase-studio | -1.8% [-3.3, -0.2] | -1.9% | -4.1% | +21.2% | +14.8% | +8.4% |
| cal-diy | -0.1% [-1.8, +1.9] | +0.0% | -4.0% | +20.8% | +15.5% | +7.2% |
| webpack | -1.4% [-2.8, +0.7] | -0.9% | -3.8% | +19.9% | +17.4% | +7.7% |
| xstate-main | -1.9% [-3.4, -0.3] | -2.7% | -4.5% | +16.7% | +8.8% | +7.1% |

Single-threaded (default mode, 3 pairs): instructions -3.1 to -4.7%, peak RSS +17-24%.

- No speed to buy: with huge pages on both sides the plain build retires 3.7-4.6% fewer instructions (the handle
  decode adds are gone) and takes the same CPU and wall time (six of eight wall intervals include zero, the other two
  are about -2%). The larger structures (+17-34% peak) cost back in cache misses what the adds saved.
- As built it is 9-20% slower at 16 checkers and 7-9% slower single-threaded because its arena loses huge pages;
  shipping it would also need its arena chunks advised.
- Rejected: a second binary in the npm package (`npm/`) for no measurable gain. Revisit only if a workload shows the
  handle decode as a real CPU cost (none of these eight does).

## 2. Table sizing: what was dropped

The `perf record` behind notes/perf-presized-tables.md (hashbrown growth at 1.7-4.4% of `--fast` samples) also put
`ReferenceInstantiations::add` at 1.1% of t3code-server's samples (one table per generic target) and the `PackedMap`
instantiation caches under `instantiate_type_with_alias_worker` at 0.4%. Two candidates for those tables were
measured on top of the symbol-table sizing change and dropped:

| candidate | single-threaded instructions on top | `--fast` result | verdict |
| --- | --- | --- | --- |
| Hash `ReferenceInstantiations` keys by argument handle instead of type id (no read of each argument type) | 0.0 to -0.1 points | not separable from noise | dropped: no gain |
| Under `--fast`, grow a full `ReferenceInstantiations` table 4x instead of 2x | n/a (fast only) | with both candidates on top: t3code-server instructions +0.2% [-0.4, +0.7] against -0.4% [-0.9, +0.2] for the sizing change alone; wall the same within noise | dropped: a `--fast` switch with no measurable gain |

Per-target instantiation tables have no size estimate before they fill (which generic gets thousands of
instantiations is a property of the program), so their growth stays.

## 3. Heap huge pages and 32 checkers together

A binary with both landed `--fast` changes (heap huge pages and one checker per core up to 32) against base at 16
checkers with its heap off huge pages, same box and method:

| project | wall | CPU | peak RSS |
| --- | ---: | ---: | ---: |
| vscode | -42.3% [-42.8, -41.9] (1.15 -> 0.66 s) | +1.4% | +81.1% (2,013 -> 3,641 MiB) |
| t3code-server | -15.3% [-19.3, -13.9] (2.19 -> 1.85 s) | +11.2% | +70.5% (2,028 -> 3,455 MiB) |
| mui-docs | -20.1% [-21.9, -19.5] (1.26 -> 1.00 s) | +14.4% | +72.7% (1,532 -> 2,640 MiB) |
| mikro-orm | -29.7% [-31.1, -24.1] (1.07 -> 0.75 s) | +25.0% | +71.3% (2,020 -> 3,455 MiB) |
| supabase-studio | -31.5% [-32.0, -31.2] (0.94 -> 0.64 s) | +19.6% | +86.6% (1,627 -> 3,036 MiB) |
| cal-diy | -27.5% [-29.0, -27.3] (0.93 -> 0.67 s) | +22.1% | +83.5% (1,806 -> 3,314 MiB) |
| webpack | -26.8% [-28.9, -23.1] (0.22 -> 0.16 s) | +19.7% | +153.6% (496 -> 1,257 MiB) |
| xstate-main | -6.6% [-7.5, -5.4] (0.21 -> 0.19 s) | -1.9% | +122.1% (265 -> 587 MiB) |

The wall-time gains roughly add up (vscode: -9.4% and -31.8% alone, -42.3% together); the memory compounds, because
every extra checker thread also holds its partly used heap pages as 2 MiB pages: 1.7-1.9x the base's peak on the
large projects. Diagnostics stayed byte-identical in default mode and `--fast`.
