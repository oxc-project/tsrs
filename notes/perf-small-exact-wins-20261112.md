# Four small exact changes from one `--fast` profile (2026-11-12)

Measured 2026-11-12 at `221ee7bf` (main) on one m8g.4xlarge (16 Graviton4 vCPUs, 64 GiB). The evidence was one `perf
record` profile (cycles, frame pointers) of `--fast` on a large private monorepo at 8 checkers on 8 cores
(`taskset -c 0-7`), plain release build. Each candidate was a few tenths of a percent of self time. Each was built as
its own commit on top of the previous one and measured against its parent. Items 1, 2 and 4 are kept; item 3 is
dropped.

| item | profile share (self) | kept | single-threaded instructions, change against parent (default / `--fast`) |
| --- | --- | --- | --- |
| 1. mimalloc's plain entry points | `mi_malloc_aligned` 0.70%, `_mi_page_malloc_zero` 0.92%, `mi_free` 0.64%, `_mi_theap_realloc_zero` 0.50% | yes | vscode -1.10 / -1.14%, mikro-orm -1.63 / -1.69%, mui-docs -1.16 / -1.18%, supabase-studio -1.54 / -1.54% |
| 2. `is_static_private_identifier_property` from the name | 0.41% | yes, marginal | vscode -0.038 / -0.037%, mikro-orm -0.047 / -0.044%, mui-docs +0.019 / +0.005%, supabase-studio -0.016 / -0.014% |
| 3. `is_deprecated_symbol`: merged parent only when read | 0.36% plus callees | no | vscode +0.029 / +0.033%, mikro-orm +0.002 / +0.006%, mui-docs +0.008 / +0.009%, supabase-studio +0.018 / +0.013% |
| 4. fresh symbol ids without a compare-and-swap | `__aarch64_cas4_relax` 0.42% | yes | vscode -0.101 / -0.035%, mikro-orm -0.168 / -0.109%, mui-docs -0.107 / -0.101%, supabase-studio -0.087 / -0.046% (on top of items 1 and 2) |

Instructions are `perf stat -e instructions:u` of `--singleThreaded` with `RAYON_NUM_THREADS=1` on one core
(`--noEmit --incremental false --pretty false`), averaged over two runs per arm; the two runs of an arm differ by
0.0003-0.024% (most under 0.01%).

## Exactness

For every arm, the `--pretty false` diagnostics were byte-identical to main:
- single-threaded, in default mode and `--fast`, on vscode, mikro-orm, mui-docs, supabase-studio and the private app;
- at 4 and 8 checkers in both modes, comparing main against the stack with all four changes and against the kept
  stack.

The default-mode single-threaded `--extendedDiagnostics` counters were also identical; only the timing lines differ.

## 1. mimalloc's plain entry points (kept)

`mimalloc_safe::MiMalloc` called `mi_malloc_aligned` / `mi_zalloc_aligned` / `mi_realloc_aligned` for every request.
In mimalloc v3 the aligned small-block path checks the alignment, looks up the page, and tests the free block's
address before it reaches the same `_mi_page_malloc_zero` that `mi_malloc` inlines. Allocations above the small size
and every realloc with 16-byte alignment go through `mi_malloc_is_naturally_aligned` / `mi_good_size` first. Rust
std's unix `System` allocator and Bun's allocator shim call the plain function when `align <= MIN_ALIGN && align <=
size`.

`tsrs_core::allocator::MiMalloc` (feature `mimalloc`) does the same with mimalloc's own bound, `MI_MAX_ALIGN_SIZE`
(16). It is the global allocator of every binary that set one: tsrs, tsrs-test, tsrs-lint, the bench harnesses, the
CLI's integration tests and the AST oracle. The binaries now depend on `libmimalloc-sys2` (through tsrs_core) with
the same `v3` and `no_thp` features instead of `mimalloc-safe`. `init_mimalloc` and the THP policy are unchanged.

Why the plain functions give the same blocks (mimalloc v3.6 source, `libmimalloc-sys2` 0.1.65):

- `mi_bin` (`page-queue.c`) rounds sizes to two machine words on 64-bit (`MI_ALIGN2W`) and to four on wasm32
  (`MI_ALIGN4W`), except the one-word class. Every size class of at least 9 bytes on 64-bit, and of more than 8 bytes
  on wasm32, is a multiple of 16.
