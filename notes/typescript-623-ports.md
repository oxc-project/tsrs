# TypeScript ports for issue 623

Port [issue 623](https://github.com/oxc-project/tsrs/issues/623) in upstream chronological order, one tsrs commit per
upstream commit, including evidence-only commits for changes that need no Rust code. Each corresponding commit has
an `Upstream-Commit` trailer. Assign each upstream commit to a dedicated subagent, and finish its validation and
corresponding commit before starting the next subagent. Start at `b85298b6a81f772d080b0455de0ca9d744cd6fd6`; finish at
`5237fd46890956e4bb8177a1a05dcd69e4166560` (112 commits). Keep the published TypeScript pin until the final entry.

Each entry must retain every passing checker baseline in both histories and pass the emit baselines, workspace
checks, Clippy, source checks, and generated-code verification. Run the affected subsystem's additional gates.
Entries 1–10 used paired performance comparisons on `vscode`, `webpack`, `Compiler-Unions`, and `t3code-server`.
The initial entry 11 investigation expanded the planned cohort to the six TypeScript benchmark projects plus
`t3code-server`, in single-threaded and default modes. The user subsequently instructed us to continue the
migration without a performance-regression gate. From entry 11 onward, correctness remains mandatory; disclose
instruction, wall-time, and peak-RSS measurements when available, including confirmed regressions, but do not
withhold a correctness-valid corresponding commit for a performance result or an unmeasured cohort cell. Keep
measurement artifacts. For no-op ports, byte-identical compiler binaries and runtime assets establish that their
performance is unchanged. Reuse completed correctness checks only when their inputs are demonstrably unchanged;
compare complete passing sets rather than totals alone.

After the identical-binary control in entry 2 also failed the original zero-tolerance median rule, the user
instructed removal of that blocking rule and continuation without human guidance. Routine migration work requires
no further human approval. The historical replacement measurement policy, also used for focused entry 11
diagnoses, was `paired-sign-test-v1`; it is no longer a landing gate from entry 11:

- For a measured cohort, retain all three raw before/after medians and every pair in each measured cell (eight
  in the original cohort; fourteen in the extended cohort).
- For each metric, compute paired relative changes and the exact one-sided sign-test probability
  `P[Binomial(n_non_tied, 0.5) >= positive_pairs]`, excluding exactly equal pairs.
- A positive raw median change screens for confirmation when that probability is at most 0.10.
- Run exactly one independent ten-pair confirmation batch per screened cell against the same preceding binary,
  reversing the initial pair order. Evaluate every screened metric in that cell.
- A reproducible regression has positive raw median changes in both batches and a pooled paired sign-test
  probability at most 0.01. Retain and disclose that evidence. Do not repeat unchanged candidates until a
  favorable batch appears.
- Otherwise report **no detected regression**, not proof of exact equality. Keep old strict-rule records unchanged;
  new verdicts identify their input files by SHA-256. Binary/runtime identity still suffices for a no-op port.

The reusable local evaluator and confirmation collector are `target/port-623/noise_gate.py`; its local manifest
records the earlier measurement rules. Evaluator sanity checks cover identical samples, consistent
increases, exact binomial tails, excluded ties and mixed samples. The explicit cohort
`typescript-six-plus-t3-v1` requires all fourteen cells; legacy reports retain their eight-cell requirement.
The cohort extension preserves the equations and thresholds and reproduces the earlier accepted and rejected
verdicts unchanged. Its evaluator rejects missing, duplicate, unknown and mismatched cells when that cohort is
requested; running it is not required to land entries 11 onward.

Prerequisite `55e77eaf` isolates the macOS watch fixtures and replaces the OS-dependent event-drop stress test with
deterministic callback injection. No compiler behavior changes. Workspace: 1,064 passed, four existing ignored;
checker passes: 13,458 diagnostics and 12,779 types/symbols in Go history (12,778 types/symbols in canonical history);
emit: 13,462 diagnostics, 13,392 JS/declaration outputs, 149 JS maps, and 156 source maps. No crashes or timeouts.

## 1. Azure Node installation task — passed

Upstream: `9adc871ff47f79b3e99ff2b1cbe9efb140d76af8` — Update Azure pipeline Node installation task (#64547).

Only `tools/pipelines/steps/setup-node-npm-ci.yml` and `tools/pipelines/steps/setup-vsce.yml` change: `NodeTool@0`
becomes `UseNode@1`, and its version input is renamed. tsrs has no corresponding Azure pipelines. No runtime or
fixture changes are needed: the upstream `tsc` tree is `ca347d58f39812fa168e48469f3d2c442467c9ee` at both the old pin
and this commit. The tsrs change is this evidence record; the compiler binary is unchanged.

Validation retained all baseline passes in both checker histories: 13,458 diagnostics and 12,779 types/symbols in
Go history; 13,458 diagnostics and 12,778 types/symbols in canonical history. The existing four diagnostic
differences and canonical `objectLiteralNormalization` type/symbol difference remain unchanged. The emit run
retained 13,462 diagnostics, 12,779 types/symbols, 13,392 JS/declaration outputs, 149 JS maps, and 156 source maps.
All complete passing sets match the baseline; no crashes or timeouts occurred. The unchanged workspace inputs
retain the prerequisite's 1,064 passing checks, four existing ignored checks, native/WASM checks, generated-code
freshness, and regression cases. Clippy, source checks, and formatting also pass.

Performance passes by binary and runtime-asset identity. The baseline and candidate release binaries both have
SHA-256 `ca8457b2c4540510e4ff45c2a111c97b18ea963f9af8c9bea5c8226c9622e23f`. All 113 bundled library files are
byte-identical; their sorted path-to-SHA-256 JSON manifest has SHA-256
`07e760b2613fce61b0eaa87c4cd965264d812ae4fb8b9dc522b1ba5e37e8c6a7`. These libraries are embedded in the default
release compiler. No compiler or runtime changes, additional fixes, or capability changes were needed.

Candidate conformance, pass-set comparison, release build, and identity artifacts are retained locally under
`target/port-623/0001-9adc871ff4/`; the preceding release binary and bundled-library snapshot are under
`target/port-623/baseline/`. Inherited workspace and generated-code evidence is under
`target/port-623/watch-diagnosis/`. The published TypeScript pin remains unchanged.

## 2. Late expando alias exports — passed

Upstream: `697b847e11e91dfbff79f1d189b802b751999f03` — Add export modifiers to other expando exports of
namespaces on all codepaths which add export declarations (#64545).

The declaration transformer checks for an existing export declaration before the alias branch and calls
`add_export_modifier_to_expando_members` before adding either an alias export or a renamed export. Earlier
namespace members remain exported; the initial variable of a renamed export remains private. The new upstream
`declarationEmitLateExpandoAliasAddsExports` case fails its declaration output before the port and passes after it.
The ignored reference checkout advances to this source revision; the published TypeScript pin stays unchanged.

Both checker histories retain every previous passing baseline and add the new case: 13,459 diagnostics and
12,780 types/symbols in Go history; 13,459 diagnostics and 12,779 types/symbols in canonical history. The four
existing checker diagnostic differences and canonical `objectLiteralNormalization` difference are unchanged.
Full emit passes 13,463 diagnostics, 12,780 types/symbols, 13,393 JS/declaration outputs, 149 JS maps and 156 source
maps. Complete pass-set comparisons find no losses, crashes or timeouts. All ten expando declaration oracle cases
match a Go compiler built at this revision. Workspace checks pass (1,064 passed, four existing ignored), as do
native/WASM checks, Clippy, source checks, formatting and all 36 regressions. Regeneration leaves every crate file
byte-identical to its pre-generation snapshot. No additional source fixes were required.

The original release comparison stopped at its first cell because the superseded strict gate rejected any median increase. VS Code,
single-threaded, ten alternating pairs after warmup: instructions 79,564,397,188.5 → 79,568,375,304.5 (+0.0050%);
wall 7.4676780835 → 7.447164208 seconds (-0.2747%); peak RSS 1,685,700,608 → 1,685,700,608 bytes. The VS Code
configuration enables neither `declaration` nor `composite`, so the changed declaration transformer does not run
in this `--noEmit --incremental false` workload.

An identical-binary control using the same saved predecessor path and SHA-256 on both sides also failed that
gate: instructions 79,517,198,584 → 79,515,883,546.5 (-0.0017%); wall 7.4730112505 → 7.478658208 seconds
(+0.0756%); peak RSS remains 1,685,700,608 bytes. Both batches retain all ten pairs; no tolerance or unchanged
candidate retries were introduced. The remaining seven cells were then collected once as evidence, retaining
the original VS Code single-threaded cell and every strict failure flag. Those original records retain their
`acceptance_passed=false`; the revised-policy verdict is separate.

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after | Strict increases |
| --- | ---: | ---: | ---: | --- |
| vscode / single | 79.564397 → 79.568375 | 7.467678 → 7.447164 | 1607.609 → 1607.609 | instructions |
| vscode / default | 91.443870 → 91.373049 | 1.006581 → 0.999924 | 2108.453 → 2109.805 | RSS |
| webpack / single | 10.566873 → 10.566473 | 0.905326 → 0.889525 | 311.344 → 311.328 | none |
| webpack / default | 14.680447 → 14.527130 | 0.200681 → 0.199886 | 498.531 → 493.344 | none |
| Compiler-Unions / single | 3.211941 → 3.211971 | 0.259233 → 0.263429 | 59.625 → 59.625 | instructions, wall |
| Compiler-Unions / default | 3.885955 → 3.885974 | 0.148487 → 0.145966 | 83.031 → 83.203 | instructions, RSS |
| t3code-server / single | 38.137103 → 38.372979 | 3.164334 → 3.173378 | 728.703 → 728.703 | instructions, wall |
| t3code-server / default | 122.113708 → 121.988545 | 1.351808 → 1.324767 | 2096.805 → 2091.438 | none |

The t3code-server configuration also has neither declaration nor composite enabled. Compiler-Unions enables
declaration, but its 41 semantic diagnostics prevent the declaration-diagnostics phase from running. These
configuration facts do not exclude whole-binary layout effects from the release rebuild. In single-threaded
t3code-server, eight of ten pairs increase instructions and wall time; their respective paired ranges are
-0.0156% to +0.8920% and -1.0021% to +8.3221%. Compiler-Unions has five increasing instruction pairs in each
mode. These initial batches establish no performance cause.

Applying `paired-sign-test-v1` screens only t3code-server/single instructions and wall time: each has eight
positive pairs out of ten, exact p = 0.0546875. The one independent confirmation batch reverses the initial pair
order and yields instructions 38,160,143,868.5 → 38,159,962,929.5 (-0.000474%); wall 3.177846896 → 3.1720029795
seconds (-0.183895%); peak RSS 764,100,608 → 764,100,608 bytes. It has five positive instruction pairs and four
positive wall-time pairs. Pooled probabilities are 0.1315879822 for instructions and 0.2517223358 for wall time;
neither raw median increase repeats. The verdict is **no detected regression** across all eight cells. No
performance source changes were needed; this does not establish exact performance equality.

Artifacts are under `target/port-623/0002-697b847e11/`: full conformance lists, `pass-set-comparison.json`, generator
snapshots, gate logs, the Go reference binary, the frozen candidate `tsrs-release` and 113 bundled library files,
and `identity.json`. `perf-attempt-1.log` records the initial Python 3.9 setup error; the runner needs Python 3.12.
`perf-attempt-2/` contains the failed candidate comparison, and `perf-control-aa/` the failed identical-binary
control. `perf-remaining/results.json` retains all eight original cells and strict flags; `perf-summary.json`
records every paired delta. `perf-confirmation/` records the sole confirmation batch, and
`noise-policy-verdict.json` records the accepted verdict with SHA-256 references to both input datasets. The
accepted release binary SHA-256 is `83daa2fa410ca5fbb6bc02ff35ac70022d11841fbdf290bd737425ba19571134`; all 113
bundled library files remain byte-identical to the preceding accepted snapshot.

## 3. Named expression declaration names — passed

Upstream: `81d3f3f5e89786acc33d1881a0864bb0bd68068b` — Fix flaky diagnostic added by declaration emit through
`MarkLinkedReferencesRecursively` (#64482).

`is_in_expression_context` now treats the name of a function or class expression as a declaration name, matching
the new Go switch arm. Before the port, `noFlakyDiagnosticsExpressionNameShadowedInBody1` gained a spurious
post-emit TS2448 diagnostic for the function name, and its `.types` baseline incorrectly resolved that name to the
shadowing body's `number` variable. After the port, its diagnostics, types, symbols and JavaScript all pass. The
same case's emitted files match a Go compiler built at this source revision. The ignored reference checkout
advances to this revision; the published TypeScript pin stays unchanged.

Both checker histories retain every previous passing baseline and add the new case: 13,460 diagnostics and
12,781 types/symbols in Go history; 13,460 diagnostics and 12,780 types/symbols in canonical history. Full emit
passes 13,464 diagnostics, 12,781 types/symbols, 13,394 JS/declaration outputs, 149 JS maps and 156 source maps.
All twelve complete passing sets retain every preceding pass, with no crashes or timeouts. The four existing
checker diagnostic differences and canonical `objectLiteralNormalization` difference remain unchanged.

Workspace checks pass (1,064 passed, four existing ignored), as do native/WASM checks, Clippy, source checks,
formatting and all 36 regressions. Regeneration leaves all 1,167 crate files byte-identical to their pre-generation
snapshot. Because editor type and symbol queries share the changed predicate, the full Go fourslash gate also
runs at this source revision: 4,074 exercised groups pass, 472 are skipped, and none fail. An isolated metadata
wrapper selects the current source without changing the published pin. No additional source fixes were required.

Performance compares the frozen preceding release with this candidate across all eight project/mode cells under
`paired-sign-test-v1`. The reusable `target/port-623/collect.py` collects every initial cell without the superseded
raw-median stop; `noise_gate.py` evaluates those measurements and collects the prescribed confirmation batches.
One warmup and ten alternating pairs per cell yield these initial medians:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.775161 → 79.759270 | 7.513546 → 7.511081 | 1607.609 → 1607.609 |
| vscode / default | 91.417699 → 91.442431 | 1.039140 → 1.023495 | 2107.812 → 2109.258 |
| webpack / single | 10.570323 → 10.568779 | 0.902873 → 0.904547 | 311.328 → 311.328 |
| webpack / default | 14.627140 → 14.504238 | 0.202013 → 0.204132 | 495.719 → 491.234 |
| Compiler-Unions / single | 3.212018 → 3.212066 | 0.260791 → 0.259606 | 59.625 → 59.625 |
| Compiler-Unions / default | 3.886428 → 3.883809 | 0.148370 → 0.147829 | 83.305 → 83.219 |
| t3code-server / single | 38.183003 → 38.181974 | 3.197595 → 3.206484 | 728.703 → 728.703 |
| t3code-server / default | 121.235186 → 121.856587 | 1.370333 → 1.379313 | 2083.805 → 2088.070 |

Only webpack/default wall time and t3code-server/single wall time screen for confirmation: each has eight positive
pairs out of ten, exact p = 0.0546875. Each receives exactly one independent ten-pair batch with reversed pair
order. webpack/default wall time changes from 0.2029849795 to 0.202675500 seconds (-0.152464%); its pooled p is
0.0576591492. t3code-server/single changes from 3.1910962295 to 3.209990125 seconds (+0.592082%), but only four
confirmation pairs increase, and pooled p is 0.2517223358. Neither meets the confirmed-regression rule. The verdict
is **no detected regression**, not proof of exact equality; no performance source changes were needed. All
measured runs retain identical diagnostics and exit codes. No build or correctness workload overlaps measurement.

Artifacts are under `target/port-623/0003-81d3f3f5e8/`: before/after reproduction, complete conformance lists,
`pass-set-comparison.json`, unchanged-difference lists, generator snapshots, gate logs, isolated fourslash
metadata/reference/JSON results, the Go oracle, and the frozen release and 113 bundled library files.
`perf-initial/` and `perf-confirmation/` retain every raw sample; `noise-policy-verdict.json` references both
datasets by SHA-256. `identity.json` records binary, asset and evidence hashes. The accepted release binary has
SHA-256 `994dfbdb156784716df7ed9cb8bf342769f91b38624afec1f701d9450234cb56`; all 113 bundled library files remain
byte-identical to the preceding accepted snapshot.

## 4. Cyclic structures and declaration truncation — passed

Upstream: `834e7862c031a3f5f24c53acf51056a4f558c3e4` — Report cyclic structures and truncation during declaration
emit (#64461).

The node builder routes anonymous and union cycle guards through `create_cyclic_structure_placeholder`, which
reports the cyclic-structure error unless anonymous identifiers are allowed. Exceeding the symbol nesting limit
also sets `truncating`, so declaration emit reports the existing serialization-length diagnostic. This follows
the Go helper, call sites and control flow. The new `declarationEmitUnserializableTypes` case previously omitted
four TS5088 diagnostics and one TS7056 diagnostic; it now matches the reference, including the explicitly
annotated recursive declarations. Across all 13 changed or new fixtures, the pre-port run had 11 diagnostic and
JavaScript/declaration failures and three type failures; every diagnostic, type, symbol and emit baseline passes
after the port. All 13 cases also match the emitted files of a Go compiler built at this revision (one produces
no files). The additional filesystem gate repairs and performance repair are described below.

Both checker histories retain every previous passing baseline and add the new case: 13,461 diagnostics and
12,782 types/symbols in Go history; 13,461 diagnostics and 12,781 types/symbols in canonical history. Full emit
passes 13,465 diagnostics, 12,782 types/symbols, 13,395 JS/declaration outputs, 149 JS maps and 156 source maps.
All twelve complete passing sets retain every preceding pass, with no crashes or timeouts. The four existing
checker diagnostic differences and canonical `objectLiteralNormalization` difference remain unchanged. No
upstream cases were removed or renamed.

Workspace checks pass (1,066 passed, four existing ignored), as do native/WASM checks, Clippy, source checks,
formatting and all 36 regressions. Regeneration leaves all 1,168 crate files byte-identical to their pre-generation
snapshot. The full Go fourslash gate at this revision passes the same 4,074 exercised groups, with 472 skipped
and none failing; its complete passing group set is unchanged. An isolated metadata wrapper selects this source
without changing the published pin.

Two unrelated filesystem failures surfaced during revalidation and were repaired in this corresponding commit.
The FSEvents burst fixture required all 20 files in one callback, but received valid batches of one and 19 files:
its five-millisecond sleeps cannot guarantee the OS native batch shape. The real-filesystem fixture now waits
for the exact delivered path set. An isolated controlled callback case verifies that a single complete file
is immediate while one native multi-file batch holds the leading edge and delivers all files together.

A later parallel workspace run panicked in `ReadDir` cleanup with EBADF. Inspection found that kqueue cached raw
descriptors in subscriptions after an entry had been closed or replaced. A deterministic isolated regression
proved that both watch close and backend shutdown closed an unrelated file that reused the old descriptor,
leaked the replacement descriptor, and leaked one descriptor when overlapping watches were closed. The fix
removes redundant descriptor ownership from subscriptions: shutdown closes the current descriptor registry,
and watch close releases an entries map's current descriptors when its last subscription leaves. The regression
now passes all close, shutdown and overlap cases. The stale-descriptor bug also exists in the reference Go
algorithm; this is an explicit Rust I/O ownership repair. It is consistent with the observed parallel EBADF,
whose precise scheduling race was not reconstructed. Native filesystem and CLI watch integration gates pass.

The initial release failed the agreed performance gate on Compiler-Unions/single peak RSS. Its first ten-pair
median increased from 62,537,728 to 62,554,112 bytes (+16 KiB, +0.0262%); the independent reversed-order
confirmation increased from 62,521,344 to 62,545,920 bytes (+24 KiB, +0.0393%). Across both batches, 16 pairs
increase and four tie, with no decreases: pooled sign-test p = 0.0000152588. Compiler-Unions/default instructions
also screened initially but did not confirm (pooled p = 0.0576591492). The failed candidate and every raw sample
remain at their original paths. Initial medians for that candidate:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.531368 → 79.519333 | 7.362400 → 7.353514 | 1607.609 → 1607.609 |
| vscode / default | 91.335771 → 91.360507 | 0.997510 → 1.000161 | 2122.750 → 2116.070 |
| webpack / single | 10.566985 → 10.566332 | 0.895866 → 0.915597 | 311.328 → 311.328 |
| webpack / default | 14.617355 → 14.602850 | 0.205301 → 0.204599 | 496.203 → 496.938 |
| Compiler-Unions / single | 3.211780 → 3.211759 | 0.265617 → 0.263570 | 59.641 → 59.656 |
| Compiler-Unions / default | 3.882342 → 3.884291 | 0.148847 → 0.150356 | 83.258 → 83.242 |
| t3code-server / single | 38.155625 → 38.158179 | 3.122199 → 3.153594 | 728.703 → 728.703 |
| t3code-server / default | 121.885298 → 121.808789 | 1.346027 → 1.339640 | 2091.891 → 2090.469 |

A temporary entry probe showed neither changed branch executes in this workload. The release's data-section
sizes are unchanged; its text grows by 128 bytes, and page reclaims and physical footprint usually rise with RSS.
These observations suggest code generation affecting stack or other dirty-page residency; they do not establish
the exact mechanism. The existing rare-path and layout notes were reviewed before experimenting. Marking the new
helper `#[cold]` did not repair the focused RSS increase (+32 KiB), and plain `#[inline]` produced identical
executable sections, so both were rejected. Each measured experimental candidate received one focused batch; failed candidates were retained.

The repair keeps only `#[inline(never)]` on `create_cyclic_structure_placeholder`, preserving the Go helper and
control flow while preventing its recovery body from being inlined into callers. A focused ten-pair comparison
then gives identical peak RSS medians of 62,521,344 bytes; instructions change by +0.00512% and wall time by
+2.00303%, so this focused experiment is not an acceptance verdict. No cache, thread, allocator or algorithm
change is involved. Both checker histories, full emit, all 36 regressions and full fourslash were rerun for the repaired compiler.
The final filesystem repair changes no compiler inputs; its workspace, native/WASM, lint and generator gates
are rerun before full measurement.

The final release is compared with the same frozen source-3 baseline across all eight project/mode cells under
`paired-sign-test-v1`, with one warmup and ten alternating pairs per cell. Final initial-batch medians:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.500755 → 79.505384 | 7.289498 → 7.282193 | 1607.609 → 1607.578 |
| vscode / default | 91.285955 → 91.347801 | 0.984374 → 1.001522 | 2115.078 → 2111.188 |
| webpack / single | 10.562779 → 10.562005 | 0.906454 → 0.904980 | 311.328 → 311.305 |
| webpack / default | 14.650723 → 14.651033 | 0.205286 → 0.203064 | 497.578 → 498.086 |
| Compiler-Unions / single | 3.211742 → 3.211923 | 0.264454 → 0.266140 | 59.625 → 59.594 |
| Compiler-Unions / default | 3.884599 → 3.884684 | 0.150327 → 0.149670 | 83.133 → 82.984 |
| t3code-server / single | 38.134394 → 38.135493 | 3.110807 → 3.133931 | 728.688 → 728.672 |
| t3code-server / default | 122.154342 → 122.007797 | 1.372061 → 1.343244 | 2093.641 → 2091.094 |

Compiler-Unions/single instructions screen at +0.00564%; the sole confirmation gives +0.00272%, with five
positive and five negative pairs. Its pooled p = 0.0206947327 exceeds the 0.01 confirmation threshold.
t3code-server/single wall time screens at +0.74332%; its sole confirmation decreases by 0.37563%, with pooled
p = 0.4119014740. No RSS metric screens. The final verdict is **no detected regression**, not exact equality.
Every run preserves the baseline's output and exit status. No unchanged candidate was retried for a favorable
result; each screened cell received exactly the one prescribed confirmation.

The accepted frozen binary is `target/port-623/0004-834e7862c0/candidate-5/tsrs-release`, SHA-256
`36e55d4877ca18488bfddcecc7bf622278a1d843004e0ac3c78651a0f79a1af8`; all 113 runtime library files remain
byte-identical to the preceding snapshot. Artifacts under `target/port-623/0004-834e7862c0/` retain the upstream
patch, before/after reproductions, all complete passing sets, rejected immutable candidates and raw measurements,
controlled descriptor-reuse failure and repair, and final source/input identities. `candidate-4/` contains the
repaired compiler's full conformance, emit, oracle and fourslash evidence; `candidate-5/` records the final
filesystem, workspace, native/WASM, lint, formatting, generator and regression gates plus unchanged compiler
input evidence. Final performance data are in `perf-final/` and `perf-final-confirmation/`, with the acceptance
verdict in `noise-policy-final-verdict.json`. The published TypeScript pin remains unchanged.

## 5. Static properties and self-contextual typing — passed

Upstream: `619d485a639c83a2d82860b5797db4984cecfc91` — Avoid contextually typing static properties by their own
class to prevent spurious circularities (#64525).

`get_contextual_type_for_static_property_declaration` now rejects a contextual parent type whose symbol is the
class's own declaration symbol, preserving the Go short-circuit order. Before the port, the new
`classStaticPropertyNoSpuriousCircularityError1` fixture reports four spurious TS7022 diagnostics and infers `any`
instead of `42`, `number`, `number[]` and `{}`. All its diagnostics, types, symbols and emit baselines now pass.
A Go compiler built at this exact source revision produces byte-identical diagnostics and the same exit code;
the seven genuine circular-reference diagnostics remain. The ignored reference checkout advances; the published
TypeScript pin stays unchanged. The additional performance repair is described below.

Both checker histories retain every preceding passing baseline: 13,462 diagnostics and 12,783 types/symbols in
Go history; 13,462 diagnostics and 12,782 types/symbols in canonical history. Full emit passes 13,466 diagnostics,
12,783 types/symbols, 13,396 JS/declaration outputs, 149 JS maps and 156 source maps. All twelve complete passing
sets retain every preceding pass, with no crashes or timeouts. The four existing checker diagnostic artifacts and
canonical `objectLiteralNormalization` type/symbol difference remain unchanged. No upstream cases were removed.

The final candidate passes the full workspace gate (1,066 passed, four existing ignored), native/WASM checks,
Clippy, source checks, formatting and all 36 regressions. Regeneration leaves all 1,168 crate files byte-identical.
Full Go fourslash passes the same 4,074 exercised groups, with 472 skipped; its complete passing set is unchanged.
The initial sandboxed bridge check could not bind its Unix socket; its failure log is retained, and the gate passes
with authorized local-socket access. The isolated metadata wrapper selects this source without changing the pin.

The first release confirms single-threaded RSS regressions under `paired-sign-test-v1`: VS Code rises by 16 KiB
in both batches (pooled p = 0.0000381470); Compiler-Unions rises by 24 KiB initially and 16 KiB in confirmation
(p = 0.0000076294); t3code-server rises by 16 KiB in both batches (p = 0.0020904541). webpack/default wall time
and t3code-server/default RSS screen initially but do not confirm. Every failed raw sample and the original
immutable release remain at their original paths.

After reviewing the existing performance notes and source-4 diagnosis, temporary entry probes find that
Compiler-Unions never calls this helper, t3code-server calls it seven times without reaching the new guard, and
VS Code calls it 331 times and reaches the guard 12 times. The added guard cannot explain the Compiler-Unions
increase through executed work. The release's text grows by 64 bytes, with unchanged segment and data-section
sizes. Mach-O function boundaries identify two bodies growing by 48 and 16 bytes; their stack prologues remain
112 and 80 bytes. Symbols are stripped, so their identities are not established. This does **not** demonstrate
larger stack frames; code generation affecting page residency remains a hypothesis, not an established mechanism.

The repair adds only `#[inline(never)]` to this existing static-property helper, preserving the Go function and
control flow and the preceding cyclic-recovery helper's attribute. It adds no cache, thread or allocator change.
This narrow experiment responds to the newly confirmed port regression; it does not repeat the broad contextual
member outlining rejected in earlier notes. A focused ten-pair Compiler-Unions comparison changes peak RSS from
62,488,576 to 62,472,192 bytes (-16 KiB), instructions by -0.003004%, and wall time by -0.709589%. That focused
batch is diagnostic evidence, not acceptance. The temporary probes are removed, and all correctness gates above
are rerun on the repaired candidate.

The repaired release is compared against source 4's accepted `candidate-5/tsrs-release`, with one warmup and ten
alternating pairs in all eight configurations. No builds or correctness workloads overlap measurement. Final
initial-batch medians:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.586344 → 79.590718 | 7.339378 → 7.313885 | 1607.578 → 1607.562 |
| vscode / default | 91.496305 → 91.472670 | 0.992566 → 0.989575 | 2107.539 → 2108.641 |
| webpack / single | 10.563613 → 10.561301 | 0.899224 → 0.904487 | 311.297 → 311.266 |
| webpack / default | 14.541304 → 14.593499 | 0.203591 → 0.203820 | 494.219 → 495.250 |
| Compiler-Unions / single | 3.211762 → 3.211699 | 0.264822 → 0.268189 | 59.594 → 59.578 |
| Compiler-Unions / default | 3.884001 → 3.886338 | 0.149279 → 0.149961 | 83.281 → 83.062 |
| t3code-server / single | 38.142110 → 38.142063 | 3.150723 → 3.115938 | 728.672 → 728.641 |
| t3code-server / default | 121.745705 → 122.217984 | 1.357603 → 1.362288 | 2093.336 → 2097.469 |

Only Compiler-Unions/single wall time screens (+1.271514%, eight positive pairs of ten). Its sole independent
reversed-order confirmation increases by 1.666293%, with six positive and four negative pairs; pooled
p = 0.0576591492 exceeds the 0.01 threshold. No RSS or instruction metric screens. The final verdict is **no
detected regression**, not exact equality. Every measured run preserves the predecessor's output and exit
status. No unchanged candidate was retried for a favorable result.

Artifacts are under `target/port-623/0005-619d485a63/`: the upstream patch, before/after reproduction and Go oracle,
initial complete conformance sets, rejected release and raw `perf-initial/` / `perf-confirmation/` datasets,
`noise-policy-verdict.json`, and causal probes/disassembly under `diagnosis/`. `candidate-2/` contains the final
full correctness evidence, complete pass-set comparisons, unchanged source-input snapshots, frozen release and
113 byte-identical runtime library files. Final performance samples are in `perf-final/` and
`perf-final-confirmation/`, referenced by SHA-256 in `noise-policy-final-verdict.json`. The accepted release is
`candidate-2/tsrs-release`, SHA-256 `9c6bba2d437088e45963d774a9f6e2fa7d54614df33b3d49345ab3860634a46a`.


## 6. String literal module import attribute types — passed with build-identity adjudication

Upstream: `792ffccb90f548cd3bdd7fc0b44f0ca9e1b169a7` — fix: disallow NoSubstitutionTemplate in module import
attribute types (#64243).

`check_grammar_import_attributes_type` now requires a literal type node containing a string literal, preserving
the Go short-circuit order before accessing the literal. The updated `importAttributeType1` fixture fails before
the port because the no-substitution template `css` lacks TS1555; its interpolated template already errors.
After the port, all 33 import-attribute variants pass, including 28 applicable type/symbol and 32 emit baselines.
A Go compiler built at this exact source revision produces byte-identical diagnostics and the same exit status
for the materialized upstream case. The ignored reference checkout advances; the published TypeScript pin stays
unchanged. The measurement problem and its controlled resolution are described below.

All twelve complete passing sets exactly match source 5's accepted candidate: 13,462 diagnostics and 12,783
types/symbols in Go checker history; 13,462 diagnostics and 12,782 types/symbols in canonical history; full emit
passes 13,466 diagnostics, 12,783 types/symbols, 13,396 JS/declaration outputs, 149 JS maps and 156 source maps.
There are no crashes or timeouts. The four existing checker diagnostic artifacts and canonical
`objectLiteralNormalization` type/symbol difference remain unchanged.

The workspace passes 1,066 checks with four existing ignored. Native/WASM checks, Clippy, source checks,
formatting and all 36 regressions pass. All seven generators plus formatting leave all 1,168 crate files
byte-identical. Full Go fourslash retains every one of the preceding 4,074 exercised groups and 4,130 passing
events, with 472 groups skipped; its six Python gate checks and local-socket bridge checks pass. The isolated
metadata wrapper selects this source without changing the published pin. The initial Go oracle build could not
access the default Go cache in the sandbox; its retained failure log precedes the successful build using a cache
under `target/`.

Performance compares the frozen preceding `candidate-2/tsrs-release` with this release on all four public
projects, single-threaded and at the default checker count, under `paired-sign-test-v1`. No build or correctness
workload overlaps measurement. One warmup and ten alternating pairs per cell give these initial medians:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.709651 → 79.714917 | 7.367043 → 7.384303 | 1607.562 → 1607.594 |
| vscode / default | 91.435059 → 91.530841 | 1.064486 → 1.085740 | 2112.938 → 2109.008 |
| webpack / single | 10.563169 → 10.561039 | 0.905310 → 0.864459 | 311.266 → 311.312 |
| webpack / default | 14.576860 → 14.643088 | 0.207951 → 0.209863 | 494.555 → 497.656 |
| Compiler-Unions / single | 3.211774 → 3.211768 | 0.266543 → 0.268167 | 59.578 → 59.594 |
| Compiler-Unions / default | 3.884214 → 3.883863 | 0.149601 → 0.144370 | 83.266 → 83.070 |
| t3code-server / single | 38.149629 → 38.150524 | 3.126021 → 3.154399 | 728.641 → 728.672 |
| t3code-server / default | 122.045047 → 122.229939 | 1.315348 → 1.335166 | 2102.773 → 2099.227 |

The ordinary fixed-build verdict **confirms four single-thread RSS increases**: VS Code +32 KiB in both
batches (pooled p = 0.0000009537), webpack +48 KiB initially and +40 KiB in confirmation (p = 0.0000009537),
Compiler-Unions +16 KiB in both (p = 0.0000305176), and t3code-server +32 KiB in both (p = 0.0000019073).
Other screened instruction/wall metrics do not confirm. This verdict remains unchanged at
`noise-policy-verdict.json`; it is not relabeled passed. Every original and rejected candidate and raw sample
remains at its original path.

After reviewing earlier rejected work, temporary entry probes observe zero calls to this grammar helper in
VS Code, Compiler-Unions and t3code-server. Mach-O inspection finds unchanged text-section size and unchanged
192-byte parent stack frame. Three focused experiments fail: outlining the grammar helper gives +16 KiB
Compiler-Unions RSS; adding cold placement gives +64 KiB; outlining the parent reduces its frame to 160 bytes
but gives +24 KiB. All attributes are reverted. These observations do not establish a code-layout mechanism.
The source-4 cyclic-recovery and source-5 static-context helper attributes remain unchanged.

Equal-byte copies at the original short and long path lengths do not reproduce the RSS increase. Debugger and
external vmmap attempts yield no usable snapshot and their failure logs remain. A diagnostic injected
self-inspection library instead localizes a repeatable startup difference to system `MALLOC_SMALL` residency.
The equal-length-path controls have equal live allocation counts and bytes; the original short/long paths
differ by 48 live bytes. The residency difference also occurs with `--version`, before checker work. Injection changes the environment and exit residency need not equal peak residency, so these probes are
not acceptance measurements.

Strict reciprocal metadata controls then change only the 16-byte Mach-O UUID and its 32-byte signature hash
covering the header. All executable/data sections, remaining bytes, signature identifier/flags/hash-page format
and 113 runtime assets stay unchanged; all copies pass strict signature verification and use equal-length paths.
Normal non-injected measurements reproduce a metadata-only +16-KiB startup increase (ten positive pairs) and
+24-KiB Compiler-Unions median increase (eight positive, two tied pairs) for baseline code. Initial-code copies
also differ by +16 KiB at startup (ten positive pairs), though their Compiler-Unions medians tie (six positive,
four tied pairs). Every output and exit matches. `diagnosis/strict-control-summary.json` retains exact byte
locations, complete section hashes, signatures and all pairs. These controls demonstrate a benchmark nuisance;
they do not independently exclude a code memory effect.

The measurement problem is addressed with root-reviewed, preregistered **build-identity sampling**, described in
[the reusable evidence note](perf-build-identity.md). Apple's revision-pinned primary allocator documentation
supports entropy stable across executions of a binary within one boot, so repeated launches of one build do not
independently sample that nuisance. The complete installed allocator mechanism remains unproven. Before any new
measurement, `diagnosis/build-identity-protocol.json` freezes ten SHA-256-derived UUID identities, each applied
symmetrically to both original code variants, plus ten disjoint reserved confirmation identities. Every identity
counts; none is selected for favorable results. Each of the four previously confirmed single-thread cells gets
one warmup and one timed run per binary/identity in alternating order, with no injection or allocator override.
All three ordinary metrics are retained and screened using the existing exact sign-test thresholds. Only a
screened cell receives the single reserved confirmation, reversing pair order. No forced UUID or production
workaround ships, and the original frozen release remains the candidate. Controlled initial medians:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.487772 → 79.484529 | 7.269334 → 7.280619 | 1607.594 → 1607.594 |
| webpack / single | 10.562651 → 10.562622 | 0.887391 → 0.860729 | 311.305 → 311.297 |
| Compiler-Unions / single | 3.211773 → 3.211704 | 0.264461 → 0.269008 | 59.609 → 59.609 |
| t3code-server / single | 38.134651 → 38.134738 | 3.129247 → 3.106347 | 728.672 → 728.672 |

No controlled RSS or instruction metric screens. Compiler-Unions wall time screens at +1.719593%
(0.264461 → 0.269008 seconds, eight positive pairs, p = 0.0546875). Its sole confirmation decreases by
0.213206% (0.269771 → 0.269196 seconds), with five positive and five negative pairs; pooled p = 0.1315879822.
The controlled verdict is **no detected code-associated regression across the preregistered identities**,
not proof of equality. The original shipping binaries still measure +16–48 KiB RSS in the fixed-build initial
batch. `diagnosis/build-identity-final-verdict.json` links the frozen protocol and both complete datasets.
Its evaluator verifies exact coverage, hashes, UUIDs, run order, unchanged binaries/assets and output parity.
This source-6 adjudication does not automatically change later sources' policy.

After removing the experiments, all 1,608 original correctness inputs and all 1,640 broader build inputs match
the completed gate snapshots byte-for-byte, so those gates are reused with recorded proof. The dev CLI is rebuilt
to remove the temporary probe and again matches the exact-source Go diagnostic oracle. Final source lint,
formatting and diff checks pass. The frozen original release is `tsrs-release`, SHA-256
`9b0a663ce1fe83506e50952d0ec4827d33c6fd658a9a5f7d22985dc983864d7d`; its 113 runtime assets match the
preceding accepted snapshot. `target/port-623/0006-792ffccb90/` contains the upstream patch, reproduction,
full correctness and pass-set evidence, unchanged-input proofs, original eight-cell performance rejection,
all failed candidates, diagnostic controls, preregistration/review and final controlled verdict.

## 7. `c4d731aae17e` — constrain recursive return inference

Port the exact upstream condition in `get_inferred_type`: pure return-type inference is filtered by its
constraint even during recursive call resolution. The exact priority equality, left-to-right evaluation and
existing filtering/fallback stay intact. The new `recursiveTypeInferenceReturnTypeConstraint` fixture agrees
byte-for-byte with a Go compiler built at this source: two TS2345 diagnostics. The preceding compiler instead
produces four diagnostics, including spurious constraint errors, and misses the recursive-call error.
The ignored reference checkout advances; the published TypeScript pin remains unchanged.

All twelve complete passing sets retain every preceding pass. The new fixture is the sole addition in ten
sets; maps are unchanged. Go checker passes 13,463 diagnostics and 12,784 types/symbols; canonical checker
passes 13,463 diagnostics and 12,783 types/symbols. Full emit passes 13,467 diagnostics, 12,784 types/symbols,
13,397 JS/declaration outputs, 149 JS maps and 156 source maps. All 66 nonpassing lists/artifacts are
byte-identical, including the four existing checker diagnostic differences and canonical
`objectLiteralNormalization` type/symbol difference. There are no crashes or timeouts.

The final formatted candidate passes the isolated workspace (1,066 passed, four ignored), all 36 regressions,
native/WASM checks, Clippy, source lint and formatting. All seven generators plus formatting preserve all
1,168 crate files. Full Go fourslash retains all 4,074 exercised passing groups and 4,130 passing events,
with 472 groups skipped; its gate and socket-bridge checks pass. Its isolated metadata wrapper selects the
exact source without modifying the published pin. All 1,608 correctness and 1,640 broader build inputs remain
byte-identical through the final gates. Root independently verified the complete passing sets, artifacts,
input hashes and performance verdict.

The first workspace run overlapped emit and Go compilation; the existing FSEvents burst-under-load fixture
reported one dropped batch. Its failed log is retained. One isolated full-workspace run passed, as did the
final candidate's isolated run. The workflow repair is to run the entire workspace without unrelated heavy
work; no filesystem-watcher code changes. Initial evidence setup failures are also retained: the Go build
command was corrected from nonexistent `./cmd/tsgo` to `./cmd/tsc`, and snapshot handling now hashes symlink
target text instead of reading a directory symlink as a file.

The initial mechanical port **confirms a t3code-server single-thread wall regression** under
`paired-sign-test-v1`: 3.107099 → 3.159965 seconds (+1.701464%, ten positive pairs) initially, and
3.132455 → 3.146151 seconds (+0.437227%, six positive/four negative pairs) in its sole reversed-order
confirmation; pooled p = 0.0059089661. Instructions decrease slightly and median RSS ties in both batches.
Other screened metrics do not confirm. The original binary, complete eight-cell run, confirmation and
`noise-policy-verdict.json` retain their failed status.

After reading prior rejected experiments, diagnostic probes find only two `NoConstraintChecks` candidates
in t3code-server, neither pure `ReturnType`: the new condition enables zero extra constraint checks there.
Existing raw counters show cycles increasing in all 20 pairs, with median increases of 1.42967% and
0.59744%; CPU time increases while page-reclaim medians tie and involuntary switches decrease. Static codegen
shows the ordinary constraint branch changing from fallthrough to taken, jumping over the new priority test.
The helper grows by 12 bytes, with its 368-byte stack frame unchanged. This supports a branch/layout diagnosis;
it does not establish every microarchitectural cause. All probes are removed.

The targeted repair places `std::hint::cold_path()` on the rare right-hand side, before the exact priority
comparison. This preserves upstream control flow and restores ordinary-path fallthrough at the preceding
binary's addresses, with no stack-frame growth or new spills. The helper gains four rare-path instructions;
no new outlining attribute is added. One focused ten-pair diagnostic on a fresh candidate yields wall
3.115891 → 3.107622 seconds (-0.265367%), instructions 38.135097 → 38.130792 billion, RSS
728.672 → 728.656 MiB and cycles 11.598189 → 11.570484 billion (-0.238872%). It is diagnostic evidence,
not the final acceptance run.

That candidate's prebuild formatting check failed but an incorrectly sequenced shell command continued the
build; generation subsequently exposed the formatting change. Both failures and the superseded validation
attempts remain. The orchestration repair applies formatting and all generators before snapshots/builds,
checks every prerequisite exit, and runs final gates serially without source writes during Cargo reads.
The formatted final release is frozen at a new immutable path of the same length as the baseline path.
Its executable sections and all 17,781 functions match the preceding repair candidate exactly; only UUID,
eight source-location constants and signature hashes differ (183 bytes total). All correctness and final
performance evidence below belongs to these formatted inputs. No forced UUID or metadata override ships.

Final performance uses the accepted source-6 release and final source-7 release on all four installed public
projects, single-threaded and at the default checker count, with `noEmit`, incremental disabled and plain
output. No heavy workload overlaps timing. One warmup and ten alternating pairs per cell produce:

| Project / mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | ---: | ---: | ---: |
| vscode / single | 79.550229 → 79.532649 | 7.369314 → 7.328296 | 1607.594 → 1607.578 |
| vscode / default | 91.297143 → 91.326799 | 1.003273 → 0.997858 | 2116.938 → 2110.109 |
| webpack / single | 10.569473 → 10.567291 | 0.907911 → 0.902916 | 311.312 → 311.297 |
| webpack / default | 14.515828 → 14.511265 | 0.202901 → 0.202121 | 493.711 → 491.625 |
| Compiler-Unions / single | 3.211806 → 3.211823 | 0.262375 → 0.265229 | 59.609 → 59.602 |
| Compiler-Unions / default | 3.882497 → 3.884607 | 0.149139 → 0.150228 | 82.727 → 82.953 |
| t3code-server / single | 38.138322 → 38.134795 | 3.134343 → 3.126166 | 728.672 → 728.656 |
| t3code-server / default | 121.490636 → 122.249147 | 1.323314 → 1.310708 | 2087.469 → 2098.672 |

The final verdict is **no detected regression**. No metric screens, so no confirmation is required or run.
This is not proof of equality: some medians rise, but their paired signs do not satisfy the preregistered
screen. The previously regressing t3code-server single-thread wall median decreases by 0.260882%.
Every output and exit matches the baseline. All original failures and rejected/superseded candidates remain;
there is no unchanged-candidate retry or build-identity adjudication for this source.

Final correctness lives in `target/port-623/0007-c4d731aae1/candidate-3/`; raw performance is in
`perf-final/`, with final decision `noise-policy-final-verdict.json` under the source artifact root.
That root also holds the source patch, exact-source oracle, immutable snapshots, all original failures,
codegen/probe/cycle diagnosis and focused measurements. The accepted frozen binary is
`target/port-623/0007-cand000003/tsrs-release`, SHA-256
`7d5ca9c24e62ce861072fce926c6d93fb33b6ee5e51a8c177a035ac26e2b17f5`; all 113 adjacent runtime
assets match source 6. The original rejected binary remains `0007-c4d731aae1/tsrs-release`, SHA-256
`7c52a06fe8c3fec705a3a73e0fbf31c3fcc5078f84452a8a851a6254a8c5e97d`.

## 8. `79feed291a5c` — disable upstream VSIX template publishing

Upstream `79feed291a5c6c1d42ec040c4b42e7c2e9a1e2c4` adds `mb.publish.enabled: false` to the
Validate and Publish jobs in `tools/pipelines/vscode-typescript-publish.yml`. tsrs has no corresponding
Azure/VSIX pipeline, so this is an evidence-only port. No capability or runtime behavior changes.
The ignored reference checkout advances to this source and is clean; the published pin stays unchanged.

All 66,843 other upstream entries are identical, including the complete `tsc` and `packages` trees,
fixtures, baselines, generator schemas and bundled libraries. All 1,608 correctness inputs, 1,640 broader
build inputs and 1,168 generated crate files match source 7's final snapshots. A complete local tracked-file
comparison also covers npm/API tooling and configuration outside those snapshots; only these two evidence
documents change. This permits inheritance of source 7's final compiler, workspace, native/WASM,
regression, generator and fourslash gates. All twelve complete passing sets and 66 nonpassing artifacts
are preserved byte-for-byte: Go checker 13,463 diagnostics / 12,784 types and symbols; canonical
13,463 / 12,783; emit 13,467 diagnostics / 12,784 types and symbols / 13,397 outputs / 149 JS maps /
156 source maps; workspace 1,066 passed and four ignored; 36 regressions; fourslash 4,074 passing groups,
472 skipped and 4,130 passing events. Fresh Clippy, source lint, formatting and diff checks pass.

Performance is unchanged by compiler/runtime identity, without another timing run. Both sides use
`target/port-623/0007-cand000003/tsrs-release`, SHA-256
`7d5ca9c24e62ce861072fce926c6d93fb33b6ee5e51a8c177a035ac26e2b17f5`, with all 113 adjacent
runtime assets unchanged. Source 7's instruction, wall-time and RSS measurements remain the applicable
record. The patch, input/tree identities, inherited result hashes and fresh lint logs are retained under
`target/port-623/0008-79feed291a/`; inherited gate logs are in `0007-c4d731aae1/candidate-3/`.

## 9. `64c5fa0b92ad` — exercise both custom export conditions

Upstream `64c5fa0b92ade9b5f14db616d351ffe1990ca7fd` removes the space in the fixture directive
`@customConditions: webpack,browser`, updates its exports-enabled type baseline from `webpack` to `browser`,
and updates two Go resolution-trace baselines. The existing Rust compiler already handles both conditions.
Both variants pass diagnostics, types, symbols and emit before and after the fixture change. An exact-source
Go build agrees on diagnostics and exit status; supplementary literal-assignment probes also agree byte-for-byte:
exports enabled selects `webpack` before and `browser` after, while exports disabled selects `index` in both.
Go traces independently confirm those module paths. Rust trace-output parity is not asserted by these gates.
The ignored reference checkout advances and is clean; the published TypeScript pin remains unchanged.

Fresh full compiler runs preserve all twelve complete passing sets exactly: Go checker 13,463 diagnostics /
12,784 types and symbols; canonical checker 13,463 / 12,783; emit 13,467 diagnostics / 12,784 types and symbols /
13,397 outputs / 149 JS maps / 156 source maps. All 66 known nonpassing lists/artifacts remain byte-identical,
including the four checker diagnostic differences and canonical `objectLiteralNormalization` difference.
There are no new failures, crashes or timeouts. Fresh Clippy, source lint, formatting and diff checks pass.

Only the four upstream fixture/baseline files change; all 66,840 other entries, including Go code, fourslash
fixtures/baselines, generator schemas and bundled libraries, are identical. All 1,608 local correctness inputs,
1,640 broader build inputs and 1,168 generated crate files match source 7's final snapshots; a complete comparison
of all 2,352 tracked paths changes only the compatibility page and this ledger. The changed conformance fixture
is covered by the fresh compiler runs and is not an input to the inherited workspace, native/WASM, generator,
regression or fourslash gates. Those retain source 7's 1,066 workspace passes with four ignored, all 36 regressions,
and 4,074 passing fourslash groups / 472 skipped / 4,130 passing events. An initial evidence setup command named
a nonexistent fourslash tree; the recorded failure was repaired to use the actual Go fixture and baseline trees.
No compiler repair was needed.

Performance is unchanged by compiler/runtime identity, without another timing run. Both sides use the same
accepted `target/port-623/0007-cand000003/tsrs-release`, SHA-256
`7d5ca9c24e62ce861072fce926c6d93fb33b6ee5e51a8c177a035ac26e2b17f5`, and all 113 adjacent
runtime assets remain byte-identical. Source 7's instruction, wall-time and RSS measurements remain the applicable
record. The source patch, before/after fixtures, exact-source oracle, fresh compiler results, all passing sets,
input/tree identities and inherited-gate hashes are under `target/port-623/0009-64c5fa0b92/`.

## 10. `5f5dee5b6105` — switch upstream signing to MSBuild

Upstream `5f5dee5b610526cf3c597457cca336fde65e5f11` replaces DDSignFiles with MSBuild/MicroBuild
in `Herebyfile.mjs`, updates two Azure signing pipelines and adds a signing project and NuGet configuration.
tsrs's Cargo/Node/pnpm release tooling has no corresponding Microsoft signing integration, so this is an
evidence-only port. No capability or runtime behavior changes and no compiler repair is needed. The ignored
reference checkout advances and is clean; the published TypeScript pin remains unchanged.

All 66,841 other upstream entries are identical, including complete compiler, fixture, baseline, package,
generator and library trees. All 1,608 correctness inputs, 1,640 broader build inputs and 1,168 generated
crate files match the accepted snapshots. Comparing all 2,352 local tracked paths changes only this ledger
and the compatibility page. Source 9's full compiler results and source 7's other gates are therefore inherited:
all twelve complete passing sets and 66 known nonpassing artifacts remain byte-identical. Go checker retains
13,463 diagnostics / 12,784 types and symbols; canonical 13,463 / 12,783; emit 13,467 diagnostics / 12,784
types and symbols / 13,397 outputs / 149 JS maps / 156 source maps. Workspace retains 1,066 passes and four
ignored; all 36 regressions and 4,074 fourslash groups / 472 skipped / 4,130 passing events remain applicable.
Fresh Clippy, source lint, formatting and diff checks pass.

Performance is unchanged by compiler/runtime identity, without another timing run. Both sides use
`target/port-623/0007-cand000003/tsrs-release`, SHA-256
`7d5ca9c24e62ce861072fce926c6d93fb33b6ee5e51a8c177a035ac26e2b17f5`, with all 113 adjacent
runtime assets unchanged. Source 7's instruction, wall-time and RSS measurements remain the applicable
record. The patch, scope review, input/tree identities, inherited result hashes and fresh checks are under
`target/port-623/0010-5f5dee5b61/`.

## 11. `c8e9b7d25926` — cache inference from type arguments

Upstream `c8e9b7d259268e30605ed14910cb8d118929ed63` puts same-alias and same-reference type-argument
inference under `invokeOnce` and extends the visited key with inference priority, contravariance and bivariance.
The Rust port mirrors those branches and their recursion state. Its owned, pooled visited map ends its borrow before
recursive inference and updates the original key afterward; the full key retains separate states that the old
two-TypeId key would conflate. Rust-specific repairs keep priority in a checked two-byte encoding, reuse borrowed
variance slices and candidate buffers, and avoid a nesting walk for stacks shorter than two. The added
`inference-type-argument-variance` regression checks the upstream correlated-union TS2345 diagnostic against Go,
the dev CLI and the frozen release CLI.

The exact candidate-20 compiler rerun retains all twelve complete passing sets and all 66 known nonpassing
artifacts from entry 10. Go-history checking passes 13,463 diagnostics and all 12,784 type/symbol baselines;
canonical history has the same diagnostic passes and 12,783 type/symbol passes, with its known differences
unchanged. Emit passes 13,467 diagnostics, 13,397 JavaScript outputs, 149 JS maps and 156 source maps. Workspace
tests pass 1,067 with four ignored, all 37 regressions pass, and fourslash has 4,074 passing groups, 472 skipped,
and 4,130 passing events. Twenty-seven memo/deep-shadow runs preserve output and exit status. Native and WASM
checks, warnings-denied Clippy, source lint, formatting, and generator verification pass; all 1,168 generated
crate files remain unchanged. The serial-workspace fixtures were repaired to parse libtest's same-line child PID
prefix and accept either root or child kqueue descriptor exhaustion, which depends on map order. The initial
sandboxed workspace and fourslash runs could not bind local Unix sockets; their preserved failures were followed
by passing runs with socket access. The full current-source evidence and input proofs are under
`target/port-623/0011-c8e9b7d259/candidate-20/`.

Performance is disclosed under the user's entry-11 nonblocking policy. Against entry 10's byte-identical
accepted compiler, the frozen candidate-20 release (`3edc24074ab2ce687196eb816f114115c6292f3d27bf06a5e918fdfaee07a1dd`)
has a confirmed `webpack` single-threaded peak-RSS increase of **90,112 bytes** in each ten-pair batch
(20/20 paired increases; `candidate-20/formal-webpack-rss/run/results.json`). `t3code-server` single-threaded
instructions retired increase **0.182604%** in the initial ten pairs and **0.178883%** in the reversed
confirmation (19/20 paired increases; `diagnosis/c20-formal-t3/run/results.json`). The full fourteen-cell
performance cohort was not an entry-11 acceptance requirement after the user's policy change. These measured
regressions are retained in the record; they do not change the correctness verdict or hold the corresponding port.

## 12. `43521c80ec5e` — completion symbols remain in their snapshot

Upstream `43521c80ec5ebe4bc2948f1df3747773e48d8312` adds `userPreferences` and `prepareAutoImports` to
`createSnapshot` and `updateSnapshot`. The Rust API now passes both into the same clone, verifies the requested
file's default project and prepared auto-import registry before registering the result, and dereferences an
invalid clone. The requested filename is also passed to the registry's ancestor-directory walk, so a fresh
snapshot can index dependencies below a mixed-case project directory even when the file is not open. Completion
uses the active source filename. If auto-imports are needed, `includeSymbol: true` now returns the upstream
client error for an unprepared snapshot; without symbols, the existing transient-clone retry still works.
This prevents returning symbol handles owned by another snapshot. Focused Rust tests cover those paths,
per-snapshot preferences, creation/update with synthetic programs and a dependency auto-import, and an
unprepared dependency auto-import retry without symbols.

The public sync/async/protocol types export the new preference interfaces and snapshot fields. The new
`userPreferences.generated.ts` is byte-identical to upstream (SHA-256
`07d16cbb3a300152a1533628529baa0df9861815f8d0b82943acfde087fd4e3c`). Rust's existing tag-table
decoder now accepts all 83 raw names in source 12's generated Go decoder and matches its tested enum/default
edge cases. The Go schema/generator and reflection-to-generated parser refactor are implementation tooling,
not an additional runtime component to port; tsrs retains its equivalent Rust table and the exact generated
public TypeScript artifact. Go Fourslash fixture changes only rename preference fields to the generated shape;
the full exact-source Fourslash gate covers their behavior. LSP-derived API snapshots remain outside tsrs's
standalone Node API scope, as documented in `docs/LSP.md`. Upstream Go linter scoping and preference-generator
task registration do not apply to Cargo/pnpm builds; their generated public artifact is carried above.

The full dev compiler suite preserves all twelve complete source-11 passing sets and all known nonpassing
artifacts byte-for-byte, excluding timing summaries: Go-history checker 13,463 diagnostics and 12,784
types/symbols; canonical checker 13,463 and 12,783; emit 13,467 diagnostics, 12,784 types/symbols, 13,397
JavaScript/declaration outputs, 149 JS maps and 156 source maps. There are no new failures, crashes or timeouts.
The exact-source Go API, project, auto-import registry and preference tests pass. Workspace tests pass 1,072
with four existing ignored;
the public sync/async API suites pass 691 tests, including the new snapshot tests; all 37 regressions pass.
The full source-12 Go Fourslash gate passes 4,074 groups with 472 skipped, zero failed, and an identical
4,602-event outcome set to source 11. Native and WASM checks, warnings-denied Clippy, source lint, formatting,
and generated-code verification pass; regenerating leaves every tracked crate file byte-identical to its
pre-generation snapshot. The initial sandboxed workspace run failed only because a local Unix socket could not
be created; the complete rerun with socket access passed. An initial Clippy run found an assigning-clones lint
in the new snapshot preference copy; `clone_from` fixed it before the passing run.

No source-12 release performance measurements were collected. Under the user's nonblocking regression policy,
the source-11 measured regressions above remain disclosed, and no claim of source-12 performance equality or
absence of further regressions is made. Evidence is under `target/port-623/0012-43521c80ec/`, including the
exact-source Go diff, full compiler results and pass-set comparison, Go/API/fourslash/workspace and lint logs,
and generated-source hashes. The ignored reference checkout advances to this revision; the published
`Cargo.toml` TypeScript pin remains unchanged until the full source upgrade.

## 13. `edf7da4e93ab6` — preserve reverse-mapped declaration types

Upstream `edf7da4e93ab65e1f7766deae156c35132c0cde3` keeps reverse-mapped properties and index signatures
in declaration emit instead of replacing them with display placeholders. The Rust node builder follows all three
Go changes: placeholders remain for display only; reverse-mapped types without symbols enter the visited-type
guard during declaration serialization; and a separate mapped-declaration node identity limits expansion to 100
levels, restoring its depth on exit. The upstream `reverseMappedDeclarationSerialization` fixture covers indexed,
nested, named, and recursive/growing types. It passes its diagnostic, type, symbol, and JavaScript/declaration
baselines against this exact Go revision. The new `TestReverseMappedDeclarationQuickInfo` also passes through the
Rust LSP subprocess, preserving display behavior.

All twelve full compiler passing sets retain every source-12 pass and known nonpassing outcome; only the new
compiler case is added. Go-history checker: 13,464 diagnostics and 12,785 types/symbols; canonical checker:
13,464 diagnostics and 12,784 types/symbols; emit: 13,468 diagnostics, 12,785 types/symbols,
13,398 JavaScript/declaration outputs, 149 JS maps, and 156 source maps. The full source-13 Go Fourslash gate
passes 4,075 groups with 472 skipped and zero failures: the complete source-12 outcome set is unchanged, with
the new quick-info group added as a pass.

The upstream `reverse mapped declaration consumption` incremental scenario is dumped from the exact Go source
and passes the Rust `tsctests` harness. An isolated direct Go/Rust comparison exercises initial build, no-change
rebuild, comment-only producer edit, and another no-change rebuild. Both compilers report the same three TS2322
consumer errors and emit identical producer `.d.ts` and JavaScript in each phase. The consumer build info is
byte-identical. Producer build info differs only in Rust's existing optional `tsrsCanonicalCheckSignature` field.
The exact-source Go compiler, Fourslash, and incremental tests pass independently. All 37 existing regression
projects pass. The full workspace has 1,072 passing tests and four existing ignored tests; the public Node API
suite passes 910 tests. Native and WASM checks, warnings-denied Clippy, source lint, formatting, and
generated-source verification pass. Regeneration leaves every tracked crate file byte-identical to its
pre-generation snapshot. No performance measurement was collected for source 13 under the user's nonblocking
policy; the known source-11 regressions remain disclosed above.

Evidence is under `target/port-623/0013-edf7da4e93/`, including all compiler pass sets and their complete
comparison, exact-source Go oracle and four-phase build comparison, focused scenario dump and Rust result,
Fourslash raw events/outcome comparison, and workspace, API, lint and build logs. The ignored reference checkout
advances to this revision; the published TypeScript pin remains unchanged until the full source upgrade.

## 14. `02c1c9a6ab49` — preserve a removed CommonJS `with` body

Upstream `02c1c9a6ab49334a5ca9c1d7852647b7a3fad18a` fixes a CommonJS transformer crash when a top-level
`with` statement's nested visitor removes its body. Rust had the same unchecked unwrap. The port visits that
body first, substitutes an empty statement only when the visitor returns none, then visits the expression and
updates the `with` statement, matching the Go operation order. The new upstream
`withStatementCommonJsEmit` compiler case has four forms: the removed `export let a` emits `;`, while an
initialized export, a local variable, and an export in a block keep their expected output. All eight expected
strict-mode and unsupported-`with` diagnostics match. The exact source-14 Go compiler passes all nine fixture
subtests, and Rust passes the diagnostic, type, symbol, and JavaScript baselines.

Full Go-history and canonical checker runs and the full emit run preserve every source-13 passing case and all
known nonpassing outcomes. The new fixture adds one pass to every applicable checker and emit list: Go-history
checker has 13,465 diagnostic and 12,786 type/symbol passes; canonical checker has 13,465 diagnostic and
12,785 type/symbol passes; emit has 13,469 diagnostic, 12,786 type/symbol, and 13,399 JavaScript/declaration
output passes, with the existing 149 JavaScript map and 156 source-map passes unchanged. All 37 existing
regression projects pass. The full workspace has 1,072 passing tests and four existing ignored tests. Native
and WASM checks, warnings-denied Clippy, source lint, and formatting pass. Regenerating with all seven generators
leaves every tracked crate file byte-identical. A direct `tools/gen-check.sh` on the uncommitted transformer edit
reported that intended edit as a diff; the generator write and full before/after crate hash comparison verify
that no generated file changes.

The full exact-source Go Fourslash gate passes 4,075 groups with 472 skipped and zero failures. All 4,603 named
outcomes match source 13 exactly, including its 4,131 passing events. The first sandboxed launch could not bind
the bridge's local Unix socket; the complete rerun with socket access passed.

No source-14 release performance measurements were collected under the user's nonblocking regression policy.
The measured source-11 regressions remain disclosed above; no source-14 performance-equivalence claim is made.
Evidence is under `target/port-623/0014-02c1c9a6ab/`. The ignored reference checkout advances to this revision;
the published TypeScript pin remains unchanged until the full source upgrade.

## 15. `237b14a5d98f` — stabilize merged-declaration diagnostic ownership

Upstream `237b14a5d98f8e8f748ff36002fdd99786230d90` anchors merged-interface TS2320 and TS2430
diagnostics to the symbol's first interface declaration even when a checker first visits another declaration.
The Rust checker now uses that anchor in both Go-history and canonical modes. Canonical mode retains its per-file
execution for index-constraint checks; the earlier virtual four-checker rule no longer chooses the interface
diagnostic site. The enum const-consistency check compares against the first enum declaration. The merged-export
check now runs once per symbol per checker when there are at least two declarations, regardless of which
declaration kind a checker visits first. Existing merged-interface regression expectations are updated from the
exact source-15 Go compiler. The older diagnostic-history investigation has a supersession note.

The exact-source Go `TestLocal` oracle passes all six newly added interface fixture variants and four existing
merged/enum fixtures. Rust passes the new `mergedInterfaceAugmentationDiagnostics`,
`mergedInterfaceInheritanceDiagnostics`, and four `mergedInterfaceDefaultLibraryDiagnostics` configurations.
Full Go-history and canonical checker runs each retain all source-14 passes and add these six diagnostic cases;
Go history has 13,471 diagnostic and 12,786 type/symbol passes, canonical history 13,471 diagnostic and 12,785
type/symbol passes. The full emit run retains all previous passes and adds the same six diagnostic cases plus
two JavaScript cases: 13,475 diagnostics, 12,786 type/symbols, 13,401 JavaScript/declaration outputs, 149 JS maps,
and 156 source maps. No previously nonpassing set changes and there are no new crashes or timeouts.

The exact-source Go CLI produces identical ordered diagnostics for both updated public regression fixtures at
one, four, and eight checkers; reversing the first two declaring roots moves the sole TS2320 to the new first
declaration. The Rust CLI, API, and incremental tests now assert that ownership while retaining the per-file
index-constraint and cache-reuse checks. All 37 regression projects pass. The full workspace passes 1,072 tests
with four existing ignored. The full source-15 Go Fourslash gate passes 4,075 groups with 472 skipped and zero
failures, retaining all 4,603 named source-14 outcomes (4,131 passes and 472 skips). The native dev build, WASM
check, warnings-denied Clippy, source lint, formatting, and diff checks pass. Regeneration with all seven
generators leaves every one of the 1,168 tracked crate files byte-identical to its pre-generation snapshot.
No source-15 release performance measurement was collected under the user's nonblocking regression policy. The
confirmed source-11 regressions remain disclosed above, and no performance-equivalence claim is made for this port.
Evidence is under `target/port-623/0015-237b14a5d9/`. The ignored reference checkout advances to this revision;
the published TypeScript pin remains unchanged until the full source upgrade.

## 16. `688d86d7f647` — resolve binder symbols from remote declarations

Upstream `688d86d7f647781565dc1f6438a4fb09b5179160` adds `getSymbol(decl)` to the sync and async Node APIs.
The Rust core server now implements `getSymbolOfDeclaration`: it acquires the exact cached ordinary source file,
resolves the declaration's encoder index, rejects index zero, out-of-range and non-declaration nodes, rejects
declarations without binder symbols, and returns a file-owned symbol response. The source-file lease is released
on success and error. The protocol inventory and uint32 request decoder include the new method. A Rust
integration test checks file-owned identity against the checker response and validates zero, out-of-range, and
malformed indices. The exact-source Go `TestCreateSourceFile` suite passes.

The five changed public client source files (`async/api.ts`, `sync/api.ts`, `node/node.ts`, `sourceFileCache.ts`,
and `proto.generated.ts`) are byte-identical to this upstream revision. The changed upstream sync, async, and
generator tests are carried with the existing tsrs package-name and earlier snapshot-test edits preserved.
Remote source files retain their owning API and symbol-cache record. Successful declaration lookups are interned
by both binder ID and declaration index; concurrent async lookups share one request, while failures are not
cached. The full public npm API suite passes 925 tests, including the upstream checks for checker symbol identity,
synthetic and configuration nodes, cache clearing, lease release, stale/recreated files, and generator behavior.

All twelve full compiler passing sets and their known nonpassing outcomes match source 15 exactly. Go-history
checker has 13,471 diagnostic and 12,786 type/symbol passes; canonical checker has 13,471 diagnostic and
12,785 type/symbol passes. Emit has 13,475 diagnostic, 12,786 type/symbol, 13,401 JavaScript/declaration,
149 JavaScript-map, and 156 source-map passes. The full exact-source Go Fourslash gate passes 4,075 groups with
472 skipped and zero failures; its 4,603 named outcomes are identical to source 15. All 37 regression projects
pass. The workspace has 1,073 passing tests and four existing ignored. Native and WASM checks, warnings-denied
Clippy, source lint, formatting, and the npm SDK build pass. All 1,168 tracked crate files remain byte-identical
after running the seven code generators. Initial sandboxed workspace and Fourslash runs could not bind their
local Unix sockets; complete reruns with socket access passed.

No source-16 release performance measurements were collected under the user's nonblocking regression policy.
The measured source-11 regressions remain disclosed above, and no performance-equivalence claim is made for
this port. Evidence is under `target/port-623/0016-688d86d7f6/`, including the compiler pass-set and
Fourslash outcome comparisons, exact-source Go API test, full npm API and workspace suites, regression fixtures,
lint and build logs, and generator hashes. The ignored reference checkout advances to this revision; the
published `Cargo.toml` TypeScript pin and `npm/tsrs/UPSTREAM.json` base manifest remain unchanged until the full
source upgrade.

## 17. `e8fd69fc1e19` — generate compiler options and publish config schemas

Upstream `e8fd69fc1e1974f7f7410f79a04adad692dff0dc` moves option definitions to a shared generator and
publishes `tsconfig` and `jsconfig` JSON schemas. No compiler, build or type-acquisition option field is added or
removed. The observable parser change is deletion of seven watch-tuning flags: `watchInterval`, `watchFile`,
`watchDirectory`, `fallbackPolling`, `synchronousWatchDirectory`, `excludeDirectories`, and `excludeFiles`.
The Rust CLI and build parser now reject each one while preserving `--watch`; the exact Go baseline pair for
each flag is TS5023 in compiler mode and TS5072 in build mode. The former watch-option types and parsed fields
are removed. Rust's watcher uses the new parsed-option equality semantics, including type-acquisition's
nil/empty-list equivalence and ordered compiler `paths` comparison. The Rust compiler fixture runner now uses
the exact 91-name generated Go variation inventory, rather than deriving 72 names from declaration flags.

The public npm package contains upstream's byte-identical generated `CompilerOptions` and `proto` files, the
changed enum source files, and both byte-identical JSON schemas. It exports both schemas; its SDK sync inventory
will hash them when the final TypeScript pin advances. The local `gen_decls.mjs` now accepts the combined
`declarations_generated.go` source and preserves Rust metadata fields that upstream moved out of declarations.
Dry-run regeneration keeps the same order for all 126 compiler and seven build declarations, with compiler
field values unchanged. Go's generated option parsing, build-info, transpile, and declaration metadata keep
their existing Rust behavior; the exact Go `tsoptions`, `transpile`, and `tsctests` packages pass.

All twelve full compiler passing sets and every known nonpassing outcome match source 16 exactly. Go-history
checker retains 13,471 diagnostic and 12,786 type/symbol passes; canonical checker retains 13,471 diagnostic
and 12,785 type/symbol passes. Emit retains 13,475 diagnostic, 12,786 type/symbol, 13,401 JavaScript/declaration,
149 JavaScript-map, and 156 source-map passes. The full exact-source Go Fourslash gate passes 4,075 groups with
472 skipped and zero failures; all 4,603 named outcomes match source 16. The Rust workspace passes 1,076 tests
with four existing ignored, including the new watch-option and equality cases. The public npm API suite passes
925 tests, the SDK typechecks, both schemas appear in the npm package dry run, and all 37 regression projects
pass. Native and WASM checks, warnings-denied Clippy, source lint, formatting, and upstream artifact hashes
pass. Regeneration leaves all 1,166 retained tracked crate files byte-identical.

`--showConfig` and full `--help --all` output were already outside tsrs's supported CLI scope, so source 17's
new show-config scenarios and removed help section do not change those capabilities. Rust's internal parsed
configuration representation also does not distinguish nil from empty file-name/content-mapper collections
or nil entries inside them, unlike Go's pointer/slice equality; ordinary parsed configurations and the
observable conformance sets above retain parity. No source-17 release performance measurement was collected
under the user's nonblocking regression policy; the measured source-11 regressions remain disclosed above,
and no performance-equivalence claim is made. Evidence is under `target/port-623/0017-e8fd69fc1e/`. The ignored
reference checkout advances to this revision; the published TypeScript pin and base SDK manifest remain
unchanged until the full upgrade.

## 18. `15dee00a1c65` — detect array and tuple serialization cycles

Upstream `15dee00a1c6549a4953ab17ccac4c56d252ba4cc` routes array and tuple references through
`visitAndTransformType` so recursive containers use the same visited-type cycle check as other serialized
types. The Rust node builder now separates array/tuple serialization from ordinary type-reference
serialization and canonicalizes deferred and regular container references to one type identity before
checking for a cycle. It does not apply the shared `Array` symbol or tuple-origin depth limit to finite
container nesting. The visited-type check runs before cache lookup, and the serialized-type cache key
includes the current `infer` type parameters, matching the Go control flow.

The new `recursiveContainerDeclarationSerialization` compiler fixture passes diagnostics, types, symbols,
JavaScript and declaration output. Four unnameable recursive results report TS5088; named recursive array
and tuple aliases serialize without errors, and finite 13-level tuple and array containers remain expanded.
The changed `conditionalTypes1` and `recursiveTypeInference2` baselines also pass. The exact source-18 Go
compiler passes those fixtures, the nine-edit incremental recursive tagged-tuple scenario, and the changed
quick-info truncation test. The incremental case keeps the consumer's recursive `Doc` type across comment,
union and no-change rebuilds, then reports TS2322 for an invalid number assignment even after deleting
build info. The corresponding Rust `tsctests` replay passes byte-for-byte across all nine edits. The full
exact-source Go Fourslash gate passes 4,075 groups, skips 472 and fails none; all
4,603 named outcomes match source 17.

All twelve full compiler passing sets retain every source-17 pass and every known nonpassing outcome.
Go-history checker passes 13,472 diagnostic and 12,787 type/symbol baselines; canonical checker passes
13,472 diagnostic and 12,786 type/symbol baselines. Emit passes 13,476 diagnostic, 12,787 type/symbol,
13,402 JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines. The 37 existing
regression projects pass. The full workspace passes 1,076 tests with four existing ignored. Native and
WASM builds, warnings-denied Clippy, source lint, formatting and generated-source verification pass;
regeneration leaves all 1,166 tracked crate files byte-identical. No source-18 release performance
measurement was collected under the user's nonblocking migration policy; the source-11 regressions remain
disclosed above, and no performance-equivalence claim is made for this port.

Evidence is under `target/port-623/0018-15dee00a1c/`, including complete compiler pass-set comparisons,
the exact-source Go oracle and incremental logs, Fourslash outcome comparison, workspace and lint logs,
and generator hashes. The ignored reference checkout advances to this revision; the published TypeScript
pin and base SDK manifest remain unchanged until the full source upgrade.

## 19. `af36d532b012` — follow merged export-alias chains for declaration visibility

Upstream `af36d532b0120413a8b6d2157a7f5be7f8d61f03` extends `isAccessible` when a direct
symbol match fails: it follows a still-aliased `resolvedAliasSymbol` through `resolveAlias` and
`getMergedSymbol`, comparing each target with the requested merged symbol. The Rust checker mirrors
that loop and uses a set of seen symbol identities to stop circular alias chains. Its ordinary
qualification and external-module checks still run after a match. This preserves accessible class
types through `export =` with separate type exports and through merged re-export aliases, including
classes with private members.

The exact source-19 Go testrunner passes all five new compiler cases across 13 module configurations
and all seven existing cases whose type or symbol baselines changed. Rust matches diagnostics,
`.types`, `.symbols`, and JavaScript/declaration output on the same 13 new configurations. The
circular merged-alias case intentionally reports two TS2484 conflicts; the two-merged-alias case
reports TS2484 for `Factory` in each module mode. The changed existing baselines also pass in the
full suite.

All twelve full compiler passing sets retain every source-18 pass and every known nonpassing
outcome. Go-history checker passes 13,485 diagnostic and 12,800 type/symbol baselines; canonical
checker passes 13,485 diagnostic and 12,799 type/symbol baselines. Emit passes 13,489 diagnostic,
12,800 type/symbol, 13,415 JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
The full exact-source Go Fourslash gate passes 4,075 groups, skips 472, and fails none; all 4,603
named outcomes match source 18. All 37 regression projects pass. The workspace has 1,076 passing
tests and four existing ignored tests. Native and WASM builds, warnings-denied Clippy, source lint,
formatting, and generated-source verification pass; regeneration leaves all 1,166 tracked crate
files byte-identical. Initial sandboxed Fourslash and workspace runs could not bind local Unix
sockets; complete reruns with socket access passed.

No source-19 release performance measurement was collected under the user's nonblocking migration
policy; the source-11 measured regressions remain disclosed above, and no performance-equivalence
claim is made for this port. Evidence is under `target/port-623/0019-af36d532b0/`, including the
compiler pass-set and Fourslash outcome comparisons, exact-source Go oracle, workspace and regression
logs, build and lint logs, and generator hashes. The ignored reference checkout advances to this
revision; the published TypeScript pin and base SDK manifest remain unchanged until the full source
upgrade.

## 20. `bf00f213ffd6` — add keyed Promise methods to the ESNext library

Upstream `bf00f213ffd67ce4cc765c0db341e86c7057be44` gives `esnext.promise` its own
library file, referenced by the `esnext` aggregate, rather than aliasing
`lib.es2025.promise.d.ts`. The new library still references ES2020 and ES2025 promise
declarations, then adds `Promise.allKeyed` and `Promise.allSettledKeyed`. Both methods
preserve the input object's string and symbol keys, remove property `readonly`, and map
each value through `Awaited`; the latter wraps each result in `PromiseSettledResult`.
The Rust bundled-library generator includes the file in its name and embedded-content
inventories, and the ordered `--lib` option map now resolves `esnext.promise` to it after
the ES2026 feature aliases. The new library, changed `lib.esnext.d.ts`, and both npm
configuration schemas are byte-identical to upstream. The npm package dry run contains
and exports both parsed JSON schemas.

The exact source-20 Go `TestLocal/promiseAllKeyed.ts` case and its bundled/compiler/options
package tests pass. The new Rust compiler case matches diagnostics, `.types`, `.symbols`,
and JavaScript output. Its four TS2345 errors intentionally reject `number`, `string`,
`null`, and `undefined` inputs under the `object` constraint. `Promise.try` remains
available through the referenced ES2025 promise library.

All twelve full compiler passing sets retain every source-19 pass and known nonpassing
outcome. Go-history checker passes 13,486 diagnostic and 12,801 type/symbol baselines;
canonical checker passes 13,486 diagnostic and 12,800 type/symbol baselines. Emit passes
13,490 diagnostic, 12,801 type/symbol, 13,416 JavaScript/declaration, 149 JavaScript-map,
and 156 source-map baselines. Each gained only the new `promiseAllKeyed` case where that
baseline exists. The full exact-source Go Fourslash gate passes 4,075 groups, skips 472,
and fails none; all 4,603 named outcomes match source 19. All 37 regression projects
pass. The workspace passes 1,076 tests with four existing ignored. Native and WASM
builds, warnings-denied Clippy, source lint, formatting, and npm schema/package checks
pass. Regeneration leaves all 1,167 tracked and new crate files byte-identical.

No source-20 release performance measurement was collected under the user's nonblocking
migration policy. The measured source-11 regressions remain disclosed above, and no
performance-equivalence claim is made for this port. Evidence is under
`target/port-623/0020-bf00f213ff/`. The ignored reference checkout advances to this
revision; the published TypeScript pin and base SDK manifest remain unchanged until the
full source upgrade.

## 21. `82f054616325` — clarify security information

Upstream `82f054616325c1550b184f26a64f26b44ce6b99d` adds Workspace Trust/content-mapper
and Go-compiler CVE guidance only to `.github/skills/security-report-check/SKILL.md`.
`tsrs` has a user-facing `SECURITY.md` rather than an equivalent security-report skill.
Its Language Service section now says that editor integrations decide workspace trust,
direct LSP clients must make their own trust decision, and a trusted workspace may run
project tools from `node_modules`. It also states the current tsrs boundary: the language
server does not start content mappers, while compiler/build mode can start configured
mappers only with `--runExternalCode`. The upstream Go-compiler CVE and `govulncheck`
advice does not apply to this Rust compiler, so no Go-specific claim or new release
policy was copied.

The upstream source-20-to-21 diff contains only that skill file; the `tsc` tree is
`639627690565280fef5ece5479386bf63db835f1` at both revisions. The source and fixture
directories used by the tsrs conformance runner are unchanged. The ignored reference
checkout advances to source 21, while the published TypeScript pin remains unchanged.
Only `SECURITY.md` and this port record change in tsrs; there is no compiler, library,
runtime-asset, test, or capability change.

Full checker conformance in Go and canonical histories and full emit conformance were
rerun against the final dev build. All twelve complete passing sets and every known
nonpassing set match source 20 exactly: Go-history checker has 13,486 diagnostic and
12,801 type/symbol passes; canonical checker has 13,486 diagnostic and 12,800
type/symbol passes. Emit has 13,490 diagnostic, 12,801 type/symbol, 13,416 JavaScript/
declaration, 149 JavaScript-map, and 156 source-map passes. No new crash or timeout
occurred. The source-20 Fourslash, workspace, regression, native/WASM, npm and generator
results are reused because their product inputs and upstream fixture trees are unchanged;
they were not independently rerun for this documentation-only entry. The dev build,
warnings-denied Clippy, source lint, formatting, and whitespace checks pass.

The release compiler binary is byte-identical before and after this port (SHA-256
`3edc24074ab2ce687196eb816f114115c6292f3d27bf06a5e918fdfaee07a1dd`), and no
runtime assets change. No new release performance measurements were collected; the
source-11 measured regressions remain disclosed above. Source identity, exact passing-set
comparison, full gate logs, and build/lint evidence are under
`target/port-623/0021-82f0546163/`.

## 22. `59f5b0233f9d` — typed path prep bugfixes

Upstream `59f5b0233f9d2411aacb77c7342ddb31002dd3dd` changes typed path preparation across
the API, compiler, resolver, source-map reader/writer, incremental build, project watcher, and
language server. Most of its 282 changed paths are regenerated source-map, build and Fourslash
baselines. The Rust port encodes non-file document URIs without collapsing exceptional path
segments, preserves their case-sensitive identity, and resolves imports, package exports and
root-directory candidates through encoded dynamic paths. Project-reference symlink and
auto-import paths retain their original spelling. The upstream npm path implementation and
its changed API tests are carried into `npm/tsrs`.

Source-map emit omits an empty `sourceRoot`; reverse mapping handles explicit empty/null
sources, URL roots, invalid indices and duplicate source paths. Incremental snapshots resolve
nested triple-slash references from the referring source file. Content-mapper manifest watches
compare canonical paths. Project watchers retain original path spelling, and closing a project
session cancels and drains background tasks, idle cache work and its current snapshot. The API
rejects empty `createSourceFile` names and resolves relative names from its session directory;
auto-import preparation uses the program's resolved source file. LSP completion resolution
rejects relative or malformed dynamic file names. The existing Rust project builder already
clones API state and rejects failed snapshots before adoption, matching the upstream rollback
change without new code.

The source commit also adds cancellation of npm installs for automatic type acquisition and
closes LSP-attached API sessions on server shutdown. Neither operation has a tsrs counterpart:
automatic type acquisition and attached API sessions remain unported, while the standalone
Node API server is supported. The Go test harness changes do not alter product behavior.

The exact-source Go path, module, symlink, source-map, LSP-conversion, project, API,
incremental and VFS package suites pass. The new Rust resolver and source-mapper scenarios,
dynamic URI/path/glob and symlink cases, API empty-name case, project watcher/shutdown cases,
and LSP completion validation pass. Full checker conformance in Go and canonical histories and
full emit conformance pass, with every passing and nonpassing set identical to source 21:
13,486 diagnostic, 12,801 Go-history `.types` and `.symbols`, 12,800 canonical `.types` and
`.symbols`, 13,416 JavaScript/declaration, 149 JavaScript-map, and 156 source-map passes. No
new crash or timeout occurred.

The exact-source-22 Fourslash run passes 4,075 groups, skips 472 and fails none; all 4,603
named outcomes match source 20 (4,131 passes and 472 skips). The first full run used a stale
`fourslash_server` binary and showed a quick-info truncation mismatch; rebuilding that binary
made the unchanged upstream case pass, and the full run above uses the fresh binary. The
`DocumentUri.FileName` differential fixture was regenerated from source-22 Go for its new
non-file URI encoding. The full workspace passes 1,103 tests with four existing ignored; the
Node transport round trip passes 15 client tests, the npm API suite passes 933, and all 37
regression projects pass. The native dev build, WASM target check, warnings-denied Clippy,
source lint and formatting pass. All seven generators plus formatting preserve all 1,169
crate files byte-for-byte. A malformed absolute-looking completion filename now returns an
error instead of panicking in the Rust LSP server.

No source-22 release performance measurements were collected under the user's nonblocking
migration policy. The measured source-11 regressions remain disclosed above; no performance
equivalence claim is made for this port. Full gate logs, passing-set comparisons, exact-source
Go oracle and Fourslash results, and generator integrity evidence are under
`target/port-623/0022-59f5b0233f/`.

## 23. `09b1db061731` — elide erased named imports beside a default import

Upstream `09b1db0617311f3b39f165bb3480c0ce2f718b27` changes the type eraser under
`verbatimModuleSyntax`: when every named import was type-only and was erased, a remaining
default import keeps the import declaration, so the generated empty `{}` is omitted. A
source-written `{}` stays in the output; a named list that retains a value import is also
unchanged. The Rust transformer mirrors the Go import-clause branch. The exact new upstream
`verbatimModuleSyntaxDefaultImportTypeOnlyNamed` fixture passes the Go reference's nine
baseline/structural checks and matches tsrs diagnostics, `.types`, `.symbols`, and emitted
JavaScript. In particular, its `A1`, `A5`, and `A6` cases become plain default imports,
while `import A4, {} from "./a"` retains its braces.

All twelve full compiler passing sets retain every source-22 pass and known nonpassing
outcome, adding only the new fixture where its baseline exists. Go-history checker passes
13,487 diagnostic and 12,802 type/symbol baselines; canonical checker passes 13,487
diagnostic and 12,801 type/symbol baselines. Emit passes 13,491 diagnostic, 12,802
type/symbol, 13,417 JavaScript/declaration, 149 JavaScript-map, and 156 source-map
baselines. No new crash or timeout occurred. The exact-source Fourslash gate passes 4,075
groups, skips 472, and fails none; all 4,603 named outcomes match source 22. The full
workspace passes 1,103 tests with four existing ignored tests, and all 37 regression
projects pass. The dev build, WASM target check, warnings-denied Clippy, source lint and
formatting pass. All seven generators plus formatting leave all 1,169 tracked crate
files byte-identical. The initial sandboxed workspace run could not create local Unix
sockets; the complete rerun with socket access passed.

No source-23 release performance measurement was collected under the user's nonblocking
migration policy. The measured source-11 regressions remain disclosed above; no
performance-equivalence claim is made for this port. Evidence is under
`target/port-623/0023-09b1db0617/`, including exact-source Go output, full compiler
pass-set and Fourslash outcome comparisons, workspace, lint, and generator logs. The ignored
reference checkout advances to source 23; the published TypeScript pin and base SDK
manifest remain unchanged until the full source upgrade.

## 24. `2f9fd09a104e` — source-phase imports

Upstream `2f9fd09a104ef14268944a0aeb9cdc4d80abb6a6` adds static
`import source binding from "module"` and dynamic `import.source("module")`.
The Rust port follows its `SourceKeyword` scanner/parser/AST additions, grammar
and TS18111–TS18115 diagnostics, `AbstractModuleSource` and
`Promise<AbstractModuleSource>` types, source-phase module-resolution elision,
program-reuse and package-name rules, printing and CommonJS transform behavior,
and language-service auto-import, organization and source-definition behavior.
The `esnext` library now includes `lib.esnext.modulesource.d.ts`, and the
`esnext.modulesource` option selects that file. The npm AST, syntax-kind,
encoder and schema changes were copied from the matching upstream revision.

All 31 new `importSource` conformance variants pass diagnostics, `.types`,
`.symbols` and JavaScript emit. The three full compiler gates retain every
source-23 pass and known nonpass, with no crashes or timeouts. Go-history and
canonical checker diagnostic passes rise to 13,518; Go-history type and symbol
passes rise to 12,833, canonical to 12,832. Emit has 13,522 diagnostic, 12,833
type and symbol, 13,448 JavaScript/declaration, 149 JavaScript-map and 156
source-map passes. The exact-source Go parser, scanner, compiler and API
encoder suites pass. A Rust program test verifies source-phase specifiers never
resolve or retain files, while changing their phase prevents stale program
reuse. Parser, scanner, printer and codec focused tests pass.

The exact-source Fourslash gate passes 4,083 groups, skips 472 and fails none;
all 4,615 named outcomes match or add to source 23 (12 new passes, no changed
or lost outcomes). The full Rust workspace passes 1,106 tests with four
existing ignored tests; all 37 regression projects pass. The npm API suite
passes 935 tests, including the new sync and async source-phase cases. The
native dev build, WASM target check, warnings-denied Clippy, source lint and
formatting pass. The seven generators are run and checked for idempotence.

The new `SourceKeyword` shifts numeric `SyntaxKind` values in the API wire
format. The source-23 binary codec goldens could no longer decode under the
source-24 enum, and the static Go API-shape fixture likewise contained old
kind numbers. Both sets of fixtures were regenerated from the exact source-24
Go server and updated local JavaScript client; codec byte parity, print
parity and all decoder tests now pass. The `AbstractModuleSource` conformance
errors in the first focused run exposed a missing `esnext.modulesource` option
mapping, which was added before the full gate. The published TypeScript pin
and base SDK manifest remain unchanged until the whole source upgrade.

No source-24 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above; no performance-equivalence claim is made for this port.
Evidence is under `target/port-623/0024-2f9fd09a10/`, including pass-set and
Fourslash outcome comparisons, exact-source Go and codec goldens, workspace,
regression, npm, lint and generator logs.

## 25. `a4b1410230c7` — correct `Array.at` parameter documentation

Upstream `a4b1410230c7299957ec3f8f8dee4cccc55a4798` replaces “code unit” with
“item” in the `index` parameter documentation for `Array.at`, `ReadonlyArray.at`,
and eleven typed-array `at` signatures. The Rust bundled
`lib.es2022.array.d.ts` is copied byte-for-byte from the exact source-25 Go
reference; declaration signatures and executable code do not change. The
generator's embedding list remains byte-identical after formatting. The
ignored reference checkout advances to source 25, while the published
TypeScript pin and base SDK manifest remain unchanged until the whole upgrade.

All twelve full compiler pass and nonpass sets are identical to source 24,
without crashes or timeouts. Go-history checker passes 13,518 diagnostic and
12,833 type/symbol baselines; canonical checker passes 13,518 diagnostic and
12,832 type/symbol baselines. Emit passes 13,522 diagnostic, 12,833 type and
symbol, 13,448 JavaScript/declaration, 149 JavaScript-map, and 156 source-map
baselines. The exact `compiler/indexAt` fixture passes all three variants in
diagnostics, types, symbols and JavaScript emit. Exact-source Fourslash passes
4,083 groups, skips 472 and fails none; all 4,615 named outcomes match source
24. The full Rust workspace passes 1,106 tests with four existing ignored tests;
the npm API suite passes 935 tests, and all 37 regression projects pass. The
dev build, WASM target check, warnings-denied Clippy, source lint and formatting
pass. All seven generators leave 1,172 tracked crate files byte-identical.
The first workspace attempt was denied a local Unix socket by the sandbox;
the complete rerun with socket access passed. The npm shutdown test must run
from the npm package directory for its child process to resolve the package;
the full suite passed there.

The compatibility table and README summaries need no change: this port alters
documentation wording without changing a capability or a cited pass count.
No source-25 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above; no performance-equivalence claim is made for this port.
Evidence is under `target/port-623/0025-a4b1410230/`, including exact-source
library hashes, full compiler pass-set and Fourslash outcome comparisons,
workspace, npm, regression, lint, WASM and generator logs.

## 26. `efbf3b506754` — VS Code JSX auto-insert in content-mapped files

Upstream `efbf3b5067547539c9cf8331d51943543c8bc91a` changes only the
TypeScript VS Code extension: it moves the JSX auto-insert provider into the
selector-scoped registrations so content-mapped extensions are included, and
it disposes a conditional-registration listener when that selector changes.
tsrs ships the language server but not that VS Code client. Its existing
`_vs_onAutoInsert` request implementation is unchanged, and language-server
content mappers remain unported. There is therefore no tsrs production code
to change for this upstream client commit; JSX auto-insert in content-mapped
files remains unavailable through tsrs. The existing compatibility table
already records that language-server content-mapper gap. The ignored
reference checkout advances to source 26, while the published TypeScript
pin and base SDK manifest remain unchanged until the full source upgrade.

All twelve full checker and emit pass and nonpass sets match source 25
exactly, with no crashes or timeouts. Go-history checker passes 13,518
diagnostic and 12,833 type/symbol baselines; canonical checker passes 13,518
diagnostic and 12,832 type/symbol baselines. Emit passes 13,522 diagnostic,
12,833 type/symbol, 13,448 JavaScript/declaration, 149 JavaScript-map and
156 source-map baselines. The exact-source Fourslash gate passes 4,083 groups,
skips 472 and fails none; all 4,615 named outcomes match source 25. The dev
build, warnings-denied Clippy, source lint and formatting pass. The Rust
workspace and generators were not rerun because this port changes no Rust,
npm or generator input; source 25's full workspace and generator checks passed.

The first Fourslash attempt could not bind a local Unix socket in the sandbox.
An initial socket-enabled run used the gate helper's default Cargo.toml pin
(the unchanged published base), rather than source 26, and encountered that
older reference's known quick-info mismatch. The final run used a fresh
source-26 snapshot and the rebuilt server, and passed. No source-26 release
performance measurement was collected under the user's nonblocking migration
policy; no runtime code changes in this port. The measured source-11
regressions remain disclosed above. Evidence is under
`target/port-623/0026-efbf3b5067/`, including the exact-source snapshot
stamp, full compiler pass-set comparison and Fourslash outcome comparison.

## 27. `dca8e236a227` — bump `brace-expansion` to 5.0.12

Upstream `dca8e236a227e37183dcf1dd4e51b22571f5b558` changes only its
`package-lock.json`, moving `brace-expansion` from 5.0.9 to 5.0.12. tsrs uses
pnpm, and its existing lockfile already selects 5.0.12 with the exact upstream
SHA-512 integrity. This is a transitive development-tool dependency through
`void` and `minimatch`; there is no tsrs code or lockfile change to make. The
installed package reports 5.0.12, and `minimatch` successfully expands a
brace-pattern smoke case. The ignored reference checkout advances to source
27, while the published TypeScript pin and base SDK manifest stay unchanged.

All twelve complete checker and emit pass and nonpass sets match source 26,
with no crashes or timeouts. Go-history checker passes 13,518 diagnostic and
12,833 type/symbol baselines; canonical checker passes 13,518 diagnostic and
12,832 type/symbol baselines. Emit passes 13,522 diagnostic, 12,833
type/symbol, 13,448 JavaScript/declaration, 149 JavaScript-map and 156
source-map baselines. The exact-source Fourslash gate passes 4,083 groups,
skips 472 and fails none; all 4,615 named outcomes match source 26. The dev
build, warnings-denied Clippy, source lint and formatting pass. The Rust
workspace, npm API suite and generators reuse their last successful checks:
the upstream compiler, library, generator and SDK input trees, as well as
all tsrs product inputs, are byte-identical to source 26.

The frozen pnpm lock passes its supply-chain check, but an install attempt
exits with the repository's existing ignored-build-script policy for
`esbuild` and `workerd`; the exact package is installed and the focused
dependency smoke passes. This does not affect the compiler or Fourslash
gates. No compatibility table or README status changes are needed because
this port changes no user-visible capability or cited pass count.

No source-27 release performance measurement was collected under the user's
nonblocking migration policy. The rebuilt dev compiler, test runner and
Fourslash server binaries have the same SHA-256 hashes as before this port,
and the runtime source and assets are unchanged. The measured source-11
regressions remain disclosed above. Evidence is under
`target/port-623/0027-dca8e236a2/`, including source-tree and binary
identity, dependency resolution, complete compiler pass-set and Fourslash
outcome comparisons, and build and lint logs.

## 28. `0ad3777160ac` — VS Code LSP response middleware

Upstream `0ad3777160acc032a2063da0cb0edd02be8b23a9` adds
`registerLspMiddleware` to its VS Code extension API. The new client-side
registry transforms language-feature responses after the server replies,
with ordered callbacks, cloned values, cancellation and error fallback, and
registrations that survive server restarts. It also exports type-only
`@typescript/typescript/unstable/vscode` declarations and moves the LSP
protocol generator from `tsc/internal/lsp/lsproto/_generate/` to
`tools/scripts/lsp/`. The generated Go protocol bindings, Go compiler and
LSP-server behavior, and conformance fixtures are unchanged.

tsrs ships an LSP server, not a VS Code extension. Users can point the
upstream TypeScript extension at tsrs, so that extension's middleware can
operate on tsrs responses without a server change. tsrs has no extension
registration API to implement, and adding a `tsrs/unstable/vscode` export
would claim support for a client it does not provide. The corresponding
tsrs changes update the Rust protocol generator's two moved-source
attributions, the LSP guide, and the compatibility table's explicit
extension-API status. The ignored reference checkout advances to source
28; the published TypeScript pin and base SDK remain unchanged.

All twelve complete checker and emit pass and nonpass sets match source
27 exactly, with no crashes or timeouts. Go-history checker passes 13,518
diagnostic and 12,833 type/symbol baselines; canonical checker passes
13,518 diagnostic and 12,832 type/symbol baselines. Emit passes 13,522
diagnostic, 12,833 type/symbol, 13,448 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. The exact-source Fourslash
gate passes 4,083 groups, skips 472 and fails none; all 4,615 named
outcomes match source 27. The LSP protocol generator is byte-identical
after formatting, and the generated-Go-to-Rust check finds all 3,686
names. The dev build, warnings-denied Clippy, source lint and formatting
pass. The full Rust workspace, npm API suite and unrelated generators
were not rerun: their runtime and generator inputs are unchanged from
source 27, whose preceding full checks passed.

No source-28 release performance measurement was collected under the
user's nonblocking migration policy. The rebuilt dev compiler, test
runner and Fourslash server binaries have the same SHA-256 hashes as
source 27; no runtime code changed. The measured source-11 regressions
remain disclosed above. Evidence is under
`target/port-623/0028-0ad3777160/`, including upstream source-tree and
binary identity, complete compiler pass-set and Fourslash outcome
comparisons, LSP generator identity, build and lint logs.

## 29. `d2f9dd68cfff` — checker-scoped merged symbol API methods

Upstream `d2f9dd68cfff50ca3c2fdf3152f808fd9d4ca2d6` adds four
checker requests: `getMergedSymbol`, `getSymbolOfNode`,
`getSymbolOfDeclarationForChecker`, and `getParentOfSymbolForChecker`.
The matching Rust checker exports and API handlers use the specified
snapshot/project's persistent checker, resolve node or symbol handles
through its registry, and return nullable symbols where the Go handlers
do. The sync and async SDKs, generated protocol types, generator methods,
and exact upstream tests advance with the source. A Rust real-program
test covers merged namespaces, missing node symbols, merged parents and
late-bound computed members. The existing file-owned `getSymbol(decl)`
continues to return the raw binder symbol; its no-symbol behavior remains
unchanged even though upstream removes redundant assertions for it.
The API coverage matrix now has 177 methods (58 core, 119 checker). The
ignored reference checkout advances to source 29; the published TypeScript
pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass and nonpass sets match source
28 exactly, without crashes or timeouts. Go-history checker passes 13,518
diagnostic and 12,833 type/symbol baselines; canonical checker passes
13,518 diagnostic and 12,832 type/symbol baselines. Emit passes 13,522
diagnostic, 12,833 type/symbol, 13,448 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. Exact-source Fourslash
passes 4,083 groups, skips 472 and fails none; all 4,615 named outcomes
match source 28. Go API and checker package tests pass. Four new upstream
sync/async client cases pass against both the source-29 Go server and tsrs;
the generator parity case and full npm API suite pass (939/939). The full
Rust workspace passes 1,108 tests with four existing ignored tests, and
the regression projects pass. The dev CLI/testrunner and Fourslash server
builds, WASM target check, warnings-denied Clippy, source lint and format
check pass. All seven generators leave 1,172 tracked crate files
byte-identical.

The first workspace and Fourslash attempts could not bind local Unix
sockets in the sandbox; complete reruns with socket access passed. An
initial full npm run from the repository root reached 938/939, with its
shutdown child unable to resolve the package from that working directory;
the full run from `npm/tsrs` passed. No source-29 release performance
measurement was collected under the user's nonblocking migration policy;
these changes add API methods and checker exports without altering the
compiler's existing call paths. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0029-d2f9dd68cf/`,
including exact-source Go and npm runs, full compiler pass-set and
Fourslash outcome comparisons, workspace, regression, WASM, lint and
generator logs.

## 30. `7e5d1c1c1de4` — retry Darwin FSEvents delivery tests

Upstream `7e5d1c1c1de408086e2dd74cb42d5b9c6ba77461` changes only
Go tests: nine Darwin-specific FSEvents event-delivery cases now run
through `runWithRetry`, with fresh temporary directories and scaled
timeouts on each attempt. Tsrs's generic watcher tests already call
`run_with_retry` through `run_for_each_watcher`, with the same three
attempts and 1×/5×/15× timeout scaling. None of the nine modified
Darwin-specific Go cases has a Rust counterpart yet, so this port aligns
the retry-helper description and records that test coverage gap; it does
not change production behavior or claim new coverage. The ignored
reference checkout advances to source 30. The published TypeScript pin
and base SDK manifest remain unchanged until the full upgrade.

The exact-source Go `fswatch` package passes with native FSEvents delivery
outside the sandbox. Its first sandboxed attempt could not start FSEvents
streams, so its failures were environmental; both logs are preserved.
All twelve complete checker and emit pass and nonpass sets match source
29 exactly, with no new crashes or timeouts: Go-history checker passes
13,518 diagnostic and 12,833 type/symbol baselines; canonical checker
passes 13,518 diagnostic and 12,832 type/symbol baselines; emit passes
13,522 diagnostic, 12,833 type/symbol, 13,448 JavaScript/declaration,
149 JavaScript-map and 156 source-map baselines. Exact-source Fourslash
passes 4,083 groups, skips 472 and fails none; all 4,615 named outcomes
match source 29. The dev CLI/testrunner and Fourslash server build,
warnings-denied Clippy, source lint and format check pass. The full
Rust `tsrs_fswatch` all-targets suite passes: 45 unit and three native
macOS integration tests. The full Rust workspace and unrelated npm,
WASM and generator checks were not rerun: their runtime and generator
inputs are unchanged from source 29, whose complete checks passed.
No source-30 release performance measurement was collected under the
user's nonblocking migration policy. No production runtime code changed;
the measured source-11 regressions remain disclosed above. Evidence is
under `target/port-623/0030-7e5d1c1c1d/`.

## 31. `ed4807212c28` — strongly typed file paths

Upstream `ed4807212c28c90777c1d7ef2bf8e47af5d08519` introduces
`RootedPath`, `RootedFilePath`, `RootedDirectoryPath`, `PathKey` and
`CaseSensitivity` across the Go compiler, project system, API, virtual
filesystem and test infrastructure. The corresponding Rust port focuses on
the observable semantics: a dynamic URI with no authority separator is not
treated as a valid root, and common-parent grouping uses canonicalized
components on case-insensitive filesystems. Auto-imports rebase unresolved
relative module augmentations from the importing file, retain extensionless
specifier text, and keep rooted ambient module names rooted. A trailing-slash NodeNext
directory import now reports TS2834, rather than TS2835. The test runner
preserves the parsed configuration's base directory and case sensitivity, so
TS6059 explains the matching include pattern and emits related TS1408.
The new empty/dot-stem filename and trailing-slash type-reference fixtures
also pass. Existing Rust watch-root handling already normalizes `..`.

The npm SDK advances its source-31 API, AST, enum, path, protocol and test
files, including the `unstable/path` export. The Rust API sends numeric
`caseSensitivity` in `initialize`, omits `configFileName` for inferred
projects, resolves raw compiler-option paths against the server directory,
rejects invalid document identifiers, and resolves relative callback
`realpath` results from the queried path's directory. The Go Fourslash
subprocess bridge now accepts upstream's typed virtual-filesystem paths and
project-state path keys. The Rust compiler continues to use its established
string and canonical `Path` storage internally; the Go-wide compile-time
newtype conversion is not mechanically mirrored in every Rust signature.
The ignored reference checkout advances to source 31; the published
TypeScript pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass and nonpass sets match source 30,
with no lost passing cases or new failures, crashes or timeouts. The three
new compiler cases pass in both histories and emit. Go-history checker passes
13,521 diagnostics and 12,836 type/symbol baselines; canonical checker
passes 13,521 diagnostics and 12,835 type/symbol baselines. Emit passes
13,525 diagnostics, 12,836 type/symbol, 13,451 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines.
The exact-source Fourslash suite passes 4,084 groups, skips 472 and fails
none. All 4,615 named source-30 outcomes are preserved; the five new
empty/dot-stem rename outcomes pass. Its first run found a CSS-module
auto-import mismatch at the newly added extensionless and rooted cases;
the auto-import representation now preserves both kinds of module name,
and the full rerun passes all 4,620 named outcomes.
The source-31 npm SDK build and source-condition typecheck pass. All 18
changed API/AST/enum source files match upstream byte for byte; eight
changed test files match after the tsrs package-name edit, and reversing
the existing async connection-loss patch restores upstream's client file.
The full npm API suite passes 953/953, including the new typed-path and
relative callback-realpath cases. Exact-source Go path, options, AST,
encoder and request-filesystem package tests pass. The regression
projects and WASM target check pass.
The full Rust workspace passes 1,111 tests with four existing ignored
tests. The initial workspace run exposed three stored source-24 checker
oracle responses for document identifiers that source 31 now rejects.
An exact-source Go oracle confirmed all three new errors; a three-row
override updates those expectations while retaining the other 81
checker-response comparisons. The final workspace and focused API
configuration test pass. The dev CLI/testrunner and Fourslash-server
builds, warnings-denied Clippy, source lint and formatting pass. All seven
generators plus formatting leave 1,172 tracked crate files byte-identical.

No source-31 release performance measurement was collected under the
user's nonblocking migration policy. The measured source-11 regressions
remain disclosed above. Evidence is under
`target/port-623/0031-ed4807212c/`, including exact-source Go and npm
API results, full compiler pass-set and Fourslash outcome comparisons,
workspace, regression, WASM, lint and generator-integrity logs.

## 32. `cb72b2e17b75` — update localization files

Upstream `cb72b2e17b75557db7e3286d835544aef5d85f6c` refreshes the
OneLoc handbacks and deterministic runtime archives for 13 locales. The 26
JSON and gzip files in `crates/tsrs_diagnostics/loc/` match that commit byte
for byte; every handback has 2,220 sorted diagnostic keys, and every gzip
payload has the matching key/value map. The new
`tools/gen-diagnostics/import_localization.mjs` ports the upstream handback
rules to this repository's Python tooling: only completed `Loc` targets
replace existing translations, pending `Update` and unstated targets keep
their previous text, obsolete and English-equivalent entries are removed,
invalid metadata leaves the output unchanged, and the result is sorted CRLF
JSON. The locale checker validates canonical formatting, placeholders and
archive contents in `tools/gen-check.sh`; its importer cases mirror the new
upstream Go tests. The upstream Azure OneLoc pipeline is Go-repository
infrastructure and is not copied here.

Localized output remains a pre-existing gap: tsrs does not yet load these
catalogs, and `--locale` still leaves English messages unchanged. This data
port does not claim to implement that runtime subsystem. The ignored
reference checkout advances to source 32; the published TypeScript pin and
base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets match source 31 with
no lost passes, new failures, crashes or timeouts. Go-history checker passes
13,521 diagnostics and 12,836 type/symbol baselines; canonical checker passes
13,521 diagnostics and 12,835 type/symbol baselines. Emit passes 13,525
diagnostics, 12,836 type/symbol, 13,451 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. Exact-source Fourslash passes
4,084 groups, skips 472 and fails none; all 4,620 named source-31 outcomes
are unchanged. The source-32 Go diagnostics and importer package tests pass,
as do the seven Rust diagnostics tests, importer cases, all 1,111 workspace
tests (four ignored), regression projects, WASM check, warnings-denied
Clippy, source lint, formatting and generator integrity. Workspace and
Fourslash needed escalated reruns only because the sandbox denied Unix-socket
binds.

No source-32 release performance measurement was collected under the user's
nonblocking migration policy. No production Rust or npm runtime code
changed; the staged catalogs are not embedded or loaded by tsrs. The measured
source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0032-cb72b2e17b/`.

## 33. `f9f8d0129256` — fix formatter token-line comparison

Upstream `f9f8d01292562242b6e7c7142e46ed7e470926f9` corrects
`FormattingContext.TokensAreOnSameLine` to compare the start of the current
token with the **start** of the next token. A next token may extend over
multiple lines without changing whether the two token starts share a line.
The Rust formatter now makes that same comparison and retains the upstream
tri-state cache. Focused Rust regressions reproduce the source-32 failures
for template-head/middle spacing and an unterminated JSX attribute string,
then pass with the fix. The upstream commit also removes skips from two
template-formatting Fourslash tests and changes the JSX fixture's expected
spacing; those three exact source-33 Go Fourslash cases pass against the Rust
server. The ignored reference checkout advances to source 33; the published
TypeScript pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets match source 32 with
no lost passes, new failures, crashes or timeouts. Go-history checker passes
13,521 diagnostics and 12,836 type/symbol baselines; canonical checker passes
13,521 diagnostics and 12,835 type/symbol baselines. Emit passes 13,525
diagnostics, 12,836 type/symbol, 13,451 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. Exact-source Fourslash passes
4,086 groups, skips 470 and fails none; of all 4,620 named outcomes, only
`TestFormatNoSpaceAfterTemplateHeadAndMiddle` and
`TestFormatSpaceAfterTemplateHeadAndMiddle` change from skip to pass. The
source-33 Go formatter package, focused Rust formatter tests, full Rust
workspace, regression projects, warnings-denied Clippy, source lint,
formatting and generator integrity pass. Fourslash and workspace needed
escalated reruns only because the sandbox denied Unix-socket binds. The
capability table and language-server documentation now cite the measured
Fourslash counts.

No source-33 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0033-f9f8d01292/`.

## 34. `50d70a3f5f45` — stable inferred type-parameter order

Upstream `50d70a3f5f453a79a4323b263165da51f656a4e3` sorts the
`getInferTypeParameters` result with `CompareTypes` before the parameters are
used to construct conditional type mappers. The Rust function now uses its
existing `compare_types` comparator and Go-compatible `sort_func`, preserving
the upstream order without changing the local symbol table. The new
`stableTypeMapperOrdering` fixture passes for diagnostics, JavaScript,
`.types`, and `.symbols`; its result type retains `"m"` middleware in both
endpoint alternatives. It also passed before the Rust edit because the
source-33 local-symbol iteration happened to produce the expected order, so
the explicit sort is needed for algorithmic parity beyond that fixture. The
ignored reference checkout advances to source 34; the published TypeScript
pin and base SDK manifest remain unchanged until the full upgrade.

All existing cases retain their outcomes across the twelve complete checker
and emit pass/nonpass sets. The new fixture is added as a diagnostics,
type/symbol, and JavaScript pass and as a map-baseline skip. Go-history checker
passes 13,522
diagnostics and 12,837 type/symbol baselines; canonical checker passes 13,522
diagnostics and 12,836 type/symbol baselines. Emit passes 13,526 diagnostics,
12,837 type/symbol and 13,452 JavaScript/declaration baselines; the 149
JavaScript-map and 156 source-map passes are unchanged. No prior pass was lost
and no new failure, crash, or timeout appeared. Exact-source Fourslash passes
4,086 groups, skips 470, fails none, and all 4,620 named outcomes match
source 33. The source-34 Go checker package, 1,113 Rust workspace tests (four
ignored), regression projects, WASM check, warnings-denied Clippy, source
lint, formatting, and all seven generator checks pass. Fourslash and workspace
used the previously required Unix-socket sandbox escalation.

No source-34 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0034-50d70a3f5f/`.

## 35. `6b3da828d7f5` — remove legacy localization handbacks

Upstream `6b3da828d7f59669ba3b26e9a466b7bde9a5f6d4` deletes only the 13
legacy `tools/loc/*/diagnosticMessages.generated.json.lcl` handbacks. tsrs
never tracked `.lcl` files or referenced that format, so there is no source
asset or runtime behavior to change. The 13 source-32 imported JSON catalogs
and their 13 matching gzip archives are a separate, validated data format and
remain in place. The existing `--locale` gap is unchanged: tsrs still emits
English messages. The ignored reference checkout advances to source 35; the
published TypeScript pin and base SDK manifest stay unchanged until the full
upgrade.

All twelve complete checker and emit pass/nonpass sets match source 34 exactly.
Go-history checker passes 13,522 diagnostics and 12,837 type/symbol baselines;
canonical checker passes 13,522 diagnostics and 12,836 type/symbol baselines.
Emit passes 13,526 diagnostics, 12,837 type/symbol, 13,452 JavaScript/declaration,
149 JavaScript-map, and 156 source-map baselines. No passing case was lost and
no new failure, crash, or timeout appeared. Exact-source Fourslash passes
4,086 groups, skips 470, fails none, and all 4,620 named outcomes match source
34. The source audit confirms zero tsrs `.lcl` files and references. Locale
catalog validation, importer unit cases, seven Rust diagnostics tests, the dev
build, warnings-denied Clippy, source lint, formatting, and generator integrity
pass. The Fourslash subprocess gate used the previously required Unix-socket
sandbox escalation.

No source-35 release performance measurement was collected under the user's
nonblocking migration policy. This commit changes documentation only and
leaves production code and shipped locale data untouched. The measured
source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0035-6b3da828d7/`.

## 36. `bcd7c422559d` — retain the idle cache clean timer for session shutdown

Upstream `bcd7c422559da26b7e60a46a41cf5503a360d106` stores the newly
scheduled idle cache clean timer on `Session` so `cancelIdleCacheClean` and
`closeIdleCacheClean` can stop it. Without that handle, closing a session may
wait for the full 30-second delay. tsrs already stores the timer in
`idle_cache_clean.timer` from the source-22 session port. Cancellation removes
and stops that handle; close also waits for any callback already in flight. The
matching Rust regression opens a file to schedule the timer and verifies that
`Session::close` returns within five seconds. This source commit needs no
production change and does not change any user-visible capability. The ignored
reference checkout advances to source 36; the published TypeScript pin and
base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets match source 35 exactly,
with no lost passes, new failures, crashes or timeouts. Go-history checker
passes 13,522 diagnostics and 12,837 type/symbol baselines; canonical checker
passes 13,522 diagnostics and 12,836 type/symbol baselines. Emit passes 13,526
diagnostics, 12,837 type/symbol, 13,452 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. The exact upstream Go timer
regression, the full exact-source Go project package, and its Rust counterpart
pass, as do all 37 CLI regression projects.

Exact-source Fourslash passes 4,086 groups, skips 470, fails none, and all
4,620 named outcomes match source 35. The full Rust workspace passes 1,114
tests with four existing ignored, including the new timer case. The first
sandboxed workspace run could not bind a Unix socket; an escalated run made
that case pass but overlapped the 14-worker emit gate and saw one overflow in
the FSEvents all-core stress test. Both the isolated stress test and the
serial, escalated full workspace pass. The dev CLI/testrunner and fresh
Fourslash server builds, warnings-denied Clippy, source lint, formatting and
generator integrity pass.

No source-36 release performance measurement was collected under the user's
nonblocking migration policy. This port changes a test and documentation only;
the source-22 production implementation is untouched. The measured source-11
regressions remain disclosed above. Evidence is under
`target/port-623/0036-bcd7c42255/`.

## 37. `ca197b7b8586` — restart imports after a lower node_modules depth

Upstream `ca197b7b8586900c3b75c0365fac31cfcaaacca7` changes
`filesParser.start` to revisit a file's imports when a later path reaches that
file at a lower `node_modules` depth, even when its subtask list was already
started. The Rust parser now computes the same `lowered` condition and queues
the existing subtasks again at the file's new lowest depth. A symlinked package
first reached through a package import and then relatively no longer marks its
transitive `util.ts` as an external-library file, allowing it to be emitted.
The dedicated Rust virtual-filesystem regression puts the package-importing
root before the relative-importing root; its external-library assertion failed
before the change, while both classification and emit eligibility pass
afterward. The new exact-source Go conformance fixture and its Rust diagnostic
and JavaScript baselines also pass. The upstream fixture passed before the Rust
change under Rust's usual root order,
which is why the reversed-order regression is retained. The ignored reference
checkout advances to source 37; the published TypeScript pin and base SDK
manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets retain their source-36
outcomes. The new fixture adds one diagnostics and one JavaScript pass, with no
lost passing case or new failure, crash or timeout. Go-history checker passes
13,523 diagnostics and 12,837 type/symbol baselines; canonical checker passes
13,523 diagnostics and 12,836 type/symbol baselines. Emit passes 13,527
diagnostics, 12,837 type/symbol, 13,453 JavaScript/declaration, 149
JavaScript-map and 156 source-map baselines. Exact-source Fourslash passes
4,086 groups, skips 470, fails none, and all 4,620 named outcomes match source
36. The source-37 Go fixture, focused Rust regression, full Rust workspace
(1,115 passed, four existing ignored), CLI regression projects, WebAssembly
check, dev builds, warnings-denied Clippy, source lint and formatting pass.
All seven generators leave the preexisting crate diff byte-identical; the
generator script's clean-tree check is run after the corresponding commit.
Fourslash and the workspace socket test used the previously required Unix
socket sandbox escalation. The capability table's cited diagnostic and
JavaScript pass counts are updated.

No source-37 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0037-ca197b7b85/`.

## 38. `a1ef42b9ea70` — stabilize diagnostics for JS constructor properties

Upstream `a1ef42b9ea7032fa60df127d42b4c86fd2a110ee` eagerly resolves the
types of a JavaScript class's own constructor-declared `this` properties after
checking its members. The same step runs for class declarations and deferred
class expressions; inherited, typed, and method-only properties are excluded.
The Rust checker mirrors both call sites and the constructor-parent condition.
This makes implicit-any diagnostics on properties initialized only to
`undefined` or `null` independent of later symbol access. The tracked
`js-constructor-property-diagnostics` regression covers both class forms and
an annotated property: source-37 tsrs exited without diagnostics, while the
exact-source Go compiler and source-38 tsrs both report the same two TS7008
diagnostics. All four variants of upstream
`flakyDiagnosticsJsClassPropertyInitializedToUndefined` pass their diagnostic,
type, symbol, and JavaScript baselines. The ignored reference checkout advances
to source 38; the published TypeScript pin and base SDK manifest remain
unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets retain their source-37
outcomes. The new fixture adds four passes in each applicable baseline, with no
lost passing case, new failure, crash, or timeout. Go-history checker passes
13,527 diagnostics and 12,841 type/symbol baselines; canonical checker passes
13,527 diagnostics and 12,840 type/symbol baselines. Emit passes 13,531
diagnostics, 12,841 type/symbol, 13,457 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,086 groups, skips 470, fails none, and all 4,620 named outcomes match source
37. The exact-source Go fixture, full Rust workspace (1,120 passed, four
existing ignored), all 38 CLI regression projects, WebAssembly check, dev
builds, warnings-denied Clippy, source lint, formatting, and generator output
comparison pass. The Fourslash Unix-socket test needed its previously required
sandbox escalation. The compatibility table's source revision and cited
diagnostic, type/symbol, and JavaScript pass counts are updated; the root and
npm README summaries contain no corresponding numbers.

No source-38 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0038-a1ef42b9ea/`.

## 39. `fc636a691f1d` — await async API disposal and suggest `await using`

Upstream `fc636a691f1dbdbc9ba678911891a9e85ec283bf` makes async
`Snapshot`, `Program`, and `BuildOrchestrator` expose
`[Symbol.asyncDispose](): Promise<void>` and return the `dispose()` Promise.
The corresponding npm SDK methods now match upstream byte for byte, while
their sync counterparts call synchronous `dispose()` directly. The two
upstream API test files are ported with only the established npm package-name
substitution; their `await using` and `using` cases run against the tsrs
server. The Rust checker also mirrors the upstream `using` assignment check:
when the initializer lacks `[Symbol.dispose]` but is assignable to
`AsyncDisposable | null | undefined`, it appends TS18116 suggesting
`await using`. The English catalog is regenerated from this source revision;
its exact-count test advances from 2,220 to 2,221 messages. The ignored
reference checkout advances to source 39. The published TypeScript pin and
base SDK manifest remain unchanged until the full upgrade.

The new `usingAsyncDisposable` fixture exposed the missing suggestion before
the change and passes after it; the exact-source Go fixture passes. All twelve
complete checker and emit pass/nonpass sets retain their source-38 outcomes
with no removed passes or changed nonpassing cases. The one new fixture adds
one diagnostics, type, symbol, and JavaScript pass in each applicable mode.
Go-history checker passes 13,528 diagnostics and 12,842 type/symbol baselines;
canonical checker passes 13,528 diagnostics and 12,841 type/symbol baselines.
Emit passes 13,532 diagnostics, 12,842 type/symbol, 13,458
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,086 groups, skips 470, fails none, and all
4,620 named outcomes match source 38. The changed sync/async npm API files
pass 721/721 tests, and the complete npm SDK suite passes 953/953. Full Rust
workspace all-targets passes 1,120 tests with four existing ignored; all 38
CLI regression projects, WebAssembly check, dev and npm SDK builds, SDK
source-condition typecheck, warnings-denied
Clippy, source lint, formatting, and seven generators pass. The generator
rerun leaves the tracked crate diff and untracked-file list byte-identical.
Fourslash and workspace Unix-socket tests required the previously observed
sandbox escalation. The capability table and Node API documentation record
the updated counts and async disposal behavior; no root or npm README
summary cites these counts.

No source-39 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0039-fc636a691f/`.

## 40. `de61e6962143` — update DOM and WebWorker typings

Upstream `de61e696214322f72d967bd17c58c50e44b2920d` updates bundled
`lib.dom.d.ts` and `lib.webworker.d.ts`, including CSS, DOM, Serial, and
WebTransport declarations. The Rust bundled copies match the source-40 Go
files byte for byte. The upstream commit also updates two type baselines;
`mappedTypeRecursiveInference` and `globalThisBlockscopedProperties` pass
their exact-source diagnostic, type, symbol, and JavaScript baselines. The
ignored reference checkout advances to source 40. The published TypeScript
pin and base SDK manifest remain unchanged until the full upgrade.

The new DOM declarations add 34 `string` references to the language-service
smoke fixture. Its one affected expected response was regenerated with a
Go compiler built from source 40; the Rust response matches the new Go JSON
byte for byte, and the other 38 responses remain unchanged. All twelve
complete checker and emit pass/nonpass sets retain their source-39 outcomes,
with no removed passes, new failures, crashes, or timeouts. Go-history checker
passes 13,528 diagnostics and 12,842 type/symbol baselines; canonical
checker passes 13,528 diagnostics and 12,841 type/symbol baselines. Emit
passes 13,532 diagnostics, 12,842 type/symbol, 13,458 JavaScript/declaration,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
passes 4,086 groups, skips 470, fails none, and all 4,620 named outcomes
match source 39. Full Rust workspace all-targets, all 38 CLI regression
projects, WebAssembly check, dev builds, warnings-denied Clippy, source
lint, formatting, and all seven generators pass. The generator rerun leaves
the tracked crate diff byte-identical. Workspace and Fourslash Unix-socket
tests used the previously required sandbox escalation. The compatibility
table records the source revision and library update; no root or npm README
summary cites these counts.

No source-40 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0040-de61e69621/`.

## 41. `253bcd8623` — scope emit resolvers to one emit context and cache their node builders

Upstream `253bcd86231f2ceda21029eedf4c92c8b0d583f4` creates one emit
resolver and context per file, caches one node builder in that resolver, and
shares its declaration-visibility and JSX links through the checker. The Rust
checker now does the same for file emit and declaration diagnostics. Script
transforms, declaration transforms, and their deferred printers use the same
context and resolver for each file, resetting the context after both prints.
Declaration serialization no longer accepts a separate context, so its lazy
node builder cannot silently use a different factory. Language-service
completion, quick-info, reference, and signature-help paths also reuse a
context or builder at the scope introduced upstream. A focused compiler test
checks JS, declaration, and both map outputs across repeated emits with one
program. The ignored reference checkout advances to source 41. The published
TypeScript pin and base SDK manifest remain unchanged until the full upgrade.

The Go commit moves checker helper methods to `emitsupport.go` and removes its
checker-wide cached resolver. Rust keeps the existing helper methods and a
context-independent checker resolver for internal call sites; this avoids a
large mechanical module split while file emit always constructs a fresh
context-bound resolver. Rust's `EmitHost` still stores that per-file resolver,
because its scratch-region and deferred-printer architecture differs from Go's
host factory. The visibility/JSX caches remain checker-owned across those
per-file resolvers and scratch-keyed JSX links are cleared before the file's
scratch region is freed. These are Rust architectural deviations from the Go
patch, not intended output differences.

All twelve complete compiler checker and emit pass/nonpass sets retain their
source-40 outcomes, with no added or removed passes, failures, crashes, or
timeouts. Go-history checker passes 13,528 diagnostics and 12,842 type/symbol
baselines; canonical checker passes 13,528 diagnostics and 12,841 type/symbol
baselines. Emit passes 13,532 diagnostics, 12,842 type/symbol, 13,458
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,086 groups, skips 470, fails none, and all
4,620 named outcomes match source 40. The exact-source Go
`TestImportElision` changed by this commit passes. Full Rust workspace all-targets passes
1,121 tests with four existing ignored; all 38 CLI regression projects,
WebAssembly check, dev builds, warnings-denied Clippy, source lint,
formatting, and all seven generators pass. The generator rerun leaves tracked
crate changes and untracked crate files byte-identical. Workspace and
Fourslash Unix-socket tests used the previously required sandbox escalation.
The compatibility table records the source revision; the root and npm README
summaries contain no corresponding revision or counts.

Release performance was measured on public projects with one checker and
alternating source-40/source-41 release binaries. After one warm-up pair, VS
Code declaration-only emit (9,399 byte-identical output files, five measured
pairs) had median wall time 7.421 versus 7.418 seconds and median peak RSS
2,065,219,584 versus 2,068,709,376 bytes (+0.17%). Compiler emit (120
identical output files, 21 measured pairs) had 0.2655 versus 0.2672 seconds
and 109,936,640 versus 109,740,032 bytes (-0.18% RSS); Compiler-Unions emit
(105 identical output files, 21 measured pairs) had 0.3439 versus 0.3413
seconds and 111,230,976 versus 111,345,664 bytes (+0.10% RSS). All ordered
diagnostics and emitted bytes matched between revisions. The largest wall
difference was +0.63% on the short Compiler run, below the project's 2%
wall-time regression threshold and within timing noise. `bench/count.mjs`
reported retired instructions unavailable on macOS/arm64 (`no
perf_event_open`), so there is no instruction-count comparison. Evidence is under
`target/port-623/0041-253bcd8623/`.

## 42. `afd02f148639` — recognize typed JSDoc parameter tags as type annotations

Upstream `afd02f14863971d7a1b424f7c5a7e6a4eb2f92a7` adds
`JSDocParameterTag` to `ast.GetTypeAnnotationNode`. The Rust AST utility now
returns the tag's `typeExpression` through its existing `type_node()` accessor,
which is used by the language-service object-binding completion guard. A
focused parser test parses a typed `@param` tag, observes the accessor return
`None` before the port, and observes the same type-expression node as `type_node()`
after the port. This is a direct source-equivalent change without a Rust-specific
behavioral deviation. The ignored reference checkout advances to source 42;
the published TypeScript pin and base SDK manifest remain unchanged until the
full upgrade.

All twelve complete compiler checker and emit pass/nonpass sets retain their
source-41 outcomes, with no added or removed passes, failures, crashes, or
timeouts. Go-history checker passes 13,528 diagnostics and 12,842 type/symbol
baselines; canonical checker passes 13,528 diagnostics and 12,841 type/symbol
baselines. Emit passes 13,532 diagnostics, 12,842 type/symbol, 13,458
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,086 groups, skips 470, fails none, and all
4,620 named outcomes match source 41. Full Rust workspace all-targets passes
1,122 tests with four existing ignored; all 38 CLI regression projects,
WebAssembly check, dev builds, warnings-denied Clippy, source lint, formatting,
and all seven generators pass. The generator rerun leaves the tracked crate
diff and untracked crate files byte-identical. Workspace and Fourslash
Unix-socket tests required the previously observed sandbox escalation. The
compatibility table records the source revision and typed JSDoc completion
context; the root and npm README summaries contain no corresponding revision
or counts.

No source-42 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0042-afd02f1486/`.

## 43. `1a78786ca7` — ignore import-type qualifier names while marking emit references

Upstream `1a78786ca751559170857893fe3e01eba7c69997` excludes the
qualified names in `typeof import()` types from the emit resolver's
alias-reference walk. Rust applies the same guard in `mark_linked_references`
using its existing `is_import_type_qualifier_part` helper. Before the change,
the three new upstream compiler fixtures showed one incorrectly retained
import and two extra missing-name diagnostics added during emit; afterward,
all three pass their diagnostic, type, symbol, and JavaScript baselines. This
is a direct source-equivalent change without a Rust-specific behavioral
deviation. The ignored reference checkout advances to source 43. The
published TypeScript pin and base SDK manifest remain unchanged until the
full upgrade.

All twelve complete checker and emit pass/nonpass sets retain their source-42
outcomes, with exactly the three new upstream cases added to each applicable
passing set and no removed passes, new failures, crashes, or timeouts.
Go-history checker passes 13,531 diagnostics and 12,845 type/symbol baselines;
canonical checker passes 13,531 diagnostics and 12,844 type/symbol baselines.
Emit passes 13,535 diagnostics, 12,845 type/symbol, 13,461
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,086 groups, skips 470, fails none, and all
4,620 named outcomes match source 42. Full Rust workspace all-targets passes
1,122 tests with four existing ignored; all 38 CLI regression projects,
WebAssembly check, dev builds, warnings-denied Clippy, source lint,
formatting, and all seven generators pass. The generator rerun leaves the
tracked crate diff and untracked crate files byte-identical. Workspace and
Fourslash Unix-socket tests required the previously observed sandbox
escalation. The compatibility table records the source revision, new pass
counts, stable diagnostics, and import elision; the root and npm README
summaries contain no corresponding revision or counts.

No source-43 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0043-1a78786ca7/`.

## 44. `21b260b4e7` — exclude top-level imports from document symbols

Upstream `21b260b4e7e5123727f33d506fa686fecc258b1a` stops the document-symbol
visitor before descending into an import or import-equals declaration directly
under a source file. Rust adds the equivalent guard after collecting attached
JSDoc tags, so top-level imports disappear from outlines while their JSDoc
typedef and callback symbols remain. The new exact-source upstream Fourslash
case failed before the change with five extra import symbols and passed after
it; all three existing navigation baselines changed upstream by this commit
also pass against the new Go snapshots. This is a direct source-equivalent
change without a Rust-specific behavioral deviation. The ignored reference
checkout advances to source 44. The published TypeScript pin and base SDK
manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets are unchanged from
source 43, with no removed passes, new failures, crashes, or timeouts.
Go-history checker passes 13,531 diagnostics and 12,845 type/symbol baselines;
canonical checker passes 13,531 diagnostics and 12,844 type/symbol baselines.
Emit passes 13,535 diagnostics, 12,845 type/symbol, 13,461
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,087 of 4,557 groups, skips 470, fails none;
the only addition among its 4,621 named outcomes is the new upstream case,
and no existing outcome changes. Full Rust workspace all-targets passes
1,122 tests with four existing ignored; all 38 CLI regression projects,
WebAssembly check, dev builds, warnings-denied Clippy, source lint,
formatting, and all seven generators pass. The generator rerun leaves the
tracked crate diff and untracked crate files byte-identical. Workspace and
Fourslash Unix-socket tests required the previously observed sandbox
escalation. The compatibility table and LSP guide record the new Fourslash
count and document-symbol behavior; the root and npm README summaries contain
no corresponding source revision or count.

No source-44 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0044-21b260b4e7/`.

## 45. `c3f14c2fe8` — preserve object-literal parent during decorator metadata emit

Upstream `c3f14c2fe8971440d5a8bf1432d36bb26828c7c7` visits decorated object
literals with the literal as the current metadata parent, then restores the
enclosing parent. Rust now follows that control flow. This prevents a method or
accessor inside an object literal nested in a class from being treated as a
class member while visiting decorator metadata. The new exact-source compiler
fixture `decoratorMetadataObjectLiteralMethodNoCrash` passes its diagnostic,
type, symbol, and JavaScript baselines. It also passed before the Rust edit:
Rust's existing class-element guard already avoided the upstream Go crash, so
the new code closes a source-level parent-tracking gap without a measurable
baseline change on the fixture. The ignored reference checkout advances to
source 45. The published TypeScript pin and base SDK manifest remain unchanged
until the full upgrade.

All twelve complete checker and emit pass/nonpass sets preserve every source-44
pass and have no new failures, crashes, or timeouts. The new upstream fixture
adds one pass to each applicable diagnostic, type, symbol, and JavaScript set.
Go-history checker passes 13,532 diagnostics and 12,846 type/symbol baselines;
canonical checker passes 13,532 diagnostics and 12,845 type/symbol baselines.
Emit passes 13,536 diagnostics, 12,846 type/symbol, 13,462
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,087 of 4,557 groups, skips 470, fails none;
all 4,621 named outcomes are unchanged from source 44. Full Rust workspace
all-targets passes 1,122 tests with four existing ignored; all 38 CLI regression
projects, WebAssembly check, dev builds, warnings-denied Clippy, source lint,
and formatting pass. The generator rerun leaves the staged crate diff and
untracked crate files unchanged. Workspace and Fourslash Unix-socket tests
required the previously observed sandbox escalation. The compatibility table
records the source revision, new pass counts, and decorator metadata behavior;
the root and npm README summaries contain no corresponding revision or counts.

No source-45 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0045-c3f14c2fe8/`.

## 46. `09d4966578ae` — report reference diagnostics for configless API programs

Upstream `09d4966578aec90fdec11784559218392602a029` makes
`CreateDiagnosticAtReferenceSyntax` return no located diagnostic when an
API-created program has project references but no config file. Its caller then
reports an ordinary program diagnostic instead of dereferencing a missing
config file. Rust already had the equivalent `config.config_file?` guard and
caller fallback before this source commit; the new Rust API regression passed
against source 45 before the comment update. It now locks the upstream 6306
diagnostic and its absent file location (`pos` and `end` are `-1`). The exact
upstream Go API test also passes at source 46. The previous Rust comment
incorrectly described Go's pre-fix behavior and is now aligned with this
revision. The Rust API's pre-existing validation of relative reference paths
remains an intentional surface difference: it returns a structured client
error where Go can panic; absolute reference paths follow the new upstream
behavior. The ignored reference checkout advances to source 46; the published
TypeScript pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets are identical to
source 45: no removed passes, new failures, crashes, or timeouts. Go-history
checker passes 13,532 diagnostics and 12,846 type/symbol baselines; canonical
checker passes 13,532 diagnostics and 12,845 type/symbol baselines. Emit passes
13,536 diagnostics, 12,846 type/symbol, 13,462 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,087 of 4,557 groups, skips 470, fails none; all 4,621 named outcomes are
unchanged from source 45. The source-46 focused Rust and Go API tests, full
Rust workspace all-targets (1,123 passed, four existing ignored), dev builds,
all 38 CLI regression projects, WebAssembly check, warnings-denied Clippy,
source lint, formatting, and all seven generators pass. The generator rerun
leaves the staged crate diff and untracked crate files unchanged. Workspace
and Fourslash Unix-socket tests required the previously observed sandbox
escalation. The capability table records the source-46 API behavior; the root
and npm README summaries contain no corresponding revision or count.

No source-46 release performance measurement was collected under the user's
nonblocking migration policy. There is no production logic change in Rust;
the source edit corrects a comment and the new test guards existing behavior.
The measured source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0046-09d4966578/`.

## 47. `d61a7d2359` — skip disk-layout import diagnostics for custom API resolutions

Upstream `d61a7d235906e78be9b111de0ba060e28e67ecfa` marks a static or
callback module resolution as custom and skips checker diagnostics that assume
the import's path follows the on-disk resolution layout. Rust adds the same
internal `ResolvedModule` flag in the shared API conversion and guards the
corresponding checker branch. The new Rust API regression covers both a static
entry and a callback with `rewriteRelativeImportExtensions`: before the port
it reported two spurious TS2876 diagnostics, one for each import; afterward it
reports none. The exact upstream Go API test passes. The marker is internal,
as in Go, and does not change the `resolveModuleName` response. There is no
Rust-specific behavior or callback deviation in this port. The ignored
reference checkout advances to source 47. The published TypeScript pin and
base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets are identical to
source 46: no removed passes, new failures, crashes, or timeouts. Go-history
checker passes 13,532 diagnostics and 12,846 type/symbol baselines; canonical
checker passes 13,532 diagnostics and 12,845 type/symbol baselines. Emit passes
13,536 diagnostics, 12,846 type/symbol, 13,462 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,087 of 4,557 groups, skips 470, fails none; all 4,621 named outcomes are
unchanged from source 46. The full Rust workspace all-targets passes 1,124
tests with four existing ignored; all 38 CLI regression projects, WebAssembly
check, dev builds, warnings-denied Clippy, source lint, formatting, and all
seven generators pass. The generator rerun leaves the staged crate diff and
untracked crate files unchanged. Workspace and Fourslash Unix-socket tests
required the previously observed sandbox escalation. The capability table
and Node API guide record the new custom-resolution behavior; the root and
npm README summaries contain no corresponding revision or count.

No source-47 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0047-d61a7d2359/`.

## 48. `54f05ae22a` — keep malformed destructuring assignments from crashing emit

Upstream `54f05ae22afd5f0ec5ec3ae0333b03efa1156fe6` broadens three
transformer assertions: decorator array assignment elements accept any
expression, and class-field/decorator object assignment elements accept any
object-literal element. Rust mirrors those predicates. Parser recovery can
produce array expressions and object methods or accessors that are invalid
assignment targets; the normal visitor still needs to traverse them for
private-field, `super`, and decorator transforms. Against the source-48
fixtures before the Rust edit, all seven new variants crashed in emit. After
the edit, all seven match exact-source Go diagnostic and JavaScript baselines,
and the three variants with type/symbol baselines match those too. The four
variant cases explicitly disable type/symbol baselines upstream. The focused
Go compiler harness passes all seven variants. There is no Rust-specific
behavioral deviation. The ignored reference checkout advances to source 48;
the published TypeScript pin and base SDK manifest remain unchanged until
the full upgrade.

All twelve complete checker and emit pass/nonpass sets preserve every
source-47 pass with no new failure, crash, or timeout. The seven upstream
variants add seven diagnostic and JavaScript passes in each applicable mode,
and three type/symbol passes. Go-history checker passes 13,539 diagnostics
and 12,849 type/symbol baselines; canonical checker passes 13,539 diagnostics
and 12,848 type/symbol baselines. Emit passes 13,543 diagnostics, 12,849
type/symbol, 13,469 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines. Exact-source Fourslash passes 4,087 of 4,557 groups,
skips 470, fails none; all 4,621 named outcomes are unchanged from source 47.
The full Rust workspace all-targets passes 1,124 tests with four existing
ignored; all 38 CLI regression projects, WebAssembly check, dev builds,
warnings-denied Clippy, source lint, and formatting pass. Workspace and
Fourslash Unix-socket checks required the previously observed sandbox
escalation. All seven generators pass and leave the staged crate diff
unchanged. The compatibility table records the new pass counts and
malformed-input emit behavior; the root and npm README summaries contain no
corresponding source revision or count.

No source-48 release performance measurement was collected under the user's
nonblocking migration policy. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0048-54f05ae22a/`.

## 49. `91521cf229` — prefilter compiler test files before reading them

Upstream `91521cf2299d54a40d46fcf500130d273ddfc947` adds a conservative
Go `test.run` filename filter before opening and expanding compiler fixtures.
The Rust runner now makes the corresponding early decision for its own
selection interface: a plain mixed-case `--filter` can reject an unrelated
filename because configuration descriptions are lowercase, and `--list`
can reject filenames with no matching stem or configuration prefix. Rust
`--filter` also accepts arbitrary substring or regex matches over a different
test ID shape, so lowercase, regex, escaped, and other ambiguous patterns
continue to read the file. This is the intentional runner-specific adaptation;
it preserves the selected test set rather than copying the Go heuristic onto
incompatible CLI syntax. The upstream Go filter test and three focused Rust
unit tests pass. Against the actual fixture corpus, source-48 and source-49
runner binaries select byte-identical IDs for the full 15,278-variant list,
eight filter/list cases, and the 2,000-case WebAssembly sample list. The
ignored reference checkout advances to source 49; the published TypeScript
pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets are identical to
source 48: no removed passes, new failures, crashes, or timeouts. Go-history
checker passes 13,539 diagnostics and 12,849 type/symbol baselines; canonical
checker passes 13,539 diagnostics and 12,848 type/symbol baselines. Emit passes
13,543 diagnostics, 12,849 type/symbol, 13,469 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,087 of 4,557 groups, skips 470, fails none; all 4,621 named outcomes are
unchanged from source 48. The full Rust workspace all-targets passes 1,127
tests with four existing ignored; all 38 CLI regression projects, WebAssembly
check, dev builds, warnings-denied Clippy, source lint, formatting, and all
seven generators pass. Workspace and Fourslash Unix-socket checks required
the previously observed sandbox escalation. No user-visible capability or
conformance count changed, so the compatibility table and README summaries
remain current.

For the dev runner's `list --suite all`, ten warm paired timing samples give
0.3162 -> 0.0316 s for `--filter flakyDiagnostic` and 0.2937 -> 0.0314 s for
the two-ID `--list`; unfiltered listing is 0.3204 -> 0.3146 s and the
lowercase fallback is 0.2904 -> 0.2918 s. The 2,000-case list selects the
same IDs in 0.2881 -> 0.0710 s (five warm samples). Production compiler code
does not change. No source-49 release instruction-count or peak-RSS measurement
was collected under the user's nonblocking migration policy. The measured
source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0049-91521cf229/`.

## 50. `4529d07ab5` — expose API client modules for editor integrations

Upstream `4529d07ab5b7a2d2bbe0ef8b25edbed9f35b7b7f` adds a runtime
`unstable/vscode` API module loader to the TypeScript package and connects it
to its bundled VS Code extension's selected-SDK lifecycle. The Rust compiler
and Go test corpus do not change. The tsrs npm SDK copies upstream
`src/vscode/extensionApi.ts`, `moduleLoader.ts`, and `protocol.generated.ts`
byte for byte and exports `tsrs/unstable/vscode`. The loader takes the
selected package's manifest, resolves its real path and package name, checks
the full `typescript/unstable/*` specifier, and loads the module through that
package's own exports. It works with a selected `tsrs` package as well as
with a selected TypeScript package. Focused Node tests cover selected-package
isolation, module identity, export enforcement, invalid specifiers, and a
manifest without a name. The packed-package smoke test imports both API
clients through the loader, and its consumer type-checks the loader's public
declarations. This adds an SDK capability; the compatibility table, root
README, npm README, and Node API guide are updated.

tsrs does not bundle the upstream VS Code extension or build VSIX packages.
The upstream extension's selected-SDK event, API-pipe lifecycle, package
upgrade detection, restart prompt, client state handling, and VSIX packaging
therefore have no tsrs runtime counterpart. Its `ExtensionAPI` and
`TypeScriptSDK` interfaces are exported as upstream declaration types, but
no tsrs extension implements them. The loader itself does not guard Node's
module cache against an in-place package upgrade; upstream performs that
guard in its extension, which restarts the extension host when necessary.
The ignored reference checkout advances to source 50; the published
TypeScript pin and base SDK manifest remain unchanged until the full upgrade.

All twelve complete checker and emit pass/nonpass sets are identical to
source 49: no removed passes, new failures, crashes, or timeouts. Go-history
checker passes 13,539 diagnostics and 12,849 type/symbol baselines;
canonical checker passes 13,539 diagnostics and 12,848 type/symbol
baselines. Emit passes 13,543 diagnostics, 12,849 type/symbol, 13,469
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,087 of 4,557 groups, skips 470, fails none;
all 4,621 named outcomes are unchanged from source 49. The npm source suite
passes 956 tests, including the three new module-loader tests; the packed
package smoke test passes 25 steps, including selected-package module
loading and a declaration type-check. The full Rust workspace all-targets
passes 1,127 tests with four existing ignored; all 38 CLI regression projects,
WebAssembly check, dev builds,
warnings-denied Clippy, source lint, formatting, and all seven generators
pass. Workspace and Fourslash Unix-socket checks required the previously
observed sandbox escalation.

The Rust compiler and its bundled libraries do not change in source 50.
No source-50 release instruction-count or peak-RSS measurement was collected
under the user's nonblocking migration policy. The measured source-11
regressions remain disclosed above. Evidence is under
`target/port-623/0050-4529d07ab5/`.

## 51. `bea2e849e9` — require a namespace binding in deferred imports

Upstream `bea2e849e90b5ff2889f9e120e1e0fae5e17659b` parses a bare
`import defer "module"` as a deferred import clause and reports TS18117 because
it lacks a namespace binding. The Rust parser and checker now take the same
path, the plain-JavaScript grammar-error set retains TS18117 even without
`@ts-check`, and the default-import emit-helper predicate follows the Go
change. Diagnostic 18117 is generated from the new Go catalog entry; its
catalog-size assertion advances to 2,222. The dedicated CLI regression
`deferred-import-namespace-binding` checks bare deferred imports in TypeScript,
checked JavaScript, and unchecked JavaScript alongside valid namespace and
invalid named bindings. Its diagnostics and exit status match a Go compiler
built from this exact source commit. Before the port, both new upstream
`importDeferInvalidDefault` variants failed diagnostics and JavaScript emit;
afterward, both pass diagnostics, types, symbols, and emit.

All three full compiler gate modes pass. Source 51 replaces one former
`importDeferInvalidDefault` fixture with `commonjs` and `esnext` variants:
that one-for-two rename is the sole change across complete pass/nonpass sets.
Go-history checker passes 13,540 diagnostics and 12,850 type/symbol baselines;
canonical checker passes 13,540 diagnostics and 12,849 type/symbol baselines.
Emit passes 13,544 diagnostics, 12,850 type/symbol, 13,470 JavaScript,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
passes 4,087 of 4,557 groups, skips 470, fails none, and all 4,621 named
outcomes are unchanged from source 50. The full Rust workspace passes 1,127
tests with four existing ignored; all 39 CLI regressions, WebAssembly check,
dev builds, warnings-denied Clippy, source lint, formatting, and all seven
generators pass. Fourslash and workspace Unix-socket tests required the
previously observed sandbox escalation.

The ignored reference checkout advances to source 51; the published
TypeScript pin remains unchanged until the full upgrade. No source-51
release instruction-count or peak-RSS measurement was collected under the
user's nonblocking migration policy. The measured source-11 regressions
remain disclosed above. Evidence is under `target/port-623/0051-bea2e849e9/`.

## 52. `0ba5d992db` — preserve duplicate private declarations during decorated-class emit

Upstream `0ba5d992db957fa444c3049e51243a056d3a9276` changes two
class-field declaration transforms to resolve a private name in the current
class environment, then leave a declaration unchanged when its private-name
entry is `Untransformed` or invalid. The Rust port mirrors those two lookups
and guards. The expression-use lookup still searches enclosing classes and
intentionally hides `Untransformed` entries; using it at declaration sites
caused a panic on the upstream fixture's ES2022 variant. Before this port,
that variant crashed at “Undeclared private name for property declaration”;
the ES2015 and ESNext variants passed. Afterward, all three pass diagnostics
and JavaScript emit. A Go compiler built from this exact commit and the Rust
CLI produce byte-identical emitted JavaScript, identical 12 diagnostics, and
the same exit code for each target. The compatibility table records this
closed emit gap; the root and npm README capability summaries do not change.

All three full compiler modes have exactly the source-51 pass/nonpass sets:
no removed passes, new failures, crashes, or timeouts. Go-history checker
passes 13,540 diagnostics and 12,850 type/symbol baselines; canonical checker
passes 13,540 diagnostics and 12,849 type/symbol baselines. Emit passes
13,544 diagnostics, 12,850 type/symbol, 13,470 JavaScript, 149 JavaScript-map,
and 156 source-map baselines. Exact-source Fourslash passes 4,087 of 4,557
groups, skips 470, fails none, and all 4,621 named outcomes are unchanged.
The full Rust workspace passes 1,127 tests with four existing ignored; all
39 CLI regressions, WebAssembly check, dev builds, warnings-denied Clippy,
source lint, formatting, and all seven generators pass. Fourslash and
workspace Unix-socket tests required the previously observed sandbox
escalation.

The ignored reference checkout advances to source 52; the published
TypeScript pin remains unchanged until the full upgrade. No source-52
release instruction-count or peak-RSS measurement was collected under the
user's nonblocking migration policy. The measured source-11 regressions
remain disclosed above. Evidence is under `target/port-623/0052-0ba5d992db/`.

## 53. `0bd74f77b2` — avoid CommonJS exports for shadowed names in function-local loops

Upstream `0bd74f77b276e659df9f63c3c63377cf11d1d86a` changes the
iteration-body visitors for the ordinary CommonJS `for`, `for..in`, and
`for..of` transforms from the top-level nested visitor to the normal
visitor. The Rust port mirrors those two calls. Before the port, the new
`commonjsExportShadowedInFunctionLoop` fixture passed diagnostics, types,
and symbols but emitted three spurious `exports.mount = mount` assignments
for local variables shadowing an exported function. Afterward, the fixture
passes JavaScript emit while file-level loops still assign to exports for
hoisted `var` declarations. A Go compiler built from this exact commit and
the Rust CLI emit byte-identical JavaScript with matching diagnostics and
exit status for the fixture.

All three full compiler modes pass. The new fixture is the only pass-set
addition from source 52, including diagnostics, types, symbols, and
JavaScript; no prior pass was removed and there are no new failures,
crashes, or timeouts. Go-history checker passes 13,541 diagnostics and
12,851 type/symbol baselines; canonical checker passes 13,541 diagnostics
and 12,850 type/symbol baselines. Emit passes 13,545 diagnostics, 12,851
type/symbol, 13,471 JavaScript, 149 JavaScript-map, and 156 source-map
baselines. Exact-source Fourslash passes 4,087 of 4,557 groups, skips 470,
fails none, and all 4,621 named outcomes are unchanged. The full Rust
workspace, all 39 CLI regressions, WebAssembly check, dev builds,
warnings-denied Clippy, source lint, and formatting pass. The workspace and
Fourslash Unix-socket checks required the previously observed sandbox
escalation. The generator check ran after the commit because it compares
its output against the whole working-tree diff.

The ignored reference checkout advances to source 53; the published
TypeScript pin remains unchanged until the full upgrade. No source-53
release instruction-count or peak-RSS measurement was collected under the
user's nonblocking migration policy. The measured source-11 regressions
remain disclosed above. Evidence is under `target/port-623/0053-0bd74f77b2/`.

## 54. `9055b58852` — refresh the inferred project after project-tree loading

Upstream `9055b588528600681abb8d95b3f1f39ef1707e4a` cleans the inferred
project after configured project trees are loaded, then updates its program
when it exists. The Rust port mirrors both steps in
`ProjectCollectionBuilder::did_request_project_trees`. The exact upstream
`TestWorkspaceSymbolNewInferredProject` fails against the pre-port Rust
server: after editing `a.ts` so it no longer imports open `e.ts`,
`workspace/symbol` cannot find `e` in the new inferred project. That first
assertion stops the pre-port test before its later crash case. With the port,
the same test passes both the symbol lookup and the subsequent request after
opening `b.ts`, which exercises the configured project's fast path. The
upstream Fourslash test is run unchanged through the Go subprocess runner;
there is no Rust-only test translation or protocol change.

All three full compiler modes have exactly the source-53 pass and nonpass
sets, with no removed passes, new failures, crashes, or timeouts. Go-history
checker passes 13,541 diagnostics and 12,851 type/symbol baselines;
canonical checker passes 13,541 diagnostics and 12,850 type/symbol baselines.
Emit passes 13,545 diagnostics, 12,851 type/symbol, 13,471 JavaScript,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
passes 4,088 of 4,558 groups, skips 470, fails none, and adds only the new
upstream test to the 4,621 prior named outcomes; all prior outcomes are
unchanged. The full Rust workspace passes with four existing ignored tests;
all 39 CLI regressions, WebAssembly check, dev builds, warnings-denied
Clippy, source lint, and formatting pass. Fourslash and workspace socket
tests used the previously observed sandbox escalation.

The compatibility and language-server pages record the new Fourslash count
and closed workspace-symbol gap; the root and npm README summaries have no
matching count or capability distinction. The ignored reference checkout
advances to source 54; the published TypeScript pin remains unchanged until
the full upgrade. No source-54 release instruction-count or peak-RSS
measurement was collected under the user's nonblocking migration policy.
The measured source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0054-9055b58852/`.

## 55. `a501b11f64` — index re-exporting modules for declaration emit

Upstream `a501b11f64592a18287664ecc478a09659ad3134` adds a checker-local
index of external modules by the resolved targets of their exports, preserving
program order and including a symbol's parent module even when it is not
exported. The Rust checker already has the equivalent reverse lookup in
`modules_exporting`, backed by `ModuleExportIndex` and
`get_symbol_target_for_alias_lookup`. It considers named, aliased, star, and
`export =` entries and inserts the parent module in program order. Its
per-checker index summarizes only final export tables, checks table sizes,
and falls back to the original scan during alias resolution and for modules
not yet summarized; fresh checkers begin with empty indexes. These guards
also cover reentrancy and invalidation where the Go change uses an incomplete
index fallback. The upstream `declarationEmitAlternativeContainingModules`
fixture passed diagnostics, types, symbols, and declaration output against
the unmodified Rust implementation, matching the exact-source Go baselines.
No redundant runtime cache or refactor was added. This is an evidence-only
port with the compatibility counts updated for the new fixture.

All three full compiler modes pass. Compared with source 54, the new fixture
is the sole pass-set addition (diagnostics, types, symbols, and JavaScript/
declaration output); all prior pass and nonpass sets remain unchanged, with
no new failures, crashes, or timeouts. Go-history checker passes 13,542
diagnostic and 12,852 type/symbol baselines; canonical checker passes 13,542
diagnostic and 12,851 type/symbol baselines. Emit passes 13,546 diagnostic,
12,852 type/symbol, 13,472 JavaScript/declaration, 149 JavaScript-map, and
156 source-map baselines. Exact-source Fourslash passes 4,088 of 4,558 groups,
skips 470, fails none, and all 4,622 named outcomes match source 54. The full
Rust workspace, all 39 CLI regressions, WebAssembly check, dev builds,
warnings-denied Clippy, source lint, formatting, and generator check pass.
The Fourslash and workspace socket checks required the previously observed
sandbox escalation.

The ignored reference checkout advances to source 55; the published
TypeScript pin remains unchanged until the full upgrade. Runtime source,
Cargo inputs, and bundled assets are unchanged from source 54, so this port
does not change the Rust compiler binary's behavior or performance. No
separate source-55 release performance batch was collected under the user's
nonblocking migration policy; retired-instruction collection is unavailable
on this macOS host. The measured source-11 regressions remain disclosed
above. Evidence is under `target/port-623/0055-a501b11f64/`.

## 56. `ec47d33c23` — check computed enum names despite grammar errors

Upstream `ec47d33c23e464a17cdf2475632cba629bee8763` checks the
expression in an enum member's computed name even though the computed name
itself is a grammar error. It also removes the special case that suppressed
reference resolution inside those names. The Rust port makes the same two
checker changes. Before the port, eight of ten new enum fixtures failed
their diagnostic baselines, one differed by error codes, and four existing
`parserComputedPropertyName` variants failed the revised baselines. After
the port, all ten new enum fixtures and all 41 parser variants match their
exact-source diagnostics, type/symbol baselines, and JavaScript output.
The exact upstream Go `TestLocal` runner passes all ten new fixtures and
all five changed parser fixture files, including their diagnostic and emit
subchecks.

All three full compiler modes pass. Compared with source 55, each gains
exactly the ten new fixtures and retains every prior passing and nonpassing
outcome, with no new failures, crashes, or timeouts. Go-history checker
passes 13,552 diagnostic and 12,862 type/symbol baselines; canonical
checker passes 13,552 diagnostic and 12,861 type/symbol baselines. Emit
passes 13,556 diagnostic, 12,862 type/symbol, 13,482 JavaScript/declaration,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
passes 4,088 of 4,558 groups, skips 470, fails none, and preserves all
4,622 named source-55 outcomes unchanged. The full Rust workspace, all 39
CLI regressions, WebAssembly check, dev builds, warnings-denied Clippy,
source lint, and formatting pass. The Fourslash and workspace socket tests
required the previously observed sandbox escalation. The generator check
runs after the commit because it compares its output against the whole
working-tree diff.

The compatibility table records the new counts and enum diagnostic
behavior; neither README has a matching numerical summary to update.
The ignored reference checkout advances to source 56; the published
TypeScript pin remains unchanged until the full upgrade. No source-56
release instruction-count or peak-RSS measurement was collected under the
user's nonblocking migration policy. The measured source-11 regressions
remain disclosed above. Evidence is under
`target/port-623/0056-ec47d33c23/`.

## 57. `fed0bf2414` — attach hover documentation to its function

Upstream `fed0bf24149fb1ed36039212648bafdafc1ea10e` moves the
`getDocumentationForSymbol` docstring from above the
`documentationLocationMapper` type to the function it describes. The Rust
port makes the same comment move in `hover.rs` and updates the Go line
references for the mapper type and method. Rust groups the method in the
`LanguageService` implementation before the type alias, so their order
differs from Go; the docstring now directly precedes the corresponding Rust
function. No executable logic or public capability changes.

The exact upstream diff changes only `tsc/internal/ls/hover.go`; removing
whole-line comments gives byte-identical source before and after. The Rust
diff likewise changes only `hover.rs`, whose noncomment lines are
byte-identical. No compiler or Fourslash fixture, baseline, generated asset,
or runtime input changed. Thus the source-56 full compiler conformance sets
and exact-source Fourslash outcomes apply without rerunning those unchanged
inputs: Go-history checker 13,552 diagnostics and 12,862 type/symbol
baselines; canonical checker 13,552 diagnostics and 12,861 type/symbol
baselines; emit 13,556 diagnostics, 12,862 type/symbol, 13,482
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines;
Fourslash 4,088 passing groups of 4,558, 470 skipped, no failures, and all
4,622 named outcomes unchanged. The source-56 full pass/nonpass comparisons
had no lost passes, new failures, crashes, or timeouts. The identity hashes
and both patches are retained in `identity.json`, `upstream.patch`, and
`rust.patch` under the source-57 evidence directory.

The dev CLI, test-runner and Fourslash-server builds, 80 language-server
unit tests, all 39 CLI regressions, WebAssembly check, warnings-denied
Clippy, source lint, formatting, and generator check pass. The ignored
reference checkout advances to source 57; the published TypeScript pin
remains unchanged until the full upgrade. No source-57 release
instruction-count or peak-RSS measurement was collected under the user's
nonblocking migration policy. Because only comments changed, no runtime
performance change is expected. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0057-fed0bf2414/`.

## 58. `c0ca4744c9` — prepare the JavaScript API for beta

Upstream `c0ca4744c917430282a7de06c3e8f22d2cb87353` promotes the TypeScript 7
SDK subpaths from `unstable/*` to `sync`, `async`, `fs`, `path`, `proto`, `ast/*`, and
`vscode`. It removes deprecated snapshot and project aliases, moves
`formatNodeForInsertion` to the project language service, exposes `isRemoteNode`
and `API.debug`, strips internal declarations, and adds a declaration-checking
configuration. The tsrs package exports the matching stable subpaths and syncs
all 192 recorded SDK files, tests, generated protocol and AST sources at this
commit. The Rust project response drops the duplicate `rootFiles` and
`compilerOptions` fields; its 177-method inventory places
`formatNodeForInsertion` in the upstream protocol order. SDK, package-consumer,
and Node parity tests use the promoted names and replacement APIs. Recorded
Node goldens were refreshed against the exact source-58 Go oracle, including
its current AST kind numbers and source-map serialization.

The SDK source manifest now records source 58, and the sync tool accepts an
explicit `--source-commit` so the copied SDK can advance during the migration.
The `Cargo.toml` TypeScript pin and npm package version remain at their
published base until the final source commit. The source-58 VS Code extension
edits have no counterpart here because tsrs ships the `vscode` SDK helper,
not the upstream extension. The pre-existing tsrs async connection-loss fix
and vendored JSON-RPC adjustments remain explicit sync patches. These are the
deliberate differences from the selected SDK source.

All three full compiler modes retain the exact source-57 passing and
nonpassing sets, without new failures, crashes, or timeouts. Go-history
checker passes 13,552 diagnostic and 12,862 type/symbol baselines; canonical
checker passes 13,552 diagnostic and 12,861 type/symbol baselines. Emit passes
13,556 diagnostic, 12,862 type/symbol, 13,482 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622 named
source-57 outcomes. The SDK source suite passes all 956 tests. The packed npm
consumer completes all 25 smoke steps and the separate Node consumer passes.
The full Rust workspace, 39 CLI regressions, WebAssembly check, dev builds,
warnings-denied Clippy, source lint, formatting, SDK sync, and generated-code
checks pass. Exact-source Go API and protocol-generator tests pass. The
Fourslash and workspace socket checks used the previously observed sandbox
escalation.

In the additional Node API parity lane, both the exact source-58 Go oracle
and tsrs pass 48 of 55 tests, skip four, mark two TODO, and fail the same
async checker test on the upstream `signatureToSignatureDeclarationHelper`
panic. Rust's lone-surrogate TODO succeeds where the Go oracle reports the
known server exit. The three existing soft differences (CPU profiling and
two large-declared-payload transport cases) remain documented in the parity
summary. This lane identifies no new tsrs regression from source 58; the
upstream panic remains a known gap.

No source-58 release instruction-count, wall-time, or peak-RSS comparison was
collected under the user's nonblocking migration policy. The compiler
conformance sets are unchanged, but the SDK and API response do change; no
performance-equality claim follows. The measured source-11 regressions
remain disclosed above. Evidence is under
`target/port-623/0058-c0ca4744c9/`, with Node parity summaries under
`tools/node-api/.work/source58-oracle-parity/` and
`tools/node-api/.work/source58-parity-refreshed/`.

## 59. `6ad8c56f9b` — shared data symbols

Upstream `6ad8c56f9b5a9bb910046c56059296311adc24ba` makes symbol fields
private, routes their use through accessors, and gives instantiated symbols
the original symbol's live data (name, declarations, value declaration,
members, exports, parent, and export symbol). Flags, check flags, and identity
remain independent. The Rust symbol already had accessors and a compact
optional table tail, so the mechanical Go accessor rewrites do not require
matching Rust churn. The port switches the checker to a shared-data
constructor and converts remaining direct Rust name reads to `name()`.

Rust keeps its 32-byte compressed/40-byte plain `Symbol` instead of adopting
Go's separate `Symbol`/`SymbolWithData` types. An instantiated symbol stores
its data owner's arena key in its existing `parent_or_tables` word and a
reserved zero name word marks the shared-data representation. Nested instances
flatten to that one owner when constructed. Data accessors resolve the owner;
the marker is independent of mutable check flags, so even clearing
all of an instance's check flags cannot detach its data. Ordinary symbols
retain their prior packed name, declaration slice, and optional table tail.
The checker still counts instantiated symbols and assigns separate IDs in
seed mode; `clone_symbol` continues to copy its member and export tables.
The focused Rust unit test checks live two-way sharing, lazy table creation,
independent flags and IDs, and the check-flag overwrite case. The exact-source
Go symbol and API symbol-response tests pass. There is no new upstream
conformance fixture or user-visible capability change.

All three full compiler modes preserve the exact source-58 passing and
nonpassing sets, with no new failure, crash, or timeout. Go-history checker
passes 13,552 diagnostic and 12,862 type/symbol baselines; canonical checker
passes 13,552 diagnostic and 12,861 type/symbol baselines. Emit passes 13,556
diagnostic, 12,862 type/symbol, 13,482 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622 named
source-58 outcomes. The full Rust workspace, 39 CLI regressions, WebAssembly
check, dev builds, warnings-denied Clippy, source lint, formatting, and
generator check pass. The Fourslash and workspace socket checks used the
previously observed sandbox escalation.

The ignored reference checkout advances to source 59. The published
TypeScript pin and compatibility summaries remain unchanged; this source
changes internal storage without changing feature status or pass counts.

Paired release runs against the source-58 binary on the same Apple M3 Max
preserve diagnostics and exit codes in every public-project cell. Ten
interleaved runs per binary for xstate-main and webpack, and three for the
larger VS Code and MUI Docs projects, give these median wall-time changes:

| Project | 1 checker | 4 checkers | Largest peak-RSS change |
| --- | ---: | ---: | ---: |
| xstate-main | +2.6% | +1.2% | +0.3% |
| webpack | +1.4% | +1.4% | +0.1% |
| vscode | +2.4% | +1.8% | +0.2% |
| mui-docs | +3.9% | +0.1% | +0.1% |

The two smaller projects also pass the 16-checker poisoned-arena runs. The
one-hop data-owner lookup avoids repeated traversal of nested instances, but
the ordinary-symbol marker check still adds work: the measured wall-time
increase is disclosed under the user's nonblocking migration policy. Peak
memory is effectively unchanged because Rust already used a packed symbol.
`bench/count.mjs` cannot collect retired instructions on this macOS/arm64
host (`no perf_event_open on darwin/arm64`); the single-threaded peak-RSS
runs and diagnostic comparisons still pass. The paired logs and JSON are
under `target/port-623/0059-6ad8c56f9b/verify-optimized/` and
`verify-large/`.

## 60. `aad4c72bf2` — sync connection answers nested requests in stack order

Upstream `aad4c72bf2d22e1fddb2a07d9e6f131e9b2ade44` adds a call-depth turn
condition to `ipc.SyncConn`. A client callback can re-enter the API while an
older nested request handler is still running; concurrent server-to-client
calls may then be stacked. The older handler must wait until deeper calls
finish before writing its response, and a notification handler may itself
make a nested call. The Rust port puts `calls` and `reading` beside the
reader/writer in `SyncIo`, uses a condition variable for `lock_turn`, and
reserves the protocol reader for the deepest active call. Every request
response, including timing and recovered handler errors, waits for its turn.
An `ActiveCall` guard restores the call state and wakes waiters on success,
error, or unwind. This is the same ordering as Go despite Rust's separate
protocol reader and writer. No SDK source changes were needed: upstream
changed only the Go IPC connection. The published sync SDK uses MessagePack;
the JSON-RPC sync-connection notification path is covered at the transport
level.

The two new Rust pipe tests force a shallower handler to finish while a deeper
call is active and check that responses arrive deepest first, and verify that
a notification handler can call the client. Both pass. The exact-source Go
`TestSyncConn` cases pass. Source-mode Node API tests pass 956/956, the pinned
Node transport round trip passes 15/15, and the packed npm consumer passes
25/25 steps. All three full compiler modes have result lists byte-identical
to source 59 (18 Go-history, 18 canonical, 36 emit lists), without a removed
pass, changed known nonpass, crash, or timeout: Go-history checker passes
13,552 diagnostics and 12,862 type/symbol baselines; canonical checker
passes 13,552 diagnostics and 12,861 type/symbol baselines; emit passes
13,556 diagnostics, 12,862 type/symbol, 13,482 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622
named source-59 outcomes. The full Rust workspace, 39 CLI regressions,
WebAssembly target check, dev and release builds, warnings-denied Clippy,
source lint, formatting, and generator check pass. Fourslash and the
workspace/transport socket cases used the established sandbox escalation.
The API capability table records the new nested-call guarantee; existing
README summaries remain consistent. The ignored reference checkout advances
to source 60; the published TypeScript pin remains at the base revision.

Six alternating release-server pairs processed one million public sync
`getServerTiming` frames each. Source 59 to source 60 median instructions
retired were 21,035,871,909.5 to 21,061,756,554.5 (+0.123%); median wall
time was 1.275 to 1.275 seconds; peak RSS was 8,601,600 to 8,617,984 bytes
(+0.19%). Six alternating Node sync-client pairs of 100,000 requests each
measured 10.245 to 10.134 microseconds per request (-1.09%) at the median.
These are transport microbenchmarks, not the compiler project cohort or a
measurement of contended nested callbacks. The small instruction and RSS
increases in the framed-server run are disclosed under the user's
nonblocking migration policy; neither indicates a material throughput loss.
The scripts, raw pairs, and compiler/Fourslash outcome files are under
`target/port-623/0060-aad4c72bf2/`.

## 61. `fff11d8a7c` — update localization files

Upstream `fff11d8a7c18c6f3e23eef988406732a5a9294b2` refreshes all 13
OneLoc handbacks and their runtime gzip archives. The 26 files in
`crates/tsrs_diagnostics/loc/` match the source-61 files byte for byte.
The locale checker validates canonical CRLF JSON, diagnostic placeholders,
deterministic gzip payloads, and matching handback/runtime maps against the
2,222 current English diagnostics. The handbacks have 2,218–2,222 translated
keys each; the differences include entries now removed because their text
equals current English. Upstream also changes the handback importer to filter
all existing English-equivalent translations after processing. That rule and
the strengthened test assertions were already present in the source-32
Python port, so
this commit updates the data and source references without changing importer
behavior. Exact-source Go importer and diagnostics package tests pass.

The `--locale` gap is unchanged: the Rust compiler does not load these
catalogs and still emits English diagnostics. The compatibility table and
localization tooling documentation identify the current source-61 data; root
and npm README summaries require no change. The ignored `ts-ref` checkout
advances to source 61, while the published TypeScript pin remains at the base
revision until the full upgrade.

All three complete compiler modes have outcome lists byte-identical to source
60: 18 Go-history, 18 canonical, and 36 emit pass/nonpass lists, with no lost
passes, new failures, crashes, or timeouts. Go-history checker passes 13,552
diagnostics and 12,862 type/symbol baselines; canonical checker passes 13,552
diagnostics and 12,861 type/symbol baselines. Emit passes 13,556 diagnostics,
12,862 type/symbol, 13,482 JavaScript/declaration, 149 JavaScript-map and
156 source-map baselines. Exact-source Fourslash passes 4,088 of 4,558 groups,
skips 470, fails none, and preserves all 4,622 named source-60 outcomes.
The full Rust workspace, 39 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, diagnostics generator and
locale checks, and Python importer cases pass. Fourslash and workspace socket
checks used the established sandbox escalation after local bind denials.

No release instruction-count, wall-time, or peak-RSS comparison was collected
for source 61 under the user's nonblocking migration policy. The changed
catalogs are not read by the Rust compiler, and this port changes no Rust
runtime path, but no numerical performance-equality claim follows. The
measured source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0061-fff11d8a7c/`.

## 62. `2d8450f9b5` — add ESNext iterator methods

Upstream `2d8450f9b54a2a9688a1823a57102402d6862ef5` adds
`lib.esnext.iterator.d.ts`, references it from `lib.esnext.d.ts`, and remaps
`--lib esnext.iterator` from the ES2026 iterator library to the new file.
The Rust bundle copies both declarations byte for byte and regenerates its
embedded-file and library-name lists. The ordered option map follows the Go
option order, with `esnext.typedarrays` before `esnext.iterator`. The new
library exposes `IteratorObject.chunks`, `join`, and `includes`, plus the
`Iterator.zip` and `zipKeyed` overloads. The npm `tsconfig` and `jsconfig`
schemas are copied exactly from source 62; `UPSTREAM.json` records their
hashes and the source revision, and the full 192-file SDK sync check passes.
Go's `TestProgram` covers the expanded ESNext library list and passes at
this revision; Rust's bundled-file test checks the generated list and source
contents. Existing `--help` and `--generateTrace` gaps are unchanged, though
upstream help and trace baselines now mention the new library.

All six new variants from `iteratorChunks`, `iteratorConstructorAugmentation`,
`iteratorIncludes`, `iteratorJoin`, and `iteratorZip` pass diagnostics, type,
symbol, and JavaScript baselines. The existing `builtinIterator` and
`using`/`await using` symbol baselines changed upstream to name the new
library; they continue to pass. All 18 Go-history, 18 canonical, and 36 emit
pass/nonpass result lists retain every source-61 outcome. The only new pass
entries are the six fixtures; their JavaScript-map and source-map entries
are skips because upstream provides no such baselines. Go-history and
canonical checker diagnostics pass 13,558 baselines; Go-history type/symbol
passes 12,868, canonical 12,867. Emit passes 13,562 diagnostics, 12,868
type/symbol, 13,488 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines. Exact-source Fourslash passes 4,088 of 4,558 groups,
skips 470, fails none, and preserves all 4,622 named source-61 outcomes.

The full Rust workspace, 39 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, generated-code freshness,
SDK sync check, and exact-source Go program test pass. Fourslash and the
workspace transport socket test used the established sandbox escalation
after local Unix-socket binds were denied. The compatibility table and
README summaries record the new library; the ignored `ts-ref` checkout
advances to source 62, while the published TypeScript pin stays at the base
revision until the full upgrade.

No release instruction-count, wall-time, or peak-RSS comparison was collected
for source 62 under the user's nonblocking migration policy. The new
`esnext` library can affect projects that select it, so no numerical
performance-equality claim follows. The measured source-11 regressions
remain disclosed above. Evidence is under `target/port-623/0062-2d8450f9b5/`.

## 63. `aa8149273b` — Dense link stores

Upstream `aa8149273b23401f0a79a5f0384c42de51888693` gives each checker
node and symbol link store a generator that reserves consecutive 256-ID
blocks from process-wide counters. These IDs occupy the range starting at
`0x1_0000_0000_0000`. Its paged store allocates values only for requested
slots, and an untouched slot has no links. Rust already packed checker links
densely in 32-ID inline node groups and 128-ID indirect symbol groups. The
earlier measured dense-table work favored these packed layouts over larger
page-granular approaches; this port keeps them. It routes reserved and
ordinary IDs to separate dense tables, preserving stable links and preventing
numeric collisions. The generic core `PagedLinkStore`, currently without
production consumers, now allocates per slot and distinguishes an untouched
slot from one with a value; it retains its sparse high-key fallback. As in Go,
reserved IDs beyond 16 million use a hash-backed fallback rather than grow a
dense index for a long-lived process. A node group may still report a
default-valued link for another slot in the same group. Its only checker use
reads `resolved_symbol`, whose default is `None`,
so that distinction does not change observable behavior.

Node and symbol AST objects remain compact rather than growing their
ID field to Go's 64 bits. Bit 31 of the stored 32-bit ID tags reserved IDs;
accessors reconstruct the full Go-visible number. Both ordinary and reserved
ID counters are bounded at 2^31 per kind in this representation, far above
the measured workload range. Reserved IDs matter to diagnostics: internal
well-known-symbol property names include the symbol number, and the node
builder counts their digits while truncating type text. In a public 66-case
name-length probe, 37 diagnostics differed before the ID change and zero
afterward. `unique-symbol-link-id-truncation` fixes the 35-character case as
an exact-Go CLI regression. Focused AST, core store, and checker store tests
cover ID decoding, untouched slots, stable links, and ordinary/reserved
separation.

All 18 Go-history, 18 canonical, and 36 emit pass/nonpass lists are byte
identical to source 62: no lost passes, new failures, crashes, or timeouts.
Go-history and canonical checker diagnostics pass 13,558 baselines;
Go-history type/symbol passes 12,868, canonical 12,867. Emit passes 13,562
diagnostics, 12,868 type/symbol, 13,488 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622 named
source-62 outcomes. The ignored `ts-ref` checkout advances to source 63;
the published TypeScript pin remains unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation after local Unix-socket binds were denied. On the same
macOS host, four alternating source-62/source-63 release pairs per public
project and mode measured these median wall time / peak RSS results:

| Project | Mode | Wall source 62 → 63 | RSS source 62 → 63 |
| --- | --- | ---: | ---: |
| vscode | default | 0.973 → 0.984 s (+1.1%) | 2123.1 → 2111.5 MiB (-0.6%) |
| vscode | single-threaded | 7.544 → 7.604 s (+0.8%) | 1612.0 → 1612.8 MiB (+0.0%) |
| xstate-main | default | 0.129 → 0.127 s (-1.6%) | 259.0 → 258.0 MiB (-0.4%) |
| xstate-main | single-threaded | 0.509 → 0.519 s (+1.9%) | 183.1 → 183.2 MiB (+0.0%) |
| t3code-server | default | 1.358 → 1.334 s (-1.7%) | 2094.7 → 2090.9 MiB (-0.2%) |
| t3code-server | single-threaded | 3.318 → 3.218 s (-3.0%) | 735.8 → 736.9 MiB (+0.2%) |

The changes are within run-to-run noise. Deterministic retired-instruction
counts are unavailable on this macOS host, so no instruction comparison is
claimed. The release commands used `bench/measure.mjs` with `-p <project>
--noEmit --incremental false --pretty false`, plus `--singleThreaded` for
single-threaded rows. Evidence is under `target/port-623/0063-aa8149273b/`.

## 64. `95c9d99127` — Preserve decorated class names with private self-references

Upstream `95c9d99127230f898e8edc0c75079ce9d3ad1ea9` changes the class-field
transformer to compute `hasTransformableStatics` for decorated class declarations
too, then enter their pending-statement path only when statics or pending
expressions require it. Rust mirrors that branch structure. A decorated class
with a private self-reference can reserve a constructor temporary, but without
transformable statics or pending expressions the class expression must still be
assigned directly to its named variable. Before the port, Rust emitted
`var C = _a = class` where Go emitted `var C = class`, giving the anonymous
class the wrong inferred name. The same branch also moved computed-field
initialization unnecessarily. The upstream fixtures cover private fields,
methods, accessors, and computed fields across ES2015, ES2022, and ESNext.
Before the Rust edit, the six focused variants had two JavaScript passes,
three failures, and one ES5 skip. Afterward all five emitted variants pass;
the five updated upstream JavaScript baselines match exactly.

All 18 Go-history, 18 canonical, and 36 emit pass/nonpass lists retain every
source-63 passing outcome, with no new failure, crash, or timeout. The two new
computed-field variants add two diagnostics and JavaScript passes; they skip
type, symbol, JavaScript-map, and source-map baselines because upstream provides
none. Go-history and canonical checker diagnostics pass 13,560 baselines;
Go-history type/symbol passes 12,868 and canonical 12,867. Emit passes 13,564
diagnostics, 12,868 type/symbol, 13,490 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622 named
source-63 outcomes. The ignored `ts-ref` checkout advances to source 64; the
published TypeScript pin remains unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation after local Unix-socket binds were denied. The compatibility
table records the new emit evidence. The root and npm README compatibility
summaries need no change because the existing JavaScript-emit capability stays
supported.

No release instruction-count, wall-time, or peak-RSS comparison was collected
for source 64 under the user's nonblocking migration policy; no numerical
performance-equality claim follows. The measured source-11 regressions remain
disclosed above. Evidence is under `target/port-623/0064-95c9d99127/`.

## 65. `5b3a367c29` — Preserve JSDoc modifiers on constructor-assigned properties

Upstream `5b3a367c292a1a119384969b670279f991ec5962` changes the declaration
transformer to call `ensureModifierFlags` for JavaScript `this` property
assignments, OR in `static` when needed, and synthesize modifiers from the
resulting flags. Rust mirrors that exact branch with its existing helper. Before
the edit, the new `jsDeclarationEmitThisPropertyJSDocModifiers` fixture and the
updated `jsdocAccessibilityTagsDeclarations` and `jsdocReadonlyDeclarations`
baselines all failed declaration emit: constructor-assigned properties lacked
`private`, `protected`, or `readonly`; the static property retained only
`static`. The new fixture also acquired two TS7008 errors when its generated
declaration was checked without `private`. After the edit, all three `.js`
baselines match the exact upstream source, including `private static`; their
diagnostic, type, and symbol outcomes remain passing or intentionally skipped.

All 18 Go-history, 18 canonical, and 36 emit pass/nonpass lists retain every
source-64 passing outcome, with no new failure, crash, or timeout. The new
fixture adds one diagnostic pass in each mode, one JavaScript/declaration pass
in emit, and the expected type, symbol, and map skips. Go-history and
canonical checker diagnostics pass 13,561 baselines; Go-history type/symbol
passes 12,868 and canonical 12,867. Emit passes 13,565 diagnostics, 12,868
type/symbol, 13,491 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines. Exact-source Fourslash passes 4,088 of 4,558 groups,
skips 470, fails none, and preserves all 4,622 named source-64 outcomes. The
ignored `ts-ref` checkout advances to source 65; the published TypeScript pin
remains unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation after local Unix-socket binds were denied. The compatibility
table records the new pass counts and declaration behavior. The root and npm
README compatibility summaries need no change because declaration emit remains
supported. No release instruction-count, wall-time, or peak-RSS comparison was
collected for source 65 under the user's nonblocking migration policy; no
numerical performance-equality claim follows. The measured source-11
regressions remain disclosed above. Evidence is under
`target/port-623/0065-5b3a367c29/`.

## 66. `71956e9735` — Check package reachability before using exports in declarations

Upstream `71956e9735e923befa06b9477a3eeadbf762cb1d` moves the
`node_modules` search-root and global typings-cache reachability checks ahead of
`package.json` exports lookup in `tryGetModuleNameAsNodeModule`. Rust now makes
the same checks before an exported bare module specifier can return. Its
component-aware path check also handles root directories and avoids treating a
sibling directory with the same textual prefix as reachable. Before the edit,
both `declarationEmitUnreachablePackageExports` variants (NodeNext and Preserve)
failed diagnostics and declaration output: `/packages/a/index.d.ts` contained
unresolvable `c-root`, `c-subpath/Thing`, and `c-pattern/Thing` imports. After
the edit, both variants match the exact upstream reference baselines: the
unreachable declaration is withheld with three TS2883 diagnostics, while the
reachable import from `/packages/s` still emits bare package names.

All 18 Go-history, 18 canonical, and 36 emit outcome lists retain every
source-65 passing outcome; there are no new failures, crashes, or timeouts. The
two new variants add two diagnostic passes in each mode, two
JavaScript/declaration passes in emit, and the expected type, symbol, and map
skips. Go-history and canonical checker diagnostics pass 13,563 baselines;
Go-history type/symbol passes 12,868 and canonical 12,867. Emit passes 13,567
diagnostics, 12,868 type/symbol, 13,493 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622 named
source-65 outcomes. The ignored `ts-ref` checkout advances to source 66; the
published TypeScript pin remains unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation after local Unix-socket binds were denied. The compatibility
table records the two new passes and package reachability behavior; the root
and npm README compatibility summaries need no change because declaration emit
remains supported. No release instruction-count, wall-time, or peak-RSS
comparison was collected for source 66 under the user's nonblocking migration
policy; no numerical performance-equality claim follows. The measured
source-11 regressions remain disclosed above. Evidence is under
`target/port-623/0066-71956e9735/`.

## 67. `bdc93bd987` — Keep function receivers in downleveled static blocks

Upstream `bdc93bd98765f15016f0e36632863b7d9fea8619` passes the active
visitor into `visitPropertyAccessExpressionForSubstitution`. Rust now does the
same: the class-alias-only substitution visitor traverses property-access
receivers in subtrees that need no class-field or lexical-`this` transform,
while ordinary property accesses continue to use the full class-fields
visitor. Before the edit, `classStaticBlock29(target=es2021)` emitted
`_a.bar()` inside two regular functions nested in a downleveled static block,
where Go emitted `this.bar()`. Afterward, both the ES2021 and ES2022 variants
match exact-source diagnostics, types, symbols, and JavaScript output. The
ES2021 variant changed from one JavaScript failure to a pass; ES2022 remained
passing.

All 18 Go-history, 18 canonical, and 36 emit outcome lists retain every
source-66 passing outcome, with no new failures, crashes, or timeouts. The two
new fixture variants add two diagnostics, type, symbol, and JavaScript passes
in the corresponding modes; their JavaScript-map and source-map baselines are
intentionally absent. Go-history and canonical checker diagnostics pass 13,565
of 13,569 baselines; Go-history type/symbol passes 12,870 and canonical 12,869.
Emit passes 13,569 diagnostics, 12,870 type/symbol, 13,495
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
Exact-source Fourslash passes 4,088 of 4,558 groups, skips 470, fails none,
and preserves all 4,622 named source-66 outcomes. The ignored `ts-ref`
checkout advances to source 67; the published TypeScript pin remains
unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation after local Unix-socket binds were denied. The
compatibility table records the two new passes and the corrected JavaScript
emit behavior; the root and npm README compatibility summaries need no change
because JavaScript emit remains supported. No release instruction-count,
wall-time, or peak-RSS comparison was collected for source 67 under the user's
nonblocking migration policy; no numerical performance-equality claim
follows. The measured source-11 regressions remain disclosed above. Evidence
is under `target/port-623/0067-bdc93bd987/`.

## 68. `96fffa9f65` — Keep function-check state outside value-symbol links

Upstream `96fffa9f650a7e75bf1448cf641d97e563a086cd` removes
`functionOrConstructorChecked` from Go's `ValueSymbolLinks` (56 → 48 bytes)
and records checked symbols in a checker-owned set. Rust already packs the
common value-symbol link into 16 bytes with compressed pointers, so there
was no inline 16-byte reduction to repeat. The previous Rust Boolean forced
an allocation of `ValueSymbolLinksTail` when checking a function or
constructor whose other rare link fields were absent. Rust now uses
`constructor_or_function_checked.add_if_absent(symbol)`, matching Go's
once-only check. The set is copied with a checker fork. Compile-time size
assertions hold the compressed inline record at 16 bytes and the rare-field
tail at 20 bytes (24 and 40 bytes, respectively, with plain 64-bit pointers).
The focused `overloadsInDifferentContainersDisagreeOnAmbient` diagnostic,
type, and symbol baselines pass; the full suites below cover other overload
and constructor cases. The comment on synthetic links was generalized to
remove stale implementation-specific workload counts.

All 18 Go-history, 18 canonical, and 36 emit outcome lists are identical to
source 67: no lost or added passes, failures, crashes, or timeouts. Go-history
and canonical checker diagnostics pass 13,565 of 13,569 baselines;
Go-history type/symbol passes 12,870 and canonical 12,869. Emit passes
13,569 diagnostics, 12,870 type/symbol, 13,495 JavaScript/declaration,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
passes 4,088 of 4,558 groups, skips 470, fails none, and preserves all
4,622 named source-67 outcomes. The ignored `ts-ref` checkout advances to
source 68; the published TypeScript pin remains unchanged until the full
upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test needed the established
sandbox escalation for local Unix-socket binds. The compatibility table and
README summaries do not change: this is a representation change without a
capability or pass-count change.

Paired release measurements used five alternating source-67/source-68 pairs
on each public project and mode, `-p <project> --noEmit --incremental false
--pretty false`. Instructions are macOS `time -l` retired counts (including
kernel work); wall time surrounds the compiler process; peak RSS is the same
`time -l` resident-set measure. The medians below are before → after. Raw
samples, stdout hashes, and binary hashes are in
`target/port-623/0068-96fffa9f65/public-performance.json`.

| Public project | Mode | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.274 → 3.273 (−0.022%) | 0.263 → 0.261 (−0.846%) | 59.8 → 59.6 (−0.340%) |
| Compiler-Unions | default | 3.962 → 3.961 (−0.025%) | 0.147 → 0.147 (−0.368%) | 83.5 → 83.6 (+0.056%) |
| webpack | single | 10.828 → 10.826 (−0.018%) | 0.911 → 0.914 (+0.338%) | 311.3 → 310.7 (−0.191%) |
| webpack | default | 15.234 → 15.192 (−0.276%) | 0.203 → 0.210 (+3.269%) | 504.7 → 501.9 (−0.554%) |
| vscode | single | 81.349 → 81.353 (+0.005%) | 7.706 → 7.708 (+0.031%) | 1609.8 → 1606.6 (−0.196%) |
| vscode | default | 93.437 → 93.385 (−0.055%) | 1.027 → 1.016 (−1.041%) | 2115.9 → 2111.5 (−0.208%) |

Peak RSS medians decline in five of six cells, with a +0.056% change on
Compiler-Unions/default. Wall and instruction medians are mixed. The
webpack/default +3.269% wall result came from five positive pairs; an
independent ten-pair reverse-order confirmation against the same binaries
measured 0.207 → 0.206 seconds (−0.404% median wall), 15.247 → 15.183
billion instructions (−0.416%), and 506.1 → 503.0 MiB peak RSS
(−0.617%), so the wall increase did not reproduce. Its raw samples are in
`target/port-623/0068-96fffa9f65/webpack-default-confirm.json`.
The small observed increases are disclosed rather than treated as porting
blockers under the user's nonblocking migration policy. These local pairs
are not the repository's two-publish Linux performance gate. The earlier
measured source-11 regressions remain disclosed above. All source-68
evidence is under `target/port-623/0068-96fffa9f65/`.

## 69. `fd871fcc3c` — Preserve replacement library order during sorting

Upstream `fd871fcc3c1f30e1d5f39ed4bae9ff49ad57ef50` makes `sortLibs`
stable because replacement libraries share the fallback priority and must
retain their discovery order. Rust already used the stable `sort_by_key` in
`sort_libs`; this port documents the tie-order invariant at that call and
advances the ignored `ts-ref` checkout to the exact upstream commit. The new
`libReplacementOrder` fixture passes. On its materialized project, exact-source
Go and Rust `--listFiles` outputs contain the same 14 replacement libraries in
the same ES2015-through-script-host order. A temporary Rust unstable-sort
probe also passed this fixture, so that fixture alone does not distinguish
Rust's two sorting methods on this input; the stable method remains the
required translation of Go's new `SortStableFunc`.

All 18 Go-history, 18 canonical, and 36 emit outcome lists retain every
source-68 passing outcome, with no new failures, crashes, or timeouts. The new
fixture adds one diagnostics pass in each checker history and one diagnostics
and JavaScript pass in emit; its type, symbol, JavaScript-map, and source-map
baselines are intentionally absent. Go-history and canonical checker
diagnostics pass 13,566 of 13,570 baselines; Go-history type/symbol passes
12,870 and canonical 12,869. Emit passes 13,570 diagnostics, 12,870
type/symbol, 13,496 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines. Exact-source Fourslash passes 4,088 of 4,558 groups,
skips 470, fails none, and preserves all 4,622 named source-68 outcomes. The
published TypeScript pin remains unchanged until the full upgrade.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. Fourslash and the workspace transport socket test used the established
sandbox escalation for local Unix-socket binds. An unrelated emit case timed
out once with the default limit and passed on the complete rerun at 120
seconds; no final outcome list contains a timeout. The compatibility table
records the new diagnostics pass and replacement-library ordering; the README
compatibility summaries have no count to change. No release instruction-count,
wall-time, or peak-RSS comparison was collected for source 69 under the user's
nonblocking migration policy; no numerical performance-equality claim follows.
The earlier measured source-11 regressions remain disclosed above. Evidence
is under `target/port-623/0069-fd871fcc3c/`.

## 70. `d11c071957` — Fix decorator library examples

Upstream `d11c0719574ac9124d58089cabea61bc346de3be` corrects two invalid
`@example` blocks in `lib.decorators.d.ts`. The bundled Rust library now matches
the exact upstream file byte for byte. Removing block comments from the old and
new copies leaves identical bytes: no declaration signatures or executable
TypeScript changed. Both revised examples, extracted from the library, compile
without diagnostics with the exact-source Go compiler and tsrs. The ignored
`ts-ref` checkout advances to source 70; the published TypeScript pin remains
unchanged until the full upgrade.

All 18 Go-history, 18 canonical, and 36 emit outcome lists are identical to
source 69, with no lost passes, new crashes, or timeouts. Go-history and
canonical checker diagnostics pass 13,566 of 13,570 baselines; Go-history
type/symbol passes 12,870 and canonical passes 12,869. Emit passes 13,570
diagnostics, 12,870 type/symbol, 13,496 JavaScript/declaration, 149 JavaScript
map, and 156 source-map baselines. Exact-source Fourslash passes 4,088 of
4,558 groups, skips 470, fails none, and preserves all 4,622 named source-69
outcomes.

The language-server `refs_smoke2` fixture initially held five old locations in
the bundled decorators library. Regenerating its expected output with the
source-70 Go language server changed only those five line positions in one of
39 responses: one by three lines and four by seven lines, matching the added
example lines. The full Rust workspace then passes, as do 40 CLI regressions,
the WebAssembly target check, warnings-denied Clippy, source lint, formatting,
and generated-code freshness. The Fourslash and workspace transport checks
needed the established sandbox escalation for local Unix-socket binds.
Neither the compatibility table nor README summaries change: the updated
examples introduce no capability or pass-count change.

Paired release measurements use the source-68 binary as the preceding code
equivalent, because source 69 changed only a Rust comment and documentation.
Five alternating public-project pairs per cell ran `-p <project> --noEmit
--incremental false --pretty false`. Instructions are macOS `time -l` retired
counts, wall time surrounds the compiler process, and peak RSS is the same
`time -l` resident-set measure. Medians are before → after:

| Public project | Mode | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.273 → 3.273 (−0.001%) | 0.241 → 0.242 (+0.279%) | 59.5 → 59.5 (+0.000%) |
| Compiler-Unions | default | 3.963 → 3.964 (+0.024%) | 0.131 → 0.132 (+0.597%) | 83.5 → 83.2 (−0.300%) |
| webpack | single | 10.834 → 10.833 (−0.009%) | 0.893 → 0.938 (+4.971%) | 310.7 → 310.7 (+0.010%) |
| webpack | default | 15.216 → 15.227 (+0.073%) | 0.186 → 0.188 (+0.826%) | 502.9 → 502.6 (−0.071%) |
| vscode | single | 82.231 → 82.397 (+0.202%) | 7.809 → 8.205 (+5.078%) | 1606.6 → 1606.7 (+0.002%) |
| vscode | default | 93.519 → 93.507 (−0.013%) | 1.005 → 1.059 (+5.445%) | 2114.8 → 2122.5 (+0.367%) |

The five-pair screen showed mixed instructions and RSS, with +4.971%,
+5.078%, and +5.445% wall medians on webpack/single, vscode/single, and
vscode/default. All five paired wall comparisons were positive in those
three cells, so one reversed-order ten-pair confirmation batch was run for
each. Its wall medians were +0.212%, −0.162%, and −0.916%, respectively;
the screened multi-percent wall increases did not reproduce. Confirmation
instruction medians were −0.010%, −0.013%, and +0.088%, respectively.
Single-threaded peak RSS remained 32 KiB higher (+0.010% webpack,
+0.002% vscode), while vscode/default was −0.048% in confirmation.
No further performance sampling was done for this source. These local results
are not the repository's two-publish Linux performance gate.
The user's nonblocking migration policy permits this correctness-valid port
while measured changes remain disclosed. Raw samples, stdout and binary
hashes, and all validation evidence are under
`target/port-623/0070-d11c071957/`.

## 71. `ba1819f3f9` — Add `Intl.PluralRules.selectRange` typing

Upstream `ba1819f3f98b74662aaca9ba67f6b0c10edd67da` adds the
`selectRange(start: number, end: number): LDMLPluralRule` member to the
`Intl.PluralRules` interface in `lib.es2023.intl.d.ts` and adds a
conformance fixture. The bundled Rust library is byte-identical to the
exact-source Go declaration. The new fixture passes its diagnostic, type,
symbol, and JavaScript baselines. Its materialized project has identical
Go and Rust exit status, diagnostics, and emitted JavaScript hash. The
ignored `ts-ref` checkout advances to source 71; the published TypeScript
pin remains unchanged until the full upgrade.

All 18 Go-history, 18 canonical, and 36 emit outcome lists preserve every
source-70 outcome: no lost passes, new failures, crashes, or timeouts. The
new fixture adds one diagnostics, type, and symbol pass in each checker
history, and those plus one JavaScript pass in emit; its absent map baselines
are skipped. Go-history diagnostics pass 13,567 of 13,571 baselines and
type/symbol baselines pass 12,871; canonical diagnostics pass 13,567 and
type/symbol pass 12,870 because its pre-existing
`objectLiteralNormalization` difference remains. Emit passes 13,571
diagnostics, 12,871 type/symbol, 13,497 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. Exact-source Fourslash passes
4,088 of 4,558 groups, skips 470, fails none, and preserves all 4,622
named source-70 outcomes.

The full Rust workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, formatting, and generated-code freshness
pass. The Fourslash and workspace transport tests needed the established
sandbox escalation for local Unix-socket binds. The compatibility table records the new standard-library
member and measured pass counts; the root and npm README summaries contain
no counts to change.

Paired release measurements compared the source-70 and source-71 binaries
on three public projects in single-threaded and default modes, with five
alternating pairs per cell. Each ran `-p <project> --noEmit --incremental
false --pretty false`. Instructions are macOS `time -l` retired counts,
including kernel work; wall time surrounds the compiler process; peak RSS
is the same `time -l` resident-set measure. Medians are before → after:

| Public project | Mode | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.273 → 3.273 (+0.001%) | 0.267 → 0.259 (−2.671%) | 59.5 → 59.5 (+0.000%) |
| Compiler-Unions | default | 3.964 → 3.967 (+0.076%) | 0.148 → 0.149 (+0.737%) | 83.2 → 83.3 (+0.131%) |
| webpack | single | 10.866 → 10.871 (+0.050%) | 0.917 → 0.911 (−0.623%) | 310.7 → 310.7 (−0.005%) |
| webpack | default | 15.084 → 15.256 (+1.138%) | 0.204 → 0.204 (+0.135%) | 499.2 → 503.4 (+0.845%) |
| vscode | single | 81.337 → 81.360 (+0.029%) | 7.669 → 7.739 (+0.923%) | 1606.7 → 1606.7 (+0.000%) |
| vscode | default | 93.568 → 93.428 (−0.150%) | 1.033 → 1.025 (−0.780%) | 2100.7 → 2118.7 (+0.856%) |

The five-pair webpack/default instruction median was +1.138%, despite
mixed pairwise signs and only +0.135% wall time. An independent ten-pair
reversed-order confirmation against the same binaries measured +0.357%
instructions, −0.016% wall, and +0.266% peak RSS; the >1% instruction
increase did not reproduce. Single-thread instruction medians change by
at most +0.050%, wall has no increase above +0.923%, and default-mode RSS
increases remain under 1% in the screen. These local measurements are not
the repository's two-publish Linux performance gate. The user's
nonblocking migration policy permits the correctness-valid port while
all observed differences remain disclosed. The earlier measured source-11
regressions remain disclosed above. Raw samples, stdout and binary hashes,
and all validation evidence are under `target/port-623/0071-ba1819f3f9/`.

## 72. `b5c34ba0c6` — Restore symbol names in JSX import action titles

Upstream `b5c34ba0c61a756c8abaf133236dddf982c27f45` makes an import
code action name the required symbol when it differs from the identifier in
the diagnostic. For example, a JSX `<Text>` tag needing the React namespace
now offers `Import 'React' from "react"`; a normal `Text` import keeps the
generic `Add import from "react-native"` title. The Rust port adds the Go
`errorIdentifierText` field and the `includeSymbolNameInDescription` argument
to auto-import edits. Completion details continue to request generic titles.
The Go UMD path's empty text for a nonidentifier is preserved independently
of the JSX sorting flag. No other semantic deviation was needed. The ignored
`ts-ref` checkout advances to source 72; the published TypeScript pin stays
unchanged until the full upgrade.

All three exact-source upstream Fourslash cases pass: previously skipped
`jsx4` and `jsx6` plus new `jsx8`, which adds the React default import beside
an existing named import. Full exact-source Fourslash passes 4,091 of 4,559
groups, skips 468, fails none; across 4,623 named outcomes there are no lost
passes, one new passing case, and two skip-to-pass changes. All 18
Go-history, 18 canonical, and 36 emit compiler outcome lists are byte-identical
to source 71, including pass sets, failures, crashes, and timeouts. The
Go-history checker retains 13,567 diagnostic and 12,871 type/symbol passes;
canonical retains 13,567 and 12,870; emit retains 13,571 diagnostics, 12,871
type/symbols, 13,497 JavaScript/declaration, 149 JavaScript-map, and 156
source-map passes. The compatibility table's cited Fourslash count is updated;
the root and npm README summaries cite no count to change.

The full Rust workspace, 40 CLI regressions, warnings-denied Clippy, source
lint, formatting, and the WebAssembly crate check pass. Exact-source
Fourslash and the workspace transport tests needed the established sandbox
escalation for local Unix-socket binds. The full WebAssembly packaging script
could not complete because `wasm-opt` is unavailable locally; its Rust release
build passed before that external tool was invoked. Generated-code freshness
was checked after committing, because that script compares the working tree
with HEAD. This is a language-service code-action title change with no
compiler hot-path change. Release performance was not measured, and no
numerical performance-equality claim follows; the user's nonblocking
migration policy applies. Logs and complete outcome comparisons are under
`target/port-623/0072-b5c34ba0c6/`.

## 73. `a0a0ff55ca` — Simplify generic remapped `keyof` types

Upstream `a0a0ff55cab4a1f81b7123ab219c97a2f1ed703d` resolves a generic
mapped type's `keyof` when its keys have a name type and its declaration does
not have a homomorphic `keyof` constraint. The Rust port adds
`get_simplified_index_type` under the existing `TypeFlags::Index` simplification
path and removes the old special remapping branch from indexed-access index
validation. The index type is obtained inside the per-union-member check, as
in Go. No additional semantic deviation was needed. The ignored `ts-ref`
checkout advances to this source commit; the published TypeScript pin remains
at the base revision until the full upgrade.

Both changed upstream fixtures pass diagnostics, type/symbol, and JavaScript
baselines. The new recursive indexed-access case and the added bare and
template-literal `keyof` examples exercise the simplification. In the full
suite, Go-history checking passes 13,568 diagnostic and 12,872 type/symbol
baselines; canonical checking passes 13,568 and 12,871 respectively. Emit
passes 13,572 diagnostics, 12,872 type/symbols, 13,498 JavaScript/declaration
baselines, 149 JavaScript maps, and 156 source maps. All 18 Go-history, 18
canonical, and 36 emit outcome lists preserve every source-72 result; the
only additions are the new compiler fixture's pass results and its expected
map skips. There are no new failures, crashes, or timeouts. Complete list
comparisons are in `target/port-623/0073-a0a0ff55ca/`.

Exact-source Fourslash passes 4,091 of 4,559 groups, skips 468, and fails
none; all 4,623 named outcomes match source 72. The full Rust workspace,
40 CLI regressions, WebAssembly target check, warnings-denied Clippy, source
lint, and formatting pass. Fourslash and workspace transport tests required
the established sandbox escalation for local Unix-socket binds. A concurrent
workspace run hit the macOS FSEvents load stress assertion; the isolated
stress case and an isolated full workspace rerun both passed. Generated-code
freshness is checked after the corresponding commit, because that script
compares the worktree with HEAD.

Paired release measurements compared source 73 with the source-72 compiler
binary (byte-identical to source 71) on three public projects, with five
alternating pairs per cell. Each invocation used `-p <project> --noEmit
--incremental false --pretty false`; single-threaded cells also used
`--singleThreaded` and `RAYON_NUM_THREADS=1`. Instructions are macOS
`/usr/bin/time -l` retired counts including kernel work; wall time surrounds
the compiler process; peak RSS is the same `time -l` resident-set measure.
Every before/after pair had the same stdout and exit status. Medians are
before → after:

| Public project | Mode | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.273 → 3.273 (−0.007%) | 0.225 → 0.227 (+0.864%) | 59.5 → 59.6 (+0.079%) |
| Compiler-Unions | default | 3.962 → 3.967 (+0.126%) | 0.106 → 0.107 (+1.185%) | 83.2 → 83.4 (+0.150%) |
| webpack | single | 10.831 → 10.831 (−0.001%) | 0.891 → 0.890 (−0.120%) | 310.7 → 310.8 (+0.020%) |
| webpack | default | 15.094 → 15.236 (+0.940%) | 0.185 → 0.188 (+1.802%) | 499.1 → 503.6 (+0.895%) |
| vscode | single | 81.405 → 81.394 (−0.013%) | 7.698 → 7.726 (+0.366%) | 1606.7 → 1606.8 (+0.008%) |
| vscode | default | 93.541 → 93.591 (+0.053%) | 1.037 → 1.047 (+0.962%) | 2109.0 → 2117.1 (+0.385%) |

The five-pair webpack/default screen showed +0.940% instructions and
+1.802% wall. A separate ten-pair reversed-order confirmation against the
same binaries measured 15.140 → 15.146 billion instructions (+0.042%),
0.167 → 0.168 seconds (+0.627%) wall, and 501.1 → 500.1 MiB (−0.203%)
peak RSS; the screen's larger increase did not reproduce. No threshold-level
regression was confirmed in these local macOS runs, which are not the
repository's two-publish Linux performance gate. The user's nonblocking
migration policy applies. Raw samples, hashes, binaries, and the JavaScript
collector are under `target/port-623/0073-a0a0ff55ca/perf/`.

## 74. `0a66074847` — Fix `stripInternal` type-alias declaration emit

Upstream `0a660748471910ecac1ae6e1625529908dc228f2` visits a type
alias's right-hand side before updating its declaration and substitutes an
`any` keyword node when `--stripInternal` elides that right-hand side. The
Rust transform follows the same order and fallback. No semantic deviation
was needed. The new exact upstream fixture emits `export type T = any;`
instead of aborting; its Go `TestLocal` case and Rust diagnostics,
type/symbol, and JavaScript/declaration baselines all pass. The ignored
`ts-ref` checkout advances to this source commit, while the published
TypeScript pin remains at the base revision until the complete upgrade.

Full Go-history checking passes 13,569 diagnostic and 12,873 type/symbol
baselines; canonical checking passes 13,569 and 12,872 respectively. Emit
passes 13,573 diagnostics, 12,873 type/symbols, 13,499
JavaScript/declaration baselines, 149 JavaScript maps, and 156 source maps.
All 18 Go-history, 18 canonical, and 36 emit outcome lists retain every
source-73 result. Their only additions are the new fixture's pass entries
and expected map skips; there are no new failures, crashes, or timeouts.
Exact-source Fourslash passes 4,091 of 4,559 groups, skips 468, and fails
none; all 4,623 named outcomes match source 73. The full Rust workspace,
40 CLI regressions, WebAssembly target check, warnings-denied Clippy,
source lint, and formatting pass. Fourslash and workspace transport tests
required the established sandbox escalation for local Unix-socket binds.
Generated-code freshness is checked after the corresponding commit because
that script compares the worktree with HEAD. Complete evidence is under
`target/port-623/0074-0a66074847/`.

Five alternating paired release runs per cell compared source 73 with
source 74 on the public Compiler-Unions corpus, which enables declaration
emit and `stripInternal`. Each run used fresh output directories, an
explicit `--rootDir`, and a write-deny sandbox over the corpus. Both
binaries exited with the same status and produced identical stdout and all
105 emitted files. macOS `/usr/bin/time -l` provided retired instructions
and peak RSS, including the sandbox's work; wall time surrounded the whole
process. Medians are before → after:

| Mode | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | ---: | ---: | ---: |
| Single checker | 5.71167 → 5.71165 (−0.0004%) | 0.4135 → 0.4128 (−0.17%) | 84.41 → 84.41 (0.00%) |
| Default checkers | 6.43796 → 6.43455 (−0.053%) | 0.1863 → 0.1851 (−0.66%) | 134.34 → 134.94 (+0.44%) |

No threshold-level performance regression was observed in these local
macOS runs; they do not replace the repository's two-publish Linux gate.
The user's nonblocking migration policy applies. Raw samples, output
hashes, binaries, and the JavaScript collector are under
`target/port-623/0074-0a66074847/perf/`.

## 75. `b82cd139dc` — Restore dependency-depth build scheduling

Upstream `b82cd139dcdae5024a08e2aaf4f0847112d4a023` makes builders
take projects in stable dependency-depth order, including when building one
selected project's graph or only its references. It keeps reporting in the
original selected build order. Rust already computed and cached the full
schedule but still dispatched the original order; this port passes the
selected order into the same depth sort and dispatches that schedule. The
Rust graph regression checks full, selected, and references-only schedules,
one-builder dispatch, and unchanged reporting order. Upstream Go
`TestBuildScheduling` passes. A five-project CLI fixture also produces
identical Go and Rust verbose build logs after replacing wall-clock
timestamps. No semantic deviation from this upstream change was needed.
The API's existing one-builder implementation remains sequential. The
ignored `ts-ref` checkout advances to source 75; the published TypeScript
pin stays at the base revision until the full upgrade.

All 18 Go-history, 18 canonical, and 36 emit outcome lists are byte-for-byte
unchanged from source 74. Full Go-history checking passes 13,569 diagnostic
and 12,873 type/symbol baselines; canonical checking passes 13,569 and
12,872; emit passes 13,573 diagnostics, 12,873 type/symbols, 13,499
JavaScript/declaration baselines, 149 JavaScript maps, and 156 source maps.
Exact-source Fourslash passes 4,091 of 4,559 groups, skips 468, fails none,
and all 4,623 named outcomes match source 74. The full Rust workspace,
40 CLI regressions, WebAssembly target check, warnings-denied Clippy,
source lint, and formatting pass. Fourslash and the workspace transport test
needed the established sandbox escalation for local Unix-socket binds.
Generated-code freshness is checked after this commit because that script
compares the worktree with HEAD. Evidence is under
`target/port-623/0075-b82cd139dc/`.

Five alternating paired **release** runs per cell compared source 74 and 75.
Both binaries emitted identical files and stdout on a synthetic, public
five-project composite graph: `Leaf → Middle → Root`,
`Independent → Root`, and `Other`, with 512 small generated source files in
each leaf. Each run forced a fresh build with two builders. macOS
`/usr/bin/time -l` counted retired instructions and peak RSS including
kernel work; Node measured process wall time. Medians are before → after:

| Workload | Instructions, billion | Wall, seconds | Peak RSS, MiB |
| --- | ---: | ---: | ---: |
| Graph, 2 builders | 3.501 → 3.637 (+3.88%) | 0.1135 → 0.1125 (−0.82%) | 103.1 → 124.2 (+20.54%) |
| Compiler-Unions, single checker, `--noEmit` | 3.274 → 3.273 (−0.005%) | 0.2283 → 0.2290 (+0.33%) | 59.6 → 59.5 (−0.08%) |
| Compiler-Unions, default checkers, `--noEmit` | 3.965 → 3.964 (−0.044%) | 0.1087 → 0.1095 (+0.77%) | 83.2 → 83.6 (+0.47%) |

Ten additional build-graph pairs in reversed order confirmed the RSS
increase: 103.4 → 124.0 MiB (+19.96%), with instructions 3.500 →
3.588 billion (+2.52%) and wall 0.1185 → 0.1095 seconds (−7.61%). A
one-builder control measured peak RSS 92.1 → 100.1 MiB (+8.74%),
instructions +0.11%, and wall −0.14%. The new schedule starts the other
large independent leaf earlier; with two builders the leaves overlap, and
with one builder it runs before the small dependent project gives Rust's
asynchronous per-project free thread more time to reclaim the first leaf.
This is a material build-mode memory and instruction regression on this
graph, not a general compiler-check regression. Waiting for cleanup or
delaying the independent leaf would surrender the concurrency the upstream
change restores; no low-risk parity-preserving fix was identified. The
user's nonblocking migration instruction applies, so the port continues
with the tradeoff disclosed. These local macOS samples do not replace the
repository's two-publish Linux gate. Raw paired samples, manifests,
binaries, and JavaScript collectors are under
`target/port-623/0075-b82cd139dc/perf/`.

## 76. `09e0de0ade` — Build release platforms in parallel

Upstream `09e0de0ade21319a358d8dcea11240c0a322bbef` changes only
`Herebyfile.mjs` and `tools/pipelines/typescript-build.yml`. Its Azure
pipeline creates one job per native platform package, builds the main package
after those jobs, and assembles the platform artifacts for testing and
publishing. tsrs has no Azure/Hereby release pipeline. Its GitHub Actions
`release.yml` already builds the Unix targets in a matrix, plus independent
Windows, musl, and WebAssembly jobs, each depending only on `version`.
`publish` waits for them all, downloads the artifacts, requires every release
binary, and assembles the npm packages. There is therefore no tsrs workflow
change to make for this source commit. This is an evidence-only port; no
semantic deviation or user-visible capability change was needed.

The ignored `ts-ref` checkout advances from source 75 to this exact commit.
The upstream diff touches neither `tsc/` nor compiler fixtures, and this
tsrs commit changes only this ledger entry. Thus the compiler, Fourslash,
workspace, and runtime inputs are identical to source 75; its full passing
evidence is reused: 18 Go-history, 18 canonical, and 36 emit outcome lists
had no changes from source 74; exact-source Fourslash passed 4,091 groups,
skipped 468, failed none, with all 4,623 named outcomes unchanged. Workspace,
40 CLI regressions, WebAssembly, warnings-denied Clippy, source lint, and
formatting also passed at source 75. Evidence is under
`target/port-623/0075-b82cd139dc/`. The published TypeScript pin remains at
the base revision until the complete upgrade. No compiler runtime or release
workflow code changed, so no new performance measurement applies.

## 77. `3789ddb12a` — Fix symbol table kind mask

Upstream `3789ddb12afb2ab5adc4f1148603bf3f38319f50` changes the
checker’s symbol-table kind mask to select all three high kind bits. Rust had
ported the old mask, which selected only the highest bit: exports, members,
and globals were all classified as locals, bypassing export/global alias-list
caching and the member-table early skip. The Rust mask now matches Go. A
focused unit regression checks all five kinds with a nonzero source ID; the
old mask fails for exports, members, and globals. Upstream adds no compiler
fixture, and no semantic deviation was needed. The ignored `ts-ref` checkout
advances to this source commit; the published TypeScript pin remains at the
base revision until the full upgrade.

The exact-source Go checker package test passes. All 18 Go-history, 18
canonical, and 36 emit outcome lists are byte-for-byte unchanged from source
75 (source 76 changed only release automation). Full Go-history checking
passes 13,569 diagnostic and 12,873 type/symbol baselines; canonical
checking passes 13,569 and 12,872; emit passes 13,573 diagnostics, 12,873
type/symbols, 13,499 JavaScript/declaration baselines, 149 JavaScript maps,
and 156 source maps. The initial emit attempt used the runner's short default
timeout and timed out on the existing slow
`intersectionConstructorReductionCrash` case; the complete run with
`--timeout 120` has no timeouts or lost outcomes. Exact-source Fourslash
passes 4,091 groups, skips 468, fails none, and all 4,623 named outcomes
match source 75. The Rust workspace, 40 CLI regressions, WebAssembly target
check, warnings-denied Clippy, source lint, and formatting pass. Fourslash
and the workspace transport test needed the established sandbox escalation
for local Unix-socket binds. Generated-code freshness is checked after the
commit because that script compares the worktree with HEAD. Evidence is under
`target/port-623/0077-3789ddb12a/`.

This change alters alias caching and member-table scanning, but no paired
release performance measurement was collected for this entry; no instruction,
wall-time, or peak-RSS claim is made. The user's nonblocking migration policy
applies.

## 78. `6656c1c12c` — Update GitHub Actions dependency pins

Upstream `6656c1c12c9049974a9adcd4653a895f26ab16ce` updates pinned
`actions/setup-node` 7.0.0 to 7.1.0, `actions/upload-artifact` 7.0.1 to
7.0.2, `actions/download-artifact` 8.0.1 to 8.0.2, and three
`github/codeql-action` references 4.38.1 to 4.38.3 across ten workflows.
The corresponding tsrs port updates the one matching setup-node reference,
16 upload-artifact references, and 12 download-artifact references in five
active workflows to the exact upstream SHA pins and version comments. tsrs
has no CodeQL action, so there is no matching reference to update. Its
separate artifact v4 references remain unchanged. No compiler behavior,
user-visible capability, or semantic deviation is involved. The ignored
`ts-ref` checkout advances to this exact source commit; the published
TypeScript pin remains at the base revision until the full upgrade.

The upstream diff is confined to `.github/workflows/`; it changes no `tsc/`
source, libraries, or fixtures. The tsrs diff changes only workflow pins and
this ledger, so source 77's complete compiler and Fourslash conformance
evidence is reusable: all 18 Go-history, 18 canonical, and 36 emit outcome
lists are unchanged from source 75, and exact-source Fourslash passes 4,091
groups, skips 468, fails none, with all 4,623 named outcomes unchanged.
Source 77 also passed the workspace, 40 CLI regressions, and WebAssembly
checks. For this entry, all 11 active workflow YAML files parse; a static
scan verifies all 29 matching references use the new SHA/version pairs and
none use the old pairs. Warnings-denied Clippy, source lint, formatting,
generated-code freshness, and `git diff --check` pass. Evidence is under
`target/port-623/0078-6656c1c12c/`; the reused full conformance evidence
is under `target/port-623/0077-3789ddb12a/`. No release performance
measurement is needed because compiler and runtime inputs are unchanged.

## 79. `c9b2c8e29c` — Add content mapper output extensions

Upstream `c9b2c8e29c934895a720c14d8e424580d2e56721` lets a content
mapper declare default `outputExtensions` in its package manifest and lets a
project override them in `contentMappers`. The tsrs port validates these maps,
preserves an explicitly empty override, selects the longest matching suffix,
and carries extension rewrites through output paths, module emit, helper
injection, declaration emit, and checker diagnostics. A mapped `.vue` source
whose external output is `.js` now has a `.d.ts` declaration path; static and
dynamic imports use the rewritten JavaScript path, including in declarations.
The compiler also checks collisions against mapper-produced external outputs
and diagnoses unsafe or omitted relative rewrites. The LSP protocol and npm
`tsrs/vscode` manifest type accept `outputExtensions` in mapper contributions;
the language-server contribution handler remains unported, as before. The
ignored `ts-ref` checkout advances to
this exact source commit; the published TypeScript pin remains at the base
revision until the complete upgrade.

The direct Rust regressions cover manifest defaults, project overrides and
empty overrides, invalid maps, overlapping suffixes, mapped declaration and
JavaScript emit, dynamic import and helper rewriting, collisions, unsafe
rewrites, and repeated incremental/build invocations after a manifest edit.
Focused tests in exact-source Go pass for the new output-extension and
content-mapper paths; its own compiler harness passes the 13 new
`contentMapperOutputExtensions*` variants and their baseline subtests. A
watch-session manifest-edit probe leaves an already emitted `.js` import stale
in both Rust and this exact Go revision; this is an
upstream behavior, while the repeated-invocation regression passes. The full
Rust workspace, 40 CLI regressions, `wasm32-wasip1` target check,
warnings-denied Clippy, source lint, formatting, both real-process
content-mapper e2e fixtures, and the npm SDK build/declaration check pass.

All 18 Go-history check, 18 canonical check, and 36 emit pass-set lists have
zero lost or new passing names relative to source 77 (source 78 changed only
workflows). Go-history passes 13,569 diagnostic and 12,873 type/symbol
baselines; canonical passes 13,569 and 12,872; emit passes 13,499 JavaScript,
149 JavaScript-map, and 156 source-map baselines. The runner still skips all
25 `contentMapper*.ts` compiler fixture files: this upstream commit adds 10
files with 13 variants, all `@runExternalCode` and all skipped, so those are
not conformance passes. Exact-source Fourslash passes 4,091 groups, skips
470, and fails none. Its two new output-extension project-reference groups
skip because the Go subprocess runner cannot carry mapper callbacks across
the process boundary; all previous named outcomes are unchanged. Evidence
and the JavaScript pass-set comparator are under
`target/port-623/0079-c9b2c8e29c/`.

Paired local macOS release measurements used five alternating source-78/source-79
pairs per cell, the same public inputs and output hashes, and the immediate
source-78 binary built from `b7dc9667d8`. macOS `time -l` counts instructions
including kernel work, so these are a regression screen rather than the
repository's Linux `bench/count.mjs` instruction gate or two-publish wall-time
gate. The largest no-emit increase was +0.307% instructions and +0.518% peak
RSS on VS Code's default mode; wall time there was -0.23%. The forced
five-project build/emit workload was -0.110% instructions and -0.427% RSS.
No material performance regression was observed; the small per-cell changes
are within this local counter's run-to-run variation.

| public case | instructions, source 78 → 79 | peak RSS, MiB, source 78 → 79 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.2734G → 3.2731G | 59.5 → 59.6 | +1.09% |
| Compiler-Unions, default | 3.9626G → 3.9616G | 83.4 → 83.5 | -2.13% |
| webpack, single | 10.8349G → 10.8421G | 310.7 → 310.8 | -0.62% |
| webpack, default | 15.1869G → 15.1853G | 503.3 → 502.4 | -0.85% |
| VS Code, single | 81.4433G → 81.6074G | 1,606.7 → 1,607.3 | -0.27% |
| VS Code, default | 93.4560G → 93.7425G | 2,107.1 → 2,118.0 | -0.23% |
| Forced multi-project build/emit | 3.6159G → 3.6120G | 124.5 → 124.0 | -1.87% |

The release binaries, input fixture, raw paired samples, and JavaScript
collectors are under `target/port-623/0079-c9b2c8e29c/perf/`.

## 80. `1d76970f7a` — Keep local ambient-module imports local

Upstream `1d76970f7a46fc330dc32bbe74ae3ff611d571cb` stops resolving an
import when its specifier is already an ambient module name declared in the
same source file. The tsrs file loader applies the same check before the
resolver in both its sequential and speculative prefetch paths, records an
unresolved module entry under the normal mode-aware key, and emits Go's local
ambient-module trace when `--traceResolution` is enabled. This prevents a
same-named package from entering the program through an import inside the
ambient declaration. The ignored `ts-ref` checkout advances to this exact
source commit; the published TypeScript pin remains at the base revision
until the complete upgrade.

The new upstream `ambientModuleImportingItselfNotResolved` fixture failed in
the source-79 Rust binary and now passes diagnostics, types, symbols, and
JavaScript baselines. Exact-source Go passes all its fixture subtests,
including module resolution. A materialized copy of the existing
`nodeColonModuleResolution` fixture and a standalone reproduction of the new
fixture have byte-identical Go/Rust CLI resolution traces and diagnostics; neither
compiler loads the same-named package in the new case. Exact-source Go also
passes the changed `nodeColonModuleResolution` trace baseline.

Across all 18 Go-history check, 18 canonical check, and 36 emit result lists,
no source-79 passing outcome is lost; the only additions are the new compiler
fixture's passing diagnostics, types, symbols, and JavaScript baselines and
its skipped map baselines. Go-history passes 13,570 diagnostic and 12,874
type/symbol baselines; canonical passes 13,570 and 12,873; emit passes
13,500 JavaScript, 149 JavaScript-map, and 156 source-map baselines. The
`intersectionConstructorReductionCrash` fixture exceeded the runner's
default 20-second timeout in the first local runs, passed alone with a
60-second limit, and the complete Go-history/emit runs then passed with that limit;
all prior named outcomes remained unchanged. Exact-source Fourslash passes
4,091 groups, skips 470, fails none, with all 4,627 named outcomes unchanged.
The full Rust workspace, 40 CLI regressions, `wasm32-wasip1` target check,
warnings-denied Clippy, source lint, and formatting pass. The workspace and
Fourslash gates needed local Unix-socket access after the filesystem sandbox
denied binds. Evidence and the JavaScript pass-set comparator are under
`target/port-623/0080-1d76970f7a/`.

Paired local macOS release measurements used the immediate source-79 binary,
five alternating pairs per public project and checker mode, and identical
compiler output hashes. macOS `time -l` includes kernel work in its retired
instruction counts, so this is a local regression screen rather than the
repository's deterministic Linux instruction gate. No material instruction
or peak-memory regression was observed. A five-pair forced build/emit screen
showed 3.5566G → 3.5489G instructions (-0.218%), 124.08 → 124.14 MiB peak
RSS (+0.050%), and +3.49% wall time; a reversed-order 15-pair confirmation
showed 3.6032G → 3.6051G instructions (+0.052%), 124.16 → 124.13 MiB
(-0.025%), and -2.49% wall time. Output hashes matched in both runs. The
short-workload wall-time change reversed, so it does not establish a
regression.

| public case | instructions, source 79 → 80 | peak RSS, MiB, source 79 → 80 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.2731G → 3.2732G | 59.6 → 59.6 | -0.34% |
| Compiler-Unions, default | 3.9652G → 3.9638G | 83.4 → 83.3 | +0.44% |
| webpack, single | 10.8403G → 10.8409G | 310.8 → 311.3 | -2.28% |
| webpack, default | 15.1997G → 15.1851G | 502.4 → 503.5 | +1.31% |
| VS Code, single | 81.5565G → 81.5550G | 1,607.3 → 1,603.0 | -0.04% |
| VS Code, default | 93.7209G → 93.6657G | 2,109.9 → 2,105.9 | -0.19% |

The release binaries, paired samples, output manifests, and JavaScript
collectors are under `target/port-623/0080-1d76970f7a/perf/`.

## 81. `7856840543` — Name decorated anonymous class declarations

Upstream `7856840543f3c536ac80e4b141c95a7946ddb8ac` fixes a crash when
ES2022 decorator lowering encounters an anonymous class declaration that is
not an `export default` declaration. The Rust runtime-syntax transformer now
generates a name for a directly decorated anonymous class in that case, as
Go does. It retains the existing path for valid `export default` classes.
The new upstream fixture deliberately has two grammar errors (`@dec class {}`
and `@dec default class {}`); both errors remain while their JavaScript emits
with generated names. The ignored `ts-ref` checkout advances to this exact
source commit; the published TypeScript pin stays at the base revision until
the complete upgrade.

Exact-source Go passes the new fixture's error, output, type, symbol, and
other harness subtests. Rust passes its focused error, `.types`, `.symbols`,
and `.js` baselines. All 18 Go-history, 18 canonical, and 36 emit outcome
lists retain every source-80 passing name, with only the new fixture's
expected passes and skipped maps added. Go-history checking passes 13,571
diagnostic and 12,875 type/symbol baselines; canonical passes 13,571 and
12,874. Emit passes 13,501 JavaScript, 149 JavaScript-map, and 156 source-map
baselines. Exact-source Fourslash retains all 4,627 named outcomes: 4,091
exercised groups pass, 470 groups skip, and none fail. The full Rust
workspace, 40 CLI regressions, WASM target check, warnings-denied Clippy,
source lint, and formatting pass. The workspace and Fourslash suites needed
local Unix-socket access after sandboxed socket creation was denied. Evidence
and the outcome-list comparator are under
`target/port-623/0081-7856840543/`.

Paired local macOS release measurements used the immediate source-80 binary
and five alternating pairs per cell. All public-project stdout hashes and
all emitted-file manifests for a 128-module decorated-class workload match.
The exact invalid anonymous-class crash case cannot be measured in pairs
because source 80 aborts on it; the emit workload uses valid anonymous
decorated default classes. macOS `time -l` includes kernel work in retired
instruction counts, so these results are a local regression screen, not
the repository's deterministic Linux instruction gate. The largest measured
instruction increase is +0.333% on the small decorator workload; the largest
peak-RSS increase is +0.143% on webpack. Neither is material.

| case | instructions, source 80 → 81 | peak RSS, MiB, source 80 → 81 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.273254G → 3.273150G | 59.64 → 59.61 | -0.59% |
| Compiler-Unions, default | 3.965517G → 3.963499G | 83.38 → 83.38 | -2.41% |
| webpack, single | 10.840964G → 10.840620G | 311.31 → 311.28 | -0.52% |
| webpack, default | 15.205373G → 15.231194G | 501.72 → 502.44 | -0.01% |
| VS Code, single | 81.495993G → 81.471324G | 1,602.98 → 1,602.95 | -0.74% |
| VS Code, default | 93.687082G → 93.670161G | 2,111.36 → 2,102.00 | +0.15% |
| Decorator emit, single | 0.349400G → 0.349675G | 22.27 → 22.28 | -0.21% |
| Decorator emit, default | 0.511831G → 0.513536G | 39.77 → 39.72 | -0.61% |

The release binaries, JavaScript collectors, raw paired samples, and output
hashes are under `target/port-623/0081-7856840543/perf/`.

## 82. `3c1e603122` — Exclude a config from its own extended source files

Upstream `3c1e6031220cbdf65ba9a749d3dcde7b70aa042a` prevents a
configuration file from listing itself among its extended source files when a
cached `extends` chain loops back to it. The Rust config parser now compares
each extended file's case-sensitive or case-insensitive path key with the
current config's resolved path before adding it to the sorted list. The
ignored `ts-ref` checkout advances to this exact source commit; the published
TypeScript pin remains at the base revision until the complete upgrade.

The direct Rust config-parser regression reproduced the old self-entry before
the fix and passes after it. A Rust API regression opens p1 and then p2 in one
session through the same circular shared config and checks that p2 reports
only TS18000, matching the new exact-source Go API test. All 18 Go-history,
18 canonical, and 36 emit pass/nonpass lists are byte-for-byte identical to
source 81: Go-history checking passes 13,571 diagnostic and 12,875 type/symbol
baselines; canonical checking passes 13,571 and 12,874; emit passes 13,501
JavaScript, 149 JavaScript-map, and 156 source-map baselines. Exact-source
Fourslash retains all 4,627 named outcomes: 4,091 exercised groups pass,
470 groups skip, and none fail. The full Rust workspace, 40 CLI regressions,
`wasm32-wasip1` target check, warnings-denied Clippy, source lint, formatting,
and generator freshness pass. The workspace and Fourslash suites needed local
Unix-socket access after sandboxed socket creation was denied. The API test,
full conformance comparisons, and gate logs are under
`target/port-623/0082-3c1e603122/`.

Paired local macOS release measurements used the immediate source-81 binary,
five alternating pairs per public project and checker mode, and identical
compiler output hashes. macOS `time -l` includes kernel work in its retired
instruction counts, so this is a local regression screen rather than the
repository's deterministic Linux instruction gate. Across those six cells,
the largest instruction increase was +0.046% and the largest peak-RSS increase
was +0.225%. VS Code/default initially showed +7.81% wall time despite only
+0.072% instructions and -0.078% peak RSS. Ten reversed confirmation pairs
showed +0.84% wall time, -0.028% instructions, and +0.040% peak RSS. This
does not establish a material regression.

| public case | instructions, source 81 → 82 | peak RSS, MiB, source 81 → 82 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.2748G → 3.2732G | 59.61 → 59.59 | +0.12% |
| Compiler-Unions, default | 3.9619G → 3.9633G | 83.45 → 83.64 | -1.13% |
| webpack, single | 10.8553G → 10.8603G | 311.28 → 311.28 | -0.08% |
| webpack, default | 15.3091G → 15.3111G | 502.95 → 503.56 | +1.75% |
| VS Code, single | 82.5845G → 81.7292G | 1,602.95 → 1,602.94 | -0.90% |
| VS Code, default | 93.8988G → 93.9664G | 2,109.95 → 2,108.31 | +7.81%* |

*The reversed ten-pair confirmation reduced the VS Code/default wall delta
to +0.84%; its instruction and RSS deltas were both below 0.05%.
The release binaries, paired samples, output hashes, JavaScript collector,
and confirmation runs are under `target/port-623/0082-3c1e603122/perf/`.

## 83. `6aace8b1bb` — Cache simplified conditional types by read/write mode

Upstream `6aace8b1bbe6c55eb8b4d01cd35487172905f8cc` caches the result of
`getSimplifiedConditionalType` for a type ID and the reading or writing mode.
The Rust checker adds the same two `CachedTypeKind` entries and keeps the
original algorithm in a worker invoked only on a cache miss. The two modes
must have separate keys: the new fixture reads a conditional type successfully
but writes to it with TS2322. The ignored `ts-ref` checkout advances to this
exact source commit; the published TypeScript pin stays at the base revision
until the complete upgrade.

Exact-source Go passes all nine new fixture subtests, and Rust passes the
focused diagnostic, `.types`, `.symbols`, and `.js` baselines. All 18
Go-history, 18 canonical, and 36 emit outcome lists retain every source-82
result, with only the new fixture's expected passes and skipped maps added.
Go-history checking passes 13,572 diagnostic and 12,876 type/symbol
baselines; canonical passes 13,572 and 12,875. Emit passes 13,502 JavaScript,
149 JavaScript-map, and 156 source-map baselines. Exact-source Fourslash
retains all 4,627 named outcomes: 4,091 exercised groups pass, 470 groups
skip, and none fail. The full Rust workspace, 40 CLI regressions, WASM target
check, warnings-denied Clippy, source lint, and formatting pass. The workspace
and Fourslash suites needed local Unix-socket access after sandboxed socket
creation was denied. Full conformance comparisons and other evidence are under
`target/port-623/0083-6aace8b1bb/`.

Prior work was checked before this cache port: `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, the newest
`notes/mem-round4.md`, and the checker cache/history notes. None rejected
this exact upstream cache. Cache history can change type IDs or budget errors,
so the full named outcomes and exact Go fixture were checked rather than
assuming a cached result is output-neutral.

Paired local macOS release measurements used the immediate source-82 binary,
five alternating pairs per public project and checker mode, and identical
compiler output hashes. macOS `time -l` includes kernel work in its retired
instruction counts, so this is a local regression screen rather than the
repository's deterministic Linux instruction gate. The largest instruction
increase was +0.261% and the largest peak-RSS increase was +0.662%. The
webpack/default wall sample rose +9.92%, but ten reversed confirmation pairs
showed -2.83% wall, +0.080% instructions, and +0.369% peak RSS. This does not
establish a material regression or a qualifying performance gain.

| public case | instructions, source 82 → 83 | peak RSS, MiB, source 82 → 83 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.2731G → 3.2731G | 59.58 → 59.61 | +0.26% |
| Compiler-Unions, default | 3.9615G → 3.9658G | 83.42 → 83.31 | -1.75% |
| webpack, single | 10.8566G → 10.8506G | 311.28 → 311.50 | -2.45% |
| webpack, default | 15.2064G → 15.2461G | 502.97 → 503.95 | +9.92%* |
| VS Code, single | 82.4446G → 82.4300G | 1,602.94 → 1,613.55 | -13.25% |
| VS Code, default | 93.9029G → 93.7060G | 2,102.64 → 2,098.88 | -3.89% |

*The reversed ten-pair confirmation changed the webpack/default wall delta
to -2.83%. The release binaries, raw samples, output hashes, JavaScript
collector, and confirmation runs are under
`target/port-623/0083-6aace8b1bb/perf/`.

## 84. `67a1c76aac` — Generate concrete AST accessors and token payloads

Upstream `67a1c76aacdeed279c72a87347e41c1b12a26a0f` removes the Go
`nodeData` method interface and generates kind-dispatched node accessors,
visitors, cloning, and subtree-fact calls. It also makes `NewToken` allocate
the canonical node payload for kinds shared with literals, identifiers, and
keyword expression/type nodes. Rust already has generated concrete dispatch:
its compact `NodeDataTag` selects typed payloads, and generated constant-offset
tables implement the shared accessors. Its two synthetic flow payloads already
have distinct data tags, so Go's new private flow kinds are unnecessary in
the Rust representation. The missing behavior was `new_token`: it still made
a generic token for every kind, so `new_token(NullKeyword)` lacked the
`KeywordExpression` flow slot. The Rust AST generator now derives the same
canonical cases from the schema, and an Identifier-only helper preserves
Go's zero-text `NewToken` without incrementing the factory's text count.
The ignored `ts-ref` checkout advances to this source commit; the published
TypeScript pin stays at the base revision until the complete upgrade.

The exact-source Go AST accessor tests pass. A focused Rust AST test verifies
identifier and literal text, keyword flow data, keyword-type subtree facts,
cloning, ordinary punctuation, and the unchanged text count. All 18
Go-history, 18 canonical, and 36 emit outcome lists match source 83 exactly.
Go-history checking retains 13,572 diagnostic and 12,876 type/symbol passes;
canonical checking retains 13,572 and 12,875. Emit retains 13,502 JavaScript,
149 JavaScript-map, and 156 source-map passes. Exact-source Fourslash retains
all 4,627 named outcomes: 4,091 exercised groups pass, 470 skip, and none
fail. Full workspace tests, 40 CLI regressions, WASM target check,
warnings-denied Clippy, source lint, formatting, and generator freshness pass.
The workspace and Fourslash suites needed local Unix-socket access after
sandboxed socket creation was denied. Evidence, comparators, and gate logs
are under `target/port-623/0084-67a1c76aac/`.

Prior performance work was checked in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, and the newest
`notes/mem-round4.md`. None rejected this required upstream representation
change. Paired local macOS release measurements used the immediate source-83
binary, five alternating pairs for each public project and checker mode, and
identical compiler-output hashes. macOS `time -l` includes kernel work in its
retired instruction count, so this is a local regression screen rather than
the repository's deterministic Linux instruction gate.

| public case | instructions, source 83 → 84 | peak RSS MiB, source 83 → 84 | wall change |
| --- | ---: | ---: | ---: |
| Compiler-Unions, single | 3.2733G → 3.2739G | 59.59 → 59.64 | -0.29% |
| Compiler-Unions, default | 3.9660G → 3.9643G | 83.31 → 83.31 | +3.82%* |
| webpack, single | 10.8537G → 10.8544G | 311.52 → 311.59 | +0.26% |
| webpack, default | 15.1365G → 15.3887G | 502.00 → 506.72 | -1.61%† |
| VS Code, single | 81.6137G → 81.6526G | 1,613.55 → 1,613.66 | +0.92% |
| VS Code, default | 93.9813G → 93.7960G | 2,102.31 → 2,109.79 | -1.58% |

*A reversed ten-pair confirmation changed Compiler-Unions/default wall time
to -1.18%, with -0.022% instructions and unchanged peak RSS. †The initial
webpack/default +1.666% instruction and +0.940% RSS changes fell to +0.155%
and +0.071% in a reversed ten-pair confirmation; a further 20-pair run
measured -0.173% instructions, +0.178% peak RSS, and +1.36% wall time. The
initial wall directions also reversed. The measurements do not establish a
material performance regression. Raw paired samples and the frozen release
binaries are under `target/port-623/0084-67a1c76aac/perf/`.

## 85. `f3a9375b53` — Compact checker type metadata

Upstream `f3a9375b53f80533a4bb1f6809155d8f0459e860` removes the Go
`TypeData` interface field from every checker type. A `typeKind` byte in the
header identifies the concrete type for casts, separate from mutable semantic
flags; all concrete Go types embed the header at offset zero. The constructors
and trace adapter use the tag, and Go adds cast, arena-growth, and header-size
tests. Rust already has the corresponding compact representation:
`TypeAlloc<T>` puts the tagged `Type` header and its payload in one arena
allocation, concrete casts check `TypeDataTag`, and the header is pinned at
24 bytes (the new Go header is 40 bytes on 64-bit). Rust's `TypeData` enum is
only a temporary view of that in-place payload, not a stored interface field.
For memory savings, structured base constraints intentionally live in the
checker rather than an embedded `ConstrainedType`, so Rust's structured
`try_as_constrained_type` returns `None` while Go's returns a pointer.

The Rust port adds a focused safety test for all 20 concrete type kinds:
direct payload address and tag, six shared-base cast categories, kind
independence from mutable flags, and pointer stability after 1,024 further
arena allocations. Exact-source Go's `TestTypeCasts`, `TestTypeHeaderSize`,
and `TestTypeAllocationAcrossCheckerGrowth` pass. The ignored `ts-ref`
checkout advances to this exact source commit; the published TypeScript pin
stays at the base revision until the complete upgrade.

All 18 Go-history, 18 canonical, and 36 emit named outcome lists match source
84 exactly, with no lost passes. Go-history checking retains 13,572 diagnostic
and 12,876 type/symbol passes; canonical retains 13,572 and 12,875. Emit
retains 13,502 JavaScript, 149 JavaScript-map, and 156 source-map passes.
Source 85 changes no TypeScript fixture, library, or language-server file,
and Rust runtime code is unchanged, so source 84's 4,627 Fourslash named
outcomes remain applicable. The full Rust workspace, 40 CLI regressions,
WASM target check, warnings-denied Clippy, source lint, and formatting pass.
The workspace needed local Unix-socket access after sandboxed socket
creation was denied. Outcome lists, comparator, and exact-source Go test
cache are under `target/port-623/0085-f3a9375b53/`.

Prior performance work was reviewed in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, the latest
`notes/mem-round4.md`, and the measured 32-to-24-byte Rust type-header
reduction in `notes/mem-small.md`. There is no new Rust runtime layout or
allocation path to benchmark in this port. A fresh `cargo build --release`
produced a byte-identical CLI executable to source 84 (both SHA-256
`1f01b0c58a0b5cfa2d893aa97a3649203c32e80de957532aecbde5fda243559f`).
Thus the source-84 public instruction, peak-RSS, and wall measurements apply
unchanged; source 85 cannot introduce a runtime performance regression. The
frozen source-85 executable is in its ignored evidence directory.

## 86. `ce609c851b` — Batch common checker type allocations

Upstream `ce609c851b5109a44579ee2cdc29545292dd654f` adds ten typed
`core.Arena` fields to the Go checker and allocates its common literal,
object, reference, interface, mapped, union, intersection, type-parameter,
indexed-access, and conditional type payloads from those arenas instead of
individual Go heap objects. Its diff changes only `tsc/internal/checker/checker.go`:
no fixture, library, configuration, or language-server input changes. Rust
already allocates **every** checker type in a per-thread arena. Specifically,
`Checker::new_type` calls `Type::alloc`, which allocates a `TypeAlloc<T>` with
the tagged header and payload together through `P::new`. This also avoids a
second allocation per payload. The earlier analogous Go allocation change
was reviewed in `notes/perf-tsgo-can1357-speedups.md` and found already
covered in tsrs. No Rust runtime or capability change is needed. The ignored
`ts-ref` checkout advances to this exact source commit; the published
TypeScript pin remains at the base revision until the full upgrade.

Exact-source Go's `./internal/checker` package passes with `go test -count=1`,
and Rust's compact-payload arena-growth regression passes. Because the Rust
production and conformance inputs are unchanged and the upstream
commit adds no fixture or baseline, source 85's complete 18 Go-history, 18
canonical, and 36 emit named outcome lists remain applicable with no lost
passes. Its 4,627 Fourslash named outcomes, workspace, CLI regressions, and
WASM evidence likewise carry forward. New dev and release builds, warnings-denied
Clippy, source lint, formatting, and generator freshness pass for this entry.
The root and npm
compatibility summaries and `docs/COMPATIBILITY.md` need no update.

Prior performance work was reviewed in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, and the latest
`notes/mem-round4.md`. This entry changes no Rust executable or runtime
assets. A release rebuild is byte-identical to source 85 (SHA-256
`1f01b0c58a0b5cfa2d893aa97a3649203c32e80de957532aecbde5fda243559f`),
so its instruction, peak-RSS, and wall-time results are exactly inherited from
source 85 and ultimately source 84; no new performance regression is possible
for the same inputs. Focused logs and identity evidence are under
`target/port-623/0086-ce609c851b/`.

## 87. `9eedcb0dd7` — Presize checker type-list cache keys

Upstream `9eedcb0dd7257f8a1a175adbef28f3b6d3492d7a` reserves the complete
8-byte length plus 4-byte type IDs before `keyBuilder.writeTypes`. In Go this
keeps type-list keys of up to 46 entries inline and makes longer keys use one
overflow allocation. The Rust builder already has the same 192-byte inline
buffer and key bytes, but its spilled `hash` copied the overflow buffer into a
second allocation. The port reserves before `write_types` and appends the
remaining inline bytes to that reserved overflow buffer when hashing. Hashing
twice is still idempotent, and writing more bytes afterward still appends to
the same logical key. The focused Rust tests cover exact xxh3-128 bytes with
prefix and suffix, repeated hashing, later writes, the 46/47-type spill
boundary, and capacity/pointer stability through the hash for 256 and 4,096
types. Exact-source Go `TestKeyBuilder` and `TestTypeListKeyAllocations` pass.
The ignored `ts-ref` checkout advances to this source commit; the published
TypeScript pin remains at the base until the full upgrade.

All 18 Go-history, 18 canonical, and 36 emit named compiler outcome lists
are byte-for-byte identical to source 85/86, with no lost passes. Go-history
checking retains 13,572 diagnostic and 12,876 type/symbol passes; canonical
checking retains 13,572 and 12,875. Emit retains 13,502 JavaScript, 149
JavaScript-map, and 156 source-map passes. Exact-source Fourslash has 4,091
exercised groups passing, 470 skipped, and none failing. The full Rust
workspace, 40 CLI regressions, `wasm32-wasip1` check, warnings-denied Clippy,
source lint, and formatting pass. Fourslash and workspace Unix-socket checks
needed a socket-enabled rerun after sandbox denial. The generator freshness
check runs after this entry is committed because it checks the entire dirty
worktree, including the intentional checker edit. No user-visible capability
or compatibility count changes. Evidence is under
`target/port-623/0087-9eedcb0dd7/`.

Prior work was reviewed in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, the newest
`notes/mem-round4.md`, and checker cache-key notes. None rejects this exact
upstream type-list presizing. Paired local macOS release measurements used the
immediate source-86 executable, five alternating pairs per public project and
checker mode, with identical compiler output hashes. macOS `time -l` includes
kernel work in retired instructions, so this is a local regression screen
rather than the deterministic Linux instruction gate. The small single-thread
instruction reductions and sub-0.5% RSS changes do not meet the repository's
standalone performance-change bar; this change is included to satisfy the
requested source-by-source port, with its performance evidence disclosed.

| public case | mode | instructions, source 86 → 87 | peak RSS, MiB, source 86 → 87 | wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.2737G → 3.2719G (-0.055%) | 59.64 → 59.59 (-0.079%) | -0.77% |
| Compiler-Unions | default | 3.9664G → 3.9638G (-0.064%) | 83.55 → 83.50 (-0.056%) | -1.26% |
| webpack | single | 10.8588G → 10.8554G (-0.031%) | 311.59 → 311.59 (0.000%) | -0.37% |
| webpack | default | 15.1517G → 15.2679G (+0.767%) | 500.75 → 503.03 (+0.456%) | -0.71%* |
| VS Code | single | 81.6347G → 81.6323G (-0.003%) | 1,613.66 → 1,613.63 (-0.002%) | -0.95% |
| VS Code | default | 93.9105G → 93.9110G (+0.001%) | 2,113.92 → 2,112.27 (-0.078%) | +0.26% |

*Webpack/default's reversed ten-pair confirmation changed instructions to
-0.372%, RSS to -0.152%, and wall to +5.69%; a further twenty alternating
pairs measured +0.245% instructions, +0.404% RSS, and -2.08% wall. The
direction changes do not establish a material regression. Frozen release
binaries, raw paired samples, output hashes, and the JavaScript collector are
under `target/port-623/0087-9eedcb0dd7/perf/`.

## 88. `86c0182bbf` — Avoid allocating normalized types on interning cache hits

Upstream `86c0182bbf8b57eeb3ce343f598519867245c115` changes only Go checker
allocation management and tests, with no compiler fixture, library, or
user-visible behavior change. It borrows a per-checker temporary set for union
and intersection normalization, makes `mapTypeEx` build a mapped list only
after the first changed constituent, and copies constituent lists only when
publishing new composite types or caching a subtype reduction. Its focused Go
tests check constituent ownership, zero-allocation cache hits, and reuse through
nested calls and large sets. The exact-source Go checker package passes.

Rust already copies new union and intersection lists with `alloc_slice`,
copies cached subtype-reduction lists with `alloc_slice`, and pools intersection
and mapping scratch lists. Its default union front cache also bypasses many
normalization calls on cache hits. The remaining difference is the ordinary
union-normalization list, a mapping list built before a change is known, and a
short list in `remove_type`. An earlier simple pooled-`getUnionType` experiment
was rejected at -0.04% instructions, within noise
(`notes/perf-round2-followups.md`, “Measured and rejected”). The new upstream
patch and combined reuse across the other call paths justified a fresh probe.

A 100-line Rust candidate added a reentrant checker-local set pool, retained
the intersection dedup map, reused the set in union construction and
`remove_type`, and delayed mapping-list construction until the first changed
constituent. It kept the arena copy at every publication boundary. The
candidate passed all 18 Go-history, 18 canonical, and 36 emit named outcome
lists with exact source-87 equality; there were no new failures, crashes, or
timeouts. All 13,572 Go-history diagnostic passes, 12,876 type/symbol passes,
13,502 JavaScript passes, 149 JavaScript-map passes, and 156 source-map passes
were retained. Canonical checking retained 13,572 diagnostics and 12,875
type/symbol passes. Go's exact-source ownership and temporary-set tests passed.

Five alternating paired macOS release runs per public cell compared the
immediate source-87 executable with the candidate. Both compilers produced
identical output hashes. `time -l` instructions include kernel work, so these
are local screens rather than the deterministic Linux instruction gate.

| public case | mode | instructions, source 87 → candidate | peak RSS, MiB, source 87 → candidate | wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.2721G → 3.2676G (-0.139%) | 59.59 → 59.70 (+0.184%) | +0.01% |
| Compiler-Unions | default | 3.9614G → 3.9540G (-0.185%) | 83.44 → 83.23 (-0.243%) | +1.29% |
| webpack | single | 10.8447G → 10.8513G (+0.061%) | 311.59 → 311.30 (-0.095%) | -0.31% |
| webpack | default | 15.3138G → 15.3633G (+0.323%) | 505.09 → 507.31 (+0.439%) | +0.83% |
| VS Code | single | 81.6072G → 81.6082G (+0.001%) | 1,613.63 → 1,613.55 (-0.005%) | -0.68% |
| VS Code | default | 93.7947G → 93.9679G (+0.185%) | 2,111.39 → 2,106.22 (-0.245%) | +1.90% |

The largest single-thread instruction gain is 0.185% and the largest
default-checker peak-memory gain is 0.245%, both below the 1% and 5%
performance bars; no two-publish 2% wall gain was established. The candidate
also moved webpack/default instructions and VS Code/default wall time upward
locally. Therefore it was not retained. The resulting source-88 tsrs commit is
an evidence-only port: the ignored `ts-ref` checkout advances to the exact
source commit, while the published TypeScript pin and runtime code remain as
source 87. The candidate patch, frozen binaries, raw paired samples, output
hashes, collector, and full named outcome lists are under
`target/port-623/0088-86c0182bbf/`.

The final release executable is byte-identical to source 87 (both SHA-256
`106a23c08ca5a6a6041e5d49ac28784d4e120b66d0d351a62aece14980f3d42a`),
so final runtime performance is exactly unchanged. The dev CLI and testrunner
build, exact-source Fourslash gate (4,091 exercised groups pass, 470 skipped,
none fail), full Rust workspace, warnings-denied Clippy, source lint,
formatting, and generator freshness all pass. A sandbox Unix-socket denial
needed an escalated Fourslash/workspace rerun; one FSEvents load-test assertion
on the first unrestricted workspace run passed on the clean rerun. Source 87's
CLI regressions and WASM checks apply unchanged because no production file
changed. No capability status, README summary, or compatibility count changes.

## 89. `3c105e1420` — Compact type mappers and coallocate inference mapper storage

Upstream `3c105e1420572a18e4487cd05e9db903b0feb976` replaces Go's stored
`TypeMapperData` interface with a kind header and concrete dispatch, embeds
both inference mapper records in each `InferenceContext`, and adds mapper
dispatch, layout, clone, and lifetime tests. Rust already stores every mapper
in a pinned **16-byte** two-word tagged record; Go's new simple and merged
mapper bound is 24 bytes on 64-bit. Rust's `TypeMapper::kind()` already gives
structural ordering to simple, array, and merged mappers and treats all other
kinds as unknown in comparisons, matching Go's new `comparableMapperKind`.
Its fixing and non-fixing inference mappers are created only on first use,
keep stable identities, and participate in context escape and arena recycling.

Embedding two 16-byte mapper slots would add 32 bytes to *every* Rust
inference context, including contexts that use neither mapper. This would
discard the measured lazy-allocation saving in `notes/mem-round3.md` and
require a new ownership scheme for the escape/recycle paths in
`notes/mem-recycle.md`. The already compact Rust runtime representation is
therefore retained. Focused Rust tests now exercise each packed mapper kind
and payload after arena growth, `maps_this_only`, lazy and eager inference
mapper identity, cloning, escape, and recycling, including with arena poison.
The exact-source Go checker package passes. The ignored `ts-ref` checkout
advances to this source commit; the published TypeScript pin remains at the
base revision until the complete upgrade.

All 18 Go-history, 18 canonical, and 36 emit named compiler outcome lists
match source 88 exactly, with no lost passes. Go-history checking retains
13,572 diagnostic and 12,876 type/symbol passes; canonical checking retains
13,572 and 12,875. Emit retains 13,502 JavaScript, 149 JavaScript-map, and
156 source-map passes. Exact-source Fourslash has 4,091 exercised groups
passing, 470 skipped, and none failing. The full Rust workspace, 40 CLI
regressions, `wasm32-wasip1` check, warnings-denied Clippy, source lint, and
formatting pass. The workspace and Fourslash tests needed local Unix-socket
access after sandboxed socket creation was denied. No capability status,
README summary, or compatibility count changes. Evidence is under
`target/port-623/0089-3c105e1420/`.

Prior performance work was reviewed in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, the newest
`notes/mem-round4.md`, and the mapper/layout/recycling notes above. This
entry changes only Rust test code and the port ledger, with no runtime path.
A fresh release build produced a byte-identical CLI executable to source 88
(both SHA-256 `106a23c08ca5a6a6041e5d49ac28784d4e120b66d0d351a62aece14980f3d42a`).
Thus source 88's public instruction, peak-RSS, and wall measurements apply
unchanged: this commit cannot introduce a runtime performance regression for
the same inputs. Generator freshness is checked after the commit because the
generator script rejects the intentional dirty checker test edit.

## 90. `39b621793a` — Move cold value-symbol links out of hot records

Upstream `39b621793ab62a5ca57f97756a2308dddc70975f` reduces Go's
`ValueSymbolLinks` from six inline pointers (48 bytes) to four (32 bytes).
Its `writeType`, `nameType`, and `containingType` move into a lazily allocated
three-pointer `valueSymbolExtra` (24 bytes); all checker access sites use
getters and setters, and a new Go test checks layout, nil writes, identity,
and retention across garbage collection. Rust already uses a smaller
three-word `ValueSymbolLinks`: 16 bytes with compressed handles (24 with
plain 64-bit pointers), with a lazily allocated rare-field tail. Synthetic
symbols keep containing and name types in otherwise unused target and mapper
words without allocating that tail. Compile-time size assertions, the
`value_symbol_links_synthetic_mode_keeps_every_field` regression, and the
checker-fork tail copy already cover its layout and field preservation.
Changing the Rust representation again would repeat an existing optimization,
so this port only updates the stale Go-layout comment and this ledger.

The ignored `ts-ref` checkout advances to this exact source commit; the
published TypeScript pin remains at the base revision until the complete
upgrade. Exact-source Go's new layout test and full `./internal/checker`
package pass. Rust's focused synthetic-mode test passes. The upstream diff
changes no compiler fixture, baseline, library, or user-facing behavior.
Rust's production and conformance inputs are unchanged, so source 89's full
18 Go-history, 18 canonical, and 36 emit named outcome lists carry forward
without a lost pass. Its exact-source Fourslash results likewise remain
4,091 exercised groups passing, 470 skipped, and none failing. The source-89
workspace, 40 CLI regressions, and `wasm32-wasip1` results apply to the same
runtime inputs. The compatibility table and README summaries need no update.

The prior compact-link measurements in `notes/mem-round3.md`, the rejected
alternatives in `notes/mem-dense-link-tables.md` and
`notes/perf-round2-followups.md`, `docs/RUST.md` Techniques, and the newest
`notes/mem-round4.md` were reviewed. This entry changes no runtime path or
asset. A fresh release build is byte-identical to source 89 (both SHA-256
`106a23c08ca5a6a6041e5d49ac28784d4e120b66d0d351a62aece14980f3d42a`),
so its public instruction, peak-RSS, and wall measurements apply unchanged:
source 90 introduces no runtime performance regression for the same inputs.
The dev CLI and testrunner build, warnings-denied Clippy, source lint, and
formatting pass. Generator freshness is checked after the commit because
the generator script rejects the intentional dirty checker comment. The
binary, hashes, and focused logs are in `target/port-623/0090-39b621793a/`.

## 91. `14f7762dde` — Recover concrete AST nodes from embedded headers

Upstream `14f7762dde8476906f84eb3a935268fd9d8c4420` removes the Go
`Node.data any` field and recovers each concrete node through the header at
offset zero, with kind-checked casts and constructors. Rust already
coallocates the AST header and typed payload in `#[repr(C)] NodeAlloc<T>`;
its 24-byte header precedes the payload at offset zero, and generated
`NodeDataTag` dispatch and checked casts avoid an interface field. The
missing safety behavior was rejection of invalid kinds passed to `new_token`
and the six generated constructors that accept a kind argument. The Rust
generator now derives canonical accepted kinds from the same schema ordering
as Go and asserts them before allocation. `new_token` rejects non-token
kinds; a preexisting header test now uses valid `Unknown` instead of
`Count`. Focused Rust tests cover every valid kind in the six families,
invalid kinds, cast rejection, and header offsets. Go's `Herebyfile.mjs`
coverage-only `-dwarf=false` flag has no Rust build equivalent. The ignored `ts-ref`
checkout advances to source 91; the published TypeScript pin remains at the
base revision until the full upgrade. No user-visible capability or pass
count changes, so the compatibility table and README summaries are unchanged.

The exact-source Go AST package, including its new factory-kind, layout,
cast, and payload-lifetime tests, passes. The dev CLI and testrunner build
passes. All 18 Go-history, 18 canonical, and 36 emit named compiler outcome
lists equal source 90's unchanged-runtime source 89 results: Go-history
retains 13,572 diagnostic and 12,876 type/symbol passes, canonical retains
13,572 and 12,875, and emit retains 13,502 JavaScript, 149 JavaScript-map,
and 156 source-map passes. Exact-source Fourslash has 4,091 exercised groups
passing, 470 skipped, and none failing. The full Rust workspace, 40 CLI
regressions, `wasm32-wasip1` check, warnings-denied Clippy, source lint, and
formatting pass. All generators were rerun; the generated AST output stayed
byte-identical to the formatted candidate. The Fourslash and workspace runs
used local Unix-socket access for their server tests.
Evidence, including the JavaScript outcome comparator and exact-source
Fourslash wrapper, is under `target/port-623/0091-14f7762dde/`.

Prior work was reviewed in `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, the newest
`notes/mem-round4.md`, and the source-84 AST port. No rejected redesign was
repeated: Rust's already compact allocation layout stays intact. Paired
local macOS release runs used source 90's frozen binary and the source 91
binary, five alternating pairs per public cell with matching exit codes and
output hashes. macOS `/usr/bin/time -l` includes kernel work in retired
instruction counts, so these are a local regression screen rather than the
repository's deterministic Linux instruction gate.

| public case | mode | instructions, source 90 → 91 | peak RSS MiB, source 90 → 91 | wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.2719G → 3.2720G (+0.004%) | 59.59 → 59.63 (+0.052%) | -0.10% |
| Compiler-Unions | default | 3.9644G → 3.9615G (-0.073%) | 83.58 → 83.20 (-0.449%) | -1.15% |
| webpack | single | 10.8344G → 10.8356G (+0.011%) | 311.59 → 311.61 (+0.005%) | +0.30% |
| webpack | default | 15.1066G → 15.2009G (+0.624%)* | 502.27 → 502.72 (+0.090%) | +1.67%* |
| VS Code | single | 81.5784G → 81.5708G (-0.009%) | 1613.63 → 1613.63 (0%) | -0.16% |
| VS Code | default | 93.7960G → 93.7902G (-0.006%) | 2109.94 → 2104.48 (-0.258%) | -0.81% |

*A reversed ten-pair webpack/default confirmation changed instructions to
15.2117G → 15.1904G (-0.140%), peak RSS to 503.70 → 502.61 MiB
(-0.216%), and wall time to -3.68%. The initial increase did not repeat;
there is no confirmed performance regression. The release binaries, raw
paired samples, output hashes, and both measurement batches are in the
evidence directory.

## 92. `56ce7acfc8` — Keep optional symbol tables out of hot symbol records

Upstream `56ce7acfc82bc4a465c0d65e7e18f6683cdf2f85` moves Go's `members`,
`exports`, and `exportSymbol` from every `symbolData` into an optional
`symbolExtra`. Binder and checker arenas batch those extra records; a write of
nil to a symbol without an extra record does not allocate one. The new Go
tests pin the hot-record layout and check lazy allocation, shared symbol data,
and retention after garbage collection. There is no compiler fixture, library,
or user-visible behavior change.

Rust already has the equivalent optional `SymbolTables` tail. The compressed
`Symbol` remains pinned at 32 bytes (40 bytes with plain 64-bit pointers),
and its `parent_or_tables` word holds the parent until a non-`None` optional
field needs the tail. The tail also holds the uncommon value declaration and
lazy member list. Nil setters leave an absent tail absent; `get_members` and
`get_exports` create tables on demand. Instantiated symbols share their data
owner's tail while retaining separate flags and identity. A new focused Rust
test verifies nil writes without tail allocation, parent preservation,
single-tail reuse, and reads and clears through a shared-data alias. Rust's
per-thread `P` arena owns the tail, so Go's explicit `SymbolExtraArena` has no
separate Rust counterpart. The ignored `ts-ref` checkout advances to this
exact source commit; the published TypeScript pin remains at the base revision
until the full upgrade. The compatibility table and README summaries do not
change.

The existing tail design and its earlier measured memory saving are recorded
in `notes/mem-small.md`. We also reviewed `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, and the newest
`notes/mem-round4.md`. The rejected frozen binder symbol-table design changes
the table representation and lookup cost; it is different from the already
shipped optional symbol tail. No new runtime layout or path is introduced
here.

Exact-source Go AST, binder, and checker package tests pass; the new Go symbol
layout, nil-write, and shared-extra tests are included. The focused Rust test
passes. Only Rust test code and this ledger change, so source 91's full 18
Go-history, 18 canonical, and 36 emit named outcome lists apply unchanged,
with no lost pass. Its exact-source Fourslash gate likewise remains 4,091
exercised groups passing, 470 skipped, and none failing. A fresh release CLI
build is byte-identical to source 91 (both SHA-256
`f52427e87b719039264ffc48a37bb205b1cd2dcb52b4b30bc1e7090da6cbb405`).
Thus source 91's paired public instructions-retired, peak-RSS, and wall-time
numbers apply exactly to this runtime: source 92 introduces no performance
regression for the same inputs.

The dev CLI and testrunner build, warnings-denied Clippy, source lint, and
formatting pass. The full workspace passes with its timing-sensitive macOS
FSEvents load test run separately; that test passes in isolation but dropped
one event in two otherwise passing full-workspace runs. The initial sandboxed
workspace run also could not create the API transport's Unix socket, so the
completed run used socket access. Release binary, package results, and gate
logs are under `target/port-623/0092-56ce7acfc8/`.

## 93. `82db71afa0` — Move optional structured type shape into cold storage

Upstream `82db71afa079da1e0c8b09a72914c0e3f70e85d8` moves signatures,
their call-signature count, index infos, and the cached type without abstract
construct signatures into a lazily allocated Go `structuredTypeExtra`. Its
new tests pin the smaller hot layout, absent-extra reads, writes, identity,
and retention across garbage collection. There is no compiler fixture,
library, or user-visible behavior change.

Rust already keeps the entire resolved shape, including members and
properties, behind a first-write `StructuredMembers` record. Its hot
`StructuredType` is one optional pointer: four bytes with compressed handles
or eight with plain pointers, both pinned by compile-time assertions. The
cold record is 32 bytes on 64-bit targets. Call-signature count and the
uncommon index-info tail share one word. The abstract-construct cache is a
checker-owned map populated only when needed. Adding Go's extra tail to the
Rust record would add an allocation and indirection without shrinking the
hot type. This port corrects the Go-layout comment and retains the already
measured Rust representation. The ignored `ts-ref` checkout advances to the
exact upstream commit; the published TypeScript pin remains at the base
revision until the full upgrade. The compatibility table and README
summaries do not change.

The existing layout and memory evidence are in `notes/mem-round3.md` and
`notes/mem-layout3.md`. We also reviewed `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected, and the newest
`notes/mem-round4.md`. Exact-source Go `./internal/checker` passes, including
its new structured-type layout and extra-record tests. Existing Rust layout
assertions and the payload-cast arena-growth test pass; the latter checks that
arena growth preserves a type payload's address.

Only a Rust comment and this ledger change; source 92's full 18 Go-history,
18 canonical, and 36 emit named compiler outcome lists apply unchanged,
with no lost pass. Its exact-source Fourslash result likewise remains 4,091
exercised groups passing, 470 skipped, and none failing. Source 92's full
workspace and 40 CLI regressions apply to the unchanged runtime. The release
CLI is byte-identical to source 92, so its paired public instructions-retired,
peak-RSS, and wall-time measurements apply unchanged: source 93 introduces
no performance regression for the same inputs (both SHA-256
`f52427e87b719039264ffc48a37bb205b1cd2dcb52b4b30bc1e7090da6cbb405`).
The dev CLI and testrunner
build, Rust checker unit tests, warnings-denied Clippy, source lint,
formatting, and generator freshness pass. Evidence is under
`target/port-623/0093-82db71afa0/`.

## 94. `158a0805fe` — Keep optional type-node and signature state out of hot records

Upstream `158a0805feea665f16bb7d58f0535afb18cc6930` moves a Go type
node's optional outer type parameters behind an arena-allocated slice header.
It also moves type parameters and the other optional signature fields into a
lazy `signatureExtra`; a bit represents the common resolved no-predicate
sentinel without allocating an extra record. Its new Go tests cover record
sizes, lazy allocation, nil versus non-nil empty slices, stable slice-header
identity, and garbage-collection retention. There is no compiler fixture,
library, or user-visible behavior change.

Rust's signature already had a rare tail for this parameter, isolated type,
composite signature, and resolved predicate, with the no-predicate bit in its
tail word. This port moves only `type_parameters` from every hot signature
into that tail, preserving an explicitly present empty list for cloning while
leaving an absent or empty constructor input unallocated. `TypeNodeLinks` now
holds an optional arena-owned `ThinSliceCell` pointer and reuses that cell on
subsequent non-nil writes; nil still means not computed and differs from
`Some(&[])`. The keyed-link record remains eight-byte aligned for compressed
array handles. The pinned hot sizes become 16 versus 24 bytes for
`TypeNodeLinks` and 48 versus 56 for `Signature` with compressed pointers on
64-bit targets; plain-pointer sizes become 24 versus 32 and 64 versus 72,
respectively. The rare signature tail grows from 16 to 24 bytes with
compressed pointers. The 32-bit layout assertions and WASM build pass.
Focused Rust tests exercise nil and empty lists, stable header and tail
identity, lazy allocation, predicate bit, clone independence, and arena
retention. The ignored `ts-ref` checkout advances to this exact source commit;
the published TypeScript pin stays at the base revision until the full upgrade.
The compatibility table and README summaries do not change.

Before implementing this layout, we reviewed `docs/RUST.md` Techniques,
`notes/perf-round2-followups.md` Measured and rejected,
`notes/mem-layout3.md`, `notes/checker-11.md`, and the newest
`notes/mem-round4.md`. None reject this specific Go-parity layout. The change
introduces neither a thread nor a cache nor a new phase invariant. Its measured
memory gain is smaller than the 5% standalone optimization bar in `AGENTS.md`;
the corresponding upstream port is required by this migration, and that
smaller gain is disclosed rather than presented as an independent optimization.

Exact-source Go checker tests, Rust checker unit tests, the dev CLI and
testrunner build, full workspace, exact-source Fourslash (4,091 exercised
groups passed, 470 skipped, none failed), 40 CLI regressions, warnings-denied
Clippy, source lint, formatting, and the release build pass. Full named
compiler outcome lists match source 93: 18 Go-history, 18 canonical, and 36
emit lists, with no lost pass. Go-history checker totals are 13,572 passing
diagnostic tests and 12,876 passing type and symbol baselines; emit totals are
13,576 diagnostics, 13,502 JS outputs, 149 JS maps, and 156 source maps.
The deterministic assignment check matches for xstate, webpack, and all 40
CLI cases (798 runs), but its full command exits nonzero because the Nuxt and
Drizzle benchmark checkouts are absent locally. The optional plain-pointer
build stops at an unchanged `InferenceInfo` size assertion outside this port;
the compressed and WASM layouts pass.

Five alternating release-binary pairs on each available public project and
checker mode recorded these median before/after values. Instructions are
billions retired from macOS `/usr/bin/time -l` (including kernel work); RSS is
peak MiB. All exit statuses and diagnostic hashes match within each pair.

| Project | Checkers | Instructions, before → after | Peak RSS, before → after | Wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | one | 3.27219 → 3.27177 (−0.013%) | 59.63 → 59.19 (−0.73%) | −0.15% |
| Compiler-Unions | default | 3.96291 → 3.96038 (−0.064%) | 83.39 → 82.73 (−0.79%) | +0.35% |
| webpack | one | 10.83467 → 10.83287 (−0.017%) | 311.61 → 309.27 (−0.75%) | +0.14% |
| webpack | default | 15.32852 → 15.35434 (+0.168%) | 507.53 → 503.41 (−0.81%) | +2.80% |
| vscode | one | 81.52743 → 81.55706 (+0.036%) | 1613.63 → 1603.67 (−0.62%) | −0.54% |
| vscode | default | 93.77953 → 93.66330 (−0.124%) | 2110.55 → 2097.06 (−0.64%) | −0.54% |

The webpack/default wall-time increase did not reproduce in a separate
reversed-order ten-pair batch: wall time changed −1.05%, RSS 502.52 → 499.34
MiB (−0.63%), and retired instructions 15.20156 → 15.25062 billion
(+0.323%). Individual instruction pair changes ranged from −2.05% to +4.12%,
so the kernel-inclusive count is noisy; the positive default-mode median is
disclosed, without claiming exact equality or a confirmed material
regression. The deterministic Linux `bench/count.mjs` gate was unavailable on
this macOS host. Raw pair measurements, outputs, release binaries, test logs,
and the JavaScript collector are under `target/port-623/0094-158a0805fe/`.

## 95. `e67b60e338` — Bound arena retention by bytes

Upstream `e67b60e3388a08368f09acbde2efaf8e67cb511a` changes only Go's
generic `core.Arena[T]` and adds three arena tests. Its growth limit is now
`min(2 * len, 256, max(512 / max(sizeof(T), 1), 1))` values. This targets
roughly 512 bytes before Go allocator size-class rounding: a surviving pointer
into a typed backing array otherwise keeps the entire array and its referenced
objects reachable to Go's garbage collector. The tests check large-value
retention, mixed single-value/slice allocation with stable addresses, and
zero-sized values. The exact-source Go `./internal/core` package passes,
including all three new tests.

Rust has no typed `Arena[T]` backing arrays or garbage-collector scanning of
interior pointers. `P<T>` allocates in the shared per-thread bump arena, while
explicitly owned regions release related objects together; its link stores
allocate values through that model. The Rust arena's separate 512-byte limit
is the maximum recycled-block size, not a chunk-retention limit. Shrinking
Rust chunks to Go's new size would not reproduce the Go GC fix and would
replace the measured Rust allocation design. This is therefore an
evidence-only port, with no Rust code, fixture, library, generator, or
user-visible capability change. The ignored `ts-ref` checkout advances to
this exact source commit; the published TypeScript pin remains at the base
revision until the full upgrade. The compatibility table and README summaries
stay unchanged.

We reviewed `docs/RUST.md` Techniques, the rejected ideas in
`notes/perf-round2-followups.md`, the newest `notes/mem-round4.md`, and the
arena/recycling evidence in `notes/mem-recycle.md` and
`notes/mem-linux-residency-32.md`. Rust's focused arena suite passes nine
tests; its deliberate Miri negative control remains ignored. The dev CLI and
testrunner build, warnings-denied Clippy, source lint, formatting, and
generator freshness pass. Because source 95 changes no Rust compiler inputs,
source 94's full 18 Go-history, 18 canonical, and 36 emit named compiler
outcome lists apply unchanged, with no lost pass. Its exact-source Fourslash
result remains 4,091 exercised groups passing, 470 skipped, and none failing;
its full workspace and 40 CLI regressions also apply to the identical runtime.

The source 95 release CLI is byte-identical to source 94 (both SHA-256
`ee7bf17ed86f870d4e042afe220855b2d745fa4e080946a2e6958a48378f0e99`).
Thus source 94's public paired instructions-retired, peak-RSS, and wall-time
measurements above apply unchanged to the same inputs: source 95 introduces no
Rust performance regression. Logs and the binary comparison are under
`target/port-623/0095-e67b60e338/`.

## 96. `e36179bde8` — Use paged indexes for type-node and signature caches

Upstream `e36179bde849072301c41d1090c4d75616cd3c47` switches the checker's
`typeNodeLinks` and `signatureLinks` from pointer-keyed maps to `nodeLinkStore`.
That store assigns a checker-local reserved node ID on first access, indexes
ordinary preassigned IDs separately, and distinguishes an absent link from an
untouched cell of an allocated page. Go's new test covers 4,096 nodes with
alternating preassigned IDs, two independent checkers, `Has` before `Get`,
retention across arena release and garbage collection, and an absent clone.

Rust now uses `NodeValueLinkStore` for those two caches. It reuses Go-compatible
`NodeIdGenerator` assignment, preserves per-slot absence and stable arena
addresses, falls back to an ordinary-ID map, and copies frozen shared-graph
links into a fork on first write. The old hash-only key fields leave
`TypeNodeLinks` and `SignatureLinks`; their compressed records shrink from
16 to 8 and 16 to 12 bytes, respectively (the `P` arena rounds the latter
allocation to 16). The existing inline `NodeLinkStore` remains appropriate for
its four-byte `SymbolNodeLinks` value but could not serve these stores: it
reports every cell in an allocated group as present. A focused Rust test mirrors
the upstream mixed-ID, two-checker and clone cases; both it and exact-source
Go `TestCheckerNodeLinkStores` pass. A `tsrs_core/shared-graph` focused test
also verifies that a fork reads its seed link and copies it before mutation.

The first implementation used Go-sized 256-id pages. In a reversed ten-pair
public VS Code/default run its peak RSS rose 1.34% against source 95, while
instructions fell 0.25%. Rust therefore uses 32-id value-pointer groups behind
a 1,024-id two-level index, preserving the Go lookup semantics and avoiding
allocation for neighboring untouched groups. This is an internal page-size
deviation made to prevent the measured multi-checker memory regression. It
differs from the dense-link-table ideas rejected in
`notes/mem-dense-link-tables.md` and `notes/perf-round2-followups.md`: at that
time almost none of these pointer-keyed records had node IDs, and assigning IDs
would change Go-visible numbering. This upstream commit explicitly assigns the
IDs; the smaller groups also avoid the rejected large sparse pages. We reviewed
those notes, `docs/RUST.md` Techniques, and the newest `notes/mem-round4.md`
before the layout change.

The ignored `ts-ref` checkout advances to this exact source commit. The
published TypeScript pin stays at the base revision until the complete upgrade.
No fixture, library, user-visible capability, or compatibility count changed;
the capability table and README summaries remain current. All 18 Go-history,
18 canonical, and 36 emit named compiler outcome lists match source 95 exactly,
with no lost pass, crash or timeout. Go-history checking has 13,572 diagnostic
and 12,876 type/symbol passes; canonical has 13,572 and 12,875. Emit has
13,502 JavaScript, 149 JavaScript-map, and 156 source-map passes. Exact-source
Fourslash passes 4,091 exercised groups, skips 470, and fails none. The full
Rust workspace, 40 CLI regressions, `wasm32-wasip1` check, warnings-denied
Clippy, source lint, and formatting pass. The workspace and Fourslash runs
needed local Unix-socket access after the sandbox denied socket creation.

Paired local macOS release runs compare source 95's frozen executable with
source 96, five alternating pairs per public cell. All exit statuses and
diagnostic hashes match. Instructions below are billions retired from macOS
`/usr/bin/time -l` (including kernel work), RSS is peak MiB, and wall time is
the paired process duration:

| Public case | Checkers | Instructions, before → after | Peak RSS, before → after | Wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | one | 3.27180 → 3.26909 (−0.083%) | 59.20 → 59.25 (+0.079%) | −0.16% |
| Compiler-Unions | default | 3.96420 → 3.95844 (−0.145%) | 83.09 → 82.34 (−0.903%) | +1.23% |
| webpack | one | 10.82855 → 10.80580 (−0.210%) | 309.25 → 306.67 (−0.834%) | +0.98% |
| webpack | default | 15.18968 → 15.12131 (−0.450%) | 499.02 → 494.20 (−0.964%) | −0.48% |
| VS Code | one | 81.41974 → 82.27369 (+1.049%)* | 1603.67 → 1586.80 (−1.052%) | −5.63% |
| VS Code | default | 93.82296 → 93.56103 (−0.279%) | 2073.67 → 2079.02 (+0.258%) | −8.55% |

*The VS Code/one instruction increase did not repeat in an independent
reversed ten-pair run: instructions −0.183%, RSS −1.052%, wall −1.46%.
Another reversed ten-pair VS Code/default run measured instructions −0.242%,
RSS +0.064%, wall −1.95%. The Linux deterministic `bench/count.mjs` counter
was unavailable on this macOS host; kernel-inclusive instruction and wall
readings can vary across runs. The final layout has no confirmed material
performance regression. Both candidate and final release binaries, raw
paired samples, output hashes, exact-source Go and Rust gates, and the
JavaScript comparator/collector are under `target/port-623/0096-e36179bde8/`.
The final measured release binary has SHA-256
`1f432c5d028b43491e2f8c92c9a0fb2e82841e874cfac4399924a3daab2c111a`.

## 97. `6c976dad5d` — Reuse ordered property counts during intersection reduction

Upstream `6c976dad5d65d2c67f5707f3ba4a04ae991b8481` pools the Go
checker's ordered property-count map used by `somePropertyReducesToNever`.
Its new tests cover zero allocations after warmup, ordered and isolated nested
borrows, reuse after clearing, and dropping an unusually large table. All
three new tests pass in the exact-source Go checker.

The Rust checker already implemented this optimization in `3525a693e`
(#327). Its per-checker `name_counts_pool` lends a separate `NameCounts` to
each nested call; the entries retain first-occurrence order, and the table
and vectors reuse their allocations. The existing custom open-addressed table
also avoids a heap allocation for each property name. A focused Rust
regression now checks counting order, isolation while an inner table is in
use, and reset/reuse of the outer table. No Rust production code or test
fixture changes in this port. Go discards tables with more than 4,096 names
to avoid retaining GC references; Rust's names live in a checker arena and
its existing pool retains its high-water buffers. The memory census in
`notes/mem-round4.md` finds the checker's free lists at no more than 6 KiB,
so the Go retention cutoff has no demonstrated benefit to transfer here.

Before this port we reviewed `docs/RUST.md` Techniques, the measured and
rejected list in `notes/perf-round2-followups.md`, and the newest
`notes/mem-round4.md`. An earlier count-pool candidate measured −0.88%
instructions on mui-docs and −0.83% on mikro-orm but was initially left
out of the heap-churn work; it later landed as part of #327. That round
measured −0.6% instructions for the pooled name counts on a large private
monorepo. This upstream commit therefore maps to an optimization already
adopted in tsrs, rather than reviving a rejected experiment.

The ignored `ts-ref` checkout advances to this exact source commit. Its
test corpus and baselines are unchanged; the published TypeScript pin stays
at the base revision until the complete upgrade. No user-visible capability
or compatibility count changes. The dev CLI and testrunner build, focused
Go and Rust tests, warnings-denied Clippy, source lint, and formatting pass.
Only a test-only Rust module and this ledger changed, so source 96's full
18 Go-history, 18 canonical, and 36 emit named compiler outcome lists apply
unchanged, with no lost pass. Its exact-source Fourslash result remains
4,091 exercised groups passing, 470 skipped, and none failing; its full
workspace and 40 CLI regressions also apply to the identical runtime.

The source 97 release CLI rebuild is byte-identical to source 96 (both
SHA-256 `1f432c5d028b43491e2f8c92c9a0fb2e82841e874cfac4399924a3daab2c111a`).
Thus source 96's paired public instructions-retired, peak-RSS, and wall-time
measurements above apply unchanged to the same inputs: source 97 introduces
no Rust performance regression. Focused test logs, validation logs, and
the binary comparison are under `target/port-623/0097-6c976dad5d/`.

## 98. `f5cdce998e` — Reparse JSDoc generic heritage on expressions

Upstream `f5cdce998e1b35ec1e226101e65b6b05d6993e74` fixes JavaScript
classes such as `/** @extends {A<string>} */ class B extends A.extend() {}`.
The parser now clones `@extends` type arguments onto a single heritage element
when its expression is a call, mixin, element access, conditional expression,
or parenthesized name. Named heritage still requires a matching JSDoc name,
and `extends null` remains unaugmented. The Rust parser follows the Go
predicate and preserves the cloned nodes' `Reparsed` flags, parents, source
text, and JSDoc-to-reparse mapping. The checker now obtains the base
constructor before checking the tag, compares type identity for named bases
and assignability from the actual base to the JSDoc type for expression
bases, and reports the full expression in TS8023. Declaration emit uses the
actual heritage element directly; its removed `getEffectiveBaseTypeNode`
wrapper had no remaining JSDoc behavior. No algorithmic deviation from Go
was needed.

Exact-source Go `TestJSDocAugmentsHeritage` and the matching ten-case Rust
parser test pass. All 15 focused `extendsTag` compiler variants match error,
type, symbol, and JavaScript baselines, including eight new upstream fixtures
(`extendsTag7`–`extendsTag14`). The ignored `ts-ref` checkout advances to this
source commit; the published TypeScript pin stays at the base revision until
the complete upgrade. The capability table records the behavior and updated
counts. The root and npm READMEs have no conflicting numerical summary.

All 18 Go-history, 18 canonical, and 36 emit outcome lists were compared to
source 97's unchanged source-96 runtime. Their only new names are the eight
passing `extendsTag` fixtures; no pass is lost, and no new failure, crash, or
timeout appears. Go-history checking now has 13,580 diagnostic and 12,884
type/symbol passes; canonical has 13,580 and 12,883. Emit has 13,584
diagnostic, 12,884 type/symbol, 13,510 JavaScript, 149 JavaScript-map, and
156 source-map passes. Exact-source Fourslash passes 4,091 exercised groups,
skips 470, and fails none. The dev CLI and testrunner build, full Rust
workspace, CLI regressions, `wasm32-wasip1` check, warnings-denied Clippy,
source lint, and formatting pass. Fourslash needed local Unix-socket access
after the sandbox denied the first subprocess test's bind.

Five alternating paired macOS release runs per public cell compared source
97's frozen binary with source 98. Every pair retained the same exit status
and diagnostic hash. Instructions below are billions retired from
`/usr/bin/time -l`, including kernel work; RSS is peak MiB, and wall is the
paired process duration:

| Public case | Checkers | Instructions, before → after | Peak RSS, before → after | Wall change |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | one | 3.26942 → 3.26919 (−0.007%) | 59.23 → 59.27 (+0.053%) | −0.03% |
| Compiler-Unions | default | 3.95571 → 3.95810 (+0.060%) | 82.06 → 82.06 (0%) | −0.09% |
| webpack | one | 10.80919 → 10.80932 (+0.001%) | 306.67 → 306.69 (+0.005%) | −0.43% |
| webpack | default | 15.20385 → 15.18598 (−0.118%) | 497.92 → 497.55 (−0.075%) | +0.98% |
| VS Code | one | 81.47641 → 81.48135 (+0.006%) | 1586.80 → 1586.80 (0%) | +0.25% |
| VS Code | default | 93.55789 → 93.51093 (−0.050%) | 2074.91 → 2084.98 (+0.486%) | −0.05% |

The largest instruction increase is 0.060%, and the largest peak-RSS
increase is 0.486%; no material performance regression was detected. The
Linux deterministic `bench/count.mjs` counter was unavailable on this macOS
host. Frozen release binaries, every paired sample and output hash, exact
Go and Rust results, and the JavaScript measurement collector are under
`target/port-623/0098-f5cdce998e/`.

## 99. `d9c578ddb4` — Update localization files

Upstream `d9c578ddb44a9e8dd0791a58c364f433c0011766` adds the existing
diagnostic key `A_deferred_import_must_specify_a_namespace_binding_18117` to
the `pt-BR`, `tr-TR`, and `zh-TW` OneLoc handbacks and their runtime gzip
archives. No other key or translation changes. The six files under
`crates/tsrs_diagnostics/loc/` match their upstream source-99 counterparts
byte for byte, including CRLF JSON and deterministic gzip headers and
payloads. The other ten locale pairs remain at source 61. The localization
README and capability table record those revisions; `--locale` still leaves
tsrs diagnostics in English because the Rust compiler does not load these
catalogs. The ignored `ts-ref` checkout advances to source 99 while the
published TypeScript pin stays at the base revision until the full upgrade.

The localization checker validates all 13 pairs against 2,228 Rust English
diagnostics; its two importer cases and the exact-source Go diagnostics
package test pass. The dev CLI and testrunner build, SDK-only build,
WebAssembly target check, warnings-denied Clippy, source lint, formatting,
and generator checks pass. The generator was rerun after staging the six
intended locale files so its final unstaged-diff guard could detect genuine
generator drift. The SDK source sync check still reports the preexisting
source-79 `outputExtensions` addition to
`npm/tsrs/src/vscode/extensionApi.ts` as one line beyond the recorded SDK
source (`2d8450f9b54a2a9688a1823a57102402d6862ef5`); this localization
port makes no SDK edit.

All full compiler outcome lists are byte-identical to source 98: 18
Go-history checker, 18 canonical checker, and 36 emit lists. Go-history
checking retains 13,580 diagnostic and 12,884 type/symbol passes; canonical
retains 13,580 and 12,883. Emit retains 13,584 diagnostic, 12,884
type/symbol, 13,510 JavaScript/declaration, 149 JavaScript-map, and 156
source-map passes. There is no lost pass, new failure, crash, or timeout.
Upstream changed no Fourslash fixture, Go source, or English diagnostic, and
this port changes no Rust runtime code. Source 98's exact-source Fourslash
result (4,091 passing exercised groups, 470 skipped, none failing) and
workspace/regression results therefore apply to the same inputs and runtime.

The release CLI rebuilt byte-identically to the frozen source-98 binary:
both have SHA-256 `d6cec8cc11d097da9a71b21c39d8f3b87199ae37abf0fb601c192c10b65fb946`.
No source-99 runtime instruction, wall-time, or peak-RSS regression is
possible for an identical executable on the same public inputs; source 98's
paired instruction and peak-RSS measurements remain applicable. The
byte comparisons, complete conformance lists, validation logs, and release
hashes are under `target/port-623/0099-d9c578ddb4/`.

## 100. `76c4f5078b` — Fast-path comparisons of sibling AST nodes

Upstream `76c4f5078b1e77cc2ac54a18b2cc79e346a1d7a0` adds a
`compareNodes` fast path for distinct nodes with the same non-nil parent:
their positions determine the order without walking to both source files.
It also replaces subtraction of two symbol IDs with `cmp.Compare`, preserving
correct ordering when the difference would overflow `int`. Rust already has
both equivalents in `crates/tsrs_checker/src/utilities.rs`: `compare_nodes`
orders sibling nodes by `pos().cmp`, and `compare_symbols_worker` uses the
`SymbolId` `Ord` comparison. The sibling fast path landed in `3525a693`
before this migration. Rust additionally excludes `SourceFile` nodes from
the fast path to preserve cross-file program order if they ever share a
parent; ordinary siblings follow the exact Go rule. No Rust runtime change
or new fixture is needed. The ignored `ts-ref` checkout advances to this
commit; the published TypeScript pin remains at the base revision until the
complete upgrade. No user-visible capability or pass count changes.

The existing implementation was measured in `notes/perf-icount-round4.md`
at −0.39% checker instructions in the deterministic 8-checker locality mode
on a large private monorepo. That is prior evidence for the already-landed
code, not an additional source-100 gain. We reviewed `docs/RUST.md`
Techniques, `notes/perf-round2-followups.md` Measured and rejected, and the
newest `notes/mem-round4.md`; no rejected optimization is being retried.
Exact-source Go checker package tests and all 20 Rust checker unit tests
pass. The dev CLI and testrunner build, warnings-denied Clippy, source lint,
formatting, and generator freshness checks pass.

The full compiler suite was rerun at source 100. Its 18 Go-history, 18
canonical, and 36 emit named outcome lists are byte-identical to source 99,
with no lost pass, new failure, crash, or timeout. Go-history checking
retains 13,580 diagnostic and 12,884 type/symbol passes; canonical retains
13,580 and 12,883. Emit retains 13,584 diagnostic, 12,884 type/symbol,
13,510 JavaScript/declaration, 149 JavaScript-map, and 156 source-map passes.
Source 98's exact-source Fourslash result, unchanged through source 99 and
with no new source-100 fixture, remains 4,091 exercised groups passing,
470 skipped, and none failing. The rebuilt release CLI is
byte-identical to source 99 (SHA-256
`d6cec8cc11d097da9a71b21c39d8f3b87199ae37abf0fb601c192c10b65fb946`).
Thus source 99's public paired instructions-retired, peak-RSS, and wall-time
measurements apply to source 100 on identical inputs; this port introduces
no Rust performance regression. Logs and the binary comparison are under
`target/port-623/0100-76c4f5078b/`.

## 101. `257e87406c` — Private API pipe creation

Upstream `257e87406c6a8be277cb8cd5ef8d7b458a650390` changes Unix
`--api --pipe <path>` from an ordinary socket at `<path>` to a newly created,
private directory containing `<path>/socket`, with directory mode `0700` and
socket mode `0600`. Existing paths now fail rather than being removed. The
Rust listener follows that contract, exposes an unpredictable private-path
constructor, and cleans up only the directory and
socket it created, preserving replacements. The Windows listener now creates
a current-user-only named pipe with a protected DACL and rejects an existing
pipe instance. The Node sync channel uses a random 128-bit suffix in its
Windows pipe name. The CLI help, generated LSP wire comment, SDK manifest,
and API documentation reflect the new path contract. Rust's LSP
`custom/initializeAPISession` handler remains an earlier unported stub, so
that LSP-provided API-pipe lifecycle is not available yet. The published
TypeScript pin remains at the base until the full migration finishes.

Exact-source Go IPC tests pass. Native Unix transport tests cover modes,
existing-path failure, cleanup, replacement preservation, and the pinned
Node client round trip. The Windows transport and its native security and
ping-pong test compile for `x86_64-pc-windows-msvc`; CI now runs that test on
Windows. Native Windows execution was unavailable on this host. The dev CLI
and testrunner build, workspace and regression suites, 956 SDK API source
tests, package/consumer smoke tests, WebAssembly target check, warnings-denied
Clippy, source lint, formatting, and generated-code freshness checks pass.

All 18 Go-history, 18 canonical, and 36 emit named outcome lists are
byte-identical to source 100, with no lost pass, new failure, crash, or
timeout. Go-history checking retains 13,580 diagnostic and 12,884
type/symbol passes; canonical retains 13,580 and 12,883. Emit retains
13,584 diagnostic, 12,884 type/symbol, 13,510 JavaScript/declaration, 149
JavaScript-map, and 156 source-map passes. The exact-source Fourslash gate
has 4,091 exercised groups passing, 470 skipped, and none failing.

Public paired performance measurements compare frozen source-100 and
source-101 release binaries on Compiler-Unions, webpack, and VS Code in
single-threaded and default modes. Every output hash matches between the
two binaries. The initial five alternating pairs per cell give these
instruction, wall-time, and peak-RSS medians:

| Project | Mode | Instructions, billions before → after | Wall seconds before → after | Peak RSS MiB before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.27050 → 3.27011 | 0.250 → 0.244 | 59.25 → 59.14 |
| Compiler-Unions | default | 3.95910 → 3.96317 | 0.108 → 0.110 | 82.03 → 82.02 |
| webpack | single | 10.80813 → 10.80823 | 0.894 → 0.938 | 306.69 → 306.45 |
| webpack | default | 15.19337 → 15.20910 | 0.165 → 0.169 | 497.67 → 498.31 |
| VS Code | single | 81.26311 → 82.29728 | 7.867 → 7.846 | 1586.80 → 1586.63 |
| VS Code | default | 93.54570 → 93.70511 | 0.995 → 0.995 | 2089.28 → 2081.83 |

The initial VS Code single instruction median crossed a bimodal boundary:
both binaries recorded samples near 81.2 and 82.3 billion instructions. A
separate reversed ten-pair confirmation gives 81.43219 → 81.49411 billion
instructions (+0.076%), 7.721 → 7.735 seconds (+0.181%), and 1586.80 →
1586.63 MiB peak RSS. This disconfirms the initial 1.273% instruction
increase as a stable source-101 effect. Raw measurements and the JavaScript
collector are under `target/port-623/0101-257e87406c/perf/`.

Reversed ten-pair webpack confirmations likewise reverse the initial
wall-time increases: single-threaded is 0.895 → 0.889 seconds (−0.725%),
and default is 0.162 → 0.161 seconds (−0.648%). Their instruction changes
are +0.0006% and −0.305%, and their peak-RSS changes are −0.076% and
−0.208%. No material source-101 performance regression is detected on this
public cohort. The Linux deterministic `bench/count.mjs` instruction counter
was unavailable on this macOS host.

## 102. `655f0268af` — Avoid WeakSet for executed SDK generators

Upstream `655f0268afaa43cde07b65b39782688463a99517` changes the sync
SDK's generator scheduler to mark executed extensible generator objects with a
private symbol rather than grow a `WeakSet` for every batch. Non-extensible
generators use a module-wide fallback `WeakSet`. The mark persists across
`API.batch` calls, so reusing the same generator instance after completion now
throws the upstream duplicate-generator error. The two upstream tests change
their cross-executor expectations accordingly. We synced the exact upstream
implementation and adapted tests into `npm/tsrs` (192 managed files verified
by `sync-upstream.mjs`), with no tsrs-specific SDK patch. The upstream
`api-comparison.bench.ts` is outside the copied test set by design; its two
100,000-generator long-batch modes were reproduced in the ignored JavaScript
benchmark artifact. The published TypeScript pin remains at the base.

The focused generator suite passes 44/44, and all 956 SDK source tests pass.
Additional direct checks cover frozen native generators, non-extensible
deferred wrappers, and nested `all` wrappers on the fallback path. The SDK
build, packed consumer smoke (25/25), warnings-denied Clippy, source lint,
formatting, dev CLI/testrunner build, and full Rust workspace suite pass. The
first workspace run was blocked by the sandbox's Unix-socket bind denial in
source-101 transport tests; the rerun with socket access passes. The
generated-code freshness check also passes. The upstream commit changes no `tsc`
compiler file, and this tsrs commit changes no Rust source or
compiler input. Source 101's full 18 Go-history, 18 canonical, and 36 emit
named outcome lists and exact-source Fourslash result (4,091 pass, 470 skip,
no failure)
therefore apply unchanged. The source 101 pass counts and public compiler
performance measurements remain applicable. We reviewed `docs/RUST.md`
Techniques, `notes/perf-round2-followups.md` Measured and rejected, and the
latest `notes/mem-round4.md`; this SDK change does not retry a rejected Rust
optimization.

On Node 24/macOS, ten alternating before/after process pairs per mode ran
the same 100,000-generator direct and 100-round `yield* defer` workloads
against byte-frozen source-101 and source-102 scheduler files. A small
1,000-generator setup precedes each timed batch. `/usr/bin/time -l`
instructions and peak RSS include the Node process and setup; wall time
measures the large batch itself. Medians are:

| SDK workload | Instructions retired, billions before → after | Peak RSS, MB before → after | Batch wall time, ms before → after |
| --- | ---: | ---: | ---: |
| Direct | 2.02263 → 2.03534 (+0.63%) | 175.37 → 172.63 (−1.56%) | 37.34 → 33.25 (−10.94%) |
| `yield* defer` | 1.73088 → 1.46786 (−15.20%) | 112.06 → 110.16 (−1.70%) | 30.50 → 18.42 (−39.61%) |

Direct-mode retired instructions rise by 0.63%, below the 1% threshold,
while its elapsed time and memory improve; deferred mode improves all three
measures. The raw pairs and JavaScript collectors are under
`target/port-623/0102-655f0268af/perf/`. The local JS workload is not a
Linux `bench/count.mjs` compiler-project measurement, and no two-publish
wall-time claim follows from it. No material performance regression was
detected in this SDK cohort, and the Rust compiler runtime is unchanged.

## 103. `0ee02846fb` — Keep version substitutions aligned in tsc test baselines

Upstream `0ee02846fb1e4f7710c396ff42e4289f81f1a6ba` fixes the Go
`tsctests` output sanitizer: replacing the compiler version with
`FakeTSVersion` must preserve the width of a space-padded `--help` header,
including the 75-column and 115-column logo forms. It adds six sanitizer
cases for English and Czech and corrects one Go help-output baseline. We
ported the substitution to `tsrs_execute`'s test-only English sanitizer and
added the six matching English cases, including repeated versions and both
logo widths. Czech message localization and the full `tsc --help` output
remain outside tsrs's documented scope; the CLI still prints its short usage.
The source commit also adjusts four Microsoft Azure release-branch workflows
to run token cleanup after a failure and requests issue-write permission for
their app. tsrs has no corresponding Azure release-branch jobs, so this port
does not add unrelated workflow conditions or permissions. The ignored
`ts-ref` checkout advances to the exact upstream commit; the published
TypeScript pin remains at the base until the migration finishes.

The exact-source Go `tsctests` package and focused sanitizer test pass. The
Rust sanitizer's six-case test passes, as do all 14 `tsrs_execute` unit tests,
the dev CLI/testrunner build, warnings-denied Clippy, source lint, and
formatting. The Rust tsc-scenario replay has no generated dump in this
checkout; it was not counted as baseline coverage. Full compiler runs against
source 103 pass with no new loss: the 18 Go-history, 18 canonical, and 36 emit
named outcome lists are byte-identical to source 101 (source 102 changed no
compiler input). Go-history and canonical checking each retain 13,580
diagnostic passes, with 12,884 and 12,883 type/symbol passes respectively.
Emit retains 13,584 diagnostic, 12,884 type/symbol, 13,510
JavaScript/declaration, 149 JavaScript-map, and 156 source-map passes.
Source 101's exact-source Fourslash result (4,091 pass, 470 skip, no failure)
also applies because sources 102 and 103 change no Fourslash input or Rust
language-service code.

The Rust edit is confined to `tsctests`, a module behind `#[cfg(test)]`.
`cargo build --release --locked -p tsrs_cli` succeeds, and the release CLI's
SHA-256 is identical before and after rebuilding
(`30025581b2d146d92b50b42d91e9d4c3e4b86e693994a062fdf92ee0b3893938`).
The checker instruction count, wall time, and peak RSS therefore cannot
change from this port; source 101's public compiler performance evidence
carries through without a new runtime measurement. Outcome lists and the
JavaScript comparison script are under `target/port-623/0103-0ee02846fb/`.

## 104. `ce30dd50a5` — Ban network paths wholesale

Upstream `ce30dd50a5dd0b81b83538c591b17347eca422d5` refuses UNC,
Win32 device, and NT object path roots before OS filesystem access because a
Windows network path can authenticate to an arbitrary host. The Rust port
adds the same byte-prefix check to the shared path utilities and applies it
to OS-backed reads, metadata, writes and removals, realpath and reparse-point
lookups, filesystem watcher subscriptions, and LSP watcher registration.
Watcher batches roll back already prepared subscriptions when a later path is
rejected; a mapped-drive realpath retains its local spelling. Config parsing
rejects rooted network option values, `extends`, explicit files and project
references, and remote include/exclude specs; command-line rooted options
receive the same check. The compiler reports new TS5116 for network
triple-slash references and `paths` substitutions. File-spec `/??/*.ts`
remains a valid local wildcard, and normalization can remove an `/??/`
prefix before access. Profiling remains unimplemented, so the upstream
profile-directory guard has no Rust counterpart. The published TypeScript
pin remains unchanged until the final migration entry.

Exact-source Go network-path, OS VFS, filesystem-watcher, project-watcher,
config/response-file, and `networkPathReferences` compiler tests pass. Rust
path, VFS, watcher, config/response-file, and project-watcher unit tests pass;
the new compiler fixture matches its diagnostic and JavaScript baselines.
The dev CLI/testrunner build, full workspace suite, all 40 regression cases,
WebAssembly target check, exact-source Fourslash
(4,091 exercised groups passing, 470 skipped, none failing), warnings-denied
Clippy, source lint, formatting, and generated-code regeneration pass. The
initial Fourslash and workspace runs were denied local Unix-socket binding
by the sandbox; reruns with socket access passed.

All 18 Go-history, 18 canonical, and 36 emit named outcome lists preserve
every source-103 pass with no new failure, crash, or timeout. Only
`compiler/networkPathReferences` is added: it passes diagnostics and
JavaScript output and intentionally skips type/symbol and map baselines.
Go-history and canonical checking each pass 13,581 diagnostic baselines,
with 12,884 and 12,883 type/symbol passes respectively. Emit passes 13,585
diagnostic, 12,884 type/symbol, 13,511 JavaScript/declaration, 149
JavaScript-map, and 156 source-map baselines. The complete outcome lists and
their JavaScript comparison are under `target/port-623/0104-ce30dd50a5/`.

Five alternating release-binary pairs per public project and checker mode
compare frozen source-103 and source-104 binaries. Every exit status and
output hash matches. macOS `/usr/bin/time -l` supplies instructions retired
and peak RSS; wall time includes process launch:

| Project | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.26988 → 3.22081 | 59.13 → 59.08 | 0.238 → 0.224 |
| Compiler-Unions | default | 3.95636 → 3.90783 | 82.17 → 81.80 | 0.106 → 0.106 |
| webpack | single | 10.80858 → 10.80388 | 306.44 → 306.45 | 0.898 → 0.912 |
| webpack | default | 15.23556 → 15.23900 | 499.83 → 500.63 | 0.169 → 0.163 |
| VS Code | single | 81.32857 → 81.26917 | 1586.63 → 1586.66 | 7.524 → 7.578 |
| VS Code | default | 93.59376 → 93.64996 | 2081.19 → 2086.81 | 0.989 → 0.993 |

The largest instruction increase is +0.060% and the largest peak-RSS
increase is +0.270%; no material regression is detected in this cohort.
The Compiler-Unions instruction decreases are measured binary effects, not
evidence that network-path checks cause a speedup. The deterministic Linux
`bench/count.mjs` counter was unavailable on this macOS host. Raw runs,
matching output hashes, and the JavaScript collector are retained under
`target/port-623/0104-ce30dd50a5/perf/`.

## 105. `bb9afd2bbf` — Disable ATA when no user cache is available

Upstream `bb9afd2bbf9f35481c6b755259f7103baa73b3a4` makes the global
typings-cache location empty when the user cache cannot be found or is not an
absolute path, instead of falling back to the temporary directory. The Rust
lookup now lives with the OS VFS, normalizes a valid absolute path, rejects
URL query/fragment suffixes, and preserves an empty location through the LSP
entry point. The empty location already prevents a typings watcher from being
created. Rust still does not install typings automatically, so the new Go ATA
test has no installer counterpart; the compatibility table continues to mark
ATA unported. Deterministic Rust unit cases cover valid, missing, relative,
and URL-suffixed cache paths without mutating process environment.

The draft PR's native Windows API transport test exposed an unrelated SDDL
presentation difference from source 101: a named-pipe handle can report
`FA` (file all access) where the generated descriptor uses `GA` (generic
all access). The test now canonicalizes those equivalent ACE rights before
comparing the protected, single-entry current-user DACL. The Windows test
target compiles under `x86_64-pc-windows-msvc`; it could not be executed on
this macOS host. Cross-target Clippy still reports preexisting raw-pointer and
unsafe-comment lints in the Windows transport module; the required
host all-target Clippy run passes.

The exact-source Go OS VFS and ATA packages pass, including the newly added
cache and no-install tests; the changed Go LSP and CLI test packages compile.
The Rust cache unit test, dev CLI/testrunner build, full workspace,
40 regression cases, WebAssembly target check, exact-source Fourslash
(4,091 exercised groups passing, 470 skipped, none failing), warnings-denied
host Clippy, source lint, and formatting pass. The Fourslash and workspace
tests needed local Unix-socket access after sandbox binding denials. Generated
code is unchanged. The ignored `ts-ref` checkout advances to source 105;
the published TypeScript pin stays at the base revision until the full
migration.

All 18 Go-history, 18 canonical, and 36 emit compiler named outcome lists
are byte-identical to source 104. Go-history and canonical each retain 13,581
diagnostic passes and 12,884 and 12,883 type/symbol passes, respectively.
Emit retains 13,585 diagnostic, 12,884 type/symbol, 13,511
JavaScript/declaration, 149 JavaScript-map, and 156 source-map passes.
There is no lost pass or new failure, crash, or timeout. The exact results,
comparison script, Fourslash results, and release measurements are under
`target/port-623/0105-bb9afd2bbf/`.

Five alternating release-binary pairs per public project and checker mode
compared frozen source 104 and source 105 binaries. Every exit status and
output hash matched. macOS `/usr/bin/time -l` provided retired instructions
and peak RSS; wall time includes process launch:

| Project | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.22043 → 3.22038 | 59.09 → 59.14 | 0.225 → 0.224 |
| Compiler-Unions | default | 3.91523 → 3.91152 | 82.17 → 82.05 | 0.106 → 0.107 |
| webpack | single | 11.01242 → 11.02980 | 306.44 → 306.50 | 0.911 → 0.921 |
| webpack | default | 15.20569 → 15.38212 | 497.41 → 500.22 | 0.189 → 0.195 |
| VS Code | single | 81.40750 → 81.41118 | 1586.66 → 1586.69 | 7.701 → 7.710 |
| VS Code | default | 93.70866 → 93.71087 | 2087.08 → 2075.31 | 1.129 → 1.139 |

The five-pair webpack/default cell initially rose 1.160% in instructions,
3.391% in wall time, and 0.565% in RSS. A reversed ten-pair confirmation
measured +0.698%, +2.879%, and +0.906%; a second ten-pair confirmation
measured +0.070%, −0.864%, and +0.236%. The signal did not hold across
confirmations, and no material regression was confirmed. Source 105 changes
only LSP startup behavior plus the Windows-only test, not compiler work.
The Linux deterministic `bench/count.mjs` counter was unavailable on this
macOS host. All raw paired measurements and output hashes are retained in
`perf/`, `perf-confirmation/`, and `perf-confirmation-2/`.

## 106. `0e081d6fb3` — Escape quotes in synthesized module specifiers

Upstream `0e081d6fb33dde09e084154a0a4daada31fa5a41` escapes quotes and
backslashes in synthesized module specifiers. The Rust language service now
uses the printer's quote-specific string escaping when inserting a JSDoc
`import(...)` type. Import-statement completions quote the module specifier
first, then double its backslashes and escape snippet metacharacters only for
snippet-capable clients. Plain completions retain the quoted text directly.
This follows the Go fix without an algorithmic deviation. The ignored
`ts-ref` checkout advances to source 106; the published TypeScript pin
remains at the migration base until the full series is ported.

The new exact-source Go `TestAutoImportQuotedModuleSpecifier` passes all ten
single/double-quote and named/default/require/JSDoc cases. All ten also pass
against the Rust LSP through the Go Fourslash subprocess runner. The full
exact-source gate now passes 4,092 exercised groups of 4,562, skips 470,
and fails none. The capability table records the new passing group. The
first Rust-backed run was denied permission to bind a local Unix socket by
the sandbox; the rerun with socket access passed. The dev CLI, testrunner,
and Fourslash server build, full workspace, 40 CLI regressions, WebAssembly
target check, warnings-denied Clippy, source lint, formatting, and generated
code check pass. The workspace's first sandboxed run had the same local-socket
denial in source-101 transport tests; its rerun passed.

All 18 Go-history, 18 canonical, and 36 emit compiler named outcome lists
are byte-identical to source 105. Both checker modes retain 13,581 passing
diagnostic baselines; Go history has 12,884 passing type/symbol baselines
and canonical has 12,883. Emit retains 13,585 passing diagnostic, 12,884
type/symbol, 13,511 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines, with no new failure, crash, or timeout. The comparison
script and full results are under `target/port-623/0106-0e081d6fb3/`.

Five alternating release-binary pairs per public project and checker mode
compared frozen sources 105 and 106. Every pair had the same exit status
and diagnostic hash. macOS `/usr/bin/time -l` supplied retired instructions
and peak RSS; wall time includes process launch:

| Project | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.22114 → 3.22103 | 59.14 → 59.11 | 0.230 → 0.229 |
| Compiler-Unions | default | 3.91174 → 3.90849 | 81.98 → 82.64 | 0.104 → 0.108 |
| webpack | single | 10.81831 → 10.81362 | 306.48 → 306.48 | 0.903 → 0.918 |
| webpack | default | 15.16895 → 15.33576 | 495.95 → 499.73 | 0.167 → 0.174 |
| VS Code | single | 81.38490 → 81.34459 | 1586.69 → 1586.67 | 7.995 → 7.901 |
| VS Code | default | 93.62049 → 93.68459 | 2094.98 → 2085.75 | 1.073 → 1.054 |

The initial webpack/default cell rose 1.100% in instructions and 4.181%
in wall time. A reversed ten-pair confirmation measured −1.485% and
−0.407%; a second ten-pair confirmation measured +0.455% and +1.935%.
The initial Compiler-Unions/default wall increase of 3.276% likewise
measured +0.345% on confirmation. No material regression persisted across
the confirmations. The largest initial peak-RSS increase was 0.800%; both
webpack/default confirmations remained below 1% or improved. The changed
code executes only for language-server import actions, not compiler checks.
The deterministic Linux `bench/count.mjs` counter was unavailable on this
macOS host. Frozen binaries, raw runs, output hashes, and confirmation
samples are retained under `perf/` and `perf-confirmation*/`.

## 107. `abcbb2e69d` — Allow CRLF in release workflow version files

Upstream `abcbb2e69d2a86d9ffee66582a0dd0339b674733` removes
`git diff --check` from three release-branch workflows so their generated
version files may use CRLF line endings. tsrs does not have the corresponding
release-branch creation, version-setting, or branch-sync workflows. Its
`release.yml` publishes artifacts and contains no equivalent whitespace
check, so there is no tsrs workflow or runtime change to make. The ignored
`ts-ref` checkout advances exactly one commit; the published TypeScript pin
remains at the migration base. No capability status or pass count changes.

The upstream `tsc` tree and `packages/typescript/src` tree are byte-identical
to source 106. The dev CLI and testrunner build, warnings-denied Clippy,
source lint, formatting, and generated-code freshness checks pass. The
full compiler suite was rerun against the exact source-107 checkout: all 18
Go-history, 18 canonical, and 36 emit named outcome lists are byte-identical
to source 106. Go-history and canonical each retain 13,581 diagnostic
passes, with 12,884 and 12,883 type/symbol passes, respectively. Emit retains
13,585 diagnostic, 12,884 type/symbol, 13,511 JavaScript/declaration, 149
JavaScript-map, and 156 source-map passes. There is no lost pass, new
failure, crash, or timeout. Source 106's full exact-source Fourslash gate
(4,092 exercised groups passing, 470 skipped, none failing), workspace,
and regressions remain applicable to unchanged source and runtime inputs.

The freshly built release CLI is byte-identical to source 106's frozen
binary (SHA-256
`27bc3f27b6031661aec33ef11cf80d50fc7b8fa8289e8c232478205260f32b49`).
On the same public inputs, its instructions retired, wall time, and peak RSS
are unaffected by this evidence-only port; source 106's paired measurements
remain the before-and-after evidence. The full outcome lists, comparison
script, build/check logs, and release hash are under
`target/port-623/0107-abcbb2e69d/`.

## 108. `0a27badde0` — Rebuild incremental diagnostics after global deletion

Upstream `0a27badde07113faba9f735c21071d461a37634a` fixes incremental
snapshot reuse when a program loses files. The Rust port copies old changed
and pending-emit paths only when the new program still contains them, marks
build info pending when it discards stale paths, and visits every removed
file-info entry. A removed global file invalidates every retained non-default
source file and evicts cached diagnostics from default libraries that will be
checked again; skipped libraries retain their cache. A file that stops
affecting the global scope uses the same invalidation. The tsrs-only
`has_deleted_files` guard remains, so the normal no-deletion path does not
scan old file-info entries. The Go scenario dumper also accepts upstream's
typed rooted paths, allowing the exact-source scenarios to be replayed.
The ignored `ts-ref` checkout advances to source 108; the published pin
remains at the migration base.

The exact-source Go incremental package passes all unit tests. Eleven Rust
snapshot tests pass, including four new checks for mixed deletion order,
stale pending paths, refreshed default-library diagnostics, and retained
skipped-library caches. All 14 new exact-source incremental history
scenarios pass byte-for-byte in the Rust tsc harness across the default,
single-threaded, and one-checker modes and the `noCheck`/`noEmit` composite
variants. All 18 Go-history, 18 canonical, and 36 emit compiler named
outcome lists match source 107 exactly: Go-history and canonical each retain
13,581 diagnostic passes and 12,884 and 12,883 type/symbol passes;
emit retains 13,585 diagnostic, 12,884 type/symbol, 13,511 JavaScript/
declaration, 149 JavaScript-map, and 156 source-map passes. The first emit
run timed out on the unrelated long-running
`intersectionConstructorReductionCrash` fixture at its default 20-second
limit; it passed in 36.5 seconds with a 90-second limit, restoring all 36
identical named lists. No pass was lost, and no failure or crash was added.

The dev CLI and testrunner build, full workspace suite, 40 CLI regression
cases, WebAssembly target check, warnings-denied Clippy, source lint,
formatting, and generator freshness check pass. The exact-source Go
Fourslash gate exercises 4,092 groups successfully, skips 470, and fails
none. The workspace and Fourslash checks used local Unix-socket access for
the API transport and Go subprocess bridge. Generator freshness was checked
by comparing the complete `crates/` diff before and after regeneration,
since this port itself edits the incremental crate.

Five alternating release-binary pairs per public project and mode compared
sources 107 and 108. The warm case repeats an unchanged VS Code incremental
check; the synthetic deletion case removes a global declaration before the
second check. Every pair had matching exit status and output hash, including
the expected missing-name diagnostic after deletion. macOS `/usr/bin/time -l`
supplied retired instructions and peak RSS; wall time includes process launch:

| Case | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions cold | single | 3.22113 → 3.22130 | 59.11 → 59.13 | 0.227 → 0.233 |
| Compiler-Unions cold | default | 3.90985 → 3.90988 | 82.20 → 82.02 | 0.138 → 0.140 |
| webpack cold | single | 10.80129 → 10.80132 | 306.48 → 306.47 | 0.926 → 0.910 |
| webpack cold | default | 15.19216 → 15.20851 | 495.45 → 495.98 | 0.353 → 0.360 |
| VS Code cold | single | 81.25697 → 81.26878 | 1586.67 → 1586.67 | 7.928 → 7.924 |
| VS Code cold | default | 93.81632 → 93.69340 | 2086.33 → 2081.72 | 1.686 → 1.703 |
| VS Code warm | single | 23.49115 → 23.48907 | 1243.20 → 1243.20 | 1.890 → 1.879 |
| VS Code warm | default | 24.13033 → 24.11300 | 1321.58 → 1320.94 | 0.480 → 0.474 |
| Global deletion | single | 0.79243 → 0.79244 | 42.80 → 42.81 | 0.062 → 0.062 |
| Global deletion | default | 0.88271 → 0.88420 | 51.25 → 51.17 | 0.064 → 0.063 |

Initial Compiler-Unions/single and webpack/default cold wall increases were
2.508% and 2.136%. Reversed ten-pair confirmations measured −0.971% and
+1.544%, respectively; their instruction deltas were +0.001% and −0.607%.
No regression persisted at the repository's performance threshold. The
largest initial instruction increase was 0.168% in the tiny deletion case,
and the largest peak-RSS increase was 0.107% in webpack/default. The retained
no-deletion guard keeps ordinary warm incremental checks clear of the
diagnostic-cache refresh previously rejected in `notes/perf-round2-followups.md`.
The deterministic Linux `bench/count.mjs` counter was unavailable on this
macOS host; the macOS instruction count includes kernel work. Frozen binary
hashes, raw runs, output hashes, and confirmations are under
`target/port-623/0108-0a27badde0/perf/`.

## 109. `39be28ba77` — Dump Node API timing on close

Upstream `39be28ba77f436d2a09b35fecb61ab203e2024eb` adds
`TS_API_TIMING_DIR` to the sync and async JavaScript API clients. A nonempty
directory enables timing collection even when `collectTiming` is false, and
each client writes one uniquely named JSON snapshot before closing. The
snapshot includes client timing, server timing when available, and any error
fetching the server snapshot. Closing twice does not create another file;
when the server has timing disabled, the client's measurements remain intact.
The sync channel now remembers a thrown filesystem callback error and rejects
later requests with that error instead of reading an out-of-sequence reply.
The SDK source and new `timingDump.test.ts` are copied from upstream, with the
existing tsrs async connection-loss patch rebased onto the source-109 client.
The Rust server and Go compiler are unchanged; the ignored `ts-ref` checkout
advances, while the published Cargo/npm TypeScript pin remains at the base.

The exact-source timing fixture passes 12/12 against both the Go and Rust API
servers, including unset/empty/enabled environment values, callback failure,
async socket error and disconnect, and a server with timing disabled. The
complete source-109 SDK suite passes 968/968 against each server. Strict SDK
sync reports 194 matching files; the SDK builds, and the packed npm consumer
passes all 25 smoke steps, including the existing connection-loss cases.
The dev CLI and testrunner build, warnings-denied Clippy, source lint,
formatting, and generator freshness checks pass. All 18 Go-history, 18
canonical, and 36 emit compiler named outcome lists are byte-identical to
source 108. Go-history checking retains 13,581 diagnostic and 12,884
type/symbol passes; canonical retains 13,581 and 12,883. Emit retains 13,585
diagnostic, 12,884 type/symbol, 13,511 JavaScript/declaration, 149
JavaScript-map, and 156 source-map passes. The exact-source Fourslash gate
passes 4,092 exercised groups, skips 470, and fails none.

The source-109 release CLI is byte-identical to the frozen source-108 binary
(SHA-256 `bf49874f30b6ffe935b89fbce36b504a42ccc25d076e3ed99a6d379b44a2a3c9`).
Thus the previous public CLI instructions-retired, peak-RSS, and wall-time
measurements apply unchanged: source 108's VS Code cold single-thread result
was 81.26878 billion instructions and 1586.67 MiB peak RSS; its default
result was 93.69340 billion instructions and 2081.72 MiB. This SDK-only
commit adds no Rust checker performance regression. The new SDK timing
collection is opt-in via the environment variable or existing option.
The current draft PR's Node API CI lane still checks out the old published
client at `b85298b6a81f`; pairing that client with the migration's advancing
server produces unversioned wire-protocol mismatches and an idle timeout.
The exact-source 968/968 comparison above is the parity evidence until the
final TypeScript pin advances. Logs, named-list comparison, and binary hash
are under `target/port-623/0109-39be28ba77/`.

## 110. `9de8b7a3b8` — Preserve API `pathsBasePath` across config handoff

Upstream `9de8b7a3b8c68a5ad51ffa4529b622215a32a185` makes
`RawCompilerOptions.Finalize` treat the internal `pathsBasePath` field as a
directory path. When an API client parses a config in `/project` and sends its
serialized options back to create a program while the session is in another
directory, `paths` aliases must still resolve relative to `/project`. Rust
already rooted `paths_base_path` in its API raw-option conversion since source
31. We added an assertion for a relative raw `pathsBasePath` and an API
round-trip regression that parses, serializes, creates a snapshot, and checks
that the aliased declaration file enters the program. No runtime code or
capability status changes. The ignored `ts-ref` checkout advances to source
110; the published TypeScript pin remains at the base until the entire port
finishes.

The two exact-source Go regression tests pass. All `tsrs_api` unit and
integration tests pass, including the new round trip. The dev CLI and test
runner build, full workspace, 40 CLI regressions, WebAssembly target check,
warnings-denied Clippy, source lint, and formatting pass. The full workspace
needed an unsandboxed rerun because the sandbox denied Unix socket binding in
the pre-existing private-pipe transport tests.

All 18 Go-history, 18 canonical, and 36 emit named compiler outcome lists are
byte-identical to source 109. Go-history checking retains 13,581 diagnostic
and 12,884 type/symbol passes; canonical retains 13,581 and 12,883. Emit
retains 13,585 diagnostic, 12,884 type/symbol, 13,511
JavaScript/declaration, 149 JavaScript-map, and 156 source-map passes. There
is no lost pass, new failure, crash, or timeout.

The exact-source Go Fourslash gate exercises 4,092 passing groups, skips 470,
and fails none. Generator freshness passes all seven generators. The release
CLI rebuilt byte-identically to the frozen source-109 executable: both have
SHA-256 `bf49874f30b6ffe935b89fbce36b504a42ccc25d076e3ed99a6d379b44a2a3c9`.
Its public instructions-retired, wall-time, and peak-RSS behavior is therefore
unchanged for identical inputs. Source 109's VS Code cold single-thread
reference was 81.26878 billion instructions and 1586.67 MiB peak RSS; default
was 93.69340 billion instructions and 2081.72 MiB. The deterministic Linux
`bench/count.mjs` counter was unavailable on this macOS host. The exact-source
Go/Rust logs, 72 named-list comparison, Fourslash result, and release hash are
under `target/port-623/0110-9de8b7a3b8/`.

## 111. `2e9fb00f5f` — Cover previously untested repository failures

Upstream `2e9fb00f5f5c9186d09aabd78cced5ce2497456d` adds 112 compiler
cases, two conformance cases, their 323 reference baselines, and seven `tsc`
command-line scenarios for `alwaysStrict` and `baseUrl` config values. It
changes no compiler runtime code. Rust already matches all new cases: the
114 exact-source diagnostic and JavaScript-emit checks pass, all 113
applicable `.types` and `.symbols` baselines pass. None of the new cases has a
source-map baseline, so those checks are skipped. The exact-source Go
scenario dumper records 550 scenarios; Rust replay passes all seven new
`in-configuration` scenarios with no mismatch. The ignored `ts-ref` checkout
advances to this commit, while the published TypeScript pin remains at the
base until the complete migration.

All 18 Go-history, 18 canonical, and 36 emit named outcome lists preserve
every source-110 result. The new cases add exactly 114 diagnostic passes,
113 type/symbol passes and one type/symbol skip, and 114 JavaScript passes;
there are no new failures or crashes. Go-history checking now passes 13,695
diagnostic and 12,997 type/symbol baselines; canonical has 13,695 and 12,996.
The complete emit run passes 13,699 diagnostics, 12,997 type/symbol, 13,625
JavaScript/declaration, 149 JavaScript-map, and 156 source-map baselines.
An initial 14-worker emit run timed out on the pre-existing
`intersectionConstructorReductionCrash` case at the harness's 20-second
limit. The complete ten-worker rerun with a 60-second limit passes it and
all other cases. The reference fixtures and the release CLI hash for that
case are unchanged from source 110.

The exact-source Go Fourslash gate passes 4,092 exercised groups, skips 470,
and fails none. The first attempt was blocked by the sandbox denying a local
Unix socket bind; the unsandboxed rerun passed. The dev CLI, test runner,
and Fourslash server build; warnings-denied Clippy, source lint, and
generated-code freshness all pass. No Rust source changed in this port.

The release CLI is byte-identical to source 110 (SHA-256
`bf49874f30b6ffe935b89fbce36b504a42ccc25d076e3ed99a6d379b44a2a3c9`),
so its public instructions-retired, wall-time and peak-RSS behavior on
identical inputs is unchanged. The previous VS Code cold single-thread
reference was 81.26878 billion instructions and 1586.67 MiB peak RSS;
default was 93.69340 billion instructions and 2081.72 MiB. The deterministic
Linux `bench/count.mjs` counter was unavailable on this macOS host. Exact
scenario, compiler, Fourslash, and repository-check evidence is under
`target/port-623/0111-2e9fb00f5f/`.

## 112. `5237fd4689` — Cache undefined widening properties per symbol

Upstream `5237fd46890956e4bb8177a1a05dcd69e4166560` keys the checker cache
for synthetic optional `undefined` properties by the source symbol rather than
its name. Rust previously kept both forms and used the name-keyed cache in Go
history mode. This port uses the symbol-keyed cache in both modes. A property
with the same name in another object now retains its own declaration order
when a widened object-literal type is printed. The exact-source Go fixtures
`undefinedPropertyDeclarationOrder` and `objectLiteralNormalization` pass
their diagnostic, JavaScript, `.types`, and `.symbols` baselines in both Go
and Rust. The canonical history's old `objectLiteralNormalization` type and
symbol failures become passes.

This is the final source in #623. The `ts-ref` checkout, Cargo metadata,
benchmark reference, bundled SDK manifest, npm dependency and lockfile now
pin the same upstream commit. npm `typescript@7.1.0-dev.20261018.1` reports
that exact commit as its `gitHead`. The workspace and root/npm package
versions advance to `0.19.0` and `0.19.0-ts7.1.0-dev.20261018` respectively;
the previously published 0.18.0 still points to the old source. The release
notes, compatibility table, and root/npm READMEs describe the candidate's
capabilities and existing gaps.

All 18 Go-history no-emit, 18 canonical no-emit, and 36 Go-history emit named
outcome lists preserve every source-111 pass, with no new failure, crash, or
timeout. Go-history and canonical checking each pass 13,696 diagnostic and
12,998 type/symbol baselines; emit passes 13,700 diagnostic, 12,998
type/symbol, 13,626 JavaScript/declaration, 149 JavaScript-map, and 156
source-map baselines. The two no-emit diagnostic failures in each history are
the unchanged emit-only harness artifacts; full emit has none. The exact-source
Go Fourslash gate passes 4,092 exercised groups, skips 470, and fails none.
The pinned upstream SDK source suite passes 968/968 against the Rust server.
The upstream Node API parity lane passes 885/885 against each of the exact
Go and Rust servers. A local all-suite parity fixture still carried the old
`SyntaxKind.FunctionDeclaration` numeric value, so it now reads the final
pin's enum and expects `kind: 264`. The full parity lane against both servers
has 934 passes, four existing skips, two todos, and no failures. The existing
soft-mismatch cells and the Go lone-surrogate todo remain non-gating.

The dev CLI, test runner and Fourslash server build; the full workspace tests,
40 CLI regressions, WebAssembly target check, warnings-denied Clippy, source
lint, formatting, and all seven generator freshness checks pass. The Go
Fourslash bridge and workspace API transport needed local Unix-socket access
outside the sandbox. Determinism self-tests and 756 output/exit comparisons
across XState, webpack, and all 40 regression cases pass. Nuxt and Drizzle
ORM checkouts are absent locally, so their CI determinism lane awaits the PR
CI run. The existing mapper fixture skips and the source-75 synthetic
two-builder peak-RSS increase of about 20% remain disclosed in the
compatibility table and release notes.

The PR determinism job previously compared ordered diagnostics with the
0.18.0 base binary even when the TypeScript source pin had advanced. That
cross-version comparison is invalid for this upgrade; CI now performs the
base-binary comparison only when both Cargo source pins match, while always
checking every configured project and regression case across checker
schedules. The two source-pin branches were exercised locally, and the
full four-project lane will run in PR CI.

The stamped `tsrs_cli/noembed` release build reports the exact npm nightly,
package version, and source commit. The packed macOS arm64 and main packages
contain all 116 default library declarations; an offline installed consumer
passes all 25 checks, including NodeNext declaration typing and the sync and
async API. The release binary's SHA-256 is
`f39e4348efe0815d5e47239416ba55724a8e37e05c8858bc7292fe664dd08a6a`.

Five alternating pairs of frozen source-111 and source-112 default-feature
release binaries on three public projects, in single-threaded and default
modes, produced identical output and exit status in every pair. The binaries'
SHA-256 values are `bf49874f30b6ffe935b89fbce36b504a42ccc25d076e3ed99a6d379b44a2a3c9`
and `d3702668639dbeee0ac0a58fe831783199f962efb49bcf358df8202bd227175a`.
macOS `/usr/bin/time -l` provided retired instructions and peak RSS; wall time
includes process launch:

| Project | Mode | Instructions, billions before → after | Peak RSS MiB before → after | Wall seconds before → after |
| --- | --- | ---: | ---: | ---: |
| Compiler-Unions | single | 3.221 → 3.221 | 59.13 → 58.98 | 0.231 → 0.233 |
| Compiler-Unions | default | 3.912 → 3.910 | 82.27 → 82.30 | 0.105 → 0.104 |
| webpack | single | 10.827 → 10.830 | 306.48 → 306.33 | 0.896 → 0.902 |
| webpack | default | 15.206 → 15.172 | 496.06 → 496.08 | 0.164 → 0.168 |
| VS Code | single | 82.425 → 82.444 | 1586.67 → 1586.53 | 7.828 → 7.886 |
| VS Code | default | 93.599 → 93.613 | 2080.98 → 2073.72 | 1.027 → 0.982 |

The largest instruction increase is 0.028% and largest peak-RSS increase is
0.038%. webpack/default's initial short-run wall increase was 2.492%; a
reversed ten-pair check measured +3.130%, but a longer 30-pair check measured
−0.993% wall, −0.161% instructions and +0.006% peak RSS. The subsecond wall
samples vary more than their instruction counts; there is no persistent
performance regression at the repository's threshold. The deterministic
Linux `bench/count.mjs` counter was unavailable on this macOS host, and macOS
instruction counts include kernel work. Raw runs, output hashes, and both
confirmations are under `target/port-623/0112-5237fd4689/perf/`.
