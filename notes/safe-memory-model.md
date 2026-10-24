# Safe memory model: recovered history and next experiment

Research snapshot: 2026-10-23, local `main` at `ec0dba38`. This consolidates the repository notes, relevant local
Git history and branches, and GitHub issues and pull requests retrieved on that date. PR states below are a dated
snapshot. Proposed designs are unimplemented and unmeasured.

Follow-up planning inspected `b99d1f42`: #719 is now merged at `60c302f7`, and source lint reports 510 unsafe sites.
The 519-site breakdown below is the earlier `ec0dba38` snapshot, preserved as historical evidence. The AST,
parser, binder and schema examined for the new design are unchanged between these revisions. Use the live
inventory and current contributing workflow when implementing the migration.

The next candidate to investigate is **ordinary owned collections containing graph records, typed IDs for graph
edges, and references borrowed from the owning store**. Share immutable file/program data through `Arc` at ownership
boundaries; keep mutable checker state behind exclusive access. This removes the need for the custom raw-pointer
allocation model. An indexed store is sometimes called an index arena: it still groups object lifetimes, but its
records can be ordinary Rust values without fabricated `'static` references or intrusive free lists.

## What is established on main

The [memory-model rules](../docs/PORTING.md#memory-model-the-important-part) explicitly choose arena allocation,
`Copy` identity handles, no lifetime parameters for arena data, and `P::get() -> &'static T`. They also assign
thread ownership by convention. These are the existing implementation choices to replace in this exploration,
rather than requirements that a replacement must preserve. Go algorithms and observable behavior remain the
constraints.

| Concern | Current implementation | What a safe replacement must enforce |
| --- | --- | --- |
| Object access | [`P<T>`](../crates/tsrs_core/src/ptr.rs) is a compressed offset on Unix, or a raw pointer with `plain-ptrs`; it dereferences without an owner argument | Every reference borrows live storage, or a guard that retains and protects that storage |
| Reclamation | [`Arena` / `Region`](../crates/tsrs_core/src/arena.rs), per-object free lists, parser checkpoints, scratch regions, and region drop lists | Drop, removal and reset cannot invalidate a live reference; stale IDs cannot silently name a replacement object |
| Programs | [`new_program` / `free_program`](../crates/tsrs_compiler/src/program.rs) leak and manually reclaim programs exposed as `'static` | Programs and their shared data have actual owners; borrowers/results retain the necessary owners |
| LSP ownership | [`programOwner`](../crates/tsrs_project/src/memregions.rs) retains program, base-build and file regions; checker leases delay reclamation | Preserve those retention relationships through update, cancellation, disposal and in-flight requests |
| Mutation | [`FrozenCell` / `OwnedCell`](../crates/tsrs_core/src/frozen.rs) rely on owner rules, with additional checks in checked builds | Exclusive borrows, permanent runtime borrow checks, or synchronization protect every mutable access |
| Thread sharing | `P<T>` and related wrappers implement unconditional `Send` / `Sync` | Immutable shared records satisfy ordinary trait bounds; mutable checker data is exclusive or synchronized |
| Representation | AST/type headers, adjacent payloads and packed words use unsafe casts and provenance reconstruction | Safe enum or typed-table access; allocation ownership alone does not remove representation unsafe |

The [current unsafe inventory](../tools/lint/unsafe_sites.tsv), counted directly at this revision, contains **519
sites: 408 `keep`, 84 `safe-api-possible`, 27 `defect-risk`**. By class: ownership 98, logic 153, boundary 99,
core allocator 144, tests 25. These count sites, not distinct bugs. The audit includes leaks and lifetime escapes,
not just `unsafe` blocks.

## Corrections to older memory

The [unsafe audit](unsafe-audit.md) is an earlier, 498-site snapshot. Its claim that all `defect-risk` interfaces
are sound by convention is superseded by the findings in [#646](https://github.com/oxc-project/tsrs/issues/646):

- A completely safe caller can allocate through `P` in a region, drop the region, then dereference the copied
  handle. The tracker reports Miri-confirmed undefined behavior; retaining the region until after the read is
  the passing control.
- A completely safe caller can create two simultaneous mutable guards from unchecked `FrozenCell`. The tracker
  reports Miri-confirmed aliasing UB with debug assertions and `checked-cells` disabled. The audit's statement
  that Miri cannot see this problem is incorrect.
- A `Drop` owner around a leaked program does not close either hole if it continues returning unconstrained
  `'static` references. `Arc` around a region likewise does not protect references detached from that owner,
  nor objects recycled while the region remains alive.

These findings were retrieved from the tracker, not rerun during this research. The underlying interfaces remain
on the inspected main revision. [#379](https://github.com/oxc-project/tsrs/issues/379) tracks Miri investigation.

The [Rust practices](../docs/RUST.md#techniques) list and older round notes also contain dated conclusions:

- “Lifetime-carrying arenas do not transfer” describes the old no-lifetimes policy, not a Rust limitation.
- Early shared-graph notes rejected general sharing because checker history affects results. A later frozen-seed
  experiment exists; that does not establish that arbitrary concurrent sharing preserves Go behavior.
- Historical percentage thresholds are superseded by the current [measurement rules](../AGENTS.md#performance-and-memory-changes).
  Earlier work rejected only for missing a threshold can be reconsidered; correctness and escape-analysis
  objections still apply.
- The allocation profiler, heap sampler and reachability census used by many notes have been removed. Their old
  switches are not current validation tools. See [current debugging](../docs/DEBUGGING.md#arena-recycling-checks).

## Earlier attempts and what to retain

| Record | Result or constraint to carry forward |
| --- | --- |
| [Original reachability census](mem-census.md), [recycling](mem-recycle.md) | The project moved from bumpalo to a custom allocator to reclaim provably dead allocations. Parser rollback and mapper/inference escape tracking already recover some memory. Replacing the allocator must account for those lifetimes. |
| [Scoped arenas](mem-scoped-arenas.md) | Inference and overload scopes mix temporary objects with objects retained in caches. A whole scope cannot usually be freed. Owned local symbol tables were a successful simpler case. |
| [Overload rollback](mem-overload-rollback.md) | Undoing ID counters and cache effects changes subsequent type/symbol IDs and output. Exact reclamation needs escape information; allocation scope alone is insufficient. |
| [Recycling UB](fix-arena-recycle-uaf.md), [#103](https://github.com/oxc-project/tsrs/pull/103) | Freeing a slice received as a reference parameter violated its protected borrow. LLVM removed the free-list write. Passing a raw slice fixed that incident, without making the overall lifetime model sound. |
| [#401](https://github.com/oxc-project/tsrs/pull/401), [#405](https://github.com/oxc-project/tsrs/pull/405) | Header-to-payload traversal needed chunk provenance; `plain-ptrs` changed from references to raw pointers to avoid protected-reference arguments. These fixes remain necessary while the current representation exists. |
| [Pointer compression](mem-pointer-compression.md) | Current handles and optional handles occupy four bytes. A replacement must measure edge width, table metadata and record layout. Compression being automatic on Unix supersedes the old feature-switch instructions. |
| [LSP regions](lsp-mem.md), [LSP lifetime corrections](lsp-memfix.md), [current ownership rules](../docs/LSP.md#memory-management) | File versions, programs, checker results and registry updates have different lifetimes. Scratch allocations escaping into program caches caused real failures. A language service can outlive the snapshot that produced it. |
| [Watch ownership](watch-memory.md), [batch program freeing](mem-build-free-programs.md) | Process-lifetime allocation does not work for repeated builds. File versions, shared loader data, diagnostic storage and worker-thread resources all need owners. |
| [Emit scratch regions](mem-emit-regions.md) | Per-file transformed nodes can die after printing, but checker caches, diagnostics and program caches can outlive that job. Allocation and result ownership must be explicit across those boundaries. |
| [Leaf-file freeing](mem-free-leaf-files.md), [region cost](mem-leaf-regions-cost.md), [32-checker measurements](mem-leaf-regions-32.md) | Early AST release needs a proof that later checking/emit will not read it. Returning pages has synchronization/TLB costs. Logical object removal does not guarantee an RSS reduction. |
| [Checker retirement](mem-recycle-checkers.md) | Dropping a whole checker can bound memory, at a cost of rebuilding caches. A replacement store should preserve this lifetime boundary even if its allocation mechanism changes. |
| [Checker GC](mem-checker-gc.md) | Objects never read again often remain reachable through live caches and link stores. A collector needs weak-cache/key semantics and a layout-aware trace; merely changing allocation does not find them. This remains a separate design. |
| [Shared base](mem-shared-base.md), [shared-checker study](perf-shared-checker.md), [duplication census](mem-per-checker-duplication.md) | Much checker memory is live, duplicated computation. Arbitrary sharing changes ID allocation, circularity and resolution history. Keep each checker's semantic state separate during the ownership migration. |
| [Shared-layer design](design-shared-type-layer.md), [frozen-seed spike](spike-shared-graph.md) | The later opt-in implementation freezes a seed and gives forks overlays. It is more constrained than a shared mutable graph and has its own ownership/publication requirements. |
| [Checker processes](perf-checker-processes.md) | Copy-on-write sharing was investigated and parked without a wall-time win. Process isolation does not itself establish safe ownership within a checker. |
| [Latest numbered memory round](mem-round4.md), [checker scratch](mem-checker-scratch.md), [never-read objects](mem-never-read-apps.md) | Duplication and retained semantic state matter alongside garbage and allocator slack. The round's original machine/default-checker scoreboard is historical. |
| [Persisted front-end measurements](design-persisted-frontend.md), [design constraints](../docs/PERSISTED_FRONTEND.md) | Separate storage identity from observable compiler IDs. Do not preassign or renumber semantic IDs just to simplify persistence or the new store. |

## Lifetime and ownership prototypes outside main

Closed PRs below were not merged. Their descriptions report their own validation; those results have not been
reproduced here. Closure alone does not establish a technical rejection reason.

| PR and state on retrieval | What it explored | Remaining limitation / useful lesson |
| --- | --- | --- |
| [#527, closed](https://github.com/oxc-project/tsrs/pull/527) | Checker handles tied to pool lifetimes; API results retain program/snapshot owners | The internal `P<T>` graph was explicitly unchanged. Useful retention work, not an arena replacement. |
| [#596, closed](https://github.com/oxc-project/tsrs/pull/596) | Parser/scanner state and AST views borrow an allocator; factories capture the allocation target | Raw compiler parsing remained behind unsafe contracts. Full parser Miri remained unverified in the PR description. |
| [#600, closed](https://github.com/oxc-project/tsrs/pull/600) | On top of #596, AST text/list storage and factories borrow the allocator | Lifetime erasure remained at the raw compiler boundary. Useful API shape; still the arena-backed compiler graph. |
| [#668, closed](https://github.com/oxc-project/tsrs/pull/668) | `ProgramBox` with `Drop`; shared program data in `Arc` | `get`/`Deref` still exposed `'static`, and CLI paths still called `.leak()`. Review explicitly called out the leak. The proposal itself acknowledged the remaining lifetime hole. |
| [#674, closed](https://github.com/oxc-project/tsrs/pull/674) | Expanded core/region Miri cases and a use-after-region-drop negative control | It added coverage, not a corrected access model. Review said the tests were not meaningful; future tests should exercise the replacement's actual ownership guarantees. |
| [#638, open](https://github.com/oxc-project/tsrs/pull/638) | Own protocol strings, formatting rules, pools, cached locations and frozen-seed state; borrow scoped command-line hosts | Runtime-allocation cleanup; program/checker and API/WASM ownership remain. |
| [#645, open](https://github.com/oxc-project/tsrs/pull/645) | Require reasons for `Box::leak` | Documentation/lint discipline does not prevent a leak or lifetime defect. |
| [#719, merged at local follow-up](https://github.com/oxc-project/tsrs/pull/719) | Resolver/config hosts use real borrows; remove project `assume_static` and several host leaks | Main contains this work at `60c302f7`. Auto-import lifetime extension remains blocked on checker/program ownership and escaping host closures. Do not duplicate this scoped-host migration. |
| [#669, merged](https://github.com/oxc-project/tsrs/pull/669) | Classified unsafe/lifetime-escape inventory | The ratchet is on main; use its actual table rather than the older audit totals. |
| [#703, merged](https://github.com/oxc-project/tsrs/pull/703) | Reclaim build package-cache entries and bound pending frees | A later lifecycle fix; earlier build-memory notes do not describe every current retention path. |

Recoverable local artifacts inspected:

- `origin/codex/ast-storage-lifetimes` at `bbae7cfb`, stacked on parser work at `7214ce6c`. Its
  `crates/tsrs_parser/src/scoped.rs` implements `ParsedFile<'alloc>`, `NodeRef<'alloc>` and list/text views while
  keeping raw `P` handles inside the wrapper. This demonstrates borrowed views, not removal of the allocator.
- `d3e274cd:notes/mem-program-owner.md` records the `ProgramBox` ownership/destruction order and the still-open
  choice between borrowed program views and owning handles. The note is absent from main but readable with
  `git show d3e274cd:notes/mem-program-owner.md`.
- `origin/max/miri-region-coverage` retains the expanded region/Miri experiment; `origin/max/lifetimes-config-hosts`
  retains the scoped host changes. Inspect historical code without checking it out over current work.

## Candidate models

| Model | Safety and ownership | Fit for this project |
| --- | --- | --- |
| Owned typed stores with ID edges | `Vec`, `Box` and maps own records; lookup returns a borrow of the store. ID cycles do not create ownership cycles | Best next candidate. Preserves cheap identity edges and coarse lifetime boundaries, but requires explicit store access and safe payload representations. Still resembles an index arena conceptually. |
| `Rc` / `Arc` per graph object, with `Weak` back edges | Ordinary smart-pointer ownership; mutation needs `RefCell`, a lock, or exclusive access | Appropriate for selected owners. A full graph conversion needs a correct strong/weak edge classification; strong cycles leak and weak edges must not drop live semantic dependencies. Per-edge cloning and per-object allocation costs are unmeasured. |
| Lifetimed reference arenas | References borrow an arena; reset/free requires an exclusive owner borrow | Can make arena APIs sound, and the closed prototypes provide starting points. Retains arena allocation and requires lifetime plumbing through mutually referring objects and escaping results. |
| Branded mutation (`GhostCell` / `QCell` style) | A token controls mutation and aliasing | Possible component for a cyclic graph. Does not by itself own records, reclaim unreachable objects, or remove unsafe representation casts. No project experiment found in the searched records. |
| Tracing collector | A sound collector can handle graph cycles and reachability | Requires a new tracing/root/cache design, plus measurement. Historical checker-GC work explains why collection is not a drop-in allocator swap. No replacement collector was found on main. |

No measured migration to `slotmap`, a generational arena, `index_vec`, or branded cells was found in the searched
repository notes/history. These are fresh candidates, not previously established project wins. A library providing
safe storage may use reviewed unsafe internally; the target is sound safe compiler interfaces and removal of
avoidable project-owned unsafe, with OS/FFI boundaries assessed separately as in #646.

## Proposed ownership shape

1. **File versions own front-end records and text.** Parse/bind builds mutable storage, then publishes immutable
   file data. Nodes, binder symbols and flow edges use typed storage IDs. Parent cycles are ID cycles. Lazy data
   must be held in safe synchronized caches or separately owned fragments; `Arc<ParsedFile>` is sound to share
   only after its contents actually satisfy `Send + Sync`.
2. **Program versions own shared inputs.** Snapshots, language services and active requests retain a program/file
   owner. Separate the shared program data from its checker pool so making checkers retain program data does not
   introduce an `Arc` cycle between program and pool.
3. **A checker owns its mutable graph and caches.** Types, transient symbols, signatures, mappers, inference data
   and link records live in owned stores. Acquire a checker exclusively once per operation; pass copied IDs through
   recursive calls and borrow records briefly. Copy the few fields needed before a call that mutates the store;
   do not clone whole graphs to appease the borrow checker. Use ordinary `RefCell` guards where required by the
   access pattern, without removing their checks in release builds.
4. **An emit job owns its synthetic graph.** Persistent diagnostics/results are owned values, or retain the
   storage they reference. Updating a checker/program cache allocates through that cache's explicit owner,
   replacing ambient allocation-target routing.
5. **External handles retain or validate context.** A public result either owns its protocol data or retains the
   relevant snapshot/result-storage owner. Later operations on mutable checker state still require an exclusive
   lease. An `Arc` to a mutable, non-`Sync` checker is not sufficient.

Access should have the shape `store.get(id) -> Option<&Record>` or `view.record() -> &Record` where the view borrows
the store. An ID alone has no `Deref`. Strings and lists follow the same rule: owned buffers, source offsets or
list IDs resolve to borrows of their owner, not `'static` slices. Runtime-built strings do not become static merely
because node handles changed.

Storage IDs must be distinct from the existing semantic node/symbol/type IDs: the latter's creation order and
observable uses must remain faithful to Go. The project already defines a semantic `TypeId`; a new storage handle
needs a distinct name/domain.

There are three independent handle problems:

- **Bounds/type:** a `NodeStorageId` cannot be mistaken for a mapper ID, and every lookup checks its slot.
- **Owner identity:** an ID from an old file/checker must not resolve in another store with the same slot number.
  Use a brand or validate a store/version identity; private ID fields alone do not prove this.
- **Reuse:** append-only storage needs no per-slot generation while its owner remains unchanged. If slots are
  removed/reused, check generations and handle generation exhaustion without accepting an old handle. Resetting
  an owner also needs a new identity/epoch. Generations detect stale access; they do not prove reclamation is
  semantically correct.

Do not assume generations and owner IDs fit into the current four-byte edge. Compare compact internal IDs with
context-qualified external handles against wider per-edge handles, including the metadata needed to validate them.

Safe allocation is only part of this migration. Replacing [`NodeAlloc` traversal](../crates/tsrs_ast/src/ast.rs)
and [`Type` payload casts](../crates/tsrs_checker/src/types.rs) needs safe enums or separate typed payload tables.
A single enum sized for the largest payload may erase layout wins; compare that layout with a small header plus
typed payload IDs. Likewise, link stores must stop returning owner-detached `'static` values.

## Next bounded experiment

The owner selected **AST nodes in Program-owned SoA, mutable during parsing and binding**, as the first migration
on 2026-10-23. [The AST SoA exploration](design-ast-soa.md) records its constraints and alternatives, including
file reuse, phase publication, lazy parsing and synthetic-node storage. The subsequent
[architecture](ast-soa-architecture.md) and [migration plan](plan-ast-soa-migration.md) specify the chosen target and
completion gates. This AST-first work supersedes this note's initial
mapper/inference-context-first suggestion. Keep legacy dependencies explicit and do not describe the whole
compiler as safe while other records still use `P`.

The owner subsequently required safe Rust with no performance or memory regression. For this migration that is
stricter than the general catalog acceptance policy: repeatable regressions cannot be offset by gains elsewhere,
and production preparatory changes must meet the same requirement. The architecture and plan reflect this gate.

Acceptance questions for the spike:

- Can actual parser, binder and AST-consumer paths use short owner borrows and copied IDs without pervasive
  record cloning?
- Does dropping/removing storage become impossible while a reference is live, and do stale/wrong-owner IDs fail
  predictably? Cover cached escape, reuse, cancellation and destruction, not just allocation and reading.
- Do semantic IDs, cache effects, diagnostics and all baselines remain unchanged? Storage generations must not
  reset compiler counters or undo speculative semantic work that Go retains.
- What are the changes to record/edge sizes, allocation counts, single-checker instructions, default-checker peak
  RSS, and wall time? Measure actual compiler workloads, not just a standalone ID-store microbenchmark.

Keep the independent unchecked-cell fix in scope for #646: safe standard borrow/synchronization APIs must protect
release builds. A store migration that retains unchecked `FrozenCell` or unconditional sharing of its mutable
records does not meet the goal.

For implementation validation, first verify `ts-ref` against the exact pin in `Cargo.toml` and keep reference sources
unchanged. Follow [contributing](../CONTRIBUTING.md#reference-compiler): dev-profile CLI/runner builds, the affected
regressions, all conformance baselines and both checker modes, then the relevant LSP/API/watch/build/emit lifecycle
checks. Run Clippy and source lint; measure release binaries with full symbols and stripping disabled using the
current catalog and interleaved repetitions. Add focused compile-fail and Miri cases for the new ownership contract,
including unchecked-cell configurations where applicable. Complement Miri's `plain-ptrs` coverage with the shipped
configuration and the [race-check harness](../tools/ci/tsan.sh).

## Additional retrieval paths

For layout and heap tradeoffs, also consult [layout round 3](mem-layout3.md), [memory round 2](mem-round2.md),
[memory round 3](mem-round3.md), [small records](mem-small.md), [checker heap](mem-checker-heap.md),
[dense links](mem-dense-link-tables.md), [flat symbol tables](mem-flat-symbol-tables.md),
[flow compaction](mem-flow-compaction.md), [compact AST sizing](mem-compact-ast-sizing.md),
[frontend](mem-frontend.md), [use census](mem-use-census.md), [Linux residency](mem-linux-residency-32.md),
[huge-page tradeoffs](mem-no-thp.md), and [the Bun comparison](bun-check-memory.md).

For lazy or temporary allocations, see [lazy members](lazy-members.md), [lazy memory](mem-lazy.md),
[lazy declaration members](mem-lazy-dts-members.md), [lazy declarations in builds](mem-lazy-dts-build.md),
[export scratch](mem-export-star-scratch.md), [scanner string allocation](perf-scanner-arena-builder.md), and
[emit/checker threading](emit-checker-thread-fix.md). These are supporting evidence, not safe-store migration results.

Search all notes/docs again before a new experiment, including the evolving
[measured-and-rejected list](perf-round2-followups.md#measured-and-rejected-do-not-redo). No cross-session chat-memory
service was available in this research; this document indexes the retrievable project records rather than claiming
to recover unavailable conversations.
