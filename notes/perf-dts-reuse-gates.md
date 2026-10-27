# DTS diagnostic reuse gate investigation

All experiments are local. Runtime instrumentation and gate ablations were applied only to the verified frozen source copy; production Rust remains unchanged. Existing work is preserved; `docs/DTS.md` and `docs/COMPATIBILITY.md` gain the verified ordering evidence, and this report is new. Reference HEAD matches Cargo metadata at `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, with clean tracked reference sources.

## Exact exclusions

Counts cover every matching AST node, including dependencies, rather than only the first short-circuit rejection. The maps-enabled and disabled cases have the same exclusions. None has project references, skipped library checks, external code or UMD namespace declarations.

| Workload | Project nodes | Dependency nodes |
| --- | --- | --- |
| immer-no-maps | 11 star_export | 53 star_export, 31 namespace_import |
| zod-no-maps | 157 namespace_import, 16 star_export, 8 namespace_export | none |
| xstate-no-maps | 5 star_export | 34 namespace_import, 50 star_export, 4 namespace_export |

Raw `.gates` files record exact public paths, AST positions and Program options. Immer has 189 source files, Zod 150 and XState 251. Both stages are separately recorded when enabled.

## Independent ordering reproduction

Each public build runs in a fresh process with independent source and declaration Programs. `normal` retains all gates; `syntax` bypasses only namespace import/export and bare-star exclusions in both Programs, preserving UMD and option gates; `all` additionally bypasses UMD for diagnostic investigation only; `source` bypasses the syntax exclusions for the source snapshot but forces ordinary declaration checking. These are experiment-only switches.

All six normal-mode cases match the saved unmodified binary. Full syntax bypass preserves artifact bytes and stable counters for Immer and XState, with maps both on and off. Zod differs in both modes: the declaration length and retained/emitted/removed counts are unchanged, but namespace block order changes. The first declaration difference is at character offset 212316: ordinary output emits `module_9` after `module_14`, whereas snapshot output emits `module_15` there. All 15 namespace names and bodies match; `module_15` moves earlier. This is an actual independent reproduction of the documented ordering constraint, rather than a comparison that leaves the gate enabled. Declaration maps themselves remain equal in this example. Three further independent builds per mode and map setting reproduce the same ordinary/snapshot difference, with source-only output equal to ordinary output in all repetitions.

The mechanism is visible in `output.rs`: the namespace generation loop consumes `graph.module_symbols` in SymbolId order (line 244), and the resulting namespace Vec is printed in that order. External imports similarly iterate the SymbolId-keyed import map (line 484). Snapshot construction performs checker symbol queries while collecting import references before semantic checking (`programtosnapshot.rs:226` and `:604`), so it can change lazy ID assignment. The pinned Go snapshot implementation performs the same import-reference queries; no shared compiler behavior was changed. The external-import loop is a confirmed dependency in source, but this experiment reproduced namespace order, not an external-import order difference.

Source-only mode preserves exact artifacts on all six cases and local diagnostic fixtures. This isolates the observed Zod difference to declaration snapshot/check initialization. It does not establish that all possible source snapshots preserve emitted declaration text.

Ten small fixture families, each with maps enabled and disabled, compare independent ordinary, syntax-bypass, all-bypass and source-only builds. All 20 cases preserve exact Rust diagnostic text and successful artifact bytes across modes. They cover star exports, internal/external namespace imports, namespace exports, merged declarations, UMD success/error, deferred global errors, dependency diagnostic order and a stripped declaration with a dangling reference. Three additional small chain/nested/external-star fixtures also preserve bytes; a minimal ordering failure was not found. The public Zod case remains the verified reproduction.

Pinned Go is freshly built from the verified checkout. Ordinary diagnostics are collected through `GetDiagnosticsOfAnyProgram`, matching the bundler collection API. CLI output sorts diagnostics differently for the deferred-global case, so CLI order was not used as the oracle for ordinary API order. The stripInternal case checks both Go source success and TS2304 from checking the emitted declaration; source success alone is not the expected bundle result. A separate oracle pass verifies exact ordered message text for the other 18 fixture cases, including empty successful diagnostic sets. The stripInternal oracle establishes its declaration error code; exact text equality across Rust ablation modes is also checked.

## Measurements

Same public harness/configs and release configuration as the phase profile: system allocator, one checker, `TSRS_FAST=0`, `RAYON_NUM_THREADS=1`, opt3, fat LTO, one codegen unit, full symbols and no stripping. Two warmups and seven measured repetitions per variant/input. Project and variant order rotate; each process checks stable counters/digest against the unmodified baseline. Gate reporting is disabled during measurements. The instrumented binary in normal mode serves as a control for its extra branch/environment checks. No dependency installation or input edits.

Changes below are medians of paired percentages relative to the unmodified baseline, rather than ratios of separate medians. Full ranges and all raw values are saved. Wall time is noisier than instructions/RSS; one normal-control XState run had a large wall outlier.

### Source snapshot with ordinary declaration checking

| Case | Instructions | Peak RSS | Build wall |
| --- | ---: | ---: | ---: |
| immer-no-maps | +1.28% | +1.64% | +3.66% |
| immer-maps | +1.15% | +1.76% | +1.17% |
| zod-no-maps | +0.27% | +0.54% | -2.24% |
| zod-maps | +0.18% | +0.74% | +0.50% |
| xstate-no-maps | +0.51% | +1.08% | +0.39% |
| xstate-maps | +0.47% | +0.98% | +1.48% |

Decision: reject as an optimization. Snapshot setup adds instructions and memory while no declaration diagnostic cache is reused. Every case has positive median instruction/RSS changes. Isolated wall reductions on Zod without maps do not support a general benefit.

### Full syntax bypass on byte-identical workloads

Zod is excluded from these measurements because it already fails the byte-parity guardrail. No performance benefit is claimed for its mismatching output.

| Case | Instructions | Peak RSS | Build wall | Wall ms baseline → trial |
| --- | ---: | ---: | ---: | ---: |
| immer-no-maps | -25.31% | -14.50% | -25.77% | 88.76 → 66.70 |
| immer-maps | -24.46% | -14.38% | -24.98% | 90.62 → 68.05 |
| xstate-no-maps | +1.07% | +1.16% | +1.30% | 261.30 → 264.08 |
| xstate-maps | +0.98% | +1.14% | +0.94% | 293.06 → 295.53 |

Immer improves in all seven pairs for instructions, RSS and wall in each map mode. XState regresses in all seven instruction and RSS pairs. This is useful evidence for targeted reuse, not a general policy: identical eligibility syntax classes can include both a gain and a regression. No production gate was removed or workload-name special case added.

## Decision and limits

Keep the current production gates. The ordering restriction now has direct public reproduction evidence. Reject source-only reuse for performance. The promising next experiment is a conservative eligibility rule based on actual declaration/link requirements, starting with the existing Immer case, with independent error and output parity tests and XState as a regression guard. Such a rule is not established here. Making namespace/import output order depend on stable source provenance could remove the initialization dependency, but a new canonical order must also preserve ordinary output bytes or be treated as a deliberate compatibility change; simply sorting by path is insufficient evidence.

The UMD all-bypass fixture successes are one-build/source-to-declaration evidence only and do not overturn pinned cross-edit stale-diagnostic behavior. Project references, skipped-library checks and external mapper exclusions remain unrelaxed and unmeasured. The 20 local fixtures are development cases, not a locked holdout, and these workloads do not establish catalog-wide compiler performance. No new production tests or full workspace lint run are needed for an experiment with no Rust changes; disposable code was built and exercised directly.

## Evidence

`target/scratch/dts-reuse-gates/` contains the contract and starting hashes, exact overlay and build script, gate reports, captured public artifacts, Go overlay/source and freshly built oracle, fixture results, repeated Zod confirmation, measurements, verification and event log. Measurement data with intact raw process files are in `measure-source-valid/` and `measure-full-valid/`; earlier `measure/` contents include an aborted script-generation trial and must not be used. The initial fixture harness failures were oracle-method mistakes (CLI sorting and checking only source for stripInternal), corrected in `fixture-run3/`.
