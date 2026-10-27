# Declaration bundler

`tsrs_dts` emits and bundles declarations in memory using tsrs's parser, checker, declaration transformer,
module resolver and printer. It supports single and multiple entries, shared nominal identities, declaration
maps, project references and opt-in content mappers. The crate is unpublished and has no compiler CLI flags or
Node binding. It inherits the workspace Apache-2.0 license; imported fixture notices remain separate.

## Rust API

Use path dependencies from the same tsrs checkout:

```toml
[dependencies]
tsrs_dts = { path = "path/to/tsrs/crates/tsrs_dts" }
tsrs_vfs = { path = "path/to/tsrs/crates/tsrs_vfs" }
```

| API | Purpose |
| --- | --- |
| `build` | Compile a source project and produce one declaration bundle. |
| `build_entries` | Compile multiple entries into facades and a shared bundle. Keys are output declaration basenames; values are absolute source paths. |
| `emit_declarations` | Check a caller-owned `Program` and capture declaration text, provenance and maps. |
| `link_declarations` | Check and bundle captured declarations using their compilation context and filesystem. |
| `build_from_program` | Capture and link declarations from a caller-owned `Program`. |
| `BuildSession` | Cache owned build results while observed filesystem inputs remain unchanged; reuse linked output when declarations and linking dependencies are unchanged. |

[`BuildOptions`](../crates/tsrs_dts/src/lib.rs) supplies the absolute config and entry paths, output name,
library path, external-code permission and diagnostic-reuse flag. For embedded libraries, use
`tsrs_vfs::bundled::lib_path()` with a bundled filesystem wrapper. Callers supply a filesystem that stays stable
during each call and write the returned artifacts themselves; the pipeline performs no filesystem writes.
Compiler diagnostics fail the build as tsc-style lines (`file(line,column): error TSxxxx: message`), with file names
relative to the project directory.

[`CapturedDeclarations`](../crates/tsrs_dts/src/emit.rs) owns its strings and maps, so it can outlive the source
Program. [`DeclarationInputs`](../crates/tsrs_dts/src/link.rs) pairs that capture with a `DeclarationContext` and
a matching dependency filesystem. Caller-owned Programs must enable declaration emission, disable `noEmit` and
`noCheck`, and leave `outFile` unset. Callers also prepare referenced outputs. The crate checks and emits through
these Programs but never frees or updates them.

Private builds emit owned strings, then parse, bind and check them in a separate declaration Program. The linker
uses that Program's symbols to select reachable declarations and print the bundle. No compiler arenas, AST
pointers or symbol identities survive in build results or session caches. External type imports retain their
resolution modes, including inline `import()` types. Value imports moved to a different module format use
`.d.mts` or `.d.cts` companions to preserve package export conditions; callers must write those companions
alongside the bundle.

## Workarounds and constraints

The following adaptations preserve compiler behavior or linker semantics. Replace them only when the listed
condition holds; the evidence distinguishes compiler ownership limits, pinned upstream behavior and bundler policy.

### Build-scoped allocation lifetime

[`build_impl`](../crates/tsrs_dts/src/lib.rs) enters an outer arena region before any compilation and releases it
after both Programs and their snapshots. Allocations escaping nested checker/emit scratch scopes would otherwise
fall back to the thread's leak arena; see the ownership contract in
[`tsrs_core::arena`](../crates/tsrs_core/src/arena.rs). Remove this region when escaping allocations have owners
that reclaim them without invalidating Program or snapshot pointers. In 40 repeated debug builds of a 40-module
project, resident memory grew about 6.7 MiB per build when the region existed only with diagnostic reuse, and about
2 MiB per build with it always entered; the remaining growth is not yet attributed.

### Diagnostic ordering fallback

[`diagnostics::check`](../crates/tsrs_dts/src/diagnostics.rs) uses incremental collection to detect success, then
checks a fresh private Program if errors occur. Incremental collection caches deferred globals with file
diagnostics; ordinary checking of that same mutated Program can omit file errors. This collection behavior also
occurs in pinned Go. The `deferred_global_errors_match_fresh_checking` and
`stripped_internal_declaration_errors_match_fresh_checking` tests in
[`diagnostic_reuse.rs`](../crates/tsrs_dts/tests/integration/diagnostic_reuse.rs) cover source and emitted-declaration
errors. Remove the extra error-path check when a shared API preserves ordinary diagnostic order and checker history.

### Diagnostic reuse eligibility

Setting `reuse_diagnostics` requests reuse within one build. It requires no project references, no skipped library
checking and external code disabled. These are conservative integration gates; broadening them requires diagnostic
and output parity tests for the admitted configuration.

[`eligible`](../crates/tsrs_dts/src/diagnostics.rs) then accepts either:

