# AST parser rollback and reclamation

2026-10-24. This is a focused source audit for the owned AST migration. It does not establish a reclamation
implementation or pass the no-regression gate. The reference checkout is clean at the pinned
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.

Explicit owned-parser `mark`/`rewind` still retain AST allocations made after a snapshot, preserving ordinary
keys. Shared lookahead now borrows a freshly branded parser and reclaims unretained allocations; conditional
parsing promotes an accepted node to its enclosing scope. This covers the grammar currently shared with the
owned context, not complete production reclamation. Production cannot switch with remaining costs hidden.
Historical reclamation numbers
are in [mem-recycle.md](mem-recycle.md); the lazy declaration-list saving is a separate, larger obligation in
[mem-round4.md](mem-round4.md).

## State restored by an ordinary rewind

The reference `ParserState`, Rust `ParserState` and owned prototype all preserve allocation counters across
rewind. Storage addresses are separate from node/identifier counts and observable semantic IDs.

| State | Production behavior | Owned migration obligation |
| --- | --- | --- |
| Current token, positions, value, flags and JSDoc asterisk nesting | Scanner snapshot restored; parser token refreshed from scanner | Snapshot owns cooked token text; borrowed source text stays valid through the scanner's source lifetime |
| Scanner comment directives | Snapshot stores the visible length; rewind truncates the directive vector | Reclaim truncated entries without changing directive order or visibility |
| Number, hex-number and hex-digit caches | Survive scanner rewind; a new arena-backed entry can pin the arena checkpoint | Owned cache entries already own/borrow their text independently; never truncate their strings with AST text |
| Context flags, parse-error flag and statement-await flag | Restored | Implemented for current owned grammar |
| Parse diagnostics | Truncated to saved length | Implemented with owned arguments and related information |
| JavaScript diagnostics, JSDoc info, reparsed clones and lazy records | Each restored to its saved length | Not present in the owned parser snapshot yet; add with the corresponding production grammar |
| JSDoc diagnostics | Deliberately survive ordinary rewind | Own moved errors and any source dependencies; preserve their existing timing/order |
| Node and identifier counters | Not rewound | Keep the counter values even when backing buffers are reclaimed |
| Node stack, parsing-context bitset and reparse list | Not restored by `ParserState`; grammar scopes manage them, with deliberate reparse escapes | Do not silently add generic truncation. Prove each callback balances its stack/context and handles outward reparse propagation |
| Source flags, deprecated marker, arrow disambiguation cache and parser scratch buffers | Not part of `ParserState` | Audit writes when their grammar is ported; a scanner snapshot is not a full parser transaction |

Sources: `parser_1.rs` (`ParserState`, `mark`, `rewind`, `rewind_keeping_nodes`), `scanner_impl.rs`
(`ScannerStateImpl`, `mark`, `rewind`), and the matching reference parser/scanner code. The root shared context
delegates legacy snapshots to these existing methods. Its owned snapshot intentionally covers only the state
implemented so far.

## Allocation boundaries with different behavior

| Boundary | What the source actually does | Required handling |
| --- | --- | --- |
| Token recognition and contextual modifiers | Scans tokens, sometimes produces scan diagnostics/cooked token text; no AST construction in the shared recognition bodies | Restore scanner/errors; retain cache-owned text independently. Do not impose a full AST-column snapshot on every token-only call without measuring it |
| Function-type lookahead | `next_is_unambiguously_start_of_function_type` calls `skip_parameter_start`, which can parse modifier lists and binding patterns | This boolean lookahead can allocate a real AST subtree, including expression grammar through binding defaults/computed names. It needs reclamation or measured equivalent retention |
| Arrow disambiguation | `is_parenthesized_arrow_function_expression` saves state and returns a tristate. Its current recognition worker scans tokens; the subsequent possible-arrow parse may construct an entire signature/body | Keep the token classifier and conditional AST parse distinct. A failed possible-arrow parse rewinds; a successful one retains its result |
| Infer constraints, dotted keyword types, type predicates and type arguments in expressions | Parse nodes speculatively, then keep them only when the trailing syntax accepts the interpretation | Reclaim rejection allocations while preserving counters and the exact retry position |
| Constructor declaration probe | Rejection scans only the constructor-name check; construction starts after that check succeeds | Do not treat every marked region as necessarily producing discarded nodes |
| JSDoc links and child/return tags | Prefix recognition may be token-only, but conditional child/tag parsing can construct nested nodes before rejection | Include JSDoc text, diagnostics, lists and clone bookkeeping in the audit; a prefix-only test does not prove the full path |
| Top-level-await reparsing | `reparse` calls `rewind_keeping_nodes` after retaining new statements; it also preserves the new diagnostic length explicitly | Retain the resulting nodes and their text/list dependencies. Do not reclaim merely because scanner state is restored |
| Lazy declaration-member capture | Parses the list, checks diagnostics/JSDoc/directives/source flags, escaped allocations and forbidden shapes, then discards accepted syntax and stores a lazy record | Preserve eligibility and nested-record behavior. On rejection retain the ordinary list; on acceptance release the captured body without rewinding scanner progress or counters |

