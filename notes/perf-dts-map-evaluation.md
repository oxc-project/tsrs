# Declaration bundler map evaluation (2026-10-26)

Baseline: `3231a0b00338c12d4d2eded59b0ef3b2dd2f9d9d`.

## Scope and ordering audit

The evaluated change replaces five private Graph tables: `references`,
`inline_imports`, `external_specifiers`, `owners`, and `binding_names`.
Their consumers use insert, entry, get, index, contains_key, remove and move/drop;
none iterates their keys or values. The `owners` value vectors retain their
original ordering independently of the containing map. The public APIs do not
expose these private tables.

Three variants use the same algorithms and workload:

- BTreeMap: current key-ordered tree.
- FxHashMap: rustc-hash 2 from the workspace, numeric identity keys, no iteration.
- IndexMap: `tsrs_core::collections::OrderedMap`, using FxBuildHasher and insertion
  order. The experiment uses `shift_remove` for the alias cleanup and explicit
  dereferencing at two comparator key lookups to satisfy its Equivalent API.

Keep the following BTreeMaps in this evaluation:

| Tables | Reason |
| --- | --- |
| Files and DeclarationFs.files | Public representation; file order establishes graph input order; filesystem overlay uses prefix range queries. |
| Public entry maps | Public representation and ordered processing of entries. |
| Graph imports | Iterated for reachability and emitted import order. |
| Graph anonymous | Iteration participates in collision-sensitive naming. |
| Graph module_symbols | Iterated to generate module namespaces. |
| Statement dependency_meanings | Iteration feeds the reachability work stack; preserve existing order. |
| Output names | Sorted later, but the existing tree order also breaks ties through stable sorting. |
| Directive deduplication | Values are printed in key order. |

Other lookup-only candidates are reachable's temporary `meanings`, output
`shared_exports`, and OutputPrinter `maps` and `lines`. They are not part of this
measurement; some contain only a few entries. `namespace_children` key iteration
feeds a set rather than text directly, but it is also left unchanged.

A subsequent consumer audit confirms that `reachable.meanings` uses only `entry`
and that `shared_exports` uses insertion, length/emptiness and keyed retrieval.
These are good candidates for the next separate experiment because they can grow
with retained symbols. OutputPrinter `lines` is also lookup-only, but usually has
few live buffers; a tiny Vec or its current tree may beat hashing there. Its
`maps` table is constructed once and queried by source path. None has measured
benefit yet. Membership-only sets such as `live_nodes`, `namespace_blocks` and
cycle guards are separate candidates; keep `live.statements`, `live.symbols` and
`live.modules` ordered because their traversal feeds printing or name creation.

A further audit found unused alias bookkeeping: Statement.imported_aliases is
collected into Live.aliases, and multi-entry linking merges that set, but output
and counters never read it. Removing this storage is a separate candidate, not a
measured improvement. Collection calls ast::get_symbol_id, which can assign IDs;
removing that side effect could affect later naming order. First preserve ID
assignment while testing removal of the sets themselves.

Other workload-specific candidates are borrowed link-cache comparison and shared
result ownership for declaration-unchanged rebuilds, and normalizing source paths
once per source index for mapped output. Neither has been implemented or measured.
Tiny maps and cycle guards need their own measurements; hashing has setup and
capacity costs and is not automatically faster or smaller. The subsequent direct
export-key lookup trial was rejected for API inequivalence on merged module
exports; see `perf-dts-export-lookup.md`. Its barrel benchmark gain is not retained
and does not alter the five-table recommendation.

The requirement is ordered behavior, not necessarily a BTreeMap implementation.
Hash storage plus an explicit sort can preserve it, but adds sorting and temporary
storage. IndexMap preserves insertion order, which differs from the existing key
order. For Output `names`, a replacement must preserve the stable sort's existing
tie order explicitly. Filesystem prefix queries additionally need an index or a
full scan if the ordered file map is removed. Public map types also form an API
boundary, so changing them is a separate design decision.

A directly indexed Vec is not a drop-in alternative: compiler SymbolIds come
from a process-wide/block ID allocator, and P<Node> handles identify arena
locations. Neither is a compact index in this bundle. A Vec design would need a
bundle-local remapping. A sorted Vec would need an insertion/finalization strategy
for the interleaved inserts, queries and removals during graph analysis.
The repository disallows std HashMap and already uses Fx hashing for internal
compiler identity maps.

## Results and recommendation

Recommend FxHashMap for the five tested lookup-only tables. Keep BTreeMap for
ordered traversal and prefix queries. IndexMap also works, but insertion order
has no consumer here; it offers no consistent advantage over FxHashMap and tends
to retain more memory. This recommendation is deliberately limited to the five
measured tables rather than a wholesale conversion.

Second run after the concurrent release/test work finished: medians of seven
rounds, percentage changes relative to BTreeMap. Wall time includes process
launch/teardown, measured at high resolution around system time. Peak RSS is
process memory, including compiler checking, not just map storage.

