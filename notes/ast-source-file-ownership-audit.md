# SourceFile ownership and publication audit

2026-10-23. Rust base `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1`; reference verified against Cargo's
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with no tracked changes under `ts-ref/tsc`.
This covers the handwritten fields in [`SourceFile`](../crates/tsrs_ast/src/ast.rs), which the generated owned
payload currently reduces to statements and EOF. A plain metadata subset now has an isolated implementation
described below. This remains a design/input audit, not a complete production caller inventory or parser
integration. The [architecture](ast-soa-architecture.md) remains the contract.

## Retention boundaries

Use durable source information separately from a retireable body. Durable information may own plain metadata,
source bytes and text-derived maps. It must not own body leases, binding leases, node-valued caches or closures
that capture them. A body/fragment may retain durable source information. The reverse strong edge would defeat
leaf retirement and could cycle through a syntax cache.

Diagnostic ownership needs the same direction: the source information retained by a diagnostic must not own
that diagnostic list. Keep stored parse/bind records on the file entry or result side, with locations relative
to source-only identity/text information or to an output in the immutable source bundle. An exported diagnostic
can retain that source information independently. Retaining a whole file owner from a diagnostic stored on
the same owner would create a strong cycle and keep syntax alive after retirement.

The file entry owns unpublished parser/loader metadata until finalization. Binding/publication consumes that
draft; shared Programs never need `Arc::get_mut` on an already published file version. Cache hits keep exactly
the same file identity. A standalone parse result retains the corresponding phase/owner; returning a bare key
does not transfer ownership.

Identity/text are available without acquiring a live body. SourceFile-as-node is a qualified key; reading its
payload, following a node-valued indicator, using a syntax cache or traversing imports requires a body lease.
A retired file can still supply a filename/source for a retained diagnostic. This distinction must reach the
API, rather than silently making every metadata getter reacquire a body.

## Handwritten field disposition