- Page blocks start at least `MI_MAX_ALIGN_SIZE`-aligned (`MI_PAGE_MIN_START_BLOCK_ALIGN`;
  `mi_natural_alignment_of_size`). `_mi_page_malloc_zero` asserts `block_size < 16 || aligned(block, 16)`.
- So a block for `size >= align` is `align`-aligned for every `align <= 16`. A smaller size gets the word-sized class,
  which only `align <= size <= 8` (64-bit) or `<= 4` (wasm32) requests reach.
- The aligned functions pick the same size class (`mi_theap_get_free_small_page(theap, size)`), and above the small
  size they call `_mi_theap_malloc_zero` themselves whenever the size is naturally aligned. Same bins, same pages:
  only the path changes, so memory use cannot change.

tsrs_wasm does not use mimalloc (it keeps Rust's wasm allocator), so the wasm32 bound only matters if that changes.

Paired runs (B = item 1, A = main), private app, 8 cores:

| metric | B vs A, 10 pairs |
| --- | ---: |
| instructions | -1.2% [-1.8%, -0.9%] |
| cycles | -1.0% [-7.2%, +3.4%] |
| CPU time | -1.0% [-7.1%, +1.2%] |
| wall | -1.2% [-6.0%, +2.3%] |
| peak RSS | -0.0% [-0.7%, +0.1%] |

The timings at 8 cores on this box varied by 4-5% from run to run during these item runs, so only the instruction
change is resolved. Peak RSS does not move, as expected from the argument above; the combined runs below show the
same on every project and core count.

## 2. `is_static_private_identifier_property` (kept, marginal)

The relater's property loops (`relater_1.rs`, `checker_09.rs`, `checker_10.rs`) call it per property. It read the
value declaration, a cold node, to answer a question that is almost always no. The new pre-test: when the symbol's
name does not start with the internal prefix (U+007F), return false without reading the node.

Why the pre-test is exact:

- A private-identifier class element binds under an internal name:
  - `get_symbol_name_for_private_identifier`, `"\x7f#<class id>@#name"`;
  - or `INTERNAL_SYMBOL_NAME_MISSING` when a parse error left it outside a class (`binder.rs`
    `get_declaration_name`).
- Every checker symbol that gets such a value declaration keeps the name of the symbol it came from:
  - instantiated symbols share the original's data, name included;
  - union and intersection properties take the looked-up name, and each constituent property found under it;
  - clones, `create_symbol_with_type`, object-literal members, JSX attributes and merges copy the name;
  - mapped members never set a value declaration (they link declarations only for non-remapping mapped types, where
    the name is the key's own);
  - string-literal objects in inference take an enum literal's declaration.

So a name without the prefix can only say no when the full test would.

The instruction change is small and mixed: -0.04% on vscode and mikro-orm, -0.015% on supabase-studio, +0.019% /
+0.005% on mui-docs. The point of the change is a cache miss per property, which instructions do not count. Paired
runs, private app, 8 cores:

| metric | B vs A, 10 pairs |
| --- | ---: |
| instructions | -0.4% [-0.6%, -0.2%] |
| cycles | +0.9% [-10.7%, +7.4%] |
| peak RSS | -0.1% [-0.5%, -0.0%] |

The 8-core timing is noise here (the run had lower IPC on both arms throughout: a busier host). Single-threaded on
mikro-orm (`--fast`, 10 pairs): instructions -0.05% [-0.05%, -0.04%], cycles +1.0% [-4.7%, +14.6%]. That run also had 4% run-to-run spread, so cycles are inside the noise. Kept on the instruction evidence (three of four projects lower, and -0.4% on the private app at 8 cores) and because it is exact and small. It is the first to drop if a later measurement on a quieter box shows nothing.

## 3. `is_deprecated_symbol` (dropped)

The earlier finding stands (notes/perf-checker-single-base-flag.md): skipping the deprecation path when nothing reads
suggestion diagnostics is not exact. `get_parent_of_symbol` can start late binding, and the language server, the API
and lint read suggestions. There is also a second reason: `resolve_alias_with_deprecation_check` returns the
unresolved alias when the alias itself is deprecated. The answer steers control flow, not only suggestions.

A per-symbol memo was not built. The natural home, `ValueSymbolLinks`, is packed to 16 bytes (24 with plain pointers)
and has no spare bits. A field would raise peak memory on every value symbol, and a side table would add a hash lookup
per call, about what the call costs. What was built instead: look up the merged parent only for symbols with several
declarations, the only case that reads it. Every other symbol still calls `get_late_bound_symbol(parent)` for its
side effect, and only the `merged_symbols` lookup is skipped. That was exact but added 0.002-0.033% instructions on
every project and mode, so it was dropped.

## 4. Ids for symbols the checker just created (kept)

The first link lookup of a new checker symbol (`value_symbol_links.get` right after `new_symbol` /
`new_shared_data_symbol`) assigned its id with a compare-and-swap. On arm64 that is the outlined
`__aarch64_cas4_relax`. No other thread can see such a symbol yet: checker symbols are private to their checker, and
only shared-graph seed symbols are published, which `new_symbol` / `new_shared_data_symbol` give an id at creation in
seed mode.

`SymbolArenaLinkStore::get_fresh` (24 sites) calls `SymbolIdGenerator::get_fresh_symbol_id`. That takes the next id
from the same block (`next_block_id`) at the same point and writes it with a plain relaxed store. A symbol that already
has an id returns it, exactly as before. So the sequence of ids each checker hands out is unchanged (unique-symbol
names embed them). `cargo check -p tsrs_cli -p tsrs_core --features tsrs_core/shared-graph` passes.

Paired runs, private app, 8 cores:

| metric | B vs A (item 4 on top of items 1 and 2), 10 pairs |
| --- | ---: |
| instructions | -0.1% [-0.8%, +0.7%] |
| cycles | +2.0% [-2.1%, +15.3%] |
| peak RSS | +0.0% [-0.4%, +0.3%] |

Inside the noise at 8 cores. Single-threaded on mikro-orm (`--fast`, 10 pairs): instructions -0.10% [-0.10%, -0.10%], cycles +3.2% [-3.4%, +19.1%], with the same 4% run-to-run spread, so cycles are inside the noise. Kept on the instruction change, which is lower on every project in both modes, and because the change removes an atomic read-modify-write without adding work.

## Combined (items 1, 2 and 4) against main

Single-threaded instructions:

| project | default | `--fast` |
| --- | ---: | ---: |
| vscode | -1.24% | -1.22% |
| mikro-orm | -1.84% | -1.84% |
| mui-docs | -1.25% | -1.27% |
| supabase-studio | -1.64% | -1.60% |

15 alternating pairs, `--fast`, B = kept stack (`8ae39918`), A = main; median of per-pair ratios with a ~95% interval:

| workload | CPU time | wall | cycles | instructions | peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| private app, 8 cores | -1.4% [-3.4%, +0.2%] | -1.7% [-3.4%, -0.6%] | -1.4% [-3.4%, +0.5%] | -0.9% [-1.2%, -0.5%] | -0.0% [-0.1%, +0.3%] |
| private app, 4 cores | -0.9% [-2.1%, -0.4%] | -1.1% [-2.1%, -0.4%] | -0.9% [-2.2%, +1.3%] | -1.2% [-1.2%, -1.1%] | +0.1% [-0.2%, +0.2%] |
| vscode, 8 cores | -1.7% [-3.9%, +1.1%] | -2.0% [-3.5%, +0.8%] | -1.9% [-4.0%, +1.3%] | -1.2% [-1.3%, -1.2%] | +0.1% [-0.2%, +0.3%] |
| vscode, 4 cores | -0.6% [-2.5%, +0.8%] | -0.3% [-2.1%, +1.1%] | -0.7% [-2.5%, +1.3%] | -1.2% [-1.2%, -1.2%] | +0.2% [-0.3%, +0.4%] |
| mikro-orm, 8 cores | -2.2% [-2.5%, -0.5%] | -1.9% [-2.7%, -0.2%] | -2.4% [-2.6%, -0.5%] | -1.7% [-2.1%, -1.4%] | +0.3% [-0.2%, +1.0%] |
| mikro-orm, 4 cores | -1.0% [-5.2%, +0.7%] | -1.2% [-4.8%, +1.8%] | -1.0% [-5.3%, +0.9%] | -1.8% [-2.0%, -1.8%] | +0.3% [-0.6%, +0.9%] |

Every median is lower in time, and the intervals exclude zero for wall on the private app at both core counts, CPU
time on the private app at 4 cores, and all three timing metrics for mikro-orm at 8 cores; the rest is inside the
noise. Peak RSS: every interval includes zero (medians -0.0% to +0.3%), with nothing in the changes that can allocate
more (item 1 hands out the same mimalloc blocks; items 2 and 4 allocate nothing).

(`--fast` with its own checker count, on 8 cores, `taskset -c 0-7`, or 4, `taskset -c 0-3`.)
