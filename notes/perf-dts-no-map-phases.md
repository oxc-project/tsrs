# DTS build phase profile without declaration maps

Profile of the existing public Immer, Zod and six-entry XState builds. Production code and input bytes are unchanged. Instrumentation was applied only to a frozen disposable source copy. This is attribution, not an optimization comparison.

## Findings and next target

Source checking and emitted-declaration checking dominate all three builds. Their combined share, computed within each measured run and then summarized, is 70.08% for Immer (69.44–70.83%), 78.22% for Zod (77.20–79.28%) and 75.90% for XState (75.39–76.03%). These are ordinary checks: every instrumented run recorded `source_reuse_disabled`, with no diagnostic snapshot checks. The table's broader “checking and snapshots” category labels therefore mean ordinary checking for this experiment.

Uninstrumented CPU samples corroborate this attribution: the ordinary compiler diagnostic collector appears in 68.3%, 70.2% and 72.7% of sampled stacks respectively. Individual self samples are spread across allocation, AST/symbol access, scanning and filesystem operations; these short recordings do not establish one narrow checker function as the next optimization.

The first DTS-specific investigation should examine why the existing diagnostic reuse eligibility gate excludes these inputs, and whether a supported reuse path can preserve ordinary diagnostics and exact bundle bytes. `diagnostics::eligible` rejects project references, skipped library checking, external code, and any source file containing UMD namespace exports, namespace imports/exports or bare star exports. This profile records only the aggregate eligibility result; it does not identify each workload's exact rejecting node or condition.

Existing evidence in `docs/DTS.md` matters before changing that gate. Snapshot construction can change symbol initialization order, while linker output currently depends on symbol identity. UMD invalidation also has pinned upstream constraints. The documented namespace output comparison keeps the gate enabled, so it does not independently prove behavior with the gate removed. A follow-up should first record rejection reasons, reproduce any ordering difference with independent Programs, and establish diagnostic/output parity before broadening eligibility. Keep pinned compiler behavior intact. No gate was removed here, and no reuse speedup is established.

The measured declaration check alone is 26.2–34.3% of build time, but that is a phase cost rather than a removable fraction: snapshot construction, invalidation and remaining checking have costs. As an illustrative upper-level estimate, reducing both checking phases by 10% would reduce total build time by about 7–8%, assuming all other costs remain fixed.

Graph analysis is a secondary, workload-specific target: Zod spends 13.17 ms (4.7%), mainly in module/owner/export analysis (11.26 ms), compared with 0.52 ms for Immer and 3.19 ms for XState. Output generation including printing is only 0.3–2.2%, and reachability is below 0.2%. Even eliminating those entire output-generation phases would save only their measured share. Program creation also merits attention if reuse remains blocked, especially on Immer and XState; this phase includes filesystem loading, parsing and binding, and must retain the separate declaration symbol domain described in `docs/DTS.md`.

## Setup

Same curated configs, source/dependency hashes and public library harness as `notes/perf-dts-public-e2e.md`. Fresh process per build; maps disabled, diagnostic reuse requested, no BuildSession cache, `TSRS_FAST=0`, `RAYON_NUM_THREADS=1`, one checker, system allocator, 64 MiB worker stack. Release opt-level 3, fat LTO, one codegen unit, full debug symbols, stripping disabled. Two warmups and seven measured repetitions per variant/input, rotating projects and alternating instrumented/uninstrumented order. All input hashes checked before and after.

RAII timers record nested caller-thread spans, including compiler-worker waits. Inclusive time contains child spans; exclusive time subtracts them. Exclusive spans reconcile to the complete build span, which includes Program teardown. Printer append timing is nested in output generation. Phase percentages are within the instrumented build, not percentages of the separate baseline median.

## Uninstrumented baseline

| Project | Build ms, median [range] | Retired instructions, G | Peak RSS, MiB |
| --- | ---: | ---: | ---: |
| immer-no-maps | 94.93 [89.72, 97.44] | 1.369 | 65.63 |
| zod-no-maps | 290.71 [272.53, 332.67] | 4.603 | 144.47 |
| xstate-no-maps | 281.71 [263.71, 296.83] | 4.105 | 146.53 |

## Phase results

