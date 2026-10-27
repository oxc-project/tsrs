# `tsrs`

[![npm](https://img.shields.io/npm/v/tsrs)](https://www.npmjs.com/package/tsrs)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](https://github.com/oxc-project/tsrs/blob/main/LICENSE)

`tsrs` is a drop-in replacement for the TypeScript 7 compiler and language server, written in Rust.
Our goal is 100% conformance, with much lower memory use and faster type checking. [Read the announcement](https://blog.cloudflare.com/typescript-rust-port/).

## Get started

```sh
pnpm add -D tsrs
pnpm exec tsrs --noEmit
```

You can try `tsrs` today and give us feedback on performance, memory usage, and conformance.
Try checking and emitting code in the [browser playground](https://tsrs-playground.void.app).

## Conformance

We aim to release a stable version against TypeScript 7.1 with 100% conformance.
See [compatibility and evidence](https://github.com/oxc-project/tsrs/blob/main/docs/COMPATIBILITY.md)
for supported features and known gaps.

Default mode preserves the pinned TypeScript compiler's checker behavior, including known diagnostic differences
when checker count or file order changes. This sensitivity is inherited from TypeScript itself. See
[differences from pinned TypeScript](https://github.com/oxc-project/tsrs/blob/main/docs/DEFAULTS.md) for the remaining
runtime differences and unsupported features.

## Fast mode

The following defaults and opt-in preset are available in **0.20.0** and later.

By default, tsrs follows the pinned TypeScript compiler's checker settings: four checkers (capped by file count),
Go's file assignment and checker history, and eager type members. Enable the fast preset with `--fast` or
`TSRS_FAST=1`:

```sh
pnpm exec tsrs --noEmit --fast
# Or enable the same preset through the environment:
TSRS_FAST=1 pnpm exec tsrs --noEmit
```

With no individual feature overrides, fast mode enables:

- **Adaptive checker counts.** One checker per core up to 16 cores; above that, the greater of 16 and half the
  cores, rounded down. The count stays between 4 and 32 before file-count limits: small programs reduce it toward
  4 to allow at least 32 checked files per checker, and it never exceeds the total file count. `--build` keeps
  four checkers per project. `--checkers N` overrides the count; `--singleThreaded` uses one.
- **Locality scheduling and work stealing.** Groups nearby files on the same checker, starts heavy files first,
  and lets idle checkers take unstarted work. Heavy declaration files can be checked in pieces across checkers.
  Module affinity groups files that use the same modules, starting at 16 checkers, or 8 for programs with at
  least 24,000 checked files. An explicit `--checkerAssignment` / `TSRS_CHECKER_ASSIGNMENT` disables stealing
  and file splitting.
- **Canonical checker history and additional caches.** Changes selected history-dependent checks to reduce
  dependence on check order, and enables union-front, indexed-access and inference-walk caches.
- **Lazy type members.** Resolves and instantiates class/interface, mapped-type and tuple members as needed.
  Also defers conditional/inference mappers and property-cache allocation, and avoids unnecessary member
  instantiation during unmatched-property and empty-object checks.
- **Lazy declaration parsing.** Parses `.d.ts` member lists on first use when `skipLibCheck` or `noCheck` is
  already enabled. Applies to batch and incremental compilation, and non-watch `--build` when every built
  project skips declaration checking. Watch, language-server and API modes keep eager declaration parsing.
- **Freeing finished leaf files.** Releases trees and binder data for selected unreferenced TypeScript modules during
  non-incremental, non-watch `--noEmit` compilation, without declaration output or `--explainFiles`. Requires
  Unix with compressed pointers and a checker upper bound of at most 16, computed before files are loaded.
  It is disabled in `--build`.

The preset keeps `strict`, `skipLibCheck`, `noCheck` and other checking options as configured. Control-flow
memoization and parallel parsing are enabled in both modes. Memory targets and persistent checker-cost caches
still require an explicit size or path; experimental shared type graphs and `TSRS_LAZY_HAS_PROP` remain off.

Fast mode has these side effects:

- **Check order can affect diagnostics.** TypeScript itself has cases whose diagnostics depend on checker
  count or file order. Fast mode changes scheduling, checker history and lazy instantiation. In known check-order,
  recursive-type and instantiation-limit cases, errors can appear, disappear, move or have different message
  text, and the exit status can change. See the
  [known cases](https://github.com/oxc-project/tsrs/blob/main/notes/open-history-dependence.md).
- **CPU and memory use change.** Fast mode reduces memory use through lazy type members, lazy declaration
  parsing and freeing finished leaf files where eligible. Additional checkers and caches can offset those
  savings; the overall speed and peak-memory impact depends on the project.
- **Compiler statistics change.** Lazy member resolution and additional caches change the work performed, so
  type, symbol and instantiation counts can differ from TypeScript's. Work stealing can also make these counters
  vary between runs.
- **Switching modes can require a fresh check.** Incremental and build caches with incompatible checker-history
  or lazy-member settings are discarded, so the next build can take longer.

The chart below compares the 0.20.0 beta npm binaries, with and without `--fast`, against TypeScript 7.1 beta at
default settings; see [benchmark results](https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md) for the method.

<a href="https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md">
  <img alt="tsrs --fast, tsrs and TypeScript 7.1 beta: check time and peak memory by project" src="https://raw.githubusercontent.com/oxc-project/tsrs/main/bench/benchmark.svg">
</a>

## Extra features

[`@tsrs/wasm`](https://github.com/oxc-project/tsrs/blob/main/npm/tsrs-wasm/README.md) provides WebAssembly support
targeting `wasm32-wasip1` (WASI Preview 1).

## Learn more

- [CLI usage and memory tuning](https://github.com/oxc-project/tsrs/blob/main/docs/USAGE.md)
- [Editor setup](https://github.com/oxc-project/tsrs/blob/main/docs/LSP.md#editor-setup)
- [Node API](https://github.com/oxc-project/tsrs/blob/main/docs/NODE_API.md)
- [WebAssembly for Node and browsers](https://github.com/oxc-project/tsrs/blob/main/npm/tsrs-wasm/README.md)
- [Benchmarks and methodology](https://github.com/oxc-project/tsrs/blob/main/bench/README.md)
- [Release notes](https://github.com/oxc-project/tsrs/blob/main/docs/STATUS.md)
- [Engineering notes](https://github.com/oxc-project/tsrs/blob/main/notes/README.md)

## Contributing

The [issue tracker](https://github.com/oxc-project/tsrs/issues) is open for bug reports. Please include a minimal reproduction.
Pull requests remain closed because we don't have the bandwidth to review them.

See the [contributing guide](https://github.com/oxc-project/tsrs/blob/main/CONTRIBUTING.md)
for building from source and running the suites, the [Code of Conduct](https://github.com/oxc-project/tsrs/blob/main/CODE_OF_CONDUCT.md), and
[security policy](https://github.com/oxc-project/tsrs/blob/main/SECURITY.md) for reporting vulnerabilities.

## Acknowledgments

We thank the following organizations and contributors:

<p>
  <a href="https://depot.dev">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/oxc-project/tsrs/main/assets/sponsors/depot-dark.svg">
      <img alt="Depot" src="https://raw.githubusercontent.com/oxc-project/tsrs/main/assets/sponsors/depot-light.svg" height="32">
    </picture>
  </a>
  &nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://www.owner.com">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/oxc-project/tsrs/main/assets/sponsors/owner-dark.svg">
      <img alt="Owner.com" src="https://raw.githubusercontent.com/oxc-project/tsrs/main/assets/sponsors/owner-light.svg" height="32">
    </picture>
  </a>
</p>

- [Depot CI](https://depot.dev) for sponsoring our continuous integration.
- [Owner.com](https://www.owner.com) for sponsoring the initial work.
- [Max Schwenk](https://github.com/maschwenk) for the port.
- [@walkerdb](https://github.com/walkerdb) for improvements to the project.
- [@JiangWeixian](https://github.com/JiangWeixian) for transferring the `tsrs` npm package name.

We extend special thanks to the [TypeScript team](https://www.typescriptlang.org/) for their support.

## License

`tsrs` is a derivative of TypeScript (Copyright Microsoft Corporation), licensed under [Apache-2.0](https://github.com/oxc-project/tsrs/blob/main/LICENSE).
The bundled `lib.*.d.ts` files are TypeScript's. See [NOTICE](https://github.com/oxc-project/tsrs/blob/main/NOTICE) for attribution.
Not affiliated with or endorsed by Microsoft.
