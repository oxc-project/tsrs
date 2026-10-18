# Go fourslash against the Rust language server

This runner executes the pinned TypeScript Go fourslash test bodies and assertions unchanged against separate
Rust processes. It replaces the generated Rust suite, translated harness, generator, and parser oracle.

Requirements: Python 3.12+, Rust, Go (with `GOTOOLCHAIN=auto`), and macOS or Linux. The TypeScript Git checkout must
contain the commit pinned in `Cargo.toml`; `--reference` defaults to `ts-ref`.

```sh
python3.12 tools/go-fourslash/run.py
python3.12 tools/go-fourslash/run.py -- -run '^TestQuickInfoAlias$' -v
python3.12 tools/go-fourslash/run.py -- -json > target/go-fourslash/results.jsonl
python3.12 tools/go-fourslash/summarize.py target/go-fourslash/results.jsonl
```

Use `--reference /path/to/TypeScript` to read another checkout, `--work /path/to/scratch` for a separate workspace,
or `--server /path/to/fourslash_server` to use a prebuilt server. These options go before `--`; ordinary `go test`
flags go after it. The default concurrency is four, each test has a two-minute subprocess timeout, and the whole
suite has a thirty-minute timeout. `-count=1` disables cached results.

The default run shows setup stages, elapsed time, and completed Go groups with pass/fail/skip counts. Progress
refreshes every second in a terminal, or every five seconds when redirected, and failed cases keep their diagnostic
output. `-v` keeps Go's verbose output; `-json` keeps its JSON stream on stdout for CI and `summarize.py`.

The runner extracts the pinned commit into `target/go-fourslash/reference`, then uses a Go build overlay to adapt
four harness files and add `subprocess.go`, `worker.go`, and `remotestate.go`. It never patches the source checkout, test bodies,
or reference baselines. Go's ordinary baseline verifier writes differences under the extracted checkout's
`tsc/testdata/baselines/local/`. The runner returns `go test`'s exit status.

## Process boundary

```text
Go test body → upstream Go Fourslash assertions → LSP Unix socket → Rust language server
                    ↑                                                   │
                    └──── original Go virtual filesystem ← Unix socket ─┘
```

The Rust process is the `fourslash_server` binary in `tasks/fourslash_server` (package `tsrs_fourslash_server`),
a test-only entry point around `tsrs_lsp::new_server`.
It is not the production `tsrs --lsp -stdio` executable. Standard LSP has no operation for installing a virtual
filesystem, and the pinned Go harness also sets inferred-project compiler options and waits for initialization
through internal calls. A private Unix socket carries those setup operations and filesystem callbacks; all
editor requests and responses use the existing framed LSP transport.

The Go harness leases Rust workers, keeping at most four idle processes. Each worker handles up to 32 cases
before restarting, with a fresh LSP server, session, and filesystem for every case. Only the process and the
existing immutable parse cache are shared, as in the former translated Rust runner. The case limit bounds retained
parse entries and arena allocations. Stdin/stdout carry worker commands and completion acknowledgements;
each case has its own LSP, filesystem, and project-state sockets.

For `@stateBaseline` cases, a second private Unix socket returns read-only project snapshots after LSP requests
complete. Go retains its upstream state comparison and formatting, using remote views of projects, source files,
default-project assignments, and config retention. Program and source-file handles preserve identity across
snapshots; the Rust adapter keeps the previous project owners alive until the next snapshot is captured.
Filesystem differences still come from Go's original filesystem. The `@tsc` setup commands run through Go's
existing `GetFileMapWithBuild` helper, then the Rust server reads the resulting declaration files and maps.

The Go filesystem remains authoritative, including bundled libraries, symlinks, case sensitivity, writes, and
renames. Paths are not relocated or rewritten. Compiler options use the existing Go-compatible options codec.
The task server selects Go checker-history mode for baseline compatibility, including its environment overrides.
File reads send a JSON byte count followed by raw UTF-8, avoiding JSON escaping of library contents. Missing
files remain distinct from empty files, and invalid UTF-8 uses the same replacement behavior as Go's JSON encoder.
Cleanup sends LSP shutdown, half-closes state requests and drains the final response, then waits for the worker's
completion acknowledgement before reuse. Failed or timed-out workers are killed and discarded; `TestMain` reaps
idle workers at suite exit.

## Remaining scope

- Tests that supply Go content-mapper callbacks are explicitly skipped. The subprocess cannot invoke those
  callbacks; the current Rust LSP server also does not support them.
- Existing upstream skips remain intact. Skips are not counted as passes.
- Windows transport and production CLI startup are not covered by this prototype.

The Go runner supplies the Linux CI gate and current compatibility count. PGO training drives the instrumented
`fourslash_server`; BOLT training and release verification use the corresponding instrumented and
optimized binaries. These workflows install Go and Python 3.12+ and check out the pinned `tsc` source and fixtures.
The translated Rust suite has been removed. Release-profile PGO/BOLT verification was not run locally for this migration.