| Exclusive phase | immer-no-maps | zod-no-maps | xstate-no-maps |
| --- | ---: | ---: | ---: |
| Other build work | 0.45 ms (0.5%) | 0.92 ms (0.3%) | 1.23 ms (0.4%) |
| Declaration Program creation | 11.31 ms (12.2%) | 6.65 ms (2.3%) | 19.93 ms (7.3%) |
| Output generation and printing | 0.28 ms (0.3%) | 6.08 ms (2.2%) | 3.47 ms (1.3%) |
| Graph analysis | 0.52 ms (0.6%) | 13.17 ms (4.7%) | 3.19 ms (1.2%) |
| Declaration checking and snapshots | 30.18 ms (32.6%) | 75.87 ms (26.2%) | 97.03 ms (34.3%) |
| Program teardown | 1.02 ms (1.1%) | 4.54 ms (1.6%) | 4.02 ms (1.5%) |
| Reachability | 0.01 ms (0.0%) | 0.56 ms (0.2%) | 0.40 ms (0.1%) |
| Declaration emission and capture | 2.31 ms (2.4%) | 20.02 ms (6.9%) | 11.81 ms (4.2%) |
| Source checking and snapshots | 35.19 ms (37.6%) | 148.15 ms (51.6%) | 115.98 ms (41.6%) |
| Source Program creation | 11.99 ms (12.9%) | 10.87 ms (3.7%) | 21.77 ms (8.0%) |

### immer-no-maps

Instrumented build median 94.28 ms, range 90.20–101.25 ms. Paired instrumented/baseline wall difference median 0.20%, range -2.82–3.91%; this includes host noise.

| Phase path | Calls | Inclusive ms | Exclusive ms | Exclusive share |
| --- | ---: | ---: | ---: | ---: |
| build | 1 | 94.075 | 0.010 | 0.01% |
| build/link | 1 | 43.210 | 0.030 | 0.03% |
| build/link/declaration_program | 1 | 11.310 | 0.012 | 0.01% |
| build/link/declaration_program/create_program | 1 | 11.298 | 11.298 | 12.17% |
| build/link/generate_output | 1 | 0.281 | 0.091 | 0.09% |
| build/link/generate_output/print_append | 73 | 0.190 | 0.190 | 0.20% |
| build/link/graph_analyze | 1 | 0.518 | 0.004 | 0.00% |
| build/link/graph_analyze/graph_edges_and_validation | 1 | 0.126 | 0.126 | 0.13% |
| build/link/graph_analyze/graph_modules_owners_exports | 1 | 0.387 | 0.387 | 0.41% |
| build/link/ordinary_check | 1 | 30.178 | 30.178 | 32.57% |
| build/link/program_free | 1 | 0.496 | 0.496 | 0.52% |
| build/link/reachable | 1 | 0.013 | 0.013 | 0.01% |
| build/program_free | 1 | 0.524 | 0.524 | 0.56% |
| build/public_emit | 1 | 37.463 | 0.001 | 0.00% |
| build/public_emit/emit_and_capture | 1 | 2.311 | 2.311 | 2.44% |
| build/public_emit/ordinary_check | 1 | 35.192 | 35.192 | 37.63% |
| build/references | 1 | 0.260 | 0.260 | 0.28% |
| build/reuse_eligibility | 1 | 0.144 | 0.144 | 0.16% |
| build/source_program | 1 | 11.988 | 0.044 | 0.05% |
| build/source_program/create_program | 1 | 11.943 | 11.943 | 12.82% |

count:source_reuse_disabled: 1 calls.

### zod-no-maps

Instrumented build median 284.25 ms, range 273.78–317.63 ms. Paired instrumented/baseline wall difference median -1.53%, range -4.52–4.08%; this includes host noise.

| Phase path | Calls | Inclusive ms | Exclusive ms | Exclusive share |
| --- | ---: | ---: | ---: | ---: |
| build | 1 | 284.011 | 0.017 | 0.01% |
| build/link | 1 | 104.185 | 0.186 | 0.06% |
| build/link/declaration_program | 1 | 6.648 | 0.035 | 0.01% |
| build/link/declaration_program/create_program | 1 | 6.604 | 6.604 | 2.31% |
| build/link/generate_output | 1 | 6.084 | 2.082 | 0.74% |
| build/link/generate_output/print_append | 1696 | 4.001 | 4.001 | 1.42% |
| build/link/graph_analyze | 1 | 13.166 | 0.007 | 0.00% |
| build/link/graph_analyze/graph_edges_and_validation | 1 | 2.008 | 2.008 | 0.70% |
| build/link/graph_analyze/graph_modules_owners_exports | 1 | 11.264 | 11.264 | 3.97% |
| build/link/ordinary_check | 1 | 75.868 | 75.868 | 26.21% |
| build/link/program_free | 1 | 1.598 | 1.598 | 0.52% |
| build/link/reachable | 1 | 0.560 | 0.560 | 0.20% |
| build/program_free | 1 | 2.805 | 2.805 | 0.99% |
| build/public_emit | 1 | 167.505 | 0.002 | 0.00% |
| build/public_emit/emit_and_capture | 1 | 20.019 | 20.019 | 6.86% |
| build/public_emit/ordinary_check | 1 | 148.153 | 148.153 | 51.55% |
| build/references | 1 | 0.245 | 0.245 | 0.09% |
| build/reuse_eligibility | 1 | 0.469 | 0.469 | 0.16% |
| build/source_program | 1 | 10.866 | 0.040 | 0.01% |
| build/source_program/create_program | 1 | 10.827 | 10.827 | 3.65% |

