# Preparing and publishing releases

Releases use [`cargo-release-oxc` 0.0.44](https://github.com/oxc-project/cargo-release-oxc) with
[`oxc_release.toml`](../oxc_release.toml). The 34 compiler, language-service and CLI crates are published to crates.io;
benchmarks, test helpers, task crates and the WebAssembly crate are excluded.

## Prepare a release

1. Run [Prepare Release](../.github/workflows/prepare_release.yml) from `main` with a newer version such as `0.26.0`,
   without a leading `v` or TypeScript suffix.
2. Review the generated PR. It bumps Cargo and npm versions together, refreshes `Cargo.lock`, packages and builds
   every published crate, and generates `docs/releases/<version>.md` if notes do not already exist.
3. Merge the PR to trigger [Release Crates](../.github/workflows/release_crates.yml) and
   [Release](../.github/workflows/release.yml), which publish the crates and npm packages respectively. The npm
   workflow also creates the GitHub release.

Published crates have explicit versions managed by `cargo-release-oxc update`. Their manifests, workspace dependency
versions and the private root `package.json` share the crate version. `npm/build.mjs` adds the pinned TypeScript
suffix to public npm versions, for example `0.26.0-ts7.1.0-beta`. `[workspace.package] version` supplies the default
for unpublished support crates.

Both publishing workflows require Cargo and root npm versions to change in the same push to `main`. The crate
workflow watches only root `Cargo.toml`; unrelated manifest edits skip publication.

## Prepare locally

From a clean checkout:

```sh
cargo binstall cargo-release-oxc --version 0.0.44 --locked
mkdir -p target
cargo release-oxc update --release crates --version 0.26.0
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
