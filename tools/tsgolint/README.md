# Rust tsgolint baseline tooling

The Rust implementation lives in `crates/tsrs_lint`, with binary `tsrs-lint`.
Its rule inventory and progress are recorded in `testdata/lint/PROGRESS.md`.
All 60 pinned rules are implemented; requesting an unimplemented rule fails explicitly.

## Reference and attribution

The original imported specification is oxc-project/tsgolint at
`9378e49b67c0205a36afed0dfdcce229fc7d44b5`, with its recorded typescript-go revision
`69d658da1a632a723275439339e34842d4d8f401` and the five patches shipped by that checkout.
Ported protocol, traversal, and test-export code follows that reference.
The upstream MIT license is preserved in `LICENSE` in this directory.

The exporter now targets Go HEAD as of 2026-10-16:
`8897346fff24a9bd0c719099dc1af8e7200572c1`, compiler
`2bd066d87f5bafd315be9f40889d0a60b9e58e0b`, with its six compiler patches.
All imported suites and shared declarations follow this reference. Each rule records the
pins in `REFERENCE`; the [progress table](../../testdata/lint/PROGRESS.md) records coverage.

Keep the reference in a disposable checkout outside the working repository. The exporter
applies upstream compiler patches and instruments its rule tester; never point it at a
checkout containing work you want to preserve. Go 1.26 and the current reference's dependencies
are required, including `@types/node` 24.13.2 and `undici-types` 7.18.2 from its lockfile.
The exporter preserves those declaration packages and their licenses in `dependencies`;
the runner installs them into each disposable fixture tree. Set `GOCACHE` to a writable location if the default cache is unavailable.

```sh
GOCACHE=/private/tmp/tsrs-go-build python3 tools/tsgolint/export.py \
  /private/tmp/tsrs-tsgolint-reference /private/tmp/tsrs-lint-baselines \
  --rule await_thenable
```

The exporter executes the real Go test constructors, preserving generated cases, options,
extra files, output expectations, and upstream skip markers. Original test sources are
exported alongside the cases to preserve skip reasons. It also exports the diagnostics
from each executed case, including fixes and suggestions, without updating Go snapshots.
Copy each rule's exported cases and diagnostics into `testdata/lint` in its implementation
commit. Fixture roots are represented by paths relative to the upstream fixtures directory.

## Build and invocation

```sh
cargo build --profile dev -p tsrs_lint -p tsrs_cli -p tsrs_testrunner
cargo test -p tsrs_lint
printf '%s' '{"version":2,"configs":[]}' | target/debug/tsrs-lint headless
OXLINT_TSGOLINT_PATH="$PWD/target/debug/tsrs-lint" oxlint --type-aware
```

Input is a V1 or V2 JSON payload on stdin. Unpaired JSON surrogate escapes are rejected,
matching the headless Go decoder. The native Rust compiler retains its existing permissive
decoder; its difference from Go is recorded in the [compatibility table](../../docs/COMPATIBILITY.md).
Output uses the upstream binary framing:
four bytes of little-endian JSON byte length, one byte of message type, then UTF-8 JSON.
Diagnostics and protocol errors go to stdout; CLI errors go to stderr.
The supported flags are `headless`, `-fix`, `-fix-suggestions`, and `-debug timings`
(the double-dash flag aliases are also accepted).
Program construction and binding use the native parallel loader and honor `singleThreaded`.
Source overrides provide virtual files, directories, directory entries, stat information and real paths,
so package resolution and include globs work without physical files.
Linting and requested semantic diagnostics default to the
available thread count. Positive `RAYON_NUM_THREADS` overrides that default; both are capped at 256 and the active file count. Each worker shares one checker between semantic reporting and lint rules. All request
regions remain alive until their programs are released; output frames are collected in file order.
Go profiler output formats and npm packaging are outside this port's current scope.

Run imported suites with:

```sh
python3 tools/tsgolint/run-baselines.py target/debug/tsrs-lint testdata/lint --rule await-thenable
```

The runner compares complete initial diagnostic records and repeats linting after fixes,
checking every expected fix pass and suggestion output. Ranges are UTF-8 byte offsets.

The first rule was also exercised through Oxlint 1.56.0 using `OXLINT_TSGOLINT_PATH`:
`-A all -D typescript/await-thenable --type-aware` reports the pinned diagnostic,
and `--fix-suggestions` applies its suggested removal of `await`.

## Diagnostic quoting

