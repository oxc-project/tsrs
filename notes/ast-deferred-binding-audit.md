# Deferred binding: owner and access audit

2026-10-23, inspected Rust HEAD `19ba886428c0b346d33e8f1cc0fe8811ed68a3f1`. The reference still matches
the Cargo pin `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`. This is an incomplete M0 audit, not proof that the
proposed file gate covers every production path. The [architecture](ast-soa-architecture.md) remains the target.

## Replay and calls that can force

Late binding enters `bind_lazy_list`, creates a binder with the saved unreachable flow, and calls
`bind_lazy_members(..., splice=false)`. Initial merging uses `force_pending_lazy_list` and `splice=true` on the
already exclusive file binder. The latter restores the published container-chain insertion point; the former
chains new containers only among themselves. Preserve both, dropped late diagnostics/symbol counts, Pending
reads and ParseOnly lists.

Confirmed edges requiring explicit same-file reentry in the new writer:

| Call path | Existing behavior / constraint |
| --- | --- |
| `bind_each` → `bind` → `bind_children`/`bind_container` → child/list access | Child visitation can force another lazy list. A plain file lock acquired inside every list access would reenter itself. Pass the active binding context. |
| `declare_symbol_and_add_to_symbol_table` → class/interface/type-literal members or module/enum exports | `ast::get_members`/`get_exports` call `Symbol::members`/`exports`, which can force the owner's deferred list. Even an apparently simple table lookup can recurse. |
| `add_declaration_to_symbol` → `force_pending_lazy_list` | Merges can force an earlier list before updating its symbol, preserving declaration/table insertion order. Initial inline forcing cannot reacquire the published-file gate. |
| `lookup_entity`/`lookup_name` → initializer/export symbols → `Symbol::exports` | Lookup is syntactic/container-local here, but exports access can force. Do not invoke it with a public binding read guard alive. |
| `bind_parameter` → parameter-property branch → `get_members(class.symbol)` | `constructor(public x: number)` is eligible for lazy parsing. Binding its parameter accesses the already-published class's member table and forces the very list being bound. The active writer must return its partial table/list without reacquiring the gate. A contract case now exercises this direct same-list recursion. |

The `Binder` implementation has no checker/resolver host field; the generic `NameResolver`/`ReferenceResolver`
hooks in the same crate belong to separate types. This supports a local binding model but is not a transitive
callee proof. Remaining AST helpers, parser callbacks, malformed syntax and incremental/synthetic owner paths
must be checked. The owned writer must validate that saved containers, symbol-data owners and forced lists belong
to its file version, including extensions. A checked key alone does not prove this ownership relation.

## Actual shared data that cannot escape protection

The inspected member path distinguishes syntax from binding writes. `bind` writes identifier/property-access
flow annotations; `bind_container` writes flags/end-flow annotations on newly parsed signatures and constructors;
`bind_parameter` can update the published class's symbol members through a parameter property. The latter is not
a reason to make the class's immutable syntax mutable: it is a binding-table mutation under the file writer.
Non-optional property accesses recurse without creating flow labels; optional chains are excluded by the parser.
`check_private_identifier` reports a diagnostic but does not mutate a containing class. These specific callees
were read directly; the transitive audit remains incomplete.

`declare_symbol_ex` and `add_declaration_to_symbol` read and write existing table entries, symbol flags,
declarations, value declarations, parents and export/local symbol associations. Tail allocation and the
value-first-declaration representation also mutate symbol name tags and table pointers. Protecting only
`members`/`exports` or only the newly parsed fragment leaves these operations unprotected.

`Symbol::data_owner` adds another reader boundary: instantiated checker symbols may share a binder symbol's
declarations, tables and value declaration while retaining independent identity/flags. New access must follow
and retain the data owner. Recovering ownership by reading the first declaration before acquiring its guard
would itself access potentially changing data. Store the owner relationship explicitly with the key/shell.

