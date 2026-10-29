# port-tsgo-opt-conditional-items: conditional and mapped-type fast paths of the tsgo-opt fork (rejected)

2026-10-29, base main 7dc2c938f. Five mechanisms of the teamchong/TypeScript "tsgo-opt" fork (commits 9b498ffd8 and
cb6d5dc3c there) around `getConditionalTypeInstantiation`, `getConditionalType`, `instantiateMappedType` and
`isTypeParameterPossiblyReferenced` were examined, four of them ported, gated and measured. None is landed; the
regression fixture `testdata/regressions/conditional-distribution-mapper-order` is. Measurements are release builds, single-threaded
instructions retired (min of 3 interleaved base/candidate runs, `/usr/bin/time -l`) and peak RSS (median), default
checkers the same, diagnostics byte-identical to the base binary on every run. Other work kept the machine's load
average above 100 all day, so run-to-run spreads were 0.5-2% instead of the usual 0.001%, and wall time was not
measurable; the deltas below are within that noise.

## 1-parameter distribution: simple mapper per constituent

For a distributive root with one outer type parameter `T` whose check type is `T` (`getNonDistributedTypeParameter`),
the fork evaluates each constituent `t` of the distributed union with `newSimpleTypeMapper(T, t)` instead of
`prependTypeMapping(checkType, t, newMapper)` (a merged mapper of `[T -> t]` over `[T -> arg]`).

Not exact. The two mappers map every type the same way, but a deferred result keeps its mapper
(`ConditionalType.mapper`), and `compare_type_mappers` (utilities.go `CompareTypes`, the order of union constituents
and so of type text) orders by mapper kind before anything else. A union that mixes a distributed constituent with a
directly instantiated one changes order:

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
instantiations build their own from the type arguments). That form saves one merged mapper per non-generic
constituent and the type-argument list of the root mapper; see the census below for why it is not worth having.

## 1-parameter distribution: per-constituent cache

The fork keeps each constituent's result in `root.instantiations` under `packedSingleTypeKey(t, forConstraint)`, the
key of the one-argument instantiation `[t]` without alias (the same evaluation: that instantiation reduces `t` to
itself and calls `getConditionalType(root, [T -> t], forConstraint, nil)` too), so a constituent evaluated for one
union is reused by every other union that contains it.

Not exact. A hit skips the constituent's evaluation, so `instantiation_count` and `total_instantiation_count` (the
TS2589 budget, `--extendedDiagnostics` "Instantiations") drop by that evaluation's cost. The cost is not a constant
that a hit could charge (notes/perf-checker-algorithms.md, "Rejected: it moves the instantiation-budget boundary"):
the first evaluation of a constituent instantiates its permissive/restrictive forms and resolves what its relation
needs, later ones hit those caches. The recipe the repo's other memos use (store only when the evaluation created no
type, instantiated nothing, reported nothing: unioncache.rs, the indexed-access union memo, flowmemo.rs) can never
hold here, because every evaluation instantiates the check type at least once. Type identity is not the problem:
`CompareTypes` orders structurally, so deduplicating structurally identical results does not reorder a union; only
the counters do.

### Census and measurement

Work census of doom (`TSRS_WORK_CENSUS`, one checker, base binary): conditional instantiation miss path 571,865
calls (245,665 outer), 3,991 distributions over 229,271 constituents, of which 222,963 are one `Exclude<..>`
distribution (two outer parameters, not on this path). About 6,000 constituents of 1-parameter roots per check:
nothing to save. The heavy roots (`_ProcessInputStack`, `executeInstruction`, `selectInstruction`, `Length`,
`GarbageCollect`) do not distribute at all.

Both mechanisms together (the cache guarded by "created no type, reported nothing"):

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

## Deferred conditional types: no second instantiation of the check and extends types

`getConditionalType` instantiates the check and extends types at the top of its loop and, for a deferred result,
`newConditionalType` instantiates `root.checkType` and `root.extendsType` with the same mapper again. The fork
passes the first results in (`rawCheckType` is re-instantiated only when `root.checkType` is a substitution type,
whose actual type variable the loop instantiated instead).

