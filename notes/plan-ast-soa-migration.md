# AST SoA migration plan

Proposed on 2026-10-23. Implements the [selected architecture](ast-soa-architecture.md); the
[constraint study](design-ast-soa.md) and [memory-model history](safe-memory-model.md) are prerequisites.
The [progress log](ast-soa-progress.md) records the separate, incomplete implementation and its validation.

Owner requirement: **safe Rust, no performance regression, no memory regression**. This overrides the general
repository policy that permits measured catalog tradeoffs. It applies to each production migration change as
landed and to the final backend; no temporary production regression is authorized. Experimental branches may
contain slower/larger candidates for comparison, but those candidates cannot land until the regressions are fixed.

## Scope and completion

Replace AST node/list/text allocation, representation and access with owned SoA stores and owner-backed views.
Program retains shared file-version owners; parser/binder mutate exclusive builders. Preserve current semantics,
wire formats and lifetime boundaries. Include the adjacent symbol-data changes required to make deferred AST
binding safe. General type/symbol/flow allocation replacement is a later project.

Completion requires all of the following:

- Every production AST producer uses owned storage: parser/reparser, config/JSON, JSDoc, cached tokens, deferred
  members, loading synthetics, checker factories, emit and LS/edit factories, clone/update and codec entry paths.
- No production node/source-file/list edge uses `P`, a raw pointer, packed pointer bits or a fabricated static
  reference. Runtime AST text and slices borrow retained owners. Existing semantic IDs and API bytes match.
- Every mutation has an exclusive writer or a standard guard protecting the actual data. Deferred symbol writes
  and reads obey the selected protocol; there are no unchecked-cell exceptions for migrated data.
- Reuse, requests surviving snapshot disposal, result retention, early leaf-body release, checker retirement,
  emit disposal, cancellation and failed initialization have verified ownership paths.
- The owned backend passes the applicable full-workspace, reference-parity and lifecycle gates, with no repeatable
  performance or memory regression in the measured catalog/configurations. Improvements on other workloads cannot
  compensate for a regression. Preparatory changes meet the same rule before landing.
- The legacy AST backend and temporary adapters are removed. Remaining non-AST unsafe boundaries are explicitly
  inventoried; the whole compiler is not described as sound merely because AST storage is complete.

## Dependency order and integration strategy

```text
M0 contracts and feasibility
  -> M1 owned storage and mutation proof
  -> M2 generated access facade and consumer preparation
  -> M3 file-owner lifecycle and guarded publication
  -> M4 parser/reparser + initial binder on owned storage
  -> M5 deferred binding + late/synthetic stores
  -> M6 adjacent edges and all consumers; complete owned workspace
  -> M7 full parity, retention and catalog measurements
  -> M8 default cutover and legacy removal
```

M2-M3 can land as small preparatory changes using the legacy representation. M4-M6 are a coordinated integration
effort: do not promise that switching one node kind produces a usable mixed compiler. A source tree, its parents,
lists and every referring cache need one coherent handle model. Keep normal builds passing on the legacy backend
until the owned path compiles across the workspace. Incomplete owned integration stays on its work branch.

Use a temporary build-time backend choice, with no runtime branch at every accessor. If useful during M2, the
facade's crate-internal `NodeKey`/`ListKey` aliases can initially name the legacy handles; that is migration
plumbing, not the final key contract. Owner-backed facade APIs still isolate the old unsafe implementation, whose
known risks remain until cutover. All new algorithm code uses the facade so the backend switch finds direct
dereferences and field access at compile time.

Do not maintain a second pointer tree mirroring the SoA tree, rebuild a full P-to-key map per operation, or give
the new key an implicit legacy `Deref`. Choose one backend per binary. Keep a baseline binary from the same
revision/build settings for measurements. Include every shipping adapter and its overhead in acceptance results;
do not compare only an idealized final store while landing a regressing intermediate representation.

## M0: settle contracts and prove the difficult paths

