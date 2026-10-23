# Differences from pinned TypeScript

This page records known differences from the Go implementation of TypeScript at the commit pinned in
[`Cargo.toml`](../Cargo.toml), currently [`3e9236361fa9`](https://github.com/microsoft/TypeScript/tree/3e9236361fa97a9e6c685f4833ac457ae2d8c50a/tsc). It covers changed defaults, unsupported features and
remaining parity gaps. These are comparisons with that pinned compiler, not with TypeScript 5/6 or a newer nightly.
The [compatibility table](COMPATIBILITY.md#what-it-does-and-doesnt-do) summarizes support and conformance.

Go paths are relative to `tsc/internal/` in a microsoft/TypeScript checkout, Rust paths to `crates/`.

## Shared defaults

The compiler, build, watch and type-acquisition option declarations match the reference, as do the named enum
maps and the 116 bundled library files. An option being declared does not mean its feature is implemented;
unsupported commands and API behavior are listed below.

The computed compiler defaults follow `core/compileroptions.go`: target, module kind and resolution, strictness,
JavaScript checking, class fields and declaration emit. CLI builds default to four builders, or one with
`--singleThreaded`. Library selection follows the target. Ordinary language-server formatting defaults remain
four spaces and `"\n"`; the limits below apply to explicit client settings.

## Command line

| setting | tsgo | tsrs |
| --- | --- | --- |
| Checker count without `--checkers` | 4 | Every available core up to 16; above that, the greater of 16 and half the cores (rounded down), capped at 32. Small programs reduce this to at most one checker per 32 checked files, with a floor of 4. `--build` uses 4 per project. Both compilers finally clamp the count to 1–256 and no more than the file count |
| How files are split across checkers | FENNEL association | Directory-subtree groups with work stealing, and file splitting and module affinity when eligible. `--checkerAssignment go` or `TSRS_CHECKER_ASSIGNMENT=go` restores tsgo's assignment algorithm; match `--checkers` too when comparing output |
| Checker history | tsgo's: which file reports some diagnostics depends on check order | Canonical. Also turns on tsrs-only checker caches (inference, union, indexed access and flow memos). `go` assignment restores tsgo's history ([details](../notes/fix-history-dependent-diagnostics.md)) |
| Lazy checker members | Eager member instantiation | On. `--noLazyMembers` or `TSRS_LAZY_MEMBERS=0` disables it. Reduced instantiation can change diagnostics near a depth limit ([known exceptions](../notes/open-history-dependence.md)) |
| Additional memory-saving modes | No corresponding tsrs switches | Lazy parsing of `.d.ts` members on eligible non-incremental checks with `skipLibCheck` or `noCheck`, and freeing finished leaf files on eligible `--noEmit` runs. `--maxMemory` is off by default |
| Types, Symbols and Instantiations counters | Counts the work of its checkers | Counts tsrs's work; checker count, assignment, caches and lazy members can all change the values. Matching `--checkers` alone does not make these counters identical |
| Parallel parsing | Goroutines scheduled by the Go runtime | A thread pool of `min(available parallelism, 32)`, overridden by a positive `RAYON_NUM_THREADS`; `--singleThreaded` disables parallel parsing |
| Incremental startup | Reads tsbuildinfo, then builds the program | Both at once on two threads, unless `--singleThreaded` |
| tsbuildinfo contents | tsgo's fields | Adds `tsrsCanonicalCheckSignature`. In `go` history mode or API mode, tsrs won't reuse a tsbuildinfo that has this field |
| `--version` | `Version <TypeScript version>` | Appends `(tsrs <release>, microsoft/TypeScript@<commit>)`. Unstamped source builds use `7.1.0-beta`; npm builds use the pinned package version on both sides |
| `--init` | Writes a tsconfig.json | `error: --init is not supported by tsrs.`, exit status 5, no file written |
| `--showConfig` | Prints the resolved config as JSON | Not supported, exit status 5 |
| `--help`, `--all`, `-b --help` | Full option help | A version line and short usage, exit status 0 |
| No tsconfig.json and no files | Version and full help, exit status 1 | Version and one usage line, exit status 1 |
| `--locale` | Translates messages | Accepted; messages stay in English |
| `--locale` validation | Checks the tag against the language registry: `--locale zz` gives TS6048 and exit status 1 | Checks only that the tag is well formed: `--locale zz` is accepted |
| `--generateTrace` | Writes a trace | Warns `Failed to start tracing` and runs without it |
| `--pprofDir` | Writes CPU and memory profiles | Parsed and ignored. `--lsp -pprofDir` prints the "will be written" line but writes nothing |
| `Memory used` in `--diagnostics` | Live heap after two garbage collections | Process resident set size |
| `Memory allocs` in `--diagnostics` | Number of allocations | Always 0 |
| `--diagnostics`, `--extendedDiagnostics` output | Compiler statistics | Adds subphase timings, program-load counts and lazy-member counters. Filesystem-call and lazy-list counters have been removed |
| Uncaught panic during CLI compilation | Exit status 2 with a Go stack trace | The compilation thread's panic maps to exit status 5; unflushed output can be lost. Panics before that thread starts, in LSP or in API mode have different handling |
| Stdout | Unbuffered | Buffered, flushed at exit and on watch status lines, so stdout and stderr can interleave differently |
| Ctrl-C, SIGTERM (Unix) | Caught; a non-incremental run finishes | Default action: the process stops at once. `--lsp` and `--api` catch them as tsgo does |
| Stack size | Growing goroutine stacks; `TS_GO_DEBUG_STACK_LIMIT` overrides the runtime maximum | Fixed stack reservations: 512 MiB for compilation, checker and API threads, 256 MiB for parse workers; no equivalent override |
| Concurrent filesystem calls | Separate limits of 128 blocking calls, 128 reads and 32 writes | No separate filesystem semaphore; worker concurrency still limits the calls in flight |
| Default lib location | Embedded source builds; npm builds require libs beside the executable | The same build-time choice since 0.17.0 (`noembed` for npm), plus the `TSRS_LIB_PATH` override. npm binaries have no embedded fallback |
| Case sensitivity detection | At startup | On first use (same test) |
| Realpath on Linux | `O_PATH` and `/proc/self/fd`, falling back to walking symlinks | Rust's `std::fs::canonicalize` |
| Memory management | Go garbage collection; honors Go runtime environment settings | Arenas and mimalloc. On Linux, huge pages are enabled for eligible arena allocations, not mimalloc's heap by default. Go runtime variables have no effect |
| Arguments that aren't UTF-8 (Unix) | Accepted as byte strings | Panic while collecting arguments, before the compilation thread starts (normally exit status 101) |
| tsrs-only flags with `-b` | — | The tsrs-only flags (`--checkerAssignment`, `--checkerCostCache`, `--maxMemory`, `--lazyMembers`, `--noLazyMembers`) are rejected with TS5072 |

The tsrs-only flags are described in [Performance options](USAGE.md#performance-options). Sources:
[`checkerpool.rs`](../crates/tsrs_compiler/src/checkerpool.rs),
[`execute.rs`](../crates/tsrs_execute/src/execute.rs),
[`statistics.rs`](../crates/tsrs_execute/src/tsc/statistics.rs),
[`buildinfo.rs`](../crates/tsrs_incremental/src/buildinfo.rs) and
[`bundled.rs`](../crates/tsrs_vfs/src/bundled/bundled.rs).

## Watch mode

| setting | tsgo | tsrs |
| --- | --- | --- |
| Linux backend | fanotify when the kernel and filesystem support it, otherwise inotify | inotify (fanotify isn't ported) |
| Windows | Native backend | Not ported: `--watch` exits with "not supported" |
| When a change starts a rebuild | First event after a quiet period | CLI watch waits for the writer to close the file (inotify), or while the file is empty (FSEvents/kqueue), for up to 1000 ms without writes or 2000 ms after the first write |
| FSEvents batch touching several files | Fires at once | Waits for 50 ms of quiet |
| FSEvents streams | A stream can watch multiple roots | Gives up to 256 roots their own streams; additional roots share streams. Uses a `USER_INITIATED` dispatch queue |
| When to watch a parent recursively | At least 10 watches under the parent | At least 10 distinct children of the parent with watches |
| kqueue descriptor limit | Raised by the Go runtime | Raised by tsrs, which reports when it isn't enough |
| Events that start a rebuild | Every relevant event | Skips the build's own outputs and files whose modification time didn't change; rescans files changed during the first build; watches directories reached through symlinks; rebuilds on case-only renames |
| Parsed files in `-b --watch` | Reparsed every cycle | Reused when the text is unchanged |
| Backend override | none | `TSRS_WATCH_BACKEND=kqueue` uses the macOS fallback |

Write settling is enabled for `--watch` and `-b --watch`, not the language server. The backends and consolidation
rules are shared, so other watcher differences can also affect the language server. Sources:
[`tsrs_fswatch`](../crates/tsrs_fswatch/src), [`watcher.rs`](../crates/tsrs_execute/src/watcher.rs) and
[`watchmanager`](../crates/tsrs_execute/src/watchmanager/mod.rs).

## Language server

| setting | tsgo | tsrs |
| --- | --- | --- |
| Automatic type acquisition | On by default for `--lsp` (the CLI passes a typings location and npm) | Not ported |
| Telemetry | Performance, project and request-failure telemetry when enabled | Request-failure events, including flaky diagnostics; performance and project telemetry are not ported |
| `trackFlakyDiagnostics` check | Runs a full emit (writing nothing) between the two diagnostic passes | Runs the passes back to back; does not exercise emit-induced changes |
| Indent, tab and base indent size | No limit | Capped at 100 |
| Integer preferences (`tabSize`, `indentSize`, `baseIndentSize`, `maximumHoverLength`) | Go `int` (64-bit on 64-bit builds) | 32-bit storage; values within the 64-bit range can wrap when narrowed. Formatting `tabSize` is also narrowed to 32 bits |
| `locale` from the client | Parses BCP 47 tags and uses available translations | Accepts any non-empty string; messages stay in English |
| `GOMEMLIMIT` (from `js/ts.server.goMemLimit`) | Memory limit | No effect |
| Async request threads | A goroutine per request, scheduled by `GOMAXPROCS` | Starts workers as needed and retains up to `max(4, available parallelism)` idle workers. New work is limited by available parallelism; returning from a blocking wait can temporarily exceed it |
| Background queue | Unbounded goroutines | 6 workers |
| After the idle cache clean | `runtime.GC()` | No additional collection step |
| Profiling commands, `runGC` | Supported | `MethodNotFound` |
| Content mappers | Supported | Not ported; `runExternalCode` does nothing |
| Parent process watchdog on Windows | Supported | Not ported |

[Language server](LSP.md) has the threading model and other known gaps. Sources:
[`server.rs`](../crates/tsrs_lsp/src/server.rs), [`workerpool.rs`](../crates/tsrs_lsp/src/workerpool.rs),
[`runslots.rs`](../crates/tsrs_core/src/runslots.rs),
[`userpreferences.rs`](../crates/tsrs_ls/src/lsutil/userpreferences.rs) and
[`format/api.rs`](../crates/tsrs_ls/src/format/api.rs).

## API

| setting | tsgo | tsrs |
| --- | --- | --- |
| API build orchestrator concurrency | Requested builder/checker settings | Forces `singleThreaded`, so building, parsing and checking run serially |
| Content mappers | A spawner is passed to the session | None; `--runExternalCode` does nothing |
| Callback re-entry that waits for a resource held by its caller | Can deadlock | Rejected immediately in sync mode; after a 10 s grace period in async mode. A slow async callback can also cause an unrelated waiter to be rejected |
| `-cwd=""` | Panics | Uses the current directory |
| `-useCaseSensitiveFileNames` after a non-flag argument | Ignored; the OS default remains | The parser stops, but a separate argument scan notices the flag and can replace the OS default with `false` |
| Unknown `-callbacks` name | Panics with a stack trace, exit status 2 | Error message, exit status 2 |
| Windows named pipes | Supported | Not ported |

[Node API](NODE_API.md) has the rest. Sources: [`api.rs`](../crates/tsrs_cli/src/api.rs) and
[`reentrancy.rs`](../crates/tsrs_api_transport/src/reentrancy.rs).

## Option parsing edge cases

- **`moduleDetection` number with no name, sent through the API.** tsgo keeps the raw value, which the parser treats
  like legacy detection. tsrs preserves the raw number for serialization but leaves the typed field unset and
  computes the default (`force` for `node16` through `nodenext`, otherwise `auto`), so files can become modules where
  tsgo leaves them scripts. This is a parity gap, not an intentional compiler default.
- **Enum option given as a number with no name** (reachable through tsbuildinfo options). tsgo converts any number to
  the enum (`"module": 42` gives `ModuleKind(42)`). tsrs looks the number up in the enum map and falls back to unset.
- **`watchInterval`** uses Go `int` (64-bit on 64-bit builds) and 32-bit storage in tsrs. Neither side implements
  polling with this option.
- **`builders` from the API below 1** queues nothing in tsgo; tsrs ignores it because API mode always uses 1.
- **Content mappers:** with `contentMappers`, tsrs leaves a mapped file's own `.d<ext>.ts` output out of wildcard
  includes when the mapped source is also included. tsgo includes it, which can give TS5055 on a later emit.

The numeric tsbuildinfo path is separate from API JSON decoding: API `target`, `module`, `jsx` and
`moduleResolution` retain arbitrary int32 values in both compilers. Sources:
[`gojson.rs`](../crates/tsrs_tsoptions/src/gojson.rs),
[`parsinghelpers.rs`](../crates/tsrs_tsoptions/src/parsinghelpers.rs),
[`compileroptions.rs`](../crates/tsrs_core/src/compileroptions.rs) and
[`tsconfigparsing.rs`](../crates/tsrs_tsoptions/src/tsconfigparsing.rs).

## Environment variables

tsrs reads the same `FORCE_COLOR`, `NO_COLOR`, `TERM`, `TS_WATCH_DEBUG` and `TS_CONTENT_MAPPER_DEBUG` as tsgo, and
`HOME` and `XDG_CACHE_HOME` for the typings cache location.

tsgo variables tsrs ignores:

- `TS_GO_DEBUG_STACK_LIMIT` (stack sizes are fixed).
- `OS`, `WT_SESSION`, `TERM_PROGRAM`, `COLORTERM` (tsgo uses them only to color `--help`).
- `TERMUX_EXEC__PROC_SELF_EXE` (Android).
- The Go runtime's `GOGC`, `GOMEMLIMIT`, `GOMAXPROCS`, `GODEBUG` and `GOTRACEBACK`.

tsrs-only variables that change behavior:

| variable | default | effect |
| --- | --- | --- |
| `TSRS_CHECKER_ASSIGNMENT` | `locality` | `go` uses tsgo's file split and checker history |
| `TSRS_MAX_MEMORY` | off | Same as `--maxMemory` |
| `TSRS_LAZY_MEMBERS` | on | Same as `--lazyMembers` / `--noLazyMembers` |
| `TSRS_LIB_PATH` | unset | Directory to load the default libs from |
| `TSRS_WATCH_BACKEND` | unset | `kqueue` uses the macOS fallback watcher |
| `TSRS_BINARY` | unset | Path of the binary the npm launcher runs |
| `RAYON_NUM_THREADS` | `min(available parallelism, 32)` | Parse thread count |
| `MIMALLOC_*` | Build-dependent; mimalloc heap THP is off | Allocator settings; an explicit `MIMALLOC_ALLOW_THP` also controls whether tsrs restores process-wide THP for its arenas |

The remaining `TSRS_*` variables tune or report on checker internals; [debugging](DEBUGGING.md) describes them.

## npm launcher

| setting | tsgo (`typescript`) | tsrs (`tsrs`) |
| --- | --- | --- |
| Command | `tsc` | `tsrs` |
| Binary package | `@typescript/<os>-<arch>` | `@tsrs/<os>-<arch>[-gnu\|-musl]`, chosen by libc |
| Launch | `execve` if available, else `execFileSync` | `execve` if available, else `spawn` |
| Child killed by a signal in the fallback launcher | `execFileSync` throws when there is no exit status | Forwards signals to the child and re-raises the child's signal. With `execve`, both launchers are replaced by the compiler |
| Binary override | none | `TSRS_BINARY` |

Sources: pinned TypeScript's `packages/typescript/lib/tsc.js` and
[`tsrs.js`](../npm/tsrs/lib/tsrs.js).

## Verification

The detailed defaults review compared tsrs 0.18.0 (main `470b1bdb`) with TypeScript `b85298b6a81f` on 2026-10-20. The compiler, build
and watch declarations were regenerated into scratch files and compared; all 174 enum-map entries and all 113
library files matched. Probes with source-built binaries on macOS covered help, version, unsupported commands,
locale handling, statistics, incremental metadata, tracing, profiling, non-UTF-8 arguments and API startup errors. Direct Go/Rust
probes confirmed the unknown `moduleDetection` and tsbuildinfo enum conversions. Platform-specific watcher
behavior, runtime scheduling and the remaining server/API details were checked in source. The current 0.19.0
source pin is `3e9236361fa9`; its broader conformance evidence is in the
[recorded suite results](COMPATIBILITY.md#conformance), [source-upgrade log](../notes/typescript-623-ports.md),
and [beta follow-up log](../notes/typescript-7.1-beta-ports.md).
