# Rust tsgolint port

Reference: tsgolint `8897346fff24a9bd0c719099dc1af8e7200572c1`, typescript-go
`2bd066d87f5bafd315be9f40889d0a60b9e58e0b`, with all six shipped compiler patches.
All 60 suites follow this reference; each directory records the same pins in `REFERENCE`.

Every rule must pass its active upstream cases, options, diagnostic snapshots, fixes,
and suggestions before its implementation commit. Upstream skips remain explicit.

| Rule | Status | Evidence |
| --- | --- | --- |
| await-thenable | Updated to Go HEAD | 166 current cases; UTF-8 ranges, suggestion flags, per-file configuration regressions |
| consistent-return | Updated to Go HEAD | 31 current cases; nested function state, multiple rules, undefined option regressions |
| consistent-type-exports | Updated to Go HEAD | 49 current cases; source overrides and both mixed-export fix modes |
| dot-notation | Updated to Go HEAD | 75 current cases; Go regex quoting, groups, classes, repetitions, Unicode properties, and octal escapes |
| no-array-delete | Updated to Go HEAD | 31 current cases; suggestion flags and UTF-8 ranges |
| no-base-to-string | Updated to Go HEAD | 380 current cases, 1 upstream skips; conversions/joins, recursive types, ignored names, checkUnknown |
| no-confusing-void-expression | Updated to Go HEAD | 109 current cases, 1 upstream skips; arrow/return fixes, contextual void, option combinations |
| no-deprecated | Updated to Go HEAD | 238 current cases, 12 upstream skips; aliases, signatures, contextual properties, destructuring, allow specifiers |
| no-duplicate-type-constituents | Updated to Go HEAD | 88 current cases, 2 upstream skips; nested unions/intersections, optional parameter fixes, multiline JSDoc ranges |
| no-floating-promises | Updated to Go HEAD | 188 current cases, 1 upstream skips; promises/thenables, rejection handlers, allowlists, await/void suggestions |
| no-for-in-array | Updated to Go HEAD | 24 current cases; structural array-likes and loop-header byte ranges |
| no-implied-eval | Updated to Go HEAD | 70 current cases; Function constructors, timer handlers, local shadowing |
| no-meaningless-void-operator | Updated to Go HEAD | 24 current cases; checkNever and independent fix/suggestion flags |
| no-misused-promises | Updated to Go HEAD | 228 current cases, 1 upstream skips; conditionals, predicates, spreads, nested void-return options |
| no-misused-spread | Updated to Go HEAD | 133 current cases, 1 upstream skips; string/array/iterable/class checks, allowlists, await and map suggestions |
| no-mixed-enums | Updated to Go HEAD | 51 current cases; initializer types, merged declarations and namespace/module augmentation |
| no-redundant-type-constituents | Updated to Go HEAD | 104 current cases; union/intersection overrides, aliases, literals and function return exceptions |
| no-unnecessary-boolean-literal-compare | Updated to Go HEAD | 50 current cases; nullable options, unary/precedence fixes and strictNullChecks diagnostics |
| no-unnecessary-condition | Updated to Go HEAD | 384 current cases; truthiness, nullish checks, optional chains, comparisons, loops and predicates |
| no-unnecessary-qualifier | Updated to Go HEAD | 18 current cases; namespace/enum scope, alias resolution, shadowing and qualifier fixes |
| no-unnecessary-template-expression | Updated to Go HEAD | 183 current cases; string constraints, type templates, comments, canonical literals and raw escaping fixes |
| no-unnecessary-type-conversion | Updated to Go HEAD | 77 current cases; primitive calls, unary/concatenation conversions, enum exclusions and satisfies suggestions |
| no-unnecessary-type-arguments | Updated to Go HEAD | 110 current cases, 1 upstream skips; default type identity, constructor inference, calls, heritage and JSX fixes |
| no-unnecessary-type-parameters | Updated to Go HEAD | 175 current cases; signature/class usage counts, recursive types, predicates and constraint replacement suggestions; upstream terminal-reference fix plus five regression/control sources in all four fix modes |
| no-unnecessary-type-assertion | Updated to Go HEAD | 376 current cases; contextual/generic typing, non-null flow guards, const options and comment-preserving assertion fixes |
| no-useless-default-assignment | Updated to Go HEAD | 93 current cases; object/tuple binding sources, contextual parameters, undefined defaults and optional-syntax fixes |
| no-unsafe-argument | Updated to Go HEAD | 43 current cases; recursive generic assignments, array/tuple spreads, rest consumption and tagged-template parameters |
| no-unsafe-assignment | Updated to Go HEAD | 94 current cases, 1 upstream skips; generic comparisons, nested destructuring, array spreads, contextual object/JSX properties and implicit this |
| no-unsafe-call | Updated to Go HEAD | 36 current cases; constrained any/error types, Function call/construct signatures, implicit this and template tags |
| no-unsafe-enum-comparison | Updated to Go HEAD | 93 current cases; enum/primitive comparisons, switch labels, static values and scoped member replacement suggestions |
| no-unsafe-member-access | Updated to Go HEAD | 36 current cases; optional-chain options, inner-chain deduplication, heritage exclusions, computed keys and implicit this |
| no-unsafe-return | Updated to Go HEAD | 76 current cases; deferred return types, unsafe generic assignments, virtual dependency declarations and JSDoc callbacks |
| no-unsafe-type-assertion | Updated to Go HEAD | 101 current cases; bidirectional any assignments, object widening, narrowing, unconstrained types and generic constraint diagnostics |
| no-unsafe-unary-minus | Updated to Go HEAD | 23 current cases; UTF-8 ranges, union member ordering, generic constraints, ignored options and independent fix/suggestion flags |
| non-nullable-type-assertion-style | Updated to Go HEAD | 26 current cases; UTF-8 suggestion ranges, independent flags, ignored options, assertion precedence and generic constraints |
| only-throw-error | Updated to Go HEAD | 112 current cases; UTF-8 ranges, per-file options, null defaults, invalid options, rethrow boundaries and independent flags |
| prefer-find | Updated to Go HEAD | 47 current cases; constant/coercion evaluation, constrained array unions/intersections, conditional/comma suggestions, UTF-8 comment ranges and independent suggestion flags |
| prefer-includes | Updated to Go HEAD | 65 current cases; parameter-text compatibility, regexp patterns, UTF-8 ranges, ignored options and independent fix flags |
| prefer-optional-chain | Updated to Go HEAD | 1170 current cases; structural chains, nullish guards, comment-preserving fixes, UTF-8 ranges, per-file options/state and independent fix/suggestion flags |
| prefer-nullish-coalescing | Updated to Go HEAD | 184 current cases, 2 upstream skips; UTF-8 suggestion ranges, independent flags, per-file options, Boolean shadowing, raw generic types and strictness diagnostics |
| prefer-promise-reject-errors | Updated to Go HEAD | 161 current cases, 1 upstream skips; UTF-8 call ranges, per-file options, null defaults, invalid options, computed accesses, executor boundaries, readonly constraints and independent flags |
| prefer-readonly-parameter-types | Updated to Go HEAD | 135 current cases; recursive readonly arrays/tuples/properties/indexes, branded literals, UTF-8 parameter ranges, per-file/null/invalid options and independent flags |
| prefer-regexp-exec | Updated to Go HEAD | 36 current cases; regexp2 syntax, Unicode 17 predicates, conditional branch/reference regressions, const aliases/global flags, constrained strings, computed access, UTF-8 ranges, ignored options and independent fix flags |
| prefer-readonly | Updated to Go HEAD | 162 current cases; UTF-8 ranges, constructor/nested-class scope, type relations, enum shadowing, per-file options/state and independent fix/suggestion flags |
| prefer-reduce-type-parameter | Updated to Go HEAD | 31 current cases; UTF-8 comment ranges, raw type trivia, computed members, readonly arrays, chained constraints, ignored options and independent fix/suggestion flags |
| prefer-return-this-type | Updated to Go HEAD | 22 current cases; UTF-8 ranges, exact class/this type identity, nested return boundaries, ignored options, per-file isolation and independent fix/suggestion flags |
| prefer-string-starts-ends-with | Updated to Go HEAD | 132 current cases; UTF-8 fix ranges, string constraints, regex flags/escapes, null defaults, per-file options, invalid options and independent fix/suggestion flags |
| promise-function-async | Updated to Go HEAD | 53 current cases; UTF-8 ranges, inherited and generic return types, overloads, per-file/null/invalid options and independent fix/suggestion flags |
| related-getter-setter-pairs | Updated to Go HEAD | 23 current cases; 26 oracle regressions for UTF-8 ranges, textual/computed/private names, duplicate source order, generic assignability, nested scopes, ignored options and independent fix/suggestion flags |
| require-array-sort-compare | Updated to Go HEAD | 47 current cases; UTF-8 call ranges, computed/optional accesses, readonly tuples, generic constraints, array unions/intersections, per-file/null/invalid options and independent fix/suggestion flags |
| require-await | Updated to Go HEAD | 90 current cases; UTF-8 heads/edits, nested and per-file scopes, constrained/union thenables and async iterables, decorators, ASI/comments, ignored options and independent suggestion flags |
| restrict-plus-operands | Updated to Go HEAD | 122 current cases; UTF-8 operand/label ranges, constrained types, RegExp identity, union/intersection checks, per-file/null/invalid options and independent flags |
| restrict-template-expressions | Updated to Go HEAD | 72 current cases; UTF-8 labeled ranges, null defaults, invalid options, per-file allowlists, nested arrays/tuples, constraints, tagged templates and independent flags |
| return-await | Updated to Go HEAD | 107 current cases; 29 oracle regressions for UTF-8 ranges, per-file/null/invalid options, nested scopes, resource disposal, generic constraints, precedence and independent fix/suggestion flags |
| strict-boolean-expressions | Updated to Go HEAD | 218 current cases; 24 oracle regressions for UTF-8 ranges, logical traversal state, assertion/array predicates, generic constraints, nullable options, strictNullChecks, invalid options and independent fix/suggestion flags |
| strict-void-return | Updated to Go HEAD | 214 current cases, 1 upstream skips; UTF-8 ranges, contextual/generic/overload returns, inherited members, nested scopes, per-file/null/invalid options and independent fix/suggestion flags |
| switch-exhaustiveness-check | Updated to Go HEAD | 107 current cases; 45 oracle regressions for UTF-8 ranges/indentation, constrained and branded types, undefined identity, enum escaping, default options, per-file isolation and independent suggestion flags |
| unbound-method | Updated to Go HEAD | 223 current cases, 2 upstream skips; 35 oracle regressions for UTF-8 labels, this annotations, safe uses, nested/aliased destructuring, native globals/shadowing, builtin type recursion, per-file/null/invalid options and independent flags |
| use-unknown-in-catch-callback-variable | Updated to Go HEAD | 56 current cases, 1 upstream skips; UTF-8 suggestions, ignored options, expression branches, spread boundaries, constraints, per-file configuration and independent fix/suggestion flags |

## Additional Oxlint compatibility rules

These rules extend the original 59-rule inventory. Their source revisions are recorded separately.

| Rule | Status | Evidence |
| --- | --- | --- |
| no-generated-empty-object-type | Updated to Go HEAD | 29 current cases; generic mapped types, unions, intersections, call/construct signatures and primitive assignability |
