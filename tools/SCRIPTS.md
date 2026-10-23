# Repository scripts

Run tooling from the repository root with Node.js 24 or newer and the pinned pnpm version in `package.json`. All Python tooling has been migrated to native JavaScript; shell orchestration and the existing TypeScript generators remain. The JavaScript tools use Node's standard library.

`pnpm run` lists every command. Arguments follow the command directly; with npm, put `--` before arguments. For example:

```sh
pnpm lint:source
pnpm fourslash --help
pnpm oracle:ast --help
pnpm perf:verify --base /tmp/base/tsrs --new /tmp/new/tsrs --projects xstate-main --reps 1
pnpm watch:check /path/to/tsrs /path/to/project
pnpm test:tooling
```

Compiler/oracle generators require `ts-ref` at the exact TypeScript commit in `Cargo.toml`.
[Verify the checkout and build the reference compiler](../CONTRIBUTING.md#reference-compiler) before reproductions,
fixes or probes, and rebuild reference binaries when the pin changes. Compiler probes also need the indicated
compiler binaries and prepared benchmark projects (`pnpm bench prepare --projects <names>`). Go, Cargo, Docker,
perf, and other platform tools are needed only by the commands that use them. The performance measurements use
the system `time` utility on macOS and Linux; instruction counters and `/proc` residency sampling require Linux.

Watch tools preserve and restore touched files, including on interruption. Run them on a disposable project checkout. They retain `BUILD`, `LOG`, `REPEAT`, `ONLY` and `EMITCHECK` environment options where applicable. The mutation harness uses disjoint `MUT_CLONE` and `MUT_PRISTINE` directories and restores its edits. Repository conformance runs repository code in disposable Docker containers as a non-root host user; it does not need Python in the container.

The existing regression checks run with `pnpm test:tooling`. Docker controls run when `TSRS_CONFORMANCE_SANDBOX_IMAGE` is set; real compiler controls run when both `TSRS_CONFORMANCE_TSRS` and `TSRS_CONFORMANCE_TSGO` are set. `pnpm check:scripts` checks tooling syntax and package command paths without executing the tools. Shared helper modules and fixtures are imported by their entry points.

Seeded fuzz and mutation generators use a deterministic JavaScript PRNG, so a seed produces a new sequence after this migration. Saved mutation batches remain replayable, including batches whose offsets count Unicode code points.

## build

| Root command | Entry point |
| --- | --- |
| `pnpm build:dev` | `cargo build --profile dev -p tsrs_cli -p tsrs_testrunner` |

## General

| Root command | Entry point |
| --- | --- |
| `pnpm lint` | `pnpm lint:source && pnpm lint:rust && pnpm check:scripts` |
| `pnpm bench` | `node bench/bench.mjs` |

## lint

| Root command | Entry point |
| --- | --- |
| `pnpm lint:source` | `node tools/lint/source.mjs` |
| `pnpm lint:rust` | `cargo clippy --all-targets --config 'build.warnings="deny"'` |

## check

| Root command | Entry point |
| --- | --- |
| `pnpm check:scripts` | `node tools/check-scripts.mjs` |

## test

| Root command | Entry point |
| --- | --- |
| `pnpm test:tooling` | `node --test tools/gen-diagnostics/test_*.mjs tools/go-fourslash/test_*.mjs tools/repo-conformance/test_*.mjs` |
| `pnpm test:bench` | `node --test bench/bench.test.mjs` |
| `pnpm test:regressions` | `bash tools/regressions.sh` |
| `pnpm test:contentmapper` | `bash tools/contentmapper-e2e.sh` |

## bench

| Root command | Entry point |
| --- | --- |
| `pnpm bench:measure` | `node bench/measure.mjs` |
| `pnpm bench:count` | `node bench/count.mjs` |
| `pnpm bench:wallclock` | `node bench/wallclock.mjs` |

## gen

| Root command | Entry point |
| --- | --- |
| `pnpm gen` | `bash tools/gen-check.sh` |
| `pnpm gen:ast` | `node tools/gen-ast/gen-ast.ts` |
| `pnpm gen:flags` | `node tools/gen-ast/gen-flags.ts` |
| `pnpm gen:codec` | `node crates/tsrs_api_codec/gen/gen-codec.ts` |
| `pnpm gen:codec-oracle` | `node crates/tsrs_api_codec/gen/oracle.mjs` |
| `pnpm gen:codec-probe` | `bash crates/tsrs_api_codec/gen/goprobe.sh` |
| `pnpm gen:api-probe` | `bash crates/tsrs_api/src/checker/testdata/go_probe/regen.sh` |
| `pnpm gen:diagnostics` | `node tools/gen-diagnostics/gen.mjs` |
| `pnpm gen:libs` | `node tools/gen-libs/gen.mjs` |
| `pnpm gen:lsproto` | `node tools/gen-lsproto/generate.mts` |
| `pnpm gen:lsproto-model` | `node tools/gen-lsproto/fetchModel.mts` |
| `pnpm gen:tsoptions` | `node tools/gen-tsoptions/gen_decls.mjs` |
| `pnpm gen:tsconfig-tests` | `node tools/gen-tsoptions/gen_tsconfig_tests.mjs` |
| `pnpm gen:signatures` | `node tools/sigs-from-rust.mjs` |

## localization

| Root command | Entry point |
| --- | --- |
| `pnpm localization:check` | `node tools/gen-diagnostics/check_localizations.mjs` |
| `pnpm localization:import` | `node tools/gen-diagnostics/import_localization.mjs` |

## lsproto

| Root command | Entry point |
| --- | --- |
| `pnpm lsproto:check` | `node tools/gen-lsproto/check.mjs` |

## fourslash

| Root command | Entry point |
| --- | --- |
| `pnpm fourslash` | `node tools/go-fourslash/run.mjs` |
| `pnpm fourslash:summarize` | `node tools/go-fourslash/summarize.mjs` |
| `pnpm fourslash:gate` | `bash .github/scripts/go-fourslash-gate.sh` |

## ci

| Root command | Entry point |
| --- | --- |
| `pnpm ci:conformance` | `bash .github/scripts/conformance-gate.sh` |
| `pnpm ci:diagnostics` | `node tools/ci/compare_diagnostics.mjs` |
| `pnpm ci:skip-marker` | `node tools/ci/skip_marker.mjs` |
| `pnpm ci:determinism` | Benchmark tsgo snapshot matrix and regression scheduling sweep in `tasks/determinism` |
| `pnpm ci:tsan` | `bash tools/ci/tsan.sh` |
| `pnpm ci:tsan-summary` | `node tools/ci/tsan_summary.mjs` |
| `pnpm ci:workloads` | `node tools/ci/workloads.mjs` |
| `pnpm ci:release-wasm` | `node .github/scripts/restore-release-wasm.mjs` |
| `pnpm ci:wait-npm` | `node .github/scripts/wait-for-npm-packages.mjs` |
| `pnpm ci:triage-labels` | `bash .github/scripts/setup-triagebot-labels.sh` |

## perf

| Root command | Entry point |
| --- | --- |
| `pnpm perf:verify` | `node tools/perf/verify.mjs` |
| `pnpm perf:layout` | `node tools/perf/layout-measure.mjs` |
| `pnpm perf:leaf` | `node tools/perf/leafprobe.mjs` |
| `pnpm perf:slack` | `node tools/perf/slackprobe.mjs` |
| `pnpm perf:cluster` | `node tools/perf/clusterprobe.mjs` |
| `pnpm perf:probe` | `bash tools/perf/probe.sh` |
| `pnpm perf:leaf-workflow` | `bash tools/perf/leafprobe.sh` |
| `pnpm perf:slack-workflow` | `bash tools/perf/slackprobe.sh` |
| `pnpm perf:pgo-train` | `bash .github/scripts/pgo-train.sh` |
| `pnpm perf:bolt` | `bash .github/scripts/bolt.sh` |

## watch

| Root command | Entry point |
| --- | --- |
| `pnpm watch:check` | `node tools/watch/watchcheck.mjs` |
| `pnpm watch:latency` | `node tools/watch/watchlat.mjs` |
| `pnpm watch:during-job` | `node tools/watch/editduringjob.mjs` |
| `pnpm watch:save-patterns` | `node tools/watch/savepatterns.mjs` |

## oracle

| Root command | Entry point |
| --- | --- |
| `pnpm oracle:ast` | `node tools/oracle/ast/run.mjs` |
| `pnpm oracle:ast-build` | `bash tools/oracle/ast/build.sh` |
| `pnpm oracle:ast-fields` | `node tools/oracle/ast/gen-fields.ts` |
| `pnpm oracle:printer` | `node tools/oracle/printer/run.mjs` |
| `pnpm oracle:printer-build` | `bash tools/oracle/printer/build.sh` |
| `pnpm oracle:emit` | `node tools/oracle/emit/run.mjs` |
| `pnpm oracle:emit-cases` | `node tools/oracle/emit/cases.mjs` |
| `pnpm oracle:emit-monorepo` | `bash tools/oracle/emit/monorepo.sh` |
| `pnpm oracle:emit-summary` | `node tools/oracle/emit/summarize.mjs` |
| `pnpm oracle:emit-select` | `node tools/oracle/emit/select-packages.mjs` |
| `pnpm oracle:emit-packages` | `node tools/oracle/emit/list-packages.js` |
| `pnpm oracle:emit-test-monorepo` | `bash tools/oracle/emit/test_monorepo.sh` |
| `pnpm oracle:emit-test-buildinfo` | `bash tools/oracle/emit/test_run_buildinfo.sh` |
| `pnpm oracle:incremental` | `bash tools/oracle/incremental/steps.sh` |
| `pnpm oracle:incremental-fixtures` | `bash tools/oracle/incremental/fixtures/run-all.sh` |
| `pnpm oracle:lsp` | `node tools/oracle/lsp/lsp_oracle.mjs` |
| `pnpm oracle:lsp-exit` | `node tools/oracle/lsp/exit_check.mjs` |
| `pnpm oracle:completions` | `node tools/oracle/completions/record.mjs` |
| `pnpm oracle:lsproto` | `bash tools/oracle/lsproto/run.sh` |
| `pnpm oracle:lsproto-registry` | `node tools/oracle/lsproto/gen_registry.mjs` |
| `pnpm oracle:lsproto-mutate` | `node tools/oracle/lsproto/mutate.mjs` |
| `pnpm oracle:module` | `node tools/oracle/module/gen.mjs` |
| `pnpm oracle:modulespecifiers` | `node tools/oracle/modulespecifiers/gen.mjs` |
| `pnpm oracle:tsc-dump` | `bash tools/oracle/tsctests/dump.sh` |
| `pnpm oracle:json-cases` | `node crates/tsrs_api_transport/tests/fixtures/go_json_oracle_cases.mjs` |
| `pnpm oracle:syntax-cases` | `node crates/tsrs_api_transport/tests/fixtures/go_syntax_oracle_cases.mjs` |
| `pnpm oracle:refs-record` | `node crates/tsrs_ls/testdata/refs_smoke/drive.mjs` |

## fuzz

| Root command | Entry point |
| --- | --- |
| `pnpm fuzz:parser` | `node tools/fuzz/parser.mjs` |

## mutate

| Root command | Entry point |
| --- | --- |
| `pnpm mutate` | `node tools/mutate/mutate.mjs` |

## lsp

| Root command | Entry point |
| --- | --- |
| `pnpm lsp:memory` | `node tools/lsp-mem/lsp_mem.mjs` |

## types

| Root command | Entry point |
| --- | --- |
| `pnpm types:sample` | `node tools/project-types-sample.mjs` |
| `pnpm types:compare` | `node tools/project-types-compare.mjs` |
| `pnpm types:packages` | `bash tools/project-types-packages.sh` |

## diffs

| Root command | Entry point |
| --- | --- |
| `pnpm diffs:cluster` | `node tools/cluster-diffs.mjs` |

## repo-conformance

| Root command | Entry point |
| --- | --- |
| `pnpm repo-conformance` | `node tools/repo-conformance/run.mjs` |

## node-api

| Root command | Entry point |
| --- | --- |
| `pnpm node-api:setup` | `bash tools/node-api/setup.sh` |
| `pnpm node-api:upstream` | `node tools/node-api/run-upstream.mjs` |
| `pnpm node-api:compare` | `node tools/node-api/compare-responses.mjs` |
| `pnpm node-api:inventory` | `node tools/node-api/inventory.mjs` |
| `pnpm node-api:consumer` | `node tools/node-api/consumer.mjs` |
| `pnpm node-api:controls` | `node tools/node-api/controls/run-controls.mjs` |
| `pnpm node-api:scenario` | `node tools/node-api/controls/scen.mjs` |
| `pnpm node-api:scenario2` | `node tools/node-api/controls/scen2.mjs` |
| `pnpm node-api:scenario3` | `node tools/node-api/controls/scen3.mjs` |
| `pnpm node-api:scenario4` | `node tools/node-api/controls/scen4.mjs` |
| `pnpm node-api:scenario-extra` | `node tools/node-api/controls/scen-extra.mjs` |

## wasm

| Root command | Entry point |
| --- | --- |
| `pnpm wasm:build` | `bash tools/wasm/build.sh` |
| `pnpm wasm:gate` | `bash tools/wasm/gate.sh` |
| `pnpm wasm:diff` | `node tools/wasm/diff.mjs` |
| `pnpm wasm:bench` | `node tools/wasm/bench.mjs` |
| `pnpm wasm:serve-example` | `node npm/tsrs-wasm/examples/browser/serve.mjs` |

## upstream

| Root command | Entry point |
| --- | --- |
| `pnpm upstream:measure` | `node upstream/tools/measure.mjs` |
| `pnpm upstream:summarize` | `node upstream/tools/summarize.mjs` |
| `pnpm upstream:testfiles` | `bash upstream/tools/testfiles.sh` |

## npm

| Root command | Entry point |
| --- | --- |
| `pnpm npm:build` | `node npm/build.mjs` |

## sdk

| Root command | Entry point |
| --- | --- |
| `pnpm sdk:sync` | `node npm/sdk/sync-upstream.mjs` |
| `pnpm sdk:coverage` | `node npm/sdk/method-coverage.mjs` |
| `pnpm sdk:record` | `node npm/sdk/coverage/record-methods.mjs` |
| `pnpm sdk:smoke` | `node npm/sdk/smoke-consumer.mjs` |