Ordinary lookahead's boolean return does **not** prove that allocation cannot escape: a callback can also mutate
parser fields. Do not generalize lookahead to return nodes and then truncate its allocations. The separately
marked tristate classifier does not justify changing the public shape of ordinary lookahead either.

## Retained values found in production

- `reparse_unhosted` pins before appending synthesized typedef/callback declarations, imports or overload
  signatures to `reparse_list`. These declarations can propagate outward through list grammar, including to
  source/block contexts that accept them. Restoring the list length at every rewind would change behavior.
- `parse_jsdoc_comment` moves diagnostics into `jsdoc_diagnostics` for JavaScript files and pins when the moved
  set is nonempty. Ordinary rewind does not truncate that destination.
- Scanner numeric caches are outside scanner snapshots. The legacy text policy pins if a cached key/value
  uses arena text; the owned policy stores independent `Cow` values. These cache values must not refer into a
  truncated AST cooked-text buffer.
- Top-level-await reparsing keeps statements and adjusts their parents on the final source file. Its explicit
  keep-nodes path is separate from the three ordinary escape barriers above.

These are direct source findings, not a completed transitive write audit. Existing-node mutations, JSDoc clone
maps, failed-parser cleanup and allocation-failure paths still require review before implementing reclamation.

## Key identity is an unresolved design constraint

Simply truncating all columns would reclaim their elements but reuse the same table/element addresses on the
next append. Current public keys have an owner identity and a compact local address; they have no generation.
An abandoned key could then identify a different node. This contradicts the prototype's nonreuse contract and
tests. Bounds checking alone does not prevent that semantic alias.

The following are candidates to investigate, not accepted changes:

| Candidate | What it would establish | Cost or missing proof |
| --- | --- | --- |
| Scoped speculative handles, promoted only on commit | Private rejected handles cannot escape; private slots can be reused while all retained keys stay stable | Needs a Rust type/API proof across real recursive grammar, nested speculation and outward JSDoc propagation; explicit retention must cancel reclamation where required |
| Checked generations or logical-to-physical mapping | Reject stale handles after physical storage reuse | Changes key/lookup costs and possibly edge width. Must count generations, mappings and every read, not just reclaimed payload bytes |
| Separate temporary stores transferred on commit | Rejection drops ordinary owned buffers; retained owners have distinct identities | Potentially many stores, tables, foreign edges and allocations; successful parses cannot require a second complete AST copy |
| Retained empty slots or sealed tables | Avoids address reuse while dropping some payload data | Common columns, descriptors, capacities and table-domain exhaustion remain; this is not complete reclamation |

The first candidate is worth a small feasibility proof because it need not add a generation to every persistent
edge. It is not yet selected: the current plain-key grammar permits keys in mutable parser state, and changing
only the closure return type cannot close that path. Any proof must make escaping storage writes explicit.
No candidate may introduce unsafe code, fabricate lifetimes, or accept a measured performance/RSS increase.

A standalone safe-Rust type probe at `target/ast-soa-scanner/rollback-scope-probe.rs` checks one part of the
first candidate. Its private writer exposes scoped keys through a higher-ranked callback, permits nested
lookahead and vector growth, and converts a scoped result to a public key only when conditional parsing commits.
Discarded values are truncated while the allocation counter stays monotonic; previously retained keys remain
valid. The direct executable passes. Six external-module compilation controls reject captured speculative keys,
captured conditional-parse keys, escaped text views, growth with a live text view, access to the private writer,
and extraction of a raw key index, with the expected borrow/lifetime/privacy errors. The JavaScript smoke driver
is `check-rollback-scopes.mjs` and results/source hash are in `rollback-scope-checks/results.json` beside the probe.