| Current fields | Existing producer / mutation | Owned destination and access |
| --- | --- | --- |
| `node` back pointer | Factory, once | SourceFile key in its body record/handle; no self pointer or strong owner edge. |
| `parse_options`, `text` | Factory | Durable owned options and shared source bytes. Validate normalized absolute filenames at the same factory boundary. No raw text registry. |
| `content_mapper_info` | Mapper/loader before returning the version; copied by clone/update | Guarded unpublished metadata, then immutable durable mapped-file information. Keep original/transformed text separate. Cross-output links need the bundle rules below. |
| `statements`, `end_of_file_token` | Factory/reparser/transform updates | Generated immutable syntax columns. A SourceFile clone has a new key, with the reference's original/copy rules. |
| `diagnostics`, `js_diagnostics`, `jsdoc_diagnostics` | Parser finish; transform/loader append mapper errors; empty-config factory attaches read errors | Owned diagnostic records retaining durable file information for locations. Keep the diagnostic draft mutable through host/loader finalization. Late TS JSDoc callbacks currently return nodes and discard their parser diagnostics; do not start appending them to these lists. Full diagnostic migration remains required. |
| `language_variant`, `script_kind`, `is_declaration_file` | Parser finish; clone copies | Durable scalar parse results after draft finalization. Keep their current defaults for synthetic factories until callers set them. |
| `uses_uri_style_node_core_modules` | `references::collect_external_module_references` scans imports; clone copies | Durable scalar parse result. Preserve first recognized `node:`/ordinary core-module behavior and `Unknown`. |
| `identifier_count`, `node_count`, `text_count` | Parser/factory finish | Durable counters, preserving factory/speculation counting; physical live rows are a separate measurement. |
| `text_index` | Global raw source registry, parser/cache region hooks | Removed. Source descriptors qualify their range by the retaining store/source owner. This is not a semantic NodeId. |
| `imports`, `module_augmentations` | Reference collection after initial parse/reparse; clone copies | Body-owned compact node spans. Read with retained syntax. Do not put node keys into durable metadata solely because the legacy field lives on SourceFile. |
| `ambient_module_names`, `comment_directives` | Reference collection/scanner; clone copies | Owned text-only parse data; may survive body retirement if needed. Preserve source order and nil/empty API behavior. |
| `pragmas`, `referenced_files`, `type_reference_directives`, `lib_reference_directives`, `check_js_directive` | Parser pragma processing; most copied by SourceFile clone | Owned non-node parse data; strings/ranges may be durable. Preserve exact clone omissions and directive ordering. Existing `FileReference`/`Pragma` values already own their strings; remove the surrounding arena pointers/slices. |
| `hash` | Parse cache sets content hash before binding | Loader/cache draft value, then durable immutable value. Unhashed standalone/CLI paths keep zero. It must not be computed lazily by an unrelated AST read. |
| `jsdoc_cache`, `jsdoc_mu`, `has_lazy_jsdoc` | Initial parser plus synchronized on-demand TS JSDoc | Body-owned synchronized cache of immutable owned fragments. Build/freeze before publishing, retain dependencies, preserve same-key results and empty entries. The flag is parse configuration; the cache is not durable source metadata. |
| `identifiers`, `name_table` | Once-built tree walks; name table also forces JSDoc | Body-owned lazy caches. Their stored text descriptors borrow retained source/fragment owners, or own strings; never self-borrow a growable map. Name-table initialization must not hold a binding read guard across forcing. |
| `reparsed_clones` | Parser sorts by current node positions; clone factory does not copy it | Body-owned node span. Preserve source/reparsed identity mapping and sort/semantic-ID timing. |
| `lazy_lists` | Parser, then initial binder marks unbound lists | Body-owned lazy-list identities with the existing guarded deferred protocol. Preserve ParseOnly versus Deferred and dropped late diagnostics/counters. |
| `declaration_base`, `locals_container_base` | Initial binder and eligible late binding | File-qualified guarded binding annotations. No syntax writer after publication. |
| `is_bound`, `bind_once` | Initial binder | Entry state/attempt protocol, with exactly-once success and terminal failure. Retirement follows successful binding in the current protocol, so a retired file was bound; do not make a compatibility `is_bound` query accidentally return false just because its body retired. |
| `bind_diagnostics`, `symbol_count` | Initial binder only | Initial bound results; diagnostics retain durable file info. Late lazy binding drops its diagnostics and counter as today. They need not keep the entire syntax body alive. |
| `pattern_ambient_modules`, `global_exports`, `common_js_module_indicator` | Binder (CommonJS detection writes the indicator); some clone copies | Guarded file binding data/body-owned node/symbol edges. A CommonJS indicator is not an immutable parser result. |
| `external_module_indicator` | Parser module detection; factory copies; oracle recomputes | Source/job syntax annotation finalized by the appropriate exclusive writer. Forced-module self references and clone references must retain their current identity meaning. Recomputing it on a published file needs an explicit job override or a revised write phase, not shared-cell mutation. |
| `check_leaf` | CLI before checker threads start | Retirement policy on the entry/Program path, with real synchronization. Preserve CLI-only eligibility; do not let a shared Program version retire another Program's still-needed body. |
| `ecma_line_map`, `position_map` | Text-only once initialization | Durable owned `OnceLock` values using existing map algorithms, returning borrows of real owners. `ecma_line_count` still counts without allocating a map on a cache miss. No AST/body lease needed. |
| `token_cache` | LS synchronized get-or-create | Body-owned cache retaining published token fragments. Preserve `(parent, range)` identity, kind assertion and rejection of reparsed parents. Do not insert borrowed views into a growable cache. |
| `declaration_map` | Lazy tree/JSDoc walk with overload grouping | Body-owned immutable completed cache of qualified keys. Preserve insertion order/overload replacement; callers retain the cache's body dependencies. |
| `api_symbol_index` | API once callback | Body/binding-owned completed cache. Retain the actual symbol-data owner; its callback must not reenter its own initialization or keep a read guard across forcing. |

The table groups declarations, not lexical occurrence counts. The complete field-by-field production callee
audit still needs to prove the phase rules for transforms, API/codec mutation and reused-file readers.

## Real publication paths and clone behavior

`new_parse_cache` parses, writes the hash and binds before publishing a cache value. Cache eviction/revival and
both Programs retain one file-version owner. Its current region map cannot be replaced with a detached key.
The CLI host can return unbound syntax; reverse file binding order remains independent of metadata reads.