- A closed project: every project file is an external module, and imports, re-exports and qualified literal import
  types resolve within the project. Namespace imports/exports, module declarations, augmentations, import-equals,
  and whole-module or nonliteral import types exclude this route. Default and external libraries are excluded from
  this project scan.
- A Program with no namespace imports, namespace exports or bare star exports anywhere in its source files.

UMD declarations anywhere in the Program disable both routes, including dependency type packages that declare
`export as namespace` (for example `@types/react`); projects using them always check without reuse. Pinned Go
omits UMD global-use dependencies and can reuse stale diagnostics across edits; the
[reference investigation](../notes/perf-dts-reuse-gates.md) records this behavior. Remove this gate only after
proving source-to-declaration diagnostic parity or updating to a reference that tracks those dependencies.
Independent bundle comparisons do not establish cross-edit invalidation.

The syntax gate preserves bundle order: snapshots change symbol initialization order, while synthesized imports
and namespaces currently depend on symbol identity. Bypassing it reorders namespace blocks in public Zod builds
([evidence](../notes/perf-dts-reuse-gates.md)). Remove it when linker output order is independent of that history.

Source and emitted declaration Programs are tested independently. Eligible source checking can still fall back to
ordinary declaration checking if emission introduces an excluded form. Direct tests in
[`eligibility.rs`](../crates/tsrs_dts/tests/integration/eligibility.rs) cover both stages; diagnostic-reuse tests cover
cycles, generated import types, maps, multiple entries and error fallback. Historical measurements remain in
[the evaluation](../notes/perf-dts-reuse-rule.md); they are not measurements of the current integration.

### Rebinding emitted declarations

Emission arenas are temporary, and the source checker cannot query synthesized declaration nodes. The second
Program in [`project.rs`](../crates/tsrs_dts/src/project.rs) gives the linker owned declarations with usable symbols.
Replace this parse/bind pass only with retained emit arenas and checker support that preserve bundle and map output.

### Content mapper cache bypass

[`BuildSession`](../crates/tsrs_dts/src/session.rs) clears its caches whenever `run_external_code` is enabled.
Mapper processes can observe state outside the tracking filesystem. Cache reuse requires a complete mapper
input/state contract; filesystem observations alone cannot establish validity.

### Printer and checker API constraints

| Adaptation | Cause and evidence | Condition for replacement |
| --- | --- | --- |
| Seed each fragment's map generator with preceding sources. | `Printer::write` caches source indices across writes. [`maps.rs`](../crates/tsrs_dts/src/maps.rs) keeps those indices valid while sharing generated names; pipeline tests cover repeated fragments and multiple outputs. | A printer API supports independent generator tables without changing names or mappings. |
| Collect literal-inclusive name reservations locally. | The compiler reservation cache is private. [`graph::reserve_names`](../crates/tsrs_dts/src/graph.rs) mirrors `SourceFile::has_identifier`; the nested-binding pipeline regression covers capture after flattening. | A shared reservation-set API provides equivalent spellings. |
| Select portable imports through package-path helpers. | The general module-specifier API can choose project aliases or relative paths. [`graph::portable_specifier`](../crates/tsrs_dts/src/graph.rs) needs package specifiers usable by consumers. | A shared query can require portable package specifiers. |
| Keep external value and side-effect imports in companions with their original module format. | Pinned Go rejects `resolution-mode` on value imports; moving them between module formats changes conditional package exports. The `external_resolution_*` [fixtures](../crates/tsrs_dts/tests/fixtures) check consumers across relocation. | A declaration representation preserves these conditions and value identity within one output file. |
| Track entry-based reachability and reference meanings. | Compiler emit visibility retains module exports; the linker also prunes by public entry and type/value usage. Shared AST predicates supply classification, but language-service meaning alone does not preserve qualifier retention. See `graph::reachable`, `reference_meaning` and the merged-meaning/namespace-pruning fixtures. | Shared queries support the same linker policy. |
| Traverse type-only star-export paths. | Aggregate checker metadata plus a selected alias can retain a type-only restriction from a competing path. [`type_only_metadata.rs`](../crates/tsrs_dts/tests/integration/type_only_metadata.rs) covers both star orders. | A compiler query supplies equivalent path-sensitive restrictions. |

### Private Program options and reference freshness

| Adaptation | Cause and evidence | Condition for replacement |
| --- | --- | --- |
| Use one checker. | The linker needs one symbol identity domain; [`project::create`](../crates/tsrs_dts/src/project.rs) forces single-threaded checking. | The linker supports identities across checkers with equivalent output and diagnostics. |
| Clear source output layout and incremental/composite state. | In-memory capture derives paths from source provenance. Inherited output directories can cause TS5011 despite no disk writes; pipeline tests supply such directories. Caller-owned Programs retain their options. | A capture API avoids irrelevant layout checks while preserving resolution and maps. |
| Force project-reference builds into a fresh overlay. | Caller filesystems need not supply timestamps. [`references.rs`](../crates/tsrs_dts/src/references.rs) supplies overlay timestamps and forces rebuilding. | A freshness contract accounts for all dependencies and outputs. |