This is only a type-boundary feasibility result. The probe does not implement this project's grammar, column
rollback, reparse escape promotion, cross-owner keys, allocation failure or panic cleanup. It does not prove
that the current generic parser can adopt these scopes without extra copying, code growth or runtime cost.
Those limits must remain explicit when using it to choose the next experiment.

A second standalone probe, `rollback-generic-scope-probe.rs`, checks shared recursive grammar over stable and
scoped handle types with nonstatic borrowed source. Its speculative associated type separates the fresh handle
brand from the mutable writer borrow; callbacks can recursively run a fresh scope without requiring a static
source or copying retained node records. Rollback continues through the scope's exclusive writer rather than
attempting a second mutable borrow of the original writer. The executable preserves the retained tree and
counters. Five additional compilation controls reject escaped outer/nested keys, an escaped scope and private
writer/key access (`check-generic-rollback-scopes.mjs`, `rollback-generic-scope-checks/results.json`).

This still is not a drop-in change to `LexicalParser`: the existing `FnOnce(&mut Self)` lookahead fixes the
callback's node type, while fresh scopes require a separately branded parser type. Callback qualifications and
the list hooks exposing `Vec<Self::Node>`/slices need a concrete adaptation; transmuting those collections to
change their handle lifetime is prohibited. The standalone recursive proof contains neither those real list
stacks nor the project's outward reparse state. No production source uses either scope probe.

An implementation needs generated, coordinated truncation of common/payload columns, sparse entries, list and
modifier descriptors/elements, cooked text/spans and any new table metadata. It must preserve source-backed text
and cache-owned values, retain escaped graphs, and keep counters monotonic. `Vec::truncate` alone also leaves
capacity allocated: test repeated failed parses, lazy-list capture and post-publication retained bytes, then
measure peak RSS and retired instructions on actual compiler workloads. A passing Miri container test would
prove neither memory parity nor the absence of semantic stale-handle aliases.

Lazy member capture makes the capacity issue especially important. A large declaration list can be the high
water mark of several payload columns even when its elements are immediately discarded. Truncating their
lengths would keep those allocations attached to the published base store; the lazy record would then postpone
binding while retaining much of the syntax allocation it was meant to release. Scoped keys solve neither this
capacity retention nor allocator page residency. Investigate transaction-owned buffers or reclaimable column
blocks, or a measured trimming/compaction boundary, before claiming parity with existing lazy-list discard.
Any such layout must include successful-list adoption, rejected lazy eligibility and tiny-file/table overhead
in its measurements. This is a reasoned ownership/capacity constraint, not a new measured regression.

## Real storage scope candidate

The experimental owned backend now implements
[`AstScope`](../crates/tsrs_ast/src/storage/speculation.rs) over its actual generated columns. Its storage-only
integration left parser snapshots unchanged; the subsequent shared-parser candidate is described below.

The callback receives freshly branded `ScopedNode`, `ScopedList` and `ScopedModifiers` handles. Generated
factories, setters, typed views, list/slice iterators and child cursors preserve that brand. They expose neither
an ordinary key nor the underlying writer. `ScopeError` keeps its underlying key-bearing error private. Raw
stable-key constructors are now storage-private: allowing callers to manufacture a stable key for a temporary
slot would defeat this boundary. This restriction is part of the experimental API, subject to the still-pending
public Rust caller contract.

Conditional commit promotes a handle to its immediately enclosing brand; root commit produces a stable key.
Explicit `retain_node/list/modifiers` returns a stable key only after pinning every ancestor. An edge write into
a node predating a frame also pins that frame. An inner write into an outer temporary node pins the inner frame
without preventing the outer frame from discarding the whole graph. Cross-writer speculative rebranding is
rejected; explicitly imported foreign stable keys remain supported. Scalar/range writes to existing nodes are
not undone, matching the allocation-only nature of legacy arena rewind. This is not a general mutation transaction.

