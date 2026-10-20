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

- A difference from `tsgo` at that commit (diagnostics, their order, message text, exit code) is a tsrs bug, even
  when the tsrs output looks more reasonable. A fix makes tsrs do what the Go code does, not what seems right.
- Port the Go function you are fixing; keep its name, structure and control flow (`docs/PORTING.md`). Do not
  "improve" the algorithm. Internal-only differences (data layout, caching) are fine when no diagnostic changes.
- If the Go code itself is wrong, report it upstream; tsrs follows it until the pinned commit moves.

## Build

Requirements: rustup (it installs the Rust version `rust-toolchain.toml` pins), Node.js 24 (`.node-version`), pnpm 12.10.1 (for `npm/build.mjs` and the JS tooling) and git.
The Fourslash suite also needs Python 3.12+ and Go with `GOTOOLCHAIN=auto` to select the pinned reference's version.
Run `pnpm install --frozen-lockfile` from the repository root to install the npm workspace tooling.

```sh
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner
./target/debug/tsrs -p path/to/project             # like `tsc` (emits; add --noEmit to only type check)
cargo check                                      # must be 0 errors, 0 warnings (CI sets build.warnings=deny)
cargo check -p tsrs_wasm --target wasm32-wasip1   # the WebAssembly build still compiles (notes/wasm-build.md)
cargo clippy --all-targets --config 'build.warnings="deny"'
tools/lint/source.py                              # unsafe Send/Sync inventory, comments on weakened atomic orderings
tools/gen-check.sh                                # generated code matches its generator (change the generator, then --write)
```

Use the `dev` profile (release optimization, no LTO, incremental) while editing and testing: a checker edit rebuilds
`tsrs` and `tsrs-test` in about 5 s instead of about 3 minutes with `--release`, and the conformance suite gives the
same results in the same time (notes/dev-loop-build.md). Build `--release` for performance measurements: its fat LTO
and single codegen unit are what ships.

The language-server test server and WebAssembly module have separate builds:

```sh
cargo build --profile dev -p tsrs_fourslash_server
tools/wasm/build.sh  # requires the wasm32-wasip1 target and wasm-opt
```

## Run the tests

The conformance suite and workspace tests use fixtures, API clients and the AST schema from a checkout of
microsoft/TypeScript at the pinned commit in `ts-ref/` (git-ignored):

```sh
commit=$(node npm/build.mjs --print-typescript-commit)
git clone --filter=blob:none --sparse https://github.com/microsoft/TypeScript ts-ref
git -C ts-ref sparse-checkout set tsc packages/typescript/src \
  packages/typescript/lib packages/typescript/vendor tools/scripts/tsc
git -C ts-ref checkout "$commit"

./target/debug/tsrs-test run --suite all                          # summary table; lists in target/test-results/
./target/debug/tsrs-test run --filter <substring>                 # one test or a cluster
./target/debug/tsrs-test show <suite/name>                        # expected vs actual
./target/debug/tsrs-test run --suite all --baselines types,symbols # type and symbol baselines
PYTHON=python3.12 .github/scripts/go-fourslash-gate.sh             # language-server suite (Go assertions, Rust LSP)
TSRS_TEST=target/debug/tsrs-test .github/scripts/conformance-gate.sh # what CI enforces: errors, .types, .symbols
tools/regressions.sh target/debug/tsrs                            # testdata/regressions (CI runs it too)
tools/ci/determinism.sh --self-test && tools/ci/determinism.sh target/debug/tsrs # checker assignment gate
cargo test --locked                                             # all workspace unit and integration tests (CI)
```

CI runs the gate twice: in tsgo's check history, which the baselines need, and in the default mode
(`TS_TEST_PROGRAM_SINGLE_THREADED=false TSRS_HISTORY=canonical`, where `conformance/objectLiteralNormalization`'s
`.types` / `.symbols` differ by design).

A change must not lose passing tests: compare `target/test-results/pass.txt` before and after
(`comm -23 <(sort before.txt) <(sort target/test-results/pass.txt)` must print nothing). `docs/DEBUGGING.md`
describes the full workflow, the opt-out mode (`TSRS_LAZY_MEMBERS=0`) that must stay reference-identical, and the
Go oracle programs in `tools/oracle/`.

For a bug outside the suite, add a reproduction under `testdata/regressions/<name>/` (`a.ts`, `tsconfig.json`, and
`expected.txt` with `tsc --pretty false` output from the reference compiler).

## Landing changes

- Changes target `main`. CI (`.github/workflows/ci.yml`, on GitHub Actions) runs on pushes
  to `main`, on pull requests (drafts too), on merge groups and by manual dispatch, and must pass. Fork pull requests
  follow the repository's approval setting, run with read-only tokens and never get repository secrets.
- CI has four independent Linux jobs with separate
  Rust caches: `lint` runs Clippy and source checks; `test` runs Rust checks and all workspace tests, compiler conformance in
  both checker modes, and regressions; `determinism` compares checker output across four pinned projects and regression
  cases; `fourslash` runs the server framing checks, gate checks, and
  [Go fourslash subprocess gate](tools/go-fourslash/README.md#ci-gate).
  Source checks overlap Rust setup and Clippy in `lint`. Setup and checkout actions in `test` and `fourslash`
  run sequentially: the runner rewrites the shared GitHub event file when starting steps, so parallel actions
  can read an incomplete payload.
  Autofix removes unused Cargo dependencies and updates the lock file on pull requests; merge groups reject pending fixes.
  Auxiliary workflow definitions preserved as `.github/workflows/*.yml.bak` do not run
  automatically or by manual dispatch. Restore the original `.yml` filename to re-enable one.
- Maintainers land small commits directly on `main` (rebase onto `origin/main`, `cargo check`, re-run the
  affected tests and the full suite, then `git push origin HEAD:main`; never force-push). See `docs/DEBUGGING.md`.
- Commit messages: `<area>: <what and why>`, plus the conformance totals when they change.
- Contributions are licensed under Apache-2.0 (`LICENSE`, section 5).

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues: see [SECURITY.md](SECURITY.md).
