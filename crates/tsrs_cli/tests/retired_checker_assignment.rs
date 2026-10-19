use std::process::Command;

#[test]
fn go_assignment_is_rejected_before_compilation() {
    let output = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(["--checkerAssignment", "go", "--version"])
        .env_remove("TSRS_CHECKER_ASSIGNMENT")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("unknown --checkerAssignment \"go\""), "{text}");
    assert!(text.contains("expected locality, random:<seed> or file:<path>"), "{text}");
}