At the first storage write, each unpinned frame lazily creates a boxed checkpoint with table lengths, latest-table
lookup, list/text lengths and foreign-vector lengths. Token-only scopes create no checkpoint. This avoids adding
persistent per-node/store fields, but introduces snapshot allocation, traversal and copying costs for real writes.
The constructed layout check bounds the scope object to 64 bytes; no performance claim follows from that bound.
Rejection and panic restore all generated syntax columns, sparse entries, comma bits, lists/modifiers/elements,
cooked text/descriptors, foreign deduplication indexes and new table metadata. Node/text counters remain monotonic.

`RollbackBuffers::Reuse` retains survivor-column capacity; `Release` requests trimming after truncation. The
constructed high-water test verifies a reduction in requested capacity after discarding thousands of identifiers.
It does not show allocator page release, peak RSS parity, lazy-list parity, or acceptable execution cost. Trimming
all surviving columns can copy retained data and is not an accepted production lazy-member strategy.

The isolated candidate passes ten real-storage scope tests under Miri, 104 AST unit tests, AST-target Clippy and
59 external-client rejection controls with a compiling positive control. Eleven rejection cases are new: key/
collection escape, nested/conditional escape, live-view mutation, private writer/raw key/error access, raw-key/
index forgery and typed-view conversion to a plain key. Unrelated legacy AST/parser generated outputs match root
byte for byte. Candidate/integration evidence is in `target/ast-soa-scanner/storage-scope-{candidate,root-integration}.json`.
Root workspace validation is recorded separately; the earlier disk-full cache/build failures are retained in logs.

The storage-only proof required a real grammar experiment with parser-owned state. Its fresh callback type must integrate
scanner/diagnostic restoration, `Vec<Self::Node>` list hooks, outward reparse retention and conditional parse
results without lifetime casts or a second AST copy. Existing-node writes and every outward escape still need
the transitive source audit. Then measure successful/rejected speculation, lazy member capture, retained bytes,
deep nesting and whole compiler instructions/wall/RSS. Storage-only success does not settle those obligations.

## Shared parser scope candidate

`AstScope::borrow_writer` now lends construction access at the existing handle brand with an independent
writer borrow. Its forwarding frame never captures its own checkpoint or rolls back on drop: writes and
retention reach the enclosing scope's journal. The scope that owns the checkpoint still controls reclamation.
This adds no persistent storage/node fields and leaves the schema-generated factories unchanged.

`LexicalParser::Speculative` separates source, parser-state, handle-brand and writer lifetimes. The legacy
implementation uses the same parser type; the owned implementation supplies `OwnedParserScope`. Scanner,
diagnostics, counters and context state are borrowed from the existing owned parser. Node stacks and local
reparse lists contain the current scope's handle type. They are ordinary vectors, without casts, lifetime
fabrication, copying the caller's stack or building a second AST. Shared lexical, token, recognition, context,
list and leaf-type grammar run directly against the actual scoped storage.

Lookahead remains boolean. Rejection/drop restores scanner state, diagnostics, context flags, parse-error state
and the await marker, while allocation/identifier counters survive. Unwinding also restores the entry parsing
contexts and drops the scope's local stacks. A successful conditional parse keeps parser progress and promotes
its result only to the immediately enclosing handle brand; an outer rejection can still reclaim that tree.
Explicit `mark`/`rewind` with ordinary keys remain allocation-retaining operations.

Outward reparse propagation uses the existing stable-key queue. Before entering a nested parser or dropping a
scoped parser, remaining local reparse entries pass through `retain_node`, pinning every enclosing storage
frame before a stable key is stored. List hooks import those retained keys into their current brand. This is
a conservative retention implementation: it also pins when forwarding an already-stable imported entry. It
preserves the constructed order/escape cases but has no accepted memory cost. The actual JSDoc producers,
clone bookkeeping, top-level-await reparsing and full transitive existing-node write audit remain unported.

The candidate passes 105 AST tests, 37 parser tests, a compiling external client plus 66 rejection controls,
four focused parser cases and the borrowed-writer storage case under pinned-nightly Miri, workspace Clippy
and source lint. All 48 regressions and 2,000 differential parser fuzz inputs pass. Generation leaves all
compiled source hashes unchanged. The recursive array case is a list test fixture; it is not production
expression grammar. Evidence and remaining integration runs are recorded in
`target/ast-soa-scanner/parser-scopes-validation.json`.

