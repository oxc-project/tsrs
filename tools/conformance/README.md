# Full conformance gate

CI runs the pinned upstream compiler and conformance fixtures using normal tsrs behavior.
It compares diagnostics, types, symbols, JavaScript/declarations, JavaScript maps and sourcemap records.
The GNU release gate uses the same checks on the optimized binary.

From the repository root, after fetching the reference fixtures as described in
[CONTRIBUTING.md](../../CONTRIBUTING.md):

```sh
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner
TSRS_TEST=target/debug/tsrs-test .github/scripts/conformance-gate.sh
```

`TSRS_TEST`, `TSRS_TEST_RESULTS`, `TEST_TIMEOUT` and `PYTHON` override the binary, results directory,
per-case timeout (120 seconds), and Python executable. Python 3.9 or newer is sufficient.
Results default to `target/test-results`. The gate clears `TSRS_CHECKER_ASSIGNMENT` and
`TS_TEST_PROGRAM_SINGLE_THREADED` so ambient scheduling experiments cannot change the run.
Other compiler experiment flags should be unset. There is no `--mode` option. The raw runner also uses
normal compiler threading by default; an explicit `TS_TEST_PROGRAM_SINGLE_THREADED=true` remains available
for local investigations.

The gate always requests `types,symbols,js,jsmap,sourcemap`; the runner's `--baselines all` currently means
only types and symbols. Use `tsrs-test` directly for filtered or non-emitting investigations. `BASELINES`
and the old `MIN_*` pass thresholds no longer configure the gate.

## Known mismatches

[known-mismatches.json](known-mismatches.json) records the current conformance problems.
An entry contains the test ID, baseline, classification, first difference and SHA-256 of the complete diff.
An unchanged known mismatch is permitted by the gate but is still a failure in the reported conformance counts.
Deliberate differences receive the same treatment as any other mismatch.

The gate fails for new or changed mismatches, crashes, timeouts, harness errors, missing artifacts, incomplete
runs, or changed coverage. A newly passing case cannot offset a new failure. When a recorded mismatch starts
passing, the gate asks for its stale entry to be removed.

The inventory also records the upstream commit, selected case fingerprint, excluded fixture names/reasons,
and exercised/skipped case fingerprints for every baseline. This prevents cases from silently disappearing
or becoming skipped, even if the total pass count stays unchanged. Skipped fingerprints include their reasons.

## Refresh after reviewing a change

Run the gate first, inspect its report and full diffs, then explicitly refresh the inventory:

```sh
python3 tools/conformance/gate.py refresh
git diff -- tools/conformance/known-mismatches.json
python3 tools/conformance/gate.py check
```

Use `--results <directory>` if the run used a custom results directory.
Refreshing reads completed results; it does not rerun the compiler. It refuses to record crashes, timeouts,
harness failures or incomplete output. CI cannot run `refresh` and never rewrites the inventory.
Remove fixed failures in the same change as their fix. Review new entries or changed fingerprints as changes
to conformance debt, never as an automatic way to make CI green.

For an upstream pin update, archive the previous inventory, remove it, rerun and refresh against
the new pin, and review the complete diff. The gate refuses to mix results from different upstream commits.
Inventory and run metadata use schema version 2; older two-mode artifacts are rejected. The inventory was migrated from the production-mode
results without changing mismatch or coverage fingerprints.

## Reports and coverage limits

CI publishes counts and every known/new/changed/resolved mismatch in its job summary, even on successful runs.
The gate executes after a successful build. Artifacts contain `report.md`,
`runner.log`, `summary.json`, `run.json`, `invocation.json`, `coverage.json`, case lists, and mismatch outputs/diffs.

`coverage.json` lists every exercised and skipped case, with reasons, plus fixtures excluded before variant
expansion. The report groups skips by reason. Tests with `noTypesAndSymbols` or no emitted/map output do not
produce those baselines; unsupported options, content mappers and upstream nondeterministic emit exclusions
remain coverage gaps. None is counted as passing. Running every supported baseline does not mean every
official fixture is supported.

```sh
python3 -m unittest discover -s tools/conformance -p 'test_*.py'
```
