# Compact Unicode casing table generator

```sh
cargo run -p tsrs_generate_stringutil
cargo run -p tsrs_generate_stringutil -- --check
cargo test -p tsrs_generate_stringutil
```

The Rust task downloads `@unicode/unicode-15.1.0@1.6.17` directly from the npm registry, verifies its pinned SHA-512
integrity, and caches the archive in `target/generate-stringutil/`. It decodes the package's compressed maps and
property ranges without executing JavaScript. Node, npm, and a TypeScript checkout are not required.

The default command regenerates `crates/tsrs_core/src/stringutil/js_case_generated.rs`, formatted with rustfmt.
`--check` checks the saved output without writing. Subsequent runs use the cached archive.

The casing rules follow TypeScript's `generate-unicode-data.mts`: special mappings override simple mappings,
with final sigma as the only contextual mapping. Cased and Case_Ignorable properties use the same Unicode version.
Sorted 16-byte records contain three `u16` offsets and three `u8` lengths into a deduplicated UTF-8 pool. Generation
validates scalar values, offset and length bounds, and every string's round trip through the pool.
The compiler uses checked byte slices from this table for its non-ASCII casing mappings. The Rust checks verify
all mappings and property ranges against a semantic digest of the original pinned table. The core casing checks
cover contextual sigma, multi-character mappings, and lone surrogates. `tools/gen-check.sh` invokes this task.
