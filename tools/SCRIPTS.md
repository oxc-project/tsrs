# Repository scripts

Run tooling from the repository root with Vite+ (`vp`), which manages Node.js 24 and the pinned pnpm version in `package.json`. All Python tooling has been migrated to native JavaScript; shell orchestration and the existing TypeScript generators remain. The JavaScript tools use Node's standard library.

`vp run` opens the task selector. Use `vp run <name>` to run a command; arguments follow its name directly. For example:

```sh
vp run lint:source
vp run fourslash --help
vp run oracle:ast --help
vp run perf:verify --base /tmp/base/tsrs --new /tmp/new/tsrs --projects xstate-main --reps 1
vp run watch:check /path/to/tsrs /path/to/project
vp run test:tooling
```

`vp run lint` runs the Rust, source, and script syntax checks. `vp check` checks the workspace configuration and
playground's formatting, lint, and types; `vp test run` runs the playground suite. Prepare the playground with
`vp run --filter tsrs-playground prepare-assets` first (requires `npm/tsrs-wasm/tsrs.wasm`, built by
`vp run wasm:build`). Build the site with `vp -C playground build`. Its integration suite also requires
`target/debug/tsrs`, built by `vp run build:dev`.

Compiler/oracle generators require `ts-ref` at the exact TypeScript commit in `Cargo.toml`.
[Verify the checkout and build the reference compiler](../CONTRIBUTING.md#reference-compiler) before reproductions,
fixes or probes, and rebuild reference binaries when the pin changes. Compiler probes also need the indicated
compiler binaries and prepared benchmark projects (`vp run bench prepare --projects <names>`). Go, Cargo, Docker,
perf, and other platform tools are needed only by the commands that use them. The performance measurements use
the system `time` utility on macOS and Linux; instruction counters and `/proc` residency sampling require Linux.

Watch tools preserve and restore touched files, including on interruption. Run them on a disposable project checkout. They retain `BUILD`, `LOG`, `REPEAT`, `ONLY` and `EMITCHECK` environment options where applicable. The mutation harness uses disjoint `MUT_CLONE` and `MUT_PRISTINE` directories and restores its edits. Repository conformance runs repository code in disposable Docker containers as a non-root host user; it does not need Python in the container.

The existing regression checks run with `vp run test:tooling`. Docker controls run when `TSRS_CONFORMANCE_SANDBOX_IMAGE` is set; real compiler controls run when both `TSRS_CONFORMANCE_TSRS` and `TSRS_CONFORMANCE_TSGO` are set. `vp run check:scripts` checks tooling syntax and package command paths without executing the tools. Shared helper modules and fixtures are imported by their entry points.

Seeded fuzz and mutation generators use a deterministic JavaScript PRNG, so a seed produces a new sequence after this migration. Saved mutation batches remain replayable, including batches whose offsets count Unicode code points.

## build

| Root command | Entry point |
| --- | --- |
| `vp run build:dev` | `cargo build --profile dev -p tsrs_cli -p tsrs_testrunner` |

## General

| Root command | Entry point |
| --- | --- |
| `vp run lint` | `vp run lint:source && vp run lint:rust && vp run check:scripts` |
| `vp run bench` | `node bench/bench.mjs` |

## lint

| Root command | Entry point |
| --- | --- |
| `vp run lint:source` | `node tools/lint/source.mjs` |
| `vp run lint:rust` | `cargo clippy --all-targets --config 'build.warnings="deny"'` |

## check

| Root command | Entry point |
| --- | --- |
| `vp run check:scripts` | `node tools/check-scripts.mjs` |

## test

| Root command | Entry point |
| --- | --- |
| `vp run test:tooling` | `node --test tools/gen-diagnostics/test_*.mjs tools/go-fourslash/test_*.mjs tools/repo-conformance/test_*.mjs` |
| `vp run test:bench` | `node --test bench/bench.test.mjs` |
| `vp run test:regressions` | `bash tools/regressions.sh` |
| `vp run test:contentmapper` | `bash tools/contentmapper-e2e.sh` |

## bench

| Root command | Entry point |
| --- | --- |
| `vp run bench:measure` | `node bench/measure.mjs` |
| `vp run bench:count` | `node bench/count.mjs` |
| `vp run bench:wallclock` | `node bench/wallclock.mjs` |

## gen

| Root command | Entry point |
| --- | --- |
| `vp run gen` | `bash tools/gen-check.sh` |
| `vp run gen:ast` | `node tools/gen-ast/gen-ast.ts` |
| `vp run gen:flags` | `node tools/gen-ast/gen-flags.ts` |
| `vp run gen:codec` | `node crates/tsrs_api_codec/gen/gen-codec.ts` |
| `vp run gen:codec-oracle` | `node crates/tsrs_api_codec/gen/oracle.mjs` |
| `vp run gen:codec-probe` | `bash crates/tsrs_api_codec/gen/goprobe.sh` |
| `vp run gen:api-probe` | `bash crates/tsrs_api/src/checker/testdata/go_probe/regen.sh` |
| `vp run gen:diagnostics` | `node tools/gen-diagnostics/gen.mjs` |
| `vp run gen:libs` | `node tools/gen-libs/gen.mjs` |
| `vp run gen:lsproto` | `node tools/gen-lsproto/generate.mts` |
| `vp run gen:lsproto-model` | `node tools/gen-lsproto/fetchModel.mts` |
| `vp run gen:tsoptions` | `node tools/gen-tsoptions/gen_decls.mjs` |
| `vp run gen:tsconfig-tests` | `node tools/gen-tsoptions/gen_tsconfig_tests.mjs` |
| `vp run gen:signatures` | `node tools/sigs-from-rust.mjs` |

## localization

| Root command | Entry point |
| --- | --- |
| `vp run localization:check` | `node tools/gen-diagnostics/check_localizations.mjs` |
| `vp run localization:import` | `node tools/gen-diagnostics/import_localization.mjs` |

## lsproto

| Root command | Entry point |
| --- | --- |
| `vp run lsproto:check` | `node tools/gen-lsproto/check.mjs` |

## fourslash

| Root command | Entry point |
| --- | --- |
| `vp run fourslash` | `node tools/go-fourslash/run.mjs` |
| `vp run fourslash:summarize` | `node tools/go-fourslash/summarize.mjs` |
| `vp run fourslash:gate` | `bash .github/scripts/go-fourslash-gate.sh` |

## ci

| Root command | Entry point |
| --- | --- |
| `vp run ci:conformance` | Conformance gate in both checker modes and upstream scheduling sweep in `tasks/determinism` |
| `vp run ci:diagnostics` | `node tools/ci/compare_diagnostics.mjs` |
| `vp run ci:skip-marker` | `node tools/ci/skip_marker.mjs` |
| `vp run ci:determinism` | Benchmark tsgo snapshot matrix and regression scheduling sweep in `tasks/determinism` |
| `vp run ci:thread-sanitizer` | ThreadSanitizer gate in `tasks/thread_sanitizer`; accepts an instrumented compiler binary; CI runs it in the `thread-sanitizer` job |
| `vp run ci:thread-sanitizer-summary` | `node tasks/thread_sanitizer/summary.mjs` |
| `vp run ci:thread-sanitizer-workloads` | `node tasks/thread_sanitizer/workloads.mjs` |
| `vp run ci:release-wasm` | `node .github/scripts/restore-release-wasm.mjs` |
| `vp run ci:wait-npm` | `node .github/scripts/wait-for-npm-packages.mjs` |
| `vp run ci:triage-labels` | `bash .github/scripts/setup-triagebot-labels.sh` |

## perf

| Root command | Entry point |
| --- | --- |
| `vp run perf:verify` | `node tools/perf/verify.mjs` |
| `vp run perf:layout` | `node tools/perf/layout-measure.mjs` |
| `vp run perf:leaf` | `node tools/perf/leafprobe.mjs` |
| `vp run perf:slack` | `node tools/perf/slackprobe.mjs` |
| `vp run perf:cluster` | `node tools/perf/clusterprobe.mjs` |
| `vp run perf:probe` | `bash tools/perf/probe.sh` |
| `vp run perf:leaf-workflow` | `bash tools/perf/leafprobe.sh` |
| `vp run perf:slack-workflow` | `bash tools/perf/slackprobe.sh` |
| `vp run perf:pgo-train` | `bash .github/scripts/pgo-train.sh` |
| `vp run perf:bolt` | `bash .github/scripts/bolt.sh` |

## watch

| Root command | Entry point |
| --- | --- |
| `vp run watch:check` | `node tools/watch/watchcheck.mjs` |
| `vp run watch:latency` | `node tools/watch/watchlat.mjs` |
| `vp run watch:during-job` | `node tools/watch/editduringjob.mjs` |
| `vp run watch:save-patterns` | `node tools/watch/savepatterns.mjs` |

## oracle

| Root command | Entry point |
| --- | --- |
| `vp run oracle:ast` | `node tools/oracle/ast/run.mjs` |
| `vp run oracle:ast-build` | `bash tools/oracle/ast/build.sh` |
| `vp run oracle:ast-fields` | `node tools/oracle/ast/gen-fields.ts` |
| `vp run oracle:printer` | `node tools/oracle/printer/run.mjs` |
| `vp run oracle:printer-build` | `bash tools/oracle/printer/build.sh` |
| `vp run oracle:emit` | `node tools/oracle/emit/run.mjs` |
| `vp run oracle:emit-cases` | `node tools/oracle/emit/cases.mjs` |
| `vp run oracle:emit-monorepo` | `bash tools/oracle/emit/monorepo.sh` |
| `vp run oracle:emit-summary` | `node tools/oracle/emit/summarize.mjs` |
| `vp run oracle:emit-select` | `node tools/oracle/emit/select-packages.mjs` |
| `vp run oracle:emit-packages` | `node tools/oracle/emit/list-packages.js` |
| `vp run oracle:emit-test-monorepo` | `bash tools/oracle/emit/test_monorepo.sh` |
| `vp run oracle:emit-test-buildinfo` | `bash tools/oracle/emit/test_run_buildinfo.sh` |
| `vp run oracle:incremental` | `bash tools/oracle/incremental/steps.sh` |
| `vp run oracle:incremental-fixtures` | `bash tools/oracle/incremental/fixtures/run-all.sh` |
| `vp run oracle:lsp` | `node tools/oracle/lsp/lsp_oracle.mjs` |
| `vp run oracle:lsp-exit` | `node tools/oracle/lsp/exit_check.mjs` |
| `vp run oracle:completions` | `node tools/oracle/completions/record.mjs` |
| `vp run oracle:lsproto` | `bash tools/oracle/lsproto/run.sh` |
| `vp run oracle:lsproto-registry` | `node tools/oracle/lsproto/gen_registry.mjs` |
| `vp run oracle:lsproto-mutate` | `node tools/oracle/lsproto/mutate.mjs` |
| `vp run oracle:module` | `node tools/oracle/module/gen.mjs` |
| `vp run oracle:modulespecifiers` | `node tools/oracle/modulespecifiers/gen.mjs` |
| `vp run oracle:tsc-dump` | `bash tools/oracle/tsctests/dump.sh` |
| `vp run oracle:json-cases` | `node crates/tsrs_api_transport/tests/fixtures/go_json_oracle_cases.mjs` |
| `vp run oracle:syntax-cases` | `node crates/tsrs_api_transport/tests/fixtures/go_syntax_oracle_cases.mjs` |
| `vp run oracle:refs-record` | `node crates/tsrs_ls/testdata/refs_smoke/drive.mjs` |

## fuzz

| Root command | Entry point |
| --- | --- |
| `vp run fuzz:parser` | `node tools/fuzz/parser.mjs` |

## mutate

| Root command | Entry point |
| --- | --- |
| `vp run mutate` | `node tools/mutate/mutate.mjs` |

## lsp

| Root command | Entry point |
| --- | --- |
| `vp run lsp:memory` | `node tools/lsp-mem/lsp_mem.mjs` |

## types

| Root command | Entry point |
| --- | --- |
| `vp run types:sample` | `node tools/project-types-sample.mjs` |
| `vp run types:compare` | `node tools/project-types-compare.mjs` |
| `vp run types:packages` | `bash tools/project-types-packages.sh` |

## diffs

| Root command | Entry point |
| --- | --- |
| `vp run diffs:cluster` | `node tools/cluster-diffs.mjs` |

## repo-conformance

| Root command | Entry point |
| --- | --- |
| `vp run repo-conformance` | `node tools/repo-conformance/run.mjs` |

## node-api

| Root command | Entry point |
| --- | --- |
| `vp run node-api:setup` | `bash tools/node-api/setup.sh` |
| `vp run node-api:upstream` | `node tools/node-api/run-upstream.mjs` |
| `vp run node-api:compare` | `node tools/node-api/compare-responses.mjs` |
| `vp run node-api:inventory` | `node tools/node-api/inventory.mjs` |
| `vp run node-api:consumer` | `node tools/node-api/consumer.mjs` |
| `vp run node-api:controls` | `node tools/node-api/controls/run-controls.mjs` |
| `vp run node-api:scenario` | `node tools/node-api/controls/scen.mjs` |
| `vp run node-api:scenario2` | `node tools/node-api/controls/scen2.mjs` |
| `vp run node-api:scenario3` | `node tools/node-api/controls/scen3.mjs` |
| `vp run node-api:scenario4` | `node tools/node-api/controls/scen4.mjs` |
| `vp run node-api:scenario-extra` | `node tools/node-api/controls/scen-extra.mjs` |

## wasm

| Root command | Entry point |
| --- | --- |
| `vp run wasm:build` | `bash tools/wasm/build.sh` |
| `vp run wasm:gate` | `bash tools/wasm/gate.sh` |
| `vp run wasm:diff` | `node tools/wasm/diff.mjs` |
| `vp run wasm:bench` | `node tools/wasm/bench.mjs` |
| `vp run wasm:serve-example` | `node npm/tsrs-wasm/examples/browser/serve.mjs` |

## npm

| Root command | Entry point |
| --- | --- |
| `vp run npm:build` | `node npm/build.mjs` |

## sdk

| Root command | Entry point |
| --- | --- |
| `vp run sdk:sync` | `node npm/sdk/sync-upstream.mjs` |
| `vp run sdk:coverage` | `node npm/sdk/method-coverage.mjs` |
| `vp run sdk:record` | `node npm/sdk/coverage/record-methods.mjs` |
| `vp run sdk:smoke` | `node npm/sdk/smoke-consumer.mjs` |
