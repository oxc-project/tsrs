# Triage report for #964

Issue: bug: tsrs panics when file argument is invalid utf8

## Summary

`tsrs` panics at `std/src/env.rs:878:51` when a command-line argument contains a raw non-UTF-8 byte (`0xFF`). The pinned Go compiler accepts the same argument, reports `error TS6231` with the path replaced by the Unicode replacement character, and exits with status 2.

## Revisions

- tsrs: branch `triagebot/fix-964`, commit `13a31a4cac36b980db090a91ec12a3c3686de14d`
- TypeScript Go reference: commit `3e9236361fa97a9e6c685f4833ac457ae2d8c50a` (pin from `Cargo.toml`)

## Build commands

```bash
cargo build --profile dev --locked -p tsrs_cli -p tsrs_testrunner
# tsrs binary: target/debug/tsrs

# Go reference compiler
GOTOOLCHAIN=auto GOWORK=off go build -o bin/tsc ./cmd/tsc
# from .agents/triage/964/ts-ref/tsc/
```

## Minimal reproduction

Because the runner's shell did not expand `$'\xff'` to a raw byte, Python was used to launch the program with a literal `0xFF` argument:

```python
import subprocess
subprocess.run(["target/debug/tsrs", b"\xff"])
```

## Observed output

### tsrs

```text
thread 'main' (12178) panicked at /rustc/58cf8f9c35b355199727627d07ae34a4e2846d67/library/std/src/env.rs:878:51:
called `Result::unwrap()` on an `Err` value: "\xFF"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

Exit status: `101`

### Pinned Go compiler (`tsc`)

```text
error TS6231: Could not resolve the path '�' with the extensions: '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.
  The file is in the program because:
    Root file specified for compilation
```

Exit status: `2`

## Conclusion

REPRODUCED

The difference is a Rust-side failure to handle invalid-UTF-8 `argv` entries, whereas Go treats them as opaque byte strings and only fails later as a missing file. tsrs CLI parses should use `std::env::args_os()` (or a non-panicking equivalent) instead of `std::env::args()` for parity with the reference compiler.

## Diagnosis

### First mismatching functions

- Go: `osutil.Args` (`tsc/internal/osutil/os_other.go`) returns `os.Args` directly. Go strings are byte sequences, so a non-UTF-8 argument is preserved without error.
- Rust: `crates/tsrs_cli/src/main.rs:78` collects arguments with `std::env::args().skip(1).collect::<Vec<String>>()`. `std::env::args()` internally converts each `OsStr` to `String` and panics on invalid UTF-8 before the CLI logic begins.

### Precise difference

`std::env::args()` calls `arg.to_str().unwrap()` for every OS argument (`library/std/src/env.rs:878`). When an argument contains byte `0xFF`, the `None` result is unwrapped, producing the observed panic. The Go compiler never performs this conversion; the argument reaches `execute.CommandLine` and then `tsoptions.ParseCommandLine` as raw bytes, and the missing-file diagnostic (TS6231) is reported with the replacement character `�` on output.

### Evidence

A temporary instrumentation changed `crates/tsrs_cli/src/main.rs:78` to:

```rust
let args: Vec<String> =
    std::env::args_os().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect();
```

After rebuilding `tsrs_cli`, the same reproduction (`[binary, b"\xff"]`) produced:

```text
error TS6231: Could not resolve the path '�' with the extensions: '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.
  The file is in the program because:
    Root file specified for compilation
```

Exit status: `2`

This matches the Go compiler's output and exit code. The instrumentation was reverted immediately after the test.

### Fix approach

Use `std::env::args_os()` in `crates/tsrs_cli/src/main.rs` and convert each `OsString` lossily via `to_string_lossy()`. This mirrors Go's behavior for display and path resolution while avoiding the panic. The change is confined to the `tsrs_cli` binary; `execute::command_line` already accepts `Vec<String>`, and for this error path the replacement character is the only observable difference.

### Remaining uncertainty

If a user passes a real file whose name is valid on the filesystem but not valid UTF-8, Go could resolve it, whereas `to_string_lossy()` would not. However, such a case would be a separate parity issue, because the current problem is only the panic on any invalid-UTF-8 argument.

**Confidence: high**

## Verification

The claim is that tsrs panics when given an invalid-UTF-8 command-line file argument. This has been confirmed against the pinned TypeScript Go compiler at `3e9236361fa9`.

Both compilers were launched from the same working directory with the same single argument, a raw `0xFF` byte, via:

```python
subprocess.run(["target/debug/tsrs", b"\xff"])
subprocess.run([".agents/triage/964/ts-ref/bin/tsc", b"\xff"])
```

### Comparison

| compiler | exit status | output |
| --- | --- | --- |
| tsrs (commit `13a31a4cac36`) | `101` | panic in `std/src/env.rs:878:51`: `called Result::unwrap() on an Err value: "\xFF"` |
| Go tsc (commit `3e9236361fa9`) | `2` | `error TS6231: Could not resolve the path '�' ...` |

The divergence is deterministic across multiple invalid-UTF-8 byte sequences (`0xFF`, `0x80`, `0xC0 0x80`, `0xED 0xA0 0x80`); the Go compiler consistently reports `TS6231` and exits `2`, whereas tsrs panics with exit `101`. For a valid-UTF-8 argument such as a string of ASCII control characters, both compilers behave identically.

### Verdict

**bug**: tsrs differs from the pinned Go compiler. It should accept the argument and report the missing-file diagnostic instead of panicking.

**Confidence: high**

## Fix

### Changed source

`crates/tsrs_cli/src/main.rs`: argument collection now uses `std::env::args_os()` and converts each `OsString` with `to_string_lossy()` instead of `std::env::args()`, which panics on invalid UTF-8.

### Regression test

Added `crates/tsrs_cli/tests/integration/invalid_utf8_argument.rs` and registered it in `crates/tsrs_cli/tests/integration/main.rs`. The test passes a raw `0xFF` byte as a file argument on Unix and asserts:

- exit status `2` (not a panic),
- stdout matches the pinned Go compiler's `error TS6231` message with the Unicode replacement character `\u{FFFD}`.

### Verification commands and results

```bash
cargo build --profile dev --locked -p tsrs_cli -p tsrs_testrunner
cargo test --profile dev --locked -p tsrs_cli --test integration invalid_utf8_argument
tools/regressions.sh target/debug/tsrs
cargo clippy --all-targets --config 'build.warnings="deny"'
node tools/lint/source.mjs
```

- `invalid_utf8_argument::invalid_utf8_file_argument_reports_missing_file` passes.
- `tools/regressions.sh target/debug/tsrs` passes all 51 existing regression cases.
- Clippy passes with warnings denied.
- `tools/lint/source.mjs` passes.
- A full `cargo test -p tsrs_cli --test integration` run passed after pointing `ts-ref` at the triage checkout; the only unrelated (`api_contentmapper`) failure was caused by the reference compiler not being at the default `ts-ref` path and passed once that path was satisfied.

### Diff summary

- `crates/tsrs_cli/src/main.rs`
  - Replaced `std::env::args().skip(1).collect()` with `std::env::args_os().skip(1).map(|arg| arg.to_string_lossy().into_owned()).collect()` and added a comment explaining the Go parity rationale.
- `crates/tsrs_cli/tests/integration/invalid_utf8_argument.rs` (new)
  - Unix-only integration regression for the invalid-UTF-8 file argument.
- `crates/tsrs_cli/tests/integration/main.rs`
  - Registered the new `invalid_utf8_argument` module.

**Fixed: true**
