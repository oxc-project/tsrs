# Publishing Rust crates

The [Release Crates workflow](../.github/workflows/release_crates.yml) uses
[`cargo-release-oxc` 0.0.44](https://github.com/oxc-project/cargo-release-oxc) with
[`oxc_release.toml`](../oxc_release.toml). It publishes the 34 compiler, language-service and CLI crates in dependency
order. Benchmarks, test helpers, task crates and the WebAssembly package remain unpublished.

Crates share `[workspace.package] version` with the existing npm release. When changing it, update every published
crate's version requirement in `[workspace.dependencies]` to match. Keep the existing npm release preparation and
notes workflow. `cargo-release-oxc update` is not used: it requires explicit package versions instead of inherited
workspace versions.

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

Run **Release Crates** from `main` with `dry_run` enabled to validate the archives. Disable `dry_run` to publish after
the packaging checks pass. Publishing is manual, uses a separate job with the `crates` environment, and never cancels
a release in progress. A rerun skips crate versions already on crates.io, allowing a partial release to resume.
This workflow does not create another npm release or GitHub release.

With suitable crates.io credentials, the equivalent command from a clean checkout is:

```sh
cargo binstall cargo-release-oxc --version 0.0.44 --locked
cargo release-oxc publish --release crates
```