`go run tools/tsgolint/gen-quote.go > crates/tsrs_lint/src/go_quote_generated.rs`
regenerates the Go printable-rune table used by diagnostic literal quoting. The
checked-in table is generated with Go 1.27.1 (Unicode 17), matching the baseline oracle.

The lint compiler host sets `SourceFileParseOptions::legacy_string_escapes` to
match the pinned scanner: lone surrogate escapes become U+FFFD, and only adjacent
fixed-width high/low escapes combine. Native compiler hosts retain the default
ECMAScript string behavior.

Semantic reporting and lint rules reuse the same checker and its caches. As in Go,
lint diagnostics and fixes can depend on whether semantic reporting ran first and
which files a checker previously processed. Cross-mode and cross-thread diagnostic
stability is not guaranteed.

The regexp2 word and capture-name predicates use a separate Unicode 17 table. Regenerate it with
`go run tools/tsgolint/gen-regexp-unicode.go > crates/tsrs_lint/src/rules/regexp_unicode.rs`,
then format that file with the repository Rustfmt settings. The generator rejects other Unicode versions.

## Oracle coverage

All 60 upstream rules use the rebuilt Go HEAD oracle above: 7,601 active cases and
28 explicit upstream skips. Imported diagnostics include complete ranges, messages,
labels, fixes and suggestions. Original local regression fixtures remain separate;
their `REFERENCE` files identify the oracle used for their expected records.
The mixed-rule integration fixture runs all 60 current baseline rules against Go HEAD
([reference](../../crates/tsrs_lint/tests/fixtures/all_rules/REFERENCE)) and compares complete
records without requiring stream order.

The updated type descriptions preserve the Go printer's byte truncation. If the
result cannot be serialized as UTF-8 JSON, that diagnostic is omitted as in Go.
The compiler's existing string-returning APIs retain their replacement behavior.
The Unicode regression fixture covers invalid astral truncation, valid U+FFFD,
long-name truncation recovery, and all four fix modes.


## Performance comparisons

Use optimized release binaries for performance measurements:

```sh
cargo build --release -p tsrs_lint --bin tsrs-lint
python3 bench/tsgolint.py --payload /path/to/oxlint-v2-payload.json \
  --cwd /path/to/corpus --helper rust="$PWD/target/release/tsrs-lint" \
  --helper go=/path/to/tsgolint --output /private/tmp/lint-bench
```

The macOS runner replays an identical file/rule configuration with full compiler diagnostics off
(`lint`) and on (`lint-typecheck`). Type-aware rules still resolve types on demand in `lint`.
Default and single modes are both measured, with one warm-up and three alternating repetitions.
Single mode sets `GOMAXPROCS=1 RAYON_NUM_THREADS=1`; Go may still retain several checker states,
so this controls execution concurrency rather than asserting identical cache layouts.
`--variants typecheck setup` additionally removes lint rules for attribution runs.

The output directory must not exist. It retains input payloads, framed diagnostics, stderr, macOS
`time -l` counters, helper/input hashes, and samples with diagnostic multiset digests.
Run builds and tests before benchmarking; compare the before/after digests explicitly and
investigate any difference. The runner does not treat different Go/Rust diagnostics as equivalent.


For a worker scaling sweep, add `--threads 4 8 12 --runs 5`. This replaces `--modes`,
sets both `RAYON_NUM_THREADS` and `GOMAXPROCS` to the selected count, and rotates the
worker-count order each round. Rust helper builds before the explicit-override fix cap all
these settings at four; use a release build with the override fix for a meaningful sweep.

## Untyped this parameters

The current Go HEAD `unbound-method` implementation panics when a method or function-valued
class property has an untyped explicit `this` parameter. Rust treats an absent annotation as
non-void and reports the unbound reference, matching the `this: any` control. This deliberate
robustness difference is covered in the headless regressions for both strict modes and all fix flags.

## Switch default-case comments

The switch-exhaustiveness rule recognizes trailing `no default` comments and custom
ECMAScript Unicode patterns, including lookarounds and backreferences. Go-style
`i`, `m`, and `s` modifiers preserve their group and alternative scopes. Literal braces, regex comments, and
identity escapes retain the Go parser semantics. Local regex cases and their
[Go HEAD provenance](../../crates/tsrs_lint/tests/fixtures/switch_exhaustiveness_check/REFERENCE)
cover these options. The original invalid `[` pattern remains an error regression:
Go HEAD panics, while Rust returns a structured protocol error.