CLI `fileregions::parse` deliberately places SourceFile and text outside a scratch region. Retirement may drop
node/list/binding/cache storage while text-based diagnostics remain usable. A long-lived all-body read context
would undo this behavior. Keep independent result leases only for bodies actually retained by the result.

`SourceFile::copy_from` is selective. It copies mapper info, language/script/declaration/URI state, imports,
augmentations, ambient names, comments, pragmas, three reference lists, CommonJS/external indicators and flags.
It does not copy parser/binder diagnostic lists, check-JS directive, counts/hash, JSDoc/token/name caches,
reparse/lazy-list collections, bound state, binding tables or text-map caches. Do not replace this with a derived
metadata clone. An external-module indicator copied from a forced module may refer to the original SourceFile
key; changing it to the new clone's key would change reference behavior.

### Confirmed pre-publication writers

The parser's `finish_source_file` is not the publication boundary. `contentmapper::parse_result` attaches
transform diagnostics to the canonical file after parsing, parses every supplemental output, then writes
canonical/supplemental metadata links before returning the bundle. `fileloader::parse_content_mapped_file`
also appends a loader diagnostic to a newly parsed empty placeholder. The empty-config path in
`tsoptions::read_json_config_file` creates a SourceFile with the factory and installs read diagnostics directly.
These paths need an exclusive file draft through their respective host/factory return boundaries. Freezing
metadata when the parser alone returns would either reject existing behavior or require a later shared write.

Production external-module indicator writes are confined to parser finalization and selective SourceFile
copying; the AST oracle deliberately recomputes it after parsing for its additional passes. CommonJS indicators
are written by the binder and copied by the factory. These are separate mutation phases even though the legacy
fields share a struct.

`fileregions::classify` sets `check_leaf` after binding and before the CLI checking pass. Treat that as Program
retirement policy, not immutable shared file metadata. The real compiler integration must prove that a Program
cannot retire syntax still needed by another Program; the isolated owner's `retire` method is not that proof.
These call-site checks refine the grouped table; they do not complete the transitive mutation audit.

## Isolated durable metadata prototype

[`storage/source_file.rs`](../crates/tsrs_ast/src/storage/source_file.rs) implements an exclusive
`SourceFileInfoDraft` consumed into `SourceFileInfo`. It retains owned parse options and shared `Arc<str>`
source, scalar parse results/counters/hash, and plain directive/reference data. Filename validation matches
the existing factory's normalized-absolute precondition. The draft alone exposes mutable parser/loader data;
published information returns references borrowing the actual owner. It implements `HasFileName` and
`SourceFileLike`, so text-only helpers do not require an AST body.

Line/UTF-16 maps use ordinary `OnceLock<Vec<u32>>` / `OnceLock<PositionMap>` with the existing algorithms.
Line counting on a cache miss still avoids retaining a map. Source text is immutable throughout draft and
published phases, so maps initialized through a draft cannot become stale. The optional directive block is
allocated only on mutation, and finalization discards a logically empty block and its spare capacities.

`copy_parse_metadata_from` implements just the non-node, non-mapper subset of `SourceFile::copy_from`.
It preserves the target options/text, counters/hash, check-JS directive and text-map caches. It deliberately
does not derive `Clone` for the whole metadata object. The selected directive vectors/strings use one immutable
shared Arc block, preserving the legacy update's sharing without cloning each name. `Arc::make_mut` copies
that block only if an unpublished draft edits shared data. Check-JS lives in a separate optional owned value
because the reference update excludes it. Arc metadata/refcount work and copies on later edits still require
workload measurement. Source/mapper identity and body-valued copy rules remain outside this method. Each draft
now receives a checked, non-reused `SourceFileId`; freezing keeps that identity, while selective updates retain
the target's new identity. The counter owns no file data and cannot wrap. Equal filenames/text do not identify
equal versions.

Lifecycle coverage drops the real storage body and file entry before first initializing the retained info's
maps; weak body/entry references expire while text/directives remain usable, and source expires after its
last ordinary owner. A concurrent four-reader case checks one shared completed map and UTF-8/UTF-16 boundary
round trips. Update tests cover the selective copy, original-info disposal and preservation of target values;
a separate case proves sharing, isolation of a later draft edit and final shared-block release.
External-client compilation rejects escaped text/map/directive references, mutation with a live draft view,
consuming the draft with a live view and attempted mutation of published info. All five runtime cases pass
Miri. This does not demonstrate the real compiler's retirement path.

