# AST owned-storage implementation progress

2026-10-23, implementation began on `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1`.
The [architecture](ast-soa-architecture.md) and [migration plan](plan-ast-soa-migration.md) remain the target.
The migration is incomplete; the production compiler still uses the existing arena AST. Neither M0 nor M1 has
met its exit conditions. There is no whole-compiler owned-backend performance/RSS result. The previous 89-body
preparation failed the memory gate: Compiler-Unions single-checker RSS increased in all twelve repeat pairs
(median +264 KiB), against a zero-median identical-baseline control. The current 236-body preparation has
not been measured; no current production migration change is accepted for landing.

## Git checkpoint rebased onto main

On 2026-10-24, the migration checkpoint was rebased onto
`66119d706aada74470984b8c1c0595179060a2d9`. Conflict resolution preserves the moved grammar and scanner
algorithms while adopting upstream Rust names and lint rules. Shared helpers retain the visibility their new
callers need; owned symbol-table code reads `HashedName` through an accessor instead of its private field.

Post-rebase checks passed: workspace Clippy with warnings denied, source lint, regeneration without source
changes, 105 AST tests, 42 parser tests, 15 scanner tests, 33 span-map tests, the external borrowing control and
68 rejection cases, the lazy-binding contract, dev CLI/runner/oracle builds, all 50 regressions, and all 3,802
targeted AST inputs against both pinned Go and the pre-rebase oracle results. Provenance is recorded locally in
`target/ast-soa-scanner/rebase-validation.json`. The earlier full conformance and fuzz runs belong to their
pre-rebase source snapshots; they were not rerun for this checkpoint. Release performance and memory were not
remeasured. Acceptance requires fresh comparisons against this rebased main; historical measurements remain
historical evidence and do not clear the no-regression gate.

## Implemented foundation

[`tsrs_ast::storage`](../crates/tsrs_ast/src/storage/mod.rs) forbids unsafe code. It currently provides:

- Process-unique, non-reused store identities; checked node/list index capacity; distinct qualified node,
  node-list and modifier-list keys. Qualified keys and their options occupy eight bytes.
- Owned common node columns grouped by present payload table, with checked table/element addresses and overflow
  tables. Fixed-kind tables omit kind columns; published nodes need no payload-tag/index columns or tag directory.
  Node and list references have four-byte local/foreign encodings. Foreign references are deduplicated
  during building; the reverse lookup map is discarded on publication. Same-store indexes are checked before
  mutation or storage. A foreign key still requires the target owner's validation when followed.
- Exclusive builder mutation, immutable publication and views borrowing real builders/leases. A key has no
  dereference implementation. Lazy semantic ID assignment is separate from allocation of storage keys.
- Experimental `AstScope` construction/rollback over the actual owned columns. Freshly branded node/list/
  modifier handles cannot become ordinary keys through views or errors. Stable key construction is private.
  Conditional commit promotes a result to its parent's scope; an explicit retention barrier pins all ancestors.
  Writes of edges into existing nodes pin the affected frames. Rejection/panic restores generated payload and
  common lengths, sparse fields, text/list buffers, foreign indexes and table lookup, while counters survive.
  Snapshot boxes are allocated lazily at the first write; token-only scopes create no checkpoint. This adds no
  persistent builder/node fields. `Release` requests buffer trimming; its allocator/RSS and execution costs are
  unmeasured. The production parser still uses arena pointers. The owned shared-grammar context now uses scoped
  callbacks and branded local stacks; complete grammar, reparse producers and reclamation measurements remain open.
- Schema-generated payload tags (independent of `Kind`) and columns for every schema payload except
  `SyntheticExpression`, plus both handwritten Unknown-kind flow payloads. SourceFile covers schema body fields;
  its handwritten metadata/caches remain pending. Present tables are boxed; absent payload types allocate no
  column groups. Only builders retain a tag-to-latest-table lookup. Generated casts/setters check payload type.
  Multi-kind factories validate canonical kinds and preserve aliases. Constructors apply schema token/node flag
  masks. `new_token` now dispatches to the canonical identifier/literal/keyword payload with zero values, without
  counting text factories or allocating text buffers.
- Checked source-backed and owned text descriptors, eight bytes each. Owned cooked/escaped text is copied into
  the store's UTF-8 buffer; source descriptors borrow the store's retained source. Text is independent of mutable
  node ranges. Typed views, text and list iterators borrow the actual owner, so temporary facade values do not
  need to remain alive. External clients cannot return any of these values after their owner/guard dies.
- Raw node slices share the compact element buffer without allocating NodeList identities. Raw text lists
  mix source ranges and copied cooked text, sharing the existing descriptor buffer. Encoding needs no temporary
  descriptor Vec and discards private copied text/descriptors on validation failure.
- Separate node/modifier list columns, with eight-byte start/length spans into one owned four-byte edge buffer.
  List records use 16 bytes of column elements, modifier records 20, before vector metadata/capacity. Clones have
  separate list identities/ranges and share an immutable element span. Nil remains `Option<ListKey>`, distinct
  from an allocated empty list. Parser recovery's missing-list sentinel is explicit and survives clones/freezing,
  without an extra byte in each stored list record. Trailing commas retain the existing last-node marker/end comparison, including
  lists whose last node belongs to another store.
- Optional lists in typed payloads preserve nil/empty/missing distinctions. Rare construction-only fields use
  sorted sparse entries; absence consumes no per-node slot. Rollover, failed local-list validation and foreign
  list deduplication are covered. Sparse lookup cost has not been benchmarked.
- `AstStore::memory` and the `storage_memory` example account for requested node/list/text/foreign buffer bytes,
  capacities, table metadata and allocation counts. They exclude allocator/RSS effects and owner registrations.
- `BindingData::memory` now accounts for symbol/auxiliary/declaration buffers, table records/entries/extras,
  hashbrown index allocations, generated annotation directories/boxes/columns, flows, text, foreign keys,
  deferred bookkeeping and validation journals. Standard hash map payload bounds are separate because their
  complete bucket/control allocation size is unavailable through the public API. Shared source and retained
  fragment bodies are reported separately rather than counted again. This is an allocation-accounting probe,
  not a whole-compiler result; owner/directory/Arc/lock overhead still requires integration accounting.
- A guarded Parsed → Binding → Bound/Failed file-entry protocol. Binding takes the builder out of the entry lock.
  Abandonment/panic marks binding terminally failed and wakes waiters. Waiting uses the existing run-slot blocking
  protocol. Construction-only builder metadata is boxed, rather than reserving its inline size in bound entries.
- Weak directory registrations and explicit permitted-file read contexts. Importing a file retains its original
  directory, so future file-owned extensions are visible to both importing Programs. A directory alone grants
  no permission to acquire another file's body and cannot keep the file alive.
- Retirement rejects future body acquisitions while existing leases remain usable; source text outlives body
  retirement. This is the storage protocol, not yet the real compiler's leaf-release integration.
- A distinct file-owned binding namespace shared by base syntax and later fragments. Initial binding borrows
  syntax and binding data exclusively; later symbol/flow reads and writes use standard guards. Declaration and
  identifier annotations store compact symbol/flow indexes qualified by this explicit owner. Binding leases
  retain the base and explicitly published fragments, with no syntax-to-binding strong edge. All schema Go-only
  annotations now live in guarded binding columns rather than immutable syntax payloads, including class/file
  locals and cross-fragment container chains.
- Ordered owned symbol tables support checked symbol/table ownership, lookup/prehashed lookup, replacement,
  stable insertion order, deletion, independent clones and optional symbol member/export/export-symbol tails.
  Their eight-byte entries contain symbol indexes and fingerprints, without pointer packing. Table metadata is
  currently 32 bytes versus the legacy 24; this extra cost is experimental and unmeasured.
- Opaque declaration snapshots save a binding-owner/span without holding a read guard or retaining an owner.
  Callers reacquire the sequence under a fresh guard after forcing; iterators still borrow that guard.
  Tail appends preserve existing elements and saved lengths, while interleaved spans copy compact elements
  within the buffer. No temporary qualified-key Vec is needed. Runtime/Miri and borrowing checks pass;
  shared checker-symbol/clone integration and measurement remain unfinished.
- Table-entry snapshots preserve the legacy table's existing callback snapshot contract using saved names and
  symbol indexes. The 16-byte opaque entry holds no guard/owner; reacquired names borrow the actual data guard.
  Descriptor-based set/delete operations allow same-owner mutation without borrowing/copying the saved string.
  Replacement, deletion, aliases and growth leave saved entries valid. The unchanged text-buffer accounting is
  tested; this is no compiler RSS/performance claim. Foreign checker symbols and their name owners remain pending.
- Generated schema-ordered child-edge cursors, including runtime-dependent JSDoc name/type ordering. Cursors
  retain keys, not borrowed views, allowing recursive mutation between steps. List edges remain explicit for the
  caller's owner context to resolve; raw node spans yield individual node edges. Shallow local clones preserve
  kind/flags/range and immutable text/list spans, reset parent/semantic identity/binding annotations, and survive
  table rollover. Visitor/update hooks, cross-store clones and full updating visitors remain pending.
- The production parser now uses an exclusive `ParserFactory` construction facade (187 generated schema/alias
  methods plus handwritten allocation paths). It delegates to the legacy backend and has no Clone/Deref.
  Allocation helpers require mutable access, with separate factory/node-stack borrows to avoid copying lists.
  This is preparatory production integration, not an owned parser or proof that its existing node reads are safe.
  JSDoc/pragmas and the legacy reparse cloner still have explicit legacy factory boundaries.