Deliver a checked inventory, using [gen-ast.ts](../tools/gen-ast/gen-ast.ts)'s schema, `CELL_EXPLICIT`, Go-only
binding fields and rare-field policy. For each field record its domain, write phase, owner, reader API, source
text/list dependency and late-publication behavior. Include handwritten SourceFile, identifier, flow and lazy
records. Record current layouts and allocation/retention boundaries; distinguish lexical searches from call-site
counts. Verify the pinned reference before behavior comparisons or generator work.

Trace these real paths before locking the public API:

| Path | Required proof |
| --- | --- |
| CLI host returns an unbound file, loader inspects it, Program binds later | Guarded parsed entry permits loading without eager binding or detached references. |
| Project cache parses/binds before returning a file; two Programs reuse it | One stable owner/store identity, no assumption that `Arc::get_mut` succeeds. |
| A reused/imported file creates JSDoc/tokens later | Both Programs resolve the new store, including an imported directory. |
| Two threads request different deferred lists in one file, with recursive forcing | Claim-after-gate ordering prevents cross-list deadlock; same-context recursion sees parsed nodes. |
| Lazy declarations merge with existing owner/member symbols | Every mutated field and every reader is protected; stable symbol identities survive. |
| A diagnostic/result retains nodes after checker/job or snapshot disposal | The result retains body and dependency leases, without a strong ownership cycle. |
| A CLI leaf body is retired | Metadata survives, live leases remain valid, new acquisitions fail, later work skips that body as today. |
| Binding/forcing waits with a bounded worker pool | Waiters release compute permits so the winning task can progress. |

Review the deferred binder's actual callees for file locality, callbacks, saved containers, owner-symbol tables
and reads that can force. The selected file gate cannot call another file's gate. Any violation must be resolved
here rather than appearing as an undocumented exception during implementation.

Resolve parser reclamation as a separate feasibility case. The [rollback audit](ast-parser-rollback-audit.md)
distinguishes token recognition, rejected AST construction, retained reparses and lazy-list capture. The current
owned snapshot retains abandoned nodes; the compact keys have no generation, so simply truncating columns would
reuse abandoned identities. Neither behavior passes the final contract without resolving the ownership and
measurement obligations in that audit.
The experimental scoped storage API now reclaims private allocations with generated coordinated truncation.
Shared lexical/list/type-leaf/template-token grammar runs through branded parser scopes, with parser-state
restoration, nested acceptance and explicit reparse retention. Complete grammar, real reparse producers and
lazy-capture paths remain to be ported and proved before counting this feasibility case as complete; isolated
storage tests and these partial parser paths are insufficient.

Bring key width and metadata into the feasibility decision now: the current edges occupy four bytes. The wider
correctness probe does not authorize eight-byte production edges. Show a compact representation or credible,
accounted-for layout savings before broad porting; whole-compiler measurements still decide acceptance later.

Exit: field/owner inventories and runnable feasibility cases cover these paths. Reopen separate binding columns
if a necessary unbound reader cannot obey the parsed-guard transition. Reopen the deferred protocol if file-local
mutation cannot be enforced. Do not start broad mechanical porting with either uncertainty hidden.

## M1: owned storage, views and writer foundation

Add the new implementation under `tsrs_ast`, initially isolated from production exports:

- Checked `AstStoreId`, `NodeIndex`, `NodeKey`, list/payload/text domains and size assertions.
- SoA common columns, sparse present payload tables, and representative payload columns: identifier, binary,
  declaration binding annotations, a payload-less token and both Unknown-kind flow payloads.
- Source text/ranges and owned text modes; lists with spans, empty/nil distinctions and trailing commas.
- Explicit directory registration, strong owners, leases and private writer APIs. No unsafe representation casts.
- Stable retained identities and coordinated column invariants, including allocation failure, abandoned nodes
  and scoped private-slot reclamation.

Exercise actual parser/factory and binder patterns in a narrow probe: nested constructors/hooks, recursive reads
and writes, list traversal, reparser-style replacement, vector growth, cross-store parents, foreign keys and
two-owner retention. A container-only benchmark is insufficient. Keep the probe small enough to run under Miri
without involving the legacy compressed-pointer allocator.