The `storage_memory` probe originally reported 208 inline bytes for either draft or published info on this arm64
host; adding source-version identity raises that to 224 bytes, including alignment. This prototype cost is open
at the no-regression gate.
Requested buffers include filename storage, optional directive headers/vectors/strings, check-JS values,
initialized maps and all their spare capacity. Deduplicate shared directive blocks and their pragma maps by
the borrowed block's identity across updates. Pragma hash-map payloads are separate lower bounds; shared
source/path bytes are separate payload counts. Arc/control blocks, enclosing file owners, mapped bundles,
allocator overhead and RSS are excluded. The [progress log](ast-soa-progress.md) records the constructed cases.

No production parser, host, file entry, diagnostic or content-mapper path uses this prototype. Freezing this
metadata does not imply completed binding or publication. The loader still needs a coherent unpublished draft
through the return boundaries above, and the source-version/bundle prototypes must be integrated before
production diagnostics can use them. This work neither completes M0/M1 nor clears the compiler performance/memory gate.

### Diagnostic ownership follow-up

Directly traced readers show that retaining only a filename and transformed text is insufficient for every
diagnostic. This is a focused input audit, not a complete diagnostic producer/mutation inventory:

| Current path | Durable data and behavior to preserve |
| --- | --- |
| [`Diagnostic::display_message_args`](../crates/tsrs_ast/src/diagnostic.rs#L261) | Reads the span map, transformed and original text to substitute a complete alias only for display. Stored message arguments remain unchanged. Mapper-authored diagnostics with a nonempty source prefix skip substitution. No syntax walk is needed. |
| [`diagnosticwriter::diagnostic_file` / `resolve`](../crates/tsrs_compiler/src/diagnosticwriter.rs#L62) | Needs canonical filename, original/transformed text, span maps and mapper name. Synthesized ranges stay in virtual text and gain an explanatory message. Renamed/original-text wrappers have fresh grouping identity on each call; ordinary file grouping uses stable source-file identity. A path alone cannot replace both identities. |
| [`diagnostic_response`](../crates/tsrs_api/src/diagnostics.rs#L56) | Reads the diagnostic file's own name/text and line/UTF-16 maps, clamps ranges and includes source lines. Preserve this API behavior separately from CLI remapping. Chains and related information recurse and may need other source versions. |
| [LSP diagnostic conversion](../crates/tsrs_ls/src/lsconv/converters.rs#L890) | Uses original filenames and mapped script/range conversion for both the primary and related locations. Synthesized spans use an explicit zero range. Keep its conversion/fidelity behavior. |
| [`DiagnosticsCollection`](../crates/tsrs_ast/src/diagnostic.rs#L432) | Groups and hashes by path without cloning its Arc on every addition; this already avoids measured checker contention. Retain source owners at a batch/result boundary and use compact source identities in entries rather than assuming an Arc per diagnostic is free. |

`Diagnostic` itself still stores mutable arena-backed file/range, source/message text, chains/related lists and
repopulation data. Parser attachment changes each top-level file and its direct related information before
publication; top-level-await finalization can attach the same diagnostic to the replacement SourceFile.
Content-mapper attachment happens after parsing. Config diagnostic location is also installed after creation.
Checker/grammar/LS paths additionally change category, no-emit status and repopulation data. Freeze boundaries
must be established for these real mutations before introducing immutable diagnostic batches. Do not turn a
parser-only draft into a claim that all later diagnostic changes are already private or synchronized.

[`DiagnosticExt::clone_diagnostic` / `copy_deep`](../crates/tsrs_ast/src/diagnostic.rs#L193) have distinct sharing
rules: shallow cloning shares chains/text/repopulation data while cloning arguments; deep copying recursively
owns the graph data but retains the original file identity. Incremental `Program::copy_diagnostics` invokes
the latter before old diagnostic/checker regions are freed. A safe replacement must retain every source
version reached by chains/related data after producer/snapshot disposal, without retaining syntax solely for
formatting. It must preserve shared diagnostic identity where the algorithms use it. Arbitrary strong Arc
edges between diagnostic batches need a cycle/reachability proof; source-only metadata cannot own a batch
whose locations retain it.

The current `OwnedParseDiagnostic` has plain range/message/owned arguments, but no source identity, chains or
mapper ownership. It is useful for the lexical prototype, not a replacement for the production diagnostic
graph. Production integration of immutable mapped-output bundles and a real result/batch ownership API remain
prerequisites. No diagnostic representation or producer changed in this follow-up.

The statement/declaration oracle corpus found an additional range constraint. For malformed JavaScript
`interface 123 {}\nnamespace "bad" {}\ntype "bad" = X;`, pinned Go emits JS diagnostic 8006 at start 27 with
length -1: `jsErrorAtRange` skips trivia past a missing node's end. Both the predecessor and current Rust AST
oracles initially panic when requesting an unsigned diagnostic length. The stored endpoints already match
Go. The oracle now serializes their signed difference; it does not change compiler diagnostics or their APIs.
The exact input, old/new binary hashes, Go result and predecessor panic are retained under the
`target/ast-soa-scanner/statements-oracle-repair-provenance.json` artifact. The full diagnostic-domain audit
must preserve such reference spans and distinguish them from ordered source slices. An owned diagnostic
model cannot assume all recovery spans have nonnegative length. Runtime reporting/conversion behavior for
these malformed spans remains a separate production obligation; passing the repaired AST oracle is insufficient.

## Content-mapped output bundles

The mapper publishes canonical → supplemental and supplemental → canonical links, then the loader can retain
the outputs as a bundle. Replacing both pointer directions with owning Arcs creates a cycle. Keeping only weak
links without a retaining bundle can instead lose a live result's canonical filename/text.

Keep cross-output identities as keys/indexes. A retained immutable metadata bundle can own all output
filenames/text/maps without owning any body or FileAstOwner. A separate caller/cache/Program bundle handle can
retain the body owners needed by its operations. Each body may retain the immutable metadata bundle; that
bundle cannot point strongly back to the body-owner group. A result following canonical metadata retains the
metadata bundle, while one following canonical syntax also retains/acquires the relevant body. Validate bundle
membership and preserved source-version identities at publication.

### Isolated bundle implementation

[`storage/mapped_source.rs`](../crates/tsrs_ast/src/storage/mapped_source.rs) implements this metadata-only
shape. `MappedSourceBundle` owns immutable output records, retaining each `SourceFileInfo`, an optional shared
owned `SpanMap` and owned directive strings. `MappedSourceFile` retains the bundle, its current source info and
its output index. A borrowed view can follow canonical/supplemental metadata without a body acquisition; using
`retain` creates an independently retaining handle. Source information does not point back to the bundle.

Bundle publication rejects repeated source identities and invalid map ranges. Resolving a nonmember version
fails even if its filename and text are equal to a member's. A missing span map remains distinct from an empty
map, preserving identity fallback, synthesized spans and failure-stub behavior. Empty mapper names preserve
`OriginalText`'s existing transformed-text fallback.

`copy_for` shares mapping/directive storage but retains replacement source metadata. It checks unchanged text
(Arc identity fast path, then byte equality). The pinned Go `SourceFile::copyFrom` copies mapper links without
rebasing them: a canonical copy still lists the original supplementals, and a supplemental copy still links
to the original canonical file. Canonical original filename lookup uses the current file; supplemental lookup
uses that original canonical version. Updates cannot be looked up as original bundle members.

Four runtime tests, also checked under Miri, cover post-retirement metadata, complete disposal via weak-owner
observations, clone/storage sharing, version/mapping validation and late interval-index accounting. External
clients cannot escape text, canonical text, span maps, directives or output iterators after their handle dies.
This is not a completed production mapper/codec lifetime proof. Body retention and file-entry binding are
separate pending integrations; source-info handles alone never grant access to syntax.

`MappedSourceBundle::memory` counts output capacity, metadata strings, directive storage and distinct shared
maps including lazily built interval indexes. Source-info objects/buffers, shared text/path payloads, Arc
control blocks, bodies, allocator overhead and RSS are separate. The constructed 0/1/8-supplement probe is
recorded in the [progress log](ast-soa-progress.md#mapped-source-information-accounting). Measure these costs
together with the real file-owner lifecycle before accepting the representation.
