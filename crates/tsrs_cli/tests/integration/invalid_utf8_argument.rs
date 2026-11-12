// A root file argument that isn't UTF-8 (#964): tsgo reports it missing (TS6231, exit status 2) and prints each run
// of invalid bytes as one U+FFFD (`strings.ToValidUTF8`), byte for byte what this expects.

#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

#[test]
fn invalid_utf8_file_argument_reports_missing_file() {
    for (arg, shown) in [
        (&b"\xff"[..], "\u{FFFD}"),
        (b"\xc0\x80", "\u{FFFD}"),
        (b"\xed\xa0\x80", "\u{FFFD}"),
        (b"a\xfe\xffb", "a\u{FFFD}b"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
            .arg(OsStr::from_bytes(arg))
            .args(["--pretty", "false"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{arg:x?} stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let expected = format!(
            "error TS6231: Could not resolve the path '{shown}' with the extensions: '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.\n  The file is in the program because:\n    Root file specified for compilation\n"
        );
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{arg:x?}");
    }
}
