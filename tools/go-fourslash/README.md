# Go fourslash against the Rust language server

This prototype runs the pinned TypeScript Go fourslash test bodies and assertions unchanged against a separate
Rust process. It does not compile the generated Rust tests or use `tsrs_fourslash_harness`.

Requirements: Python 3.12+, Rust, Go (with `GOTOOLCHAIN=auto`), and macOS or Linux. The TypeScript Git checkout must
contain the commit pinned in `Cargo.toml`; `--reference` defaults to `ts-ref`.

```sh
python3.12 tools/go-fourslash/run.py -- -run '^TestQuickInfoAlias$' -v
python3.12 tools/go-fourslash/run.py -- -json > target/go-fourslash/results.jsonl
python3.12 tools/go-fourslash/summarize.py target/go-fourslash/results.jsonl
```

Use `--reference /path/to/TypeScript` to read another checkout, `--work /path/to/scratch` for a separate workspace,
or `--server /path/to/fourslash_server` to use a prebuilt server. These options go before `--`; ordinary `go test`
flags go after it. The default concurrency is four, each test has a two-minute subprocess timeout, and the whole
suite has a thirty-minute timeout. `-count=1` disables cached results.

The runner extracts the pinned commit into `target/go-fourslash/reference`, then uses a Go build overlay to adapt
three harness files and add `subprocess.go` and `remotestate.go`. It never patches the source checkout, test bodies,
or reference baselines. Go's ordinary baseline verifier writes differences under the extracted checkout's
`tsc/testdata/baselines/local/`. The runner returns `go test`'s exit status.

## Process boundary

```text
Go test body → upstream Go Fourslash assertions → LSP stdin/stdout → Rust language server
                    ↑                                                   │
                    └──── original Go virtual filesystem ← Unix socket ─┘
```

The Rust process is the `tsrs_cli` example `fourslash_server`, a test-only entry point around `tsrs_lsp::new_server`.
It is not the production `tsrs --lsp -stdio` executable. Standard LSP has no operation for installing a virtual
filesystem, and the pinned Go harness also sets inferred-project compiler options and waits for initialization
through internal calls. A private Unix socket carries those setup operations and filesystem callbacks; all
editor requests and responses use the existing framed LSP transport.

For `@stateBaseline` cases, a second private Unix socket returns read-only project snapshots after LSP requests
complete. Go retains its upstream state comparison and formatting, using remote views of projects, source files,
default-project assignments, and config retention. Program and source-file handles preserve identity across
snapshots; the Rust adapter keeps the previous project owners alive until the next snapshot is captured.
Filesystem differences still come from Go's original filesystem. The `@tsc` setup commands run through Go's
existing `GetFileMapWithBuild` helper, then the Rust server reads the resulting declaration files and maps.

The Go filesystem remains authoritative, including bundled libraries, symlinks, case sensitivity, writes, and
renames. Paths are not relocated or rewritten. Compiler options use the existing Go-compatible options codec.
The example selects the same Go checker-history mode as `tsrs-fourslash`, including its environment overrides.
Each test starts a fresh process; cleanup closes stdin, drains stdout, waits for exit, and kills a hung child.

## Remaining scope

- Tests that supply Go content-mapper callbacks are explicitly skipped. The subprocess cannot invoke those
  callbacks; the current Rust LSP server also does not support them.
- Existing upstream skips remain intact. Skips are not counted as passes.
- Windows transport and production CLI startup are not covered by this prototype.

The Go runner supplies the Linux CI gate and current compatibility count. The translated Rust suite remains
in place for release verification and PGO/BOLT training; those workflows are unchanged.

The summary counts a parent with only skipped subtests as skipped: Go itself reports such a parent as passing.
Pass `--rust-results target/fourslash-results` to the summary to compare exercised groups with the translated suite.

## CI gate

The active Linux test job in `.github/workflows/ci.yml` checks out the pinned `tsc` source and fixtures, installs
Go from its `go.mod` and Python 3.12, builds the server example with the dev profile, and runs:

```sh
cargo build --profile dev --locked -p tsrs_cli --example fourslash_server
PYTHON=python3.12 .github/scripts/go-fourslash-gate.sh
```

The gate requires a completed, successful Go package result, zero failing tests, and at least **4,074 exercised
groups**. It excludes parents whose subtests all skipped, including nested groups, so additional skips cannot
silently reduce coverage. CI also runs the summary's regression checks. Go's build cache is shared with
`actions/setup-go` through `GOCACHE` at `target/go-fourslash/go-cache`.

On failure, CI uploads `results.jsonl`, `runner.log`, and baseline differences as the `go-fourslash-results`
artifact. For a local run, `PYTHON`, `TSRS_FOURSLASH_SERVER`, and `TSRS_GO_FOURSLASH_WORK` can select another Python
executable, prebuilt server, and work directory. `MIN_PASS` overrides the gate's coverage floor; raise the default
when support improves.

## Validation (2026-10-18)

At pinned TypeScript commit `b85298b6a81f772d080b0455de0ca9d744cd6fd6`, on macOS arm64:

| Runner | Passing groups | Failing groups | Skipped groups |
| --- | ---: | ---: | ---: |
| Existing translated Rust suite | 4,066 | 63 | 417 |
| Go tests with Rust subprocess | 4,074 | 0 | 472 |

All 4,066 groups passing in the translated suite are exercised successfully, including the 18 project-state
groups skipped by the initial prototype. Eight additional groups pass using Go's `@tsc` setup support. No passing
parent contains only skipped subtests. The 472 skipped groups comprise the existing 417 skips and 55 unsupported
content-mapper groups. The eight additional passes exercise Rust LSP behavior with Go-built fixtures, not Rust emit.

Also checked: a child that immediately exits reports a failure promptly; fresh pinned-source extraction and
reuse preserve the original harness and test files; the dev builds, workspace Clippy, and source checks pass.
