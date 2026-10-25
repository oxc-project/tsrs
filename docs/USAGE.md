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
commands such as `--init` and `--showConfig`, and [differences from pinned TypeScript](DEFAULTS.md) for detailed
comparisons.

`tsrs --watch -p .` and `tsrs -b --watch` rebuild when files change, like `tsc`. On Linux a rebuild starts when the
editor closes the saved file, so it never checks a half-written file and adds no fixed delay. macOS (FSEvents) reports
no close, so there a rebuild starts when the save's event arrives (about 13 ms after it), and waits while the file is
still empty (a save that truncated it and has not written yet) or while a tool is writing several files (a formatter,
a checkout). A program that writes one file in several pieces with pauses between them can still start two rebuilds;
the second has the right result. When FSEvents is unavailable the watch uses kqueue, which needs a file descriptor
per watched file; tsrs raises its descriptor limit as far as the system allows and says so if that is not enough.

## Performance options

The defaults and fast preset described here are included in the **0.19.0 beta candidate**, available in source builds
until publication. The default uses pinned TypeScript's four checkers, file assignment and checker history, with
eager members. Internal optimizations that preserve compiler behavior, including control-flow memoization and
parallel parsing, remain enabled in both modes.

TypeScript itself can report different diagnostics when checker count or file order changes. Default mode
preserves this upstream behavior; see the [known cases](../notes/open-history-dependence.md).

`--fast` or `TSRS_FAST=1` enables the fast preset for compilation, watch and `--build`:

| Setting | Default | Fast preset |
| --- | --- | --- |
| Checker count | 4, capped by file count | Adapts to CPU and project size, up to 32; `--build` keeps 4 per project |
| Scheduling | Go's file assignment and visit order | Directory locality, work stealing, and file splitting/module affinity when eligible |
| Checker history | Go's behavior | Canonical handling of selected history-dependent cases |
| Additional checker caches | Union-front, indexed-access and inference-walk caches off | Those three caches on |
| Type members | Eager | Lazy resolution and instantiation, including tuples, conditional/inference mappers and property-cache optimizations |
| Declaration parsing | Eager | Lazy `.d.ts` member parsing in batch/incremental compilation and non-watch builds when declaration checking is skipped |
| Finished leaf files | Retained | Freed on eligible non-incremental, non-watch `--noEmit` runs |

The preset keeps compiler options such as `strict`, `skipLibCheck` and `noCheck` as configured. Lazy declaration
parsing requires `skipLibCheck` or `noCheck`; in build mode every built project must qualify. Memory targets and
persistent cost caches require their own size or path. Experimental shared type graphs and the
`TSRS_LAZY_HAS_PROP` optimization remain off; see [internal switches](DEBUGGING.md#fast-mode-and-reference-defaults).

Leaf freeing requires a Unix build with compressed pointers, canonical history, and a non-incremental,
non-watch `--noEmit` run without declaration output or `--explainFiles`. By default, the checker upper bound
computed before files are loaded must be at most 16; reducing the count later for a small program does not
enable it. `TSRS_FREE_LEAVES=1` bypasses that count limit only. See the
[full eligibility rules](DEBUGGING.md#freeing-checked-leaf-files-tsrs_free_leaves).

### Side effects

- TypeScript's own checker-count and file-order sensitivity also applies to tsrs. Fast mode additionally changes
  scheduling, checker history and lazy instantiation. Known check-order, recursive-type and instantiation-limit
  cases can add or remove diagnostics, change their locations or message text, and change the exit status;
  see [known cases](../notes/open-history-dependence.md) and the
  [compatibility table](COMPATIBILITY.md#what-it-does-and-doesnt-do).
- Fast mode reduces memory use through lazy type members, lazy declaration parsing and freeing finished leaf
  files where eligible. Additional checker threads and caches can offset those savings; the overall speed and
  peak-memory impact depends on the workload.
- Type, symbol and instantiation counters reflect different work. Work stealing can make counters vary between
  runs even when diagnostics match.
- Changing checker history or lazy-member settings rejects incompatible `.tsbuildinfo` caches. Switching the
  preset on or off can therefore require a fresh check on the next incremental compilation or build.

### Precedence and scope

Only `TSRS_FAST=1` enables the environment preset; unset, `0` and other values leave it off. An explicit `--fast`
value overrides the environment. The flag accepts `true` or `false` and works in response files; it is a CLI
option, not a `tsconfig.json` compiler option.

```sh
TSRS_FAST=1 pnpm exec tsrs -p .                 # enable the preset
TSRS_FAST=1 pnpm exec tsrs -p . --fast false    # disable the environment preset
pnpm exec tsrs -p . --fast --noLazyMembers     # fast preset with eager type members
```

Individual feature flags and their environment overrides take precedence over the preset, regardless of where
`--fast` appears. `--fast false` disables the preset; it does not clear explicit settings such as `--lazyMembers`,
`--checkerAssignment locality` or `TSRS_LAZY_DTS=1`. For a comparison with the reference defaults, leave those overrides unset
and use `--fast false`. Match any explicit `--checkers` or `--singleThreaded` option on the reference compiler too.

### Individual controls

Alongside the supported `tsc` flags, tsrs offers these controls:

- `--checkers N` sets the number of checker threads (default 4). In fast mode, tsrs uses one per core up to 16 cores,
  then the greater of 16 and half the cores, capped at 32; a program with fewer than 32 type-checked files per
  checker gets fewer, down to 4, and never more checkers than files. `--build` defaults to 4 per project in both
  modes. Go's default history can produce different diagnostics at different checker counts.
- `--singleThreaded` uses one thread and avoids duplicating state across checkers.
- `--fast --checkerCostCache <file>` remembers how long each file took to check and uses that to balance threads on the next
  run (up to ~15% faster on repeated runs).
- `--fast --maxMemory <size>` (for example `12G`; `--noEmit` checks only) keeps the process near that much memory: above it,
  tsrs replaces its largest checker with a fresh one, trading CPU for memory. The diagnostics are the same.
- `--checkerAssignment go` selects the default tsgo file assignment and checker history; `locality` opts into static
  locality assignment and canonical history. Explicit assignments disable work stealing.
- `--lazyMembers` opts into lazy checker optimizations alone; `--noLazyMembers` disables them even in fast mode.
  `TSRS_LAZY_MEMBERS=1` / `0` provides the environment equivalent; the explicit CLI flags win.
- `TSRS_LAZY_DTS=1` enables lazy declaration parsing in eligible modes; `TSRS_LAZY_DTS=0` disables it even in fast
  mode. This is separate from lazy checker members.

`--fast`, `--checkers` and `--singleThreaded` work with `--build`. The other tsrs-only flags listed above are
rejected with `--build`; use the supported environment overrides where applicable.

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
