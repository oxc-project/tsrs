#[expect(dead_code, reason = "This target uses only part of the shared integration helper module.")]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use tsrs_api::{Session, SessionOptions};
use tsrs_core::json::{self, Value};
use tsrs_vfs::{bundled, osvfs, wrapvfs};

#[test]
fn failed_replacement_build_preserves_the_last_successful_build_for_cleaning() {
    let dir = TempDir::new("build-unwind");
    let config =
        |output: &str| format!(r#"{{"compilerOptions":{{"outDir":"{output}"}},"files":["a.ts"]}}"#);
    let cfg = dir.write("tsconfig.json", &config("out"));
    dir.write("a.ts", "export const value = 1;\n");

    let fail_read = Arc::new(AtomicBool::new(false));
    let fs_fail_read = Arc::clone(&fail_read);
    let fs = wrapvfs::wrap(
        osvfs::fs(),
        wrapvfs::Replacements {
            read_file: Some(Box::new(move |path| {
                if path.ends_with("/a.ts") {
                    assert!(
                        !fs_fail_read.swap(false, Ordering::SeqCst),
                        "injected build read failure"
                    );
                }
                osvfs::fs().read_file(path)
            })),
            ..Default::default()
        },
    );
    let session = Session::new(SessionOptions {
        cwd: dir.dir(),
        default_library_path: bundled::lib_path(),
        fs: Arc::new(bundled::wrap_fs(fs)),
        binary_responses: false,
        run_external_code: false,
    });
    let created =
        call(&session, "createBuildOrchestrator", &format!(r#"{{"rootNames":[{}]}}"#, quote(&cfg)));
    let params = format!(
        r#"{{"buildOrchestratorID":{}}}"#,
        json::marshal(get(&created, "buildOrchestratorID")).unwrap()
    );
    let first = call(&session, "build", &params);
    assert_eq!(get(&first, "status"), &Value::Number(0.0));
    assert!(dir.0.join("out/a.js").is_file());

    dir.write("tsconfig.json", &config("next-out"));
    fail_read.store(true, Ordering::SeqCst);
    let error = call_err(&session, "build", &params);
    assert!(error.contains("injected build read failure"), "{error}");
    assert!(!fail_read.load(Ordering::SeqCst), "the replacement build must reach the failing read");

    let cleaned = call(&session, "cleanBuild", &params);
    assert_eq!(get(&cleaned, "status"), &Value::Number(0.0));
    assert!(!dir.0.join("out/a.js").exists());
    let rebuilt = call(&session, "build", &params);
    assert_eq!(get(&rebuilt, "status"), &Value::Number(0.0));
    assert!(dir.0.join("next-out/a.js").is_file());
    call(&session, "disposeBuildOrchestrator", &params);
}
