# Borrowed resolved-module lookup

Measured 2026-10-21 against `03415780`. Prior work consulted: `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md`'s rejected list, `notes/mem-round4.md`,
`notes/perf-frontend-64.md`, and `notes/perf-symbol-map-lookups.md`.

## Finding and change

`Program::get_resolved_module_by_path` receives a borrowed module name, but each per-file
`ModeAwareCache` key held an arena-backed `&'static str`. The implementation therefore used
`HashMap::iter().find(...)`, scanning occupied buckets and comparing names instead of using the map's index.

Two clean single-threaded MUI Docs Time Profiler traces sampled that scan directly in
`get_resolved_module`: 54 of 2,809 samples (1.92%) and 59 of 2,747 (2.15%). `llvm-objdump` placed every leaf
sample in the hash-table bucket walk or its name comparison. The profiled function and cache sources at
`ee4c520c` are byte-identical to `03415780`.

The cache now uses `hashbrown::HashMap` with the existing `FxBuildHasher` and key. A temporary borrowed query
hashes the same `(name, mode)` fields and implements `Equivalent<ModeAwareCacheKey>`, so lookup is one hash probe
without allocating or extending the name's lifetime. The language-service alias resolver uses the same borrowed
lookup and now allocates its arena key only on a cache miss.

The symbolized candidate was recorded with:

```sh
env CARGO_PROFILE_RELEASE_STRIP=none CARGO_TARGET_DIR=target/profiling \
  cargo build --release -p tsrs_cli
xcrun xctrace record --template 'Time Profiler' \
  --env RAYON_NUM_THREADS=1 --output /tmp/resolved-module.trace --launch -- \
  "$PWD/target/profiling/release/tsrs" \
  -p "$PWD/bench/.work/solutions/mui-docs/docs" \
  --noEmit --incremental false --pretty false --singleThreaded
```

The candidate trace has 2 of 2,755 samples (0.07%) in `get_resolved_module`, a 96-97% reduction in that frame.

## Measurement

Clean fat-LTO release builds, the pinned public projects, `RAYON_NUM_THREADS=1`, and `--singleThreaded --noEmit
--incremental false --pretty false`. After a warm-up per binary, eight runs per binary were interleaved in ABBA
order. `/usr/bin/time -al` on Apple Silicon includes kernel instructions and split into two repeatable bands about
1.0-1.1 G instructions apart; both bands independently measure the same improvement, so the misleading pooled
median is not used.

| project / counter band | baseline | candidate | change |
| --- | ---: | ---: | ---: |
| MUI Docs, low band median | 37.157 G | 36.251 G | **-2.44%** |
| MUI Docs, high band median | 38.214 G | 37.296 G | **-2.40%** |
| MUI Docs, peak RSS median | 773,464,064 B | 773,480,448 B | +0.002% |
| Compiler-Unions, instructions | 3.223908 G | 3.223190 G | -0.022% |
| Compiler-Unions, peak RSS | 62,373,888 B | 62,324,736 B | -0.079% |

MUI Docs exits 0 with empty output for both binaries. Compiler-Unions output is byte-identical: both exit 2 with
7,386 bytes and SHA-256 `28ec666d70ccbf6dcbe1ab972273629599e667935274f62498f81c50893c9c43`.
MUI Docs extended-diagnostic semantic counters are identical, including 2,455,368 symbols, 826,542 types and
2,719,933 instantiations.

## Validation

- Full conformance gate: 13,458 errors / 12,779 types / 12,779 symbols.
- All 36 regression projects pass.
- `cargo test -p tsrs_module -p tsrs_checker -p tsrs_compiler -p tsrs_ls`: 134 passed, 1 ignored.
- `cargo clippy --all-targets --config 'build.warnings="deny"'` passes.
- `tools/lint/source.py` passes.
