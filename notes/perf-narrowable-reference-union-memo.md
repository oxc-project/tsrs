# getNarrowableTypeForReference: a union known to hold no type variable skips the constituent walk

Measured 2026-10-22 at `c6695157` (main after #680 and #699) on an 18-core Apple Silicon machine shared with two
other agents, following the owner's performance guide (profile first, attribute, smallest change, before/after
numbers).

## Finding

A CPU-time profile (`samply`, single-threaded, `RAYON_NUM_THREADS=1 --singleThreaded`) of the four projects is flat
after #680: no checker function is above 4% of self time, and the top of each list is the kernel's file reads, the
symbol tables and the flow walk. Self time of the checker functions a small change could reach, Compiler-Unions:

| self | function |
| ---: | --- |
| 3.2% | `SymbolMap::search` |
| 2.9% | `get_unmatched_properties_worker_ex` (its own loop plus the inlined property lookups) |
| 2.7% | `is_type_related_to` |
| 2.6% | `get_type_facts_worker`, all of it from `check_non_null_type_with_reporter` |
| 2.6% | `get_type_at_flow_node` |
| 2.4% | `is_reachable_flow_node_worker` |
| 1.6% | `is_simple_type_related_to_worker` |
| 1.6% | `is_generic_type_with_union_constraint`, all of it from `get_narrowable_type_for_reference` |

Compiler's list is led by `SymbolMap::position` (3.9%) and the inline enum-member entry of #680 (3.0%, the calls
themselves; the shapes that would cut them were measured and dropped in notes/perf-simple-relation-enum-members.md);
mui-docs and typeorm by `read`/`stat` (the single-threaded front end), `SymbolMap::search` and
`get_property_of_type_worker`.

`is_generic_type_with_union_constraint` is one instruction in the profile: the load of each constituent's flags.
`getNarrowableTypeForReference` asks `someType(type, isGenericTypeWithUnionConstraint)` of the declared type of
every reference, and for a union that is a call per constituent, each reading a `Type` header somewhere else in
the arena. On Compiler-Unions the `Node` type is a union of every node interface, so every `node` reference walks
it.

## Attribution

A counter at both call sites (`get_narrowable_type_for_reference` and the flow memo's
`get_narrowable_type_for_flow_reference`), one single-threaded check each; typeorm and mui-docs are the counts at
the first 200,000 calls:

| project | calls | union types | constituents walked | on unions with `couldContainTypeVariables` computed false | not computed | computed true | walk answered true |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 74,000 | 27,494 | 2,628,687 | 16,284 unions, 2,035,151 constituents (77%) | 581,372 (22%) | 12,164 | 143 |
| Compiler | 79,000 | 17,293 | 773,239 | 8,112 unions, 658,524 (85%) | 103,063 | 11,652 | 36 |
| typeorm | 200,000 | 27,017 | 215,030 | 14,503 unions, 133,014 (62%) | 77,466 | 4,550 | 135 |
| mui-docs | 200,000 | 16,595 | 105,425 | 10,129 unions, 62,749 (60%) | 31,676 | 11,000 | 144 |

The walk answers true for about one call in a thousand. Three quarters of the constituents it visits belong to a
union whose `couldContainTypeVariables` memo (`ObjectFlags::CouldContainTypeVariablesComputed`, set by
`instantiateType` and its carry-forward rules, as in Go) is already computed and false.

## Change

`some_generic_type_with_union_constraint` (checker_15.rs) answers `someType(t, isGenericTypeWithUnionConstraint)`
at both call sites. For a union whose memo is computed and false it answers false without the walk; everything else
walks as before. The memo is read, never computed: computing it resolves type arguments, which the walk does not do
(`is_const_type_variable` in checker_07.rs reads it the same way).

Why that is exact: the memo is true whenever a constituent is instantiable, except for a non-generic top-level type
alias (`isNonGenericTopLevelType`), whose constituents cannot name a type variable; the instantiable constituents
such an alias can hold are pattern literal types (`` `${number}px` ``, `Uppercase<...>` over one), and each is its
own base constraint, so `isGenericTypeWithUnionConstraint` is false for them too. A debug-build assertion runs the
walk after the short answer and checks that it agrees; `cargo test -p tsrs_checker`, the regression cases and the
measured projects exercise it.

## Measurement

Clean fat-LTO `--release` builds with debug info, `/usr/bin/time -l` instructions retired (kernel included; the
macOS screening instrument of notes/perf-icount-round4.md), `--noEmit --incremental false --pretty false`, one
warm-up then interleaved base/candidate pairs, medians. Diagnostics (exit code and output bytes) identical in every
run. The machine was shared with two other agents' builds and measurements throughout, so wall time is not
reported: the single-threaded mui-docs runs ranged from 3.0 to 8.0 s on both binaries.

Single-threaded (`--singleThreaded`, `RAYON_NUM_THREADS=1`), seven pairs:

| project | instructions base | candidate | change | paired median | peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 2.9485 G | 2.9019 G | **-1.58%** | -1.62% (lower in 7 of 7 pairs) | 59.2 -> 59.1 MiB |
| Compiler | 1.7249 G | 1.7092 G | **-0.91%** | -0.89% (lower in 7 of 7 pairs) | 55.7 -> 55.7 MiB |
| typeorm | 8.4045 G | 8.3712 G | -0.40% | -0.06% (base alone spread 8.34-8.46 G) | 195.1 -> 195.1 MiB |
| mui-docs | 39.567 G | 39.528 G | -0.10% | -0.04% (base alone spread 39.4-41.5 G) | 759.7 -> 759.7 MiB |

Default checker count (16 on this machine), five pairs:

| project | instructions base | candidate | change | peak RSS base | candidate |
| --- | ---: | ---: | ---: | ---: | ---: |
| Compiler-Unions | 3.6703 G | 3.6196 G | -1.38% | 83.4 MiB | 83.2 MiB |
| Compiler | 2.0449 G | 2.0550 G | +0.50% (paired median -0.50%; the count moves 2-3% between runs at this size) | 77.4 MiB | 78.2 MiB |
| typeorm | 9.5319 G | 9.4856 G | -0.49% | 267.5 MiB | 267.0 MiB |
| mui-docs | 96.799 G | 96.985 G | +0.19% | 1,652.7 MiB | 1,660.4 MiB |

The change allocates nothing, so the peak RSS columns are the run-to-run band. The Linux `bench/count.py` gate on
the pull request is the landing number; these are the screening numbers.

## Found on the way, not done

- `checkNonNullType` asks `getTypeFacts(type, IsUndefinedOrNull)` of the same unions: 2.6% of self time on
  Compiler-Unions, 6,198 union calls over 534,631 constituents, 452 thousand of them in unions of plain object
  types, and the answer was "not nullable" every time. No existing memo covers it (the facts depend on each
  constituent's own flags), so a shortcut would be a new per-union summary of constituent flags, an invariant the
  union builder would have to keep. Left for a round that can justify one.
- `get_unmatched_properties_worker_ex` is on every project's list (0.7% to 2.9%): the loop over the target's
  properties with the hashed source lookups inlined. Nothing in it answers a common case trivially.