count:source_reuse_disabled: 1 calls.

### xstate-no-maps

Instrumented build median 276.08 ms, range 261.33–318.76 ms. Paired instrumented/baseline wall difference median -0.25%, range -2.77–7.39%; this includes host noise.

| Phase path | Calls | Inclusive ms | Exclusive ms | Exclusive share |
| --- | ---: | ---: | ---: | ---: |
| build | 1 | 275.853 | 0.019 | 0.01% |
| build/link | 1 | 126.787 | 0.210 | 0.08% |
| build/link/declaration_program | 1 | 19.932 | 0.038 | 0.01% |
| build/link/declaration_program/create_program | 1 | 19.900 | 19.900 | 7.33% |
| build/link/generate_output | 1 | 3.474 | 1.143 | 0.43% |
| build/link/generate_output/print_append | 1094 | 2.356 | 2.356 | 0.85% |
| build/link/graph_analyze | 1 | 3.191 | 0.006 | 0.00% |
| build/link/graph_analyze/graph_edges_and_validation | 1 | 1.259 | 1.259 | 0.46% |
| build/link/graph_analyze/graph_modules_owners_exports | 1 | 1.897 | 1.897 | 0.70% |
| build/link/ordinary_check | 1 | 97.033 | 97.033 | 34.33% |
| build/link/program_free | 1 | 1.407 | 1.407 | 0.53% |
| build/link/reachable | 6 | 0.400 | 0.400 | 0.14% |
| build/program_free | 1 | 2.579 | 2.579 | 0.88% |
| build/public_emit | 1 | 127.793 | 0.002 | 0.00% |
| build/public_emit/emit_and_capture | 1 | 11.808 | 11.808 | 4.21% |
| build/public_emit/ordinary_check | 1 | 115.983 | 115.983 | 41.62% |
| build/references | 1 | 0.286 | 0.286 | 0.10% |
| build/reuse_eligibility | 1 | 0.704 | 0.704 | 0.26% |
| build/source_program | 1 | 21.773 | 0.052 | 0.02% |
| build/source_program/create_program | 1 | 21.727 | 21.727 | 7.98% |

count:source_reuse_disabled: 1 calls.

## Output parity

Captured outputs have identical filenames and bytes across the verified uninstrumented binary, instrumented build and historical public benchmark artifacts. No `.map` outputs are present. Every measured and warmup build has the same stable counters, output digest and successful exit. Timings are excluded from parity. This establishes profiling transparency on these workloads, not equivalence to an independent declaration bundler.

Captured artifacts: 9. Measured executions: 42; warmups: 12.

## CPU sampling

Time Profiler samples use the uninstrumented binary; raw traces and exported stacks are retained. Rust v0 symbols may remain mangled; function-name substrings identify compiler/bundler frames. Sample counts include activity on all sampled threads and are CPU attribution, not direct wall-time proportions.

### immer-no-maps: 293 sampled stacks across 3 runs

