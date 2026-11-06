# perf-presized-tables: size inherited-member and export tables once

A `perf record` of `--fast` on a 32-vCPU Graviton4 box (release build with frame pointers, 2,999 Hz) put hashbrown
table growth at 2.6% of t3code-server's samples, 4.1% of mui-docs', 4.4% of cal-diy's and 1.7% of vscode's. On
mui-docs and cal-diy the largest share is symbol-table index growth (`RawTable<u32>::reserve_rehash` under
`SymbolMap::insert`) from two loops that fill a table from a list of known length:

| caller | mui-docs | cal-diy | vscode | t3code-server |
| --- | ---: | ---: | ---: | ---: |
| `add_inherited_members` (a derived type's members: its own plus every base member) | 0.9% | 0.7% | 0.2% | <0.1% |
| `extend_export_symbols` (`export *` re-exports) | <0.1% | 0.6% | <0.1% | <0.1% |

(Percent of all samples. The rest of the growth is per-type instantiation tables, the relation cache and link
stores, none of which knows its final size up front.) The index rehashes every key at each doubling, and rehashing a
key reads the symbol's name and hashes it again, so a table filled one entry at a time hashes about twice its final
size in names.

`SymbolTable::reserve` sizes the entry buffer and (past the linear-scan size of 16) the index once; both loops call
it right before their first insert with the number of entries left in the source list. Capacity only, so it is not
gated on `--fast`, and a table the loop would not write (a binder's table) is never touched. Over-reserving is bounded
by the overridden members (a derived member that shadows a base member is not inserted).

## Measurements

One AWS `m8g.8xlarge` (Graviton4, 32 vCPU, 128 GiB, Ubuntu 24.04, THP `madvise`). Base = main 48cf809ef, candidate =
this branch, both `cargo build --release --locked -p tsrs_cli` in one checkout. Single-threaded: default mode with
`--singleThreaded`, 2 alternating pairs (instructions are deterministic); `--fast`: 15 alternating pairs at the
default 16 checkers. Cells: median of per-pair ratios candidate / base - 1 (interval for `--fast`).

| project | single-threaded instructions | `--fast` CPU | `--fast` wall | `--fast` peak RSS |
| --- | ---: | ---: | ---: | ---: |
| vscode | -0.3% (84.62 -> 84.36 G) | -0.4% [-1.2, +1.0] | -0.2% [-1.5, +0.9] | -0.2% |
| t3code-server | -0.3% (46.12 -> 45.97 G) | -0.5% [-1.1, +0.7] | -0.9% [-4.1, +2.8] | -0.6% |
| mui-docs | -1.2% (33.17 -> 32.78 G) | -1.4% [-2.5, -0.3] | -0.8% [-2.5, -0.5] | -0.7% |
| mikro-orm | -0.5% (51.85 -> 51.57 G) | +0.2% [-2.2, +0.7] | -0.2% [-1.0, +0.6] | -0.4% |
| supabase-studio | -0.4% (39.53 -> 39.37 G) | -1.0% [-2.3, -0.1] | -0.7% [-1.6, +0.7] | +0.1% |
| cal-diy | -0.9% (28.04 -> 27.81 G) | -1.3% [-2.6, -0.9] | -0.8% [-2.4, +0.0] | -0.9% |
| webpack | -0.7% (10.28 -> 10.21 G) | -1.3% [-1.9, +0.2] | -1.0% [-3.3, +0.6] | -0.1% |
| xstate-main | -0.3% (5.46 -> 5.45 G) | -0.6% [-1.8, +0.3] | -2.0% [-3.2, -0.0] | -1.1% |

Fewer instructions on all eight projects, 0.3-1.2%, most where large class and interface hierarchies or `export *`
barrels are (mui-docs, cal-diy); `--fast` CPU and wall time follow within their noise. Single-threaded peak RSS moves
by -0.7% to +0.7% (exact capacities replace growth slack). Diagnostics are byte-identical to base in default mode and
`--fast` on all eight projects, and the default-mode `--extendedDiagnostics` counters are unchanged.

## Measured on top and dropped

The other large grower in the same profile, the per-generic `ReferenceInstantiations` tables (1.1% of t3code-server's
samples), has no size estimate before it fills. Two candidates were measured on top of this change:

Hashing `ReferenceInstantiations` keys by argument handle instead of type id, so a lookup does not read each argument
type: 0.0 to -0.1 points more single-threaded instructions, not separable from noise under `--fast`. Dropped.

Growing a full `ReferenceInstantiations` table 4x instead of 2x under `--fast`: with both candidates on top,
t3code-server's `--fast` instructions were +0.2% [-0.4, +0.7] against -0.4% [-0.9, +0.2] for this change alone, wall
the same within noise. Dropped: a `--fast` switch with no measurable gain.
