// Pinned Go tsc accepts an invalid-UTF-8 command-line file argument and reports a missing-file
// diagnostic (TS6231). tsrs used to panic because `std::env::args()` unwraps each `OsStr` to a `String`.

#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

#[test]
fn invalid_utf8_file_argument_reports_missing_file() {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .arg(OsStr::from_bytes(b"\xff"))
        .args(["--pretty", "false"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let expected = "error TS6231: Could not resolve the path '\u{FFFD}' with the extensions: '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n  The file is in the program because:\n    Root file specified for compilation\n";
    assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
}