### Fixture adaptations and coverage limits

[`fixture.rs`](../crates/tsrs_dts/tests/support/fixture.rs) adds `declare` to one exact invalid imported declaration
and supplies a missing path mapping. Keep those adapters until valid upstream inputs/configuration are available.
Frozen pnpm installs through `vp install` and [`fixture-environments.json`](../crates/tsrs_dts/tests/fixture-environments.json) define
explicit test environments; they do not establish the original dependency versions. Recovered package sources are
recorded in [`environments/provenance.json`](../crates/tsrs_dts/tests/environments/provenance.json). Virtual placement
of a recovered symlink fixture does not exercise real symlinks.

Imported fixtures come from [typack](https://github.com/Dunqing/typack) at revision
`b9e0ca6028c6dc29b11438369428f4af87201a9b`. The `tests/rolldown-plugin-dts/` corpus
retains [rolldown-plugin-dts's MIT notice](../crates/tsrs_dts/tests/LICENSE.rolldown-plugin-dts),
verified against [upstream revision `68fb437`](https://github.com/sxzz/rolldown-plugin-dts/blob/68fb437b28865b37f2ef7ea5775712f94e879445/LICENSE).
That revision identifies the license source; the original rolldown-plugin-dts fixture revision is not recorded
by the pinned typack import.
[`rolldown-policy.json`](../crates/tsrs_dts/tests/rolldown-policy.json) documents adaptations and unsupported,
invalid or unavailable cases; the harness uses `tests/support/rolldown_cases.rs` for entries, classifications and
assertions. Reclassify cases when inputs become available or support is added. A passing inventory or
expected-diagnostic test does not mean the case produced a bundle.

## Validation

Run from the workspace root. Install locked fixture dependencies on first use:

```bash
vp run test:dts --install
```

Subsequent runs use:

```bash
vp run test:dts
```

This uses the `dev` profile and compares successful fixtures across all three API paths and with diagnostic reuse
on/off. CI runs the Rust tests through workspace `cargo test --profile ci`, without these additional fixture
comparisons or the Vue oracle suite. To include Vue cases locally, first prepare the
[pinned reference checkout](../CONTRIBUTING.md#reference-compiler), then run:

```bash
vp run test:dts --suite all --install
```

The Vue runner verifies the reference pin and clean sources, rebuilds Go and Rust helpers, and compares diagnostic
code sets and exit status for six cases. The valid case also checks consumers of barrel and direct-component bundles
with both compilers. Use `vp run test:dts:vue` for Vue alone once dependencies are installed.

For a focused Rust test:

```bash
cargo test --profile dev --locked -p tsrs_dts --test integration type_only
```

Direct Cargo runs enable fixture comparisons with `TSRS_DTS_COMPARE_APIS=1` and `TSRS_DTS_COMPARE_REUSE=1`.
The `vp run` commands also work as `node tools/dts-test.mjs` with the same options.

## Adding cases

- Put local regressions in `tests/fixtures/<name>/` and register them in `tests/support/local_cases.rs`.
- Maintain imported plugin registrations in `tests/support/rolldown_cases.rs` and classifications in
  `tests/rolldown-policy.json`.
- Preserve imported notices in `tests/LICENSE.rolldown-plugin-dts` and `tests/environments/COPYING`;
  record source repositories and revisions above or in the environment provenance.

Local fixtures infer a single entry from `index.ts`, `index.d.ts`, `index.mts`, `index.cts`, `index.d.mts` or
`index.d.cts`. If none or more than one exists, set `entries` explicitly in `fixture.json`; custom and multiple
entries also require that field. Checking defaults to strict mode and NodeNext resolution. `fixture.json` supports `entries`,
`contains`, `absent`, `occurrences`, `removed`, `files`, `eachContainsAny`, `repeat`, `consumerStage: "generated"`,
`declarations` (top-level declaration count, counting variable declarators separately), `shared: true` to exercise
single-entry facades, `output` to override a single entry's declaration basename, and `nodenext: false` for
Bundler resolution. Give each case a consumer or output assertion. Consumers use `__ENTRY__` or numbered `__ENTRY0__`, `__ENTRY1__`, etc.; the harness checks original
and generated declarations unless configured for generated consumers only.

Put authored package definitions in a fixture's `environment/` or `consumer-environment/`. Historical
`snapshot*.d.ts` files are excluded from compiler inputs; checks use consumers and explicit assertions.
`TSRS_DTS_REVIEW_OUTPUT` can capture plugin-fixture outputs in a fresh directory for inspection.
