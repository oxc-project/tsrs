# TypeScript 7.1 beta follow-up ports

The 112 commits in [issue #623](https://github.com/oxc-project/tsrs/issues/623) end at
`5237fd46890956e4bb8177a1a05dcd69e4166560` and retain their original evidence in
[the source-upgrade log](typescript-623-ports.md). The upstream `v7.1-beta` tag is
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`. It merges the release branch's
`942a59d3081b005869bc2938953520b3f22b4702` version bump with the final main-branch
compiler state; the remaining commits on the tag's first-parent path change CI workflow
files only. These follow-ups have separate corresponding tsrs commits in upstream order.

## 1. `942a59d308` — Bump version to 7.1.0-beta

The upstream release branch changes `tsc/internal/core/version.go` from `7.1.0-dev` to
`7.1.0-beta` and sets the native preview release version in `Herebyfile.mjs`. Comparing
the prior pin with the final tag under `tsc/` and `packages/typescript/` shows only that
Go version-constant change; there are no compiler algorithm, SDK, or testdata changes.
The published `typescript@7.1.0-beta` package's `gitHead` is the final tag, so this
step advances the active source pin directly to the tag while retaining a corresponding
commit for each remaining workflow-only upstream commit.

Rust's unstamped version default is now `7.1.0-beta`. `Cargo.toml`, the npm package
version and lockfile, benchmark reference, vendored SDK manifest, generated SDK method
inventory, and current release-facing documentation all name the beta tag and package.
The SDK copy remains byte-identical, and `sync-upstream.mjs --check` validates its 194
files against the tag. The pnpm lockfile's first resolution omitted the beta package's
declared native optional dependencies, so its `tsc` launcher could not run. A pnpm
`--fix-lockfile` resolution restored them and added the two minimum-release-age
exceptions required by the fresh native package versions. A frozen reinstall and
`node npm/build.mjs --sdk-only` then passed. The `tsrs_execute` build script now
canonicalizes the workspace `Cargo.toml` path before registering its rerun trigger:
Cargo had reused a build-script output from another worktree, stamping the old source
commit even though the compiler core had rebuilt. Both dev and release CLI binaries
now report `Version 7.1.0-beta (tsrs 0.19.0-ts7.1.0-beta,
microsoft/TypeScript@3e9236361fa9)`; the exact-tag Go binary reports
`Version 7.1.0-beta`.

The exact-beta full compiler suite passes all 13,700 diagnostic, 12,998 `.types`,
12,998 `.symbols`, 13,626 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines, with zero failures, crashes, or timeouts. The upstream SDK
suite passes 968/968 tests against both Go and tsrs, and its freshly generated
177-method report matches `npm/sdk/METHODS.md` byte for byte. Source lint and
format checks pass. Warnings-denied workspace Clippy and the pinned Node
client API-transport round trip also pass.

The pre-port source-112 and beta release binaries have SHA-256 values
`d3702668639dbeee0ac0a58fe831783199f962efb49bcf358df8202bd227175a`
and `14de5d279e5946f7c51747ccc7fe61d4bc1872869393234eb33ba92f0a329fc2`.
Five alternating pairs on three public projects preserved identical output
and exit status in both modes. macOS `/usr/bin/time -l` measured instructions
retired and peak RSS; wall time includes process launch. Medians are:

| Project | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.224 → 3.223 | 58.98 → 59.00 | 0.230 → 0.229 |
| Compiler-Unions | default | 3.918 → 3.913 | 82.45 → 81.91 | 0.114 → 0.112 |
| webpack | single | 10.825 → 10.832 | 306.33 → 306.34 | 0.974 → 0.973 |
| webpack | default | 15.211 → 15.187 | 497.23 → 495.63 | 0.163 → 0.172 |
| VS Code | single | 81.370 → 81.365 | 1586.53 → 1586.56 | 7.938 → 7.954 |
| VS Code | default | 93.424 → 93.549 | 2090.00 → 2085.73 | 0.996 → 1.010 |

The largest instruction increase is 0.134% and the largest peak-RSS increase
is 0.026%, both below the repository's regression threshold. webpack/default
showed +5.388% wall time in the first short run, with 4/5 positive pairs
(one-sided sign-test probability 0.1875). A reversed ten-pair confirmation
measured 0.184 → 0.177 s (−3.990%), 15.190 → 15.232 billion instructions
(+0.280%), and 495.47 → 498.34 MiB peak RSS (+0.579%). The wall increase
did not reproduce, and no new performance regression was detected. The
deterministic Linux `bench/count.mjs` counter is unavailable on this macOS
host; macOS instruction counts include kernel work. Raw samples and matching
output hashes are under `target/port-623/beta-942a59d308/perf/`.

## 2. `bf8a764721` — Use supported MicroBuild source-analysis pool

Upstream changes `sourceAnalysisPool` from `VSEngSS-MicroBuild2022-1ES` to
`VSEng-MicroBuildVSStable` in four Microsoft Azure build/publish pipeline YAML
files; one file also loses its executable bit. tsrs has no corresponding Azure
or MicroBuild pipeline, so this is an evidence-only port. The upstream `tsc/`
tree is `affae8248f115df1204769ecf61285cc1a1eec04` on both sides of the
commit, and `packages/typescript/` is likewise identical. The active beta tag
was already pinned in entry 1; no local compiler, SDK, fixture, generated
asset, or runtime input changes here.

Entry 1's exact-beta compiler, SDK, workspace lint, and API-transport checks
therefore remain applicable, with all passing conformance sets unchanged. The
same release binary (SHA-256
`14de5d279e5946f7c51747ccc7fe61d4bc1872869393234eb33ba92f0a329fc2`)
and bundled runtime assets are used, so its recorded instructions-retired,
wall-time, and peak-RSS results also apply without another timing run. The
local change is this evidence entry only.

## 3. `d7d39da5c4` — Include `weswigham/*` in TypeScript CI

Upstream adds `weswigham/*` to the push, pull-request, and merge-group branch
filters in `.github/workflows/ci.yml`. Those are TypeScript-specific branches;
tsrs CI has no corresponding filter to update. The upstream `tsc/` tree is
`affae8248f115df1204769ecf61285cc1a1eec04` before and after this commit,
and `packages/typescript/` is likewise identical. The beta tag was pinned in
entry 1, and this evidence entry changes no local compiler, SDK, fixture,
generated asset, or runtime input.

Entry 1's exact-beta conformance, SDK, lint, and API-transport results retain
the same inputs and passing sets. The release binary is unchanged at SHA-256
`14de5d279e5946f7c51747ccc7fe61d4bc1872869393234eb33ba92f0a329fc2`;
its bundled runtime assets are unchanged too. Its recorded instructions-retired,
wall-time, and peak-RSS measurements therefore apply without a new timing run.

## 4. `3e9236361f` — Merge main into `release-7.1` (`v7.1-beta`)

The beta tag merges `942a59d308` with `d7d39da5c4`. The combined merge diff is
empty: there are no conflict-resolution edits. Compared with the original
`5237fd4689` pin, the tag changes only `tsc/internal/core/version.go` under
`tsc/` and `packages/typescript/`, replacing `7.1.0-dev` with `7.1.0-beta`.
The SDK source tree is identical at both commits
(`3dc85d2e25450bf76fdf45c426b89c762e9ca71e`). Compared with the merge's
main-branch parent, only that Go version constant and the release branch's
fixed `7.1.0-beta` setting in `Herebyfile.mjs` differ. The published
`typescript@7.1.0-beta` package reports this tag as its `gitHead`.

Entry 1 already pins and validates this exact tag, so the merge requires no
additional Rust, SDK, or fixture changes. The exact-beta compiler, SDK, lint,
and API-transport checks recorded there retain the same inputs and passing
results. The release binary is still SHA-256
`14de5d279e5946f7c51747ccc7fe61d4bc1872869393234eb33ba92f0a329fc2`;
its bundled runtime assets are unchanged, so the instructions-retired,
wall-time, and peak-RSS measurements also remain applicable. This entry is
the corresponding evidence-only port of the upstream merge.

## Follow-up: refresh the module resolver oracle for the beta version

The full workspace test exposed one stale handcrafted module resolver fixture:
`typesVersions` resolution traces still named `7.1.0-dev`. The Go oracle
adapter was updated for the beta source's rooted-path and case-sensitivity
types, then built against the exact `3e9236361fa9` tag. Regenerating all 18
handcrafted scenarios changed only four trace arguments in that one scenario
from `7.1.0-dev` to `7.1.0-beta`; resolved files, diagnostics, automatic types,
and every other trace were unchanged. The refreshed fixture matches the Go
oracle byte for byte, and `cargo test --locked -p tsrs_module oracle_handcrafted`
passes. The full `tsrs_module` crate has 36 passing tests and one pre-existing
ignored corpus test; warnings-denied workspace Clippy, source lint, `gofmt`,
and `git diff --check` pass. This follow-up changes test tooling and expected
output only, so the compiler binary and performance evidence above remain
applicable. Other `7.1.0-dev` references in the codec and Node API goldens are
historical or soft expectations; release notes and benchmark records retain
their original versions.
