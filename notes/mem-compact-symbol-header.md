# Compact instantiated symbol headers

Experiment against `69c9ac78`, 2026-10-23. Two implementations were evaluated: a 16-byte compressed header and a
24-byte fallback. The ordinary allocation remains 32 bytes with compressed pointers and 40 bytes with plain
pointers. The experiment initially used a 5% whole-project wall-time or peak-RSS selection gate. The 16-byte
layout is retained at the user's request despite smaller measured gains; the current repository policy has no
fixed minimum percentage threshold.

## Layout

64-bit allocation sizes, excluding the optional symbol-table tail and declaration-list contents:

| Allocation | Go | Rust baseline | 16-byte candidate | 24-byte fallback |
| --- | ---: | ---: | ---: | ---: |
| Instantiated symbol, compressed pointers | 24 | 32 | 16 | 24 |
| Ordinary symbol, compressed pointers | 88 | 32 | 32 | 32 |
| Instance plus common checker links, compressed pointers | 56 | 48 | 32 | 40 |
| Instantiated symbol, plain pointers | 24 | 40 | 24 | 32 |
| Ordinary symbol, plain pointers | 88 | 40 | 40 | 40 |
| Instance plus common checker links, plain pointers | 56 | 64 | 48 | 56 |

Go's ordinary total includes its separate 64-byte `symbolData`. Both Rust candidates keep ordinary metadata in
the same allocation as the header; an instantiated symbol allocates only the header and references an ordinary
owner. Nested instances flatten that reference.

The 16-byte header contains:

| Field | Compressed bytes | Meaning |
| --- | ---: | --- |
| `flags` | 4 | Semantic symbol kind and attributes, independent for each instance. |
| `check_flags_and_kind` | 4 | Semantic checker flags, with private bit 31 selecting shared metadata. |
| `id` | 4 | Atomic symbol identity; its existing reserved-block bit is unchanged. |
| `parent_or_tables` | 4 | Ordinary parent or optional table tail; on an instance, the ordinary metadata owner. |

Only ordinary allocations append `SymbolData`: an 8-byte tagged name and an 8-byte declaration slice. The name's
tags still distinguish a table tail and a value declaration equal to the first declaration. `ThinSlice` keeps the
declarations slot at 8 bytes with either pointer mode, including its existing long form for large lists.

The 24-byte fallback keeps the tagged name in the header and uses its existing zero marker for shared metadata.
Only declarations move into the appended payload. It needs no private check-flag bit.

## Safety and behavior

- `Symbol` has no `Default`: ordinary symbols must be allocated with their payload through `Symbol::new`.
- Private constructors and `repr(C)` prefix/size assertions enforce the allocation layout. Payload access uses
  the existing `arena_neighbor` helper so that it has arena provenance beyond the header reference.
- All callers use semantic check-flag accessors. The 16-byte setter rejects the reserved bit and preserves the
  current layout tag, including when clearing flags; the getter masks it before API serialization or checking.
- Metadata writes retain `OwnedCell` ownership and frozen-graph handling. Ordinary copies remain independent;
  instantiated symbols share live names, declarations, parent, value declaration, tables, and lazy member lists.
- Focused tests cover nested owner flattening, identity and flag independence, flag clearing, tag collision, and
  declaration lists of 65,536 elements followed by append and replacement.

Both candidates preserve all passing conformance cases in the default and canonical histories: 13,699 error,
12,997 type, 12,997 symbol, 13,625 emit, 149 JavaScript-map and 156 source-map-record baselines. Both retain the
baseline timeout in `compiler/intersectionConstructorReductionCrash`. Native and plain-pointer tests, checked
cells, Clippy, source checks and WebAssembly checks pass.
The 16-byte candidate also passes the workspace/API tests; Unix-socket and filesystem-notification tests require
running outside the filesystem sandbox. The opt-in shared graph faults on a protected write in both the unchanged
baseline and the candidate when exercised with plain pointers; this existing limitation is not resolved here.

## Measurements

Release builds use the pinned toolchain, fat LTO, debug information enabled and stripping disabled. Hyperfine 2
collects wall time, retired instructions and peak RSS, alternating baseline/candidate order. Each cell has two
untimed warmups per binary. The catalog screen uses three measured pairs at one checker and the default count;
seven projects have ten-pair confirmation runs in both modes. `--checkers 1` still permits parallel parsing and
binding, so instruction and exact counter comparisons additionally use `--singleThreaded`.

All 17 cached catalog projects are at their pinned revisions. `typeorm` and `date-fns` are unavailable locally.
Diagnostics match in every measured cell. Results use the median of within-pair percentage changes, which avoids
the misleading larger changes seen when comparing independent medians during run-to-run drift.
Both candidates preserve Files, Symbols, Types and Instantiations exactly on all 17 projects with `--singleThreaded`.

The ten-pair 16-byte confirmation has paired wall changes from -1.77% to +1.64%. Its largest paired RSS reduction
is 2.48%. Additional ten-pair single-threaded runs on Compiler and t3code have essentially unchanged wall time
(-0.05% and -0.12%), with RSS reductions of 0.53% and 1.82%. None reaches the 5% gate.

The 24-byte fallback was screened on all 17 projects with `--singleThreaded` and the default checker count, three
pairs each. The best paired screen changes were -2.93% wall and -2.27% RSS on next-packages-next at the default
count. Ten-pair confirmation on t3code-server, mikro-orm, next-packages-next and playwright gave:

| Project, default checkers | Wall | Instructions | Peak RSS |
| --- | ---: | ---: | ---: |
| t3code-server | +0.73% | -0.36% | -1.63% |
| mikro-orm | -4.48% | -0.72% | -1.71% |
| next-packages-next | +5.91% | +0.69% | -0.70% |
| playwright | -2.40% | +1.10% | -0.60% |

Default-checker timings vary with scheduling. Several apparent wins and regressions in the three-pair screens
disappeared in confirmation, including the 16-byte MUI, Storybook and t3code speedups. The fallback's initial
next-packages-next improvement also reversed in confirmation. These are not evidence for a repeatable 5% gain.

## Decision

Keep the 16-byte layout at the user's request. It halves the compressed instantiated-symbol allocation from
32 to 16 bytes, while ordinary symbols remain 32 bytes. Confirmed whole-project RSS reductions are modest
(up to 2.48%), with essentially unchanged single-threaded wall time and instructions. Neither layout met the
experiment's original 5% gate. Ordinary symbols, types, ASTs, links, tables and other allocations still dominate
peak RSS.

The 16-byte implementation is adopted; the 24-byte fallback remains an evaluated alternative.
The [complete paired measurements](mem-compact-symbol-header-results.md) include every catalog cell, retired
instructions, the single-threaded samples and all confirmation runs. Raw JSON, binaries, measurement scripts and
both implementation patches are retained locally in `/private/tmp/tsrs-symbol-results`.
