# Publishing Rust crates

The [Release Crates workflow](../.github/workflows/release_crates.yml) uses
[`cargo-release-oxc` 0.0.44](https://github.com/oxc-project/cargo-release-oxc) with
[`oxc_release.toml`](../oxc_release.toml). It publishes every `tsrs_*` workspace crate in dependency order: 43 compiler,
language-service, CLI, WebAssembly, benchmark, test-support and task crates. Validation rejects any excluded `tsrs_*`
member. Test and maintenance binaries operate against a tsrs checkout and its reference compiler, fixtures or tools.

The initial [0.24.0 release](https://crates.io/crates/tsrs_cli/0.24.0) was published locally on 2026-11-11.
Those 34 registry versions and their trusted publishers were read back and verified after publication. The remaining
nine workspace crates are prepared for initial publication with the same version and trusted publisher.

The crates require Rust 1.99 or newer; repository builds use the version pinned in
[`rust-toolchain.toml`](../rust-toolchain.toml).

Crates share `[workspace.package] version` with the existing npm release. When changing it, update every published
crate's version requirement in `[workspace.dependencies]` to match. Keep the existing npm release preparation and
notes workflow. `cargo-release-oxc update` is not used: it requires explicit package versions instead of inherited
workspace versions.

After changing the TypeScript pin, run `node tools/gen-release-version.mjs` (also part of `tools/gen-check.sh`).
Its checked-in outputs let `tsrs_execute` and `tsrs_definitelytyped` build from crate archives without reading the
repository's root manifest.
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

Push coordinated version bumps in both the root `Cargo.toml` (`[workspace.package] version`) and `package.json` to
`main` to trigger the npm and crate release workflows. Set `package.json` to `node npm/build.mjs --print-version` and
update the versioned workspace dependencies in the same push. The crate workflow watches only `Cargo.toml`. The
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