Exit: compile-fail cases reject views escaping owner/guard lifetime and mutation with a live view in use. Runtime
cases reject wrong-owner/index/payload access, cannot reuse abandoned identities, survive growth, and release
owners once the last lease ends. Record actual sizes/capacities and prototype metadata overhead. Test compact
edge resolution, tiny-file tables, growth/rollback and lease acquisition costs now. Obvious uncompensated layout
growth must be fixed before broad porting. These probes do not substitute for the final compiler measurements.

## M2: generate the access facade and prepare consumers

Change generators, not generated output by hand. Generate owner/context-based reads, typed views, writer setters,
construction, child iteration, update/clone and base accessors. Preserve canonical token payload selection and
all handwritten variants. Extend the codec generator separately. Retain schema order, callback evaluation and
update identity rules exactly.

Port consumers to explicit views/contexts while the legacy backend still supplies behavior:

1. AST utilities, scanner text access, visitors and factory hooks.
2. Parser/reparser and binder entry signatures.
3. Checker/link/flow helpers and node-builder factories.
4. Printer, declarations, transforms and emit contexts.
5. LS/LSP, project/config, API/codec/transport, CLI, test/oracle and WASM paths.

Use copied keys/scalars before recursive mutation. Split cloneable factory settings from exclusive writers.
Long-lived state stores keys plus owners at its boundary, rather than borrowed views. Preserve function
decomposition and Go control flow; do not combine this with semantic simplification or global interning.

Exit: normal builds stay green after each preparatory slice. Generated freshness checks pass. A scan lists
remaining direct dereferences, base-field reads, `as_p()` conversions, pointer bits and static text/slice returns;
the owned backend compiler checks serve as the authoritative remaining-access check.

## M3: file ownership and initial publication

Migrate compiler/project integration around
[host.rs](../crates/tsrs_compiler/src/host.rs), [fileloader.rs](../crates/tsrs_compiler/src/fileloader.rs),
[filesparser.rs](../crates/tsrs_compiler/src/filesparser.rs), [program.rs](../crates/tsrs_compiler/src/program.rs),
[parsecache.rs](../crates/tsrs_project/src/parsecache.rs), and
[memregions.rs](../crates/tsrs_project/src/memregions.rs).

Implement the Parsed -> Binding -> Bound/Failed state machine. The binder moves the builder out after parsed
guards end; it does not run while holding the entry mutex. Phase guards are tied to caller-retained owners.
Bound acquisitions return owned leases, so published references never depend on a temporary lock guard.

Adapt host and standalone parser signatures to return owners/phase handles, not bare source-file pointers.
Preserve parse-ahead, content-mapper finalization, cache keying and existing bind timing/order. Reused bound files
keep their store IDs. Import directories through retained owners so future lazy stores remain resolvable.

Split durable file metadata from releasable bodies. Port the leaf-release proof to safe retirement and prevent
long-lived read contexts from retaining every body unnecessarily. Public AST results and codec tables acquire
independent leases; they must not rely on the old Program `'static` view or manual-free convention.

Exit: cache eviction/revival, two Program versions, imported owners, snapshot disposal during requests, initial
bind contention and cancellation all have lifecycle cases. No parsed guard crosses a binding callback.

## M4: parser, reparser and initial binder on owned storage

Use the complete generated column backend. Port
[parser_1.rs](../crates/tsrs_parser/src/parser_1.rs), [reparser.rs](../crates/tsrs_parser/src/reparser.rs),
[identifier.rs](../crates/tsrs_ast/src/identifier.rs), and [binder.rs](../crates/tsrs_binder/src/binder.rs).

Preserve parser/scanner state marks, cached escapes, top-level-await reparsing, clone mappings, comments, cooked
and raw strings, invalid-input recovery, ranges and list metadata. Remove the raw source-text registry. Use
append-only identities initially, then reclaim provably private storage without resetting node identities or
semantic counters. Report speculation retention before calling this phase production-ready.

