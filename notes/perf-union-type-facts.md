# getTypeFacts: a union of non-nullable kinds answers the nullability facts without the constituent walk

Measured 2026-10-25 at `a0db53ea` (main after #769) on an 18-core Apple Silicon machine shared with two other agents,
following the owner's performance guide (profile first, attribute, smallest change, before/after numbers). The
candidate is the first item of "Found on the way, not done" in notes/perf-narrowable-reference-union-memo.md (#715).

## Finding

`checkNonNullType` asks `getTypeFacts(type, IsUndefinedOrNull)` of the type of every property access, call and
element access target. For a union that is `getTypeFactsWorker` on each constituent: a load of the constituent's
flags somewhere else in the arena, a dozen flag tests, and for an object constituent the `callerOnlyNeeds` early
return (`possibleFacts`, Go `checker.go:31703`), which skips member resolution when the caller asks for none of the
object facts. On Compiler-Unions the `Node` type is a union of every node interface, so each `node.kind` walks it:
2.6% of single-threaded self time in `get_type_facts_worker` (#715's profile), 6,198 union calls over 534,631
constituents, 452 thousand of them in unions of plain object types, and the answer was "not nullable" every time.

No existing memo covers it. `PrimitiveUnion` excludes object constituents, `couldContainTypeVariables` (what #715
read) says nothing about `undefined`, `null` or `void`, and nothing records the kinds of a union's constituents.

## Change

`ObjectFlags::NonNullableUnion` (types.rs), a union bit set by `get_union_type_from_sorted_list` when every
constituent's flags are within `NON_NULLABLE_KINDS` (checker_13.rs): string, number, bigint, boolean, enum, symbol,
object and non-primitive kinds, literal or not, that is `TypeFlags::DefinitelyNonNullable` without the instantiable
primitives (template literal, string mapping). The bit is computed from the final sorted list, after `new_type`
(which clears the memo bits that share bits 30-31 on object types and intersections), so a union that `filterType`
or `removeType` builds from part of another union gets its own answer; the builder already walks the constituents
for `getPropagatingFlagsOfTypes`, and the extra OR of their flags reads headers it just loaded.

`get_type_facts_worker` (checker_15.rs), union case: when the union has the bit and the caller needs none of the
object facts (`possible_object_type_facts`, the same test the object case makes), it returns `None` without the
walk. The constituent fold moved into `get_union_type_facts`, which the fallback and the debug check share.

`set_structured_type_members` (checker_12.rs) cleared `WeakTypeMemo` on every type whose members it set, unions
included (`resolveUnionTypeMembers` runs for `node.kind`), which would have dropped the bit from exactly the hot
unions. The memo lives on object types only, so the clear is now conditional on `TypeFlags::Object`; a side effect
is that an intersection keeps its `ShouldNormalize` memo across member resolution (its answer depends only on the
constituents, so that memo was never invalid).

## Why that is exact

The caller masks the worker's result with `callerOnlyNeeds` (`getTypeFacts`), and the object case already returns
`None` for facts outside that mask, so the worker's contract is "correct within `callerOnlyNeeds`"; `and` and `or`
of per-constituent results commute with the mask, so the contract holds through the union fold and the intersection
case too.

The shortcut fires only when the mask has no object fact. Under `strictNullChecks` that leaves masks within
`EQUndefined | EQNull | EQUndefinedOrNull | IsUndefined | IsNull` (everything else is in `EmptyObjectStrictFacts`);
without it, within `IsUndefined | IsNull`. For every kind in `NON_NULLABLE_KINDS` the worker returns a constant
from the flags, and none of those constants has those bits: the strict base facts of string, number, bigint,
boolean and symbol kinds hold only `TypeofEQ`/`TypeofNE` and `NE` facts plus `Truthy`/`Falsy`, the non-strict ones
add the `EQ` facts but never `IsUndefined`/`IsNull`, an object constituent takes the early return (`None`), and
`object` (`NonPrimitive`) gets the object facts. So the full walk's result within the mask is `None`, and the walk
resolves nothing: no constituent reaches `getBaseConstraintOfType` (no intersection, no instantiable type), and the
object case resolves members only when the caller asks for object facts. The shortcut therefore changes no
resolution order, in either direction.

Every other kind is excluded: `undefined`, `null` and `void` carry the facts (`UndefinedFacts`, `NullFacts`,
`VoidFacts` has `IsUndefined`); an intersection or an instantiable type (template literals and string mappings
included, they are `InstantiablePrimitive`) goes through its base constraint first; `any` and `unknown` have the
`EQ` facts (`UnknownFacts = All & ~IsUndefinedOrNull`); `never` is dropped by the union builder. A union is never a
constituent (`addTypeToUnion` flattens), and a `true | false` union, which carries `TypeFlags::Boolean`, takes the
boolean case before the union case as before.

Masks with an object fact (`NEUndefinedOrNull` in `getNonNullableType`'s filter, the flow narrowing's `NE` facts)
walk as before: for them the full walk resolves the object constituents' members, and skipping that would change
resolution order. A debug-build assertion (`cfg!(debug_assertions)`) runs the walk after the short answer and checks
that its masked result is `None`; `cargo test -p tsrs_checker`, the 50 regression cases and the four projects below
ran a debug binary with it.

Related earlier work: notes/perf-icount-hillclimb.md round 4 did not build a memo of *intersection* type facts
because an intersection's base constraint can be mid-resolution; this summary covers unions only and never touches
base constraints.

## Attribution

A temporary counter in the union case of the worker (not committed), one single-threaded debug run per project:

| project | union calls | mask with an object fact (walk as before) | eligible | answered by the summary | constituents skipped | walked | of them with `undefined`, `null` or `void` | with an intersection or instantiable constituent |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 6,775 | 560 | 6,215 | 6,207 | 534,910 | 8 (628 constituents) | 0 | 8 |
| Compiler | 4,728 | 332 | 4,396 | 4,377 | 105,408 | 19 (434) | 0 | 19 |
| typeorm | 4,129 | 2,491 | 1,638 | 594 | 4,263 | 1,044 (2,845) | 983 | 63 |
| mui-docs | 3,908 | 2,604 | 1,304 | 792 | 5,416 | 512 (2,023) | 336 | 186 |

On the two compiler corpora (`strictNullChecks` off, so no union holds `undefined` or `null`) the summary answers
99.9% of the eligible constituents; the walked unions hold an intersection or a type parameter. On the two strict
application projects most union calls ask for `NE` facts (walk as before, see above) and most eligible unions that
walk really are nullable (the `x?.y` and error paths), so there is little to take and nothing lost.

## Measurement

Clean fat-LTO `--release` builds with the stock profile, `/usr/bin/time -l` instructions retired (kernel included;
the macOS screening instrument of notes/perf-icount-round4.md), `--noEmit --incremental false --pretty false`,
interleaved base/candidate pairs with the order alternating each round, medians. Diagnostics (exit code and stdout
bytes) identical in every run. The machine was shared with two other agents' builds and measurements throughout
(load average 50-150), so wall time and cycles are not reported.

Single-threaded (`--singleThreaded`, `RAYON_NUM_THREADS=1`), seven pairs:

| project | instructions base | candidate | change | paired median | peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 2.9032 G | 2.8714 G | **-1.10%** | -1.03% (lower in 7 of 7 pairs) | 59.0 -> 59.0 MiB |
| Compiler | 1.7035 G | 1.6973 G | **-0.36%** | -0.36% (lower in 7 of 7 pairs) | 55.0 -> 55.0 MiB |
| typeorm | 8.2316 G | 8.2087 G | -0.28% | -1.11% (4 of 7; base alone spread 7.72-8.64 G) | 192.0 -> 192.0 MiB |
| mui-docs | 38.583 G | 38.570 G | -0.03% | +0.17% (2 of 7; base alone spread 37.8-40.2 G) | 739.5 -> 739.5 MiB |

Default checker count (16 on this machine), three pairs:

| project | instructions base | candidate | change | peak RSS base | candidate |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 3.6247 G | 3.6000 G | -0.68% (lower in 3 of 3) | 82.6 MiB | 82.1 MiB |
| Compiler | 2.0513 G | 2.0432 G | -0.40% (2 of 3) | 77.5 MiB | 77.7 MiB |
| typeorm | 9.3314 G | 8.8986 G | -4.6% (3 of 3; the count moves several percent between runs at this size) | 261.2 MiB | 260.5 MiB |
| mui-docs | 94.756 G | 94.673 G | -0.09% (1 of 3) | 1,567.8 MiB | 1,571.5 MiB |

The change adds no field (one bit of the union's existing `ObjectFlags` word) and allocates nothing, so the peak
RSS columns are the run-to-run band. The Linux gate on the pull request is the landing number; these are the
screening numbers.

Why about 1% and not 2.6%: the walk is one flags load per constituent and a few tests, so it is cheap in
instructions and expensive in cache misses (each constituent header is elsewhere in the arena); the 2.6% was
sampled self time, which counts the misses. Cycles could not be measured on the shared machine.

## Found on the way, not done

- A 32-bit OR of constituent flags per union, which would also answer `maybeTypeOfKind` for unions without
  intersection constituents: `UnionType` is 24 bytes with no padding and the `Type` header has 3 spare bytes, so
  it costs a word per union, which adds up on a large private monorepo. One bit answers the hot question.
- Answering the `NE` masks from the summary (`getNonNullableType`'s filter): exact in value for these kinds, but
  the walk resolves the object constituents' members for them, so the shortcut would skip resolution.
- `getIntersectionTypeFacts` is unchanged (the base-constraint concern above).
