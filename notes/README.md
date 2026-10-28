# Engineering notes

These notes record measurements, design decisions, regressions and approaches that did not work. They are
evidence from a particular revision and machine; the [compatibility table](../docs/COMPATIBILITY.md) describes
current capabilities, and the [release notes](../docs/STATUS.md) describe shipped changes.

## Reading historical results

Compiler defaults, source pins, commands, tool paths and open questions in a note belong to the revision it
records. For current behavior and commands, start with [usage](../docs/USAGE.md),
[contributing](../CONTRIBUTING.md) and [debugging](../docs/DEBUGGING.md).

Since 0.20.0, ordinary compilation uses pinned TypeScript's checker settings; adaptive checker counts and the
additional performance modes require `--fast` or an explicit feature override. Older notes often call those
settings the default. See [performance options](../docs/USAGE.md#performance-options) when comparing runs.

Older acceptance percentages are historical decision criteria. The current
[measurement policy](../AGENTS.md#performance-and-memory-changes) has no fixed minimum percentage: repeatable
improvements across most catalog projects, or a significant repeatable improvement on one, may be accepted.
An idea rejected only for missing an old threshold can be revisited; correctness failures and measured
regressions still need to be addressed.

Allocation profiles and reachability-census results in older notes came from tooling that has been removed.
Their measurements remain historical evidence. For supported memory reports and ownership checks, use the
[current debugging guide](../docs/DEBUGGING.md#arena-recycling-checks).

Completed experiments' build wrappers, AST sizing table scripts and Node API probes and audit logs have also been
removed. Their last versions are available at commit `b620e8e5` (for example,
`git show b620e8e5:tools/sizing/compact_ast.py`). References to them below are historical; reusable measurement
tools, code generators and regression harnesses remain in `tools/`.

Several later tooling migrations replaced Python entry points with JavaScript and moved Rust checks into
integration binaries. Reproduce an old experiment from its recorded revision; use the
[current script catalog](../tools/SCRIPTS.md) for today's entry points. Check a follow-up record before treating
an old plan or outstanding item as current work.

## Before changing performance or memory use

Start with the [measurement and landing rules](../AGENTS.md#performance-and-memory-changes), then read the
[techniques already in use](../docs/RUST.md#techniques). Search this directory for the affected subsystem before
starting another experiment.

- [Measured and rejected approaches](perf-round2-followups.md#measured-and-rejected-do-not-redo): check this list
  before retrying an idea. The follow-up sections are dated; they are not a live roadmap.
- [Rust string helpers](rust-string-helpers.md): equivalent splitting/whitespace contracts and measurements.
- [Serial work and added complexity](perf-serial-steps.md): why small gains may not justify a cache, thread or
  overlap between phases.
- [Checker instruction-count round 4](perf-icount-round4.md): the later checker measurements and validation.
- [Enum members in isSimpleTypeRelatedTo](perf-simple-relation-enum-members.md): the one flag pair behind three
  quarters of the calls on Compiler-Unions, answered inline.
- [Front-end allocations, round two](perf-reparse-cloner.md): one reparse cloner per file instead of a visitor per
  JSDoc clone, a borrowed declaration extension, and the profile that ranks what is left.
- [Memory round 4](mem-round4.md): the latest numbered memory round against Bun.
- [Benchmark workflow](../bench/README.md) and [published results](../bench/RESULTS.md): reproduce measurements
  and compare the released build configuration.

## Design decisions

| topic | record |
| --- | --- |
| Compressed arena handles | [Pointer compression](mem-pointer-compression.md) |
| Reclaiming file trees after checking | [Leaf-file regions](mem-free-leaf-files.md) and [their cost](mem-leaf-regions-cost.md) |
| Emit scratch lifetimes | [Per-file emit regions](mem-emit-regions.md) |
| Shared checker graphs, an opt-in experiment | [Shared-graph design and measurements](spike-shared-graph.md) |
| Persisting parse and bind results, a deferred design | [Persisted front end](../docs/PERSISTED_FRONTEND.md) |
| Release build optimization | [PGO](perf-pgo.md) and [build configuration](perf-build-level.md) |
| Decimal number parsing | [Rust decimal parsing](rust-decimal-parsing.md) |

## Correctness and ownership

- [Remaining checker-history dependencies](open-history-dependence.md): cases where file order or checker
  history affects diagnostics.
- [Incremental diagnostic history](fix-history-dependent-diagnostics.md): cache reuse and diagnostic stability.
- [Arena recycling and protected borrows](fix-arena-recycle-uaf.md): the lifetime bug and its regression guards.
- [Unsafe audit](unsafe-audit.md): every unsafe site and lifetime escape classified, with the sites where safe code
  can break an invariant (issue #646).
- [Protocol quoting styles](shared-go-compat.md): shared helpers retain each caller's escaping behavior.
- [Language-server memory ownership](lsp-mem.md) and [lifetime corrections](lsp-memfix.md): regions, snapshots
  and checker disposal.

## Subsystems and earlier work

For current implementation guidance, use the [emit guide](../docs/EMIT.md),
[language-server guide](../docs/LSP.md) and [Node API guide](../docs/NODE_API.md).
The initial [emit plan](../docs/history/EMIT.md) and [language-server port log](../docs/history/LSP.md)
are archived separately from those guides.

API binary responses use padded standard [Rust Base64 encoding](rust-base64.md).

Other useful entry points are [content mappers](contentmappers.md), [WebAssembly](wasm-build.md),
[performance round 2](perf-round2.md), [performance round 3](perf-round3.md),
[memory round 2](mem-round2.md) and [memory round 3](mem-round3.md).

## Complete catalog

Every note is listed once below by subject. The links above are suggested starting points.

### Source upgrades and correctness

- [checker-11](checker-11.md)
- [determinism-base-constraint-depth](determinism-base-constraint-depth.md)
- [determinism-instantiation-count](determinism-instantiation-count.md)
- [determinism-type-reference-from-type-node](determinism-type-reference-from-type-node.md)
- [determinism-undefined-property](determinism-undefined-property.md)
- [fix-arena-recycle-uaf](fix-arena-recycle-uaf.md)
- [fix-history-dependent-diagnostics](fix-history-dependent-diagnostics.md)
- [fix-mutation](fix-mutation.md)
- [fix-project-types](fix-project-types.md)
- [fix-types-baseline](fix-types-baseline.md)
- [flow-memo](flow-memo.md)
- [fuzz-derived-variance](fuzz-derived-variance.md)
- [open-history-dependence](open-history-dependence.md)
- [perf-order-independence](perf-order-independence.md)
- [typescript-623-ports](typescript-623-ports.md)
- [typescript-7.1-beta-ports](typescript-7.1-beta-ports.md)
- [upstream-gaps-2026-10-08](upstream-gaps-2026-10-08.md)

### Checker algorithms and inference

- [cpu-checker](cpu-checker.md)
- [lazy-members](lazy-members.md)
- [perf-checker-algorithms](perf-checker-algorithms.md)
- [perf-checker-cpu2](perf-checker-cpu2.md)
- [perf-checker-cpu3](perf-checker-cpu3.md)
- [perf-checker-inline-cold](perf-checker-inline-cold.md)
- [perf-checker-single-base-flag](perf-checker-single-base-flag.md)
- [perf-derived-variance](perf-derived-variance.md)
- [perf-flow-union-inference](perf-flow-union-inference.md)
- [perf-heavy-files-infer-memo](perf-heavy-files-infer-memo.md)
- [perf-shared-tuple-elements](perf-shared-tuple-elements.md)
- [perf-simple-relation-enum-members](perf-simple-relation-enum-members.md)
- [perf-symbol-map-lookups](perf-symbol-map-lookups.md)
- [perf-t3code-instantiation](perf-t3code-instantiation.md)
- [perf-tsgo-can1357-speedups](perf-tsgo-can1357-speedups.md)
- [perf-union-inference](perf-union-inference.md)
- [perf-union-type-facts](perf-union-type-facts.md)

### Checker scheduling and parallelism

- [mem-assignment](mem-assignment.md)
- [perf-balance](perf-balance.md)
- [perf-checker-64](perf-checker-64.md)
- [perf-checker-scaling](perf-checker-scaling.md)
- [perf-checker-stealing](perf-checker-stealing.md)
- [perf-clustered-assignment](perf-clustered-assignment.md)
- [perf-default-checkers-16](perf-default-checkers-16.md)
- [perf-default-checkers-small-machines](perf-default-checkers-small-machines.md)
- [perf-fast-audit-20261027](perf-fast-audit-20261027.md)
- [perf-heavy-files](perf-heavy-files.md)
- [perf-heavy-first-threshold](perf-heavy-first-threshold.md)
- [perf-next-heavy-files](perf-next-heavy-files.md)

### Front end, filesystem and printing

- [perf-front-end-fixed-costs](perf-front-end-fixed-costs.md)
- [perf-frontend-64](perf-frontend-64.md)
- [perf-jsdoc-parser](perf-jsdoc-parser.md)
- [perf-parse](perf-parse.md)
- [perf-parse-heap-faults](perf-parse-heap-faults.md)
- [perf-printer-output](perf-printer-output.md)
- [perf-reparse-cloner](perf-reparse-cloner.md)
- [perf-resolved-module-lookup](perf-resolved-module-lookup.md)
- [perf-resolver-allocations](perf-resolver-allocations.md)
- [perf-scanner-arena-builder](perf-scanner-arena-builder.md)
- [perf-scanner-numeric-identity](perf-scanner-numeric-identity.md)
- [perf-serial-collect](perf-serial-collect.md)
- [perf-serial-load](perf-serial-load.md)
- [perf-serial-steps](perf-serial-steps.md)
- [perf-vfs-lightweight-metadata](perf-vfs-lightweight-metadata.md)
- [perf-vfs-probes](perf-vfs-probes.md)
- [speed-frontend](speed-frontend.md)

### Incremental builds, emit, watch and WebAssembly

- [contentmappers](contentmappers.md)
- [emit-checker-thread-fix](emit-checker-thread-fix.md)
- [emit-incremental](emit-incremental.md)
- [mem-build-free-programs](mem-build-free-programs.md)
- [mem-emit-regions](mem-emit-regions.md)
- [mem-lazy-dts-build](mem-lazy-dts-build.md)
- [perf-dev-loop](perf-dev-loop.md)
- [perf-dev-loop2](perf-dev-loop2.md)
- [perf-emit](perf-emit.md)
- [perf-hub-edit-shortcut](perf-hub-edit-shortcut.md)
- [perf-incremental-parallel](perf-incremental-parallel.md)
- [wasm-build](wasm-build.md)
- [watch-macos](watch-macos.md)
- [watch-memory](watch-memory.md)

### Language server

- [lsp-api](lsp-api.md)
- [lsp-fsgen](lsp-fsgen.md)
- [lsp-lsfound](lsp-lsfound.md)
- [lsp-mem](lsp-mem.md)
- [lsp-memfix](lsp-memfix.md)
- [lsp-project](lsp-project.md)
- [lsp-proto](lsp-proto.md)
- [lsp-robust](lsp-robust.md)
- [lsp-server](lsp-server.md)

### Memory representation and allocation

- [mem-compact-ast-sizing](mem-compact-ast-sizing.md)
- [mem-compact-symbol-header](mem-compact-symbol-header.md)
- [mem-compact-symbol-header-results](mem-compact-symbol-header-results.md)
- [mem-dense-link-tables](mem-dense-link-tables.md)
- [mem-export-star-scratch](mem-export-star-scratch.md)
- [mem-flat-symbol-tables](mem-flat-symbol-tables.md)
- [mem-flow-compaction](mem-flow-compaction.md)
- [mem-frontend](mem-frontend.md)
- [mem-layout3](mem-layout3.md)
- [mem-lazy](mem-lazy.md)
- [mem-lazy-dts-members](mem-lazy-dts-members.md)
- [mem-pointer-compression](mem-pointer-compression.md)
- [mem-signature-links](mem-signature-links.md)
- [mem-small](mem-small.md)

### Memory lifetime, sharing and profiling

- [bun-check-memory](bun-check-memory.md)
- [design-persisted-frontend](design-persisted-frontend.md)
- [design-shared-type-layer](design-shared-type-layer.md)
- [mem-64](mem-64.md)
- [mem-census](mem-census.md)
- [mem-checker-gc](mem-checker-gc.md)
- [mem-checker-heap](mem-checker-heap.md)
- [mem-checker-scratch](mem-checker-scratch.md)
- [mem-free-leaf-files](mem-free-leaf-files.md)
- [mem-leaf-regions-32](mem-leaf-regions-32.md)
- [mem-leaf-regions-cost](mem-leaf-regions-cost.md)
- [mem-linux-residency-32](mem-linux-residency-32.md)
- [mem-never-read-apps](mem-never-read-apps.md)
- [mem-no-thp](mem-no-thp.md)
- [mem-overload-rollback](mem-overload-rollback.md)
- [mem-per-checker-duplication](mem-per-checker-duplication.md)
- [mem-recycle](mem-recycle.md)
- [mem-recycle-checkers](mem-recycle-checkers.md)
- [mem-scoped-arenas](mem-scoped-arenas.md)
- [mem-shared-base](mem-shared-base.md)
- [mem-use-census](mem-use-census.md)
- [perf-checker-processes](perf-checker-processes.md)
- [perf-memory-traffic-32](perf-memory-traffic-32.md)
- [perf-shared-checker](perf-shared-checker.md)
- [spike-shared-graph](spike-shared-graph.md)

### Performance rounds, builds and tooling

- [depot-right-sizing](depot-right-sizing.md)
- [dev-loop-build](dev-loop-build.md)
- [lint-paydown-compiler](lint-paydown-compiler.md)
- [linux-perf](linux-perf.md)
- [linux-x86-round](linux-x86-round.md)
- [mem-round2](mem-round2.md)
- [mem-round3](mem-round3.md)
- [mem-round4](mem-round4.md)
- [monomorphization-audit](monomorphization-audit.md)
- [perf-binary-layout](perf-binary-layout.md)
- [perf-build-identity](perf-build-identity.md)
- [perf-build-level](perf-build-level.md)
- [perf-excalidraw-typefest](perf-excalidraw-typefest.md)
- [perf-icount-hillclimb](perf-icount-hillclimb.md)
- [perf-icount-round4](perf-icount-round4.md)
- [perf-mimalloc-safe](perf-mimalloc-safe.md)
- [perf-pgo](perf-pgo.md)
- [perf-probes-2026-10-05](perf-probes-2026-10-05.md)
- [perf-round2](perf-round2.md)
- [perf-round2-followups](perf-round2-followups.md)
- [perf-round3](perf-round3.md)
- [unsafe-audit](unsafe-audit.md)

### Rust helpers and collections

- [data-structure-followups](data-structure-followups.md)
- [data-structure-redesigns](data-structure-redesigns.md)
- [data-structure-review](data-structure-review.md)
- [perf-iterator-regressions](perf-iterator-regressions.md)
- [perf-lookup-insert](perf-lookup-insert.md)
- [perf-oxc-numbers](perf-oxc-numbers.md)
- [perf-unsigned-positions](perf-unsigned-positions.md)
- [rust-base64](rust-base64.md)
- [rust-decimal-parsing](rust-decimal-parsing.md)
- [rust-stdlib-bigint](rust-stdlib-bigint.md)
- [rust-stdlib-helpers](rust-stdlib-helpers.md)
- [rust-stdlib-normalization](rust-stdlib-normalization.md)
- [rust-stdlib-slices](rust-stdlib-slices.md)
- [rust-string-helpers](rust-string-helpers.md)
- [shared-go-compat](shared-go-compat.md)

### Supporting data

- [Signature-link measurements](data/signature-links.json), supporting data for `mem-signature-links.md`.
