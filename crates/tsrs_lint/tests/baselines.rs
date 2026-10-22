use std::path::Path;
use std::process::Command;

#[test]
fn all_imported_rule_baselines_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .arg(root.join("tools/tsgolint/run-baselines.py"))
        .arg(env!("CARGO_BIN_EXE_tsrs-lint"))
        .arg(root.join("testdata/lint"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
