The tsrs overlay applies to withastro/triagebot-action at
`51d30da13c0bd571a3822fc56882de40fead0086`. The upstream source is checked out
under ignored `target/triagebot-action`; `apply.mjs` checks and applies
`upstream.patch`, then copies the three replacement/new modules into it.
The action's frozen upstream lockfile remains the dependency specification.
The composite action builds and tests the result, and caches only its compiled
bundle. Updating the upstream pin also requires reviewing and refreshing the
patch and the composite action's cache key.

The changes address the successful runs reviewed on 2026-10-22:

| Evidence | Change |
| --- | --- |
| All nine sampled jobs missed Rust caches and could not save them | Issue jobs restore main CI's `warm-test` dependency cache; they never attempt to save it |
| [#664](https://github.com/oxc-project/tsrs/actions/runs/39615833976) spent about 239s in a release build and 174s building Go | Correctness uses dev; `prepare.mjs` prepares one shared pinned reference and records compiler paths/revisions |
| [#651](https://github.com/oxc-project/tsrs/actions/runs/39556383349) built compilers before skipping an unsupported capability | An independent actionability decision precedes compiler preparation |
| [#673](https://github.com/oxc-project/tsrs/actions/runs/39643203286) made 249 tool calls; reporting continued 4m59s after the fix | Stage prompts include exact instructions/paths; one short response supplies the summary and labels, and JavaScript appends the report verbatim |
| [Terminal-label follow-up](https://github.com/oxc-project/tsrs/actions/runs/39752070283) paid 24s for Node/Rust setup | A dependency-free routing step refreshes current labels and skips the job's remaining steps |

`triage-cache.yml` populates the reference compiler and compiled-action caches
on trusted main pushes, or manual dispatch on main. Reference keys include the
exact TypeScript commit, which pins `go.mod`, `go.sum`, and the required Go
version; the manifest also validates commit, required Go version, OS, and
architecture before binary reuse. Cold issue runs can build the reference
without writing caches. Main CI continues to publish the Rust dependency cache.
Workspace Rust binaries are rebuilt with Cargo for the checked-out revision.

Classification and reporting use the configured verification model in a
separate session. Reproduce, diagnose, verify, and fix retain the triage model.
Independent verification and the repository's regression/lint requirements are
preserved. Unclear verification or a low-confidence diagnosis cannot enter fix.
The normal GitHub branch, PR, label, retriage, confirmation, and cleanup code
remains upstream-owned. Reports and compiler/stage manifests are retained as
workflow artifacts for 14 days; compiler/source scratch files stay ignored.

Validate compiler preparation directly, then repeat it to check warm reuse:

```sh
node .github/scripts/triage/prepare.mjs
```

For the overlay, check out the pinned upstream into `target/triagebot-action`,
then run:

```sh
node .github/triagebot/apply.mjs
cd target/triagebot-action
pnpm install --frozen-lockfile --ignore-scripts
pnpm exec tsc --noEmit
node --test test/*.test.ts
pnpm build
node --check dist/index.mjs
```

If local Git signing is enabled, disable signing only in the test process with
`GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false`.
The existing upstream unit tests create no live issue comments, branches, or PRs.
Use focused smoke checks for the local routing, preparation, stage schemas, and
compiled launcher; do not invoke the live bot just to validate tooling changes.

Compare production results using `target/triage/metrics.jsonl`, separately for
cold and warm caches, on fixed issue snapshots and the same runner class.
Historical run timings establish bottlenecks; they are not a measured speedup
for this implementation. No compiler runtime behavior or benchmark result changes.
