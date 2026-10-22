# Bulk zero-context JSON positions (2026-10-15)

Follow-up to the [bottleneck investigation](perf-tsgolint-bottlenecks.md). The fix is in Oxc,
on `perf/json-zero-context` in `/private/tmp/oxc-json-zero-context`, based on `9a424c209e`.
The [production patch](../bench/profiles/oxc-json-zero-context.patch) is archived here; tsrs production code is unchanged.

JSON creates a zero-context span scanner for each diagnostic. The initial prefix scan previously walked every
logical newline and updated an empty context queue. The new LF-only path counts newlines in bulk and finds the
last newline with `memrchr`. It retains the original CR/CRLF path and leaves contextual rendering unchanged.
This extends the existing one-context-line optimization to the independently measured zero-context hotspot;
it adds no cache, shared state or threads.

## Release results

Public VSCode `b029fb142d7a1c7a656d8be926525d4e44c6d635`, all 60 rules, default thread count, Apple M2 Max.
Both frontends use release opt-level 3, fat LTO and one codegen unit; the Rust helper is unchanged.
The baseline strips symbols; the patched build retains them. Neither uses PGO/BOLT.
Two consecutive publishes, each with one warm-up and three rotated measured runs per case. Cells are medians.
Cached mode replays all diagnostics without checking; real mode runs the complete frontend/helper command.

| Publish | Variant | Helper | Before | After | Wall reduction | Frontend instructions before → after | Peak RSS before → after |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Lint | Cached | 4.791 s | 1.740 s | 63.7% | 59.57 → 34.77 G | 692.5 → 695.6 MiB |
| 1 | Lint | Real | 8.939 s | 5.931 s | 33.6% | 59.76 → 35.27 G | 2803.8 → 2810.6 MiB |
| 1 | Full checking | Cached | 4.883 s | 1.793 s | 63.3% | 59.97 → 35.19 G | 970.0 → 980.9 MiB |
| 1 | Full checking | Real | 10.306 s | 7.298 s | 29.2% | 61.09 → 35.96 G | 3948.0 → 3939.0 MiB |
| 2 | Lint | Cached | 4.811 s | 1.829 s | 62.0% | 59.00 → 34.84 G | 695.6 → 697.2 MiB |
| 2 | Lint | Real | 8.951 s | 5.927 s | 33.8% | 59.72 → 34.97 G | 2802.3 → 2809.8 MiB |
| 2 | Full checking | Cached | 4.890 s | 1.857 s | 62.0% | 59.65 → 35.21 G | 975.7 → 980.8 MiB |
| 2 | Full checking | Real | 10.243 s | 7.249 s | 29.2% | 60.09 → 35.46 G | 3940.1 → 3944.2 MiB |

All 64 runs preserve complete diagnostic multisets within each variant, including warm-ups, both builds
and cached/real helper modes. Peak memory is broadly unchanged; no memory gain is claimed.
Instructions are frontend-only process counters, excluding the helper and proxy. Peak RSS is a process
high-water statistic, not the simultaneous aggregate of the process tree; real-mode peaks may come from the helper.
The cached rows isolate the frontend. These wall times include command execution and Python wait bookkeeping.

The earlier investigation used instrumented binaries and a different session. Compare the paired before/after
numbers above; do not subtract these medians from its phase observations or call replay command time JSON render time.
No new rendering-only timer was added to these production binaries.

## Validation and reproduction

- `just test -p oxc_diagnostics --locked --offline`: 30 passed, including exhaustive scanner/span-reader tests
  and a new zero-context prefix comparison against the generic path at every byte cut for LF, mixed CR/CRLF,
  empty, unbroken and multibyte sources.
- `just lint -p oxc_diagnostics --locked --offline` and `cargo fmt --all --check`: passed.
- `just test -p oxlint --locked --offline`: 369 passed, 1 ignored. The fresh worktree initially lacked
  `node_modules`; after linking the existing installation and restoring an aborted-cleanup fixture, the unfiltered
  suite passed. The temporary link was removed.
- Every timed output was parsed and compared as a complete diagnostic multiset; ordering is immaterial.

[Raw counters, all samples, binary/config/patch identities and the exact executed scripts](../bench/results/2026-10-15-oxc-json-zero-context.json).
Build both with `cargo build --release --locked -p oxlint`; the patched measurement additionally sets `CARGO_PROFILE_RELEASE_STRIP=none`.
The archived runner and proxy use local paths for the public corpus, config, cached frames and binaries;
adjust these when reproducing elsewhere. Local raw files are under `/private/tmp/oxc-json-zero-context-bench`.
Cached frames are the same full lint/full-checking frames recorded in the preceding investigation.
No builds or tests overlap the measured runs. No speedup is claimed for CR/CRLF prefixes or other corpora.
