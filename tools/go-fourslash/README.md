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
two harness files and add `subprocess.go`. It never patches the source checkout, test bodies, or reference
baselines. Go's ordinary baseline verifier writes differences under the extracted checkout's
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

The Go filesystem remains authoritative, including bundled libraries, symlinks, case sensitivity, writes, and
renames. Paths are not relocated or rewritten. Compiler options use the existing Go-compatible options codec.
The example selects the same Go checker-history mode as `tsrs-fourslash`, including its environment overrides.
Each test starts a fresh process; cleanup closes stdin, drains stdout, waits for exit, and kills a hung child.

## Remaining scope

- Tests that inspect internal project state (`@stateBaseline`) are explicitly skipped. They need a remote
  inspection API before this runner could replace the Rust suite.
- Tests that supply Go content-mapper callbacks are explicitly skipped. The subprocess cannot invoke those
  callbacks; the current Rust LSP server also does not support them.
- Existing upstream skips remain intact. Skips are not counted as passes.
- Windows transport and production CLI startup are not covered by this prototype.

The translated Rust suite remains the default. This runner is an experiment in retaining upstream test logic,
not a change to the published compatibility counts.

The summary counts a parent with only skipped subtests as skipped: Go itself reports such a parent as passing.
Pass `--rust-results target/fourslash-results` to the summary to compare exercised groups with the translated suite.

## Validation (2026-10-18)

At pinned TypeScript commit `b85298b6a81f772d080b0455de0ca9d744cd6fd6`, on macOS arm64:

| Runner | Passing groups | Failing groups | Skipped groups |
| --- | ---: | ---: | ---: |
| Existing translated Rust suite | 4,066 | 63 | 417 |
| Go tests with Rust subprocess | 4,048 | 0 | 498 |

Go's raw top-level totals are 4,055 passes and 491 skips. Seven passing parents contain only skipped subtests
(45 skipped subtests in total); the table counts those parents as skipped. All 4,048 exercised groups also pass
in the translated suite. The additional skips cover its 63 existing failures and 18 passing groups that inspect
internal project state. These results establish feasibility, not full coverage equivalence.

Also checked: a child that immediately exits reports a failure promptly; fresh pinned-source extraction and
reuse preserve the original harness and test files; the dev builds, workspace Clippy, and source checks pass.