Exact only with the budget charged: the second instantiations create nothing (the first ones populated every keyed
cache they touch, the mapper is not active at that point, so nothing is stored in an active mapper's cache either)
but count: a type parameter check type is one instantiation, a generic extends type one plus its generic
subterms. The port recorded `total_instantiation_count` across the two first instantiations and charged that at the
deferred point (making the real instantiations instead when the charge would reach the 5,000,000 budget, so TS2589
reports as in Go);
`conditional-instantiation-limit-at/-under` and the conformance suite stayed identical. Expected gain: two
`push_active_mapper`/worker calls per deferred conditional type. Measured:

| project | mode | base Ginst | cand Ginst | inst delta | base RSS MiB | cand RSS MiB | RSS delta |
| --- | --- | --- | --- | --- | --- | --- | --- |
| doom | single | 75.447 | 75.530 | 0.11% | 1405.6 | 1405.6 | 0.00% |
| doom | default | 109.624 | 109.556 | -0.06% | 2005.7 | 2003.6 | -0.11% |
| xstate-main | single | 6.218 | 6.247 | 0.47% | 211.3 | 211.3 | 0.01% |
| xstate-main | default | 7.493 | 7.472 | -0.28% | 266.7 | 266.5 | -0.05% |
| webpack | single | 11.078 | 11.051 | -0.24% | 323.8 | 323.9 | 0.03% |
| webpack | default | 13.035 | 13.005 | -0.23% | 406.3 | 406.4 | 0.03% |
| t3code-server | single | 48.333 | 48.243 | -0.19% | 1066.6 | 1067.5 | 0.09% |
| t3code-server | default | 87.482 | 87.567 | 0.10% | 1847.5 | 1849.7 | 0.12% |

Noise, and 25 lines of budget bookkeeping for it; not landed. Redo on a quiet machine if deferred conditional types
ever show up in a profile (they are rare in type-level code like doom, which resolves its conditionals).

## Possibly referenced outer type parameters: one walk instead of one per parameter

`getTypeFromConditionalTypeNode` and `getObjectTypeInstantiation` keep the outer type parameters that
`isTypeParameterPossiblyReferenced` says the node may mention, one subtree walk per parameter. The fork's
`filterPossiblyReferencedTypeParameters` walks the subtree once, collecting the symbols of the type references
without type arguments and whether a `this` type occurs, and answers each parameter from that (falling back to the
per-parameter walks when the subtree has a type query or a method, the two cases whose answer depends on the
parameter); the per-parameter prefix (one declaration, no block and no conditional extends type mentioning the
parameter between the node and the declaration's container) stays.

Exact: the same set for every candidate (the predicate's `containsReference` matches exactly those node kinds),
no types or instantiations, only the order in which the ignore-errors symbol resolution of the references runs
(all at once instead of up to the first match per parameter). Ported as `filter_possibly_referenced_type_parameters`
(80 lines) and measured:

| project | mode | base Ginst | cand Ginst | inst delta | base RSS MiB | cand RSS MiB | RSS delta |
| --- | --- | --- | --- | --- | --- | --- | --- |
| doom | single | 75.374 | 75.411 | 0.05% | 1405.6 | 1405.5 | 0.00% |
| doom | default | 109.584 | 109.557 | -0.02% | 2000.3 | 1995.5 | -0.24% |
| xstate-main | single | 6.213 | 6.214 | 0.00% | 211.3 | 211.3 | 0.00% |
| xstate-main | default | 7.409 | 7.418 | 0.13% | 265.5 | 265.7 | 0.05% |
| webpack | single | 11.060 | 11.075 | 0.13% | 324.3 | 323.6 | -0.20% |
| webpack | default | 13.060 | 13.054 | -0.05% | 406.2 | 406.3 | 0.02% |
| t3code-server | single | 48.603 | 48.578 | -0.05% | 1067.7 | 1066.1 | -0.14% |
| t3code-server | default | 87.518 | 87.547 | 0.03% | 1843.8 | 1847.4 | 0.19% |

Noise: the filter runs once per conditional type node and once per anonymous type (`outer_type_parameters` is
cached on the node's links), over subtrees with one to three outer parameters. Not landed.

## Homomorphic mapped types: per-constituent instantiation cache

In `instantiateMappedType`'s `instantiateConstituent`, when the mapper is simple (`[T -> arg]`, which is the case
exactly when the mapped type has one outer type parameter, since `getObjectTypeInstantiation` builds the mapper from
the outer parameters) and the constituent `s` is neither the type variable nor `arg` itself, the fork keeps the result
of `instantiateAnonymousType(t, prependTypeMapping(T, s, m), nil)` in `t.instantiations` under the key of the
one-argument instantiation `[s]`, with `getObjectTypeInstantiation`'s `CouldContainTypeVariablesComputed`
propagation copied for the entry.

Sound as a mapping (`prependTypeMapping(T, s, [T -> arg])` maps every type as `[T -> s]` does, so the result depends
on `t` and `s` only; the whole-path entry for `[s]` without alias holds the same instantiation), but not exact by
this repository's standard (notes/perf-t3code-instantiation.md: the same types created in the same order, identical
`--extendedDiagnostics` counters): `instantiateAnonymousType` makes a fresh mapped type on every call, so a hit
creates fewer types, and the reused type keeps the mapper of its first union (`compare_type_mappers` reads it when
two constituents tie on everything else). The memo recipe cannot hold for a call that always creates a type. Not
ported.
