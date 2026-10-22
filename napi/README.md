# @tsrs/napi

Native Node.js bindings for TypeScript project checking. The addon runs the same driver as
`tsrs --noEmit -p <project>` and bundles the default TypeScript libraries. It is independently
versioned and released from `napi/`; the CLI and its subprocess-based Node API retain their own release.

```js
import { check, checkSync } from '@tsrs/napi';

const result = await check({ project: './tsconfig.json' });
console.log(result.exitCode, result.diagnostics);

const syncResult = checkSync({ cwd: '/path/to/project', project: '.' });
```

CommonJS exposes the same functions with `require('@tsrs/napi')`. Both options are optional:
`cwd` defaults to the working directory captured when called, and `project` defaults to `.`.
The process working directory is never changed. Compiler errors return a nonzero `exitCode` and
structured `diagnostics`; invalid arguments throw and native failures throw or reject. Diagnostics
include message text, codes, UTF-16 positions, source lines, message chains and related information.
`stdout` and `stderr` capture other driver output such as `listFiles` and `traceResolution`.

`check()` runs off the JavaScript thread; `checkSync()` blocks it. Calls from promises and Node workers
are serialized inside the compiler to isolate its process-wide state. Each call reads files again and
releases its programs and source-file regions. Existing compiler limitations apply (see
[compatibility](https://github.com/oxc-project/tsrs/blob/main/docs/COMPATIBILITY.md)). This API does not
expose emit, build mode, watch, language-server or compiler-object APIs. Project references and
incremental checking follow the CLI, including writing `.tsbuildinfo` when configured.

Node.js `^20.19.0 || >=22.12.0` is required. The release builds these native targets:

| OS | architectures |
| --- | --- |
| macOS | arm64, x64 |
| Windows (MSVC) | arm64, x64, ia32 |
| Linux (glibc) | arm64, x64, armv7, ppc64le, s390x, riscv64 |
| Linux (musl) | arm64, x64, armv7, riscv64 |

The matching `@tsrs/napi-<platform>` optional dependency contains the `.node` binary. Android,
FreeBSD, WASI and browser builds are outside this package's target set. The first npm publication
is pending; CI exercises macOS arm64, Linux x64 glibc and Windows x64/ia32 directly. On 32-bit
targets, compiler thread stacks reserve 16 MiB each to fit the process's address space; extremely
deeply recursive inputs can exhaust them sooner than on 64-bit targets.

## Development

From the repository root:

```sh
pnpm install --frozen-lockfile --ignore-scripts
pnpm --dir napi build-dev
cargo build --profile dev --locked -p tsrs_cli
pnpm --dir napi test
# Substitute the current host's Rust target to pack and check a local build.
node napi/scripts/release.mjs assemble napi/src-js aarch64-apple-darwin
node napi/scripts/smoke.mjs
```

## Release

Bump only `napi/package.json` to a new stable semver version and update the workspace lockfile.
The `Release NAPI` workflow starts on that change on `main`; manual dispatch retries a real release.
There is no dry-run input. The workflow builds all 15 targets, validates and packs their artifacts,
checks an installed consumer, then publishes platform packages before `@tsrs/napi`. It waits for
each exact version, tarball and `latest` tag to be visible. Retries skip already published versions;
registry failures and version rollback stop the release. CLI, WebAssembly, tags and GitHub releases
are handled by their existing workflow.

Before the first automated release, create the npm packages and configure GitHub Actions trusted
publishing for `@tsrs/napi` and every `@tsrs/napi-<platform>` package: repository `oxc-project/tsrs`,
workflow `release_napi.yml`, no environment. Publishing uses OIDC with `id-token: write` and enables
provenance for public repositories. No npm publish token is stored in the workflow.