- Production `finish_node_with_end` and immediate-child parent assignment now share one generic construction
  routine with `AstStoreBuilder`. Its callbacks receive the exclusive writer; owned traversal copies a child
  key before each callback, permitting column/list growth and nested finalization. The legacy implementation
  still uses arena nodes internally. This ports one real parser routine, not the complete grammar or binder.
- The isolated deferred-member protocol now uses `FileBindingContext` for public forcing and one explicit
  `LazyBindSession` for recursive forces. The file gate is acquired before claiming a list. Parsed private keys
  become available before binding, so same-list and two-list recursion share the current writer. Pending,
  Deferred, ParseOnly, Building, Ready and Failed remain distinct; initial parse-now skips deferred replay.
  Published syntax has no mutable writer; existing binding annotations and symbols change under the file gate.
- Initial publication validates binding edges and base/retained-fragment syntax dependencies. Initial fragments
  must be retained before publication. Later writes journal node targets/edges and new retained fragments;
  `BindingWrite::complete` is fallible and validates them before releasing readers. Deferred commits freeze and
  register both parser-built and binder-retained fragments. Retirement follows gate → body lock order.
  Panics, including nested panics caught by callbacks, cannot publish incomplete initial or late bindings.
- Completed-list forcing has a read-guard fast path. Newly registered lazy lists enter a queue drained once,
  avoiding a full file-list scan on every late commit. Lazy-list bookkeeping is boxed only when present;
  validation journals are boxed on the first published write. The initial queue and journal buffers are freed
  after use. These are prototype allocation choices, not accepted compiler performance results.
- Scanner/regexp share one algorithm source with separate concrete legacy lowering. `OwnedScanner` borrows
  caller-retained input and uses ordinary owned cooked text. `tools/gen-scanner.mjs` emits concrete production
  scanner/regexp methods, preserving arena allocation/pinning and Copy snapshots. Generic production lowering
  repeatedly regressed instructions; the concrete split is experimental and not yet accepted by measurements.
  The shared lexical grammar uses direct token observations for either scanner. The oracle exercises the actual
  production `Scanner`. The scanner crate forbids unsafe code; comment iterators borrow non-static input.
  The owned parser now moves cooked token values out of the scanner; saved snapshots remain independent.
  Snapshot/cache clones and copying cooked text into the AST buffer remain unaccepted costs.
- The production parser's lexical routines now share `LexicalParser` with an experimental `OwnedParser`:
  identifiers/private names, qualified names, literals, keyword expressions and token nodes. The owned context
  borrows caller-retained `Arc<str>` for scanning while its exclusive builder retains an Arc clone. Source
  descriptors avoid copying plain text; cooked text enters owned buffers. Node construction/finalization runs
  directly on the owned store, without creating a parallel arena tree. The complete source-file grammar,
  JSDoc/reparser, metadata and binder are not connected to this context yet.
- Token expectation/recovery now uses shared `TokenParser` grammar in both contexts: optional/expected tokens,
  non-advancing checks, custom diagnostics, missing token nodes and matched/unmatched delimiters. Related
  bracket diagnostics use copied record handles during construction; owned diagnostics retain their own
  argument/related-information vectors. The grammar preserves duplicate-error suppression and error-flag
  consumption by missing nodes. Identifier-name, binding-name and optional-binding roles also use shared grammar,
  including contextual await/yield, private-name recovery and forbidden Unicode escapes. JSDoc-specific token
  entry points, expression/statement/type grammar and the full source-file entry remain to be connected.
  This is a migration dependency, not a restricted replacement parser or an accepted production performance cost.
- List recognition and construction now use shared `RecognitionParser` / `ListParser` algorithms in both
  contexts. The 38 token-only recognition bodies retain the existing algorithms, including statement/type/member
  starts, using declarations, heritage recovery, ASI and every list context's element/terminator/error rules.
  Owned lists allocate directly into the builder's compact edge buffer; recursive callbacks keep copied keys on
  a parser-owned stack. Failed callbacks restore the caller's stack/context, missing openings retain the explicit
  missing-list sentinel, and JSDoc aliases/imports preserve insertion/propagation order. The owned parser adds two
  Vec headers, key buffers and a parsing-context bitset. These transient costs and changed production lowering
  remain unmeasured. The recursive array helper used by tests is a test fixture, not production expression grammar.
- A further 51 token-only routines now share the same recognition context: modifier follow rules, import phases,
  await/yield heuristics, optional/member chains, mapped/index/tuple type starts, parameter/return-type recognition,
  semicolons and remaining context checks. Every extracted body matches the validated predecessor after replacing
  field reads with context getters and qualifying callbacks. Speculative function-type recognition still builds
  modifiers/binding patterns through the legacy grammar and is deliberately not classified as token-only.
  This adds no owned node/store fields; production lowering remains an unmeasured change.
- Shared `ContextParser` preserves context flag scopes and conditional modifier commit/rollback. Legacy hooks
  retain the existing arena snapshot behavior; owned snapshots independently retain cooked token values and
  restore diagnostics, context flags, parse-error state and the await marker. Owned lexical lookahead now uses
  those same snapshot hooks. Explicit ordinary-key snapshots retain abandoned allocations; shared lookahead now
  uses scoped construction and reclamation. Unported JSDoc/source state remains outside the implemented snapshot.
  The [rollback audit](ast-parser-rollback-audit.md) records the actual escape/reclamation boundaries.
- Shared grammar now runs inside `OwnedParserScope`, borrowing existing parser state and a journal-forwarding
  `AstScope::borrow_writer`. Independent parser, brand and writer lifetimes permit nested lookahead/conditional
  commit without casts, copying the caller's node stack or constructing another AST tree. Scope-local lists use
  branded keys; outward reparse entries become stable only through an ancestor retention barrier. Rejection and
  panic restore implemented parser state and discard unretained allocations while preserving counters. Nested
  acceptance promotes only to the enclosing brand. This adds transient scope/vector headers, journal forwarding
  and local stack buffers; reparse forwarding conservatively pins already-stable imports too. Their instruction,
  retained-capacity and RSS costs are unmeasured. Full grammar/JSDoc producer and lazy-capture integration remain open.
- Four type-leaf routines now share `LeafTypeParser`: keyword types, `this`, JSDoc all-type and literal types,
  including signed numeric/bigint literals. Owned constructors allocate typed payload columns directly and
  shared finalization attaches immediate parents through the exclusive writer. Complete recursive type and
  expression grammar remains absent from the owned context. Source hashes were checked before integration, and
  the validated context predecessor is retained under `target/ast-soa-scanner/parser-context-{source,binaries}/`.
- Template heads, middle/tail tokens and missing-span recovery now share `TemplateTokenParser` in production,
  the owned root parser and branded parser scopes. Rescanning preserves tagged/untagged escape diagnostics,
  raw CRLF/Unicode text, cooked values, template flags and unterminated tokens. Raw text uses source ranges;
  only cooked strings enter the owned text buffer. The 128-template growth/disposal case requests 5,504 text
  buffer bytes (three cooked strings plus three eight-byte descriptors per template); raw text and identifier
  names add no text-buffer allocations. This is requested-buffer accounting, not compiler memory acceptance.
  Nested accepted tail construction is reclaimed by outer rejection, and raw views cannot cross a writer
  mutation or escape speculation. Full template-expression loops now share `TypeParser`; those loops and the
  shared type-span loops still have only a legacy backend.
- An initial 89 production type/signature/expression routines use `TypeParser`: conditional, union/intersection,
  operators/infer, postfix arrays/indexes/JSDoc nullability, references/arguments, queries, predicates and
  parentheses, including function/constructor-constituent diagnostics. The preceding 18 bodies add operand dispatch,
  mapped/tuple/type literals, full template-type spans, type parameters, function/constructor types, JSDoc type
  prefixes/suffixes and type annotations. Constructors require exclusive access; reads return copied values.
  Optional-tuple conversion copies its child key before allocating, then preserves flag/range/parent mutation
  order. Type literals retain the real lazy-member callback and owner installation. Scanner rescans preserve
  the original cases that report errors without assigning `Parser.token`.
  Another 21 bodies share parameters, type members/signatures, modifiers/decorators, property names, accessor
  declarations and function-body/context entry. Modifier-list construction uses the exclusive shared key stack;
  modifier range reads copy a range before reporting an error. Parameter parsing preserves outer-await decorator
  handling, ambiguity rejection, initializer recovery and JS-syntax bookkeeping. Modifier fast/worker attributes
  remain in place. Another 27 bodies share import types/attributes, assignment/conditional/comma expressions,
  binary precedence, assertions, yield/await and unary/update parsing. Missing import braces use a copied
  last-diagnostic handle, including when same-position suppression reused an existing error. Expression reads
  copy kinds, ranges and operators before recursive mutation. Existing modifier/unary/update method attributes
  and the binary-operator binding helper are preserved. The latest mechanical check covers the 27 new and
  62 unchanged predecessor bodies.
  Only the complete legacy parser implements this trait so far. Binding patterns, initializers,
  decorator expression grammar, statement blocks, full classes, JSDoc, JS-syntax bookkeeping, constructing
  function-type lookahead and lazy-member capture remain backend dependencies. Arrow/left-hand-side/JSX bodies
  are now shared in the subsequent extraction below. No owned type entry point or restricted replacement
  grammar is supplied. Legacy mark/rewind still reclaims an unpinned arena checkpoint; ordinary owned snapshots
  retain AST allocations and still require branded conditional construction for these rejection paths.
  This preparation adds no node/store/parser fields and has no new performance/RSS acceptance result.
