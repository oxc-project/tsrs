# perf-cache-line-tables: one cache line per probe in the checker's big hash tables, and huge pages for them

Measured 2026-11-12 on main `221ee7bf`, one m8g.4xlarge (16 Graviton4 vCPUs, 64 GiB, THP `madvise`, kernel 7.0). Both
arms of every comparison ran on that box, built the same way (`cargo build --release --locked -p tsrs_cli`: fat LTO,
no PGO, no BOLT) and run from a fresh `cp` of each binary (see "Method"). Timed command: `taskset -c 0-7` (8 cores,
"8c") or `0-3` ("4c") `tsrs -p <project> --noEmit --incremental false --pretty false --fast`, 15 alternating pairs per
cell (`aws-bench-box` `runner.mjs` and `analyze.mjs`: the median of the paired ratios with a ~95% interval; `*` marks
an interval that excludes 0). The main workload is a large private monorepo, for which only relative numbers are
given; vscode and t3code-server are the public checks.

**Result.** None of the three table rewrites the round set out to build pays: one-line buckets for the relation
cache's pair table, one-line buckets for `PackedMap`, and the hash beside each handle in `ReferenceInstantiations`
are slower than, level with, or cost memory against the hashbrown tables they replace. What did measure is huge pages
for main's own hashbrown tables of those two kinds once they reach 2 MiB: -1.5% / -1.3% CPU at 8 / 4 cores against
the same build with the huge path off, -0.6 to -0.7% peak RSS, nothing on the smaller public projects. That is kept
(`tsrs_core::hugealloc`); the rest is measured, not kept.

## Method: run fresh copies

A binary run from the path cargo linked it to (`target/release/tsrs`) can have 2.5x to 50x the instruction-TLB misses
of a `cp` of the same file on these boxes, for 0.5-2.4% more cycles, by a different amount for each build (how the
linker wrote the file decides how the page cache maps its text). The first pass of this round ran binaries in place,
and its results disagreed with the fresh-copy reruns by up to about 2 points (an in-place +2.9% on vscode for the
allocator hook below is level from fresh copies). Every number below is from fresh copies unless it says otherwise.

## The evidence that started it

On main at 8 cores backend memory stalls are about a fifth of user cycles. A cycles profile put `Relation::lookup` at
5% of self time, with 57% of its samples after the control-group load and 28% after the slot load: hashbrown keeps
control bytes and slots in separate parts of its allocation, so a hit is two dependent loads. `PackedMap` probing
(mostly inlined into `instantiate_type_with_alias_worker`) has the same shape, and `ReferenceInstantiations::add`'s
rehash dereferences every entry. A page-fault profile attributed 19% of the run's faults to growing `PackedMap`
tables and 14% to growing relation tables, all on the 4 KiB pages of the mimalloc heap (`no_thp`,
notes/mem-no-thp.md), and `Relation::lookup` alone took 15% of the dTLB walks.

notes/perf-memory-traffic-32.md had concluded that padding, prefetching and hot/cold splits had nothing to win because
IPC does not fall from 1 to 32 checkers. That compares scaling, not the absolute cost of misses, and none of its
experiments changed how many cache lines a probe touches.

## Results, monorepo, fresh copies

| change (against main unless stated) | 8c CPU | 8c wall | 4c CPU | 4c wall | instructions 8c / 4c | peak RSS 8c / 4c |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| relation v1: 64-byte buckets of eight `u64` slots, 0 = empty, linear probing bucket by bucket, growth at 7/8 | +1.5% [+1.2, +2.5] * | +1.4% [+0.6, +2.2] * | +3.2% [+2.0, +3.6] * | +3.3% [+2.0, +3.7] * | −0.8% * / +0.2% * | −1.1% * / −0.4% * |
| relation v3 with Fx: dense one-byte tags as hashbrown has, each group of eight slots one aligned line, `prfm` of the line with the tag load | −0.2% [−0.9, +0.4] | −0.1% [−0.7, +0.7] | +0.3% [−0.2, +0.8] | +0.3% [−0.2, +0.8] | −0.2% * / +0.3% * | +0.0% / +0.1% |
| `PackedMap` with dense 32-bit tag words beside one-line groups of three 20-byte slots, on relation v3 with Fx | +0.7% [−0.3, +1.6] | +0.6% [−0.0, +1.7] | +0.8% [+0.2, +1.2] * | +0.7% [+0.1, +1.6] * | +1.7% * / +1.7% * | −0.6% * / +0.1% |
| steps 1-2 together (relation v3 with Fx and that `PackedMap`) | −0.3% [−0.7, +0.6] | +0.1% [−0.5, +0.5] | +1.0% [+0.1, +1.4] * | +1.1% [−0.1, +1.3] | +1.8% * / +1.9% * | −0.7% * / +0.1% |
| step 3: `ReferenceInstantiations` slots `(u32 hash, P<Type>)` | −0.4% [−1.0, −0.1] * | −0.6% [−0.8, +0.1] | −0.8% [−1.2, +0.1] | −0.6% [−1.1, +0.3] | +0.2% / −0.0% * | +0.6% * / +0.4% * |
| huge pages for steps 1-2's tables (`HugeBuf`), against steps 1-2 | −0.0% [−0.9, +0.7] | −0.4% [−0.9, +0.0] | −0.1% [−0.4, +0.5] | −0.2% [−0.8, +0.2] | +0.1% / −0.0% | −0.3% / −0.1% * |
| hashbrown tables through `HugeAlloc`, huge path compiled out | −0.3% [−0.7, +0.4] | −0.2% [−0.5, +0.8] | −0.0% [−1.3, +0.2] | −0.2% [−1.4, +0.2] | −0.0% / +0.0% | +0.0% / −0.1% |
| `HugeAlloc` huge path on, against off (same code otherwise) | −1.5% [−1.8, −0.3] * | −1.1% [−1.8, −0.2] * | −1.3% [−1.6, −0.1] * | −1.4% [−1.9, −0.1] * | −0.2% * / −0.0% | −0.7% * / −0.6% * |
| **kept: hashbrown tables through `HugeAlloc`** | −0.9% [−1.4, −0.1] * | −0.7% [−1.5, +0.1] | −0.7% [−1.7, −0.1] * | −1.2% [−1.8, −0.3] * | −0.3% * / −0.0% | −0.7% * / −0.6% * |

