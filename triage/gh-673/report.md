# Triage report for issue #673

## Reproduction

Created the issue's files in `/tmp/tsrs-issue-673` and confirmed the parity mismatch:

- **Pinned Go compiler (`b85298b6a8`)**: `a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'object'.`
- **tsrs (`target/debug/tsrs`)**: `a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'typeof import("/tmp/tsrs-issue-673/a")'.`

The mismatch persists with `--singleThreaded`, `--noLazyMembers`, and `TSRS_LAZY_MEMBERS=0`.

## Diagnosis

The difference is in how an empty CommonJS module instance type (`typeof import("...")`) is narrowed after a `typeof == "object"` test followed by a truthiness test.

The relevant flow is:

1. `exports` in a CommonJS file has a `FunctionScopedVariable | ModuleExports` symbol whose type is the module instance type, created as an anonymous object type with the module symbol in `get_type_of_func_class_enum_module_worker` (`crates/tsrs_checker/src/checker_08.rs:3136`).
2. The `typeof exports == "object"` condition narrows through `narrow_type_by_type_name` -> `narrow_type_by_type_facts(t, non_primitive_type, TypeFacts::TypeofEQObject)` (`crates/tsrs_checker/src/flow.rs:1498`).
3. `narrow_type_by_type_facts` checks whether `t` is strictly related to `nonPrimitiveType`; if so it returns `t`. For this check to fail (so that the function falls back to returning the more general `object` type), the module instance type must be recognized as an empty anonymous object.
4. Recognition happens in `is_empty_anonymous_object_type` (`crates/tsrs_checker/src/checker_13.rs:1408`), which only succeeds when `ObjectFlags::MembersResolved` is set **and** the resolved structured type is empty, **or** when the symbol is a `TypeLiteral` with no members. A CommonJS module symbol is a `ValueModule`, not a `TypeLiteral`, so the first branch is decisive.
5. The Go compiler at the pinned commit narrows to `object`, so its equivalent check must be returning `true` for this empty module instance. In tsrs it appears to return `false`, causing `narrow_type_by_type_facts` to keep the module instance type.

The same divergence does **not** occur for a plain `{}` literal (both compilers collapse to `object`), so the issue is specific to the module-instance type, not anonymous empty object types in general.

Because `--noLazyMembers` does **not** restore the Go behavior, the difference is not one of the tsrs-specific lazy-member candidates; it is a genuine porting gap likely around how/when the module-instance type is recognized as an empty anonymous object in the strict-subtype / empty-anonymous-object tests used by narrowing.

### Likely fix approach

Ensure that the empty CommonJS module instance type is treated as an empty anonymous object type during narrowing. Candidates:

- Adjust `is_empty_anonymous_object_type` (or its worker) to consider a `ValueModule` symbol whose resolved members are empty, similar to the existing `TypeLiteral` branch.
- Or, when creating the module instance type for an empty CommonJS module in `get_type_of_func_class_enum_module_worker`, populate it with an empty structured member set and set `MembersResolved` so the existing empty-anonymous-object test succeeds.

The affected crate is `tsrs_checker`.

## Confidence

Medium: the failure is isolated to the narrowing path and the empty-module-instance type, but the exact line where tsrs diverges from the Go `isEmptyAnonymousObjectType` / member-resolution timing has not been instrumented.

## Verification

Verdict: **bug**.

Performed a direct deterministic comparison between the pinned Go compiler and tsrs on the reporter's exact reproduction:

```
$ /.../tsgo -p . --pretty false
a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'object'.
a.js(2,9): error TS2591: Cannot find name 'fs'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.

$ /.../tsrs -p . --pretty false
a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'typeof import("/tmp/tsrs-issue-673/a")'.
a.js(2,9): error TS2591: Cannot find name 'fs'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.
```

Both compilers report TS2339 at the same span, but the printed type differs. The Go compiler (the specification per the project contract) says `object`; tsrs says `typeof import(...)`. The exit status is 2 for both. No capability in `docs/COMPATIBILITY.md` places diagnostic message text out of scope.

Confidence: high (direct comparison against the pinned Go compiler on the reporter's reproduction).

## Fix

Implemented a fix in `crates/tsrs_checker/src/checker_13.rs` that recognizes an empty CommonJS module instance type as an empty anonymous object type.

### Change

Extended `is_empty_anonymous_object_type_worker` so that, when the type has a `ValueModule` symbol whose resolved exports are empty, it reports the type as an empty anonymous object. This lets `narrow_type_by_type_facts` follow the same path as the pinned Go compiler: the simple strict-subtype relation is blocked, the type is intersected with `object`, and the intersection simplification drops the empty module wrapper, yielding `object` in the diagnostic.

```rust
if symbol.flags().intersects(SymbolFlags::ValueModule) {
    let exports = self.get_exports_of_symbol(symbol);
    return exports.is_some_and(|e| e.is_empty());
}
```

The rest of the function is unchanged and the TypeLiteral branch is preserved.

### Verification

- Re-run the original reproduction: tsrs now reports `type 'object'`, matching tsgo exactly.
- Non-empty CommonJS module (`exports.foo = 1`) still reports `typeof import(...)`, matching tsgo.
- `tools/regressions.sh target/debug/tsrs`: all regression tests pass, including the newly added `commonjs-exports-empty-module-narrowing`.
- `cargo test -p tsrs_checker`: 7 unit tests pass.
- `cargo clippy --all-targets --config 'build.warnings="deny"'`: passes.
- `python3 tools/lint/source.py`: passes.

### New regression test

Created `testdata/regressions/commonjs-exports-empty-module-narrowing/` with:

- `tsconfig.json`: `allowJs`, `checkJs`, `noEmit`, `types: []`
- `a.js`: the reporter's reproduction
- `expected.txt`: generated from the pinned Go compiler.

### Commands and results

```
$ cd /tmp/tsrs-issue-673 && /.../tsgo -p . --pretty false
a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'object'.
a.js(2,9): error TS2591: Cannot find name 'fs'. ...

$ cd /tmp/tsrs-issue-673 && /.../tsrs -p . --pretty false
a.js(1,58): error TS2339: Property 'nodeType' does not exist on type 'object'.
a.js(2,9): error TS2591: Cannot find name 'fs'. ...

$ bash tools/regressions.sh target/debug/tsrs
pass commonjs-exports-empty-module-narrowing
...

$ cargo test -p tsrs_checker
7 passed; 0 failed

$ cargo clippy --all-targets --config 'build.warnings="deny"'
Finished ...

$ python3 tools/lint/source.py
source checks: ok ...
```

Confidence in the fix: high for the reported reproduction and non-empty module case; broader conformance testing would be the next step in CI.