The summary counts a parent with only skipped subtests as skipped: Go itself reports such a parent as passing.

## CI gate

The independent Linux `fourslash` job in `.github/workflows/ci.yml` checks out the pinned `tsc` source and fixtures, installs
Go from its `go.mod` and Python 3.12, builds the task server with the dev profile, and runs:

```sh
cargo build --profile dev --locked -p tsrs_fourslash_server
PYTHON=python3.12 .github/scripts/go-fourslash-gate.sh
```

The gate requires a completed, successful Go package result, zero failing tests, and at least **4,074 exercised
groups**. It excludes parents whose subtests all skipped, including nested groups, so additional skips cannot
silently reduce coverage. The runner first checks bridge shutdown with a deliberately delayed response terminator;
CI also runs the summary's regression checks. Go's build cache is shared with
`actions/setup-go` through `GOCACHE` at `target/go-fourslash/go-cache`.

On failure, CI uploads `results.jsonl`, `runner.log`, and baseline differences as the `go-fourslash-results`
artifact. For a local run, `PYTHON`, `TSRS_FOURSLASH_SERVER`, and `TSRS_GO_FOURSLASH_WORK` can select another Python
executable, prebuilt server, and work directory. `MIN_PASS` overrides the gate's coverage floor; raise the default
when support improves.

## Validation (2026-10-18)

At pinned TypeScript commit `b85298b6a81f772d080b0455de0ca9d744cd6fd6`, on macOS arm64:

| Runner | Passing groups | Failing groups | Skipped groups |
| --- | ---: | ---: | ---: |
| Former translated Rust suite (before removal) | 4,066 | 63 | 417 |
| Go tests with Rust subprocess | 4,074 | 0 | 472 |

All 4,066 groups passing in the translated suite are exercised successfully, including the 18 project-state
groups skipped by the initial prototype. Eight additional groups pass using Go's `@tsc` setup support. No passing
parent contains only skipped subtests. The 472 skipped groups comprise the existing 417 skips and 55 unsupported
content-mapper groups. The eight additional passes exercise Rust LSP behavior with Go-built fixtures, not Rust emit.

Also checked: a child that immediately exits reports a failure promptly; fresh pinned-source extraction and
reuse preserve the original harness and test files; the dev builds, workspace Clippy, and source checks pass.
After removing the translated suite, the full gate also passes with a dev-built `tsrs_cli/noembed` server,
matching the release workflow's feature selection. The retained LSP unit checks and workflow syntax checks pass.

### Runner cost

The worker pool and raw file payloads reduce the full Go suite from **54.55 s to 24.96 s**, with the same
4,074 passing groups and 472 skips. These measurements use the CI dev profile and four concurrent cases on
the same macOS arm64 machine; they measure harness cost, not production compiler performance.

| Measurement | One process per case, JSON file contents | Workers, raw file contents |
| --- | ---: | ---: |
| Full suite via `run.py` (warm build cache) | 54.55 s | 24.96 s |
| Direct-binary repeat 1 | 59.82 s | 25.78 s |
| Direct-binary repeat 2 | 67.88 s | 42.83 s |
| Go + Rust instructions retired | 3.030 trillion | 0.828 trillion |
| Rust process launches | 4,129 | 133 |
| Maximum per-process peak RSS across repeats | 195 MiB | 303 MiB |

Wall time varied between runs, but both repeat pairs improved. Retaining parsed files trades memory for speed;
workers restart after 32 cases rather than retaining the entire suite. RSS above is the largest process peak,
not the sum of concurrent workers. A raw-payload-only intermediate run took 44.61 s; capping `GOMAXPROCS` at
four made little additional difference and is not part of this change.

For reproduction, compare commit `8d4c4cf4` with this version, building each server with the dev profile and
using `run.py -- -json` for complete results. For build-free repeats,
compile each overlay with `go test -c -overlay=<work>/overlay.json -o <suite> ./internal/fourslash/tests`, then
run that binary from the extracted `tsc` directory with `TSRS_FOURSLASH_SERVER=<matching-server>` and
`-test.count=1 -test.parallel=4 -test.timeout=30m -test.v`. The paired instruction run wrapped the Go binary
and every Rust child separately in macOS `/usr/bin/time -l`, summing instructions and taking the maximum RSS;
its wall times were 64.08 s and 26.10 s. Timing a Python launcher alone does not count child instructions.

The framing checks run with `cargo test -p tsrs_fourslash_server` in the Fourslash CI job. Local lifecycle checks additionally
repeat hover, cross-project call hierarchy, and auto-import cases 40 times under Go's race detector to cross the
worker restart boundary, and verify that an immediate child failure is reported and the next case can launch a healthy worker.