Public projects, `HugeAlloc` huge path on against off: vscode +0.9% [−0.2, +1.8] / +0.2% [−0.9, +1.2] CPU at 8 / 4
cores, t3code-server +1.2% [−0.6, +2.6] / +0.1% [−1.1, +2.2], peak RSS within ±0.2%: inside noise, as expected for
programs whose tables mostly stay under 2 MiB.

The kept change against main on the public projects (fresh copies):

| project | 8c CPU | 8c wall | 4c CPU | 4c wall | instructions 8c / 4c | peak RSS 8c / 4c |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| vscode | +0.4% [−1.4, +1.5] | +0.2% [−1.7, +1.3] | −0.5% [−1.7, +1.6] | −0.6% [−1.8, +2.2] | −0.0% / +0.0% | +0.1% / −0.2% |
| mui-docs | +3.1% [−4.7, +6.8] | +0.6% [−3.4, +4.3] | −0.2% [−1.0, +0.6] | −0.5% [−1.2, +1.1] | +0.1% / +0.0% | +0.3% / +0.1% |
| supabase-studio | −0.2% [−1.4, +1.3] | −0.4% [−1.1, +1.5] | +0.3% [−1.0, +2.1] | +0.1% [−0.9, +1.6] | −0.0% / −0.1% | +0.0% / −0.1% |
| t3code-server | −0.4% [−1.2, +1.7] | +1.0% [−2.4, +2.3] | +0.2% [−1.7, +1.1] | +0.6% [−2.2, +2.1] | −0.1% / −0.0% | −0.2% / −0.1% |
| mikro-orm | +0.1% [−1.4, +1.0] | +0.0% [−1.1, +0.9] | −1.1% [−2.0, +0.3] | −1.7% [−2.2, +0.1] | −0.0% / −0.0% | −0.5% / −1.0% * |

### Step 1: the relation cache's pair table

`Relation::pairs` is a `hashbrown::HashTable<u64>` whose slot is the packed key shifted left by 6 with the result bits
below. A packed key is never 0: type ids start at 1 (`type_count` is incremented before use; only the checker's
placeholder type has id 0). Nothing iterates the table.

Why one line per probe loses: an instrumented build of v1 (8 checkers) counted 56% hits and 44% misses among the pair
lookups, most of them in the largest tables; a hit read 1.35 bucket lines on average and a miss 1.53. hashbrown answers
a miss from its control bytes alone, an eighth of the table's size and so far more often still cached; a one-line
bucket brings in a whole slot line for every miss. The profile's two dependent loads are real for hits, but for the
44% of misses the one-line layout trades an often-cached control load for an uncached slot line. v3 keeps hashbrown's
dense tags and only puts each group's slots in one aligned line; with hashbrown's hash it is level with hashbrown.

Two lessons that cost most of the first pass:

- **The pair hash matters as much as the layout.** v1-v3 first used a folded multiply (`lo ^ hi` of
  `key * 0x9e3779b97f4a7c15`). Measured in place only (not rerun from fresh copies, so good to a couple of points),
  hashbrown with that hash instead of Fx was +2.3% / +3.4% CPU at 8 / 4 cores, and with murmur3's `fmix64` +3.0% /
  +4.3%. The pair keys are built from type ids handed out in sequence, and Fx, a plain multiply, spreads such keys
  more evenly than a random-looking hash does; neither hash gives related keys any cache locality (checked on runs of
  keys sharing a source or a target).
- **The prefetch paid only on the badly hashed table.** In place, prefetching the slot line with the tag load was
  -1.6% / -1.4% CPU on v3 with the folded multiply, and -0.5% / +1.0% on v3 with Fx.

Also measured in place only and not kept: v2 (v1 with a branchless eight-slot compare, a key-derived odd probe step
and an overflow flag per bucket in result bit 2 of its last slot) was slower than v1, +4.3% / +4.3% CPU against main.

