// `--init` is not ported (#938): the error goes to stderr with exit status 5, so a script that captures stdout gets
// nothing instead of the error text.

use std::process::Command;

#[test]
fn unsupported_commands_report_on_stderr() {
    let dir = std::env::temp_dir().join(format!("tsrs-unsupported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
    let out =
        Command::new(env!("CARGO_BIN_EXE_tsrs")).arg("--init").current_dir(&dir).output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "error: --init is not supported by tsrs.\n");
    assert!(!dir.join("tsconfig.json").exists(), "--init wrote a tsconfig.json");
    let _ = std::fs::remove_dir_all(&dir);
}
