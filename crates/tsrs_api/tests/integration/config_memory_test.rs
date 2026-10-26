// The config RSS test runs alone in its own process. Its result changed with the order of the other memory tests,
// even when their bodies were serialized, because RSS includes the whole process.
#![cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::common::rss::rss_kib;
use crate::common::*;

/// Config requests build their parsed command lines in a scratch region that is freed after the response is
/// serialized.
#[test]
fn repeated_config_parsing_reclaims_memory() {
    if !crate::isolated::in_child("config_memory_test::repeated_config_parsing_reclaims_memory") {
        return;
    }
    let dir = TempDir::new("cfgloop");
    let cfg = dir.write("tsconfig.json", &format!("{{ \"compilerOptions\": {{ \"strict\": true, \"paths\": {{ {} }} }}, \"include\": [\"src\"] }}", (0..200).map(|i| format!("\"p{i}/*\": [\"src/p{i}/*\"]")).collect::<Vec<_>>().join(",")));
    for i in 0..50 {
        dir.write(&format!("src/f{i}.ts"), "export {};\n");
    }
    let s = session(&dir.dir(), false);
    let n: usize =
        std::env::var("TSRS_API_STRESS").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let req = format!("{{\"file\":{}}}", quote(&cfg));
    let first = call(&s, "parseConfigFile", &req);
    // Warm both request paths before measuring their steady-state memory growth.
    for _ in 0..10 {
        assert_eq!(call(&s, "parseConfigFile", &req), first);
        call(&s, "readConfigFile", &req);
    }
    let before = rss_kib();
    for _ in 0..n {
        assert_eq!(call(&s, "parseConfigFile", &req), first);
        call(&s, "readConfigFile", &req);
    }
    let grown = rss_kib().saturating_sub(before);
    eprintln!("parseConfigFile+readConfigFile x{n}: rss grew {grown} KiB");
    assert!(grown < (n as u64) * 8, "rss grew {grown} KiB over {n} iterations");
}