Binder changes include flags that are cleared as well as set, declaration symbols, locals/container chains,
flow/end/return/fallthrough annotations and diagnostics. Keep semantic-ID timing and reverse file order in the
single-threaded Program path. Binder reads its exclusive context rather than waiting on its own file entry.

Current preparation shares 62 type/signature bodies, including operand dispatch, tuple/mapped/type literals,
type parameters, function/constructor types, template-type spans, members, parameters, modifiers and accessor/
function-body entry. Only the complete legacy parser implements that trait. The owned lexical/list/type-leaf/
template-token slices do not yet supply the real binding-pattern, initializer, decorator/expression, statement
block, JSDoc, import, lazy-capture or constructing-lookahead dependencies. Preserve their recursive
grammar and provisional counters rather than supplying a restricted owned type entry point. The current
mechanical/dependency evidence is under `target/ast-soa-scanner/type-signatures-*`; it does not close this milestone.

Exit: owned parser and binder fixtures match the pinned AST/binder oracles, invalid input and reparser cases
included. Initial bind is exactly once per file version, exposes no partial body, and wakes waiters on failure.

## M5: deferred binding and all late producers

This phase is required before any default-backend proposal:

- Move the shared fields reached by deferred writes into protected owned `DeferredBindingData`; migrate all
  reader access too, including cached-symbol paths. Preserve stable symbol identity and semantic ID allocation.
- Implement file-gate-before-slot-claim ordering, explicit same-file recursive `LazyBindContext`, short public
  read guards, saved binder contexts, Ready publication and terminal failure/poison handling.
- Preserve Pending, Deferred and parse-only/UNBOUND behavior; initial inline forcing uses the current exclusive
  binder instead of acquiring a published-file gate. Blocking gate waits cooperate with run-slot scheduling.
- Implement owned JSDoc fragments and LS token cache publication. Validate same-key identity, source parents,
  repeat reads, imports and visibility across Programs. Measure tiny token-store overhead.
- Port loading imports, checker-generated nodes and emit/edit jobs to explicit builders and owner domains.
  Remove ambient AST allocation-target routing. Handle retained synthetic type payloads without detached `Any`.
- Audit every late setter for a reused source target. Clone or provide a job-local override only when it preserves
  reference behavior. Do not silently mutate immutable source columns.

Exit: recursive and concurrent force cases terminate; readers see complete bindings; canceled/panicked initializers
wake waiters and reject failed versions. Current lazy-enabled, disabled, force and concurrent modes agree with
their existing reference expectations. Record serialization/contention within a file. No eager-force workaround
is accepted as completion.

## M6: switch adjacent edges and complete the owned workspace

Update every stored node/source-file/list edge in symbols, flow nodes, diagnostics, node/symbol/type link stores,
checker caches, printer original/text-source maps, transforms, LS caches, API node tables and public results.
Audit compressed/tagged words, hashes, serialization, optional niches and identity comparisons. Key widening
must not truncate storage bits or introduce pointer-address ordering into observable output.

Remove consumers' direct payload fields, `as_p()` round trips, static AST text/slice returns and assumptions that
`NodeKey` dereferences itself. Wire serialization remains unchanged, including duplicate node occurrences,
list records and lazy semantic-ID assignment during sorting. Preserve standalone config/JSON/transpile paths.

Exit: the entire workspace, CLI, server, oracle and WASM consumers compile on the owned backend. Source scans
show no production `P<Node>`, `P<SourceFile>`, `P<NodeList>`/`P<ModifierList>` edge outside the explicitly temporary
legacy backend. Every result/cache retaining keys has a documented storage owner. The complete owned compiler
can run both checker modes and all baseline categories.

During transition, file-body and result leases also retain any legacy semantic allocation owners they depend on.
Check that retention ends with the last dependent lease and does not cycle through Program/checker pools. A safe
node store must not free its still-referenced binder/flow/diagnostic data when only the old file handle drops.

## M7: parity, lifetime and performance gates

