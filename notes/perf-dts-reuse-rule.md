# Conservative DTS diagnostic reuse eligibility evaluation

The revised rule is promising: it admits Immer in both Programs, preserves exact output, and reduces instructions about 24–25% and peak RSS about 14% in two paired measurement rounds. Zod and XState retain ordinary checking with instruction/RSS changes close to baseline. The evaluated rule is now integrated locally in `crates/tsrs_dts/src/diagnostics.rs`; integration validation is recorded separately below. The performance tables measure the retained experimental release patch.

All work is local. The evaluation preserved production Rust; the subsequent integration changes only DTS eligibility, regressions and its documentation. README files, inputs and dependencies remain unchanged. No commits or publication. Reference HEAD and clean tracked tsc sources match Cargo pin `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.

## Candidate rule

Keep existing eligibility, with one additional route for closed project modules. Every project file must be an external module. Project files are identified by excluding default libraries and external-library inputs through existing Program APIs, not path-name heuristics.

Reject namespace imports/exports, UMD namespace declarations, module/namespace declarations and augmentations, import-equals declarations, and whole-module/nonliteral import types. Every import/re-export and qualified literal import type must resolve through the Program module-resolution cache to another non-external, non-default-library project module. Recursively scan every such project module. Keep UMD exclusions across the entire Program, including dependencies. Keep project-reference, skipped-library-checking and external-code exclusions.

This targets linker requirements rather than package names. Internal named imports, star exports and qualified import types link directly to local owners without synthesizing ordered external import statements or module namespace objects. Dependency namespace/star syntax alone does not require those objects when project imports cannot expose dependency modules to the linker. Recheck the condition on emitted declarations: source syntax does not predict all generated syntax. No snapshot algorithm or linker ordering changes.

The exact measured experimental patch remains in `target/scratch/dts-reuse-rule/overlay/crates/tsrs_dts/src/diagnostics.rs`. The local production implementation removes experiment reporting, factors internal-target classification and short-circuits option gates before scanning; snapshot and linker ordering behavior are unchanged.

## Initial rule and revision

The initial rule rejected every import type. It admitted Immer source files but rejected emitted `immer.d.ts`: pinned emission synthesizes qualified types such as `import("./internal").StrictMode` and `import("./internal").Patch`. Declaration checking fell back to ordinary checking, retaining source-snapshot overhead without reuse. Exact output passed, but instructions/RSS regressed. Its overlay, binary and results remain as `rule1-*`; reject this version.

One revision admits qualified literal import types only when their target belongs to the closed project set. An unqualified `typeof import("./module")` still fails because it needs module namespace identity. Transitive namespace-bearing targets also fail the project scan. The existing linker handles qualified import types through local declaration owners (`graph.rs:855–909`), supporting this distinction.

## Public guard selection

Three independent processes per case compare exact filenames, declaration/map bytes, emitted/retained/removed counters and digest against the existing unmodified baseline.

| Workload | Source route | Declaration route | Exact output |
| --- | --- | --- | --- |
| Immer, maps off/on | closed modules | closed modules | yes |
| Zod, maps off/on | ordinary | ordinary | yes |
| XState, maps off/on | ordinary | ordinary | yes |

Zod has namespace imports/exports and still fails the added route. XState has a project `declare global` augmentation, so it also fails; its existing dependency namespace/star forms retain the old rejection. No workload names, counts, versions or timing thresholds occur in the rule.

XState's augmentation also explains a profitability hazard: replacing a global-affecting source path with an emitted declaration path deletes the old global path for snapshot purposes. Both pinned Go and Rust then invalidate non-library and checked default-library diagnostics (`programtosnapshot.rs:472` and `file_affects_global_scope` at `:554`). This is a verified mechanism; counters were not added to prove that every XState cache was invalidated in the earlier ablation.

## Measurements

Same public configs, dependency/source hashes and harness as earlier DTS experiments. One checker, system allocator, 64 MiB worker stack, `TSRS_FAST=0`, `RAYON_NUM_THREADS=1`, fresh processes and warm filesystem. Release opt3, fat LTO, one codegen unit, full debug symbols, no stripping. Two warmups plus seven measured repetitions per binary/case in each round, alternating binary order and rotating projects. No locked holdout or catalog-wide claim.

Instructions and peak RSS come from `/usr/bin/time -l`; API wall includes Program teardown. Build/compiler interference checked at process boundaries. Rule reporting is disabled for measurements, though an experiment-only environment lookup remains compiled into the binary.

Percentages below are medians of round-paired changes. Absolute values are separate binary medians; their ratio can differ from the paired percentage. Full ranges and improved-pair counts are retained.

### First measurement

| Case | Instructions | Peak RSS | Build wall | Wall ms baseline → trial | Instructions G baseline → trial | RSS MiB baseline → trial |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| immer-no-maps | -25.26% | -14.45% | -26.32% | 91.90 → 70.10 | 1.370 → 1.024 | 65.56 → 56.14 |
| immer-maps | -24.35% | -14.27% | -24.70% | 97.74 → 70.59 | 1.417 → 1.072 | 65.67 → 56.33 |
| zod-no-maps | -0.06% | +0.00% | -0.31% | 273.34 → 271.17 | 4.589 → 4.591 | 144.50 → 144.20 |
| zod-maps | -0.11% | +0.27% | +0.49% | 323.78 → 337.92 | 5.811 → 5.803 | 147.36 → 147.81 |
| xstate-no-maps | +0.10% | +0.02% | -0.56% | 261.27 → 259.18 | 4.105 → 4.113 | 146.56 → 146.16 |
| xstate-maps | -0.01% | -0.07% | -0.62% | 308.44 → 292.63 | 4.798 → 4.797 | 148.55 → 148.09 |

### Confirmation

| Case | Instructions | Peak RSS | Build wall | Wall ms baseline → trial | Instructions G baseline → trial | RSS MiB baseline → trial |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| immer-no-maps | -24.42% | -14.59% | -25.34% | 95.22 → 71.98 | 1.383 → 1.037 | 65.72 → 56.25 |
| immer-maps | -24.28% | -13.92% | -23.79% | 98.11 → 74.11 | 1.417 → 1.072 | 65.88 → 56.56 |
| zod-no-maps | -0.12% | -0.04% | +0.57% | 288.23 → 289.86 | 4.599 → 4.595 | 144.72 → 145.00 |
| zod-maps | -0.13% | +0.08% | -0.19% | 344.89 → 346.90 | 5.819 → 5.811 | 147.77 → 147.63 |
| xstate-no-maps | +0.02% | -0.01% | -0.56% | 279.24 → 280.62 | 4.121 → 4.116 | 146.66 → 146.70 |
| xstate-maps | +0.11% | -0.02% | +0.88% | 311.31 → 315.46 | 4.804 → 4.806 | 148.70 → 148.67 |

Immer improves in every measured instruction, RSS and wall pair in both rounds and both map modes. Zod/XState instruction medians remain within about 0.14% of baseline and RSS medians within about 0.27%; pair ranges overlap zero. Those guard results do not establish a speedup. Wall outliers and drift affect absolute times, so use paired distributions and hardware counters together.

## Correctness

Thirty initial focused configurations and sixteen additional boundary configurations compare independent baseline/candidate Programs, in both map modes. Coverage includes star collisions, cycles, nominal classes, namespace/external imports, global/module augmentations, qualified and whole-module import types, dependency and deferred-global errors, stripInternal, UMD, local declaration inputs, skipped-library checks, external star re-exports and a three-entry nominal-collision bundle. All compare exact diagnostics, stable counters and successful artifact bytes. Public inputs are development inputs, and extra fixtures were designed after inspecting the rule; neither is a locked holdout.

The freshly built pinned ordinary-diagnostics Go oracle from the preceding gate investigation verifies exact ordered messages on source-check fixtures, including successful empty diagnostic sets. The stripInternal fixtures separately emit through pinned Go and check the emitted declaration, establishing TS2304 for the dangling Hidden reference. All 46 oracle checks pass. Go has no equivalent declaration bundler, so bundle bytes are compared with the unmodified tsrs baseline.

## Decision and remaining work

Integrate the revised rule as a narrow additional eligibility route; reject the initial all-import-type restriction. Existing eligibility remains an alternative, and the whole-Program UMD and option gates remain in force. The evidence supports this route, not a general namespace/star gate removal.

Generated declarations can still introduce excluded syntax and force ordinary declaration checking after source snapshot work. Dependency topology and surviving library diagnostics also affect profitability; the structural rule is not a guarantee of benefit on arbitrary projects. Existing error fallback remains necessary. UMD, project-reference and external mapper restrictions remain in force.

The local integration adds direct eligibility tests for source and emitted declaration Programs, including compiler-generated qualified import types, transitive namespace targets, external and malformed module edges, preserved dependency UMD exclusions, option gates and the original eligibility alternative. A dynamic import source qualifies while its generated whole-module import type disables declaration reuse, directly testing the second eligibility check. Regression builds compare cycles, nominal multiple entries, maps and fresh-error fallback.

The integrated development probe reproduces the retained baseline's exact artifacts, diagnostics and counters for all six public cases and all 46 focused configurations. The existing pinned Go oracle checks remain the reference for those fixture diagnostics. Malformed import/export specifiers additionally match Go TS1141; the production target helper rejects them before calling the resolution API, whose contract requires a string literal. Validation logs and preservation hashes are retained separately in `target/scratch/dts-reuse-integration/`.

Completed integration validation:

- `cargo build --profile dev -p tsrs_cli -p tsrs_testrunner` passes.
- `cargo test -p tsrs_dts --lib --tests --locked --offline` passes every target: 12 diagnostic-reuse, 9 linker, 18 pipeline, 223 corpus and 5 type-only metadata tests. Its first unit-test binary contained six direct tests; the final eight direct tests were subsequently rerun successfully, including the added fallback and malformed-specifier cases.
- `TSRS_ARENA_POISON=1 cargo test -p tsrs_dts --lib --test diagnostic_reuse --test pipeline --locked --offline` passes all 38 final tests (8 direct eligibility, 12 diagnostic reuse, 18 pipeline).
- The compiler `symlink_cache_uses_shared_program_regions` regression passes with arena poisoning.
- `cargo clippy --all-targets --config 'build.warnings="deny"'`, `tools/lint/source.mjs`, focused Rust formatting and `git diff --check` pass.
- The final integrated probe matches all six public builds and 46 fixture configurations exactly; pinned Go additionally confirms the generated qualified/whole-module import types and malformed TS1141 diagnostics.
- Starting-file hashes confirm only the five intended existing files changed, plus the new direct-test file. All 3,550 other starting files, README files, reference sources and HEAD are unchanged. Nothing is committed or pushed.

These are integration correctness/lifetime checks, not new performance measurements. The release measurements above remain evidence for the evaluated rule; no catalog-wide or arbitrary-program gain is claimed.

## Evidence

`target/scratch/dts-reuse-rule/` contains starting hashes/contract, exact overlays and binaries, probe lock/config, pinned-reference capture, per-Program decisions, captured public/fixture outputs, measurements, oracle verification, preservation checks and event log. `measure/` is the rejected initial rule. `measure2/` and `rule2-first-*` hold the first revised-rule measurements; `measure3/` and `measurement*.json` hold confirmation. Both revised rounds have 84 measured processes plus 24 warmups (168 measured, 48 warmups total). Eighteen public capture builds independently check output.
