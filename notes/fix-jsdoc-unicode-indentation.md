# JSDoc Unicode indentation: current-reference validation

This supersedes the initial investigation against `b85298b6a81f`. The fix was rebased onto `fc384313`, whose
`Cargo.toml` pins microsoft/TypeScript at `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`.

## Exact reference

Before the follow-up validation, both commands identified the same full commit:

```sh
node npm/build.mjs --print-typescript-commit
git -C ts-ref rev-parse HEAD
```

The Go JSDoc oracle was rebuilt from that checkout. For each source string in
`crates/tsrs_parser/tests/integration/jsdoc_indentation.rs`, it called `parser.ParseSourceFile` with the matching
JS or TS script kind, forced `Node.JSDoc(file)`, visited the `JSDocText` children, and serialized their text with
`encoding/json`. A Node.js check decoded the Rust source/expected string pairs and compared the oracle's JSON
output with each expected string. All six cases matched in both script kinds: twelve comparisons, with no
expectation changes needed.

| case | Go JSON comment text (Unicode escaped here for visibility) |
| --- | --- |
| Fenced example tag | `"\u0060\u0060\u0060javascript\nvar e = 1;\n\ufffd\nuse(e);\n\u0060\u0060\u0060"` |
| Top-level comment | `"example\n\ufffd"` |
| Trailing tag, one space before U+200B | `"\ufffd\u200b\u200bexample"` |
| Trailing tag, two spaces before U+00A0 | `"\ufffdexample"` |
| Trailing tag, two spaces before U+200B | `"\ufffd\ufffdexample"` |
| Trailing tag, two spaces before U+FEFF | `"\ufffd\ufffdexample"` |

`cargo test --locked -p tsrs_parser --test integration jsdoc_indentation` also passes against the same checkout.
This validates the rune-decoded comment text; it does not erase the documented difference where Go writes
incomplete UTF-8 bytes in synthesized declaration comments.

## Conformance at the current pin

The [conformance job for `4cba5b03`](https://github.com/oxc-project/tsrs/actions/runs/42374577788/job/128409590335)
explicitly checked out `3e9236361fa97a9e6c685f4833ac457ae2d8c50a`, then ran the conformance gate in Go and default
checker histories and the scheduling sweep. Both full-suite histories reported:

| baseline | pass | fail | crash | timeout |
| --- | ---: | ---: | ---: | ---: |
| Errors | 13,700 | 0 | 0 | 0 |
| Types | 12,998 | 0 | 0 | 0 |
| Symbols | 12,998 | 0 | 0 | 0 |
| JavaScript/declarations | 13,626 | 0 | 0 | 0 |
| JavaScript maps | 149 | 0 | 0 | 0 |
| Source maps | 156 | 0 | 0 | 0 |

These are the CI gate's reported results against the current pin, rather than a claim that the earlier pin's
13,457-pass local run establishes current-reference parity. The scheduling sweep and regression cases also passed.
