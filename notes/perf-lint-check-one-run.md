# Type check and type-aware lint in one run, without the second pass (2026-11-12)

Follow-up to [one lint pass instead of lint plus a separate typecheck](perf-tsgolint-check-program.md), the
[shared checker](perf-tsgolint-shared-checker.md) and the stealing experiment on branch `perf/lint-steal`
(`git show origin/perf/lint-steal:notes/perf-tsgolint-steal.md`).

`oxlint --type-aware --type-check` with tsrs-lint as the backend builds each tsconfig program once and both checks
and lints it, so it can replace a separate `tsrs --noEmit`. On main it still cost much more than the check alone:
on vscode at 8 cores the one run took 1.8 times the check's wall time, and on a large private monorepo the helper
alone took 1.5 times the check's wall time. This records where that time went, what changed, and what is left.

PLACEHOLDER
