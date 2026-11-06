// `--fast` must set mimalloc's `allow_thp` and `arena_reserve` before mimalloc starts (src/main.rs
// `fast_heap_huge_pages`), so the tests read the values from its start-up report; that also checks the hard-coded
// option numbers against the linked mimalloc.

use std::process::Command;

fn option(name: &str, args: &[&str], env: &[(&str, &str)]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .args(args)
        .env_remove("TSRS_FAST")
        .env_remove("MIMALLOC_ALLOW_THP")
        .env_remove("MIMALLOC_ARENA_RESERVE")
        .envs(env.iter().copied())
        .env("MIMALLOC_VERBOSE", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains(&format!("option '{name}'")));
    line.unwrap_or_else(|| panic!("{args:?}: no {name} line in\n{stderr}"))
        .rsplit(':')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

#[test]
fn fast_gives_the_heap_huge_pages() {
    let allow_thp = |args: &[&str], env: &[(&str, &str)]| option("allow_thp", args, env);
    assert_eq!(allow_thp(&["--version"], &[]), "0");
    assert_eq!(allow_thp(&["--version", "--fast"], &[]), "1");
    assert_eq!(allow_thp(&["--version", "-Fast"], &[]), "1");
    assert_eq!(allow_thp(&["--fast", "--version", "--fast", "false"], &[]), "0");
    assert_eq!(allow_thp(&["--version"], &[("TSRS_FAST", "1")]), "1");
    assert_eq!(allow_thp(&["--version", "--fast", "false"], &[("TSRS_FAST", "1")]), "0");
    assert_eq!(allow_thp(&["--version", "--fast"], &[("MIMALLOC_ALLOW_THP", "0")]), "0");
}

#[test]
fn fast_reserves_the_heap_in_small_arenas() {
    let reserve = |args: &[&str], env: &[(&str, &str)]| option("arena_reserve", args, env);
    assert_eq!(reserve(&["--version"], &[]), "1048576 KiB");
    assert_eq!(reserve(&["--version", "--fast"], &[]), "65536 KiB");
    assert_eq!(
        reserve(&["--version", "--fast"], &[("MIMALLOC_ARENA_RESERVE", "256MiB")]),
        "262144 KiB"
    );
}
