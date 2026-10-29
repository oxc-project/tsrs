# perf-mapped-tuple-set: a one-element fast path for `{ [K in keyof T]: K extends "<i>" ? X : T[K] }` (not landed)

Measured 2026-10-29 on main 7dc2c938f plus the lazy simplified-conditional worker and the tuple literal-index fast
path (`testdata/regressions/tuple-literal-index-fast-path`), from the tsgo-opt fork's `tryFastMappedTupleSet`
(teamchong/TypeScript#1, commit b6c366256, "small Set/BSet mapped-tuple fast paths").

## The idea

`instantiate_mapped_tuple_type` instantiates the mapped type's template once per element of a fixed tuple. When the
template is the "set element" idiom `K extends "<i>" ? X : T[K]` (doom's `ReplaceValueByKey`, type-level array
stores), every element but `i` resolves `"j" extends "i"` to its false branch `T[K]`, which is the element itself.
The fast path instantiates the extends type once with the tuple mapper (which does not map `K`), requires a string
literal naming an element, instantiates the true branch with `K -> "<i>"` and copies the other type arguments into
a new tuple. Guards: no modifiers, every element fixed and required, no name type, the template is the root
conditional with no `infer` parameters, the check type is the mapped type's parameter and the false branch is
literally `T[K]`; an error type among the elements or in the replacement collapses to `error_type` as the
per-element path does (the fork returns a tuple there, which differs from Go). Note that the fork's own guard
`root.checkType != typeParam` compares the distributed form of `K` (what a bare `K` in a conditional's check
position resolves to through `getDistributedTypeParameter`) with the declared parameter and so never fires; the
port compared through `get_non_distributed_type_parameter`.

## Result

Exact: the fixture `testdata/regressions/mapped-tuple-set-element` (kept; tsgo output) matched, the 52 other
regressions and the conformance suite were unchanged (13623 / 12995 / 12995 / 149 / 156), and diagnostics were
identical to the previous binary on doom, xstate-main, webpack, t3code-server, cal-diy and supabase-studio at one
and at default checkers.

Useless on the catalog: under `--features tsrs_checker/site-counts` the fast path fired 5 times on its fixture and 0
times on doom (the public repo's tsconfig), xstate-main, webpack, t3code-server, cal-diy and supabase-studio;
`--extendedDiagnostics` Instantiations, Types and Symbols were identical to the previous binary on all six projects
(supabase-studio +4 types from the guard's wasted extends-type instantiation). Single-threaded instructions moved
inside the shared machine's noise
(doom 75.075 -> 75.070 G, xstate-main -2.9%, webpack -0.9%, t3code-server +0.02%, cal-diy 0.00%,
supabase-studio -0.2%, min of 3-5 runs at load average 80-200). The public doom repo defines the idiom once
(`packages/playground/helpers.ts` `ReplaceValueByKey`) and its checked files do not instantiate it over tuples;
the fork's numbers come from a private "doom-full" file set.

Not landed: ~90 lines for a path no catalog project takes. Revisit if a type-level workload that stores into tuples
through this idiom joins `bench/projects.json`; the draft is in this note's history (lane `simplified-tuple`,
2026-10-29) and the fixture already pins the semantics, including the error-type collapse and the shapes that must
stay on the per-element path (union, numeric, `"length"` and self-referential keys, optional elements).
