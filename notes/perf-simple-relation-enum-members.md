# isSimpleTypeRelatedTo: enum members of one kind answered inline

Measured 2026-10-22 at `467182f2` on an 18-core Apple Silicon machine, following the owner's performance guide
(profile first, attribute, smallest change, before/after numbers).

## Finding

A CPU-time profile (`samply`, single-threaded, `RAYON_NUM_THREADS=1 --singleThreaded`) of the pinned
Compiler-Unions project put 4.7% of self time in `is_simple_type_related_to` (relater_1.rs), spread over its
chain of flag tests down to the final `false`: the calls walk the whole chain. A histogram of the flag pairs at
its entry over one run (3.1M calls dumped, about 4M in all) attributed the calls:

| calls | relation | source | target |
| ---: | --- | --- | --- |
| 2,334,822 | comparable | `NumberLiteral \| EnumLiteral` | `NumberLiteral \| EnumLiteral` |
| 187,900 | comparable | `Object` | `Object` |
| 116,946 | comparable | `Union` | `NumberLiteral \| EnumLiteral` |
| 116,946 | comparable | `NumberLiteral \| EnumLiteral` | `Union` |
| 114,631 | assignable | `Object` | `Object` |

Three quarters of the calls are one enum member against another under the comparable relation: narrowing
`kind === SyntaxKind.X` asks it of every member of the enum, in both directions (`is_type_related_to`). The
call-site shortcut for two different literals of one kind excludes enum members on purpose, since two members of
related enums with equal values are related (`isEnumTypeRelatedTo`), so every such pair ran the full chain.

## Change

`is_simple_type_related_to` is now an `#[inline]` entry whose first test is that case: when both flag words are
exactly `NumberLiteral | EnumLiteral` or exactly `StringLiteral | EnumLiteral`, the answer is
`value_equals && is_enum_type_related_to`, because every other test of the chain needs a flag neither type has (a
union, a base primitive, `any`, `unknown`, `never`, `undefined`, `null`, an object, a computed enum value). The
structured-types test that was the function's first line moved into the same entry; the rest of the chain is the
out-of-line worker, unchanged.

Two earlier shapes were measured and dropped: a flag-set prefilter for "no test can answer true" and shortcuts for
an enum member against an enum union. Each added a few inline tests that every one of the ~4M calls paid and that
almost no call took: -0.13% and then +1.30% instructions on Compiler-Unions. Inline code on this entry has to be
one comparison for the common pair, nothing more.

## Measurement

Clean fat-LTO `--release` builds with debug info, `/usr/bin/time -l` instructions retired (kernel included; the
macOS screening instrument of notes/perf-icount-round4.md), `--noEmit --incremental false --pretty false
--singleThreaded`, `RAYON_NUM_THREADS=1`, one warm-up then five interleaved base/candidate pairs, medians.
Diagnostics (exit code and output bytes) identical in every run.

| project | instructions base | candidate | change | peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Compiler-Unions | 3.1707 G | 2.9335 G | **-7.48%** | 59.6 -> 59.9 MiB |
| Compiler | 1.7162 G | 1.6902 G | **-1.51%** | 57.0 -> 57.1 MiB |
| mui-docs | 38.10 G | 37.80 G | -0.80% (inside this project's run-to-run band of about 1 G) | 747 -> 747 MiB |

The Linux `bench/count.mjs` gate on the pull request is the landing number; these are the screening numbers.
