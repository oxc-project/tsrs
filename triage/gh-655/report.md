# Issue 655 Reproduction Report

## Claim

Every call to `ModuleResolver.resolveModuleName` through the sync API's `ModuleResolver` in `tsrs --api` grows process RSS by ~3.4 KB and the growth is not released by `resolver.dispose()`, by creating a fresh resolver, or by idling. Only `api.close()` frees the memory because it terminates the server. The TypeScript 7 Go reference (`typescript/unstable/sync`) stays flat.

## Compiler revisions

- tsrs: built from current checkout
  - crate version: `0.18.0`
  - package version: `0.18.0-ts7.1.0-dev.20260929`
  - pinned TypeScript commit: `b85298b6a81f772d080b0455de0ca9d744cd6fd6`
  - binary: `/home/runner/work/tsrs/tsrs/target/release/tsrs` (built with `cargo build --release --locked -p tsrs_cli`)
- TypeScript 7 reference: `typescript@7.1.0-dev.20260929.1` (npm, Go-backed native server matching the pinned commit above)

## Minimal input

`package.json`:

```json
{
  "private": true,
  "type": "module",
  "dependencies": {
    "tsrs": "file:../../npm/tsrs",
    "typescript": "7.1.0-dev.20260929.1"
  }
}
```

`repro.mjs`:

```js
// node repro.mjs tsrs|typescript
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';

const impl = process.argv[2] ?? 'tsrs';
const {API, ModuleKind, ModuleResolutionKind} = await import(`${impl}/unstable/sync`);

const root = path.resolve('fixture');
fs.mkdirSync(path.join(root, 'src'), {recursive: true});
fs.writeFileSync(path.join(root, 'src/index.ts'), 'export {};\n');

const serverRssMb = () => {
  const pid = execFileSync('ps', ['-o', 'pid=', '--ppid', String(process.pid)], {encoding: 'utf8'}).trim().split(/\s+/)[0];
  return Math.round(Number(fs.readFileSync(`/proc/${pid}/status`, 'utf8').match(/VmRSS:\s+(\d+)/)[1]) / 1024);
};

const api = new API({cwd: root});
const resolver = api.createModuleResolver({
  module: ModuleKind.ESNext,
  moduleResolution: ModuleResolutionKind.Bundler,
});
console.log(`${impl}: start ${serverRssMb()} MB`);
for (let i = 1; i <= 60_000; i++) {
  resolver.resolveModuleName('./index.ts', path.join(root, 'src'));
  if (i % 20_000 === 0) console.log(`${impl}: ${i} resolutions ${serverRssMb()} MB`);
}
resolver.dispose();
await new Promise((resolve) => setTimeout(resolve, 1000));
console.log(`${impl}: after dispose ${serverRssMb()} MB`);
api.close();
```

## Commands

```sh
# from repository root
cargo build --release --locked -p tsrs_cli
cd npm && pnpm install --frozen-lockfile --allow-build esbuild --allow-build workerd
node ../npm/build.mjs --sdk-only

cd triage/gh-655
npm install
TSRS_BINARY=/home/runner/work/tsrs/tsrs/target/release/tsrs node repro.mjs tsrs
node repro.mjs typescript
```

## Observed outputs

### tsrs

```
tsrs: start 9 MB
tsrs: 20000 resolutions 87 MB
tsrs: 40000 resolutions 167 MB
tsrs: 60000 resolutions 243 MB
tsrs: after dispose 243 MB
```

Exit status: `0`

### TypeScript 7 reference

```
typescript: start 15 MB
typescript: 20000 resolutions 20 MB
typescript: 40000 resolutions 21 MB
typescript: 60000 resolutions 21 MB
typescript: after dispose 21 MB
```

Exit status: `0`

## Conclusion

REPRODUCED. The tsrs API server RSS grows by ~234 MB over 60,000 identical `resolveModuleName` calls and `dispose()` frees none of it, while the pinned TypeScript 7 Go server stays flat within ~6 MB total.

# Diagnosis

## First mismatching functions

- Go spec: `tsc/internal/api/module_resolution.go:316` — `Session.handleResolveModuleName`.
- Rust port: `crates/tsrs_api/src/session.rs:472` — the dispatch arm for `"resolveModuleName"` calls `self.handle_resolve_module_name(p)` directly.
- The resolver created for every call: Go `tsc/internal/module/resolver.go:171` `module.NewResolver`; Rust `crates/tsrs_module/src/resolver.rs:255` `tsrs_module::new_resolver`, which calls `new_resolution_data` in `crates/tsrs_module/src/cache.rs:115`.

## Precise semantic / control-flow difference

Go creates a fresh `module.Resolver` and its caches on each `resolveModuleName` call and lets the GC reclaim them when the call returns. tsrs mirrors the fresh-resolver-per-call logic, but the Rust equivalent allocates resolver state through `P::new`, which lands in the API server thread's *leak arena* (`crates/tsrs_core/src/ptr.rs`: `P::new` allocates in the current thread's leak arena and destructors never run). Because the API server thread stays alive for the whole session, those transient arena blocks are never released. `dispose()` only removes the `ModuleResolverRegistration` from the session map (`crates/tsrs_api/src/session.rs` and `crates/tsrs_api/src/module_resolution.rs`); the underlying arena memory is held by the thread, not by the registration, so disposal frees nothing.

