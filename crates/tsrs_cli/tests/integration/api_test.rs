use tsrs_api::Session;

use tsrs_api::{Handler, Response};
use tsrs_cli::api::*;
use tsrs_core::json::{self, Value};

fn call(s: &Session, method: &str, params: &str) -> Value {
    match s.handle_request(method, params.as_bytes()) {
        Ok(Response::Json(t)) => json::unmarshal(&t).unwrap(),
        other => panic!("{method}: {other:?}"),
    }
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Object(o) => o.get(key).unwrap_or(&Value::Null),
        _ => &Value::Null,
    }
}

#[test]
fn api_flags() {
    let f = parse_api_flags(&[
        "--async".into(),
        "-cwd".into(),
        "/x".into(),
        "--callbacks=readFile,fileExists".into(),
        "--useCaseSensitiveFileNames=false".into(),
    ])
    .unwrap();
    assert!(f.is_async && !f.case_sensitive);
    assert_eq!(f.cwd, "/x");
    assert_eq!(f.callbacks, vec!["readFile", "fileExists"]);
    assert!(parse_api_flags(&["--nope".into()]).is_err());
}

#[test]
fn api_build_refreshes_cli_cached_interface_absence() {
    let dir = std::env::temp_dir().join(format!("tsrs-api-canonical-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/regressions/merged-interface-check-site");
    for file in ["bases.ts", "a.ts", "b.ts", "c.ts", "d.d.ts", "tsconfig.json"] {
        std::fs::copy(fixture.join(file), dir.join(file)).unwrap();
    }
    let mut config =
        json::unmarshal(&std::fs::read_to_string(dir.join("tsconfig.json")).unwrap()).unwrap();
    let Value::Object(ref mut config_object) = config else { unreachable!() };
    let Value::Object(options) = config_object.get_mut("compilerOptions").unwrap() else {
        unreachable!()
    };
    options.insert("incremental", Value::Bool(true));
    std::fs::write(dir.join("tsconfig.json"), json::marshal(&config).unwrap()).unwrap();
    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_tsrs"));
    let cli = || {
        let output = std::process::Command::new(&exe)
            .args(["-p", ".", "--pretty", "false", "--fast"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, [] as [u8; 0]);
        String::from_utf8(output.stdout).unwrap()
    };
    let expected_cli = cli();
    assert_eq!(expected_cli.matches("error TS2320").count(), 1);
    let info = dir.join("tsconfig.tsbuildinfo");
    let cli_info = std::fs::read_to_string(&info).unwrap();
    assert!(cli_info.contains("tsrsCanonicalCheckSignature"));
    let flags = ApiFlags {
        cwd: dir.canonicalize().unwrap().to_string_lossy().into_owned(),
        pipe_path: String::new(),
        callbacks: Vec::new(),
        case_sensitive: true,
        is_async: true,
        timing: false,
        run_external_code: false,
    };
    let (s, _) = new_api_session(&flags).unwrap();
    let r = call(&s, "createBuildOrchestrator", r#"{"rootNames":["."]}"#);
    let id = json::marshal(get(&r, "buildOrchestratorID")).unwrap();
    let build = || call(&s, "build", &format!("{{\"buildOrchestratorID\":{id}}}"));
    let migrated = build();
    assert_eq!(get(&migrated, "status"), &Value::Number(2.0));
    let Value::Array(diagnostics) = get(&migrated, "diagnostics") else { panic!("{migrated:?}") };
    assert_eq!(diagnostics.iter().filter(|d| get(d, "code") == &Value::Number(2320.0)).count(), 1);
    let api_info = std::fs::read_to_string(&info).unwrap();
    assert!(!api_info.contains("tsrsCanonicalCheckSignature"));
    for _ in 0..2 {
        assert_eq!(get(&build(), "diagnostics"), get(&migrated, "diagnostics"));
        assert_eq!(std::fs::read_to_string(&info).unwrap(), api_info);
    }
    std::fs::remove_file(&info).unwrap();
    assert_eq!(
        get(&build(), "diagnostics"),
        get(&migrated, "diagnostics"),
        "CLI cache migration must match fresh API output"
    );
    assert_eq!(std::fs::read_to_string(&info).unwrap(), api_info);
    assert_eq!(cli(), expected_cli, "API-to-CLI must refresh all ordered diagnostics");
    assert_eq!(std::fs::read_to_string(&info).unwrap(), cli_info);
    call(&s, "disposeBuildOrchestrator", &format!("{{\"buildOrchestratorID\":{id}}}"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_orchestrator_builds_references_and_cleans() {
    let dir = std::env::temp_dir().join(format!("tsrs-api-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("core")).unwrap();
    std::fs::create_dir_all(dir.join("app")).unwrap();
    let w = |p: &str, t: &str| std::fs::write(dir.join(p), t).unwrap();
    w("core/tsconfig.json", r#"{ "compilerOptions": { "composite": true, "outDir": "out" } }"#);
    w("core/index.ts", "export const one = 1;\n");
    w(
        "app/tsconfig.json",
        r#"{ "compilerOptions": { "composite": true, "outDir": "out" }, "references": [{ "path": "../core" }] }"#,
    );
    w("app/main.ts", "import { one } from '../core/index';\nexport const two: number = one + 1;\n");
    let cwd = dir.canonicalize().unwrap().to_string_lossy().into_owned();
    let flags = ApiFlags {
        cwd,
        pipe_path: String::new(),
        callbacks: Vec::new(),
        case_sensitive: true,
        is_async: true,
        timing: false,
        run_external_code: false,
    };
    let (s, _) = new_api_session(&flags).unwrap();
    let s = s;
    let r = call(&s, "createBuildOrchestrator", r#"{"rootNames":["app"]}"#);
    let id = json::marshal(get(&r, "buildOrchestratorID")).unwrap();

    let r = call(
        &s,
        "buildReferences",
        &format!("{{\"buildOrchestratorID\":{id},\"project\":\"app\"}}"),
    );
    assert_eq!(get(&r, "status"), &Value::Number(0.0), "{}", json::marshal(&r).unwrap());
    assert_eq!(get(get(&r, "statistics"), "Projects"), &Value::Number(1.0));
    assert!(dir.join("core/out/index.js").exists());
    assert!(!dir.join("app/out/main.js").exists());

    let r = call(&s, "build", &format!("{{\"buildOrchestratorID\":{id}}}"));
    assert_eq!(get(&r, "status"), &Value::Number(0.0), "{}", json::marshal(&r).unwrap());
    assert_eq!(get(get(&r, "statistics"), "Projects"), &Value::Number(2.0));
    assert!(dir.join("app/out/main.js").exists());

    // Second build: everything is up to date.
    let r = call(&s, "build", &format!("{{\"buildOrchestratorID\":{id}}}"));
    assert_eq!(
        get(get(&r, "statistics"), "ProjectsBuilt"),
        &Value::Number(0.0),
        "{}",
        json::marshal(&r).unwrap()
    );

    let r = call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{id}}}"));
    assert_eq!(get(&r, "status"), &Value::Number(0.0));
    assert!(json::marshal(get(&r, "filesDeleted")).unwrap().contains("main.js"));
    assert!(!dir.join("app/out/main.js").exists());
    assert!(!dir.join("core/out/index.js").exists());

    // Upstream "returns build response information after clean": clean one project, rebuild it, then
    // cleanReferences of the downstream project deletes the rebuilt outputs too.
    call(&s, "build", &format!("{{\"buildOrchestratorID\":{id}}}"));
    let r =
        call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{id},\"project\":\"core\"}}"));
    assert!(
        json::marshal(get(&r, "filesDeleted")).unwrap().contains("index.js"),
        "{}",
        json::marshal(&r).unwrap()
    );
    let r = call(&s, "build", &format!("{{\"buildOrchestratorID\":{id},\"project\":\"core\"}}"));
    assert_eq!(
        get(get(&r, "statistics"), "ProjectsBuilt"),
        &Value::Number(1.0),
        "{}",
        json::marshal(&r).unwrap()
    );
    assert!(dir.join("core/out/index.js").exists());
    let r = call(
        &s,
        "cleanReferences",
        &format!("{{\"buildOrchestratorID\":{id},\"project\":\"app\"}}"),
    );
    assert!(
        json::marshal(get(&r, "filesDeleted")).unwrap().contains("core/out/index.js"),
        "{}",
        json::marshal(&r).unwrap()
    );

    // Upstream "cleans and rebuilds ..." tail, parity f703: after build + clean, the client recreates only one
    // output; Go's next clean reuses the cached existence answers (cachedvfs, cleared only by a build), so it
    // lists every output of the project again (index.js, index.d.ts, tsbuildinfo) and status stays 0.
    call(&s, "build", &format!("{{\"buildOrchestratorID\":{id}}}"));
    let all = |r: &Value| {
        let mut v: Vec<String> = match get(r, "filesDeleted") {
            Value::Array(a) => a
                .iter()
                .map(|f| {
                    json::marshal(f)
                        .unwrap()
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .trim_end_matches('"')
                        .to_string()
                })
                .collect(),
            _ => Vec::new(),
        };
        v.sort();
        v
    };
    let r =
        call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{id},\"project\":\"core\"}}"));
    assert_eq!(
        all(&r),
        ["index.d.ts", "index.js", "tsconfig.tsbuildinfo"],
        "{}",
        json::marshal(&r).unwrap()
    );
    w("core/out/index.js", "export const one = 1;\n");
    let r =
        call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{id},\"project\":\"core\"}}"));
    assert_eq!(get(&r, "status"), &Value::Number(0.0), "{}", json::marshal(&r).unwrap());
    assert_eq!(
        all(&r),
        ["index.d.ts", "index.js", "tsconfig.tsbuildinfo"],
        "{}",
        json::marshal(&r).unwrap()
    );
    assert!(!dir.join("core/out/index.js").exists());

    // moduleResolution with no named kind (runtime 0689274 review): pinned Go builds import-free projects and
    // panics only when a module is resolved; tsrs answers a stable client error there and stays usable.
    let r = call(
        &s,
        "createBuildOrchestrator",
        r#"{"rootNames":["core"],"compilerOptions":{"moduleResolution":12345,"checkers":9223372036854775807}}"#,
    );
    let other = json::marshal(get(&r, "buildOrchestratorID")).unwrap();
    let r = call(&s, "build", &format!("{{\"buildOrchestratorID\":{other}}}"));
    assert_eq!(get(&r, "status"), &Value::Number(0.0), "{}", json::marshal(&r).unwrap());
    assert_eq!(get(get(&r, "statistics"), "ProjectsBuilt"), &Value::Number(1.0));
    assert!(dir.join("core/out/index.js").exists());
    let r = call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{other}}}"));
    assert!(
        json::marshal(get(&r, "filesDeleted")).unwrap().contains("index.js"),
        "{}",
        json::marshal(&r).unwrap()
    );
    assert_eq!(
        call(&s, "disposeBuildOrchestrator", &format!("{{\"buildOrchestratorID\":{other}}}")),
        Value::Bool(true)
    );
    let r = call(
        &s,
        "createBuildOrchestrator",
        r#"{"rootNames":["app"],"compilerOptions":{"moduleResolution":-1}}"#,
    );
    let other = json::marshal(get(&r, "buildOrchestratorID")).unwrap();
    let e = s
        .handle_request("build", format!("{{\"buildOrchestratorID\":{other}}}").as_bytes())
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "api: client error: unsupported moduleResolution value -1 (not a ModuleResolutionKind)"
    );
    let r = call(&s, "cleanBuild", &format!("{{\"buildOrchestratorID\":{other}}}"));
    assert_eq!(get(&r, "status"), &Value::Number(0.0), "{}", json::marshal(&r).unwrap());
    assert_eq!(
        call(&s, "disposeBuildOrchestrator", &format!("{{\"buildOrchestratorID\":{other}}}")),
        Value::Bool(true)
    );
    call(&s, "createBuildOrchestrator", r#"{"rootNames":["app"]}"#);

    // Go `*int` builders: any int64 is accepted (API builds run one builder regardless; runtime-f2-review).
    for n in ["2147483648", "-2147483649", "9223372036854775807"] {
        let r = call(
            &s,
            "createBuildOrchestrator",
            &format!(r#"{{"rootNames":["app"],"buildOptions":{{"builders":{n}}}}}"#),
        );
        let other = json::marshal(get(&r, "buildOrchestratorID")).unwrap();
        let r = call(&s, "build", &format!("{{\"buildOrchestratorID\":{other}}}"));
        assert_eq!(
            get(&r, "status"),
            &Value::Number(0.0),
            "builders {n}: {}",
            json::marshal(&r).unwrap()
        );
        call(&s, "disposeBuildOrchestrator", &format!("{{\"buildOrchestratorID\":{other}}}"));
    }

    assert_eq!(
        call(&s, "disposeBuildOrchestrator", &format!("{{\"buildOrchestratorID\":{id}}}")),
        Value::Bool(true)
    );
    assert!(
        s.handle_request("build", format!("{{\"buildOrchestratorID\":{id}}}").as_bytes()).is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