- A further 61 real routines move arrow classification/construction, left-hand-side/member/call/new expressions,
  optional-chain repair, template loops, primary/object/array/function/decorated expressions, JSX and method
  declarations behind copied handles and exclusive writers. The token-only async-arrow classifier runs in
  owned recognition scopes too. The recursive arrow-return-type error helper brings `TypeParser` to 150
  default bodies. Instantiation absorption and JSX recovery copy payload/list keys before recursive construction;
  parent repair reads one element key before each writer call. No normal-path list copy or persistent parser/
  store field is added. A source-text diagnostic hook preserves existing normalization. Compiler-reported dead
  forwarding wrappers were removed after checking their delegating bodies. The lexical inventory now finds
  187 remaining legacy routines / 4,373 lines under the recursive type/expression roots, with the same explicit
  limits: fields, free functions, indirect callbacks, source-file/JSON entry points and ownership are separate
  work. Blocks, full classes, binding patterns, initializers, decorator expressions, JSDoc and lazy capture still
  need owned implementations. Construction-capable rejection still uses legacy snapshots; branded node/list
  commit must be connected before exposing owned recursive grammar. See the [rollback audit](ast-parser-rollback-audit.md).
- Another 82 real routines move statements/blocks, variable declarations and binding patterns/initializers,
  functions, full classes/heritage, interfaces/type aliases/enums, modules/namespaces, imports/exports and
  decorator expressions into `TypeParser`. Its 236 default bodies comprise the previous 150, these 82 and
  four explicit modifier/heritage helpers. It still has only the legacy implementation. Ambient modifier
  mutation reads one copied key before each flag write; heritage conversion copies expression/type-argument
  and property-name keys before recursive construction. JavaScript heritage checks traverse lists by copied
  keys. No ordinary-path list copying or persistent parser/node/store fields are added. Invalid-name
  diagnostics read cooked scanner text without consuming it, because recovery can reuse the same token.
  The constructor's rejection branch performs token recognition only; existing construction-capable
  mark/rewind paths elsewhere still need branded promotion. Compiler-reported unused delegating wrappers
  and two superseded legacy helpers were removed. The current lexical inventory finds 100 remaining legacy
  routines / 2,304 lines under the recursive grammar roots; it excludes fields, free functions, indirect
  callbacks, source-file entry plumbing and ownership, so this is neither a completion measure nor a safety
  proof. JSDoc, constructing function-type lookahead, source-file/reparser integration and real lazy capture
  remain unconnected. No complete or restricted owned recursive parser entry point is added.
- Isolated durable `SourceFileInfoDraft` / `SourceFileInfo` own options, shared text, scalar parse results and
  plain directives independently of retireable bodies. Consuming finalization removes the mutable writer;
  maps use ordinary OnceLocks and text-only traits borrow the retaining info. The explicit metadata-copy
  subset preserves legacy omissions and shares immutable directive blocks; only later draft edits copy them.
  A checked, non-reused four-byte `SourceFileId` identifies each version independently of semantic node IDs;
  freezing preserves it, while a SourceFile update receives a new identity. No production parser/loader/file
  owner uses this metadata yet; diagnostic ownership and real publication remain pending.
- Isolated `MappedSourceBundle` owns canonical/supplemental metadata, original text, mappings and directives.
  Cross-output links are bundle indexes, without body owners or strong ownership cycles. Retained file handles
  and borrowed views preserve the reference's clone links: a supplemental copy still names the original
  canonical version. Copies share mapping/directive storage while retaining their own current source metadata.
  Publication checks membership, duplicate identities and mapping bounds; wrong versions cannot resolve even
  with equal filenames/text. Ordinary owned span-map constructors/decoders use the existing algorithms.

The [`--inventory` generator mode](../tools/gen-ast/gen-ast.ts) emits 192 schema nodes and 318 unique field
declarations. Its mutation phases are inferred from existing layout policy; this is not a completed call-site
audit. The generated JSON explicitly lists handwritten structures requiring separate review.
The [handwritten SourceFile audit](ast-source-file-ownership-audit.md) now classifies every field group,
its producer and intended retention boundary, including selective clone behavior and cyclic mapper links.
The plain durable metadata subset and mapped metadata bundles are implemented in isolation. The production
caller proof, file-entry integration and diagnostic ownership remain unimplemented.

## Validation so far

- The 236-body statement/declaration preparation passes 105 AST and 42 parser tests, the external-client
  positive control plus 68 borrowing rejection cases, the real lazy-binding contract, workspace Clippy with
  warnings denied, source lint and registered regeneration. The mechanical check covers all 82 moved and
  150 unchanged predecessor bodies; a separate audit preserves attributes on all 232 routines, including
  the 19 carrying attributes. The four new modifier/heritage helpers are additional bodies. The source
  inventory remains 702 files / 511 unsafe sites / 34 Send/Sync implementations. All 48 regressions,
  4,000 differential fuzz inputs (seed `20261107`) and 3,802 targeted pinned-Go AST inputs pass. Both full
  conformance modes complete 15,448 variants with zero differences, crashes, timeouts or lost passes in all
  six categories: diagnostics 13,700; types/symbols 12,998 each; JavaScript 13,626; JS maps 149; source-map
  records 156. Each uses four workers and the unchanged 600-second case limit.
  The expanded corpus covers statements/ASI, for/await/using, patterns, classes/heritage/ambient flags,
  malformed declarations/imports/exports and truncated recovery. Its first run exposed an inherited AST
  oracle panic on a negative JavaScript diagnostic length produced by Go's missing-node recovery. The
  predecessor binary reproduces that panic; the stored endpoints already match Go. The oracle now prints
  their signed difference, retaining the case in the corpus. Compiler semantics and diagnostic APIs are
  unchanged. The first snapshot, finding and binaries are preserved separately. Only the oracle example
  changed before resuming validation; unchanged compiler runtime checks are retained, and workspace Clippy,
  source lint, regeneration, oracle build, the entire corpus and fuzzing pass again after the repair.
  Evidence uses the `target/ast-soa-scanner/statements` stem. Final verification checks 71 current and
  archived source files, three configuration files, four binaries and retained copies, four automation
  files, corpus input hashes, validation logs, both pass sets and the before/after oracle evidence. Source
  hashes stay unchanged throughout each validation snapshot. Regenerable incremental cache was removed
  after the compiler build to recover disk space; source, evidence and baseline artifacts were retained.
  No new Miri, complete owned parser, release performance/RSS or final migration acceptance is claimed.
- The 150-body expression/arrow/JSX preparation passes 105 AST and 42 parser tests, the external-client
  positive control plus 68 borrowing rejection cases, the real lazy-binding contract, workspace Clippy with
  warnings denied, source lint and registered regeneration. The 61 moved routines and all 89 predecessor
  bodies match the recorded backend substitutions; attributes and the operator-binding helper are preserved.
  The live source inventory remains 702 files / 511 unsafe sites / 34 Send/Sync implementations. All 48
  regressions, 4,000 differential fuzz inputs (seed `20261106`) and 2,474 targeted pinned-Go AST cases pass.
  The expanded corpus covers optional-chain/instantiation parent repair, ambiguous arrows, function/object/
  array/template expressions, malformed/namespaced/nested JSX and truncated recovery. Both full conformance
  modes complete 15,448 variants each with zero differences, crashes, timeouts or lost passes in any of the
  six baseline categories. Counts remain diagnostics 13,700; types/symbols 12,998 each; JavaScript 13,626;
  JS maps 149; source-map records 156. Both use four workers and the unchanged 600-second case limit.
  Evidence and retained binaries use the `target/ast-soa-scanner/expression-completion` stem. Final verification
  checks 71 source files, three configuration files, four binaries, their retained copies and validation-log
  hashes; every validation stage reports zero source changes. A documentation Clippy failure is retained separately
  under `expression-completion-doc-lint-attempt`; it was corrected before this source snapshot.
  Disk exhaustion during the first CLI build left a non-executable 1,664-byte artifact despite Cargo returning
  success with an objcopy warning. The first regression attempt therefore ran no compiler comparisons.
  Regenerable incremental cache and incomplete generated executables were removed, the CLI rebuilt, executable
  access/version checked, and all regressions rerun successfully. The manifest preserves the invalid attempt,
  its original binary hashes/logs and recovery details; it is not counted as a parity result. The owned
  recursive grammar remains unconnected, no new Miri result is claimed, and the current source has no matched
  release performance/RSS measurement. The prior preparation's repeatable RSS increase is still unresolved.
- The 89-body type/import/expression candidate passes 105 AST and 41 parser tests, the external-client positive
  control plus 68 rejection cases, and the real lazy-binding contract. The 27 new and 62 predecessor bodies,
  preserved method attributes and operator-binding helper pass the mechanical check. Workspace Clippy with
  warnings denied, source lint and registered regeneration pass, with no generated source hash changes and
  the unchanged 702-file / 511-unsafe-site / 34-Send/Sync inventory. Required development CLI/runner/oracle
  builds, all 48 regressions, 3,000 differential fuzz inputs (seed `20261105`) and 1,824 targeted pinned-Go AST
  cases pass. The corpus covers unfinished import braces, import attributes, operator/assertion precedence,
  assignments, unary/update forms, exponentiation errors and TS/TSX/JS recovery. Both full conformance modes
  complete all 15,448 variants with zero differences, crashes, timeouts or lost predecessor passes. Counts in
  both modes remain diagnostics 13,700; types/symbols 12,998 each; JavaScript 13,626; JS maps 149; source-map
  records 156. Runs use four workers and the unchanged 600-second timeout. Source/configuration/binary hashes,
  zero source changes during validation and retained development binaries are recorded under the
  `target/ast-soa-scanner/type-expressions` stem. Final verification rechecks all 71 source files, three
  configuration files, four binaries and their retained copies. A matched release preparation binary is
  retained separately, with build/source provenance in `type-expressions-release.json`. The owned backend is
  unchanged; no new Miri or complete owned-parser result is claimed. The production migration and no-regression
  gates remain open; correctness evidence alone does not authorize landing.