Table access has two separate mutation boundaries. `Symbol::members`/`exports` can force a list;
`utilities_1::get_members`/`get_exports` also allocate and install a missing table, and `get_locals` calls
`get_symbol_table`, which allocates and installs missing locals. These helpers are not read-only merely because
their names start with `get`. The migrated helpers need mutable binding authority for forcing or installation;
an ordinary guarded lookup can return an existing table key or `None`. Keep forcing and table creation in the
same active session, including parameter-property recursion. Every existing caller must be classified before
replacing the legacy helpers.

Checker ownership cannot be inferred from `SymbolFlags::Transient`. `new_instantiated_symbol` calls
`new_shared_data_symbol`, giving a transient checker identity the original symbol's data owner; nested instances
are flattened to that owner. Its flags/links are private, but names, declarations, value declarations, parent,
members/exports and pending-list access still follow the shared owner. In contrast, `clone_symbol` creates a
new ordinary data owner, shares the existing declaration slice until append, and clones member/export tables.
`merge_symbol` tests Transient before deciding whether to clone; that test is not sufficient to choose an owned
storage writer. Classify each actual target's data owner, and prove which shared-data instances can reach these
mutating helpers before migration. This inspection does not establish that a current cross-file write occurs.

`clone_symbol` is also a guard-order constraint: it reads declarations/value/parent before asking for
`members`/`exports`, which can force. The owned version must finish using those borrowed binding views before
forcing, while preserving the old declaration snapshot and copy-on-append behavior. Copying every declaration
list or retaining every file body is not an accepted workaround; account for snapshot ownership and its measured
cost when integrating shared checker-symbol data.

The isolated binding store now exposes `DeclarationSnapshot`: an opaque binding-owner identity and immutable
declaration span, with no reference or strong owner. A caller saves it under the old read guard, releases that
guard before forcing, then calls `BindingData::read_declarations` under a fresh guard from the same owner.
The resulting iterator borrows that guard. Declaration replacement never rewrites old elements. Appending to
the buffer's final span extends it while old snapshots retain their saved lengths; an interleaved append copies
the old compact elements directly within the buffer. No temporary Vec of qualified keys or per-snapshot allocation
is needed. The caller must independently retain the correct binding/body owner.

This resolves the declaration-sequence borrowing mechanism in the prototype, not `clone_symbol` integration.
Cross-owner checker clones still need explicit source-owner retention, private append ownership and measured
layout/cost. A snapshot cannot be read through a different binding namespace, nor can a borrowed iterator cross
another write. Runtime and Miri checks cover order preservation across initial publication, released guards,
published append/replacement, local/foreign buffer growth, interleaved spans, wrong owners and final owner release.
External-client compilation accepts saving a snapshot before forcing, and rejects both an escaping iterator and
mutation with an iterator still in use. These are representation/protocol checks; the real checker clone path
remains unimplemented.

Table iteration has a different existing contract. Legacy `SymbolTable::entries()` returns an owned
`Vec<(&'static str, P<Symbol>)>`; `for_each` explicitly permits its callback to mutate the table. A saved table
length plus indexed reads cannot preserve that snapshot if a callback replaces or deletes entries. Nor can a
declaration-style immutable span name mutable table elements. Preserve the existing snapshot allocation for
these callers; the guard-borrowing `SymbolTableView::entries()` iterator is for reads without mutating callbacks.

The prototype's `snapshot_entries()` copies opaque name descriptors and symbol indexes into a Vec. Each
`SymbolTableEntrySnapshot` is 16 bytes on this host and retains neither a guard nor an owner. Its name descriptor
is independent of the table's later replacement, deletion, shifts and cloning. After a callback, a fresh guard
from the saved binding owner supplies `read_symbol_table_entry`; the returned name borrows that guard. The legacy
snapshot element is 24 bytes on this host, but these sizes establish no compiler performance or memory result.
Runtime and focused Miri checks cover source/owned Unicode names, aliases, cloning, replacement, deletion,
symbol/text/table growth, saved order, wrong owners and final owner release. External clients can save entries
before forcing and cannot escape a saved-name borrow or mutate while using it. Cross-owner checker tables still
require the foreign symbol/name ownership work below; this protocol currently covers file-local entries only.

