# Callback constraint inspection in no-misused-promises

Ports [tsgolint PR #1334](https://github.com/oxc-project/tsgolint/pull/1334), commit
`10902acbcf2b5caf14ed08f667311461c8901692`. Querying signatures on a generic callback constraint can synthesize
combined union signatures and cross-product intersections. The rule now reduces the apparent type before
recursively inspecting its union members. It also skips parameter positions without arguments while continuing
through every overload, preserving Promise-accepting callbacks.

The first upstream reproduction creates 131,263 types in the original Rust rule. The integration regression
requires at most 4,096 types per call and compares complete Go diagnostic records for all nine upstream cases:
generic and optional constraints, missing arguments, constructors, Promise-accepting overloads and an impossible
intersection. Its [reference record](../crates/tsrs_lint/tests/fixtures/no_misused_promises/REFERENCE) identifies
the Go lint oracle separately from the pinned TypeScript compiler; all inputs compile without errors with
TypeScript source `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.

Existing performance notes, the rejected-experiment list in `perf-round2-followups.md`, the Rust techniques list,
and `mem-round4.md` were checked before the change. No previous rejected experiment covered this rule-local
signature expansion.

## Measurements

Apple M4 Pro, 12 logical CPUs, 48 GiB RAM. Base `eba609620c4120c3a0a45f302483cb0355c35f10` and candidate use the
same release configuration: full debug information, stripping disabled, fat LTO, one codegen unit, no PGO/BOLT.
Build with `cargo build --release --config profile.release.debug=true --config profile.release.strip=false
-p tsrs_lint --bin tsrs-lint`. Measurements use `bench/tsgolint.mjs`, macOS `time -l` hardware counters, one warmup
and five alternating base/candidate pairs per checker mode. The final batch ran after builds and tests finished.
The single mode sets `RAYON_NUM_THREADS=1`; default mode uses the normal checker count. Values below are medians.

| Input | Single-thread instructions, G (base → candidate) | Default peak RSS, MiB (base → candidate) | Default wall, ms (base → candidate) |
| --- | ---: | ---: | ---: |
| Generic callback, 16 union members | 0.703 → 0.174 | 55.72 → 29.91 | 49.10 → 21.91 |
| webpack | 15.272 → 15.263 | 418.55 → 418.27 | 446.76 → 451.41 |
| xstate-main | 13.574 → 13.474 | 624.44 → 620.00 | 432.18 → 425.68 |
| drizzle-orm | 7.514 → 7.531 | 434.11 → 435.67 | 309.16 → 311.17 |

The reproduction reduces instructions by 75.2% and peak RSS by 46.3%. Doubling its union from 8 to 16 members
changes base instructions from 175,695,320 to 703,210,204 (4.002×); candidate instructions change from 173,955,785
to 174,069,352 (1.001×). These are end-to-end measurements including startup, rather than an asymptotic proof.

All three projects in the dedicated lint benchmark catalog (`bench/lint.mjs`) were measured at their
`bench/projects.json` commits, with all 61 rules and compiler diagnostic reporting off: webpack (909 files),
xstate-main (248), drizzle-orm (448). Their full diagnostic multiset hashes match base/candidate in each checker
mode. Default and single modes can differ from one another because checker history differs.

Public-project changes are small and mixed. Single-thread instructions decreased in every measured pair on
webpack and xstate-main. Drizzle-orm's final median increased 0.23%, with peak RSS up 0.36%; an earlier screening
batch had instructions down 0.05%. No general end-to-end project speedup is claimed. The remaining compiler
benchmark catalog was not measured because this change only affects a lint rule.

## Validation

All 7,933 imported active lint cases pass (28 upstream skips), as do the other 120 lint integration tests, the
62 focused compiler overload variants and their available error/type/symbol/emit baselines, workspace Clippy,
source checks and formatting checks. The existing content-mapper integration test needs an unsandboxed Node
subprocess and passed in that environment. Three independent reviewers completed one read-only wave covering
semantics, complexity/API consumers, regression coverage and comments; no production findings remained.
