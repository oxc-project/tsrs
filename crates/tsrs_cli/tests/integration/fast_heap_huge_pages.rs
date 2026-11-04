// `--fast` turns on transparent huge pages for mimalloc's heap (src/main.rs `fast_heap_huge_pages`). The option must
// reach mimalloc before it starts, so its start-up report (`MIMALLOC_VERBOSE=1`) shows the value it runs with; that
// also checks the hard-coded option number against the linked mimalloc.

use std::process::Command;

fn allow_thp(args: &[&str], env: &[(&str, &str)]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .env_remove("TSRS_FAST")
        .env_remove("MIMALLOC_ALLOW_THP")
        .envs(env.iter().copied())
        .env("MIMALLOC_VERBOSE", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("option 'allow_thp'"));
    line.unwrap_or_else(|| panic!("{args:?}: no allow_thp line in\n{stderr}"))
        .rsplit(':')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn fast_gives_the_heap_huge_pages() {
    assert_eq!(allow_thp(&["--version"], &[]), "0");
    assert_eq!(allow_thp(&["--version", "--fast"], &[]), "1");
    assert_eq!(allow_thp(&["--version", "--Fast", "true"], &[]), "1");
    assert_eq!(allow_thp(&["--fast", "--version", "--fast", "false"], &[]), "0");
    assert_eq!(allow_thp(&["--version"], &[("TSRS_FAST", "1")]), "1");
    assert_eq!(allow_thp(&["--version", "--fast", "false"], &[("TSRS_FAST", "1")]), "0");
    assert_eq!(allow_thp(&["--version"], &[("TSRS_FAST", "0")]), "0");
    // An explicit allocator setting wins.
    assert_eq!(allow_thp(&["--version", "--fast"], &[("MIMALLOC_ALLOW_THP", "0")]), "0");
}