There is also a same-owner borrowing boundary after reacquisition: a name borrowed from `BindingData` cannot
be passed to a setter on that same mutable data. `set_symbol_table_entry_from_snapshot` and
`delete_symbol_table_entry_from_snapshot` consume the saved descriptor instead. They check its owner, locate
the current entry by the saved name, finish the name read and then mutate with exclusive authority. The setter
can reuse the immutable saved descriptor for an alias rather than copy the string again. The runtime case
replaces/reinserts names in a hashed table, deletes through shifting linear/hashed tables, restores saved order
in a new table, checks wrong-owner rejection and verifies unchanged text-buffer accounting. Miri passes; the
external positive control uses these setters while the live-name mutation control remains rejected.

The current prototype's symbol parents, export-symbol links and table entries are file-local indexes.
`set_symbol_parent`, `set_symbol_export_symbol` and `set_symbol_table_entry` reject another binding namespace.
This is sufficient for the isolated file binder; it cannot yet represent a checker clone retaining its original
parent or a checker-owned global/augmentation table containing symbols from several files. Production
`merge_symbol_table` can install the un-cloned source symbol on a missing target entry and can assign a merged
parent after a merge. Preserve those identities and their data owners during integration. A compact foreign
symbol reference and its retained name/data dependencies still need a real feasibility path and accounting;
widening every entry or copying whole foreign symbols is not an accepted shortcut. The snapshot API alone
does not solve this boundary.

### Foreign names need a real borrow owner too

A four-byte local/foreign symbol word would preserve the entry width but is not a complete extension. The
ordinary entry name currently comes from `data.symbols[entry.symbol].name`, and the returned `&str` borrows the
same `BindingData` guard. A foreign symbol's name is in another guarded namespace; acquiring its guard inside
`SymbolTableView::entry` cannot return a name after that temporary guard drops. Copying just its `StoredText`
descriptor into the target namespace is invalid because its source and cooked-text indexes name another owner.
This applies to lookup/rehashing and table snapshots as well as iteration. The original stored key must also
survive replacement by a differently named symbol and deletion of its source entry.

The real checker feasibility path must choose and account for either explicitly retained foreign read contexts
or separately retained immutable name storage. The former must release guards before forcing and cannot nest
one file gate under another; the latter must preserve late-name publication and original key identity without
per-symbol Arc allocation or an unmeasured copy of every name. Retaining a `BindingLease` merely for a name also
retains its syntax body in the current prototype, potentially defeating leaf retirement. Name-only dependencies
and symbol-data/body dependencies therefore need separate accounting at the checker/result boundary. File
binding remains file-local; a checker store cannot be introduced by allowing arbitrary file-to-file strong
binding references, which could create retention cycles. No foreign-name representation has been selected or
accepted by whole-compiler measurements yet.

### Checker late binding continues after initialization

The merge audit cannot stop at checker initialization/augmentation. The sole caller of `combine_symbol_tables`
is `get_resolved_members_or_exports_of_symbol`, reached by ordinary `get_members_of_symbol` /
`get_exports_of_symbol` and by augmentation handling. It installs early members/exports in checker links before
resolving computed members, then combines early and late tables. Preserve that early recursive fallback and
nil-table behavior. The Rust paths below were compared directly with the pinned Go `checker.go` functions
`getResolvedMembersOrExportsOfSymbol`, `lateBindMember`, `lateBindIndexSignature` and `getLateBoundSymbol`.

