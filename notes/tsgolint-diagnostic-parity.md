# Go/Rust lint diagnostic investigation (2026-10-16)

Update 2026-10-17: lint workers now share their semantic and lint checker at the user's
request, accepting checker-history differences. The isolated-checker observations below
are historical; see [the shared-checker measurements](perf-tsgolint-shared-checker.md).

The observed differences are largely reference-version drift. Rust implements the pinned June rule set,
with selected October diagnostic refreshes; the benchmark Go package includes newer rule fixes.
The parallel-loading change preserved Rust's complete diagnostic multiset.

This investigation changes no production code or baseline expectations. The
[structured evidence](tsgolint-diagnostic-parity.json) records counts, unpaired diagnostics,
small repros with actual helper output, binary/input hashes, source references and the comparison algorithm.

## References and method

- Rust `d2088d35`, release helper from the parallel-loading benchmark.
- Original rule reference: tsgolint `9378e49b67c0205a36afed0dfdcce229fc7d44b5`, compiler
  `69d658da1a632a723275439339e34842d4d8f401`; the existing disposable `tsgolint-rule60` oracle
  includes the additional empty-object rule. Its source modifications are the existing test exporter,
  rule registration and shim additions, not changes to these investigated rules.
- Native Rust compiler reference: TypeScript `b85298b6a81f772d080b0455de0ca9d744cd6fd6`.
- Installed Go: oxlint-tsgolint 7.0.2004, stamped rule revision
  `a61289117febcb1a60cf3bc75ef2b6bbccc4b665+dirty`. Its stamped source revision records compiler
  `2bd066d87f5bafd315be9f40889d0a60b9e58e0b`. The dirty stamp prevents claiming an exact source rebuild.
- Same public VSCode commit/payload as the [benchmark](perf-tsgolint-parallel-load.md): all 60 rules,
  9,324 requested paths. Comparisons use saved default-thread outputs, publish 1, repetition 0.

Stream order is ignored; exact records are cancelled with multiplicity. Remaining records are grouped by
rule/file, preferring equal ranges, then overlapping ranges with compatible message IDs. The nearest-range
pairing is a heuristic for triage, not proof of semantic equivalence. Different IDs at the same range
remain visible as metadata differences. The artifact retains all unpaired lint records for follow-up.

## Lint-only comparison

Rust emits 275,891 diagnostics; this Go sample emits 275,834:

| Comparison bucket | Records |
| --- | ---: |
| Exactly equal records | 250,643 |
| Paired reports with changed metadata, equal ranges | 23,095 |
| Paired reports with changed ranges and/or metadata | 1,956 |
| Unpaired Rust reports | 197 |
| Unpaired Go reports | 140 |

Net counts hide both removals and additions, as well as presentation changes.

| Rule | Unpaired Rust | Unpaired Go | Investigation |
| --- | ---: | ---: | --- |
| no-deprecated | 65 | 135 | New contextual-property/alias handling; reasons also changed on 460 paired reports |
| no-duplicate-type-constituents | 3 | 0 | Optional parameter-property exception added upstream |
| no-unnecessary-condition | 16 | 2 | New logical-context/record-property handling; some fixture locations also vary in Go |
| no-unnecessary-type-assertion | 58 | 1 | New assertion safeguards and reporting behavior; enum false positive reduced |
| no-unsafe-member-access | 1 | 1 | Swapped invalid JavaScript fixture locations; Go also varies between runs |
| no-unsafe-return | 6 | 0 | Isolated to the newer rule with a compiler-preserving Go overlay experiment |
| no-unsafe-type-assertion | 0 | 1 | Go's count and output vary between identical runs |
| prefer-optional-chain | 47 | 0 | New falsy-guard/narrowing safeguards |
| unbound-method | 1 | 0 | Intl.Collator.compare is now recognized as bound |

On the same corpus, the original pinned Go oracle and Rust have exactly equal records for
`no-deprecated`, `no-duplicate-type-constituents`, `prefer-optional-chain`, `unbound-method`, and
`no-misused-promises`. Assertion and return-rule detections also match that oracle, with the expected
refreshed diagnostic formats. This is evidence of version drift for these rules, not a claim of
whole-corpus parity with either Go version.

## Confirmed behavior changes

The evidence contains small sources replayed against Rust, installed Go and the pinned oracle.

