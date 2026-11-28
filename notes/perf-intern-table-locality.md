# Interning tables filed by their youngest type id (Bun's layout): not worth building in tsrs

Measured 2026-11-02 at `ce090c5a` on an 18-core Apple Silicon machine shared with other agents. Not built: the
profile below bounds what the layout can save, and the bound is under 1% of CPU on every project measured.

## The idea

oven-sh/bun#46001 (merged 2026-10-20) changed `bun check`'s global table of interned types, signatures and mappers
from one hash table in 64 shards to one small table per 64 consecutive ids. A key is filed under the youngest id it
contains, and the pointers to the small tables sit in an array in id order. The PR reports that a lookup's first
load had been a random read from a 30-60 MB table, 7-12% of all cycles on five large projects, and that the change
saved 1.6-6.8% at 8 threads (mikro-orm 6.8%, t3code 6.0%, MUI 2.8%, Supabase 2.6%, vscode 1.6%). Young ids are looked
up most, and their tables sit next to memory that was just written.

The layout would be exact in tsrs. A key returns the same value from either layout, and nothing iterates these maps
in an order that reaches output (`packedmap.rs`).

## Why tsrs has less to gain

Most of what Bun's single table held is already split in tsrs:

- Instantiations of a generic target live with that target: `ReferenceInstantiations` on interfaces and tuples, a
  `GoPackedMap` per type alias, and `object_type_instantiations` keyed by target (`checker.rs`).
- Instantiations under an active mapper go to that mapper's own cache (`active_type_mappers_caches`).
- Every checker has its own tables, so each one holds only what that checker interned.

What is left in one table per checker is `union_types`, `intersection_types`, `tuple_types`, `indexed_access_types`,
`template_literal_types` and `error_types`: `PackedMap<CacheHashKey, P<Type>>`, 20-byte slots keyed by Go's 128-bit
xxh3 hash of the type ids.

## Profile

`samply record -r 4000` of single-threaded default-mode checks (`--singleThreaded`, `RAYON_NUM_THREADS=1`, `--noEmit
--incremental false --pretty false`) of a fat-LTO `--release` build with debug information and no stripping. Each
leaf sample is weighted by its thread's CPU delta and expanded to its inlined call chain with `atos -i`. A sample in
hash-table code is charged to the first frame outside the table code, looking through the parent frame when the
table code was not inlined (`PackedMap::insert`). Thread CPU: mikro-orm 3.7 s, t3code-server 3.2 s, vscode 7.1 s.

Percent of process CPU:

| | mikro-orm | t3code-server | vscode |
| --- | ---: | ---: | ---: |
| `SymbolMap` lookups and inserts (name hashing, text comparison, index growth) | 7.54 | 6.94 | 5.64 |
| Other hash tables (relation cache, link stores, reference instantiations, ...) | 3.31 | 2.65 | 3.78 |
| All `PackedMap` probes and inserts | 2.67 | 1.33 | 0.45 |
| of which the per-checker interning tables: unions | 0.31 | 0.07 | 0.10 |
| intersections | 0.37 | 0.04 | 0.00 |
| indexed accesses, tuples, template literals | 0.07 | 0.01 | 0.02 |
| of which per-mapper caches (`instantiate_type_with_alias_worker`, push/pop/clear) | 1.01 | 0.85 | 0.11 |
| of which per-target instantiations (`get_object_type_instantiation`, conditional, signatures) | 0.83 | 0.33 | 0.18 |
| xxh3 key hashing (unchanged by any layout) | 0.38 | 0.19 | 0.12 |

The tables the layout would change cost 0.75% of mikro-orm's CPU and 0.12% of t3code-server's and vscode's. That
time includes the hash, the tag match and the key comparison as well as the first-load miss. A layout can save only
part of the miss, so the gain would be well under those numbers. At the fast preset's 16 checkers each checker's
tables are smaller still. A new table type, plus youngest-id tracking in `KeyBuilder`, is not worth a gain below
0.75%.

## What the profile points at instead

`SymbolMap` is 5.6-7.5% of CPU on all three projects: hashing names, comparing their text, and rehashing when an
index grows (notes/perf-symbol-map-lookups.md). Bun avoids that cost by keying member tables with u32 atoms from one
program-wide table that takes a lock only on insert, with a per-thread direct-mapped front (`src/sema/atom.rs`). The
earlier rejection of global interning (notes/mem-round2.md) measured a design with a lock per identifier. Bun's design
has not been measured in tsrs.

## Reproduce

```sh
CARGO_TARGET_DIR=target/prof cargo build --release -p tsrs_cli \
  --config profile.release.debug=true --config profile.release.strip=false
dsymutil target/prof/release/tsrs -o tsrs.dSYM
RAYON_NUM_THREADS=1 samply record --save-only -r 4000 -o mikro-orm.json.gz -- \
  target/prof/release/tsrs -p <mikro-orm>/tests --noEmit --incremental false --pretty false --singleThreaded
```

The attribution script (samply profile plus `atos -i` and `rustfilt`) was an ad hoc script and is not committed.