The root default conformance run passes all 15,448 variants with zero differences/crashes/timeouts and no lost
baseline passes. The canonical run reports zero differences/crashes and one timeout on
`compiler/intersectionConstructorReductionCrash`; its diagnostic/type/symbol/JS entries are missing against
the retained leaf predecessor. This case also timed out in earlier default-history predecessor/candidate
replays. A matched canonical follow-up is recorded in `parser-scopes-timeout-followup.json`; the retained
predecessor and candidate both pass their focused runs (254.8s and 272.0s). One sequential pair does not
establish the cause or repeatability of that difference. The full canonical gate is unpassed. Do not classify
the timeout as introduced or cleared by this slice, or treat these correctness-run times as performance
acceptance.

Token-only lookahead allocates no AST checkpoint or key buffer, but adds transient scope/vector headers and
journal forwarding. Allocating speculation still pays for lazy storage checkpoints and local stack buffers.
Cooked snapshot clones, repeated failures, retained capacity, deep nesting, lazy capture and real compiler
instructions/wall/RSS remain measurement obligations. Neither the production cutover nor M0/M1 is accepted.

### Template tokens and recursive type preparation

Template heads and span literals now run through the same production grammar inside the owned scopes. The
nested-acceptance case rescans an invalid head, constructs an unterminated cooked tail in an inner accepted
parse, then rejects the outer scope. Scanner/errors return to the original head and all AST/text live buffers
are empty. Raw template text uses retained source ranges, and typed raw-text views remain borrowed from their
writer/lease. The full type/expression span loops have not moved to the owned parser.

The 89 extracted type/signature/expression bodies in `type_grammar.rs` still have only a legacy implementation. Their
explicit mark/rewind sites delegate to the legacy snapshots, which rewind an unpinned arena checkpoint.
Ordinary owned snapshots retain AST allocations; the construction-capable rejection paths still need branded
conditional construction. This is grammar preparation, not completion of the scoped owned-type path. The
remaining pattern/initializer/JSDoc/arrow/left-hand-side/JSX/block/lazy-capture dependencies and constructing
function-type lookahead must be supplied by real grammar before exposing an owned `parse_type` entry point.

The next conditional-construction cases are infer constraints, identifier type predicates and keyword types
that fall back to dotted references. Preserve their operation order and monotonic node/identifier counters.
`LexicalParser::Speculative` currently promises only recognition grammar. Once the owned recursive dependencies
are closed, it needs the construction grammar bound and a shared conditional-node hook that promotes only a
successful result to the enclosing brand. Do not replace these cases with token-only recognition that skips
their provisional construction/counters, or expose a scoped key through a stable-key callback result.

The earlier 23-body/template-token candidate passes 105 AST tests, 41 parser tests and the external-client
positive control plus 68 rejection cases. Both four-worker full conformance runs pass all 15,448 variants with the 600-second case
timeout unchanged and all six baseline pass sets preserved. Earlier failed runs remain recorded above; the
new passes establish no performance attribution. All 48 regressions, 2,000 differential fuzz inputs and 221
targeted pinned-Go AST cases pass. Source/binary evidence is `type-grammar-validation.json` under
`target/ast-soa-scanner/`.

The subsequent 41-body and 62-body legacy preparations also pass both full conformance modes, with every
predecessor category pass retained. The 62-body candidate passes the same 105 AST / 41 parser tests and
68 borrowing rejection cases, the real lazy-binding contract, all 48 regressions, 2,500 differential fuzz
inputs and 442 targeted pinned-Go AST inputs. Evidence is `type-signatures-validation.json` under that
directory, with retained exact sources/binaries. These extractions preserve the production checkpoint calls;
they do not implement owned type speculation. Complete owned grammar, actual reparse/lazy producers,
retained-capacity accounting and the no-regression gate remain open.

### Import and expression preparation boundaries

The next 27 shared bodies bring the default-body count to 89. Direct comparison with the pinned Go
`parseImportType`, `parseImportAttributes`, `shouldConsumeBinaryOperator` and `parseUnaryExpressionOrHigher`
confirms these integration constraints:

