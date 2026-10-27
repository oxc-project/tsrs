# Recovered package fixtures

These are tiny authored test packages, not downloaded npm dependencies. Real
third-party dependencies are installed from `crates/tsrs_dts/tests/dependencies`
with frozen pnpm resolution through Vite+ and are never vendored:

```bash
vp install --frozen-lockfile --ignore-scripts --no-optional
```

Package files were recovered from
[Swatinem/rollup-plugin-dts at 20c8afb5021ba876fdf18c63ba00bae1f4d867e8](https://github.com/Swatinem/rollup-plugin-dts/tree/20c8afb5021ba876fdf18c63ba00bae1f4d867e8).
`provenance.json` records original paths, verified Git blob identities, SHA-256
hashes and byte lengths. The source license is retained as `COPYING`.

The selected source revision predates typack's fixture import. The two fixture
entry files match upstream Git blobs, establishing the source relationship;
this is not proof of the exact historical revision used by typack.

- `externals-link`: exact `tests/d3-drag-local` package metadata and declaration.
  The original fixture links this directory with a symlink. Here normal virtual
  `node_modules` placement restores its type surface; symlink behavior remains
  covered separately by the existing procedural regression.
- `inline-external-node-next-module`: exact `package-a` package metadata, type
  subpath and empty runtime file. The upstream metadata requests Node16 module
  resolution. An explicit module package context lets the unchanged `.d.ts`
  fixture import this ESM-only package under semantic checking; this package
  context is an adapter choice. No runtime file is executed.

The foo-family modules, the type-only fixture's bare modules and the absolute
`/some/absolute/path` reference have no definitions in the inspected source
revisions. They remain classified as missing environments.
