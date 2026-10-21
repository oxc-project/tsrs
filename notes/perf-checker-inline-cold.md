# Checker inline/cold balance: unknown-like union memo hits

Measured 2026-10-21 at `c4b7341b`. The checker asks `isUnknownLikeUnionType` from
`isSimpleTypeRelatedTo`; after the first question about a union, the answer is already held in
`IsUnknownLikeUnionComputed` / `IsUnknownLikeUnion`. The old out-of-line function nevertheless saved six arm64
register pairs on every call before reading those bits.

The memo-bit test is now `#[inline(always)]`, and only the first computation is an out-of-line `#[cold]` function.
`nm` confirms that the old function is present in the baseline binary and absent from the candidate; the candidate
contains only `compute_is_unknown_like_union_type`. The cold worker sets the computed bit before walking constituents,
as before, so recursive queries observe the same state.

## xctrace evidence

A clean symbolized release build was recorded with:

```sh
env CARGO_PROFILE_RELEASE_STRIP=none CARGO_TARGET_DIR=target/profiling \
  cargo build --release -p tsrs_cli
xcrun xctrace record --template 'Time Profiler' \
  --output /tmp/tsrs-compiler-unions-clean-single.trace --launch -- \
  "$PWD/target/profiling/release/tsrs" \
  -p "$PWD/bench/.work/suite/cases/solutions/Compiler-Unions/tsconfig.json" \
  --noEmit --incremental false --pretty false --singleThreaded
```

The project is short enough that the trace has only 41 useful checker samples, but
`is_unknown_like_union_type` is a direct 5 ms leaf sample at `0x103102491`, reached from
`is_type_related_to` during flow narrowing. `is_simple_type_related_to` is also sampled directly several times.
This confirms that the memo-hit call itself, rather than only a descendant, is retired work on the hot relation path.
The candidate trace has 190 rows and no `is_unknown_like_union_type` or
`compute_is_unknown_like_union_type` frame: the repeated path is inlined, and the cold first-computation path was not
sampled.

## Measurement

Clean fat-LTO release builds, the pinned public Compiler-Unions project, `RAYON_NUM_THREADS=1`, and
`--singleThreaded --noEmit --incremental false --pretty false`. `/usr/bin/time -al` on Apple Silicon reports retired
instructions including kernel work. After one warm-up per binary, eight runs were interleaved in ABBA order; values
are the median of four runs.

| | baseline | candidate | change |
| --- | ---: | ---: | ---: |
| retired instructions | 3,210,020,542 | 3,161,076,887 | **-1.525%** |
| peak RSS | 62,881,792 bytes | 62,455,808 bytes | -0.68% |

Compiler output is byte-identical: both exit 2 with 7,386 output bytes and SHA-256
`28ec666d70ccbf6dcbe1ab972273629599e667935274f62498f81c50893c9c43`.

An earlier unmerged branch, `perf/checker-unknown-like-union-fast-path`, independently measured the same split at
-1.44% instructions on Compiler-Unions and below 0.4% on MUI and MikroORM. The new trace and paired counts reproduce
its qualifying result on current `main`.
