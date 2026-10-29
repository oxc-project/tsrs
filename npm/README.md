# npm packaging

Install tsrs and enable its opt-in fast preset:

```sh
pnpm add -D tsrs
pnpm exec tsrs --noEmit --fast
```

`--fast` enables adaptive checker counts, lazy members and additional caches. It can change diagnostics and
exit status in known cases; see [fast mode](#compiler-defaults-and-fast-mode) for details.

tsrs ships on npm the way TypeScript 7 ships its native compiler (`typescript@7` / `@typescript/typescript-<os>-<arch>`):

| package | contents |
| --- | --- |
| `tsrs` | `bin/tsrs` (Node launcher), `lib/` (binary lookup, `version.cjs`), `dist/` + `vendor/` (the JavaScript API, see below), `optionalDependencies` on every platform package |
| `@tsrs/<os>-<arch>[-<libc>]` | the `tsrs` binary and default TypeScript libraries for one platform, with `os`/`cpu` (and `libc: glibc` or `musl` on Linux) so package managers install only the matching one |
| `@tsrs/wasm` | the WebAssembly module (`npm/tsrs-wasm`, notes/wasm-build.md): a `tsrs-wasm` command and a `tsc()` API for Node 22+ and browsers, single-threaded; no platform packages |

Native platforms: `darwin-arm64`, `darwin-x64`, `linux-x64-gnu`, `linux-arm64-gnu`, `linux-x64-musl`,
`linux-arm64-musl` and `win32-x64`. Intel macOS and Windows x64 joined the current platform set in 0.20.0.
See the [compatibility table](../docs/COMPATIBILITY.md#what-it-does-and-doesnt-do) for platform limitations.

The main package name is set in `npm/tsrs/package.json`; platform packages are `@tsrs/<os>-<cpu>` with `-gnu` or
`-musl` on Linux. The launcher derives that name from the current platform, architecture and libc at runtime.

The repository's root `README.md` is also the main package README. `npm/build.mjs` copies it into the staged package.

## Compiler defaults and fast mode

Since 0.20.0, tsrs uses the pinned TypeScript checker defaults. `--fast` or `TSRS_FAST=1` opts into
adaptive checker counts, lazy members, additional caches and eligible memory-saving modes; `--fast false`
overrides the environment preset. Known cases can change diagnostics and exit status, resource use and compiler
counters; switching modes can invalidate incremental caches. The launcher forwards CLI arguments and environment
variables to the compiler. See the [main README](../README.md#fast-mode) and
[performance options](../docs/USAGE.md#performance-options) for side effects and per-feature overrides.

Default mode preserves TypeScript's checker behavior, including its known diagnostic sensitivity to changes
in checker count or file order. This sensitivity is inherited from TypeScript itself.

## How the launcher finds the binary

`bin/tsrs` -> `lib/tsrs.js` -> `lib/getExePath.js`:

(`bin/tsrs` is a CommonJS stub, via `bin/package.json`, that imports the ESM launcher: Node 16 cannot run an
extensionless ES module in a `"type": "module"` package. `npm/sdk/smoke-consumer.mjs --node <path>` checks the
launcher and the JS API on each given Node; 16.20.0, 18, 20, 22 and 24 pass.)

1. `TSRS_BINARY` set: run that file (for local builds: `TSRS_BINARY=$PWD/target/debug/tsrs pnpm exec tsrs ...`).
2. Otherwise resolve `@tsrs/<process.platform>-<process.arch>[-<libc>]/package.json` from the launcher's own location (with
   pnpm that is the sibling link in `node_modules/.pnpm/<main>/node_modules/`), and run `tsrs`/`tsrs.exe` next to it.
   Linux libc detection follows Oxc's ldd/report probes to choose `-gnu` or `-musl`.
   If that fails, exit 1 with a message naming the missing optional dependency (or listing the supported platforms).
3. Like TypeScript's `tsc`, on Node >= 22.15 the launcher `execve`s the binary (no Node process left, signals and the
   exit code are the binary's own). On older Node and on Windows it spawns the binary, forwards SIGINT/SIGTERM/SIGHUP/
   SIGQUIT/SIGBREAK, and exits with the binary's code or re-raises its terminating signal.

`require("tsrs")` returns `{ version, typescriptVersion, typescriptCommit }` (TypeScript's
`lib/version.cjs` pattern).

## Versions

The workspace `Cargo.toml` is the version source of truth. This checkout prepares 0.20.1:

```toml
[workspace.package]
version = "0.20.1"

[workspace.metadata.typescript]
version = "7.1.0-beta"                                 # the TypeScript version the port follows
commit = "3e9236361fa97a9e6c685f4833ac457ae2d8c50a"    # and its commit (CI checks out its tsc/testdata)
nightly = "7.1.0-beta"                                 # matching npm package used for the compiler's reported version
```

npm version: `<version>-ts<typescript version>` = `0.20.1-ts7.1.0-beta`, for every package including
`@tsrs/wasm` (`npm/tsrs-wasm/package.json` holds a placeholder that `npm/build.mjs` replaces). `tsrs --version`
and `tsrs-wasm --version` include the package version, embedded at build time by `crates/tsrs_execute/build.rs`.
The example below shows the 0.20.1 release version; read `Cargo.toml` for the current source pin:

```
Version 7.1.0-beta (tsrs 0.20.1-ts7.1.0-beta, microsoft/TypeScript@3e9236361fa9)
```

To release a fix on the same TypeScript commit, bump `[workspace.package] version`; after porting a newer TypeScript
commit, update `[workspace.metadata.typescript]`, including the matching npm package version in `nightly`.

Source builds default to `7.1.0-beta` unless `TSRS_TS_VERSION` is set. Release builds stamp the matching npm package as the
compiler version so `.tsbuildinfo` files and `typesVersions` agree with the reference compiler.

## Building packages locally

Use Vite+ from the repository root to select the pinned Node.js and pnpm versions. Build a dev binary for local
package checks; the release workflow builds optimized binaries separately.

```sh
vp install --frozen-lockfile
cargo build --profile dev -p tsrs_cli --features noembed
vp run npm:build --binary aarch64-apple-darwin=target/debug/tsrs --pack
# -> npm/dist/tsrs-darwin-arm64-<version>.tgz, npm/dist/tsrs-<version>.tgz, npm/dist/packages.json
```

`--binary <triple>=<path>` can repeat; `--artifacts <dir>` picks up `<dir>/<triple>/tsrs[.exe]` (the release
workflow's layout). The main package lists exactly the platforms that were given, so a release never depends on a
platform package that was not published. `node npm/build.mjs --print-version` prints the version.

To try local tarballs in a pnpm project, the simplest route is to depend on both directly:

```sh
pnpm add -D tsrs@file:/abs/npm/dist/tsrs-<version>.tgz \
            @tsrs/darwin-arm64@file:/abs/npm/dist/tsrs-darwin-arm64-<version>.tgz
```

(The launcher then finds the platform package at the project root. A pnpm `overrides` entry pointing
`@tsrs/darwin-arm64` at the tarball reproduces the registry layout exactly, but changing `overrides` makes
pnpm re-resolve the whole lockfile.)

## JS API (`sync`, `async`, `ast`, and related exports)

`npm/tsrs/src`, `npm/tsrs/schemas`, `npm/tsrs/vendor` (vscode-jsonrpc, MIT, with its license) and `npm/tsrs/test` are TypeScript 7's JS API
package (`packages/typescript` in microsoft/TypeScript) at the SDK commit, copied by `npm/sdk/sync-upstream.mjs`
(byte for byte, except a few listed test edits; `npm/tsrs/UPSTREAM.json` holds the commit and sha256 of every file).
The SDK and compiler both follow `3e9236361fa9` (TypeScript 7.1 beta). The completed
[#623 source upgrade](../notes/typescript-623-ports.md) and
[beta follow-up](../notes/typescript-7.1-beta-ports.md) record the migration.

The package exports `tsrs/sync`, `tsrs/async`, `tsrs/ast`, `tsrs/vscode` and related upstream beta subpaths,
plus the `tsconfig` and `jsconfig` JSON schemas. Deprecated SDK aliases are removed; package builds strip
internal declarations and validate the emitted declarations. The API includes typed paths, an enum-valued
`caseSensitivity` handshake, and project checker queries for merged symbols, declaration/node symbols and parents.
Sync generator instances are single-use, including across completed batch calls. `TS_API_TIMING_DIR` enables
timing collection in either client and writes one JSON snapshot on `close()`. See
[Node API usage and migration](../docs/NODE_API.md#javascript-api) for the supported entry points and examples.

The VS Code module loader resolves imports from the selected package manifest. tsrs supplies the loader and SDK
types; the VS Code extension runtime and its initialization/restart lifecycle are separate.

When explicitly advancing the TypeScript source pin, follow the [reference workflow](../CONTRIBUTING.md#reference-compiler)
and run `vp run sdk:sync --source-commit <sha>`. `vp run sdk:sync --check` verifies the revision recorded in
`UPSTREAM.json` and fails if the copy drifted.

```sh
vp install --frozen-lockfile
vp run npm:build --sdk-only                # npm/tsrs/src -> npm/tsrs/dist
(cd npm/tsrs && TSRS_BINARY=/path/to/server node --conditions @typescript/source --test 'test/**/*.test.ts')
vp run sdk:smoke                          # after npm:build --pack: installed-package CLI and API checks
vp run sdk:coverage --server tsgo=/path/to/pinned/tsgo --server tsrs=target/debug/tsrs  # -> npm/sdk/METHODS.md
```

The tests and the inventory can run against a tsgo built and verified using the
[reference compiler workflow](../CONTRIBUTING.md#reference-compiler). The SDK spawns `getExePath()`
(so `TSRS_BINARY` applies) unless the caller passes `tsserverPath`.

Deliberate patches (the only differences from the recorded SDK sources; each patch file explains itself, and
`sync-upstream.mjs` applies them strictly and records the original sha256 in `UPSTREAM.json` `patchedFiles`):

- `npm/sdk/patches/async-client-connection-loss.patch` (`src/api/async/client.ts`): upstream, if the server dies or
  closes the connection while the async client is open, in-flight and queued requests never settle. Now every
  pending and later request rejects with an `Error` whose message starts with `API server connection lost: `.
  The patch preserves upstream's idempotent `close()` and timing-dump behavior; `close()` after a crash rejects
  promptly with that error if it had snapshots to release.
- `npm/sdk/patches/vscode-jsonrpc-send-request-write-error.patch` (vendored vscode-jsonrpc 9.0.2): a request written
  to a dead server also became an unhandled rejection that terminated Node; the request's own rejection is kept.
- `npm/sdk/patches/vscode-jsonrpc-msgpack-version.patch`: keeps tsrs's existing vendored development dependency
  adjustment when the upstream SDK copy advances.

The sync client is unpatched: after a server crash its next call throws at once with the raw pipe error.
`npm/sdk/smoke/consumer.mjs` kills the real server with work in flight to cover all of this (the unpatched
package hangs those requests and crashes on the unhandled rejection).

Wire contract the server has to speak (from the pinned `tsc/cmd/tsc/api.go`, `tsc/internal/ipc`, `src/api/options.ts`):

- argv: `--api [--async] --cwd <dir> --useCaseSensitiveFileNames=<bool> [--callbacks=<names>] [--timing]
  [--runExternalCode] [--pipe <path>]` (`--pipe` only on Windows for the spawned sync client). On Unix, the optional
  `--pipe` argument names a new private directory and the client connects to `<path>/socket`; on Windows it names a
  current-user-only named pipe. stdout carries only protocol bytes; stderr is inherited.
- sync: every message is a MessagePack 3-array `[uint8 type, bin method, bin payload]`; types 1 request, 2 callback
  response, 3 callback error (client to server), 4 response, 5 error, 6 callback call (server to client). Payloads are
  JSON text, except binary responses (`getSourceFile` and friends: the AST encoding, protocol version 9 in the top
  byte of the metadata word).
- async: JSON-RPC 2.0 with `Content-Length` framing; filesystem callbacks are server-to-client requests.
- handshake: the first request is `initialize` with params `null`; the result is
  `{"caseSensitivity": <0 or 1>, "currentDirectory": "<cwd>"}` (`0` is insensitive, `1` is sensitive).
  There is no version negotiation: client and server must come from the same commit.

## Releasing

To prepare a release, bump `[workspace.package] version` in `Cargo.toml`, refresh `Cargo.lock`, set the root
`package.json` version to the output of `node npm/build.mjs --print-version`, and add `docs/releases/<workspace version>.md`.
`.github/workflows/release.yml` publishes when that root package version is bumped on `main`:

1. checks the root package version against `Cargo.toml`, requires release notes, and runs `cargo check`;
2. builds PGO release binaries (`release` profile, trained by `.github/scripts/pgo-train.sh`) for macOS arm64 (`macos-latest`),
   macOS x64 (`macos-26-intel`, with native PGO training and installed-package smoke tests), and Linux x64 and arm64 GNU
   (`ubuntu-22.04`), and a Windows x64 PGO binary trained on the conformance suite and a CLI project;
   on GNU, BOLT-optimizes them and runs the conformance and fourslash gates on the
   BOLT-optimized binaries (`.github/scripts/bolt.sh`); smoke-runs the native ones (`--version`, exit code 2 on a type
   error) and the installed package (`npm/sdk/smoke-consumer.mjs`);
   musl x64/arm64 uses static release builds with Zig 0.13.0 and cargo-zigbuild 0.24.0;
3. builds the WebAssembly module (`tools/wasm/build.sh`, binaryen 133), runs `pnpm test` in `npm/tsrs-wasm`, packs
   `@tsrs/wasm` with `npm/build.mjs --wasm`, installs that tarball and runs it (`--version` must name the
   release version, exit code 2 on a type error), and uploads it as the `npm-wasm` artifact;
4. assembles and packs the native packages with `npm/build.mjs`, uploads the tarballs as the `npm-packages` artifact,
   and publishes the platform packages, then the main package, then `@tsrs/wasm`, with
   `--access public --tag latest` using npm trusted publishing (skipping versions already published or staged, so a
   failed run can be re-run). The publish job allows 90 minutes. Before publishing the main package, waits up to
   30 minutes for every optional dependency's exact version metadata and tarball to be available on npm, including
   platform packages skipped on a re-run.
   Dry runs skip this check. Releases publish directly, without a staged approval step.
5. waits for all published packages to be available on npm, creates the GitHub release tagged `v<workspace version>`
   with the version alone as its title and `docs/releases/<workspace version>.md` as its notes, marks it as the
   latest GitHub release, then deploys the browser playground.

The conformance and Go fourslash gates run on the GNU Linux release runners (`bolt.sh`), with Fourslash driving
the BOLT-optimized `fourslash_server` task binary. Unix PGO training uses the same Go harness. The macOS binaries get only the
smoke tests; Windows gets CLI smoke tests in the release job. The broader WebAssembly differential gate (`tools/wasm/gate.sh`) is
available locally; the release's WebAssembly package checks remain active.

`workflow_dispatch` runs the same with `pnpm publish --dry-run` by default.

What maintainers must configure before the first release:

- The `@tsrs` npm org and each package must exist. Configure a GitHub Actions trusted publisher for `tsrs`,
  `@tsrs/wasm` and each platform package, including `@tsrs/darwin-x64` and `@tsrs/win32-x64`: owner `oxc-project`,
  repository `tsrs`, workflow filename `release.yml`, no environment name, with direct publishing allowed.
- The publish job uses pnpm 12.15.0 and `id-token: write` to authenticate through GitHub Actions OIDC. No registry publish
  token or repository secret is required. See [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/)
  for configuration and first-publish validation requirements.
- Provenance: npm only accepts `--provenance` from public repositories. The workflow adds `--provenance` automatically
  when the repository is public and omits it while it is private.
- GNU binaries are built on Ubuntu 22.04, so they need glibc >= 2.35 at most (a build on Debian 12 needed 2.34).
  Musl binaries are statically linked for Alpine/musl.

`.github/workflows/ci.yml` runs on pushes to main, pull requests into main, merge groups and manual dispatch.
Its Linux jobs run Clippy/source checks and workspace integration checks, build the compiler and conformance
runner with the `ci` profile, and compare compiler conformance in both checker modes plus regression cases.
Separate jobs cover determinism, ThreadSanitizer, Node API clients, Windows checks and the Go Fourslash suite.
See [contributing](../CONTRIBUTING.md#landing-changes) for the gate details.

Auxiliary workflows saved as `.github/workflows/*.yml.bak` are disabled; their local tools remain available.

## Adopting it in a pnpm workspace

After a release is on npm:

```sh
# 1. Add it next to the TypeScript compiler. If the workspace sets pnpm's minimumReleaseAge, either wait or list
#    the main package and the platform packages in minimumReleaseAgeExclude.
pnpm add -D tsrs
#    Review the pnpm-lock.yaml diff: a non-frozen install can re-resolve unrelated entries.

# 2. Run it.
pnpm exec tsrs --version
pnpm exec tsrs -p path/to/project --noEmit --extendedDiagnostics

# 3. Switch a tsc script to tsrs: the flags are the same. Like tsc, tsrs emits unless the options say otherwise
#    (`--noEmit` for a typecheck-only script).
```

Without `--checkers`, tsrs 0.20.0 and later use four checkers, as tsgo does, capped by file count. With `--fast`
or `TSRS_FAST=1`, the count adapts to the machine: every core up to 16, then the greater of 16 and half the cores,
clamped to 4–32; a program with fewer than 32 type-checked files per checker gets fewer, down to 4, and never more
checkers than files. `--build` uses 4 per project in both modes; `--singleThreaded` uses one.
`GOMEMLIMIT` has no effect on tsrs.
`--extendedDiagnostics` uses tsgo's counter labels; `Memory used` is the process RSS and `Memory allocs` is always 0.
Compilation timings are collected only when `--diagnostics` or `--extendedDiagnostics` is enabled. Pass the flag on
the command line to include config-read timing.

## TODO

- Gate the macOS release binary on the conformance suite too (the Linux builds already run it in `bolt.sh`).
- Optionally target an older glibc for Linux (e.g. `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17`).

## Type-aware linting

The source-built `tsrs-lint` binary currently implements all 60 pinned tsgolint rules.
It is not included in npm artifacts; see [progress](../testdata/lint/PROGRESS.md).