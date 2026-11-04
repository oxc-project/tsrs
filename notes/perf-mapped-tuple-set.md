# perf-mapped-tuple-set: a one-element fast path for the mapped-tuple "set element" idiom (not landed)

Tried 2026-10-29 on main 7dc2c938f, ported from the tsgo-opt fork's `tryFastMappedTupleSet` (teamchong/TypeScript#1,
commit b6c366256).

The idea: in `{ [K in keyof T]: K extends "<i>" ? X : T[K] }` over a fixed tuple, every element but `i` resolves to
itself, so instantiate the replacement once and copy the other type arguments instead of instantiating the template
per element. The fork's guard compares the distributed `K` with the declared parameter, so it never fires there; the
port compared through `get_non_distributed_type_parameter`.

Result: exact (the fixture `testdata/regressions/mapped-tuple-set-element` matches tsgo; regressions, conformance and
diagnostics on six catalog projects unchanged), but under `--features tsrs_checker/site-counts` it fired 0 times on
doom, xstate-main, webpack, t3code-server, cal-diy and supabase-studio, and counters and instructions did not move.

Not landed: about 90 lines for a path no catalog project takes. Revisit if a project that stores into tuples through
this idiom joins `bench/projects.json`; the fixture already pins the semantics, including the shapes that must stay on
the per-element path.
