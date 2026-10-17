# Go HEAD lint refresh (2026-10-16)

The lint branch was rebased onto repository main `b3a04abb`. The native Rust compiler
keeps main's compiler reference; only the lint rule oracle is updated here.

All 60 lint rules now follow tsgolint `8897346fff24a9bd0c719099dc1af8e7200572c1`
and its recorded compiler `2bd066d87f5bafd315be9f40889d0a60b9e58e0b`, with all
six shipped compiler patches. The oracle was rebuilt with Go 1.27.1; its SHA-256 is
`4bf2bd341d7bd53b69c59228820c2045da45eb7ea0e25119a906a28524972d87`.
The exported declaration dependencies are `@types/node` 24.13.2 and `undici-types` 7.18.2.

The refresh resolves the rule-version gaps identified in the earlier
[diagnostic investigation](tsgolint-diagnostic-parity.md): deprecated aliases and
contextual properties, optional parameter properties, bound built-ins, falsy guards,
truthiness consumers, deferred returns, assertion inference/narrowing, and diagnostic
presentation. It also ports newer upstream fixes exposed by the full suite, including
switch default comments, tuple defaults and precedence-sensitive fixes. Each behavior
change has a separate rule commit and a precommit deep review.

The corpus contains 7,601 active cases and 28 explicit upstream skips. Every rule has
its HEAD `REFERENCE`; legacy diagnostic-reference overrides have been removed.
The exporter runs Go packages and tests serially because parallel test execution can
share cached virtual filesystem state. All 60 Go test packages passed during export.
The unchanged integration source was replayed with all 60 rules in all four fix modes:
each produces the same 23 complete diagnostic records as its checked-in expectation.
Local fixtures retain their own oracle provenance.

Validation commands:

```sh
cargo build --profile dev --locked -p tsrs_cli -p tsrs_testrunner -p tsrs_lint
cargo test --locked -p tsrs_lint
cargo clippy --workspace --locked --all-targets --config 'build.warnings="deny"' -- -D dead-code
tools/lint/source.py
```

The Rust test command includes every imported case, expected fix pass and suggestion
output, plus protocol, lifecycle and local regressions. Diagnostic ordering is not a
compatibility requirement; the mixed-rule fixture compares exact record multisets.

Two deliberate robustness differences are covered locally: Go HEAD panics on untyped
explicit `this` parameters and invalid switch comment patterns; Rust reports the unbound
reference and returns a structured invalid-pattern error, respectively.

The native compiler conformance run after rebasing retains main's four known failures
and one timeout (intersection constructor reduction). The timeout was reproduced on a
clean checkout of `b3a04abb`; the lint rule refresh does not change native compiler code.
Compiler-version differences and Go checker-history variation from the earlier corpus
investigation remain separate from rule-version parity. This suite refresh does not
claim a new whole-VSCode diagnostic or performance comparison.