Other standalone API methods that only return owned JSON (`parseCommandLine`, `readConfigFile`, `parseJsonConfigFileContent`, `parseConfigFile`) are already wrapped in `session.rs:537` `scratch`, which enters a temporary arena region, runs the handler, then frees the region and its drop list. `resolveModuleName` is missing that wrapper, so every call's resolver, resolution caches, `ResolutionData`, `InfoCache`, and result objects accumulate in the leak arena.

Evidence from a temporary instrumentation: changing the dispatch arm at `session.rs:472` to `scratch(|| self.handle_resolve_module_name(p))` and rebuilding the CLI made the same reproduction stay flat at 11 MB across 60,000 calls (from 9 MB start), matching the TypeScript 7 Go reference behavior. The change was reverted after measurement.

## Fix approach

Wrap the standalone `resolveModuleName` request in `scratch` so its transient arena allocations are freed once the JSON response is built. The `inProgressSnapshot` path must be excluded (or handled separately): program-resolution-context resolvers are cached in `ModuleResolvers.contexts` and outlive the request, so allocating them in a scratch region would create use-after-free. For the common standalone and `snapshot`-scoped cases, entering a region around `handle_resolve_module_name` and dropping it after serializing the response is sufficient. Affected crate: `crates/tsrs_api`.

## Uncertainty

- Whether the `snapshot`-scoped `resolveModuleName` path can safely share the same scratch region (it does not store the resolver in session state, so it should be safe).
- Whether the `inProgressSnapshot` path is reachable from public `resolveModuleName` calls in practice; if it is, the fix must keep that branch outside the scratch region.

## Confidence

High. The reproduction numbers, the `scratch` wrapper experiment, the `P::new`/arena documentation, and the parity with the Go GC-per-request model all point to the missing scratch region for `resolveModuleName`.

# Verification

## Verdict

`bug`.

## Evidence

Both the checked-out tsrs and the pinned TypeScript 7 Go server were run with the same `fixture`, the same `repro.mjs`, and the same `ModuleKind.ESNext` / `ModuleResolutionKind.Bundler` options. The pinned Go server stays flat while tsrs grows at a constant per-call rate and `dispose()` recovers no memory.

Fresh verification run on the release build:

### tsrs

```
tsrs: start 11 MB
tsrs: 20000 resolutions 89 MB
tsrs: 40000 resolutions 169 MB
tsrs: 60000 resolutions 245 MB
tsrs: after dispose 245 MB
```

### TypeScript 7 reference (pinned Go compiler)

```
typescript: start 17 MB
typescript: 20000 resolutions 20 MB
typescript: 40000 resolutions 20 MB
typescript: 60000 resolutions 21 MB
typescript: after dispose 21 MB
```

## Confidence

High. The comparison is a direct, deterministic run of the issue's reproduction against the pinned Go compiler at the exact nightly (`typescript@7.1.0-dev.20260929.1`) corresponding to `workspace.metadata.typescript` commit `b85298b6a81f772d080b0455de0ca9d744cd6fd6`.

# Fix

## What changed

- `crates/tsrs_api/src/session.rs`: the `"resolveModuleName"` dispatch arm now routes standalone requests (those with `inProgressSnapshot == 0`) through the existing `scratch` helper, which allocates inside a temporary arena region and frees it after the JSON response is built. Requests that carry an `inProgressSnapshot` are excluded because their resolver may be cached in `ProgramResolutionContext` and must outlive the request.
- `crates/tsrs_api/tests/memory_test.rs`: added `repeated_resolve_module_name_reclaims_scratch_memory`, a regression test that resolves the same module 300 times and asserts RSS growth stays below allocator noise.

## Why this matches the Go behavior

Go's `handleResolveModuleName` (`tsc/internal/api/module_resolution.go:316`) creates a fresh Go heap-allocated `module.Resolver` on every call and relies on the garbage collector to reclaim it. tsrs's equivalent allocates the resolver and caches through `P<T>` into the API-server thread's leak arena; without a per-request region those allocations persist until the process exits. `scratch` already provides the per-request GC-like boundary used by `readConfigFile`, `parseCommandLine`, etc.; applying it to `resolveModuleName` restores parity.

## Verification after fix

### Reproduction (release build)

```sh
cargo build --release --locked -p tsrs_cli
TSRS_BINARY=/home/runner/work/tsrs/tsrs/target/release/tsrs node repro.mjs tsrs
node repro.mjs typescript
```

### tsrs (fixed)

```
tsrs: start 9 MB
tsrs: 20000 resolutions 10 MB
tsrs: 40000 resolutions 10 MB
tsrs: 60000 resolutions 10 MB
tsrs: after dispose 10 MB
```

### TypeScript 7 reference

```
typescript: start 17 MB
typescript: 20000 resolutions 20 MB
typescript: 40000 resolutions 20 MB
typescript: 60000 resolutions 20 MB
typescript: after dispose 20 MB
```

### Tests and checks

```sh
cargo test -p tsrs_api                    # 89 integration tests passed
cargo test -p tsrs_api --test memory_test # 5 passed, regression grew only 4 KiB over 300 calls
tools/regressions.sh                       # 33/33 passed
cargo clippy --all-targets --config 'build.warnings="deny"'  # clean
tools/lint/source.py                        # ok
```

## Final diff

```diff
 crates/tsrs_api/src/session.rs       | 13 ++++++++++++-
 crates/tsrs_api/tests/memory_test.rs | 47 ++++++++++++++++++++++++++++++++++++
 2 files changed, 59 insertions(+), 1 deletion(-)
```

No user-visible capability changed, so `docs/COMPATIBILITY.md` does not need an update for this bug.
