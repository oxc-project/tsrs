# Triage report for issue #649

## Reproduction status

REPRODUCED.

A minimal TypeScript file (no `drizzle-orm` dependency) reproduces the exact
Go-vs-Rust divergence:

- `tsrs` default: `Type 'number' is not assignable to type '[{ [x: string]: any; }]'`.
- `tsrs --noLazyMembers`: `Type 'number' is not assignable to type '[{}]'`.
- Pinned Go `microsoft/TypeScript@b85298b6a81f`: `[{}]`.

Reproduction files are in `triage/gh-649/repro/`.

## Diagnosis

The reference Go compiler at `b85298b6` resolves the mapped type
`SQLiteTable["$inferInsert"]` / the `Simplify<...>` that defines it to `{}`.
tsrs's current `main` resolves it to `{ [x: string]: any; }`.

The difference is not in lazy-table machinery by itself, but in two control-flow
shortcuts that tsrs inherited from upstream PRs #64475 and #64526. Those
shortcuts change the order in which mapped types are resolved relative to Go's
`resolveMappedTypeMembers` empty-members guard. For this input they reorder
resolution such that an inner mapped type finishes (producing a string index
signature) before the outer `Simplify<...>` is cached, producing
`{ [x: string]: any; }`. Go at `b85298b6` resolves the outer `Simplify<...>`
while the inner mapped type still has the empty members set by
`resolveMappedTypeMembers`, so it caches `{}`.

A community contributor (`qwer9052`) identified the two concrete places where
tsrs deviates from the Go reference behavior for this case and verified a fix
(checked counters, baselines, and determinism):

1. `crates/tsrs_checker/src/checker_09.rs` — `signatures_of_unresolved_structured_type`.
   With `lazy_members` enabled, mapped types are short-circuited to "no
   signatures" without resolving members. Go at `b85298b6` does not take this
   shortcut; `resolveMappedTypeMembers` resolves the type through
   `getSignaturesOfStructuredType`. The shortcut should be guarded so that it
   is not taken while `resolveMappedTypeMembers` is resolving a type whose
   members were set to empty upfront.

2. `crates/tsrs_checker/src/checker_11.rs` — `some_property_reduces_to_never`.
   With `lazy_members` enabled, the first unresolved mapped or lazy-member
   constituent is skipped during property-count collection. For a mapped type
   with an `as` clause this reordering is wrong: the mapped type has no lazy
   table and will be fully resolved on the later lookup anyway, so skipping it
   only changes resolution order and, in this case, changes the final result.
   The skip should be skipped when the mapped type has an `as` clause (a
   name-type remap).

Both changes are required; either one alone still produces the index signature.

## Fix approach

A candidate implementation (from `qwer9052`) applies two fixes:

- In `signatures_of_unresolved_structured_type`, only return the "mapped types
  have no signatures" shortcut when no `resolveMappedTypeMembers` resolution is
  in progress. A new `Checker::mapped_types_resolving` counter tracks the depth
  of that resolution.
- In `some_property_reduces_to_never`, do not skip a mapped constituent that has
  an `as` clause (i.e. `get_name_type_from_mapped_type(...).is_some()`).
- Add a regression case at
  `testdata/regressions/lazy-mapped-pending-members` whose expected output is
  the `{}` result from Go at `b85298b6`.

## Affected crate

`crates/tsrs_checker` is the only affected crate.

## Evidence and confidence

- Reproduced locally on the minimal no-drizzle-orm example.
- Verified `--noLazyMembers` makes tsrs match Go at `b85298b6`.
- Read Go `resolveMappedTypeMembers` (line 21235) and confirmed it sets empty
  structured members upfront.
- Read Rust `signatures_of_unresolved_structured_type` and
  `some_property_reduces_to_never` and confirmed they contain the shortcuts
  responsible for the reordering.
- The fix has been prototyped and counter-checked by another contributor.

Confidence: **medium**. The reproduction and the renamed control-flow path are
well isolated, but I have not instrumented a single-step trace or verified the
candidate fix with `bench/count.py` instruction counts in this run.

## Verification

Built Go `tsc` from `microsoft/TypeScript@b85298b6a81f` at
`triage/gh-649/ts-ref/tsc/tsc` and ran it on the minimal reproduction in
`triage/gh-649/repro/` with `--pretty false -p .`.

Verdict: **bug**.

Comparison evidence on the same input, config, and working directory:

| compiler | output |
| --- | --- |
| tsgo (pinned Go at `b85298b6`) | `a.ts(38,14): error TS2322: Type 'number' is not assignable to type '[{}]'.` |
| tsrs default | `a.ts(38,14): error TS2322: Type 'number' is not assignable to type '[{ [x: string]: any; }]'.` |
| tsrs `--noLazyMembers` | `a.ts(38,14): error TS2322: Type 'number' is not assignable to type '[{}]'.` |

Both compilers exit with status 2. tsrs default differs from the pinned Go
compiler (types ` { [x: string]: any; }` vs `{}`); `tsrs --noLazyMembers` matches
the pinned Go compiler exactly. No `docs/COMPATIBILITY.md` entry describes this
behavior as intentional or out of scope. Confidence: **high** for the verdict,
because this is a direct deterministic comparison.

## Fix

Implemented the two-point candidate fix from the diagnosis, with mechanical
ports of Go's control flow:

- `crates/tsrs_checker/src/checker.rs`: added `mapped_types_resolving: u32` to
  `Checker`, initialized to `0`, copied on fork.
- `crates/tsrs_checker/src/checker_10.rs`: increment the counter after the
  upfront empty-members guard in `resolve_mapped_type_members` and decrement it
  before final `set_structured_type_members`.
- `crates/tsrs_checker/src/checker_09.rs`: in
  `signatures_of_unresolved_structured_type`, only take the "mapped types have
  no signatures" shortcut when `self.mapped_types_resolving == 0`.
- `crates/tsrs_checker/src/checker_11.rs`: in
  `some_property_reduces_to_never`, do not skip a mapped constituent if it has
  an `as` clause (checked via `get_name_type_from_mapped_type`).
- Added regression project `testdata/regressions/lazy-mapped-pending-members/`
  with `a.ts`, `tsconfig.json`, and `expected.txt` generated from the pinned Go
  compiler (`[{}]`).

### Fix verification

- `target/debug/tsrs --pretty false -p triage/gh-649/repro` now produces the
  same output as the pinned Go compiler:
  `a.ts(38,14): error TS2322: Type 'number' is not assignable to type '[{}]'`.
- `target/debug/tsrs --pretty false -p triage/gh-649/drizzle-test` (the
  original `drizzle-orm@0.45.2` reproduction) now matches the pinned Go
  compiler: only `b.ts(2,14): ... type '[{}]'`, with no spurious TS4111 in
  `a.ts`.
- `tools/regressions.sh target/debug/tsrs` passes, including the new
  `lazy-mapped-pending-members` case and all 38 existing regression cases.
- `cargo test -p tsrs_checker --profile dev` passes.
- `cargo clippy --all-targets --config 'build.warnings="deny"'` passes.
- `tools/lint/source.py` passes.
- `cargo fmt --check` passes.
- The full compiler conformance suite is not present in this checkout (missing
  `testdata/tests`), so `tsrs-test run --suite compiler` could not be run.
  The regression suite and crate unit tests cover the affected code paths.

Confidence: **medium**. The reproduction now matches the pinned Go compiler on
both the minimal reproduction and the regression test, and the relevant lint,
unit, and regression checks pass. This run did not include `bench/count.py`
instruction counts or the upstream conformance suite.
