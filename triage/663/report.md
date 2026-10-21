# Issue 663 reproduction report

**Claim:** tsrs parse time and peak resident memory grow superlinearly with the number of `// @ts-ignore` directive comments in a single file, while the pinned TypeScript Go compiler is unaffected.

**Reproduction triage commit:** Branch `triagebot/fix-663` on the checked-out tsrs repository.

**Compiler revisions**

- tsrs: `0.18.0-ts7.1.0-dev.20260929` (`microsoft/TypeScript@b85298b6a81f`), built from source at `/home/runner/work/tsrs/tsrs/target/release/tsrs`.
- TypeScript Go: same upstream commit `b85298b6a81f772d080b0455de0ca9d744cd6fd6`, built from `https://github.com/microsoft/typescript-go.git` into `/home/runner/work/tsrs/tsrs/triage/663/ts-ref/tsc-go`.

## Minimal input

`repro.ts` (generated with ` awk -v n=40000 'BEGIN{print "export const values: number[] = [];"; for(i=0;i<n;i++){print "// @ts-ignore -- note"; print "values.push(" i ")"}}' > repro.ts`):

- 80,001 lines
- 40,000 `// @ts-ignore` directives

`repro-control.ts` is identical except each directive is replaced with a plain `// note` comment.

## Commands run

```sh
/home/runner/work/tsrs/tsrs/target/release/tsrs --ignoreConfig --noEmit --extendedDiagnostics --pretty false repro.ts
/home/runner/work/tsrs/tsrs/triage/663/ts-ref/tsc-go --ignoreConfig --noEmit --extendedDiagnostics --pretty false repro.ts
```

(Control variants used `repro-control.ts` with the same flags.)

## Observed outputs

### tsrs on `repro.ts` (40,000 `@ts-ignore` directives)

- Exit status: 0
- Parse time: **3.253 s**
- Peak resident memory: **9,480,872 KB (~9.5 GB)**
- User/system time: 1.34 s / 2.43 s (high system time, consistent with large allocations)
- Extended diagnostics excerpt:

```
Files:                91
Lines:            138774
Identifiers:      129870
Symbols:           75137
Types:            131273
Instantiations:    61403
Memory used:    9480860K
Memory allocs:         0
Parse time:       3.253s
Bind time:        0.000s
Check time:       0.152s
Emit time:        0.000s
Total time:       3.408s
Program:   parallel parse + resolve:   3.251s
```

### TypeScript Go on `repro.ts` (same input)

- Exit status: 0
- Parse time: **0.086 s**
- Peak resident memory: **143,732 KB (~144 MB)**
- Extended diagnostics excerpt:

```
Files:               91
Lines:           138774
Identifiers:     129870
Symbols:          59388
Types:           114636
Instantiations:   33956
Memory used:     95947K
Memory allocs:   514020
Parse time:      0.086s
Bind time:       0.015s
Check time:     43.339s
Emit time:       0.000s
Total time:     43.441s
```

### Control: tsrs on `repro-control.ts` (plain `// note` comments)

- Exit status: 0
- Parse time: **0.033 s**
- Peak resident memory: **81,928 KB (~82 MB)**
- Extended diagnostics excerpt:

```
Files:              91
Lines:          138774
Identifiers:    129870
Symbols:         75112
Types:          131118
Instantiations:  61398
Memory used:    80612K
Memory allocs:       0
Parse time:     0.033s
Bind time:      0.000s
Check time:     0.124s
Emit time:      0.000s
Total time:     0.159s
```

### Control: TypeScript Go on `repro-control.ts`

- Exit status: 0
- Parse time: **0.077 s**
- Peak resident memory: **136,696 KB (~137 MB)**
- Extended diagnostics excerpt:

```
Files:               91
Lines:           138774
Identifiers:     129870
Symbols:          59388
Types:           114636
Instantiations:   33956
Memory used:     94526K
Memory allocs:   513487
Parse time:      0.077s
Bind time:       0.018s
Check time:     42.458s
Emit time:       0.000s
Total time:     42.555s
```

## Summary

