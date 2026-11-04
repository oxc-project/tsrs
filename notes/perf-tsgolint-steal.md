# Work stealing for the lint pass under the fast preset (2026-11-04)

Follow-up to [lint on the checker pool](perf-tsgolint-shared-checker.md) and the stealing item in
[the bottleneck investigation](perf-tsgolint-bottlenecks.md).

Lint runs one pass over its files on the program's checker pool. It used the static assignment
(`for_each_checker_group`) without the type-check pass's heavy-files-first order and stealing, so the checker whose
directory groups were slowest set the end of the pass. Lint is the only pass over those files, so nothing later needs a
file on its first checker; `Program::for_each_checker_group_stealing` runs the pass like the type-check pass.
Stealing stays off where `stealing_enabled` is off: default mode keeps the static assignment and its history.

On the large private monorepo, per-checker CPU with the static assignment had a busiest/average ratio of 1.36; with
stealing 1.07, for more total CPU (a thief checks a stolen file without the types its first checker built for that
neighbourhood).

## Measurements

AWS m8g.4xlarge (Graviton4, 16 vCPU, 64 GiB), Ubuntu 24.04, `TSRS_FAST=1` (16 checkers). Release builds of main
`48cf809e` and this change with the same flags (fat LTO, no PGO). Both arms on the same box, alternating with the
order flipped every pair; values are medians, changes are medians of per-pair ratios with ~95% intervals. Peak RSS is
the process maximum. Every rule with default options (`bench/lint.mjs` requests) for the catalog projects; the
private monorepo uses its own oxlint request.

| Project | Pairs | Wall | Instructions | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| large private monorepo | 15 | 11.13 -> 8.39 s, −24.9% [−25.6, −24.1] | +13.0% | +8.5% |
| vscode | 10 | 3.26 -> 2.59 s, −20.7% [−21.0, −20.0] | +0.6% | +1.7% |
| mikro-orm | 10 | 4.57 -> 3.91 s, −14.7% [−14.9, −13.7] | +6.4% | +3.8% |
| webpack | 10 | 0.43 -> 0.37 s, −14.2% [−15.6, −11.5] | +1.1% | +0.7% |
| xstate-main | 10 | 0.69 -> 0.62 s, −9.4% [−10.3, −8.6] | +0.6% | +0.4% |
| t3code-server | 10 | 4.14 -> 4.25 s, +2.5% [−1.8, +3.2] | +39.2% | +39.3% |
| drizzle-orm | 10 | 0.31 -> 0.32 s, +0.7% [−0.9, +1.5] | +19.8% | +16.5% |

Default mode on the private monorepo (15 pairs): instructions −0.0%, wall −0.1% [−0.9, +0.2], identical output.
Single-threaded runs are unaffected: stealing needs more than one checker.

t3code-server and drizzle-orm pay the thief cost without a shorter pass: thieves converge on the last queues and each
builds the same neighbourhood again (the case `max_thieves_per_queue` describes, which applies from 8 to 15 checkers
only). A probe with `TSRS_STEAL_MAX_THIEVES=2` at 16 checkers cut t3code-server's instructions 9.7% and peak 10% but
left most of the gap, and changed nothing else measurably; it is not part of this change.

Where the helper's time is hidden behind another tool's work (a linter frontend that runs its own rules while the
helper lints), the shorter pass buys nothing and the extra CPU competes with that work; this was measured with an
overlapping frontend prototype on the private monorepo (−21.8% end to end with stealing, −24.5% without).

## Output

Identical output on every project except mikro-orm, where 17 of 25,908 diagnostics print a type in their `help`
text with different truncation (same diagnostics, ranges and messages): the checker that first builds the type names
it. Each arm's output hash was stable across all of its runs on every project.
