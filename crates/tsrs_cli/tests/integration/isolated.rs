// Tests that measure the process's RSS run alone in a fresh process of this test binary.

use std::process::Command;

const CHILD: &str = "TSRS_ISOLATED_TEST";

/// Returns whether the caller should run its body. In the child process it returns true; otherwise it runs `test`
/// (its full name, as `--exact` takes it) alone in a child process, asserts that it passed and returns false.
pub fn in_child(test: &str) -> bool {
    if std::env::var_os(CHILD).is_some_and(|name| name == test) {
        return true;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, test)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("1 passed;"), "the child must run the isolated case:\n{stdout}");
    false
}