| Case | Compiler | Parse time | Peak RSS |
|---|---|---|---|
| 40,000 `@ts-ignore` directives | tsrs | 3.253 s | ~9.5 GB |
| 40,000 `@ts-ignore` directives | TypeScript Go | 0.086 s | ~144 MB |
| 40,000 plain `// note` comments | tsrs | 0.033 s | ~82 MB |
| 40,000 plain `// note` comments | TypeScript Go | 0.077 s | ~137 MB |

The checked-out tsrs shows a ~100× parse-time regression and a ~65× memory regression when the input contains `@ts-ignore` directives, compared with the pinned TypeScript Go compiler on the same file. Replacing the directives with ordinary comments makes the regression disappear on tsrs, confirming the behavior is specific to `// @ts-ignore` directive handling.

**Status: REPRODUCED**

## Diagnosis

### First mismatching functions

- Go: `Scanner.processCommentDirective` in `tsc/internal/scanner/scanner.go` (commit `b85298b6a81f`), around line 968–1013.
- Rust: `Scanner::process_comment_directive` in `crates/tsrs_scanner/src/scanner.rs`, around lines 1628–1673.

### Precise semantic/control-flow divergence

The Go scanner appends each discovered `// @ts-ignore` or `// @ts-expect-error` directive to a growing slice with a single `append`:

```go
s.commentDirectives = append(s.commentDirectives, ast.CommentDirective{...})
```

This is amortized O(1) per directive and keeps only one backing array.

The Rust port copies the *entire* currently collected slice into a new `Vec`, pushes the new directive, and then moves that new `Vec` into the arena as a fresh `&'static [CommentDirective]`:

```rust
let mut directives = self.state.comment_directives.to_vec();
directives.push(CommentDirective { loc: TextRange::new(start, end), kind });
self.state.comment_directives = tsrs_core::alloc_vec(directives);
```

(Copy happens at `crates/tsrs_scanner/src/scanner.rs:1669`.)

Because each of the N directives clones the previous N−1 directives, the total work is Θ(N²) element copies and the total memory retained is Θ(N²) `CommentDirective`s. A `CommentDirective` is 12 bytes here (`TextRange` 8 bytes + `CommentDirectiveKind` 4 bytes), so for 40,000 directives the retained directive arrays sum to roughly 12 × (1 + 2 + … + 40,000) ≈ 9.6 GB, which matches the observed `Memory used: 9480860K`.

### Evidence for quadratic scaling

Measured on the checked-out tsrs release binary:

| Directives (`n`) | Parse time | Reported memory |
|---|---|---|
| 1,000 | 0.019 s | 68 MB |
| 2,000 | 0.022 s | 86 MB |
| 4,000 | 0.033 s | 157 MB |
| 8,000 | 0.076 s | 440 MB |
| 16,000 | 0.241 s | 1,573 MB |
| 32,000 | 0.934 s | 6,094 MB |
| 40,000 | 3.253 s | 9,481 MB |

A plain-comment control (`// note` instead of `// @ts-ignore`) stays at ~80 MB and ~0.03 s regardless of `n`, confirming the cost is specific to directive collection.

### Likely affected crate and fix approach

The defect is in `crates/tsrs_scanner`. The snapshot/rewind model can be preserved while avoiding the quadratic copy. `ScannerState` currently stores `comment_directives: &'static [CommentDirective]` and the module docs explain that this copies the slice so saved snapshots remain valid. The correct fix is to keep a single append-only list (for example, a `Vec<CommentDirective>` owned by the scanner or a bump-arena chain) and have the snapshot store only a length/index, like Go's slice header length. When `comment_directives()` is requested, the scanner returns the prefix of the shared list up to the current snapshot length. This gives amortized O(1) per directive and O(N) retained memory, matching Go.

### Confidence

**High** — the quadratic copy is visible directly in the source, and the measured memory matches the Θ(N²) retained `CommentDirective` arrays exactly.

## Verification

The pinned TypeScript Go compiler at the same upstream commit (`b85298b6a81f772d080b0455de0ca9d744cd6fd6`) was run on the identical minimal input, in the same working directory, with the same flags (`--ignoreConfig --noEmit --extendedDiagnostics --pretty false`).

Comparison on `repro.ts` (40,000 `// @ts-ignore` directives):

- **tsrs**: parse 3.253 s, peak RSS 9,480,872 KB (~9.5 GB), exit 0.
- **TypeScript Go**: parse 0.086 s, peak RSS 143,732 KB (~144 MB), exit 0.