| Declarations / workload | Fx instructions | Index instructions | Fx RSS | Index RSS | Fx wall | Index wall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 960 / single entry | -0.22% | -0.21% | +0.41% | +0.14% | +0.12% | -0.24% |
| 960 / multiple entries | -0.91% | -0.80% | +0.75% | +1.05% | -2.34% | -2.18% |
| 960 / maps | -0.76% | -0.74% | +0.98% | +1.11% | -1.20% | -1.40% |
| 9,600 / single entry | -0.18% | -0.16% | -0.19% | +0.81% | -0.21% | -0.25% |
| 9,600 / multiple entries | -3.80% | -3.66% | -0.21% | -0.23% | -5.44% | -4.99% |
| 9,600 / maps | -1.45% | -1.33% | +0.34% | +0.97% | -2.99% | -3.01% |

Absolute counts and memory for the repeat run:

| Declarations / workload | BTree instructions / MiB | Fx instructions / MiB | Index instructions / MiB |
| --- | ---: | ---: | ---: |
| 960 / single entry | 1,090,212,951 / 46.14 | 1,087,777,214 / 46.33 | 1,087,886,999 / 46.20 |
| 960 / multiple entries | 861,590,860 / 46.14 | 853,743,028 / 46.48 | 854,722,037 / 46.63 |
| 960 / maps | 1,109,140,928 / 46.39 | 1,100,719,746 / 46.84 | 1,100,932,008 / 46.91 |
| 9,600 / single entry | 20,985,573,580 / 73.14 | 20,947,473,432 / 73.00 | 20,951,758,118 / 73.73 |
| 9,600 / multiple entries | 1,605,950,681 / 75.14 | 1,544,969,511 / 74.98 | 1,547,204,526 / 74.97 |
| 9,600 / maps | 4,075,877,390 / 77.72 | 4,016,576,002 / 77.98 | 4,021,480,949 / 78.47 |

Both alternatives reduce instructions in all seven paired rounds of each repeat
workload. The first seven-round run also found savings in every workload median:
Fx -0.10% to -4.03%, Index -0.14% to -3.42%. The first large multi-entry RSS was
+0.51% for Fx and +1.42% for Index, whereas the repeat was slightly below baseline;
there is no established memory reduction. Small-workload RSS rises by a few
hundred KiB. The first large single-entry wall medians were +8.27% / +4.27% during
other activity and became -0.21% / -0.25% on repeat, so do not infer a single-entry
speedup. The stronger result is reduced instructions for larger multiple-entry
bundles, with a small memory tradeoff on several cases.

The synthetic workload primarily stresses references, owners and binding_names;
the full fixture suite validates inline_imports and external_specifiers, but does
not provide isolated performance attribution for them. External real-world
bundler inputs and full compiler conformance were not run. The compiler's normal
check-only benchmark catalog does not invoke this crate and cannot measure this
change. These measurements do not establish whole-catalog compiler gains.

## Method

macOS arm64, equal release builds: opt-level 3, fat LTO, one codegen unit,
debug symbols enabled and stripping disabled. Retired instructions and peak RSS
come from `/usr/bin/time -l`; a baseline Time Profiler recording is retained.

Use the same probe as `perf-dts-allocations.md`, with four captured modules and
120 or 1,200 interfaces plus exported const declarations per module (960 or
9,600 declarations). Workloads: a single barrel entry, four entries with shared
output, and the same multiple entries with declaration maps. Every process links
once and hashes every output path and byte. One warmup plus seven measured rounds
rotate the three variant orders. The linker forces one checker internally.
These are synthetic bundler inputs, not the compiler's check-only catalog.

The existing corpus also exercises anonymous declarations, inline imports,
external packages, namespaces, type-only exports and cycles. Baseline, FxHashMap
and IndexMap each pass all 258 bundler tests across six suites. Both alternatives
preserve all 139 captured artifact paths and contents byte for byte. Focused
Clippy with warnings denied passes for both variants, including the prohibition
on unordered hash-map iteration. After the evaluation, the five FxHashMap
replacements were applied locally at the user's request. Ordered maps elsewhere
remain unchanged; the public API and compiler algorithms are unchanged.

Validation of the applied change: all 258 bundler tests pass again and all 139
captured artifacts remain byte-identical. Full workspace Clippy with warnings
denied, source checks, formatting and the dev compiler CLI/runner build pass.
The plain-ptrs configuration compiles and all nine link tests pass with plain
node-pointer hash keys.

Raw scripts, baseline sources, binaries, profile and measurements are retained
under `target/scratch/dts-maps/`; the release build target directory is
`target/dts-alloc-release`. The variant generator changes only the five tables,
their initializers, the direct dependency needed by FxHashMap, and the two
IndexMap-specific call adaptations.

To repeat the experiment locally, run `node target/scratch/dts-maps/variant.mjs`
with `fx`, `index` or `btree`, then format/check and build the probe using:

```sh
cargo build --release --manifest-path target/scratch/dts-alloc/Cargo.toml --offline --target-dir target/dts-alloc-release
```

Copy the resulting `dts-alloc-probe` to the variant name under
`target/scratch/dts-maps/`, and run `node target/scratch/dts-maps/measure.mjs`.
Baseline source copies and binary SHA-256 hashes are retained with the results.
The generator is a local experiment helper, not repository tooling.