- Import types set `PossiblyContainsDynamicImport` before parsing their tokens. This flag is absent from
  `ParserState` and survives rejection; an owned implementation must not rewind it with the scanner.
- Missing import braces inspect the last diagnostic with code `X_0_expected`, including a preexisting error
  reused by same-position suppression. Both nested braces can append related information to that error.
  `last_expected_diagnostic` returns a copied handle; the related-information writer borrows diagnostics
  exclusively. Replacing it with the fresh-error-only matching-bracket helper changes recovery.
- `node_is_left_hand_side_expression` follows `PartiallyEmittedExpression` wrappers before classifying a node.
  Its owned implementation must walk copied child keys through the active read context. Testing only the
  outer kind would narrow the existing helper's contract. `node_is_present` also preserves synthetic-range
  and EndOfFile rules, rather than treating every zero-width node as missing.
- `binary_operator_kind` reads the binary payload's operator child and copies its kind before parsing a type
  assertion. Range/kind reads for exponentiation errors likewise finish before the diagnostic write. These
  hooks must resolve actual retained storage, including permitted foreign children, without detached views.
- Assignment parsing still delegates arrow disambiguation, simple-arrow construction and left-hand-side/JSX
  grammar to the real legacy parser. Conditional parsing restores `DisallowInContext` before parsing its
  colon/false branch; expression parsing restores decorator context after the comma loop. Those local flag
  scopes do not replace the parser scope's panic/rejection cleanup.

No new shared body introduces a persistent parser/node/store field. The legacy hooks still read arena data;
copied return types alone do not make that backend safe. The updated lexical dependency inventory is
`target/ast-soa-scanner/type-expressions-dependencies.json`: each type/expression root reaches the same
463 recognized routines, with 254 remaining legacy bodies (6,278 source lines). This excludes field,
free-function and indirect-callback classification and is not a completed ownership audit. Complete owned
grammar and branded conditional construction remain prerequisites for an owned type/expression entry point.

The 89-body preparation passes the same AST/parser/borrowing and real lazy-binding checks, all 48 regressions,
3,000 differential fuzz inputs and 1,824 targeted pinned-Go AST inputs. Both full conformance modes finish
all 15,448 variants without differences, crashes, timeouts or lost category passes. Exact sources, binaries
and final verification are retained under the `type-expressions` stem. No complete owned grammar, new Miri
result or performance/memory acceptance follows from these legacy-backend passes.

### Expression, arrow and JSX construction boundaries

The next extraction moves 61 real routines: 60 expression/JSX/class-expression/method routines to
`TypeParser`, and the token-only async-arrow classifier to `RecognitionParser`. Moving the recursive
arrow-return-type error helper as well brings `TypeParser` to 150 default bodies. Blocks, full classes,
binding patterns and JSDoc remain legacy dependencies; this is still preparation for the complete owned
parser. No restricted owned expression/type entry point was added.

The parent and flag mutations were compared directly with the pinned Go parser's `tryReparseOptionalChain`,
`unparseExpressionWithTypeArguments`, `parseJsxElementOrSelfClosingElementOrFragment` and
`typeHasArrowFunctionBlockingParseError`:

- Optional-chain repair now requires `&mut self`. It copies each non-null expression child before advancing
  and writes flags through the backend writer, preserving the existing order and short-circuit behavior.
- Tagged-template parsing saves a copied `(expression, type_arguments)` pair before recursively parsing
  the template. Call/new parsing likewise retains copied child/list keys before absorbing instantiation
  nodes. Parent repair reads one list element key before each parent write; no borrowed list crosses a write.
- Mismatched-child JSX recovery saves copied opening/children/closing keys before allocating missing nodes.
  It repairs the same parent links and retains the existing rare-path replacement-list allocation. Sibling
  recovery still uses the exact synthetic comma range and suppresses itself in unary contexts. Normal paths
  introduce no list copies.
- JSX scanner wrappers retain the original distinction between rescanning a token and updating the parser's
  current token. Text construction receives `ScannedText` before the next scan, preserving the owned scanner's
  source/cooked-text policy without borrowing AST buffers across construction.
- Arrow rejection still preserves the negative-position cache and uses the legacy mark/rewind calls. Return
  types preserve recursive missing-reference/missing-parameter checks. These construction-capable rejection
  paths and conditional type-argument lists still need branded conditional node/list promotion before owned
  grammar integration. Stable IDs must not be reclaimed by ordinary `ContextParser::rewind`.
