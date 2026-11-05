# One lint pass instead of lint plus a separate typecheck (2026-11-04)

Update: the behavior below is now the fast preset's default for semantic reporting, without
`TSRS_LINT_CHECK_PROGRAM`, and it also reports each program's global diagnostics (`tsrs --noEmit` prints them without
a location; the linter reports them on the tsconfig). Listed files outside every tsconfig are type-checked, as in
default mode, and a file that several programs have is reported once: a listed file by the program it maps to, any
other file by the first program that has it. The measurements below were taken with the variable.

A project that runs type-aware lint and `tsrs --noEmit` builds the same program twice and resolves most of the same
types twice: lint asks the checker about nearly every expression. With `report_semantic` (oxlint `--type-check`) the
lint pass already reports compiler diagnostics on the checker that lints each file
([shared checker](perf-tsgolint-shared-checker.md)), and that costs about as much as a typecheck alone, but only for the
files oxlint sends: generated files, declaration files and anything else oxlint ignores are not checked, while files
outside every tsconfig are checked under the default program's options. So it could not replace a typecheck.

With the fast preset, `TSRS_LINT_CHECK_PROGRAM=1` makes requested semantic diagnostics follow `tsrs --noEmit`: each
tsconfig program also checks the files the request did not list (`skip_type_checking` decides, as in the CLI), and
files outside every tsconfig are linted but not type-checked. Without the preset or the variable nothing changes.

## Parity

Compiler diagnostics by (file, code) against `tsrs -p <config> --noEmit` on the same config: webpack 840 = 840, vscode
371 = 371, t3code-server 6 = 6, the large private monorepo 0 = 0 (and the frontend's whole output with
`--type-check` identical to its output without it). Without the variable, webpack's request-only reporting misses 26
of the 840.

## Measurements

AWS m8g.4xlarge (Graviton4, 16 vCPU, 64 GiB), Ubuntu 24.04, `TSRS_FAST=1`. Release builds of main `48cf809e` and this
change (fat LTO, no PGO). Both arms on one box, alternating with the order flipped every pair; changes are medians of
per-pair ratios with ~95% intervals. A runs `tsrs -p <config> --noEmit --incremental false` and then the lint
request; B runs the same request with `report_semantic`/`report_syntactic` and the variable. Catalog requests have
every rule with default options (`bench/lint.mjs`), 10 pairs; the private monorepo uses its own oxlint request,
15 pairs.

| Project | Wall | Instructions | Peak RSS |
| --- | ---: | ---: | ---: |
| large private monorepo | 18.09 -> 11.49 s, −36.5% [−37.1, −36.0] | −49.1% | +0.6% |
| same, typecheck and lint concurrently | 17.47 -> 11.54 s, −34.4% [−35.3, −31.9] | −48.6% | +2.8% |
| vscode | 4.74 -> 3.74 s, −21.5% [−22.7, −19.0] | −27.4% | +2.6% |
| t3code-server | 6.56 -> 5.03 s, −23.9% [−24.5, −22.6] | −41.1% | +5.6% |
| webpack | 0.64 -> 0.48 s, −24.5% [−25.8, −23.9] | −31.2% | +16.1% |
| mikro-orm | 5.55 -> 9.75 s, +74.6% [+71.5, +77.5] | +38.6% | +16.6% |

Peak RSS compares one process with the larger of two sequential ones.

mikro-orm is the case this does not fit: its benchmark typechecks one `tests` config, while lint maps each file to its
nearest tsconfig, a dozen per-package programs whose imports overlap, and checking each of them whole repeats that
work (and reports a different set of diagnostics). The variable pays where lint and typecheck use the same tsconfigs.

Through an oxlint frontend on the private monorepo (12 pairs), `oxlint --type-aware --type-check` with the variable
against `oxlint --type-aware` and `tsrs --noEmit` run concurrently: wall −12.3% [−12.9, −11.5] (the frontend's own
rules then set most of the wall), CPU −29.0%, instructions −25.1%, peak RSS +1.8%.
