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

## Fast mode

The following defaults and opt-in preset are **unreleased** and available in source builds.

By default, tsrs follows the pinned TypeScript compiler's checker settings: four checkers, Go's file assignment
and checker history, and eager type members. `--fast` opts into adaptive parallelism, lazy type members,
additional checker caches, and eligible memory-saving modes:

```sh
pnpm exec tsrs --noEmit --fast
# Or enable the same preset through the environment:
TSRS_FAST=1 pnpm exec tsrs --noEmit
```

Fast mode has these side effects:

- **Diagnostics can differ from TypeScript.** In known cases involving check order, recursive types or
  instantiation depth limits, errors can appear, disappear, move or have different message text. The exit status
  can change too. Fast mode reduces some check-order dependencies, but does not guarantee identical results
  across every run or checker count. See the [known cases](https://github.com/oxc-project/tsrs/blob/main/notes/open-history-dependence.md).
- **CPU and memory use change.** The checker count adapts to the machine and project, up to 32. More checkers
  can use more memory even with the memory-saving optimizations. Use `--checkers N` to set the count or
  `--singleThreaded` to use one thread; `--build` keeps four checkers per project unless overridden.
- **Compiler statistics change.** Lazy member resolution and additional caches change the work performed, so
  type, symbol and instantiation counts can differ from TypeScript's. Work stealing can also make these counters
  vary between runs.
- **Switching modes can require a fresh check.** Incremental and build caches with incompatible checker-history
  or lazy-member settings are discarded, so the next build can take longer.

Fast mode works with compilation, watch, build mode and response files. It keeps your `tsconfig.json` checking
options, including `strict`, `skipLibCheck` and `noCheck`, as configured. An explicit `--fast false` overrides
`TSRS_FAST=1`; individual feature overrides still apply. See
[performance options](https://github.com/oxc-project/tsrs/blob/main/docs/USAGE.md#performance-options) for the full
preset, precedence rules and tuning controls.

## Conformance

We aim to release a stable version against TypeScript 7.1 with 100% conformance.
See [compatibility and evidence](https://github.com/oxc-project/tsrs/blob/main/docs/COMPATIBILITY.md)
for supported features and known gaps.

The default checker settings follow the pinned TypeScript reference, including its sensitivity to checker count
and file order. Matching those settings does not guarantee deterministic results for every program. See
[differences from pinned TypeScript](https://github.com/oxc-project/tsrs/blob/main/docs/DEFAULTS.md) for the remaining
runtime differences and unsupported features.

<a href="https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/oxc-project/tsrs/main/bench/large-projects-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/oxc-project/tsrs/main/bench/large-projects.svg">
    <img alt="tsrs vs the TypeScript nightly: relative type-checking speed and peak memory by project" src="https://raw.githubusercontent.com/oxc-project/tsrs/main/bench/large-projects.svg">
  </picture>
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