Use [CONTRIBUTING.md](../CONTRIBUTING.md#reference-compiler) and
[docs/RUST.md](../docs/RUST.md). Reverify the exact reference pin and unchanged sources for implementation work.
Build the compiler/test runner with the dev profile:

```sh
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner
TSRS_TEST=target/debug/tsrs-test .github/scripts/conformance-gate.sh
cargo clippy --all-targets --config 'build.warnings="deny"'
tools/lint/source.mjs
tools/gen-check.sh
```

Run the existing conformance gate in both checker-history modes, regressions, determinism gate, workspace tests,
AST/binder oracles and parser fuzzing, then Fourslash, Node API/codec clients, emit/maps, config/content-mapped
files, watch/build and WASM builds. Use the repository's current harness commands, not old profiler switches.
Compare pass sets; no passing case may disappear. Tests check actual ownership and concurrency contracts rather
than duplicating accessors. Keep focused Miri coverage for the pure owned backend and relevant race checks via
[tools/ci/tsan.sh](../tools/ci/tsan.sh); do not claim that passing Miri proves all compiler interfaces sound.

Required lifecycle cases include: drop/retire with active leases, old request during snapshot replacement,
wrong-version keys, imported late stores, failed/canceled binding, nested/concurrent lazy forces, cached-symbol
reads during forcing, codec/result lifetime after producer disposal, checker retirement and emit-job release.

Build matched release binaries with `profile.release.debug=true` and `profile.release.strip=false`. Compare the
same source revision/configuration except for storage, on the same hardware/workloads/checker counts. Use warmups
and repeated interleaved baseline/candidate runs. Across [bench/projects.json](../bench/projects.json), report:

- Single-checker retired instructions, default-checker peak RSS, and wall time for any speed claim.
- Parse/bind/check/emit timing where available, capacities and bytes per node/list/text/payload table.
- Directory/lease overhead, deferred-binding contention, token-store metadata and speculation retention.
- Repeated no-edit/edit/watch/build/snapshot workloads, separating peak RSS from retained storage after disposal.

Report every catalog project, regressions and tradeoffs. Use xctrace/Hyperfine 2 hardware counters on macOS where
available. Consult the current measured-and-rejected list and newest numbered memory round before optimization.
There is no minimum regression percentage to dismiss: any repeatable regression is a failure. Correctness and
lifetime invariants are mandatory, as are no increases in wall time, retired instructions, peak RSS or retained
memory in the relevant measured workloads/configurations. Do not trade memory for speed or average a regression
away with gains elsewhere. Keep the same checker counts and features. Investigate noise with additional
interleaved repetitions before drawing a conclusion. This gate applies before each production landing and before
default cutover; publishing measurements alone does not resolve a regression.

## M8: default cutover and removal

After M7, make owned AST storage the default in one coordinated change. Remove the legacy AST backend, temporary
aliases/adapters, AST-specific arena allocation/routing, header/payload casts and source-text registry. Retain
custom allocators only for remaining non-AST domains; do not remove the core arena wholesale in this project.

Rewrite [docs/AST.md](../docs/AST.md), the AST portions of
[docs/PORTING.md](../docs/PORTING.md#memory-model-the-important-part), Rust practices, debugging ownership checks
and LSP lifetime documentation to describe implemented behavior. Update unsafe inventory entries rather than
merely moving unsafe code into a different file. Update the capability table if supported behavior/evidence
changes. Do not modify README files without an explicit request.

Exit: all completion criteria at the top hold with the legacy backend absent, full checks pass, catalog results
are recorded, and retained-memory behavior is understood. Subsequent compression/layout work is separately
measured against this backend. Roll back a failing cutover as a coherent backend change rather than restoring
individual raw-pointer escape hatches.

## First implementation batch

Start with M0-M1: the field/owner inventory, guarded initial-bind transition, directory/lease/key foundation and
one real parser/binder vertical probe. Include a reentrant two-list case and two Programs sharing a version that
publishes a fragment later. These resolve the highest-risk assumptions before the repository-wide access rewrite.
This planning request does not start the implementation or claim that any milestone has passed.
