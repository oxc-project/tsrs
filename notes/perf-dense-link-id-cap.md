# Keeping reserved checker link ids dense past 16M

The port of upstream's checker-local id generators (`944ed157`, port ledger entry 63 in
[typescript-623-ports](typescript-623-ports.md)) kept Go's dense limit for the reserved ids: `dense_link_key` sent
only the first `MAX_PAGE_LINK_COUNT = 0x100_0000` (16.7M) reserved ids to the dense tables of `NodeLinkStore`,
`NodeValueLinkStore` and `SymbolArenaLinkStore`, and every id above it to a hash map behind a cold, out-of-line
call (`IdLinkStore::wide_slot`, `InlineIdStore::wide_get`, `NodeValueLinkStore`'s `ordinary` map). The reserved
ids come from the process-wide counters in `tsrs_ast/src/utilities_1.rs`, in blocks of 256 per checker generator,
so the count is per program, not per checker, and it grows with the checker count (every checker numbers the
symbols it touches that no other checker has numbered yet, and each leaves a partial block).

## Why the catalog cannot show it

No `bench/projects.json` project reaches the old limit. Reserved ids handed out (block-granular: the counters'
final values, so up to 255 unused ids per generator), from a temporary local patch that printed the counters at
exit:

| run | reserved node ids | reserved symbol ids |
| --- | ---: | ---: |
| vscode, default (`--singleThreaded`) | 6.12M | 7.92M |
| vscode, default | 6.12M | 9.26M |
| vscode, `--fast --checkers 8` | 6.12M | 4.86M |
| vscode, `--fast --checkers 16` | 6.12M | 5.14M |
| mui-docs, `--fast --checkers 16` | 0.24M | 5.46M |

vscode's largest is 55% of the limit. A large private monorepo passes it. There a `perf` profile under `--fast` at
8 checkers had `IdLinkStore<ValueSymbolLinks>::wide_slot` at 1.1% of all cycles, with
`IdLinkStore::create`'s hash insert and rehash on top, and 2.9% of all `dtlb_walk` samples.

## The fix

`MAX_PAGE_LINK_COUNT` is now `ast::BLOCK_ID_COUNT` (2^31): every reserved id the AST can store takes the dense
path. The ids themselves do not change. They are Go-visible (unique-symbol internal names embed them, and the node
builder counts their digits), and only the storage path a store picks for them moves. Nodes and symbols keep a
32-bit id whose high bit tags a reserved id (`BLOCK_ID_TAG`), and `next_block_id` asserts that the reserved counter
stays below 2^31, so no reserved id can fall outside the dense range. The `try_get_if_id_assigned` branches for a
reserved id past the dense range are gone: `decode_id` cannot produce one.

Go keeps its limit because its reserved counters are 64-bit and never stop in a long-running process, so a dense
index sized by the highest id would grow without bound. Here the 2^31 assert bounds the ids first. A store's index
grows to the highest id its checker has seen (by a quarter at a time for `IdLinkStore` and `InlineIdStore`):

| store | index cost | at the old limit (16.7M) | at the top of the range (2^31) |
| --- | --- | ---: | ---: |
| `SymbolArenaLinkStore` (`IdLinkStore::index`, `value_symbol_links`) | 4 bytes per 128 symbol ids | 512 KiB | 64 MiB |
| `NodeLinkStore` (`InlineIdStore::blocks`, `symbol_node_links`) | 4 bytes per 1,024 node ids | 64 KiB | 8 MiB |
| `NodeValueLinkStore` x2 (`blocks`, `type_node_links`, `signature_links`) | 4 bytes per 1,024 node ids each | 64 KiB each | 8 MiB each |

(Twice that without compressed pointers.) That is 1/32 byte per symbol id and 3/256 byte per node id per checker,
next to a symbol or node of tens of bytes, and the hash entries it replaces were 16 bytes plus control bytes per
linked id, with a doubling rehash. The cost a cap would still bound is a long-running process (the language server,
the test runner) whose counters keep growing across programs: a new checker's index spans every id handed out
before it. Reaching 64 MiB per checker takes 2^31 symbol ids, the point at which the process already stops on the
assert, so no cap is kept.

`tsrs_core::PagedLinkStore` keeps its own `MAX_PAGE_COUNT` limit and hash fallback. It is generic over arbitrary
`u64` keys (not reserved ids offset to zero, which are what make a bound possible), its index entries are 16-byte
slice references per 256 keys, and nothing in the compiler uses it outside its integration test, so there is
nothing to measure and no bound to rely on.

## Results

One `m8g.4xlarge` (16 Graviton4 vCPUs, 64 GiB), both arms on the same box: A is main `221ee7bf`, B is this change
(`723b50e7`; the final commit differs from it only in comments, tests and notes), release builds with
`--locked`. Paired ratios B vs A, median with ~95% interval, from `analyze.mjs` of the `aws-bench-box` scripts.

The private monorepo, `--noEmit --incremental false --pretty false --fast` pinned with `taskset`, 15 alternating
pairs per core count, output byte-identical:

| cores | wall | user + sys CPU | cycles | instructions | peak RSS |
| --- | --- | --- | --- | --- | --- |
| 8 | -2.2% [-2.7, -1.8] | -1.8% [-2.3, -1.4] | -1.7% [-2.3, -1.3] | -0.5% [-0.6, -0.3] | -3.6% [-3.9, -3.5] |
| 4 | -1.2% [-1.8, -1.1] | -1.2% [-1.4, -1.1] | -1.2% [-1.4, -1.0] | -0.5% [-0.5, -0.4] | -2.7% [-3.0, -2.6] |
| 16 | -3.2% [-3.7, -3.0] | -2.9% [-3.3, -2.5] | -2.9% [-3.2, -2.5] | -1.1% [-1.4, -0.6] | -5.2% [-5.5, -4.8] |

The win grows with the checker count because the number of ids past the old limit does. Peak RSS falls more than
time: the hash maps held 16-byte entries plus control bytes for every linked id past the limit and doubled on
growth, against 1/32 byte per id of dense index.

The catalog, where no id passes the old limit:

- `--extendedDiagnostics` counters (files, lines, identifiers, symbols, types, instantiations), single-threaded
  default mode: identical on vscode and mui-docs.
- `--fast` output: byte-identical on vscode (exit 2, its usual diagnostics) and mui-docs.
- Single-threaded default mode, 5 pairs: vscode 84.73G -> 84.60G instructions (-0.15%, spread 0.0%), CPU -0.2%
  [-3.8, +1.6], peak RSS 2069 -> 2066 MiB; mui-docs 33.23G -> 33.18G (-0.15%), CPU -0.5% [-2.2, +0.8], peak RSS
  850 -> 848 MiB.

The instruction drop on the catalog is not a storage change (no lookup there takes the hash path, before or after);
it is code generation. A build of main with only the constant changed retires 84.52G on vscode, -0.26%, so the
same fat-LTO build moves by a few tenths of a percent with this constant alone. The counters and output above show
that no behavior changed.

## Reproduce

```sh
cargo build --release --locked -p tsrs_cli   # once per arm, in its own worktree
cd <project> && taskset -c 0-7 <tsrs> -p tsconfig.json --noEmit --incremental false --pretty false --fast
```

For the pairs, a `runner.mjs` config with one workload per core count (`taskset -c 0-7`, `0-3`, `0-15`), 15
pairs. For the catalog, `node bench/bench.mjs prepare --projects vscode,mui-docs`, then `--singleThreaded`
(instructions, `--extendedDiagnostics`) and `--fast` (output) in each arm. The reserved-id counts came from a
throwaway patch that read `NEXT_NODE_BLOCK_ID` and `NEXT_SYMBOL_BLOCK_ID` in an `atexit` handler; it is not
committed.