| Path inspected | Actual forcing / mutation | Required ownership boundary |
| --- | --- | --- |
| `get_resolved_members_or_exports_of_symbol` | Reads early exports even on the member path, then members or module exports; iterates symbol declarations, declaration members and static assignment declarations. `utilities::get_members_of_declaration` calls `node.members()` and currently copies into a Vec. | Release the early table/declaration views before any helper that forces or checks types. Save declaration order after the existing initial forcing, reacquire saved declaration elements under their owner, copy one node key and release the guard before following the declaration. Preserve existing member-copy and table-snapshot contracts where used; do not introduce extra whole-table copies merely to release a guard. |
| `has_late_bindable_name` / `has_late_bindable_index_signature` | `is_late_bindable_name` / `is_late_bindable_index_signature` call `check_computed_property_name` or `check_expression_cached`. These predicates can enter ordinary type checking before `late_bind_member` itself is called. | Treat the predicates as forcing callbacks too. A name-based read/write classification or a guard scoped around the outer declaration loop is insufficient. |
| `late_bind_member` | Sets a checker node-link fallback before checking the name expression. Creates ordinary checker data owners with `new_symbol_ex(..., CheckFlags::Late)` in a new checker table. Updates their flags, declarations, value declaration and parent; errors read early declarations. | The newly allocated late symbols have private data. Their parent and declaration keys can still name file-owned objects. Read early declarations under their actual owner; keep checker links private and retain named file owners. The fallback cannot be delayed until after expression checking. |
| `late_bind_index_signature` | Uses a fresh late symbol when absent, otherwise `clone_symbol(early)` before appending a declaration and setting Late. | Preserve clone-before-append and the saved pre-forcing declaration sequence. An index symbol cloned from a file retains foreign declarations/parents while owning its private append data. |
| `add_declaration_to_late_bound_symbol` | Records a checker link for the member's binder symbol. Appends or filters/replaces the private late symbol's declaration sequence, recomputes flags and applies binder value-declaration precedence. | A checker link pointing at a binder symbol is not a mutation of its binder declarations. Protect actual data owners rather than treating every mentioned symbol as private or shared. Declaration replacement must preserve old saved sequences. |
| `get_late_bound_symbol_worker` | Iterates declarations with the forcing `has_late_bindable_name` predicate, then reads parent/static declarations and resolves members/exports. | End each declaration read before the predicate, and reacquire later reads in their original order. One read guard spanning `.any(...)` would reenter the file gate. |

For a missing entry, `merge_symbol_table` installs `get_merged_symbol(source_symbol)` directly. The combined
table therefore mixes file symbols with private late/merged symbols; cloning the table does not clone its
symbols or make their data private. A conflicting entry can reach `merge_symbol` and recursive member/export
merging during checking. The Transient test still does not prove private ownership because instantiated
symbols share a data owner. This inspection does not prove that an instantiated shared-data target reaches
that mutating branch; the target provenance proof remains open.

Direct checker `Symbol::set_members`/`set_exports` installations were also inspected. `clone_symbol`,
`clone_type_as_module_type` and `combine_value_and_type_symbols` install table clones on newly allocated
ordinary checker symbols; the ImportMeta synthetic installs a newly built table; checker initialization
installs globals on its own globalThis symbol. Type `set_members` calls modify structured type data, a distinct
domain. This narrows table provenance, but does not replace the recursive merge/callback proof or classify
tables installed by the binder. The ordered prototype table API still forbids foreign symbols and must be
extended before these real checker operations can use it.

`report_duplicate_declaration` is another reachable write to an existing symbol: conflicting accessor kinds
add `SymbolFlags::Accessor` before allocating the new conflict symbol. Its diagnostics inspect old declaration
nodes but do not rewrite them. Protect the flag update as well as the declaration/table mutations already listed.

Flow reachability must include the parser's existing exclusions, not just binder call edges:

- `LazyBindContext` retains `current_flow` and `unreachable_flow` from initial binding.
- `create_flow_condition`, `create_flow_mutation`, `create_flow_switch_clause`, `create_flow_call` and
  `add_antecedent` call `set_flow_node_referenced` on an existing antecedent. A saved current flow may be a
  published base-file node rather than a fresh fragment node.
- That helper changes `Referenced`, then `Shared`, but `parser_1::lazy_list_blocked` explicitly rejects bodies,
  initializers, decorators, `this`/`infer`, and expressions that make flow effects. Parse-error/watch checks
  reject recovery and other side effects before a list is made lazy. These are semantic preconditions for the
  deferred protocol, not optional optimizations to discard during the owned parser migration.