- The latest 62-body type/signature candidate passes 105 AST and 41 parser tests, the external-client positive
  control plus 68 rejection cases, and the real lazy-binding contract test. Workspace Clippy with warnings
  denied passes on the final source before archiving. Source lint and registered regeneration pass with the
  unchanged 702-file / 511-unsafe-site / 34-Send/Sync inventory and no generated source hash changes.
  Required development CLI/runner/oracle builds, all 48 regressions, 2,500 differential fuzz inputs (seed
  `20261104`) and 442 targeted pinned-Go AST cases pass. The expanded corpus includes parameter modifiers,
  decorators, outer-await contexts, binding-pattern/default recovery, accessors and type-member separators in
  TS/TSX/JS/declaration inputs. Both full conformance modes complete all 15,448 variants with zero differences,
  crashes or timeouts and no lost passes against the 41-body predecessor. Counts in both modes remain
  diagnostics 13,700; types/symbols 12,998 each; JavaScript 13,626; JS maps 149; source-map records 156.
  Both runs use four workers and the unchanged 600-second case timeout. Exact source/binary/log hashes and zero
  source changes during validation are recorded in `target/ast-soa-scanner/type-signatures-validation.json`;
  the 71-file source archive and development binaries are retained beside it. The 21 new and 41 predecessor
  bodies pass the mechanical check, including the two modifier method attributes. The dependency report is
  lexical, not a completed field/owner audit. No new Miri or release performance/memory acceptance is claimed;
  owned implementations are unchanged apart from a test import. Final source/configuration/binary and retained-
  binary hashes are rechecked in `type-signatures-final-verification.json`. Production still uses arena ASTs, and the
  complete owned grammar/binder/lifecycle/consumer migration and no-regression gates remain open.
- The 41-body type-operands candidate passes 105 AST and 41 parser tests, the external-client positive control
  plus 68 rejection cases, and the existing real lazy-binding contract test. Workspace Clippy with warnings
  denied, source lint and registered regeneration pass; generation changes no source hashes. The source
  inventory remains 702 files, 511 unsafe sites and 34 Send/Sync implementations. Required development
  CLI/runner/oracle builds, all 48 regressions, 2,000 differential fuzz inputs (seed `20261103`) and 346 targeted
  pinned-Go AST cases pass. Both full conformance modes complete all 15,448 variants with zero differences,
  crashes or timeouts, retaining every predecessor pass in all six categories. Counts in both modes remain
  diagnostics 13,700; types and symbols 12,998 each; JavaScript 13,626; JS maps 149; source-map records 156.
  The runs use four workers and the unchanged 600-second timeout; they establish no performance result.
  `target/ast-soa-scanner/type-operands-validation.json` records source/binary/log hashes and zero source changes;
  source and binaries are retained beside it. The mechanical check covers the 18 newly moved and 23 unchanged
  predecessor bodies. `type-operands-extra-checks.json` records the additional binder contract run, and
  `type-operands-dependencies.json` is a lexical dependency inventory, not a completed ownership audit.
  The owned implementations are unchanged; no new Miri or release/performance claim follows. These passes
  do not close M0/M1, the owned grammar/binder migration, or the no-regression gate.
- The template-token and recursive-type preparation slice passes 105 AST tests, 41 parser tests and a compiling
  external client with 68 rejection cases. The predecessor archive contains 66 rejection cases plus its positive
  control; the earlier count of 67 rejections was corrected. Two focused owned-template cases pass pinned-nightly
  Miri: nested accepted construction followed by outer rejection, and growth/source disposal. Those owned
  implementations were unchanged by the subsequent legacy-only type extraction. Workspace Clippy with warnings
  denied and source lint pass (702 files, unchanged 511 unsafe sites / 34 Send/Sync implementations).
  Registered regeneration in `--write` mode changes no source hashes; plain dirty-tree diff checking cannot pass
  before these tracked refactors are committed. Required development CLI/runner/oracle builds, all 48 regressions,
  2,000 differential fuzz inputs (seed `20261102`) and 221 targeted pinned-Go type/template AST cases pass.
  Both full conformance runs complete all 15,448 variants with zero differences, crashes or timeouts and no lost
  passes across all six categories. Final pass counts in both modes: diagnostics 13,700; types and symbols 12,998
  each; JavaScript 13,626; JS maps 149; source-map records 156. The default run restores the prior leaf run's
  timed-out diagnostic/type/symbol/JS entries. Both runs use four workers and the unchanged 600-second case
  timeout; this limits contention but establishes no performance attribution or speedup. The failed earlier
  runs remain recorded below. These passes do not clear M0/M1 or the complete migration/performance gates.
  `target/ast-soa-scanner/type-grammar-validation.json` records all source/binary/log hashes and zero source
  changes during validation. The archive contains 71 changed/untracked source/tool files; exact development
  binaries are retained in `type-grammar-binaries/`. The 23-function mechanical check is in
  `type-grammar-mechanical-check.json`. No release instruction/RSS/wall measurements exist for this slice.
- The parser-scope slice passes 105 AST and 37 parser tests, including nested acceptance/rejection, cooked-text
  rollback, recursive list construction, panic cleanup, retained reparse order and source disposal. The external
  client has a positive control and 66 negative controls, including eight new parser key/collection/text/private
  writer and live-view cases. Four parser cases and the borrowed-writer storage case pass pinned-nightly Miri.
  Required development CLI/testrunner/oracle builds, all 48 regressions and 2,000 differential fuzz inputs pass.
  Workspace Clippy/source lint pass with
  the unchanged 511 unsafe sites and 34 Send/Sync implementations. Regeneration changes no compiled source hash.
  Current evidence is `target/ast-soa-scanner/parser-scopes-validation.json`; no performance/RSS acceptance follows.
  Default full conformance passes all 15,448 variants with zero differences/crashes/timeouts and all six pass
  sets preserved. Canonical full conformance has zero differences/crashes but times out on
  `compiler/intersectionConstructorReductionCrash`, losing its diagnostic/type/symbol/JS entries against the
  retained leaf predecessor. The previous slice also recorded this case timing out in default-history focused
  replays. Matched canonical replays are recorded separately in `parser-scopes-timeout-followup.json`; the
  predecessor and candidate both pass the focused run (254.8s and 272.0s respectively). One sequential pair
  does not establish the cause or repeatability of that difference. The full canonical gate remains unpassed.
  These correctness runs are not performance measurements and do not establish attribution or the absence of
  a regression.
- The real-storage scope candidate passes 104 isolated AST unit tests, all ten selected scope cases under
  `nightly-2026-10-04` Miri, AST-target Clippy with warnings denied, and real external-client compilation:
  a positive control plus 59 negative controls (eleven new scope/key-boundary cases). The cases include nested
  commit/rejection, panic/error cleanup, ancestor retention, cross-store rebranding rejection, foreign indexes,
  sparse columns/table rollover, comma-bit reuse, source disposal, and capacity release after a constructed
  discarded body. Existing legacy AST/parser generated outputs remain byte-identical after regeneration.
  Root integration/source hashes are in `target/ast-soa-scanner/storage-scope-root-integration.json`. Root
  regeneration, 104 AST tests, 31 parser tests, external-client controls, workspace Clippy, source lint and required
  dev CLI/testrunner builds pass with unchanged compiled source hashes. Source lint reports 700 files, the
  unchanged 511 unsafe sites and 34 Send/Sync implementations. All 48 regressions pass against the rebuilt CLI.
  `storage-scope-root-validation.json` records terminal success and zero compiled-source changes for every step.
  These storage cases do not prove parser reclamation or full compiler parity,
  and do not clear M0/M1 or the performance/memory gates. Earlier cache-write/external-build checks hit a full
  disk; only incremental caches were removed, and the affected Miri/client checks then completed successfully.
- The type-leaf slice passes 31 isolated and root parser tests, parser-target and root workspace Clippy with
  warnings denied, formatting/source lint, development CLI/testrunner/oracle builds, and separate isolated/root
  2,000-mutant Go comparisons (seed `20261031`, zero findings each). Pinned-nightly Miri selects and passes the
  owned type-payload growth/source-disposal case on the exact candidate sources adopted by root. Source lint
  reports 699 files and the unchanged 511 unsafe sites / 34 Send/Sync implementations. The candidate/source
  hashes are in `target/ast-soa-scanner/leaf-types-candidate.json`; integration is recorded separately in
  `leaf-types-root-integration.json`. Root regressions and full canonical conformance pass, with unchanged source
  hashes and all six canonical pass sets preserved. Default conformance reports one timeout on
  `compiler/intersectionConstructorReductionCrash`, no differences/crashes, and one missing diagnostic/type/
  symbol/JS pass. Same-config focused 600-second replays time out on the same case in both the retained context
  predecessor and the retained leaf candidate. This does not establish performance attribution or clear the
  default-history gate. Results remain in `leaf-types-root-validation.json`, `leaf-types-timeout-followup.json`
  and the `leaf-types-timeout-*` logs. Production allocation and
  performance/memory acceptance remain unchanged.
