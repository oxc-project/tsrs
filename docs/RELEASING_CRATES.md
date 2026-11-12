# Preparing and publishing releases

Releases use [`cargo-release-oxc` 0.0.44](https://github.com/oxc-project/cargo-release-oxc) with
[`oxc_release.toml`](../oxc_release.toml). The 34 compiler, language-service and CLI crates are published to crates.io;
benchmarks, test helpers, task crates and the WebAssembly crate are excluded.

## Prepare a release

1. Run [Prepare Release](../.github/workflows/prepare_release.yml) from `main` with the next TypeScript-aligned version,
   such as `7.1.1-beta.2`, without a leading `v`.
2. Review the generated PR. It bumps Cargo and npm versions together, refreshes `Cargo.lock`, packages and builds
   every published crate, and generates `docs/releases/<version>.md` if notes do not already exist.
3. Merge the PR to trigger [Release Crates](../.github/workflows/release_crates.yml) and
   [Release](../.github/workflows/release.yml), which publish the crates and npm packages respectively. The npm
   workflow also creates the GitHub release.

Published crate manifests, workspace dependency versions, the private root `package.json` and all generated npm
packages share one version managed by `cargo-release-oxc update`. `[workspace.package] version` supplies the default
for unpublished support crates.

Both publishing workflows require Cargo and root npm versions to change in the same push to `main`. The crate
workflow watches only root `Cargo.toml`; unrelated manifest edits skip publication.

## Versioning

Following [#961](https://github.com/oxc-project/tsrs/issues/961), stable versions use the TypeScript major and minor
versions, with patch `TypeScript patch × 1000 + tsrs counter`. The counter starts at `001` and increments for each
tsrs fix against the same TypeScript release:

| TypeScript reference | First tsrs release | Next tsrs fix |
| --- | --- | --- |
| `7.0.2` | `7.0.2001` | `7.0.2002` |
| `7.0.3` | `7.0.3001` | `7.0.3002` |
| `7.1.0-beta` | `7.1.1-beta.1` | `7.1.1-beta.2` |
| `7.1.0-dev.20261112` | `7.1.1-alpha.20261112.1` | `7.1.1-alpha.20261112.2` |

SemVer omits leading zeros, so TypeScript patch zero with counter `001` becomes patch `1`. Stable counters stop at
`999`; the version checks reject rollover because it would overlap the next TypeScript patch's range. Upgrade the
TypeScript reference to start a new range.

For prereleases, the combined patch stays at `TypeScript patch × 1000 + 1`, and the final prerelease identifier is
the tsrs counter. Preserve TypeScript's `alpha`, `beta` or `rc` identifiers; map nightly `dev` to `alpha` so nightlies
sort before beta, rc and stable releases. A new upstream prerelease or nightly date resets the final counter to `1`.
Every release must increase in SemVer order. For example, `7.1.1-beta.2` precedes stable `7.1.1`.

Prereleases publish to npm's `next` tag and create a GitHub prerelease. Stable releases use npm's `latest` tag and
the latest GitHub release. Tags and notes use the same version: `v<version>` and `docs/releases/<version>.md`.

The first release under this scheme is `7.1.1-beta.1`, replacing `0.25.0-ts7.1.0-beta` for npm packages. Cargo uses
the same version, and all native platform and WebAssembly npm packages move together. The pinned TypeScript version,
matching npm compiler package and exact source commit remain in `[workspace.metadata.typescript]`; generated npm
packages expose the version and commit through their `tsrs` metadata. Compiler compatibility uses that TypeScript
version, independently of the tsrs counter.

## Prepare locally

From a clean checkout:

```sh
cargo binstall cargo-release-oxc --version 0.0.44 --locked
mkdir -p target
cargo release-oxc update --release crates --version 7.1.1-beta.2
cargo check
node tools/release/crates.mjs --allow-dirty
```

Commit the version changes, updated lockfile and release notes in `docs/releases/<version>.md` together.
After changing the TypeScript pin, regenerate its release metadata with `node tools/gen-release-version.mjs`.

## Validate and retry

```sh
node tools/release/crates.mjs --check  # Check metadata, versions and the TypeScript pin
node tools/release/crates.mjs          # Package and compile all published crates
```

Use `--allow-dirty` for uncommitted changes or `--no-verify` to inspect archives without compiling them. Archives
are written to `target/package`; Cargo verifies unpublished dependency versions together using a temporary registry.

Run **Release Crates** manually from `main` with `dry_run` enabled to validate without publishing. Disable it to
publish or retry a partial release. Releases run serially, and retries skip versions already on crates.io.

## Publishing configuration

The GitHub `crates` environment permits deployments only from `main`. Crates.io trusted publishing uses OIDC with
these settings for each crate:

- Repository owner: `oxc-project`
- Repository name: `tsrs`
- Workflow filename: `release_crates.yml`
- Environment: `crates`

[Initial publication requires a crates.io API token](https://crates.io/docs/trusted-publishing). Publish a new crate
locally, then configure its trusted publisher. With local credentials and `cargo-release-oxc` installed:

```sh
mkdir -p target
cargo release-oxc publish --release crates
```

Keep the local `target` directory even with a custom `CARGO_TARGET_DIR`: the tool writes `target/CRATES_VERSION` there.
The prepare workflow uses the same `CAPTAIN_ID` / `CAPTAIN_KEY` GitHub App credentials as TypeScript sync so its PRs
trigger CI.
