# port-conditional-distribution-fast-paths: the tsgo-opt fork's 1-parameter distribution fast paths (rejected)

2026-10-29, base main 7dc2c938f. Two mechanisms of the teamchong/TypeScript "tsgo-opt" fork's
`getConditionalTypeInstantiationEx` (commit 9b498ffd8 there) were ported into
`get_conditional_type_instantiation_ex` (checker_11.rs), gated and measured. Neither is landed; the regression
fixture `testdata/regressions/conditional-distribution-mapper-order` is.

## What the fork does

For a distributive root with one outer type parameter `T` whose check type is `T` (`getNonDistributedTypeParameter`),
it evaluates each constituent `t` of the distributed union with `newSimpleTypeMapper(T, t)` instead of
`prependTypeMapping(checkType, t, newMapper)` (a merged mapper of `[T -> t]` over `[T -> arg]`), and keeps each
constituent's result in `root.instantiations` under `packedSingleTypeKey(t, forConstraint)`, the key of the
one-argument instantiation `[t]` without alias, so a constituent evaluated for one union is reused by every other
union that contains it.

## Why the simple mapper is not exact

The two mappers map every type the same way, but a deferred result keeps its mapper (`ConditionalType.mapper`), and
`compare_type_mappers` (utilities.go `CompareTypes`, the order of union constituents and so of type text) orders by
mapper kind before anything else. A union that mixes a distributed constituent with a directly instantiated one
therefore changes order:

```ts
type D<T> = { x: T extends string ? 1 : T };
function g<U, V, W>(b: D<U | V>, c: D<W>, k: boolean) {
    const n: number = k ? b.x : c.x;
}
```

tsgo-ref: `Type '(W extends string ? 1 : W) | (U extends string ? 1 : U) | (V extends string ? 1 : V)' is not
assignable to type 'number'` (`W` first: its mapper is simple, the others are merged). With the fork's mapper every
constituent is simple and the text becomes `(U ...) | (V ...) | (W ...)`. The fixture pins the reference order.

The exact form is a simple mapper only for a constituent that is not generic (`is_generic_type`): its evaluation
cannot defer (the check type is not generic, and nothing else a one-parameter root's extends type can mention stays
generic once `T` is mapped), and no non-deferred result keeps the mapper (object, mapped and deferred-reference
instantiations build their own from the type arguments).

## Why the per-constituent cache is not exact

A hit skips the constituent's evaluation, so `instantiation_count` and `total_instantiation_count` (the TS2589
budget, `--extendedDiagnostics` "Instantiations") drop by that evaluation's cost. The cost is not a constant that a
hit could charge (notes/perf-checker-algorithms.md, row "Rejected: it moves the instantiation-budget boundary"): the
first evaluation of a constituent instantiates its permissive/restrictive forms and resolves what its relation
needs, later ones hit those caches. The recipe the repo's other memos use (store only when the evaluation created no
type, instantiated nothing, reported nothing: unioncache.rs, the indexed-access union memo, flowmemo.rs) can never
hold here, because every evaluation instantiates the check type at least once. Type identity is not the problem:
`CompareTypes` orders structurally, so deduplicating structurally identical results does not reorder a union; only
the counters do.

## It would not pay anyway

Work census of doom (`TSRS_WORK_CENSUS`, one checker, base binary): conditional instantiation miss path 571,865
calls (245,665 outer), 3,991 distributions over 229,271 constituents, of which 222,963 are one `Exclude<..>`
distribution (two outer parameters, not on this path). About 6,000 constituents of 1-parameter roots per check:
nothing to save. The heavy roots (`_ProcessInputStack`, `executeInstruction`, `selectInstruction`, `Length`,
`GarbageCollect`) do not distribute at all.

Measured anyway, with both mechanisms (the cache guarded by "created no type, reported nothing"), release builds,
single-threaded instructions retired (min of 3 interleaved runs) and peak RSS (median), default checkers the same:

| project | mode | base Ginst | cand Ginst | inst delta | base RSS MiB | cand RSS MiB | RSS delta |
| --- | --- | --- | --- | --- | --- | --- | --- |
| doom | single | 75.097 | 75.117 | 0.03% | 1406.9 | 1405.5 | -0.10% |
| doom | default | 109.316 | 109.335 | 0.02% | 2012.2 | 2004.2 | -0.39% |
| xstate-main | single | 6.187 | 6.181 | -0.09% | 211.3 | 211.1 | -0.09% |
| xstate-main | default | 7.402 | 7.397 | -0.08% | 266.4 | 266.1 | -0.13% |
| webpack | single | 11.013 | 11.014 | 0.00% | 324.0 | 324.3 | 0.08% |
| webpack | default | 12.983 | 13.024 | 0.31% | 406.9 | 406.7 | -0.05% |
| t3code-server | single | 48.255 | 48.176 | -0.16% | 1067.4 | 1067.1 | -0.03% |
| t3code-server | default | 86.904 | 86.798 | -0.12% | 1849.6 | 1846.8 | -0.15% |

Diagnostics were byte-identical to the base binary on every run. Noise everywhere, as the census predicts. Wall
time was not measurable that day (other work on the machine kept the load average above 250; instructions retired
are per process and unaffected).

Do not redo unless a workload has many 1-parameter distributions over recurring constituents and the budget can be
charged exactly (for example by evaluating with the counters saved and restored, which would need the same relation
and symbol-resolution side effects to be replayable).