- The context/snapshot candidate is integrated after checking source/binary hashes and archiving the validated
  recognition predecessor under `target/ast-soa-scanner/recognition-extension-{source,binaries}/`. All 29 parser
  tests, workspace Clippy with warnings denied, formatting, source lint, required development/oracle builds,
  diff checks and 2,000 Go differential mutants (seed `20261030`, zero findings) pass. Pinned-nightly Miri selects
  and passes the owned cooked-token/abandoned-key snapshot lifecycle test. Source lint reports 698 files with
  the unchanged 511 unsafe sites / 34 Send/Sync implementations. All regressions and both full conformance
  histories pass with unchanged source hashes and all six pass sets preserved: 13,700 diagnostics, 12,998 types
  and symbols each, 13,626 JavaScript, 149 JavaScript maps and 156 source-map records. Neither history reports
  differences, crashes or timeouts. Results are in `target/ast-soa-scanner/parser-context-validation.json`.
  The logs report 448.6/543.8 seconds for default/canonical; orchestration elapsed times are recorded separately
  and these correctness runs are not benchmarks. Production still uses arena AST allocation and the complete
  owned grammar/binder plus no-regression gates remain open.
- The recognition extension passes 25 isolated parser tests, parser-target Clippy with warnings denied and all
  2,000 Go differential mutants (seed `20261029`, zero findings). It is integrated after archiving the validated
  list predecessor in `target/ast-soa-scanner/list-grammar-source/` and `list-grammar-binaries/`. A new owned-only
  await/yield/semicolon lifecycle case brings the root parser suite to 26 passing tests and passes pinned-nightly
  Miri after parser/source disposal. Root workspace Clippy with warnings denied, development CLI/testrunner and
  oracle builds, formatting, source lint (697 files, unchanged 511 unsafe sites / 34 Send/Sync implementations),
  diff checks and another 2,000-mutant Go comparison (same seed, zero findings) pass. All regression projects pass.
  Both default and canonical conformance pass with all six pass sets preserved: 13,700 diagnostics,
  12,998 types and symbols each, 13,626 JavaScript, 149 JavaScript maps and 156 source-map records.
  Neither history reports differences, crashes or timeouts, and source hashes stayed unchanged. The results
  are recorded in `target/ast-soa-scanner/recognition-extension-validation.json`. The production arena AST, incomplete grammar
  integration and open performance/RSS gate remain.
- Shared recognition/list grammar passes all 24 parser tests. New cases cover all 26 parsing contexts with
  scanner/lookahead/context-state comparisons, recursive lists and stack prefixes, trailing commas, malformed
  delimiters, failed and nonadvancing callbacks, missing versus empty lists and JSDoc declaration ordering.
  The owned-only growth/freeze/source-disposal case passes pinned-nightly Miri in both the isolated copy and
  integrated root. Root and isolated full-parser Go differential runs each compare all 2,000 mutants (seed
  `20261028`) with zero findings. Root parser tests, workspace Clippy with warnings denied, the required development
  CLI/testrunner build, formatting, source lint (697 files, unchanged 511 unsafe sites / 34 Send/Sync implementations)
  and diff checks pass. One prematurely started Miri invocation selected the predecessor's zero matching tests
  and is excluded; the final root run selected and passed one test. All regression projects and full conformance
  in both histories pass. All six baseline pass sets match the token-recovery predecessor: diagnostics 13,700,
  types/symbols 12,998 each, JavaScript 13,626, JavaScript maps 149 and source-map records 156. Logs report
  437.0 seconds default / 497.3 seconds canonical, with no differences/crashes/timeouts; orchestration timings
  remain separately recorded. `target/ast-soa-scanner/list-grammar-validation.json` retains source/binary hashes,
  per-category comparisons and separate output directories, with no source changes during either run.
  The migration and production performance/RSS gate remain incomplete.
- Shared token recovery and identifier roles pass all 17 parser tests. Differential cases cover token/range/flag
  equality, advancement, counters, contextual names, diagnostic order and related information. The owned-only
  recovery case passes pinned-nightly Miri after parser/source disposal. A 2,000-mutant full-parser comparison
  against pinned Go (seed `20261027`) has zero findings. Workspace Clippy with warnings denied, formatting,
  source lint (695 files, unchanged 511 unsafe sites / 34 Send/Sync implementations) and the required development
  CLI/testrunner build pass. All regression projects and full conformance in both histories pass, with no lost
  entries in any of the six baseline categories: diagnostics 13,700, types/symbols 12,998 each, JavaScript 13,626,
  JavaScript maps 149 and source-map records 156. Logs report 460.0 seconds default / 625.6 seconds canonical;
  orchestration timings are retained separately in `target/ast-soa-scanner/token-recovery-validation.json`.
  The manifest records unchanged source/binary hashes; the validated predecessor's changed sources and development
  binaries are archived under `token-recovery-source/` and `token-recovery-binaries/` beside it before list integration.
  The owned diagnostic's additional Vec header/related allocations and changed production lowering are unmeasured
  costs, so the performance/RSS gate remains open. This does not complete the source-file parser or the migration.
- Mapped metadata passes 94 AST library tests, all 33 span-map tests and the external-client check, which now
  rejects 48 clients. All four mapped metadata cases pass pinned-nightly Miri with `plain-ptrs`, covering
  body retirement/final release, clone links/storage sharing, version/mapping validation and lazy map accounting.
  The source-ID exhaustion case and concurrent owned span-map case also pass Miri. Workspace Clippy with
  warnings denied, formatting, source lint (694 files, 511 unsafe sites, 34 Send/Sync) and `git diff --check`
  pass. Previous successful content-mapper tests and the development CLI/testrunner build were reused after
  verifying all recorded source hashes were unchanged; the subsequent test edit strengthens equal-name/text
  version rejection. The memory probe exposes bundle overhead and a 208 → 224-byte source-info increase after
  adding identity. These are prototype costs, not accepted compiler memory/performance results. No new full
  compiler conformance or no-regression claim follows. Reference HEAD/pin and clean `ts-ref/tsc` were reverified.
- Durable source info passes 89 AST library tests and the external-client check, now rejecting 43 clients.
  Its five lifecycle/concurrency/copy/sharing/accounting cases pass pinned-nightly Miri with `plain-ptrs`. Workspace
  Clippy with warnings denied, source lint (693 files, 511 unsafe sites, 34 Send/Sync) and `git diff --check`
  pass. The direct `storage_memory` probe passes and accounts for optional headers, nested strings, map payload
  bounds and spare capacities. This is unselected prototype code; no new compiler conformance or performance
  result is claimed. Reference HEAD/pin and clean tracked `ts-ref/tsc` sources were reverified.
  The source-file audit also traces diagnostic display, CLI/API/LSP conversion, collection keying and incremental
  deep-copy retention. It records source/bundle/identity dependencies without claiming a complete producer audit
  or changing any diagnostic representation.
- Table-entry snapshots and descriptor-based mutations pass 84 AST library tests and the external-client check,
  now rejecting 37 clients. The focused snapshot case also passes the pinned-nightly Miri command. Workspace
  Clippy with warnings denied, source lint (692 files, 511 unsafe sites, 34 Send/Sync) and `git diff --check` pass.
  No persistent binding/table/symbol layout fields were added. The snapshot API remains isolated from production.
  The binding-memory full default-history run, built before table snapshots, had no baseline differences/crashes
  but timed out `compiler/intersectionConstructorReductionCrash` at 600 seconds. Its original result lists are
  preserved under `target/ast-soa-scanner/binding-memory-default-timeout-results/`. The one-worker rerun of that
  case passed under the same 600-second limit (201.7 seconds); the combined default diagnostic pass set has all
  13,700 entries, with no lost passes. The default full run itself remains a timeout. Canonical history passed all
  15,448 variants, zero differences/crashes/timeouts, in 497.7 seconds. It has 13,700 diagnostic passes and no lost
  entries against the saved concrete-scanner run. Full result folders, recovery comparison and binary hashes are
  under `target/ast-soa-scanner/binding-memory-*`; both conformance histories used binaries built before table
  snapshots. That validation's Rust/generator inputs are archived in `table-snapshot-source.{tar.gz,json}`;
  the later durable source-info work is absent from that archive. These validation run times are not performance measurements.
  Independent category-set comparisons also find zero lost entries in all six baseline categories for each
  history (default uses the full run plus its focused recovery). Counts are diagnostics 13,700, types/symbols
  12,998 each, JS 13,626, JS maps 149 and source-map records 156.
- The owned token handoff passes 83 AST tests, 13 parser tests, 15 scanner tests, the ten-input deferred-binder
  contract and the external-client test, now rejecting 35 clients. The new scanner case verifies allocation
  transfer, unchanged kind/range/flags, independent prior snapshots, borrowed source values and survival after
  scanner disposal. It passes Miri; both pure owned parser lifecycle/growth/speculation tests also pass Miri.
  The rebuilt entity-name oracle has zero differences against pinned Go on 2,163 inputs for both contexts.
  Workspace Clippy, source lint, registered regeneration and dev CLI/testrunner/example builds pass. Generated
  concrete scanner methods did not change. All 48 CLI regression cases also pass. This does not yet repeat full compiler conformance or establish a
  measured performance result for the latest prototype.
- Declaration snapshots pass the combined AST/parser/scanner/binder command: 83 AST library tests, 13 parser
  tests, 14 scanner tests, the ten-input real deferred-binder contract, and the external borrowing test, now
  rejecting 34 clients. The new runtime case covers snapshots across initial publication, release of read
  guards, published append/replacement, declaration/foreign buffer growth, interleaved spans, original order,
  wrong-owner rejection and final owner release. It passes Miri with `plain-ptrs`, including the public length
  helpers. The required dev CLI/testrunner build, workspace Clippy, source lint and registered regeneration
  pass; no source/generator hashes changed during regeneration. These remain prototype tests, not the real
  checker-symbol/clone path or an accepted performance result.
