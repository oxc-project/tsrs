# Triage report for #1042

## Claim

`@tsrs/wasm`’s in-memory `memoryFileSystem` returns `mtimeNs: 0n` for every file.
When a composite project is built in memory, the first build writes outputs
(including `dist/tsconfig.tsbuildinfo`). On the second in-memory build the
edited `a.ts` and the `tsbuildinfo` have the same timestamp, so the builder
decides the project is up to date and exits 0 without re-checking.

## Reproduction

Installed `@tsrs/wasm@7.1.0-beta.tsrs3` in `/tmp/tsrs-wasm-repro` and ran the
issue’s `repro.mjs`.

Observed:

- Build 1 exits `0` and emits `dist/a.js`, `dist/a.d.ts`, `dist/tsconfig.tsbuildinfo`.
- Build 2 (with `a.ts` edited to `"wrong"`) exits `0`, emits nothing, and prints
  no diagnostics.
- Build 2 with `--force` exits `2` and reports `TS2322`.

With `--verbose` the second build reports:

```text
Project 'tsconfig.json' is up to date because newest input 'a.ts' is older than output 'dist/tsconfig.tsbuildinfo'
```

The same project on disk is correctly detected as out of date by both the
pinned Go compiler (`target/reference/tsgo`) and native `target/debug/tsrs`:
after editing `a.ts`, the second `tsc -b . --pretty false` reports `TS2322` and
exits `2`.

## Diagnosis

The divergence is not in the Rust port but in the JavaScript host file-system
shim.

First mismatching layer:

- `npm/tsrs-wasm/core.js` `memoryFileSystem.stat()` always returns
  `mtimeNs: 0n` and `chtimes()` is a no-op.
- The real file-system shim in `npm/tsrs-wasm/node-fs.js` uses real `mtimeNs`
  and real `utimesSync`, which is why the on-disk case works.

Precise semantic difference:

`tsrs_execute::build::buildtask` decides whether a composite project is up to
date by comparing the newest input mtime against the oldest output mtime
(initially the `.tsbuildinfo` file):

```rust
if after(input_time, oldest_output_file_and_time.time) {
    return UpToDateStatus::with(
        UpToDateStatusType::InputFileNewer,
        ...
    );
}
```

Because every in-memory entry reports an mtime of `0`, the edited `a.ts` is
never newer than the `tsbuildinfo`, so the status stays `UpToDate`.

Minimal fix approach:

Make `memoryFileSystem` return distinct, monotonically increasing mtimes so that
files stat’d/written later are seen as newer than earlier ones. A minimal change
is to return `nowNs(false)` in `stat()` instead of `0n`. This keeps the host
layer only and requires no Rust changes. A more robust fix would keep stable
per-entry mtimes that advance on content changes (`put`/`writeFile`/`chtimes`),
but even the one-line change is enough for the reported scenario because the
tsbuildinfo is stat’d before the input files in the up-to-date check, so the
freshly edited input receives a larger timestamp.

Verified with a local edit of `node_modules/@tsrs/wasm/core.js`:
replacing `mtimeNs: 0n` with `mtimeNs: nowNs(false)` makes the second in-memory
build exit `2` and report `TS2322`.

## Evidence and confidence

- The repro script matches the issue exactly and shows the failure.
- Reference compiler and native tsrs behave correctly on disk.
- Patching only the JS host mtime is sufficient to resolve the issue.

Confidence: **high**.

## Verification

Verdict: **bug**.

The pinned Go compiler (`ts-ref/tsc` built as `target/reference/tsgo`, commit
`3e9236361fa97a9e6c685f4833ac457ae2d8c50a`) deterministically reports the edit
as a type error on the second `tsc -b . --pretty false` run on disk. Native
`tsrs` (`target/debug/tsrs`) matches it. Only the in-memory `@tsrs/wasm` path
skips the edit, because `memoryFileSystem.stat()` discards mtimes.

Comparison evidence:

| Step | Pinned Go compiler on disk | `@tsrs/wasm` in memory |
|------|----------------------------|------------------------|
| First build | exit 0, emits outputs | exit 0, emits outputs |
| Second build after editing `a.ts` | exit 2, reports `TS2322` | exit 0, no diagnostics |
| Second build with `--force` | exit 2, reports `TS2322` | exit 2, reports `TS2322` |

A minimal patch to the JS host (`memoryFileSystem.stat()` returning
`nowNs(false)` instead of `0n`) makes the in-memory path report `TS2322` on the
second build without `--force`, matching the Go/N Age output.

Confidence: **high**.

## Fix

Changed `npm/tsrs-wasm/core.js` `memoryFileSystem` to maintain per-file
modifiable mtimes. A monotonic clock assigns an increasing timestamp on the
first `stat()` of a file and on every `writeFile()` or `chtimes()`; directories
still report `0n`. This makes the in-memory host behave like a real file system
for timestamp-based up-to-date checks.

Go source followed: none directly; the Rust native compiler and the Go compiler
already agree (they use real file-system mtimes). The fix is entirely in the
JavaScript host layer that the wasm module delegates to.

Regression test added to `npm/tsrs-wasm/test/tsc.test.mjs`:
"in-memory build detects an edited source file on the second run". It builds a
composite project, passes emitted files back, edits `a.ts`, and asserts the
second `tsc -b` exits `2` with a `TS2322` diagnostic.

Verification commands and results:

```sh
cargo clippy --all-targets --config 'build.warnings="deny"'
# finished with 0 warnings/errors

tools/lint/source.mjs
# source checks: ok

TSRS_WASM=/tmp/tsrs-wasm-repro/node_modules/@tsrs/wasm/tsrs.wasm \
TSRS_NATIVE=/home/runner/work/tsrs/tsrs/target/debug/tsrs \
node --test npm/tsrs-wasm/test/*.test.mjs
# 15 tests pass, including the new regression
```

Final diff summary:

```
npm/tsrs-wasm/core.js
npm/tsrs-wasm/test/tsc.test.mjs
```

Commit message:
`fix(tsrs-wasm): assign monotonic mtimes in memoryFileSystem for incremental builds`
