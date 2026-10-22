# Terminal type-reference traversal fix (2026-10-15)

This records the serial helper at `8dbe8842`. See the later [worker and type-check
variant benchmarks](perf-tsgolint-workers.md) for the current concurrency implementation.

The final all-60-rule VS Code helper median is **15.254 s**, down from
18.956 s after the [infrastructure fixes](perf-tsgolint-infrastructure.md) and
153.908 s originally: **10.09× faster overall**.

## Correctness fix

This ports upstream tsgolint `47e98b4f4caa48154d014e5c69a63a03d7a51a41`.
`no-unnecessary-type-parameters` used to stop walking a type reference only when it had
arguments. A reference without arguments fell through into object-member traversal.
That can re-enter a containing class's generic method and double-count the very type parameter
being checked, suppressing a legitimate diagnostic. It also repeatedly visits irrelevant type graphs.
The reference branch now returns after processing its arguments, including when there are none,
matching the newer Go rule.

The upstream regression is equivalent to:

```ts
declare class C {
  m<T>(x: { self: C; arg: T }): void;
}
```

`T` is only used once in the signature. The fixed rule reports it; the previous rule did not.
The original 164 pinned cases and snapshots remain unchanged and pass. Five new sources
(the upstream regression adapted with an export prefix, plus four local controls) match the
installed Go binary in all four fix modes, including full suggestion records.

A complete decoded-diagnostic multiset comparison on VS Code confirms **exactly one addition
and zero removals or other changes** relative to `d501c471`. The added diagnostic is for this
rule and is also present, with identical content, in the Go helper output. All 60 per-rule
listener call counts remain identical to the infrastructure-only instrumented run.

## Measurements

Same machine and exact 9,324-path payload/config as the historical and infrastructure reports.
Optimized release build, no PGO/BOLT. Default mode: one warm-up each, three alternating Rust/Go
pairs, no overlapping builds or tests, warm filesystem cache and fresh compiler state per run.
Values below are independent medians. Go was rerun as a same-session control.

| Metric | Rust original | Rust infrastructure | Rust final | Go, fresh |
| --- | ---: | ---: | ---: | ---: |
| Default helper wall | 153.908 s | 18.956 s | 15.254 s | 10.866 s |
| Default peak RSS | 2.263 GiB | 2.132 GiB | 2.123 GiB | 7.858 GiB |
| Default instructions | 2800.061 G | 216.539 G | 164.298 G | 482.753 G |

The final wall range is 15.213–15.276 s.
This upstream correctness fix improves wall time by 19.5% relative
to infrastructure alone. Final RSS is 6.2% below the original.
Rust remains 1.40× slower at defaults, with serial lint evaluation versus Go's
default multiple workers; no workers were added by either fix.

A separate instrumented sample uses `GOMAXPROCS=1 RAYON_NUM_THREADS=1` and
`headless -debug timings` on both Rust snapshots. Its instruction count changes from
269.253 G to 217.316 G.
Accumulated callbacks for this rule change from 3793.153 ms to
61.254 ms. These callback times share type caches and depend on rule/file order;
they are not isolated cold-rule benchmarks. These are local macOS instruction counters,
not the deterministic Linux instruction gate. This change restores a missing diagnostic;
its performance effect is reported alongside that correctness change.

One complete Oxlint sample per helper takes **19.221 s with Rust versus
15.553 s with Go**. Both include the intentionally invalid JavaScript fixture
parser errors. These are individual observations, not repeated medians.

## Validation and reproduction

- 164 unchanged pinned rule cases pass in all harness fix modes.
- All 99 headless tests pass; the new test checks 20 independently captured Go snapshots.
- All Oxlint Rust suites and 642 JavaScript tests pass, with one upstream JS skip.
- Full VS Code diagnostic comparison shows the one expected upstream correction and nothing else.
- Required workspace Clippy and source checks pass.

[Raw measurements and diagnostic delta](../bench/results/2026-10-15-tsgolint-vscode-terminal-fix.json) include hashes,
all samples, instructions, RSS, per-rule timings, and validation details. The Go binary and
original corpus/version metadata are unchanged. Reproduction uses the same commands in the
[historical report](bench-tsgolint-vscode.md). Full local outputs and runner remain under
`/private/tmp/tsrs-vscode-lint-terminal-fix/`.
