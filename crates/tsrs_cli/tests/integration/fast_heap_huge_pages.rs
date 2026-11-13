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

// The size gate at parse end (src/main.rs `fast_heap_at_parse_end`) reports its decision under `TSRS_MEM_SPLIT`. A
// program of a few files is far below the threshold.
fn gate(args: &[&str], env: &[(&str, &str)]) -> Option<String> {
    let case = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/regressions/leaf-alternative-containers");
    let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
        .current_dir(case)
        .args(["-p", ".", "--noEmit", "--pretty", "false", "--checkers", "2"])
        .args(args)
        .env_remove("TSRS_FAST")
        .env_remove("TSRS_FAST_HEAP_HUGE_PAGES")
        .env_remove("MIMALLOC_ALLOW_THP")
        .envs(env.iter().copied())
        .env("TSRS_MEM_SPLIT", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find_map(|l| l.strip_prefix("tsrs mem split: heap huge pages "))?;
    Some(line.split(" (").next().unwrap().to_string())
}

#[test]
fn small_programs_keep_the_heap_off_huge_pages() {
    assert_eq!(gate(&["--fast"], &[]).as_deref(), Some("off"));
    assert_eq!(
        gate(&["--fast"], &[("TSRS_FAST_HEAP_HUGE_PAGES", "1")]).as_deref(),
        Some("on, forced")
    );
    assert_eq!(
        gate(&["--fast"], &[("TSRS_FAST_HEAP_HUGE_PAGES", "0")]).as_deref(),
        Some("off, forced")
    );
    assert_eq!(gate(&[], &[("TSRS_FAST_HEAP_HUGE_PAGES", "1")]), None);
    assert_eq!(gate(&["--fast"], &[("MIMALLOC_ALLOW_THP", "1")]), None);
}