- **Deprecation reasons:** `/** @deprecated Use replacement instead. */ declare const legacy: number; legacy;`
  loses the reason in Rust and pinned Go. Installed Go includes it. Upstream
  [322b3856](https://github.com/oxc-project/tsgolint/commit/322b3856) walks to the containing variable statement.
- **Contextual deprecation:** `take({link: "hello"})` where `take` accepts `{link: string} | string`
  spuriously inherits deprecated `String.link` in Rust/pinned Go. A discriminated union with one deprecated
  `key` also reports both alternatives in Rust, while installed Go reports the deprecated alternative.
  Conversely, `new KeyboardEvent("keydown", {keyCode: 9})` is missed in Rust and reported in installed Go.
  [a639a610](https://github.com/oxc-project/tsgolint/commit/a639a610) uses apparent contextual types and
  the checker's deprecated-symbol predicate. The corpus has 52 Rust-only `link` reports, 13 Rust-only
  `keyCode` reports and 102 Go-only `keyCode` reports; the individual remaining sites are not all reduced.
- **Optional parameter properties:** Rust reports `constructor(readonly value?: string | undefined)`;
  installed Go permits it, while still reporting ordinary optional parameters. This is
  [08282b48](https://github.com/oxc-project/tsgolint/commit/08282b48).
- **Bound built-ins:** Rust reports passing `new Intl.Collator().compare` as a callback. Installed Go
  exempts it via [ace02d66](https://github.com/oxc-project/tsgolint/commit/ace02d66).
- **Falsy guards:** for `a: {flag: boolean; fn(): boolean} | undefined`, Rust reports
  `a && a.flag && a.fn()`, while installed Go preserves the falsy guard. See
  [dc4765dc](https://github.com/oxc-project/tsgolint/commit/dc4765dc); another upstream negated-guard fix
  [50ec92e5](https://github.com/oxc-project/tsgolint/commit/50ec92e5) is also absent from the original reference.
- **Value-producing logical expressions:** Rust reports the `a.values` operand in
  `const values = (a && a.values || [])` for non-null `a: {values: string[]}`. Installed Go reports only `a`.
  [22b148ad](https://github.com/oxc-project/tsgolint/commit/22b148ad) distinguishes truthiness consumers;
  [60c373bc](https://github.com/oxc-project/tsgolint/commit/60c373bc) also changes unchecked record handling.
- **Numeric enum assertions:** Rust reports `numberValue as E` as unnecessary. Installed Go retains it.
  [0a6ccb02](https://github.com/oxc-project/tsgolint/commit/0a6ccb02) adds enum/narrowing safeguards and
  other assertion changes. This reproduces a real missing safeguard; it does not individually explain
  all 58 unmatched assertion reports.
- **Deferred returns:** Rust reports a return explicitly typed as `Infer<S>` when its constraint is `any`.
  Installed Go retains the original deferred type. The repair is
  [b71f3dfb](https://github.com/oxc-project/tsgolint/commit/b71f3dfb).

For stronger attribution of `no-unsafe-return`, a Go `-overlay` build replaces only that rule's file
in the pinned oracle with the exact installed-revision source. Compiler, all other rules and input stay
unchanged. Its VSCode count changes from 1,037 to 1,031; **all 1,031 complete records then match installed Go**.
This isolates all six corpus differences to that rule update, including the generic `instanceof` case.
A preliminary partial experiment changing only the early equality checks did not fix the corpus;
the complete upstream rule also uses the original type for unsafe-assignment checks.

## Presentation differences

These matter to protocol parity even when the same code is flagged:

- 21,695 `strict-boolean-expressions` reports split instructions out of the description into `help` in Go.
- 571 `no-confusing-void-expression` reports and 20 `consistent-type-exports` reports similarly split help.
- All 2,032 `no-misused-promises` reports add labels in Go; 1,956 also change the primary range,
  often highlighting an arrow token or return annotation instead of the whole function.
- 81 `no-useless-default-assignment` reports have updated wording; upstream also changed fix/suggestion
  policy, which a no-fix corpus run does not validate.
- Other paired differences include type-printer truncation/representation and 18 assertion diagnostic-ID
  changes. These should not be silently normalized away in a parity test.

The artifact includes separate minimal presentation repros. Relevant upstream commits include
`5511fbcd`, `812cc8b7`, `35b7e76e`, `7b3e31e7`, `5f54862a` and `af5d7402`.

## Full compiler checking and Go variation

In the selected full-checking outputs, Rust has 1,034 compiler reports and Go has 1,028:
926 exact pairs, 102 pairs with changed message text, and six Rust-only reports.
The six involve `ConfigurationResolverExpression`/`resolveAsync` assignability.

A Go compiler built at the **native Rust pin** reports all six too. Running that Go compiler and
`target/debug/tsrs` separately with `-p <vscode>/src/tsconfig.json --noEmit --pretty false --singleThreaded`
produces equal output-line multisets: 1,153 lines each. This is a native CLI control with different
filtering from the headless helper, not a claim that its diagnostic count is 1,153 or that all compiler
histories are equivalent. The six extra reports are not evidence of a Rust-only defect.

Go's six measured lint totals range from 275,832 to 275,834; full-checking totals range from 276,864
to 276,912. Contents also vary when per-rule counts remain equal. Between the first two saved Go runs,
lint-only output removes 21 and adds 19 complete records; full-checking removes 169 and adds 161.
Varying full-checking rules include assertion rules, `non-nullable-type-assertion-style` and `no-unsafe-return`.
Rust's multiset stays constant across its measured runs and across semantic-reporting modes for lint records.

There is a controlled checker-history repro, using one checker and no libs:

```ts
export {};
declare function factory<T = unknown>(): T;
const a = factory() as string;
a.toUpperCase();
declare const n: number;
n as number;
const wrong: string = 1;
```

Without semantic reporting, both helpers report only `n as number`. With semantic reporting,
Go also reports `factory() as string`; Rust still retains that assertion. Both report the unrelated
compiler error. Go's semantic pass precedes linting and reuses program checkers; its default linter
assigns a shared file queue to those checkers. Rust deliberately separates semantic/lint checker state
([regression test](../crates/tsrs_lint/tests/headless.rs)). This confirms one history-sensitive mechanism;
it does not isolate every varying corpus record. Duplicate global JavaScript fixtures also appear in
swapped reports. Ordering-insensitive comparison alone cannot eliminate these content differences.

## Next changes

Prioritize the correctness safeguards: optional-chain falsy guards, assertion preservation,
contextual deprecation, optional parameter properties, bound Collator methods, logical-context checks
and the complete unsafe-return update. Port their upstream tests with each fix. Refresh presentation
separately, retaining exact messages, labels, ranges and fix/suggestion behavior in tests.

Keep the semantic/lint checker separation while investigating Go's history-sensitive cases. Do not
make Rust drop or add diagnostics merely to match one nondeterministic full-checking sample.
The remaining assertion and fixture deltas need individual reductions before attributing each to a
specific upstream change; passing the old imported suite does not establish current-package parity.
