# AST SoA constraints and alternatives

Exploration requested on 2026-10-23: the first ownership migration is AST nodes in a Program-owned structure of
arrays (SoA), mutable during parsing and binding. This supersedes the mapper-first suggestion in
[the memory-model history](safe-memory-model.md). No compiler implementation or performance result is claimed here.

This document preserves the constraint exploration and alternatives. The subsequent
[selected architecture](ast-soa-architecture.md) resolves the initial publication choice with a guarded parsed
entry and specifies a file-level deferred-binding protocol. Its [migration plan](plan-ast-soa-migration.md)
supersedes the preliminary next-work suggestions below. Treat unchosen options here as alternatives, not as
additional implementation requirements.

Subsequent owner constraint: the migration must produce safe Rust with no performance or memory regression.
Costs discussed below are investigation targets, not accepted production tradeoffs; the selected architecture
and plan apply this stricter acceptance rule.

Inspected local main: `ec0dba38`. The reference checkout's HEAD matches the Cargo pin
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with no tracked changes under `ts-ref/tsc`. The
[Go AST](https://github.com/microsoft/TypeScript/blob/3e9236361fa97a9e6c685f4833ac457ae2d8c50a/tsc/internal/ast/ast.go),
its schema and the current Rust accessors constrain behavior; storage
layout and access plumbing can change while preserving those algorithms.

## Recommendation

Build a **Program-owned directory of file-version AST stores**, with exclusive builders for parsing and
binding. Published AST stores are immutable except for specifically synchronized metadata. Unchanged file versions
can be shared between Programs. Lazy syntax and checker/emit-generated syntax need explicit additional AST stores,
not mutation of a `Vec` that readers are borrowing.

This is Program ownership through owned/shared storage handles, not exclusive ownership of every file by exactly
one Program. If exclusive ownership and one contiguous set of Program-wide columns are requirements, cloning and
remapping reused files or introducing a persistent shared backing store become necessary.

There are three independent choices: physical column layout, storage-key encoding, and ownership/publication.
Choose a clear safe baseline for each, then measure compression and grouping choices. SoA alone does not solve
lifetimes, and a safe AST store does not make the remaining symbol/type/Program interfaces sound.

## What the current code requires

| Constraint | Evidence | Consequence |
| --- | --- | --- |
| Node identity is pervasive | [AST contract](../docs/AST.md), symbol declarations, flow nodes, checker link keys and API node tables | Keep one identity per node. Replacing names/tokens with values or rebuilding wrappers with new identities is a different change. |
| A node is currently a physical header plus an adjacent typed payload | [`ast.rs`](../crates/tsrs_ast/src/ast.rs), generated `as_*` methods | SoA needs generated views and accessors instead of header/payload pointer arithmetic. |
| Parsing and binding both mutate | [parser](../crates/tsrs_parser/src/parser_1.rs), [reparser](../crates/tsrs_parser/src/reparser.rs), [binder](../crates/tsrs_binder/src/binder.rs) | Expose a writer during those stages. The writer must cover flags, parents, locations, reparser child replacements, symbols, locals and flow annotations. |
| Bind order is observable | [`Program::bind_source_files`](../crates/tsrs_compiler/src/program.rs) binds in reverse file order when single-threaded | Do not combine parse+bind per file merely for ownership convenience when the current mode binds later. Preserve private-name/symbol-ID timing and the reference order. |
| Parsing/binding can run per file in parallel | File loader and `bind_source_files` | Workers need disjoint mutable file builders, not a shared global mutable vector. |
| Files are reused between Programs | [`Program::reuse_program`](../crates/tsrs_compiler/src/program.rs) retains unchanged source-file objects; [parse cache](../crates/tsrs_project/src/parsecache.rs) shares parsed/bound versions | Stable file-version storage and identities must survive the old Program. The cache can own a file even before a new Program exists. |
| Old requests outlive snapshot replacement | [LSP ownership](../docs/LSP.md#memory-management) | Releasing a Program/snapshot cannot invalidate a request's AST views. Retain an actual storage owner. |
| Lazy JSDoc creates nodes after binding | [`SourceFile::resolve_jsdoc`](../crates/tsrs_ast/src/ast.rs), [JSDoc parser](../crates/tsrs_parser/src/jsdoc.rs) | Immutable base columns need an owned extension/publication mechanism. This is reference behavior, not just a tsrs optimization. |
| Language-service token caches also create nodes on demand | [`SourceFile::get_or_create_token`](../crates/tsrs_ast/src/ast.rs), [LS token access](../crates/tsrs_ls/src/lsutil/children.rs) | File-owned late syntax includes cached tokens, with stable cache identity and source parents, not just JSDoc/member fragments. |
| Lazy declaration members can be parsed and bound during checking | [`lazylist.rs`](../crates/tsrs_ast/src/lazylist.rs), parser and binder hooks | Keep deferred parse/bind timing and publication semantics. Disabling laziness is a comparison mode, not a completed migration. |
| Lazy member binding is reentrant | `force_nodes` returns parsed nodes to the forcing thread before binding completes; other threads wait | A recursive `OnceLock::get_or_init` replacement can deadlock. Distinguish the binder's in-progress view from published reader views. |
| Checker and emit factories create more nodes | [synthetic element access](../crates/tsrs_checker/src/checker_09.rs), [emit context](../crates/tsrs_printer/src/emitcontext.rs) | Program source syntax and temporary/generated syntax cannot all share one permanently frozen allocation domain. |
| Node edges can cross storage domains | Synthetic nodes can retain source children/parents; file-loader synthetic imports have a source-file parent | A local node index is insufficient as the universal node reference. |
| Syntax exists without a compiler Program | Parser oracles/tests, config/JSON parsing, API transpilation and codec paths | Define storage APIs in `tsrs_ast`; let `ProgramBuilder` own them in compiler paths. Standalone callers need an AST owner of their own. |
| Source-file state is more than syntax | [`SourceFile`](../crates/tsrs_ast/src/ast.rs) includes text, caches, parse/bind diagnostics, pragmas, imports and content-mapper state | Split file metadata from node columns deliberately; preserve `SourceFile`-as-node identity at the API boundary. |

The current generation policy already identifies the main mutable fields: `CELL_EXPLICIT` and Go-only binding
fields in [gen-ast.ts](../tools/gen-ast/gen-ast.ts). The binder also changes flags such as `ExportContext`,
`ContainsThis`, return flags and error propagation; binding is not just filling symbol IDs beside immutable flags.

## Ownership and publication options

| Option | Advantages | Cost / unresolved constraint |
| --- | --- | --- |
| One exclusive monolithic SoA per Program | Simple indexing and a clear bulk owner; little per-file table metadata | Parallel append needs batching/ranges or synchronization. Reused files need copying/remapping; old snapshots and parse-cache sharing do not fit exclusive ownership directly. Per-file early release is difficult. |
| Program directory owns shared file-version SoAs | Preserves existing file reuse, parallel file work and per-file lifetime boundaries | Adds AST store lookup and per-file column metadata. Lazy and synthetic stores still need explicit handling. Recommended baseline. |
| Persistent/chunked Program-wide columns | Shares unchanged chunks and amortizes payload-table metadata | Requires immutable chunk/version directories, allocation/publication rules and a retention policy; coarse chunks can retain dead files. A follow-up if file-local metadata proves expensive. |
| One lock around a mutable Program AST | Can enforce access soundly with standard guards | Serializes mutation and makes reads guard-bound. Returning references after dropping the guard is invalid. Useful for a bounded prototype, not an assumed production cost model. |

The recommended directory can conceptually look like this; these are type shapes, not compiled APIs:

```rust
struct ProgramAst {
    files: Vec<Arc<FileAstVersion>>,
    // Resolve stable AstStoreIds to the file/fragment owners this Program retains.
    // The representation of this directory is a measured design choice.
}

struct FileAstBuilder {
    common: CommonColumns,
    payloads: PayloadTables,
    lists: ListColumns,
    // Mutable file metadata, owned text, binding annotations and factory state.
}

struct FileAstVersion {
    base: FrozenFileColumns,
    // Explicit lazy fragment slots/caches; retained independently of any one checker.
}
```

This minimal directory retains every file body until the directory drops. Preserving CLI leaf-file reclamation
needs a further split: keep file identity/metadata independently, and retain releasable AST bodies through directory
entries and operation leases. Removing a body's directory owner then permits reclamation only after its last lease
ends. A lock around an `Option<Arc<AstBody>>` is one safe starting shape for such an entry; read operations retain
the returned `Arc` and borrow columns from it after releasing the entry lock. The existing proof that later work
does not need the leaf still applies. Changing the allocator or making failed lookups safe does not establish that
proof. A whole-program lease retaining every body across checking would defeat early reclamation too.

An `Arc` per file version is an ownership-boundary cost. An `Arc` per node or a lock per scalar field is unnecessary
for the base parse/bind model. `Arc` can share `FileAstVersion` across threads only when the fields actually satisfy
the standard `Send`/`Sync` bounds; it does not legitimize unchecked cells.

The Program itself currently exposes manually freed `'static` views. Merely adding an `ast: ProgramAst` field and
returning a reference from that view would preserve the lifetime hole. New AST entry points must borrow an actual
AST owner/lease, and a complete public ownership path must retain it. Honest Program ownership is a dependency for
calling the entire Program interface sound, even if the first storage migration is only AST nodes.

## The mutation boundary

Two phase models are viable; their tradeoffs need to be explicit.

**Exclusive parse/bind builder.** Parse into a `FileAstBuilder`, inspect its syntax during loading, then give the
binder exclusive access when the existing pipeline would bind that file. Publish the finished version afterward.
This avoids interior mutability in ordinary node columns. A Program builder can hand different file builders to
different workers, join them, and assemble the read directory. Attaching an AST store moves its owner; node keys do
not depend on the address of the moved `Vec`.

This requires an ownership transition in host/cache APIs that currently return shareable pointers to unbound files.
Do not assume `Arc::get_mut` succeeds after a cache or another Program already holds the file. The project parse
cache already parses and binds before publishing; the CLI loader and standalone parsed-tree APIs need separate
consideration. Do not eagerly bind at parse time to bypass this problem.

**Immutable parsed syntax plus published binding columns.** Publish parse output first; bind against read-only
syntax and an exclusively owned `BindingBuilder`, then publish its result. This fits lazy binding and existing
parse-only readers more naturally. The final flag view must preserve both parser and binder writes, including
clears, not blindly OR two sets of flags. It can require copied flag columns, explicit overrides, or a staged
ownership transfer; compare their space and lookup costs. `ParsedAstView` and `BoundAstView` can select the proper
columns once at operation entry rather than testing binding state on every scalar read.

Start by prototyping the exclusive builder API. Resolve the publication choice against the actual loader/cache
paths before committing to a production representation. Both models can keep ordinary parse/bind mutation safe;
neither requires every field to be a `Cell`.

For parser/binder code:

- Pass copied node keys through recursive calls; borrow a column briefly to read or write it. Do not retain a
  slice or element reference while an operation can grow its backing vector.
- Split source text, syntax columns and binding columns into disjoint borrows where possible. Construct a scanner
  borrowing input text on the caller's frame; move text into the finished file only after scanner/parser borrows
  end. Avoid a self-referential parser that owns text and stores a borrow into itself.
- Give constructors an explicit writer. Existing `NodeFactory` is cloneable, takes `&self`, and is shared by
  visitors; an exclusive writer cannot be cloned that way. Keep factory configuration/hooks separate from the
  current `&mut` writer, or use a checked `RefCell` during the thread-local build phase.
- A `RefCell` builder can preserve nested `f.new_x(f.new_y())` call patterns if insertion releases its borrow
  before callbacks run. Its guards must never escape or remain held over reentrant construction. With `&mut`
  constructors, sequence nested writes into locals, preserving evaluation order.
- Generated visitors should read the necessary child keys, release their borrow, then invoke mutating callbacks.
  A stable list range plus indexed key reads can avoid cloning whole child lists.

### Concrete mutation authority

Program ownership determines retention. A writer borrow or synchronization guard determines who can mutate. A
copied `NodeKey` grants neither ownership nor write access. Generated setters should therefore move from
`node.set_flags(flags)` to `writer.set_flags(node, flags)`.

| Place | Mutation authority | What readers can observe |
| --- | --- | --- |
| Parsing/reparsing one file | Exclusive `&mut FileAstBuilder` | Temporary views borrowing that builder; no simultaneous mutable access while such a view remains in use. |
| Initial binding | The same exclusive builder, or an exclusive `BindingBuilder` in the two-stage model | Binding reads syntax and writes annotations through explicit contexts. |
| Parallel files | A different exclusive builder per worker | Shared input is immutable; a worker cannot mutate another worker's file through its writer. |
| A published file shared by Programs | Immutable columns; atomics for scalar lazy IDs and standard guards for designated caches | No mutable reference to the whole store. Shared `Arc` ownership does not grant mutation. |
| Lazy syntax or cached LS tokens | Private fragment/token builder, followed by synchronized cache publication | Fully constructed syntax; cache lookup retains an owner before returning a borrowed view. |
| Deferred member binding | Exclusive in-progress binding context, followed by synchronized publication | The binding thread can read its own partial work through that context; ordinary readers wait for completion. Existing owner-symbol updates also need protection. |
| Checker/emit-generated syntax | Exclusive writer borrowed from that checker/result or emit job | Read-only source inputs plus locally mutable synthetic storage. A setter rejects a target belonging to another store. |

For example, ordinary recursive binding can copy the few needed keys and then mutate without retaining payload
borrows. This is pseudocode for nodes local to one builder:

```rust
fn bind_binary(ast: &mut FileAstBuilder, node: NodeIndex, computed_flags: NodeFlags) {
    let (left, right) = {
        let binary = ast.binary(node);
        (binary.left(), binary.right())
    }; // The view's borrow ends here; only copied indexes remain.

    bind_node(ast, left);
    bind_node(ast, right);
    let flags = ast.flags(node);
    ast.set_flags(node, flags | computed_flags);
}
```

This allows vector growth during either recursive call. Holding `binary` for use after a mutable call would be
rejected by Rust. A writer accepting qualified `NodeKey`s must validate that the target belongs to its own store;
reading another store's node does not authorize writing it. A parent value may point into another store while
the node whose parent is being assigned remains local to the writer.

Publishing should consume the builder, for example `finish(self) -> FrozenFileAst`, so the mutable construction
interface cannot remain live after sharing. Any remaining atomics/caches are explicit fields with safe APIs,
rather than unchecked mutation hidden behind ordinary node references. Cache guards must protect the actual
mutable collection, not just act as an external convention around an unchecked cell.

For deferred member binding, the private fragment rule is only part of the solution. Binding also fills existing
owner-symbol tables. The probe must account for every such read and write: stage updates in an in-progress view
and publish them through synchronized state, or require guards on both readers and writers of shared tables.
Reentrant binder reads must receive the in-progress context explicitly. Holding an ordinary non-reentrant mutex
and calling a public accessor that tries to acquire it again is invalid; releasing a guard and continuing through
an escaped mutable reference is invalid too. The exact owner-symbol publication API remains an open design task.

## What to put in SoA

Use dense common columns for all nodes, plus typed payload storage for fields that apply only to some kinds.
Generate both from the pinned schema. An illustrative common layout is:

```rust
struct CommonColumns {
    kinds: Vec<Kind>,
    flags: Vec<NodeFlags>,
    ranges: Vec<TextRange>,
    parents: Vec<Option<NodeKey>>,
    payloads: Vec<PayloadIndex>, // Safe discriminated reference to a typed payload record.
    semantic_ids: Vec<AtomicU32>,
}
```

These columns are private. Allocation reserves/pushes the required payload and all common fields before returning
a node key. Readers cannot observe half-created nodes. On allocation failure/panic, either unwind the unpublished
builder or restore every affected column to a consistent length. Any rewind must restore all columns and side
tables involved, not just the common node count.

Keep three distinctions:

1. **Common-field SoA versus complete per-kind SoA.** `Vec<BinaryExpressionPayload>` beside common columns is a
   useful hybrid baseline. Full payload SoA instead uses arrays such as `lefts`, `operators`, `rights` indexed by
   the binary payload index. Both can use safe indexing; measure before splitting every cold field into a vector.
2. **Kind versus payload tag.** They are not interchangeable. `FlowSwitchClauseData` and `FlowReduceLabelData`
   both use `Kind::Unknown` with different payloads. The generated token factory also has canonical payload rules.
   Keep a safe payload discriminator; do not derive every cast solely from `Kind`.
3. **Dense common fields versus rare fields.** Preserve sparse storage for the existing `RARE_FIELDS` policy.
   Allocating every optional payload field for every node would undo earlier layout work.

Typed views replace physical typed references. `as_binary_expression()` can return a small
`BinaryExpressionView<'a>` whose `left()` reads a column and returns a key. It cannot return `&BinaryExpression`
when that Rust object no longer exists. Direct field reads, embedded-base references and `as_p()` round trips
must migrate; keeping their spelling through unsafe fake objects would defeat the new memory model.

The generator must also preserve traversal order, update/clone identity rules, hooks, nil-able fields, and the
existing SourceFile/flow synthetic variants. Modify [the generator](../tools/gen-ast/gen-ast.ts), not generated.rs
by hand. The codec has its own schema-generated access path that also needs updating.

## Storage keys and access

`NodeId` already means a lazily assigned semantic ID (`u64`) used by compiler/link/codec behavior. Give storage
handles another name, such as `NodeKey`, and keep semantic ID assignment in the existing places and order.

Use these terms for the proposed storage model:

- **`AstStore`**: one owned set of AST columns, usually for one source-file version. Lazy fragments and synthetic
  syntax can have additional stores with their own owners.
- **`AstStoreId`**: the identity of that store, preserved when Programs share the same immutable file version.
- **`NodeIndex`**: a node's index within the store's common columns.
- **`NodeKey`**: the combination needed to identify a node across stores.

```rust
struct NodeKey {
    store: AstStoreId,
    index: NodeIndex,
}
```

The store identifies ownership; the index selects the node's entries in its SoA columns. One monolithic Program
store would need only a `NodeIndex` inside that Program.

| Key design | Benefit | Tradeoff |
| --- | --- | --- |
| Program-wide `NodeIndex` backed by `u32` | Four-byte edges and direct common-column indexing | Requires a stable shared namespace/backing store across reused Programs, or node-key remapping when importing files. Parallel allocation and rollback also need rules. |
| `NodeKey { store, index }`, initially eight bytes | Explicit identity for reused files, lazy fragments and synthetic stores; easy to validate | Doubles the width of edges currently using four-byte `P<Node>` handles and adds store lookup. This is a correctness baseline, not a promised memory win. |
| Four-byte local edges plus qualified external keys | Keeps most parsed child edges compact; qualifies keys when leaving an AST store | Parent/child edges between stores need a checked escape table or tagged representation. Stored keys in symbols, diagnostics and caches still need a valid namespace. |
| Compact global key with a relocation directory | Stable narrow keys independent of physical column placement | Adds per-node or per-chunk directory metadata and another lookup. Compare the total cost, not just the handle width. |

Start with explicit AST store identity for the ownership probe, and measure narrow local edges before scaling it to
the whole compiler. AstStoreIds identify storage versions, not file paths or positions in one Program's file list.
Two versions of the same file need distinct identities. Keep the same AstStoreId when the same immutable
version is shared by multiple Programs. Prevent wraparound/reuse from accepting stale external keys.

Provide a concrete read context defined in `tsrs_ast`, owned/borrowed through Program storage. For example,
`AstRead::node(NodeKey) -> NodeView<'_>` and `AstWrite::set_parent(NodeKey, Option<NodeKey>)`. A node key alone has
no `Deref`. `NodeView` is temporary; edges and caches store keys, not owner references per edge. The read context
can resolve source, extension and synthetic domains while the operation retains all required owners.

That direct read signature assumes the context already retains the referenced AST store. For a mutable directory
or lazy cache, first acquire an owned lease on the AST store, then borrow node views from that lease. Do not return a
reference borrowed through a temporary directory guard or an `Arc` local that immediately drops. Hoist leases out
of local traversal loops rather than adding a refcount operation to every field access.

Do not hide the read context in a thread-local “current Program.” That makes keys ambiguous across nested API
calls, old snapshots, and concurrent Programs, recreating an ambient ownership contract. Keep the AST crate
independent of `tsrs_compiler` to avoid a dependency cycle.

## Lists, text and speculation

**Lists:** introduce a separate `ListKey`/list-index domain, with element ranges into owned key arrays. Preserve list
identity where update/clone code compares it, and preserve `pos`, `end`, empty/nil distinctions and trailing-comma
behavior. The [codec](../crates/tsrs_api_codec/src/encoder.rs) serializes list records; the printer uses list spans
for comments and maps. A raw child range alone is insufficient. Mutable build lists can append replacement runs
and update their owning list record, without changing published slices in place.

**Text:** own source text with the file version and represent source-derived text by checked UTF-8 byte ranges.
Escaped identifiers, synthetic names and cooked/raw literal strings need separate owned text storage. Avoid
introducing global interning as part of this migration: [it was already measured and rejected](mem-round2.md).
The [current identifier](../crates/tsrs_ast/src/identifier.rs) packs text and flow state together; SoA can separate
them, but their total space needs measurement. Do not preserve its unsafe global source-text pointer registry just
to make owner-free `text()` calls possible.

**Speculation:** the current [parser checkpoints](../crates/tsrs_parser/src/parser_1.rs) sometimes retain nodes or
cancel rewind when data escaped into a cache. The safe first model can retain speculative node entries until the
builder dies, while measuring the cost. A production rewind must truncate all relevant payload/list/text columns
and undo any writes into preexisting entries, or retain the current proof that none escaped. Reusing a truncated index
can alias a stale key even though indexing remains memory-safe. Use append-only/tombstoned identities, generations,
or an enforceable transaction boundary; do not reset semantic ID counters.

## Lazy and synthetic storage

**Lazy JSDoc:** parse into an exclusively owned fragment and publish it in the file version's synchronized cache.
The fragment keeps the source owner alive or borrows through a caller-retained file owner; avoid a strong ownership
cycle where the file owns a fragment that owns the same file. Nodes use qualified parent/source references.
An owned fragment handle or guard must survive any read returned from a mutable cache; a temporary lock cannot
justify returning a reference after it is released.

**Language-service tokens:** `get_or_create_token` currently creates tokens under a file-owned mutex and caches
their identity by token key. Give these nodes file-owned extension storage too. Construct and parent each token
before publication; repeated requests must resolve the same cached node. Preserve the chosen lock/reentrancy
contract without lending references into a token vector that later cache insertions can grow.

**Lazy member lists:** preserve the existing ordering: parse, make parsed nodes available to the binding thread,
bind using saved context, then publish the completed result to other threads. One possible safe shape is immutable
parsed fragment columns plus a separate in-progress binding view held by `BindContext`; completed binding data is
published afterward. Reentrant reads use that explicit context rather than recursively initializing the same
`OnceLock`. Synchronization must also complete or report failure after panic/cancellation so waiters cannot hang.
This publication protocol and its interactions with owner-symbol tables need a focused prototype.

**Synthetics:** file-loader synthetic imports belong to Program-loading storage. Checker-created syntax belongs to
a checker/result-storage owner; emit transformations belong to an emit-job owner. Each uses the same safe column
backend, but retains its own lifetime. Read contexts resolve edges into source storage. If “Program-owned” requires
all of these AST stores to live until Program drop, that simplifies retention but gives up existing emit/checker
reclamation; quantify the cost before making that a production policy.

The remaining arena-backed `Symbol`, `FlowNode`, diagnostics and checker records hold node references. Their
`P<Node>` fields must become keys too. Keeping their allocators temporarily is possible; retaining raw pointers
into retired AST storage is not. AST safety remains distinct from the known lifetime risks of those other owners.
Retention works in the other direction as well: a longer-lived cache/result that stores a node key must retain its
storage owner, or be covered by an owner that does. Keys do not keep their targets alive. Synthetic payloads that
currently refer back to checker types also need an explicit checker/result lifetime; storing them in a Program
store does not extend that checker's lifetime safely.

## Space and performance constraints

The current common node header is 24 bytes and a compressed node edge is four bytes. Separating fields does not
automatically shrink them: an illustrative split of kind (2), payload tag (1), payload index (4), flags (4), range
(8), local parent (4), and semantic ID (4) is **27 bytes per node before payloads**, ignoring capacities and table
metadata. An eight-byte optional parent adds another four bytes if its encoding has a niche; a naive
`Option<(u32, u32)>` is larger still. A safe enum descriptor may occupy more than its split
fields. These are layout arithmetic, not measured compiler results; pin actual candidate sizes before comparing.

The schema currently has **192 node types**, plus two extra flow payload types. One empty `Vec` header for every
schema type in every file would already cost `192 * 24 = 4,608` bytes per file on a 64-bit target, before common
columns or any payload elements. Full field-by-field SoA adds more headers. Even one `Option<Box<Table>>` slot
per type costs `192 * 8 = 1,536` bytes per file before allocating present tables. These are illustrative designs,
not an instruction to give payload-less types actual tables.

Compare sparse allocation of present payload tables, hybrid payload records, and chunking several files together.
Grouping reduces metadata but can retain removed files and complicate early leaf-file release. Measure reserved
capacity and per-file metadata on tiny files as well as aggregate live elements on large ones.

Other costs to measure: AST store lookup per accessor, cache locality when a traversal reads many columns,
vector growth/compaction and copying, false sharing in the lazy semantic-ID column, wider symbol declaration and
cache keys, and any guard/refcount work on the read path. Hoist the file/store context through local traversals
where possible; avoid a hash lookup for every common-field read by default.

The [compact-AST study](mem-compact-ast-sizing.md) provides historical public-project counts and fidelity constraints;
it did not implement this SoA design. Its old percentage rejection threshold is superseded by current policy.
The new constraint here is sound ownership and removal of unsafe AST layout/access, in addition to any measured
memory benefit. Preserve [leaf release](mem-free-leaf-files.md), [file reuse](../docs/LSP.md#memory-management) and
[emit lifetime boundaries](mem-emit-regions.md) unless a measured tradeoff justifies a change.

## Suggested next work

1. **Set the storage contract.** Define Program ownership as a directory retaining file versions, choose explicit
   `NodeKey`/`ListKey` domains and owner-backed read/write views, and specify parsed versus bound publication.
   Keep keys distinct from semantic IDs and API wire indexes.
2. **Build a storage/API probe.** Exercise common columns, a payload-less token, identifiers with both text modes,
   binary expressions, declaration binding annotations and lists. Test recursive reads/writes and vector growth
   without retaining invalid borrows. Include a source-to-synthetic edge and two Programs sharing a file version.
   A standalone container demonstration is not enough to establish parser/binder feasibility.
3. **Move schema generation to the chosen access model.** Generate typed table allocation, accessors, mutators,
   traversal, clone/update logic and invariant checks. Port parser/reparser and binder through explicit views,
   preserving the phase order. A build-time experimental backend can keep comparison binaries available; a full
   pointer-mirror AST would distort both ownership and memory measurements.
4. **Close late-allocation paths before claiming completion.** Implement JSDoc, cached LS tokens, reentrant lazy members, Program
   synthetic imports, checker/emit stores, source-file metadata and old-snapshot ownership. Migrate node references
   in symbols/flow/diagnostics, codec and language-service consumers. Preserve the existing wire format.
5. **Validate and measure the actual compiler.** Use AST/binder reference comparisons, all conformance baselines
   and both checker modes, then LSP/API, emit/maps, watch/build, leaf release and snapshot lifetime checks. Cover
   early owner drop, wrong-version keys, reentrant forcing, concurrent forcing and cancellation. Run Clippy/source
   lint, generated-source checks, Miri on the new store and the relevant race checks. Measure instructions and
   default-checker peak RSS across the catalog, plus parse/bind timings and repeated lifecycle retention; use
   release binaries with full symbols and stripping disabled.

The first decision to settle from the probe is the publication boundary, not a packed bit layout: whether all
ordinary file mutation finishes under one unique builder, or parsed syntax and binding annotations publish in two
stages. Both are compatible with Program-owned SoA, but they lead to different host/cache APIs and reader views.