- Import source flags still survive rejection. The source-text diagnostic hook preserves the existing
  normalization and transformed-literal behavior; a future owned hook must not assume every diagnostic is a
  raw source substring.

The new owned async-arrow lookahead test covers line breaks, escaped/invalid tokens, await/yield contexts,
repeated scanner/error restoration, and absence of retained nodes/text buffers. It does not prove owned
expression construction. Compiler-reported unused forwarding wrappers were removed only after checking that
each body delegated directly to shared grammar.

The updated lexical inventory is `target/ast-soa-scanner/expression-completion-dependencies.json`: the
type/expression roots reach 458 recognized methods, including 187 remaining legacy routines / 4,373 source
lines. The inventory still excludes field, free-function and indirect-callback ownership classification and
is not an ownership proof. Complete owned grammar, real lazy/reparse producers and final performance/memory
acceptance remain open.

The separate `expression-completion-entry-dependencies.json` adds the source-file, JSON and top-level-await
entry roots to the same limited inventory. The source-file root reaches 194 legacy routines / 4,698 lines;
JSON reaches 194 / 4,671; top-level-await reaches 188 / 4,488. These counts still exclude free entry functions,
field access and unclassified callbacks. The 187-routine type/expression figure is not the remaining size of
the full migration or proof that publishing source files, JSDoc and lazy reparses is connected.

This preparation passes 105 AST / 42 parser tests, the external-client positive control plus 68 borrowing
rejection cases, the real lazy-binding contract, workspace Clippy/source lint and registered regeneration.
All 48 regressions, 4,000 differential fuzz inputs and 2,474 targeted pinned-Go AST inputs pass. Both full
conformance modes complete 15,448 variants with no differences, crashes, timeouts or lost predecessor passes
in any category. Sources and development binaries are retained under the `expression-completion` stem.
The successful compiler checks follow recovery from an incomplete CLI artifact caused by disk exhaustion;
the invalid first regression attempt and recovery evidence are preserved separately in the manifest/logs.
This remains a legacy-backend parity result, without complete owned grammar or performance/memory acceptance.

### Statement, declaration and binding-pattern construction boundaries

The next extraction moves 82 complete algorithms, including blocks/statements, all variable-declaration and
binding-pattern routines, function/full-class grammar, heritage conversion, declarations/namespaces, imports/
exports and decorator expressions. Four additional modifier/heritage helpers bring `TypeParser` to 236 default
bodies, still implemented only by the legacy parser. No owned recursive entry point is introduced.

- Ambient declarations and class members copy one modifier key before writing its flags. Their original
  context-save/restore order remains. Cached modifier flags are read where the original reads them; the
  class ambient check still derives flags from modifier kinds as before.
- Heritage type conversion copies `(expression, type_arguments)` before recursive allocation. Property-access
  conversion copies expression/name keys and preserves explicit range finalization. Its validity predicate
  preserves missing/synthetic/EOF semantics and rejects the same optional property-access chains.
- Class JavaScript heritage checks copy a clause's token/list pair and then one element key before each
  syntax check. No borrowed list or payload is retained across a writer call, and no replacement Vec is added.
- `try_parse_constructor_declaration` marks before testing its token, but reaches rejection without constructing
  nodes: only a token-value comparison and token-only lookahead run on that branch. Success performs the same
  construction and JSDoc/JS-syntax handling. This does not authorize ordinary rollback of allocated stable IDs
  in the other construction-capable paths.
- Statement recovery preserves automatic semicolon insertion, throw's line-break behavior, `for` declaration
  disambiguation, await/using cases, missing blocks/lists and nested context scopes. Import/export declaration
  routines preserve their saved await-identifier and context flags, including malformed specifiers.
- The missing-semicolon suggestion path borrows identifier text only until its spelling/space suggestion has
  been computed; later diagnostic writes retain no node view. Invalid-name diagnostics use a backend operation
  that observes cooked token text without taking it. Taking an owned scanner value here would empty a token
  that recovery may still inspect; future owned implementation must split scanner/diagnostic field borrows.