- 200 stacks (68.3%): `_RNCNvNtCs2hR9EbBUov4_13tsrs_compiler11programlike35get_diagnostics_of_any_program_like0B5_`
- 200 stacks (68.3%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program30get_diagnostics_of_any_program`
- 173 stacks (59.0%): `_RNvMs_NtCsjgTCbu44vib_12tsrs_checker10checker_07NtNtB6_7checker7Checker24get_diagnostics_exported`
- 95 stacks (32.4%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4link4link`
- 93 stacks (31.7%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project5check`
- 71 stacks (24.2%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program11new_program`
- 71 stacks (24.2%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project6create`
- 37 stacks (12.6%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project14source_program`
- 37 stacks (12.6%): `_RNvXsq_NtCs3bJEMBXHbwl_4core6resultINtB5_6ResultNtCshfPaj1BZIOO_8tsrs_dts11BuildResultNtBM_5ErrorEINtNtNtB7_3ops9try_trait12FromResidualIBy_NtNtB7_7convert10InfallibleB1m_EE13from_residualBM_`
- 34 stacks (11.6%): `_RNvMs_NtCs8Hh7wWMBHrQ_5alloc3vecINtB4_3VechE8as_sliceCshfPaj1BZIOO_8tsrs_dts`
- 34 stacks (11.6%): `_RNvMNtCshfPaj1BZIOO_8tsrs_dts4emitNtB2_20CapturedDeclarations8validate`
- 8 stacks (2.7%): `_RNvXs0_NtCsf60x7h5eRhX_8tsrs_vfs3vfsINtNtCs8Hh7wWMBHrQ_5alloc4sync3ArcDNtB5_2FSEL_EB17_9read_fileCshfPaj1BZIOO_8tsrs_dts`
- 7 stacks (2.4%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4emit25emit_checked_declarations`
- 3 stacks (1.0%): `_RNvXs_NtCshfPaj1BZIOO_8tsrs_dts4hostNtB4_13DeclarationFsNtNtCsf60x7h5eRhX_8tsrs_vfs3vfs2FS16directory_exists`
- 2 stacks (0.7%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts5graph13export_origin`

### zod-no-maps: 904 sampled stacks across 3 runs

- 636 stacks (70.4%): `_RNCNvNtCs2hR9EbBUov4_13tsrs_compiler11programlike35get_diagnostics_of_any_program_like0B5_`
- 635 stacks (70.2%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program30get_diagnostics_of_any_program`
- 579 stacks (64.0%): `_RNvMs_NtCsjgTCbu44vib_12tsrs_checker10checker_07NtNtB6_7checker7Checker24get_diagnostics_exported`
- 231 stacks (25.6%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4link4link`
- 189 stacks (20.9%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project5check`
- 168 stacks (18.6%): `_RNvMNtCsjgTCbu44vib_12tsrs_checker10checker_02NtNtB4_7checker7Checker24check_source_file_worker`
- 70 stacks (7.7%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4emit25emit_checked_declarations`
- 55 stacks (6.1%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project6create`
- 54 stacks (6.0%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program11new_program`
- 34 stacks (3.8%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project14source_program`
- 34 stacks (3.8%): `_RNvXsq_NtCs3bJEMBXHbwl_4core6resultINtB5_6ResultNtCshfPaj1BZIOO_8tsrs_dts11BuildResultNtBM_5ErrorEINtNtNtB7_3ops9try_trait12FromResidualIBy_NtNtB7_7convert10InfallibleB1m_EE13from_residualBM_`
- 22 stacks (2.4%): `_RNvMNtCshfPaj1BZIOO_8tsrs_dts4emitNtB2_20CapturedDeclarations8validate`
- 21 stacks (2.3%): `_RNvMs_NtCs8Hh7wWMBHrQ_5alloc3vecINtB4_3VechE8as_sliceCshfPaj1BZIOO_8tsrs_dts`
- 19 stacks (2.1%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts6output11bundle_impl`
- 19 stacks (2.1%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts6output8generate`

### xstate-no-maps: 811 sampled stacks across 3 runs

- 590 stacks (72.7%): `_RNCNvNtCs2hR9EbBUov4_13tsrs_compiler11programlike35get_diagnostics_of_any_program_like0B5_`
- 590 stacks (72.7%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program30get_diagnostics_of_any_program`
- 536 stacks (66.1%): `_RNvMs_NtCsjgTCbu44vib_12tsrs_checker10checker_07NtNtB6_7checker7Checker24get_diagnostics_exported`
- 276 stacks (34.0%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4link4link`
- 258 stacks (31.8%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project5check`
- 125 stacks (15.4%): `_RNvNtCs2hR9EbBUov4_13tsrs_compiler7program11new_program`
- 125 stacks (15.4%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project6create`
- 67 stacks (8.3%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts7project14source_program`
- 67 stacks (8.3%): `_RNvXsq_NtCs3bJEMBXHbwl_4core6resultINtB5_6ResultNtCshfPaj1BZIOO_8tsrs_dts11BuildResultNtBM_5ErrorEINtNtNtB7_3ops9try_trait12FromResidualIBy_NtNtB7_7convert10InfallibleB1m_EE13from_residualBM_`
- 58 stacks (7.2%): `_RNvMs_NtCs8Hh7wWMBHrQ_5alloc3vecINtB4_3VechE8as_sliceCshfPaj1BZIOO_8tsrs_dts`
- 58 stacks (7.2%): `_RNvMNtCshfPaj1BZIOO_8tsrs_dts4emitNtB2_20CapturedDeclarations8validate`
- 39 stacks (4.8%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts4emit25emit_checked_declarations`
- 12 stacks (1.5%): `_RNvMNtCsjgTCbu44vib_12tsrs_checker10checker_02NtNtB4_7checker7Checker24check_source_file_worker`
- 9 stacks (1.1%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts6output11bundle_impl`
- 9 stacks (1.1%): `_RNvNtCshfPaj1BZIOO_8tsrs_dts6output16generate_entries`

## Evidence and limits

Raw evidence is under `target/scratch/dts-phase-profile/`: contract, starting hashes, instrumented source, build provenance, captures, per-process logs/counters, per-stage TSVs, trace exports, results and independent summary. Only three existing curated public workloads are covered. No dependency installation, network access, commits, publication or production optimization was performed. Profile overhead and external machine load limit fine-grained interpretation.
