use tsrs_api::{Handler, Response};
use tsrs_cli::api::*;
// `malloc_trim` is glibc-only and `rss_kib` reads `/proc`; the RSS tests below run only on Linux glibc, each alone in
// its own process because RSS includes the whole process.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use tsrs_core::json::{self, Value};

#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn rss_kib() -> u64 {
    // SAFETY: glibc `malloc_trim` has no preconditions.
    unsafe { malloc_trim(0) };
    tsrs_core::utf8::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

/// API builds write the same files, byte for byte, as the Go-history CLI on the same project graph.
#[test]
fn api_build_outputs_match_cli_build() {
    let base = std::env::temp_dir().join(format!("tsrs-api-buildeq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mk = |root: &std::path::Path| {
        let w = |p: &str, t: &str| {
            std::fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
            std::fs::write(root.join(p), t).unwrap();
        };
        w(
            "core/tsconfig.json",
            r#"{ "compilerOptions": { "composite": true, "outDir": "out", "declarationMap": true, "sourceMap": true } }"#,
        );
        w("core/index.ts", "export const one = 1;\nexport class C { x = 1; private y = 'a'; }\n");
        w(
            "app/tsconfig.json",
            r#"{ "compilerOptions": { "composite": true, "outDir": "out", "strict": true }, "references": [{ "path": "../core" }] }"#,
        );
        w(
            "app/main.ts",
            "import { one, C } from '../core/index';\nexport const two: number = one + 1;\nexport const c = new C();\nlet bad: string = 1;\n",
        );
    };
    let cli = base.join("cli");
    let api = base.join("api");
    mk(&cli);
    mk(&api);
    // The tsrs binary next to this test binary (target/<profile>/tsrs; `cargo build -p tsrs_cli` first).
    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_tsrs"));
    assert!(exe.exists(), "build the tsrs binary first: {}", exe.display());
    let status = std::process::Command::new(&exe)
        .args(["-b", "app"])
        .env("TSRS_CHECKER_ASSIGNMENT", "go")
        .current_dir(&cli)
        .status()
        .unwrap();
    let cwd = api.canonicalize().unwrap().to_string_lossy().into_owned();
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
    let Response::Json(r) =
        s.handle_request("createBuildOrchestrator", br#"{"rootNames":["app"]}"#).unwrap()
    else {
        unreachable!()
    };
    assert!(r.contains("\"buildOrchestratorID\":1"), "{r}");
    let Response::Json(b) = s.handle_request("build", br#"{"buildOrchestratorID":1}"#).unwrap()
    else {
        unreachable!()
    };
    assert!(
        b.contains(&format!("\"status\":{}", status.code().unwrap())),
        "cli exit {:?} vs api {b}",
        status.code()
    );
    let mut files = Vec::new();
    files.extend(["core/out", "app/out"].into_iter().flat_map(|proj| {
        std::fs::read_dir(cli.join(proj))
            .unwrap()
            .map(move |e| format!("{proj}/{}", e.unwrap().file_name().to_string_lossy()))
    }));
    files.sort();
    assert!(files.len() >= 8, "{files:?}");
    for f in &files {
        let a = std::fs::read(cli.join(f)).unwrap();
        let b = std::fs::read(api.join(f)).unwrap_or_else(|_| panic!("api build missing {f}"));
        assert!(a == b, "{f} differs between tsrs -b and the API build");
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// Repeated rebuilds on one API build handle (an edit between builds forces a real program build).
#[test]
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn repeated_builds_memory() {
    if !crate::isolated::in_child("api_memory_test::repeated_builds_memory") {
        return;
    }
    let dir = std::env::temp_dir().join(format!("tsrs-api-buildmem-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "composite": true, "outDir": "out" } }"#,
    )
    .unwrap();
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
    let id = match s.handle_request("createBuildOrchestrator", br#"{"rootNames":["."]}"#).unwrap() {
        Response::Json(t) => {
            let v = json::unmarshal(&t).unwrap();
            let id = match &v {
                Value::Object(o) => o.get("buildOrchestratorID").unwrap().clone(),
                _ => unreachable!(),
            };
            json::marshal(&id).unwrap()
        }
        Response::Binary(_) => unreachable!(),
    };
    let n: usize = std::env::var("N").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    let mut before = 0;
    for i in 0..n + 5 {
        if i == 5 {
            before = rss_kib();
        }
        std::fs::write(dir.join("a.ts"), format!("export const v{i}: number = {i};\n").repeat(50))
            .unwrap();
        s.handle_request("build", format!("{{\"buildOrchestratorID\":{id}}}").as_bytes()).unwrap();
    }
    let grown = rss_kib().saturating_sub(before);
    eprintln!("BUILDMEM x{n}: rss grew {grown} KiB");
    // Before per-build regions each rebuild retained ~19 MiB (lib files reparsed into never-freed arenas and
    // leaked texts); now one build's worth stays (kept for cleans) and older builds are freed.
    assert!(grown < (n as u64) * 512, "rss grew {grown} KiB over {n} builds");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A build that unwinds (moduleResolution with no named kind: the module resolver's `Unexpected
/// moduleResolution` panic, answered as a client error) must free its fresh orchestrator (codec d9be067 review:
/// ~9 MiB leaked per failed build), and the retained orchestrator keeps working.
#[test]
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn failed_builds_free_their_orchestrator() {
    if !crate::isolated::in_child("api_memory_test::failed_builds_free_their_orchestrator") {
        return;
    }
    let dir = std::env::temp_dir().join(format!("tsrs-api-failedbuild-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tsconfig.json"), r#"{ "compilerOptions": { "outDir": "out" } }"#)
        .unwrap();
    std::fs::write(dir.join("w.ts"), "export const w = 1;\n").unwrap();
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
    let create = || match s
        .handle_request(
            "createBuildOrchestrator",
            br#"{"rootNames":["."],"compilerOptions":{"moduleResolution":-1}}"#,
        )
        .unwrap()
    {
        Response::Json(t) => match json::unmarshal(&t).unwrap() {
            Value::Object(o) => json::marshal(o.get("buildOrchestratorID").unwrap()).unwrap(),
            _ => unreachable!(),
        },
        Response::Binary(_) => unreachable!(),
    };
    let id = create();
    let fail = |i: usize| {
        std::fs::write(
            dir.join("a.ts"),
            format!("import {{ w }} from './w';\nexport const v{i} = w;\n").repeat(200),
        )
        .unwrap();
        let e = s
            .handle_request("build", format!("{{\"buildOrchestratorID\":{id}}}").as_bytes())
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            "api: client error: unsupported moduleResolution value -1 (not a ModuleResolutionKind)"
        );
    };
    for i in 0..5 {
        fail(i);
    }
    let n = 30;
    let before = rss_kib();
    for i in 5..5 + n {
        fail(i);
    }
    let grown = rss_kib().saturating_sub(before);
    eprintln!("failed build x{n}: rss grew {grown} KiB");
    assert!(grown < (n as u64) * 256, "rss grew {grown} KiB over {n} failed builds");
    assert!(
        s.handle_request("cleanBuild", format!("{{\"buildOrchestratorID\":{id}}}").as_bytes())
            .is_ok()
    );
    assert!(
        s.handle_request(
            "disposeBuildOrchestrator",
            format!("{{\"buildOrchestratorID\":{id}}}").as_bytes()
        )
        .is_ok()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
