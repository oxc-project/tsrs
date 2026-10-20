# DefinitelyTyped conformance task

This Rust task compares the TypeScript 7 `--expectOnly` checks on the same
DefinitelyTyped checkout with tsgo and tsrs. It checks compiler diagnostics and
`$ExpectType` assertions through each compiler's JSON-RPC API. Matching failures
count as conformance; a package that neither compiler completes is incomplete.

Build with `cargo build --profile dev -p tsrs_cli -p tsrs_testrunner -p tsrs_definitelytyped`.
Install DefinitelyTyped's dependencies with its declared pnpm version before
running. Build tsgo from the TypeScript commit pinned in the workspace Cargo
manifest.

```sh
target/debug/tsrs_definitelytyped plan --dt /path/to/DefinitelyTyped --out /tmp/dt/manifest.json
target/debug/tsrs_definitelytyped run --manifest /tmp/dt/manifest.json \
  --dt /path/to/DefinitelyTyped --tsgo /path/to/tsgo --tsrs target/debug/tsrs \
  --shard 0/16 --jobs 2 --out /tmp/dt/results
target/debug/tsrs_definitelytyped report --manifest /tmp/dt/manifest.json \
  --results /tmp/dt/results --out /tmp/dt/report.md
```

`plan --package NAME` selects one package for a smoke run. Shards group versions
of the same package. `run` records one JSON file per package with both results.
`report` fails on missing or incomplete results and publishes compiler mismatches
without failing, so daily runs retain evidence while parity defects are being fixed.
Full conformance requires every planned package to be identical in a completed
run. The workflow uploads the per-package JSON results as artifacts; those files
retain the exact diagnostics and assertion results.
