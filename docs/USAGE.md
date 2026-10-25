# Using tsrs

Install tsrs in your project, then run it in place of `tsc`:

```sh
pnpm add -D tsrs
pnpm exec tsrs -p path/to/project           # compile
pnpm exec tsrs -p path/to/project --noEmit  # type check only
pnpm exec tsrs -p path/to/project --fast    # opt into tsrs performance modes
pnpm exec tsrs -p . --extendedDiagnostics   # counters and timings
pnpm exec tsrs --version
```

You can also run it without installing it in the project: `pnpm dlx tsrs -p .`.
Your existing `tsconfig.json` controls compilation. tsrs emits JavaScript and declarations according to those
options by default, just like `tsc`; pass `--noEmit` when you only want type checking.

See [TypeScript compatibility](COMPATIBILITY.md) for supported features and known gaps, including unsupported
commands such as `--init` and `--showConfig`, and [differences from pinned TypeScript](DEFAULTS.md) for changed
defaults and detailed comparisons.

`tsrs --watch -p .` and `tsrs -b --watch` rebuild when files change, like `tsc`. On Linux a rebuild starts when the
editor closes the saved file, so it never checks a half-written file and adds no fixed delay. macOS (FSEvents) reports
no close, so there a rebuild starts when the save's event arrives (about 13 ms after it), and waits while the file is
still empty (a save that truncated it and has not written yet) or while a tool is writing several files (a formatter,
a checkout). A program that writes one file in several pieces with pauses between them can still start two rebuilds;
the second has the right result. When FSEvents is unavailable the watch uses kqueue, which needs a file descriptor
per watched file; tsrs raises its descriptor limit as far as the system allows and says so if that is not enough.

## Performance options

The default uses pinned TypeScript's four checkers, file assignment and checker history, with eager members.
`--fast` opts into tsrs's performance modes for compilation, watch and `--build`. It enables adaptive checker
counts, locality scheduling with work stealing, canonical checker history and caches, lazy members, and eligible
lazy declaration parsing and leaf-file freeing. Checking settings such as `skipLibCheck` and `noCheck` stay as
configured. Known history and instantiation-depth cases can produce different diagnostics; see the
[compatibility table](COMPATIBILITY.md#what-it-does-and-doesnt-do).

`TSRS_FAST=1` enables the same preset through the environment; unset or `0` keeps it off. An explicit `--fast`
value overrides `TSRS_FAST`, so `TSRS_FAST=1 tsrs --fast false` uses the default preset.
The flag works in response files. Explicit feature flags and their environment
overrides take precedence regardless of where `--fast` appears. Memory targets and persistent cost caches still
require their own size or path. Internal optimizations that preserve compiler behavior remain enabled in both modes.

Alongside the supported `tsc` flags, tsrs offers these controls:

- `--checkers N` sets the number of checker threads (default 4). With `--fast`, tsrs uses one per core up to 16 cores, then the greater
  of 16 and half the cores, capped at 32; a program with fewer than 32 type-checked files per checker gets fewer,
  down to 4, and never more checkers than files. `--build` uses 4 per project. More threads are faster but use more
  memory. Go's default history can produce different diagnostics at different checker counts.
- `--singleThreaded` uses one thread, for the lowest memory use.
- `--fast --checkerCostCache <file>` remembers how long each file took to check and uses that to balance threads on the next
  run (up to ~15% faster on repeated runs).
- `--fast --maxMemory <size>` (for example `12G`; `--noEmit` checks only) keeps the process near that much memory: above it,
  tsrs replaces its largest checker with a fresh one, trading CPU for memory. The diagnostics are the same.
- `--checkerAssignment go` selects the default tsgo file assignment and checker history; `locality` opts into static
  locality assignment and canonical history. Explicit assignments disable work stealing.
- `--lazyMembers` opts into lazy checker optimizations alone; `--noLazyMembers` disables them even with `--fast`.
  They can affect diagnostics near an instantiation depth
  limit; see the [known exceptions](../notes/open-history-dependence.md) and [patches](../upstream/).

## Native packages

The launcher installs the appropriate optional `@tsrs/<os>-<arch>[-<libc>]` dependency for your platform.
See the [compatibility table](COMPATIBILITY.md#what-it-does-and-doesnt-do) for released platform support.
Set `TSRS_BINARY=/path/to/tsrs` to run a locally built binary through the same launcher.

The package version includes the TypeScript version it ports: `0.19.0-ts7.1.0-beta` means tsrs 0.19.0 following
TypeScript `7.1.0-beta`. For a source checkout, `Cargo.toml` records the exact TypeScript commit. Follow
[the reference compiler workflow](../CONTRIBUTING.md#reference-compiler) when reproducing bugs or checking fixes.

## Other ways to use tsrs

- [Editor setup](LSP.md#editor-setup): run the language server with `tsrs --lsp -stdio`.
- [JavaScript API](NODE_API.md#javascript-api): TypeScript 7's synchronous and asynchronous APIs.
- [WebAssembly](../npm/tsrs-wasm/README.md): single-threaded compilation in Node and browsers.
- [Build from source](../CONTRIBUTING.md#build): native and WebAssembly builds and the development workflow.
