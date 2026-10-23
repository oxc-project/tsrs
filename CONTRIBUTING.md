# Contributing to tsrs

tsrs is a Rust port of the TypeScript 7 type checker (the Go code in microsoft/TypeScript, `tsc/internal`).

Bug reports are welcome in the [issue tracker](https://github.com/oxc-project/tsrs/issues). Include a minimal
reproduction, the tsrs and reference compiler versions, the command you ran, and the expected and actual output.

Pull requests remain closed because we don't have the bandwidth to review them. This guide covers building from
source, reproducing bugs and the checks required for changes.

The port was written largely by AI coding agents, directed and reviewed by Max Schwenk. Its correctness rests on
the [conformance results](docs/COMPATIBILITY.md#conformance) rather than on review alone.

## The fidelity rule: Go is the spec

The Go implementation at the commit in `Cargo.toml` (`[workspace.metadata.typescript]`) is the specification.
Every reproduction and fix must use that exact reference. [Prepare and verify the reference compiler](#reference-compiler)
before comparing behavior or generating expected output. Use the same input files, configuration, working directory
and compiler flags on both sides.

- A difference from `tsgo` at that commit (diagnostics, their order, message text, exit code) is a tsrs bug, even
  when the tsrs output looks more reasonable. A fix makes tsrs do what the Go code does, not what seems right.
- Port the Go function you are fixing; keep its name, structure and control flow (`docs/PORTING.md`). Do not
  "improve" the algorithm. Internal-only differences (data layout, caching) are fine when no diagnostic changes.
- If the Go code itself is wrong, report it upstream; tsrs follows it until the pinned commit moves.

## Build

Requirements: rustup (it installs the Rust version `rust-toolchain.toml` pins), Node.js 24 (`.node-version`), pnpm 12.15.0 (for `npm/build.mjs` and the JS tooling) and git.
The Fourslash suite also needs Node.js 24+ and Go with `GOTOOLCHAIN=auto` to select the pinned reference's version.
Run `pnpm install --frozen-lockfile` from the repository root to install the npm workspace tooling.

Use `pnpm run` at the repository root to list tooling commands. The [script catalog](tools/SCRIPTS.md) covers generators,
oracles, benchmarks, watch checks and maintenance tools. Scripts use Node.js 24 or newer; Python is not required.

```sh
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner
./target/debug/tsrs -p path/to/project             # like `tsc` (emits; add --noEmit to only type check)
cargo check                                      # must be 0 errors, 0 warnings (CI sets build.warnings=deny)
cargo check -p tsrs_wasm --target wasm32-wasip1   # the WebAssembly build still compiles (notes/wasm-build.md)
cargo clippy --all-targets --config 'build.warnings="deny"'
tools/lint/source.mjs                              # unsafe Send/Sync inventory, comments on weakened atomic orderings
tools/gen-check.sh                                # generated code matches its generator (change the generator, then --write)
```

Use the `dev` profile while editing and testing. It uses Cargo's default development settings with debug information
disabled. CI uses the optimized `ci` profile without LTO for workspace tests, compiler conformance and determinism.
Build `--release` for performance measurements: its fat LTO and single codegen unit are what ships.

The language-server test server and WebAssembly module have separate builds:

```sh
cargo build --profile dev -p tsrs_fourslash_server
tools/wasm/build.sh  # requires the wasm32-wasip1 target and wasm-opt
```

## Reference compiler

Reproductions, fixes, tests and generators use a checkout of microsoft/TypeScript at the pinned commit in
`ts-ref/` (git-ignored). Create it once:

```sh
commit=$(node npm/build.mjs --print-typescript-commit)
git clone --filter=blob:none --sparse https://github.com/microsoft/TypeScript ts-ref
git -C ts-ref sparse-checkout set tsc packages/typescript/src \
  packages/typescript/lib packages/typescript/vendor tools/scripts/tsc
git -C ts-ref checkout --detach "$commit"
```

For a new or existing checkout, verify the source pin and build the Go reference from it:

```sh
commit=$(node npm/build.mjs --print-typescript-commit)
test "$(git -C ts-ref rev-parse HEAD)" = "$commit"
git -C ts-ref diff --exit-code HEAD -- tsc
mkdir -p target/reference
reference_bin="$PWD/target/reference/tsgo"
(cd ts-ref/tsc && GOTOOLCHAIN=auto GOWORK=off go build -o "$reference_bin" ./cmd/tsc)
"$reference_bin" --version
```

Both source checks must pass before building or reproducing a bug. Keep tracked reference sources unchanged;
[Go overlays](docs/DEBUGGING.md#where-go-reports-an-error-an-instrumented-reference-with-go-build--overlay) support
instrumentation. Rebuild the reference after the pin changes; its version string alone does not identify the commit.
The checkout stays at the pinned revision until explicitly updated. `Cargo.toml` records the matching TypeScript
package version; [the compatibility guide](docs/COMPATIBILITY.md) distinguishes the source pin from published releases.

## Run the tests

With the [reference checkout](#reference-compiler) ready:

```sh
./target/debug/tsrs-test run --suite all --baselines all          # every supported baseline; lists in target/test-results/
./target/debug/tsrs-test run --filter <substring>                 # one test or a cluster
./target/debug/tsrs-test show <suite/name>                        # expected vs actual
./target/debug/tsrs-test run --suite all --baselines types,symbols # type and symbol baselines
.github/scripts/go-fourslash-gate.sh             # language-server suite (Go assertions, Rust LSP)
TSRS_TEST=target/debug/tsrs-test .github/scripts/conformance-gate.sh # errors, types, symbols, emit and source maps
tools/regressions.sh target/debug/tsrs                            # testdata/regressions (CI runs it too)
cargo run --profile dev -p tsrs_determinism -- check --tsrs target/debug/tsrs # committed tsgo snapshots
cargo test --locked                                             # all workspace unit and integration tests (CI)
```

CI and release BOLT gates compare every supported compiler baseline in both tsgo's checker history and the default
mode (`TS_TEST_PROGRAM_SINGLE_THREADED=false TSRS_HISTORY=canonical`). `--baselines all` includes errors, types,
symbols, JavaScript/declaration emit, JavaScript maps and source-map records. The runner exits unsuccessfully on any
baseline difference, crash, timeout or empty selection. The gate also rejects a testdata checkout at a different
TypeScript source pin. Unsupported cases remain reported as skips.

The Node API job verifies generated compiler sources and the vendored SDK, then runs all existing upstream client
and local parity cases against both the Go oracle and tsrs.

A change must not lose passing tests: compare `target/test-results/pass.txt` before and after
(`comm -23 <(sort before.txt) <(sort target/test-results/pass.txt)` must print nothing). `docs/DEBUGGING.md`
describes the full workflow, the opt-out mode (`TSRS_LAZY_MEMBERS=0`) that must stay reference-identical, and the
Go oracle programs in `tools/oracle/`.

For a bug outside the suite, add a reproduction under `testdata/regressions/<name>/` (`a.ts`, `tsconfig.json`, and
`expected.txt`). Run the pinned reference binary from the case directory with `-p . --pretty false --singleThreaded`.
Capture its combined stdout and stderr, replacing the case's absolute directory path with `<dir>` to match
`tools/regressions.sh`.

## Landing changes

- Changes target `main`. CI (`.github/workflows/ci.yml`, on GitHub Actions) runs on pushes
  to `main`, on pull requests (drafts too), on merge groups and by manual dispatch, and must pass. Fork pull requests
  follow the repository's approval setting, run with read-only tokens and never get repository secrets.
- CI builds the CLI and conformance runner once with `--profile ci` in `build-correctness`; conformance, regressions
  and determinism use those same binaries. The Linux checks have separate Rust caches: `lint` runs Clippy and source checks;
  `test` runs Rust checks and all workspace tests, compiler conformance in
  both checker modes, and regressions; `determinism` compares every benchmark project against committed snapshots
  from the root pnpm TypeScript release matching Cargo metadata, using Go, locality and default scheduling at four and
  all available CPU checkers, three runs each, plus three fixed random assignments per count. It also sweeps every
  regression fixture under default and seeded random scheduling at two and four checkers. Diagnostic order and exit
  status must match; crashes and incomplete runs fail.
  See [the determinism task](tasks/determinism/README.md) for snapshot updates. Regression output must still match
  the checked-in reference output; `fourslash` runs the server framing checks, gate checks, and
  [Go fourslash subprocess gate](tools/go-fourslash/README.md#ci-gate); `node-api-clients` runs the pinned upstream
  sync and async client suites against both compilers.
  Source checks overlap Rust setup and Clippy in `lint`. Setup and checkout actions in `test` and `fourslash`
  run sequentially: the runner rewrites the shared GitHub event file when starting steps, so parallel actions
  can read an incomplete payload.
  `benchmark.yml` asks Clef whether a push or PR needs measurement, then builds all release benchmark artifacts in one
  job before sharding CodSpeed and PR comparisons. Cross-compiler reports run only by manual dispatch and remain in
  workflow artifacts without opening update PRs. Manual dispatch forces measurement; unavailable Clef decisions run
  benchmarks. PR comparisons and cross-compiler reports share the plain release CLI. Measured main
  pushes cache it for the exact commit and build configuration; misses build the base in the same job. Benchmark project
  and Node API harness caches are saved on main pushes and restored by PRs.
  Autofix removes unused Cargo dependencies and updates the lock file on pull requests; merge groups reject pending fixes.
  Auxiliary workflow definitions preserved as `.github/workflows/*.yml.bak` do not run
  automatically or by manual dispatch. Restore the original `.yml` filename to re-enable one.
- Maintainers land small commits directly on `main` (rebase onto `origin/main`, `cargo check`, re-run the
  affected tests and the full suite, then `git push origin HEAD:main`; never force-push). See `docs/DEBUGGING.md`.
- Commit messages: `<area>: <what and why>`, plus the conformance totals when they change.
- Contributions are licensed under Apache-2.0 (`LICENSE`, section 5).

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues: see [SECURITY.md](SECURITY.md).