- The [real parser/binder contract test](../crates/tsrs_binder/tests/lazy_binding_contract.rs) covers eligible
  interface/class/computed-member/merge cases and excluded initializer/body/this/infer/import cases. After
  forcing via shared-data checker symbols, their member/export flags and insertion order match eager controls;
  saved flow flags do not change in these cases. This narrows the suspected shared-flow path; it is not evidence
  of an existing flow race or a proof over every eligible syntax shape.
- Other flow writes include new flow-start associated nodes, label antecedents, and linked-list tails. Prove
  fragment ownership of these writes. Only newly private labels/list cells may be recycled/abandoned.

First complete the exclusion/callee proof. If an eligible late path can mutate a saved flow, protect its actual
data and all readers. A possible representation is an immutable classification plus monotone atomic reference
flags, or one atomic flag word. Exclusive builders can use mutable access to the contained word; published
updates would need atomic operations. This has not been implemented or benchmarked. A broader semantic gate is
another option. Do not add either cost to production merely because a lexical call edge suggests it is needed.
No design authorizes holding a borrowed binding view across forcing.

### Direct flow paths inspected

The following narrows the flow audit by reading the binder branches, rather than treating all calls to a flow
helper as reachable from an eligible deferred list. It does not complete the transitive helper/callback audit.

| Path | Precondition in the current parser/binder | Consequence for the owned implementation |
| --- | --- | --- |
| Method/signature/function-type/constructor control-flow container | `bind_container` creates a fresh Start flow and restores the saved enclosing flow afterward. Function expressions and static blocks cannot appear in an eligible list: expressions and bodies are blocked. | Start flows and constructor return labels belong to the active fragment's binding session. A constructor can call `add_antecedent`, but its current flow and return label are newly allocated, rather than the saved enclosing flow. |
| Plain property access and qualified type-query names | `lazy_list_blocked` permits a non-optional property access; `bind_access_expression_flow` only visits its children. `bind` may attach the current flow to identifiers/accesses/qualified names. | Assigning an annotation to a new fragment node does not mark its saved antecedent Referenced/Shared. The annotation still requires the correct binding-owner identity and retained flow storage. |
| Unary numeric/bigint literals | The parser permits only unary `+`/`-` directly on a numeric/bigint literal. `bind_prefix_unary_expression_flow` changes flow for increment/decrement, which this rule excludes. | These permitted computed-name/literal-type expressions do not require a shared-flow atomic update. Do not widen the parser eligibility rule during migration. |
| Parameter/binding-element initializers | `lazy_list_blocked` rejects initializers; `bind_initializer(None)` returns immediately. | The branch that joins entry/exit flows is unreachable for an eligible initializer-free parameter. Constructor parameter properties still mutate existing class member tables. |
| Conditional/logical/optional-chain/call/switch/loop flow | The expression kinds are blocked explicitly. Statement bodies are blocked, and parse recovery/side effects reject the lazy candidate before publication. | Preserve both the syntax exclusions and the parse-watch checks. A new parse path or eligibility change must reopen this proof. |

The parser-side gate was checked directly in `parse_member_list_lazily`: the list must be nonempty, at most
64 KiB, start without an outstanding parse error and finish without one. Parsing cannot change diagnostic
counts, eager JSDoc records, reparse-clone count, comment-directive count or source flags. It must also satisfy
the existing arena rewindability check; nested lazy records are truncated before rewind. The owned path must
preserve these observable eligibility rules and prove equivalent private storage discard. Append-only identities
alone do not replace the reclamation or side-effect checks.

`bind_lazy_member_list` adds a separate binder gate: the current flow must be reachable, the owner must have a
symbol, all break/continue/return/true/false/exception/pre-switch targets must be absent, the active-label list
empty and assignment-pattern mode false. Otherwise it parses and binds inline. `bind_lazy_members` restores
those targets to empty again during replay and restores the caller afterward. Saved flows/containers therefore
need checked identities, but replay must not carry an unrecorded surrounding label or target into the fragment.

