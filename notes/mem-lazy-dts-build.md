# Lazy declaration members in incremental and project builds (#722)

The original [lazy declaration member experiment](mem-lazy-dts-members.md) left incremental and build mode eager
because their readers had not been audited or measured. [Issue #722](https://github.com/oxc-project/tsrs/issues/722)
supplies a public reproduction and a proposed extension. The reader audit, CLI regressions and release measurements
below justify enabling those two modes.

## Gate and lifetime

`perform_incremental_compilation` uses the same `skipLibCheck` / `noCheck` gate as batch compilation. The incremental
builder uses a declaration file's version as its shape signature (`update_shape_signature_with`), reads imports,
augmentations and symbol declarations through their usual accessors (`checker_references_of`), and inspects
statements for global effects (`file_affects_global_scope`). Its recursive `contains_interface_declaration` walk
forces any lists it visits through `NodeList::nodes`.

A non-watch CLI build enables lazy parsing only when every project in its selected build order skips declaration
checking. Solution configs (no files, with project references) create no program and do not restrict the gate. This
must be decided before building the first program: `host::get_source_file` shares declaration trees across projects,
and lazily bound member lists discard bind diagnostics. Enabling the mode separately per project could therefore
lose diagnostics in a later project that checks the same declarations.

Shared declaration files are parsed and bound in file regions. `LazyNodeList::force` allocates forced members in
`enter_thread_arena`, whose arena is never freed, so freeing an earlier project's program cannot invalidate them.
Watch, API and command-line harness runs remain eager. `TSRS_LAZY_DTS=0` / `off` and the shared-object check retain
their existing opt-outs.

## Measurements

Apple M4 Pro, macOS arm64, 12 available CPUs, release profile (fat LTO, no PGO), five interleaved before/after pairs
with pair order alternated. Baseline: revision `485e9965`, with existing local changes held identical in both builds.
The public reproduction has two composite projects importing `stripe@22.6.2` and `googleapis@188.0.0`, as in the issue.
Install with `npm install --ignore-scripts --no-audit --no-fund`. Remove each project's `dist` and
`tsconfig.tsbuildinfo` before every run. Measure with `/usr/bin/time -l`; use `--singleThreaded` for instruction
counts, and no checker flags for default peak RSS. All commands also use `--pretty false` and exit 0.

| Command | Single-threaded instructions, G: before → after | Default peak RSS, MiB: before → after | RSS change |
| --- | ---: | ---: | ---: |
| `-p a --noEmit --composite false --incremental false` (control) | 4.337 → 4.337 | 305.4 → 305.6 | +0.1% |
| `-p a` (composite / incremental) | 5.800 → 4.689 | 502.1 → 308.1 | -38.6% |
| `-b a` | 5.931 → 4.811 | 592.9 → 399.1 | -32.7% |
| `-b` (solution) | 6.256 → 5.139 | 606.8 → 406.0 | -33.1% |

Single-threaded peak RSS also drops: 490.7 → 291.6 MiB for incremental, 518.8 → 320.0 MiB for `-b a`, and
523.6 → 325.1 MiB for the solution. These are measurements of the public reproduction, not the published benchmark
projects or a Linux wall-time claim.

## Validation

- `crates/tsrs_cli/tests/lazy_dts_build.rs` compares diagnostics, emitted files and build info with lazy parsing on
  and off over clean, no-op and declaration-shape-edit builds, for both `skipLibCheck` and `noCheck`, incremental
  compilation, sequential builders and default builders. Arena poisoning is enabled. A mixed build checks that
  duplicate parameter bind diagnostics survive when a later project checks a declaration shared with an earlier
  unchecked project.
- On the public reproduction, diagnostics, exit statuses, JavaScript, declarations and build info are byte-identical
  before/after and with lazy parsing disabled, in canonical and Go histories. The Go-history results also match
  pinned TypeScript `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`. Sequences with and without two type errors cover
  clean build, no-op build, fixing b, fixing a, adding an exported interface extending `Stripe.Customer`, and `-p a`.
- After rebasing onto main `69c9ac78`, complete conformance passes in Go and canonical histories: 13,700 diagnostic,
  12,998 type, 12,998 symbol, 13,626 JavaScript/declaration, 149 JavaScript-map and 156 source-map-record baselines;
  no failures, crashes or timeouts. All 47 regression projects and 10 CLI tests (lazy declarations, build program
  freeing and default emit), Clippy over all targets, source lint and formatting pass.