- Separate concrete production scanner lowering passes both full checker histories: all 15,448 variants,
  zero failures/crashes/timeouts, 13,700 diagnostic passes and 1,748 skips, with no diagnostic pass losses.
  New per-category pass sets match across the histories. Runtime was 306.5 / 308.9 seconds; these are correctness
  runs, not a speed comparison. The 82 AST tests, 13 parser tests, 14 scanner tests, deferred binder contract,
  external-client borrowing check, 13,081-file scanner oracle and 2,163-input entity-name oracle also pass.
  Workspace Clippy, source lint (692 files, 511 unsafe sites, 34 Send/Sync implementations) and registered
  regeneration pass. Regeneration/formatting changed none of 51 source/generator hashes.
  **Performance acceptance still fails:** Compiler-Unions single-checker peak RSS increases in all 24 paired
  runs (median +192 / +176 KiB), unlike the identical-binary control. The large generic scanner instruction
  penalty is absent from these screens; smaller instruction differences remain unresolved. The
  [performance note](ast-soa-performance.md#separate-concrete-production-lowering-experimental) records profiles
  and a completed matched lexical-integration isolation: restoring the old routines does not resolve RSS.
  Inlining the exact generated scanner text and restoring two observed parser call boundaries also fail to
  resolve the predecessor comparison. Removing the unused legacy lexical trait implementation, excluding all
  owned parser/scanner modules from legacy compilation, and omitting the two owned-only concrete Scanner methods
  also fail. Both newest candidates increase Compiler-Unions single-checker RSS in all twelve direct predecessor
  pairs (about +0.416% median). All four newest benchmark stages ran after every build/test job was terminal.
  No scratch changes are adopted; no production preparation or owned cutover is accepted. Source-organization
  experiments need new evidence before repetition. See the performance note for all results and provenance.
- Shared scanner preparation matches the pinned Go scanner on 13,081 public source files in all four oracle
  passes, with zero differences for either text policy; eight invalid-UTF-8 inputs were explicitly excluded.
  A 2,000-mutant parser run with seed `20261025` also had zero findings. Full production conformance subsequently
  passed all 15,448 variants in both checker histories with the actual `--timeout 600` argument, zero mismatches,
  crashes or timeouts (260.1 / 296.3 seconds). Diagnostics had 13,700 passes and 1,748 skips. The earlier run that
  set only the unused `TEST_TIMEOUT` environment variable had three timeouts and did not pass.
- Shared lexical grammar matches the pinned Go `ParseIsolatedEntityName` output on 2,163 generated inputs for
  both legacy and owned contexts. This covers valid tree ranges/flags/text/parents and rejection of invalid
  names; it does not compare Go's hidden recovery diagnostics. Separate runtime tests compare detailed recovery,
  contextual keywords, diagnostics/order, counters, literal text/flags and parents with the production context.
  Four new owned-parser tests pass; its two pure-owned lifetime/growth/speculation cases pass Miri. All five
  owned scanner tests, including borrowed comment iteration, also pass Miri. Source handoff, retirement and
  append-only speculative keys preserve both source and cooked text. Another 2,000-mutant full-parser run with
  seed `20261026` has zero findings.
- The combined lexical validation command passes 82 AST tests, 13 parser tests, 14 scanner tests and the
  external-client test, which now rejects 32 clients. New cases reject an escaping parser, source drop while
  it is live, node views across parser mutation/drop, and escaping comment iterators. Positive controls permit
  consuming the parser into a source-retaining builder. Generator regeneration, workspace Clippy and source
  lint pass; the current source inventory is 691 files, 511 unsafe sites and 34 Send/Sync implementations.
  Shared lexical production conformance with the outlined scanner passed all 15,448 variants in both histories
  with `--baselines all --timeout 600`, zero mismatches/crashes/timeouts. Diagnostics had 13,700 passes and 1,748
  skips in each mode; the saved full pass sets have no losses. Runtime was 328.7 seconds default and 532.6 seconds
  canonical, while release builds were active; these are correctness results, not performance comparisons.
  Logs are `target/ast-soa-scanner/lexical-conformance-600.log` and `lexical-canonical-conformance-600.log`;
  saved pass sets there are `conformance-lexical-{default,canonical}-pass.txt`.
  Separate performance screens have not cleared the gate.
- After deferred protocol validation and lazy metadata changes, the combined AST/parser/binder command below
  passed 82 AST tests (30 existing, 52 owned-storage), all nine parser tests, the ten-input legacy binder
  contract, and the external-client borrowing test. The latter now rejects 22 clients, adding live binding
  views across forcing, deferred-list lease/iterator escape, and live session node/list views across recursive
  forcing or private growth, with positive controls for reads ending before mutation.
  New runtime cases cover same-list/two-list recursion, concurrent different lists and cached-symbol readers,
  returned errors and panics, caught nested/initial panics, same-slot foreign contexts, forged local edges,
  unretained syntax dependencies, binder-retained fragment registration, retirement during forcing and bounded
  run-slot progress. Hooks construct schema nodes directly: these are not real owned parser/binder parity tests.
  All 52 owned-storage runtime cases also pass the pinned-nightly Miri command below with `plain-ptrs`.
  Workspace Clippy with warnings denied, source lint, development CLI/testrunner build and generator regeneration
  (`tools/gen-check.sh --write`) pass. The source inventory remains 511 unsafe sites and 34 Send/Sync
  implementations across 687 files; the safe storage module adds neither.
- `cargo test -p tsrs_ast --lib --test storage_borrows -p tsrs_parser -p tsrs_binder --test lazy_binding_contract`:
  68 AST library tests passed (30 existing, 38 owned-storage), all seven parser unit tests and the real binder
  contract test passed, plus the external-client compilation test. The latter compiles positive controls and
  rejects sixteen clients, covering escaped node/parsed-guard/text/symbol/annotation lifetimes, mutation with live
  node/session/binding/annotation views, owner/guard drops with live views, and list/table/raw/text-list iterator
  lifetimes. These validate compiler-enforced borrowing, not production owned-parser integration.
- `cargo +nightly-2026-10-04 miri test --target-dir target/ast-soa-miri -p tsrs_ast --lib --features tsrs_core/plain-ptrs storage::`:
  all 38 owned-storage tests passed, including failure wakeups, bounded run-slot progress, shared-file lifecycle,
  typed payload validation, factory flag masks, table-address boundaries/rollover, sparse list fields,
  missing-list identity, allocation accounting, source/owned text growth, binding namespace validation,
  symbol declaration replacement, flow domains, base/fragment retention, cached-symbol readers during writes,
  ordered table mutation/cloning, all canonical token kinds, mixed JSDoc text/raw node spans, class/file annotations,
  container links in both directions across fragments, traversal mutation and shallow clone identity/reset rules.
- The required development CLI/testrunner build passed after the expanded owned-storage implementation.
- `cargo test -p tsrs_binder --test lazy_binding_contract`: the nine real parser/binder contract cases passed,
  including forcing through shared-data symbols, in-file merge order, saved flow stability and parser exclusions.
  Their eager binder dumps also match the pinned Go oracle. This validates legacy behavior to preserve; the
  complete owned source-file parser/binder path is still absent.
- The subsequent shared-finalization change passes all nine parser library tests (seven existing plus two
  owned finalization/recursive-growth cases). Those two owned cases also pass Miri with `plain-ptrs`. The binder
  contract now covers ten inputs, adding same-list reentry through a constructor parameter property; it passes.
  The extra input's eager binder dump also matches the pinned Go oracle exactly (309 bytes).
- After shared finalization, a second differential parser run compared 2,000 mutants with seed `20261024`,
  with zero findings. The nine parser tests, full-workspace Clippy, source lint and development CLI/testrunner
  build passed. Generator regeneration completed; the legacy generated AST has no diff. The source inventory
  is now 686 files, still 511 unsafe sites and 34 Send/Sync implementations.
- Shared-finalization production conformance selected all 15,448 variants and passed every applicable baseline,
  with zero mismatches, crashes or timeouts using `TEST_TIMEOUT=600` (316.2 seconds total). Diagnostics had
  13,700 passes and 1,748 skips. The original baseline runner also passed the slow constructor-reduction case
  in 288.3 seconds under the same limit. These concurrent correctness runs are not timing measurements; they
  confirm that exceeding the standard 120-second limit predates the migration. Alternate-history conformance
  (`TS_TEST_PROGRAM_SINGLE_THREADED=false TSRS_HISTORY=canonical`) also passed all 15,448 variants, with the same
  counts and no failures/crashes/timeouts, in 304.2 seconds under the 600-second limit. This does not turn the
  earlier 120-second gate into a pass.
- The Go binder-oracle harness was updated to the pinned reference's symbol getters and typed path constructors;
  it builds and directly matches the nine cases above. The reference checkout itself has no tracked changes.
- The AST oracle also needed updating: the reference now embeds Node in concrete payloads and no longer has the
  reflected `Node.data` interface. Its generator now emits canonical checked-cast dispatch for reflection, and
  path construction uses the pinned typed path APIs. A simple parser dump matches exactly. Differential parser
  fuzzing with seed `20261023` compared 2,000 mutants with zero findings, using the repaired Go oracle and the
  production Rust parser. This validates the legacy parser preparation, not an owned parser.
- The full development conformance run selected 15,448 variants: zero baseline mismatches and zero crashes, with
  one timeout (`compiler/intersectionConstructorReductionCrash`). The isolated rerun with one worker and a
  300-second limit passed all applicable baseline categories in 221.3 seconds. The original 120-second gate
  did not pass; this rerun does not establish that the timeout is unchanged from the baseline compiler.
  Subsequent shared-finalization runs in both histories passed under a 600-second limit, as recorded above.
- The source lint remains at 511 existing unsafe sites and 34 existing Send/Sync implementations; the new storage
  code adds neither. Full-workspace Clippy passed with grouped tables, sparse payload/list fields and memory
  accounting. `tools/gen-check.sh --write` regenerated all outputs; the legacy AST generated file has no diff.

## Real paths checked against this revision

The reference checkout still matches Cargo's pinned `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with no changes under
`ts-ref/tsc`. These observations constrain integration; container tests alone do not establish their migration:

| Existing path | Observation and remaining integration |
| --- | --- |
| [`Program::bind_source_files`](../crates/tsrs_compiler/src/program.rs#L1159) | Collects unbound files and binds in reverse file order when single-threaded; preserve timing and symbol-ID effects. The new entry never starts binding implicitly from a read. |
| [`new_parse_cache`](../crates/tsrs_project/src/parsecache.rs#L122) | Parses, writes the content hash and binds before publishing the cache value. Every Program retaining the version keeps the same identity. Replace region retention with the new owner without changing this ordering. |
| [`fileregions::parse`](../crates/tsrs_compiler/src/fileregions.rs#L369) and [`parse_source_file_keep_text`](../crates/tsrs_parser/src/parser_1.rs#L256) | CLI leaf bodies keep source text and SourceFile outside the releasable scratch region. Retaining the new source owner after retiring its body preserves this existing boundary. |
| [`is_missing_node_list`](../crates/tsrs_parser/src/parser_1.rs#L210) | Recovery distinguishes a missing opening token from an ordinary empty list by sentinel slice identity. The new list span encodes this state explicitly; nil, empty and missing cannot collapse during the parser migration. |
| [`bind_lazy_list`](../crates/tsrs_binder/src/binder.rs#L127) | Creates a binder using saved context and drops late bind diagnostics/symbol count. Preserve that behavior. The isolated storage protocol supports deferred callbacks; the production binder is not connected to it. |
| [`bind_lazy_members`](../crates/tsrs_binder/src/binder.rs#L2661) | Late forcing uses `splice=false`; it cannot splice new containers into the published file chain. Initial inline forcing uses `splice=true` and the existing exclusive binder. |
| [`add_declaration_to_symbol`](../crates/tsrs_binder/src/binder.rs#L3740) | Forcing and merges touch existing symbol flags, declarations, value declarations and AST declaration-symbol annotations. The deferred gate must protect the actual data and all readers, including already-cached symbols. File locality still needs the complete callee audit. |
| [`set_flow_node_referenced`](../crates/tsrs_binder/src/binder.rs#L863) and [`lazy_list_blocked`](../crates/tsrs_parser/src/parser_1.rs#L3757) | Flow helpers mark antecedents Referenced/Shared, but the parser excludes flow-producing syntax from lazy lists. Preserve exclusions and complete the callee proof; saved flow flags remained unchanged in the nine contract cases. |
| [`Symbol::data_owner`](../crates/tsrs_ast/src/symbol.rs#L149) | Instantiated checker symbols can share binder declarations/tables/value-declaration storage. Protection follows that data owner even if the reader cached a checker symbol. |

## Outstanding work

The common-only allocator remains a prototype API; typed factories cover all schema payloads except retained
synthetic semantic types. Schema binding annotations, ordered symbol tables, raw lists, child-edge traversal and
local shallow clones now exist. Durable SourceFile metadata, source-version identities and mapped bundles have
isolated prototypes; loader/file-entry integration and diagnostics remain pending. Synthetic semantic payload retention,
factory hooks, updating visitors and cross-store cloning remain unimplemented. The callee audit must still validate the complete
field/read/write ownership protocol in production.
Low-level binding lease APIs now validate writes but remain probe primitives; production callers must use the
explicit forcing/session facade. The file-owner protocol
is not wired into host loading, cache reuse or real Program retirement. It currently owns
no legacy semantic allocation region, so it cannot yet be used to retain production binder/type/diagnostic
dependencies. The isolated deferred-member state machine is implemented and tested; complete owned source-file
parser/binder integration remains absent. The new owned context runs shared lexical, token-recovery and list
grammar, without a complete source-file entry. The production
parser's exclusive construction facade still allocates arena nodes; direct node reads/mutations, JSDoc and source-file metadata
remain to be ported before replacing that backend.

Next: finish the field/owner and [deferred-callee audit](ast-deferred-binding-audit.md), then a real parser/binder
feasibility path using the existing guarded annotations and tables. Count table headers, spare capacity,
source/owned text, directory registrations and retained bodies before broad porting. Common metadata and eight-byte public keys are not
accepted production costs merely because their local correctness tests pass.

The next parser dependency is JSDoc and constructing function-type lookahead, followed by connecting the complete
owned grammar, its speculative paths, source-file entry and actual lazy/reparse producers. Shared type/expression/
statement/declaration bodies still run only through the legacy backend.
The list test's recursive array helper deliberately supplies element callbacks and cannot replace
`parse_argument_or_array_literal_element`, which invokes the full assignment grammar. Type lookahead is also
not uniformly token-only: `next_is_unambiguously_start_of_function_type` calls `skip_parameter_start`, which can
construct modifiers and binding patterns speculatively. Preserve these calls, their diagnostics and append-only
allocation/counter effects rather than replacing them with token scans. A preliminary self/callback call inventory
is saved in `target/ast-soa-scanner/parser-grammar-dependencies.json`; it includes multiline method calls and
finds a large recursive group shared by arrays, types and the source-file entry. It is not a complete field,
free-function, indirect-callback or ownership audit, and supplies no justification for narrowing parser semantics.

The now-shared import grammar has a distinct diagnostic boundary, checked against pinned Go `parseImportType`,
`parseImportAttributes` and `parseErrorAtRange`. After a missing closing brace it inspects the last diagnostic
for code `X_0_expected`, even when same-position suppression prevented a fresh diagnostic. The generic matching-
bracket helper instead uses the diagnostic returned by the fresh error attempt; substituting it would lose
related information in that case. The shared path uses a copied last-diagnostic handle and the existing
related-info writer, preserving both nested brace additions. The targeted oracle corpus includes an unfinished
`type T = import("x", { with: { type` case. Import-type source flags are set before
parsing its tokens and survive ordinary rewind; the owned state must preserve that behavior. Attribute values
still require full assignment-expression recovery.

Follow-up review made table installation explicit in the audit: `get_members`/`get_exports` can both force and
create tables, and `get_locals` can install locals. Conflicting accessor declarations also update an existing
symbol's flags. These paths require the active binding writer. Direct flow branches now distinguish fresh
constructor/signature flows from annotations that refer to saved flows; the transitive audit remains open.
The architecture records unanswered questions about public Rust API changes and a possible payload-record
fallback. The preceding handoff assumed the API change was acceptable; that was an assumption, not owner approval.

## Current layout gate

The `storage_memory` example using `size_of` on this arm64 host reported these inline metadata sizes. They exclude
Arc control blocks, all heap allocations, capacity slack, table/registration hash slots and source bytes:

| Type | Bytes |
| --- | ---: |
| `AstStoreBuilder` | 816 |
| `AstStore` | 328 |
| `FileAstOwner` | 120 |
| `AstStoreLease` | 8 |
| `BindingData` | 400 |
| `BindingLease` | 8 |
| `NodeKey` / `ListKey` | 8 |
| `PayloadTag` | 1 |

The earlier global-common prototype used 696 inline builder bytes, 664 inline store bytes and 27 common element
bytes plus a marker bit. The implemented grouped layout removes per-node payload tags/indexes and omits kinds
in fixed-kind tables. Its common element widths sum to 20 bytes (flags 4, range 8, parent 4, semantic ID 4), plus
one marker bit; variable-kind Token/CommonOnly tables use 22 bytes plus a bit. The builder alone retains a
390-byte tag-to-latest-table lookup. The store includes foreign-list metadata and an explicit binding-owner
identity added after the earlier size probe. Binding records currently occupy 32 bytes per symbol, 12 per flow,
and 8 per flow-list element, before capacities, auxiliary records, declaration buffers, text and maps. Binding
buffer accounting is now available below; these widths still cannot be compared directly with whole-compiler RSS.
The deferred protocol first raised inline binding metadata from 384 to 488 bytes. Boxing lazy-list bookkeeping
and allocating write journals only when published writes occur reduced that to 400 bytes. This adds two optional
pointer fields to an ordinary file; affected files also own the boxed headers, map buckets and live buffers.
It is not an RSS improvement claim, and all those allocations must be counted at the production gate.
These element widths exclude payloads, tables and spare capacity. Published metadata improved; parser/binder
speed and complete compiler RSS remain unmeasured.

`NodeIndex` now uses the checked 11-bit table / 20-bit element partition within the existing 31-bit local domain.
Overflow adds another table for the same payload without reusing keys. The final element of the final table is
reserved by one-based/foreign encoding. Tests force small table limits and check boundaries, stale/wrong-owner
keys, foreign edges, and published reads under Miri. Keys are opaque addresses, not allocation-order IDs.

Reproduce metadata/capacity measurements on this arm64 host with:

```sh
cargo run --quiet --target-dir target/ast-soa-tests -p tsrs_ast --example storage_memory
```

Selected actual requested buffer totals (bytes), excluding the 328-byte store itself, source/Arc ownership,
file owners/directories, all binding-data allocations, allocator size classes and resident-page effects:

| Payload case | Requested nodes | Live bytes | Capacity bytes | Live allocations |
| --- | ---: | ---: | ---: | ---: |
| Empty store | 0 | 0 | 0 | 0 |
| Token | 1 | 190 | 328 | 8 |
| Token | 1,000 | 22,288 | 22,864 | 8 |
| Source-text numeric literal | 1 | 248 | 416 | 9 |
| Source-text numeric literal | 1,000 | 32,336 | 33,152 | 9 |
| New expression, no type arguments | 1 | 536 | 856 | 20 |
| New expression, no type arguments | 1,000 | 28,628 | 29,512 | 20 |
| New expression, 5% with type arguments | 1,000 | 29,028 | 30,024 | 21 |

Each new-expression case also owns one numeric child and one one-element list, so its actual node count is N+1.
The sparse case adds 50 eight-byte entries (element address + compact list edge) and a 64-entry buffer capacity.
No sparse per-node slot is allocated for absence. Tiny-table overhead remains a concrete feasibility risk:
smaller element widths do not establish acceptable costs for small files or for many distinct payload tables.
A common-SoA/record payload hybrid remains a measured fallback. Do not expand this layout across production
consumers until the remaining real parser/binder and workload gates pass.

### Binding buffer accounting

The same `storage_memory` command now exercises ordinary binding tables at 0/1/16/17/128/1,000 entries,
table cloning with an odd alias, annotation columns, a retained fragment during/after a published write, and
pending/published lazy-list records. Counts on this arm64 host are requested bytes and actual Vec/box/hashbrown
index allocations. They exclude the 400-byte inline `BindingData`, lock/owner/Arc storage, source/syntax bodies,
allocator overhead, and standard hash map backing. The latter is a separate payload lower bound, not silently
treated as zero or as an exact bucket size. No representation fields were added for accounting.

| Ordinary binding case | Live buffer bytes | Capacity bytes | Allocations |
| --- | ---: | ---: | ---: |
| Empty namespace, no table | 0 | 0 | 0 |
| Allocated empty table | 32 | 128 | 1 |
| One symbol/declaration/flow/annotation and table entry | 132 | 432 | 8 |
| 16 entries with the same fields | 1,032 | 1,192 | 8 |
| 17 entries, hashed table | 1,216 | 2,416 | 10 |
| 128 entries | 8,320 | 9,464 | 10 |
| 1,000 entries | 64,128 | 72,104 | 10 |

Each ordinary entry uses source-backed names and one declaration; the value declaration uses its first-entry
encoding, and identifiers have flow annotations. These are constructed storage cases, not actual compiler files.
Cloning the one-entry table, installing symbol member/export tails and adding one alias raises that case to
277 live / 824 capacity bytes and 14 allocations. After its first fragment write completes, it retains
393 live / 1,008 capacity bytes and 19 allocations, plus three standard maps with 60 live / 180 capacity payload
bytes. The fragment syntax itself is counted separately. A completed write releases journal element buffers
but retains its 48-byte boxed header; a first public write currently allocates that header even without new
dependencies. This is an exposed prototype cost, not a measured compiler regression or accepted overhead.

One pending lazy-list record requests 64 live / 88 capacity buffer bytes (two allocations), plus a standard
map with 104 live / 312 capacity payload bytes. Initial publication drops its queue, leaving a 56-byte boxed
header and that map. Tiny-file overhead, map backing and retained syntax still need real workload accounting.
The direct smoke run is `target/ast-soa-scanner/binding-memory.log`; the complete compiler gate remains unpassed.

### Durable source information accounting

Before source-version identity was added, the same `storage_memory` example reported 208 inline bytes for
`SourceFileInfo` and its exclusive draft on this arm64 host; the current size is 224 bytes, including alignment.
That includes parse options, an Arc text handle, scalar results/hash, optional directive/check-JS pointers
and both inline OnceLocks. Neither AST body nor file-owner layouts changed. These are constructed allocation
counts, not compiler memory measurements.

The following cases all own a 10-byte filename buffer. The separately shared path payload is 10 bytes; source
payloads are 0 / 8 / 14 bytes for empty / ASCII / Unicode. All exclude Arc control blocks, enclosing owners,
allocator rounding/overhead and RSS. Pragma maps, when present, are reported separately as payload lower bounds.
Deduplicate shared directive blocks and their pragma maps across updates using the borrowed block's identity.

| Case | Directive live/capacity bytes | Map live/capacity bytes after initialization | Directive/map allocations |
| --- | ---: | ---: | ---: |
| Empty text | 0 / 0 | 4 / 4 | 0 / 1 |
| ASCII with two newlines | 0 / 0 | 12 / 12 | 0 / 1 |
| Unicode with CRLF and U+2028 | 0 / 0 | 36 / 48 | 0 / 2 |
| Same Unicode, one owned reference | 197 / 317 | 36 / 48 | 3 / 2 |

All text-map buffers are zero before initialization, including after line counting on a cache miss. ASCII
position maps need no entry buffer. Empty directive blocks and their reserved buffers are discarded on
finalization. A separate check-JS-only case uses no directive block and requests one 16-byte value allocation,
plus 8 live/capacity bytes for its two-entry line map; its shared source payload is 13 bytes. SourceFile updates
share immutable directive vectors/strings. A later draft edit copies shared data under exclusive authority;
Arc/refcount and edited-copy costs remain unmeasured. No whole-compiler no-regression claim follows.
The direct output is retained in `target/ast-soa-scanner/source-info-memory.log` and current changed/untracked
inputs in `source-info-source.{tar.gz,json}` beside it. This slice remains isolated from parser/loader publication.

### Mapped source information accounting

The constructed probe now includes a metadata bundle with 0/1/8 supplemental outputs, all sharing one
one-segment map and one source-text allocation. Inline sizes on this arm64 host are 136 bytes per bundle,
64 bytes per output record and 24 bytes per retaining file handle. Each source-info object separately occupies
224 inline bytes; the new four-byte identity moved that object from 208 to 224 bytes with alignment.
Neither body nor file-owner layouts changed. These are prototype costs to account for at the no-regression gate.

Requested bundle buffer totals, including the shared map object once and all buffer capacity:

| Supplemental outputs | Before reverse lookup: live / capacity bytes | Allocations | After reverse lookup: live / capacity bytes | Allocations |
| --- | ---: | ---: | ---: | ---: |
| 0 | 209 / 209 | 7 | 241 / 241 | 9 |
| 1 | 289 / 303 | 8 | 321 / 335 | 10 |
| 8 | 849 / 961 | 15 | 881 / 993 | 17 |

Reverse lookup initializes the existing immutable interval index; its two buffers add 32 requested bytes here.
The separate source-info filename buffers total 11 / 27 / 139 bytes with 1 / 2 / 9 allocations. All these totals
exclude bundle/source-info inline storage, Arc control blocks, shared source/path payloads, AST bodies,
allocator overhead and resident-page effects. The path/text payloads and per-info buffers are exposed separately
for deduplication; do not sum shared payloads once per handle. The direct output is
`target/ast-soa-scanner/mapped-source-memory.log`. No compiler RSS or performance acceptance follows.

Matched release artifacts were rebuilt with `profile.release.debug=true` and `profile.release.strip=false`:

| Artifact | Source | SHA-256 |
| --- | --- | --- |
| `target/ast-soa-baseline/release/tsrs` | Git archive of `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1` | `0f1c00c32b2942a3a5adf8f6b14680771b5502dc976c967b46f06262d960299e` |
| `target/ast-soa-candidate/release/tsrs` | Uncommitted storage foundation, exclusive legacy parser factory and oracle repairs | `863e0a80a78d9f260369d6290da47a8587705a67ca4c9327748b3958a87caef6` |

The baseline source is `/private/tmp/tsrs-ast-soa-base-19ba886`. The candidate measures production preparation;
the unused owned backend does not make it an owned compiler. Hyperfine 2.0.0 screening completed across all 19
catalog projects with warmups, three alternating baseline/candidate pairs, single-threaded instructions and
default-checker RSS. [The results](ast-soa-performance.md) include increases requiring follow-up; this screening
does not pass the no-regression gate. Raw data/provenance live under `target/ast-soa-bench/preparation/`; the
candidate binary is retained there as `tsrs` before rebuilding for shared finalization. These are local ignored
artifacts. The shared-finalization source change is not included in these measurements.
Full reference parity, lifecycle/retention coverage and interleaved catalog measurements remain required.
No candidate with a repeatable speed/instruction/memory regression is authorized to land.

The subsequent shared-finalization run completed all 19 catalog projects, six balanced pairs per checker mode:
456 measurements with matching output/status and validated binary hashes. The complete table is in
[the performance note](ast-soa-performance.md#shared-parser-finalization). Several instruction, RSS and wall
medians increased. Compiler default RSS increased in all six pairs (+0.248% median), prompting 12 focused pairs
and an identical-baseline control. The repeat had +0.062% median default RSS (8/12 positive) against -0.124% for
the control (5/12 positive), so attribution remains unresolved. Single-threaded candidate RSS was +56 KiB median
(11/12 positive), against +16 KiB for the control (7/12 positive); that possible small cost remains open too.
The no-regression gate is unpassed. These runs measure legacy production preparation, not an owned compiler;
neither unrelated improvements nor passing ownership-container tests clear the remaining gates.

The current 89-body type/import/expression preparation also completes all 19 catalog projects with six
balanced pairs in both checker modes: 456 measurements with matching output/status and checked binary hashes.
The [performance note](ast-soa-performance.md#current-typeimportexpression-preparation) records the complete
table. VS Code and Compiler-Unions single-checker instructions increase in all six pairs; Compiler-Unions
single-checker peak RSS also increases in all six (median +0.443%). Focused repeats and identical-baseline
controls add another 96 measurements. The twelve-pair Compiler-Unions repeat increases peak RSS in every
pair (median +264 KiB / +0.430%), against a zero-median identical-baseline control. The preparation therefore
fails the memory gate. VS Code's instruction increase does not repeat consistently; the control also varies
substantially, so no instruction improvement or regression is attributed there. Checked results and raw
measurement hashes are in `target/ast-soa-scanner/type-expressions-followups.json`. This measures legacy
preparation; the real owned compiler and retention/lifecycle costs remain unmeasured. No production change
with the observed repeatable memory cost is authorized to land.
