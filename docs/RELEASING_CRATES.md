# Publishing Rust crates

The [Release Crates workflow](../.github/workflows/release_crates.yml) uses
[`cargo-release-oxc` 0.0.44](https://github.com/oxc-project/cargo-release-oxc) with
[`oxc_release.toml`](../oxc_release.toml). It publishes the 34 compiler, language-service and CLI crates in dependency
order. Benchmarks, test helpers, task crates and the WebAssembly crate are excluded from publication, and validation
rejects accidentally enabling them.

The initial [0.24.0 release](https://crates.io/crates/tsrs_cli/0.24.0) was published locally on 2026-11-11.
All 34 registry versions and their trusted publishers were read back and verified after publication.

The crates require Rust 1.99 or newer; repository builds use the version pinned in
[`rust-toolchain.toml`](../rust-toolchain.toml).

Published crates use explicit versions, as in oxc, so `cargo-release-oxc update` can bump their manifests and every
versioned workspace dependency together. The same release configuration updates the private root `package.json` to
the crate version. `npm/build.mjs` derives the public npm version by adding the pinned TypeScript suffix, for example
`0.26.0-ts7.1.0-beta`. `[workspace.package] version` is only the default for unpublished support crates.

Run [Prepare Release](../.github/workflows/prepare_release.yml) from `main` with a version such as `0.26.0`, without a
leading `v` or TypeScript suffix. Like tsgolint's manual release workflow, it takes an explicit version. It runs
`cargo-release-oxc update`, refreshes `Cargo.lock`, packages and builds the 34 published crates, and opens a release PR.
It preserves existing release notes or generates a first draft in `docs/releases/<version>.md` for review. The PR
uses the same `CAPTAIN_ID` / `CAPTAIN_KEY` GitHub App credentials as TypeScript sync, so CI runs on it.

For local preparation from a clean checkout, use:

```sh
cargo binstall cargo-release-oxc --version 0.0.44 --locked
mkdir -p target
cargo release-oxc update --release crates --version 0.26.0
cargo check
node tools/release/crates.mjs --allow-dirty
```

`cargo check` refreshes `Cargo.lock` after the bump. Commit the version changes with the release notes in
`docs/releases/<version>.md`. The explicit `--version` works before any `crates_v*` tags exist.

After changing the TypeScript pin, run `node tools/gen-release-version.mjs` (also part of `tools/gen-check.sh`).
Its checked-in output lets `tsrs_execute` build from a crate archive without reading the repository's root manifest.
The `tsrs_vfs` archive includes the standard TypeScript libraries, so the Cargo CLI needs no separate library install.
Every published archive also carries the repository's license and attribution notices.

## Validate

```sh
node tools/release/crates.mjs --check       # metadata, versions and TypeScript pin
node tools/release/crates.mjs               # package and compile all publishable crates
```

Use `--allow-dirty` while preparing changes locally, or `--no-verify` to inspect archives without compiling them.
Cargo packages the crates together using its temporary registry, so this works before the first release. Archives
are written to `target/package`. `cargo-release-oxc publish --dry-run` checks crates individually and requires their
dependencies to have been published already.

## Publish

The GitHub `crates` environment allows deployments only from `main`. The workflow uses the same crates.io OIDC
authentication action as oxc and needs no long-lived publishing secret. Each crate must trust owner `oxc-project`,
repository `tsrs`, workflow `release_crates.yml`, environment `crates`.

[crates.io requires an API token for initial publication](https://crates.io/docs/trusted-publishing). When adding a
new crate, publish it locally with a suitable token, then configure its trusted publisher with the values above.
Keep bootstrap credentials local.

Push the changes produced by `cargo release-oxc update` in both root `Cargo.toml` and `package.json` to `main` to trigger
the npm and crate release workflows. The crate workflow watches only `Cargo.toml`. The
version gate compares against the commit before the whole push and rejects a bump to only one version. Other
manifest edits leave publication skipped. The crate workflow packages and builds every archive before publishing.

To validate manually, run **Release Crates** from `main` with `dry_run` enabled. Disable `dry_run` for a manual publish
or retry. Publishing uses the `crates` environment and never cancels a release in progress. A rerun skips crate
versions already on crates.io, allowing a partial release to resume.
This workflow does not create another npm release or GitHub release.

With suitable crates.io credentials, the equivalent command from a clean checkout is:

```sh
cargo binstall cargo-release-oxc --version 0.0.44 --locked
node -e 'require("node:fs").mkdirSync("target", { recursive: true })'
cargo release-oxc publish --release crates
```

The tool writes `target/CRATES_VERSION` even when `CARGO_TARGET_DIR` points elsewhere, so create `target` before
publishing from a fresh checkout.