The control run (plain `// note` comments, not directives) shows both compilers behaving similarly and within normal bounds:

- **tsrs**: parse 0.033 s, peak RSS 81,928 KB (~82 MB), exit 0.
- **TypeScript Go**: parse 0.077 s, peak RSS 136,696 KB (~137 MB), exit 0.

Because a single-file input with many `// @ts-ignore` directives produces a large, deterministic parse-time and memory divergence between tsrs and the pinned Go compiler, and because the difference disappears when the directives are replaced with ordinary comments, this is a Go parity defect rather than expected behavior or a host/runtime quirk.

**Verdict: bug**, **confidence: high**.

## Fix

### Changed behavior

The scanner no longer copies the entire `// @ts-ignore`/`// @ts-expect-error` directive list into a new arena slice for every directive. Instead it appends to a single growable `Vec<CommentDirective>` and stores only the current length in the `Copy` `ScannerState` snapshot. On `rewind`, the vector is truncated back to the saved length. The parser copies the final list into the arena once via `alloc_slice` when finishing the source file.

### Go function followed

`Scanner.processCommentDirective` in `tsc/internal/scanner/scanner.go` (commit `b85298b6a81f`), which simply appends a new directive to `s.commentDirectives`. The Rust port now mirrors that amortized O(1) append while preserving Go's snapshot/rewind semantics with a length-stamped vector.

### Files changed

- `crates/tsrs_scanner/src/scanner.rs`: replaced the per-directive slice copy with a single `Vec`; `ScannerState` now stores `comment_directives_len`; `rewind` truncates; `comment_directives()` returns the prefix.
- `crates/tsrs_parser/src/parser_1.rs`: allocate the final directive list into the arena once with `alloc_slice`.
- `crates/tsrs_scanner/src/scanner_test.rs`: added `test_comment_directives_scale_linearly` with 75,000 directives, which would OOM under the old quadratic copy.
- `testdata/regressions/issue-663-ts-ignore-directives/`: new CLI parity case (`a.ts`, `tsconfig.json`, `expected.txt`) for directive suppression behavior.

### Regression evidence

Release tsrs on the original `repro.ts` (40,000 `// @ts-ignore` directives):

| Metric | Before fix | After fix | Go reference |
|---|---|---|---|
| Parse time | 3.253 s | 0.035 s | 0.086 s |
| Peak RSS | 9,480,872 KB | 85,032 KB | 143,732 KB |

Post-fix scaling is linear:

| Directives | Parse time | Reported memory |
|---|---|---|
| 1,000 | 0.019 s | 66 MB |
| 2,000 | 0.031 s | 64 MB |
| 4,000 | 0.034 s | 65 MB |
| 8,000 | 0.037 s | 66 MB |
| 16,000 | 0.044 s | 73 MB |
| 32,000 | 0.057 s | 82 MB |
| 40,000 | 0.064 s | 86 MB |

### Commands and checks

```sh
# Build (dev loop)
cargo build --profile dev -p tsrs_cli -p tsrs_testrunner

# Affected crates
cargo test -p tsrs_scanner --locked
cargo test -p tsrs_parser --locked

# Regression suite (dev and release binaries)
tools/regressions.sh target/debug/tsrs
tools/regressions.sh target/release/tsrs

# Lint
cargo clippy --all-targets --config 'build.warnings="deny"'
tools/lint/source.py
cargo fmt -- --check
```

All of the above passed. A full `cargo test --locked` run uncovered only unrelated missing-baseline failures in `crates/tsrs_ls` because the local `ts-ref` checkout was incomplete for the language-server fixture paths; those tests need the upstream LSP baselines and are not affected by this scanner change.

### Final diff summary

- `crates/tsrs_scanner/src/scanner.rs`: quadratic per-directive slice copy removed; snapshot stores directive count; single vector backing the directives.
- `crates/tsrs_parser/src/parser_1.rs`: final directive slice allocated once with `alloc_slice`.
- `crates/tsrs_scanner/src/scanner_test.rs`: regression test for 75,000 directives.
- `testdata/regressions/issue-663-ts-ignore-directives/`: CLI parity regression.

**Fixed: true**
