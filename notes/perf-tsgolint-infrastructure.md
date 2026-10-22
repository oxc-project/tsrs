# Lint project discovery and dispatch performance fix (2026-10-15)

Infrastructure-only results from `d501c471`; the [terminal-reference follow-up](perf-tsgolint-terminal-references.md)
records the final rule correction and additional improvement.

All 60 rules on the same 9,324-file VS Code corpus improve from **153.908 s to
18.956 s**, an **8.12× speedup**. Diagnostic output is byte-identical.
This fixes the major regression identified by the [historical benchmark](bench-tsgolint-vscode.md).

## Causes and changes

Project selection reparsed `tsconfig.json` and expanded its file globs for every input file.
It also scanned and normalized the configuration's complete root-file list for every lookup.
A symbolized release CPU sample during startup was dominated by `get_parsed_command_line_of_config_file`,
`get_file_names_from_config_specs`, and `GlobVisitor::visit`.

The lint request now reuses parsed discovery configurations by canonical config path and uses
`ParsedCommandLine::file_names_by_path()` for membership. Unrecoverable parses and malformed config
syntax are cached as failed discovery results. This state is dropped after discovery, before
program construction; execution still parses each selected program's own configuration.
Nothing persists between library calls. Reference traversal, composite filtering, ancestor search,
and configuration diagnostics retain their previous paths.

The next CPU sample showed substantial time checking all 60 rules for every AST event.
Dispatch now remembers matching rule indexes for each listener within a file, preserving configured
rule order. Rule instances and this index are rebuilt per file, and listener membership must stay
stable after initialization. Current state-dependent listeners depend on immutable rule options.

Initial isolated screening observations were 23.190 s after discovery reuse and 19.352 s after
indexed dispatch. The final paired measurements below use the final formatted source snapshot.

## Measurements

Same machine, source corpus, default rule configuration, captured payload, cwd, and flags as the
historical benchmark: Apple M2 Max, 12 logical CPUs, 96 GiB RAM, macOS 27.0.1; optimized release
binaries, no PGO/BOLT. The input payload and configuration are byte-identical to the original.
The original pre-fix Rust binary is preserved and matches its recorded hash.

Default measurements use one warm-up per helper, then three alternating Rust/Go pairs. No builds
or tests overlapped these final measurements. Values are independent medians; fresh compiler state
is built on every run with a warm filesystem cache. The before medians are the original three runs
at `ab2eb669`. Go was rerun as a same-session control.

| Metric | Rust before | Rust after | Go, fresh control |
| --- | ---: | ---: | ---: |
| Default helper wall | 153.908 s | 18.956 s | 10.907 s |
| Peak RSS | 2.263 GiB | 2.132 GiB | 7.841 GiB |
| Default instructions | 2800.061 G | 216.539 G | 479.765 G |
| User CPU | 217.06 s | 18.14 s | 54.68 s |
| System CPU | 159.42 s | 1.88 s | 3.30 s |

Default peak RSS falls by 5.8%.
The default after wall range is 18.955–19.216 s;
the Go control range is 10.623–10.915 s.

For the single-threaded instruction comparison, both runs use `GOMAXPROCS=1`,
`RAYON_NUM_THREADS=1`, and `headless -debug timings`. The saved original instrumented run and
one new instrumented run measure the same workload and identical callback counts for all 60 rules:

| Metric | Before | After |
| --- | ---: | ---: |
| Single-threaded instructions, instrumented | 2702.016 G | 269.253 G |
| Instrumented wall | 168.273 s | 22.358 s |

Single-threaded instructions fall by 90.0%.
Instrumentation overhead is present in both instruction samples and is excluded from the default
wall-time medians. These macOS counters are local screening evidence, not deterministic Linux
`bench/count.py` results or two publishes of the compiler wall-time table. The verified 5.8%
default RSS reduction meets the repository's 5% memory criterion. The much larger local
instruction and wall-time reductions corroborate the benefit of these request-scoped caches;
the deterministic instruction gate was not run.

One full Oxlint validation sample per helper takes 23.086 s with Rust and
15.406 s with Go. These are individual observations, not repeated medians.
Oxlint exits with status 1 for the corpus's intentional invalid JavaScript fixtures;
all direct helper samples exit successfully with no protocol errors.

The Rust default helper remains 1.74× slower than Go's default.
Rust's lint callbacks remain serial; Go uses its default multiple lint workers. This patch adds
no workers. The benchmark does not attribute all of the remaining difference to concurrency.

## Correctness and evidence

Every uninstrumented Rust helper output, including warm-up, hashes to
`75d496a15a200251315a06d5260f46e81a50b52bc9ab81d480f8ba420384d40b`: byte-for-byte identical to the original 275,890-diagnostic stream.
The instrumented runs have identical per-rule callback counts. The original cross-version Go
count differences remain; this infrastructure patch changes no rule semantics or snapshots.

- All 7,169 active rule baselines pass; 36 upstream skips remain.
- 98 headless, 2 lifecycle, 8 protocol, and 2 project-discovery unit tests pass.
- New tests cover one config read across 200 files, negative parse reuse, cyclic solution
  references, canonical case-insensitive membership, and changed configuration across requests.
- All Oxlint Rust integration suites and 642 JavaScript tests pass, with one upstream JS skip.
- Native compiler conformance remains 13,458 error, 12,779 type, and 12,779 symbol passes.
- Required workspace Clippy and source checks pass. The broader `--all-targets` Clippy probe
  also found pre-existing warnings in the unchanged headless tests; those are outside this fix.

[Raw before/after counters and source/binary hashes](../bench/results/2026-10-15-tsgolint-vscode-perf-fix.json) record all
samples. Reproduction uses the commands in the historical benchmark, substituting the new Rust
binary while preserving the same config and captured request. Full local run logs, outputs,
and runner remain in `/private/tmp/tsrs-vscode-lint-perf-fix/`; the symbolized CPU samples are
`/private/tmp/tsrs-lint-before-symbols.sample.txt` and `/private/tmp/tsrs-lint-cache-symbols.sample.txt`.
