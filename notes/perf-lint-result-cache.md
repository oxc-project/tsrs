# Lint result cache (2026-11-04)

`tsrs-lint headless` (and `tsrs --lint`) can keep each file's output frames between runs, for repeated
`oxlint --type-aware` runs over a mostly unchanged tree. It is opt-in: the fast preset (`TSRS_FAST=1`) and
`TSRS_LINT_CACHE=<directory>`. Without both, or with `source_overrides` in the payload, nothing is read or written
and the output is unchanged. `TSRS_LINT_CACHE_STATS=1` prints `tsrs-lint cache: <n> linted, <m> replayed` per
program on stderr. The code is `crates/tsrs_lint/src/cache.rs`; the tests are
`crates/tsrs_lint/tests/integration/cache.rs`.

## When a file is linted again

The cache adds no dependency tracking of its own. Each program's cache file holds its tsrs_incremental snapshot as a
tsbuildinfo, and a file's frames replay while that snapshot keeps the file's semantic diagnostics: after
`new_program(program, old)` and `Program::collect_affected_files`, the file still has an entry in
`semantic_diagnostics_per_file`. That is TypeScript's rule, so a file is linted again when

- its text, module format or global-scope status changes, or the files it references change (an import now
  resolves elsewhere, a referenced file is added or deleted);
- a file it references, directly or through other files, changes its declaration signature (the hash of its
  `.d.ts` output): a body-only edit of a dependency relints only that dependency, an exported-type change relints
  every file that reaches it through imports and re-exports;
- a file affecting the global scope changes or is deleted (every file), or a library changes;
- a compiler option that affects semantic diagnostics changes.

On top of that, every compiler option and the linter build (package version, executable size and modification time)
are fingerprinted, so any option change or rebuild is a cold run; the payload's `fix`, `fix_suggestions`,
`report_syntactic`, `report_semantic` and the suppress-program-diagnostics setting select the cache file, so runs
with and without `--fix` keep separate files; and each entry stores the hash of the file's rules and options, so a
changed rule list or option relints the files it applies to. A missing, truncated, corrupt (the body is
checksummed) or mismatched file means a cold run, never an error.

Linted files get an empty semantic-diagnostics entry rather than their diagnostics: the cache reads nothing back
from those entries, and the semantic diagnostics a payload asks for are already in the file's frames. Computing the
real diagnostics would check every relinted file a second time.

## Declaration signatures

A file's first snapshot stores its version as a stand-in for its declaration signature. Left alone, the first edit
of any file after a cold run would compare a real signature with that stand-in, find it changed and relint every
file that imports it, transitively. After linting, the cache therefore runs `compute_placeholder_signatures` (the
watcher's idle-time job): on a cold run that is a declaration emit of every non-declaration file; on a warm run only
of the files whose signature `handle_dts_may_change_of` reset to the stand-in, the importers relinted in that run.
It emits 256 files at a time, as the watcher does: one emit of every file of webpack (below) exhausted the 32 GiB
arena address range. Limiting the cold run to files that something references was measured and not kept (below).

## What a warm run still pays

Program construction and binding; the references of every file (checker lookups of imports, augmentations and
ambient modules) and the version hash of every file, as `tsrs --incremental` does; reading and converting the build
info; the declaration emit of the changed files and of each level of importers whose signature changed; linting the
invalidated files; their signatures; and, when anything changed, writing the build info and frames again (a run that
replays everything writes nothing).

## Known limits

- TypeScript's rule has gaps, and the cache follows it. Deleting a module that another module re-exports leaves the
  re-exporting file's declaration text unchanged, so the files importing the re-export keep their old result; the
  pinned tsgo keeps the stale semantic diagnostic the same way with `--incremental`.
- A few rules read syntax of other files that declaration output does not keep: `unbound-method` looks at whether a
  class property is initialized with a function expression, and `no-deprecated` reads `@deprecated` tags, which
  `removeComments` drops from declarations. Such an edit in a dependency does not relint its importers.
- A warm run lints a subset of the files, so history-dependent lint diagnostics (the checker's earlier work,
  [shared checker](perf-tsgolint-shared-checker.md)) can differ from a cold run, as with any per-file cache.
- Files outside every tsconfig, and programs whose configuration sets `incremental: false`, are not cached.
- The signatures cost a declaration emit, which TypeScript makes far more expensive than linting on some JavaScript
  programs. On webpack (public, 777 checked JavaScript files, all rules), linting retires 23 G instructions, while a
  declaration emit of every file retires 5.9 T in the pinned tsgo (48 GiB peak RSS) and 4.8 T here (28 GiB). A cold
  run with the cache pays that, and a warm run pays it for each changed file and each level of importers whose
  signature changed. The cache is a loss on such programs. Version stand-ins for JavaScript files, or a cold run once
  the signature work exceeds the lint work, would bound it; neither is implemented.

## Measurements

Preliminary, on a large private monorepo with the `ci` profile on a loaded laptop (instructions only; wall time
meaningless there): a cold run with the cache retires 22% more instructions than a run without it, of which the
declaration signatures are about nine tenths; skipping files no other file references reduced the signature work by
a third of its files but only 9% of its instructions. A warm run with no change retires 86% fewer instructions than
a run without the cache, with about a third of its peak RSS. Cold output decoded equal to the uncached run's, and the warm
run replayed every file with byte-identical output.

On a copy of webpack (all rules), every run with the cache printed exactly what a run without it printed on the same
tree: cold, warm, a body-only edit of `lib/util/comparators.js` (imported by 69 modules; 1 file relinted), an edit of
the declared return type of one of its exports (661 of 777 files relinted, its transitive importers), the revert, and
a warm run after it.

### Quiet machine

To be filled in: release builds with Hyperfine and hardware counters, instructions, peak RSS and wall time for a
run without the cache, a cold run, a warm run with no change, and warm runs after a body-only edit and after an
exported-type edit of a widely imported file.
