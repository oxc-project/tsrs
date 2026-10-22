The triage workflow invokes the unchanged upstream action at
`51d30da13c0bd571a3822fc56882de40fead0086`.

`route.mjs` avoids setup for no-op events. It refreshes issue state and labels
to suppress queued events that have since become terminal, while preserving the
original event route consumed by upstream. Initial/reopened issues, retriage,
fix confirmation, and closed-issue cleanup retain their existing handlers.

The reproduction skill screens actionability before calling `prepare.mjs`.
The helper builds correctness binaries with the dev profile and reuses the
shared reference checkout and Go binary. Later stages reuse the compiler paths
and exact revisions in `target/triage/compilers.json`.

`triage-cache.yml` populates the reference cache on trusted main pushes or
manual dispatch on main. Its key includes OS, architecture, and the exact
TypeScript commit, which pins Go dependencies and the required Go version.
The helper validates those identities and the binary before warm reuse.
Issue runs restore this cache and main CI's Rust dependency cache; a cold
issue run builds without attempting to save. Reports and compiler manifests
are retained as artifacts for 14 days.

Validate preparation directly, then repeat to check warm reuse:

```sh
node .github/scripts/triage/prepare.mjs
```

Use focused smoke checks for routing and cache invalidation, and actionlint
for both triage workflows. Do not run the live bot just to check tooling.

Historical evidence: [#664's run](https://github.com/oxc-project/tsrs/actions/runs/39615833976)
spent about 239 seconds building release Rust and 174 seconds building Go;
[a terminal-label follow-up](https://github.com/oxc-project/tsrs/actions/runs/39752070283)
spent 24 seconds setting up Node/Rust before doing no work. These identify
bottlenecks, not measured before/after speedups for this change.
