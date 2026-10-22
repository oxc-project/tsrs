# Continuous TypeScript ports

`.github/workflows/port-typescript.yml` ports upstream commits in topological
order, one commit per run. It starts after the completed mainline port
`5237fd46890956e4bb8177a1a05dcd69e4166560`. The historical beta release
commits are already recorded by `Upstream-Commit` trailers and are skipped if
they later appear in the upstream `main` ancestry. The workflow derives the
queue from the latest `microsoft/TypeScript` `main` graph and trailers on the
current tsrs branch, so it does not depend on a separate queue service.

The first run creates a draft `chore(port):` PR. Later runs append exactly one
tsrs commit per selected upstream SHA to the same PR. Each commit has an
`Upstream-Commit` trailer and an entry in
[`notes/typescript-continuous-ports.md`](../notes/typescript-continuous-ports.md).
If that PR is merged, the next run
starts a new branch from `main`; the trailers survive a normal merge, while
the ledger keeps the history even if the PR is squash merged. The published
TypeScript package pin in `Cargo.toml` stays on an available matching package
until a complete source upgrade is ready. CI on the rolling PR reads its latest
ledger SHA for upstream fixtures and Go oracle checks; main and other PRs keep
using the published Cargo pin. Both the Node API and Fourslash harnesses receive
that selected SHA through `TS_REFERENCE_COMMIT`, so they use the same source as
the CI checkout.

The workflow runs hourly at minute 17 or by manual dispatch. It waits for all
PR checks to pass before selecting another SHA. Each AI port gets two fresh
attempts. The runner then checks formatting, a dev build, workspace tests,
Clippy, source lint, and the
full compiler conformance gate against that exact upstream checkout. A failed
port or PR check opens a `Port blocked` issue and stops the queue. After fixing
the failure, dispatch the workflow with the full `retry_sha`, or close the
issue to allow the next scheduled run to retry. The next scheduled run resumes
the queue once CI passes.

## Repository setup

Install the `oxc-guard` GitHub App with Contents, Issues, and Pull requests
write access. Set these Actions secrets:

| Secret | Purpose |
| --- | --- |
| `CAPTAIN_ID`, `CAPTAIN_KEY` | GitHub App identity, shared with the existing triage workflow. |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account that owns the AI Gateway. |
| `CLOUDFLARE_GATEWAY_ID` | AI Gateway slug. |
| `CLOUDFLARE_AI_GATEWAY_TOKEN` | Authenticated gateway token with Run permission. |

The gateway needs Unified Billing credits or a stored OpenAI provider key with
the `default` alias. The agent uses OpenCode `2.0.32` with
`openai/gpt-6.1-sol` through the gateway's OpenAI Responses endpoint. Gateway
credentials are only passed to the agent step; the GitHub App token is minted
after that step for publication. The agent's configuration stays on the
ephemeral runner. See [Cloudflare's gateway setup](https://developers.cloudflare.com/ai-gateway/integrations/coding-agents/opencode/)
and [OpenCode's provider configuration](https://opencode.ai/v2/docs/providers/)
for the service settings.

To inspect the selected CI reference locally without contacting either service,
run `node tools/port/reference-commit.mjs`. To start a port manually, dispatch
**Port TypeScript commits** in GitHub Actions. Repository secrets and an
available gateway balance are required for a live run.
