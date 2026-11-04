// Runs tests/node/contentmapper.test.mjs: the pinned async Node API client from ts-ref drives this crate's `tsrs` with
// `--api --runExternalCode` over testdata/contentmapper/types and its Node mapper (issue #877), and compares the
// diagnostics with tsgo's (expected.txt). Like tsrs_api_transport's node_roundtrip, it fails when `node` (>= 22.6,
// type stripping) or the pinned client (`<workspace>/ts-ref` or `$TSRS_TS_REF`) is missing.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn node_api_transforms_content_mapped_files() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let ts_ref =
        std::env::var_os("TSRS_TS_REF").map_or_else(|| workspace.join("ts-ref"), PathBuf::from);
    let node_ok = Command::new("node").arg("--version").output().is_ok_and(|o| o.status.success());
    assert!(node_ok, "missing dependency: `node` (>= 22.6) is not on PATH");
    assert!(
        ts_ref.join("packages/typescript/src/api/async/api.ts").exists(),
        "missing dependency: the pinned TypeScript client is not at {} (see CONTRIBUTING.md, reference compiler)",
        ts_ref.display()
    );
    let output = Command::new("node")
        .args([
            "--conditions=@typescript/source",
            "--test",
            "--test-timeout=60000",
            "--test-reporter=tap",
        ])
        .arg(manifest.join("tests/node/contentmapper.test.mjs"))
        .env("TS_REF", &ts_ref)
        .env("TSRS", env!("CARGO_BIN_EXE_tsrs"))
        .env("FIXTURES", workspace.join("testdata/contentmapper"))
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = |key: &str| {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(&format!("# {key} ")))
            .and_then(|v| v.trim().parse::<u32>().ok())
    };
    assert!(
        output.status.success() && count("fail") == Some(0) && count("pass") == Some(1),
        "node API content-mapper test failed\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
