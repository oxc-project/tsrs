# TypeScript nightly synchronization

`.github/workflows/port-typescript.yml` checks for a published TypeScript
nightly hourly and by manual dispatch. It resolves `typescript@next` once,
then reads that exact version's npm metadata for its `gitHead` and native
compiler dependencies and registry integrity hashes. One PR contains the complete change from the current
Cargo source pin to that published snapshot, including merged side branches.
The workflow never selects intermediate branch commits as reference states.
Scheduled runs and manual dispatches are restricted to `main`.
It rejects a nightly that omits the previous nightly's ancestry. An initial
release-to-nightly switch may differ at the branch point only in Go's version
constant; other omitted source changes block the update.

Each update advances Cargo's `version`, `commit` and `nightly`, the root and
npm tooling TypeScript dependencies, the pnpm lockfile, vendored SDK and
benchmark reference metadata and diagnostic snapshots together. The source checkout matches Cargo's
pin throughout validation. Compiler conformance, Node API, Fourslash and
benchmark parity all use their normal pinned-reference paths. No ledger or
CI reference override is needed. Snapshot generation uses the exact installed
npm compiler, and a normal parity check never rewrites its expectations. The
resolver checks both package manifests' versions and source commits plus the
binary version. The worker freezes compiler/library hashes before the agent
runs and checks them afterward; publication checks lockfile integrity against
the registry metadata frozen during preparation.

Source baselines keep Go's source-default version. A second noembed CLI build
uses the exact published nightly version and adjacent standard libraries for
benchmark, Node API and packed-consumer parity. A focused fixture compares
CLI/source stamps, incremental metadata, version-sensitive package resolution,
diagnostics and exit status. These additional checks run only in the new
nightly-sync workflow; existing CI and release workflows are unchanged.
Node API setup keeps its source-built oracle by default. The nightly worker
opts into `--published-reference`, using a separate `published-oracle` cache.

The update includes the common generators, manual option/tsconfig generated
regions, API method classifications and recorded SDK coverage. The agent must
inspect affected internal oracle fixtures, account for removed or renamed
upstream cases, update compatibility evidence and verify WebAssembly builds.
Platform, Fourslash and sanitizer gates remain in the normal PR checks.

Nightly synchronization preserves the root package version. Publication
continues to follow the existing release workflow. Release preparation still
aligns the root version with `node npm/build.mjs --print-version`.

Only one nightly PR is active. Scheduled runs wait for it to merge, including
when its checks are already green. If a port or PR check fails, the workflow
opens a blocker and stops automatic progress. Dispatch with its exact
`retry_version` to repair that nightly. A failed published PR requires manual
dispatch; closing its issue alone does not repair it. An unpublished blocker
can also be closed to allow the next scheduled run to try the current nightly.
Conflicts on an existing PR need manual resolution before retrying.

The worker gets two fresh attempts, first with Kimi K2.7 Code and then with
Kimi K2.6 if the first attempt fails. It runs a dev build, formatting, workspace
checks, Clippy, source/script checks, generated-source and SDK freshness,
compiler conformance in both modes and the scheduling sweep, regressions, and
benchmark snapshot regeneration and parity, published-version behavior,
Node API suites and packed SDK consumers. Dev conformance allows 900 seconds
per fixture for the known slow case documented in [STATUS.md](STATUS.md).
It uses the shared Vite+/Node and Rust setup actions, with the Rust cache
targeting `candidate/target`, and installs Go from the selected source's
`go.mod`. Worker cache saves are disabled.

## Credential isolation

Preparation and publication run trusted automation from the exact main
revision selected at the start of the run. The agent runs on a separate
runner with no repository-write token or GitHub App private key. Its patch,
report and failure log are transferred as artifacts. Publication and blocker
reporting run on a fresh runner; they never execute a file from the agent's
checkout, even when an attempt is rejected.

The publisher applies the patch to Git's index without executing candidate
scripts, rejects protected paths, checks its package/source pins and snapshot
metadata, and creates the commit with Git hooks disabled. Trusted automation
stays in a separate checkout. GitHub credentials are used only in this fresh
publication job. Reports and artifact content are data, never shell commands.

## Repository setup

Install the `oxc-guard` GitHub App with Contents, Issues and Pull requests
write access. Configure these Actions secrets:

| Secret | Purpose |
| --- | --- |
| `CAPTAIN_ID`, `CAPTAIN_KEY` | GitHub App identity used only in the publication job. |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account used for Workers AI, shared with issue triage. |
| `CLOUDFLARE_API_KEY` | Workers AI API token with Workers AI Read and Edit, used only in the worker. |

The agent uses Vite+ to run OpenCode `2.0.32` with the same Workers AI models as
issue triage: `cloudflare-workers-ai/@cf/moonshotai/kimi-k2.7-code` and
`cloudflare-workers-ai/@cf/moonshotai/kimi-k2.6`. OpenCode's native Workers AI
provider uses the account's Chat Completions endpoint directly. See
[Workers AI's OpenAI compatibility](https://developers.cloudflare.com/workers-ai/configuration/open-ai-compatibility/)
and [OpenCode's provider configuration](https://opencode.ai/v2/docs/providers/).
A live port requires these secrets; local smoke checks use stub commands and
never call a model, open an issue or publish a PR.