The current limited lexical inventory is `target/ast-soa-scanner/statements-dependencies.json`: each recursive
grammar root reaches 457 recognized methods, including 100 remaining legacy routines / 2,304 lines. Most
remaining grammar bodies are JSDoc; constructing function-type lookahead, actual lazy capture, source-file/
JSON entry paths, reparsers, diagnostic ownership and backend writers still require integration. The inventory
excludes fields, free functions and indirect callbacks and does not establish ownership or total remaining scope.

The corresponding `statements-entry-dependencies.json` inventory includes source-file, JSON and top-level-await
entry roots. It finds 107 remaining legacy routines / 2,629 lines under the source-file root, 107 / 2,602 under
JSON, and 101 / 2,419 under top-level-await reparse. Those figures have the same exclusions and do not count
free entry functions, the separate reparser module or any binder/checker/consumer migration.

The next JSDoc tranche has a text-lifetime boundary as well as node ownership. `parse_jsdoc_comment_worker`,
`parse_tag_comments` and `parse_jsdoc_link` currently collect `Vec<&'static str>` and retain descriptor arrays
in the AST. Most pieces come from `scanner.token_text`; `JSDocCommentTextToken`'s value is also assigned a direct
source slice by `scan_jsdoc_comment_text_token`. Leading/trailing trimming changes slice bounds rather than
the content. `skip_whitespace_or_asterisk` constructs a String from one contiguous source interval; the initial
margin copied by `parse_trailing_tag_comments` is a substring of that interval. These are promising source-range
recipes for the owned implementation, subject to the full JSDoc/error/reparse corpus. Cooked tag identifiers
must still use scanner text ownership; they are a separate path from raw comment pieces.

Do not solve this by borrowing the owned text buffer across AST construction, joining every comment into a
new String, or collecting a second normal-path descriptor Vec solely to call the existing generated
`new_jsdoc_text(&[TextInput])` API. The actual generated text-list encoder currently needs a slice; connecting
source-range scratch descriptors will need a checked iterable/adaptor or a matching construction API. Preserve
fragment boundaries, trimming, cached scratch capacity, byte-based indentation and fenced/link behavior.
JSDoc comment parsing also temporarily shortens the scanner source and restores scanner/context/diagnostics
afterward; the builder must continue to address the full retained source. None of these paths is connected yet.

The JSDoc rejection cases differ. `parse_jsdoc_link` rejects from a token-only prefix scan before constructing
its link/name. In contrast, `parse_child_parameter_or_property_tag` can parse a complete child tag and then
reject it because its name is not nested under the parent name. Its enclosing nested-type/typedef/signature
mark/rewind loops must conditionally promote the whole child graph on acceptance and reclaim private nodes,
lists/text descriptors on rejection. Replacing that parse with a tag-name scan would lose existing diagnostic
and recovery behavior. `parse_jsdoc_signature` likewise parses a whole tag and rewinds if it is not a return
tag. These rejection paths need private-graph reclamation; the retained-parse paths elsewhere in the grammar
must remain distinct. The separate `reparser.rs` producers are outside the lexical method counts and remain
in scope.

This tranche passes 105 AST / 42 parser tests, the positive external client and 68 borrowing rejection cases,
the real lazy-binding contract, workspace Clippy/source lint and registered regeneration. Its mechanical
check covers 82 moved and 150 unchanged bodies; all prior/moved routine attributes are preserved. All 48
regressions, 4,000 differential fuzz inputs and 3,802 targeted pinned-Go AST inputs pass. Both full conformance
modes complete 15,448 variants without differences, crashes, timeouts or lost category passes, using four
workers and the unchanged 600-second case limit. Evidence is retained under the `statements` stem.

The expanded corpus first exposed an inherited oracle panic on a negative diagnostic length from missing-node
JavaScript recovery. The validated predecessor reproduces it. The serializer now observes signed differences
between the already-correct stored endpoints, and the entire corpus passes without removing the case.
The original finding/snapshot and repaired oracle evidence are preserved. This repairs observation in the
oracle; production diagnostic-domain handling remains a separate obligation described in the SourceFile audit.
Final verification covers current/archive sources, configurations, binaries, retained snapshots, logs, corpus
inputs and both category pass sets. Complete owned integration and release performance/memory acceptance remain
open; these are legacy-backend parity results.