Additional directly inspected helpers do not introduce a hidden syntax cache write here. `Node::modifier_flags`
reads the existing modifier-list flags; `has_syntactic_modifier`, `get_combined_modifier_flags` and
`is_implicitly_exported_jsdoc_declaration` inspect flags/parents without populating a cache. Destructuring
parameter/binding-element flow visits tokens, names and type children; its initializer helper returns immediately
when absent, as required by the eligibility gate. Parameter binding can scan its parent's parameter list before
declaring the symbol, so that list view must end before table mutation. Function/constructor-type binding creates
two fresh symbols and its own member table; it does not install that table on a previously published symbol.
These checks narrow the audited paths but do not classify all AST/helper callbacks or finish M0.

The publication probe must reject an unexpected write to an old flow or a foreign binding owner, rather than
silently assuming these observations cover every future caller. Such a rejection is a feasibility failure to
resolve before cutover, not permission to change TypeScript behavior.

The contract test uses the legacy backend, in one process with eager controls collected before enabling lazy
parsing. It proves no owned-storage lifetime or concurrent-access property. Its ten eager inputs (including the
subsequent constructor parameter-property case) are also checked against the pinned Go binder oracle; all dumps
match. Keep that separate from eventual owned-path
oracle parity, which is still unimplemented.

## Exit work still required

1. Complete a transitive callee/write/read inventory, including indirect lazy forcing and shared symbol-data
   readers in checker caches. Lexical searches are evidence, not a complete inventory.
2. Complete owned protected binding data. The isolated prototype now has file-qualified symbol/flow/list keys,
   guarded records, all schema binding annotations, ordered symbol tables and both Unknown-kind flow payloads.
   Shared-data checker-symbol integration and the complete annotation mutation audit remain unfinished.
   Do not place legacy `P` fields in the safe storage module.
3. Port a real parser/binder feasibility case with nested forcing and merges, concurrent different lists,
   same-context recursion, terminal failure, cached symbols and run-slot waits.
4. Measure guards/atomics, contention, allocations, instructions and RSS before selecting a production protocol.

## Binding namespace probe

The [binding implementation](../crates/tsrs_ast/src/storage/binding.rs) assigns a binding-owner identity distinct
from syntax-store identities. Base and fragment syntax encode symbol/flow indexes relative to that one owner.
Initial binding borrows both builders exclusively; published binding reads/writes use standard `RwLock` guards.
The binding lease retains base syntax and explicitly published fragments without a syntax-to-binding strong edge.

Nine [owned-storage tests](../crates/tsrs_ast/src/storage/binding_tests.rs) exercise shared base/fragment identity,
annotation validation, value-declaration replacement, flow/payload domains, fragment retention, readers of cached
symbols during completed/abandoned writes, class/file container links and ordered symbol table mutations/clones.
External-client compilation failures reject escaping symbol/annotation views, dropping their read guard, mutation
with a live binding/annotation view, and mutation with a table iterator. These pass under the normal compiler;
the nine runtime cases also pass Miri. They exercise the new representation, not the production parser/binder.

Schema Go-only annotation columns are separate from immutable syntax, and published writes go through binding
authority. Initial publication now validates all binding and base/retained-fragment syntax dependencies; later
writes validate a journal before completion. The isolated `FileBindingContext`/`LazyBindSession` implementation
also covers reentry, initial Pending/ParseOnly behavior, failure, fragment registration and retirement ordering.
See the [progress log](ast-soa-progress.md) for current test evidence. Its callbacks construct nodes directly;
they do not execute the production parser/binder algorithms.

Still unproven: the complete production annotation/callee audit, real owned parser/binder integration, and all
shared checker-symbol readers following their binding data owner. Raw lease read/write APIs can still be misused
to reenter a lock; production forcing must use the explicit context. Namespace/protocol tests do not substitute
for that integration or establish a no-regression result.