### Step 2: `PackedMap`

`PackedMap` wraps a `hashbrown::HashTable` of 20-byte `(CacheHashKey, P<Type>)` slots. Both variants kept its API
(`from_one`, `get`, `insert` returning the old value, `len`, `is_empty`, `capacity`, `clear` keeping storage, `Clone`;
`values` was already gone from main, removed as unused in #813), allocated nothing when empty, and kept `GoPackedMap`
and `Based<PackedMap>` working with `--features tsrs_core/shared-graph`. The dense-tag form (table above) costs 1.7%
more instructions and loses at 4 cores. The single-line form (three slots plus a count byte and an overflow flag per
64-byte bucket, tables of one or two buckets filled completely, larger ones to 7/8) was +2.7% / +2.1% CPU on top of
step 1 v2, in place: its `clear` has to visit every bucket of the per-mapper caches that `pop_active_mapper` clears
constantly, where hashbrown's is a memset of the control bytes.

### Step 3: `ReferenceInstantiations`

Candidate A2 of notes/perf-memory-traffic-32.md, at 8 and 4 cores: growth rehashes from the slots, and a lookup reads
a reference's argument list only on a 32-bit hash match. A few tenths of a percent of CPU, inside noise on three of
the four time cells, for +0.4-0.6% peak RSS. Not kept: the round's bar was no increase in peak memory.

## Kept: huge pages for the big relation and `PackedMap` tables

`tsrs_core::hugealloc::HugeAlloc` is an `allocator-api2` allocator (hashbrown's `allocator-api2` feature) that
`Relation::pairs` and `PackedMap` pass to `HashTable::new_in`. An allocation of 2 MiB or more, on Linux with compressed
handles, is a chunk of the arena reservation (`reserve::alloc_table_chunk`, sharing `carve` with `alloc_chunk`): 2 MiB
aligned, its whole 2 MiB blocks advised `MADV_HUGEPAGE` and the part past the last whole block left on 4 KiB pages,
since hashbrown's allocation (slots, control bytes, a group-width tail) is never a multiple of 2 MiB. Growth gives the
old chunk back with `reserve::release_chunk`, which unmaps it at once. Everything smaller, and every other platform,
goes to `Global` (mimalloc), and which way an allocation went follows from its layout, which `deallocate` gets back.
Same contents and lookups, so output cannot change; nothing iterates these tables in an order that reaches output.
Checked: `--pretty false` diagnostics byte-identical to main on the monorepo, vscode, mui-docs, supabase-studio,
t3code-server and mikro-orm, in the default mode and with `--fast`, single-threaded and at 4 and 8 checkers; the
default-mode single-threaded `--extendedDiagnostics` Types, Symbols and Instantiations counters identical;
`tools/regressions.sh` (on Linux, where the huge path runs) and the conformance gate pass.

This is not the "huge pages only for allocations of 2 MiB or more: no gain" of notes/perf-fast-heap-thp.md (branch
`perf/fast-heap-thp`), which put every large mimalloc allocation on huge pages at 16 checkers. Here only two kinds of
table get them, so there is no partly used huge page of unrelated data and no per-thread slack; peak RSS falls a
little, plausibly because a grown table's old chunk is unmapped at once rather than after mimalloc's purge delay.

`perf stat` at 8 cores on the monorepo, in place (two runs each): huge pages cut dTLB walks by 21% and page faults by
15%. On steps 1-2's tables they cut walks by 32% and faults by 17%, but those tables made 26% more walks than
hashbrown's to begin with. The exit teardown was not measured.

## Not done

Step 4 (the same shape in `SymbolMap`, the string-literal map and the link stores) was conditional on steps 1-3
paying, and they did not.

## What would be worth trying next

- The pair hash moves the check by 2-4%; Fx is the best of the three tried. Hashes that use the structure of the keys
  on purpose (for instance keeping the target id's low bits in the bucket index) are untried.
- `HugeAlloc` for the other tables that reach 2 MiB on large programs (`StringLiteralTypes`, `ReferenceInstantiations`,
  the `FxHashMap` caches of `checker.rs` would need `hashbrown::HashMap` with an allocator).

## Reproduce

The variants not kept are not on any branch; each is a change of a few dozen lines to `Relation::lookup` / `set`
(relater_1.rs), `packedmap.rs` or `ReferenceInstantiations` (types.rs), described above.

```sh
# per arm, on the bench box
git -C ~/src/tsrs worktree add --detach ~/<arm> <sha>
(cd ~/<arm> && cargo build --release --locked -p tsrs_cli)
mkdir -p ~/bin-fresh && cp ~/<arm>/target/release/tsrs ~/bin-fresh/<arm> && sync
# runner config: one workload per project and core count, argv
#   taskset -c 0-7 ~/bin-fresh/<arm> -p tsconfig.json --noEmit --incremental false --pretty false --fast
node runner.mjs cfg.json res.jsonl --box a && node analyze.mjs res.jsonl
perf stat -e cycles:u,instructions:u,stall_backend_mem:u,l2d_cache_lmiss_rd:u,dtlb_walk:u,page-faults:u -- taskset -c 0-7 ...
```
