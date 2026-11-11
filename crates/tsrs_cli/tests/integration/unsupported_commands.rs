// `--init` and `--showConfig` are not ported (#938): the error goes to stderr with exit status 5, so a script that
// captures stdout gets nothing instead of the error text.

use std::process::Command;

#[test]
fn unsupported_commands_report_on_stderr() {
    let dir = std::env::temp_dir().join(format!("tsrs-unsupported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
    for (flag, config) in [("--init", None), ("--showConfig", Some(r#"{ "files": ["a.ts"] }"#))] {
        if let Some(config) = config {
            std::fs::write(dir.join("tsconfig.json"), config).unwrap();
        }
        let out =
            Command::new(env!("CARGO_BIN_EXE_tsrs")).arg(flag).current_dir(&dir).output().unwrap();
        assert_eq!(out.status.code(), Some(5), "{flag}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "", "{flag}");
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("error: {flag} is not supported by tsrs.\n")
        );
        if config.is_none() {
            assert!(!dir.join("tsconfig.json").exists(), "--init wrote a tsconfig.json");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
