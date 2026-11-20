// A name that is only a drive root (`C:`) or a UNC server root (`//server`). Go resolves these names through
// `tspath.ToRootedPath` (`rooted_path.go:29`), and `ensureRootedPathRootSeparator` (`rooted_path.go:65`) appends the
// root separator: the candidate is `C:/`, not `C:`. A type root candidate keeps file intent, so Go probes `C:/.d.ts`
// first. A relative module name that is a root is directory-only (`normalizePathForCJSResolution`,
// `resolver.go:2275`). The expected traces are the pinned Go resolver's output for the same file systems.

use tsrs_core::{CompilerOptions, ModuleKind, ModuleResolutionKind, P, Tristate};
use tsrs_module::resolver::{ResolverOptions, new_resolver, resolve_path_for_module};
use tsrs_module::{DiagAndArgs, ResolutionHost, Resolver};
use tsrs_vfs::{FS, vfstest};

struct Host {
    fs: Box<dyn FS>,
    cwd: String,
}

impl ResolutionHost for Host {
    fn fs(&self) -> &dyn FS {
        &*self.fs
    }
    fn get_current_directory(&self) -> &str {
        &self.cwd
    }
}

fn host(files: &[(&str, &str)], cwd: &str) -> &'static Host {
    let fs = vfstest::from_map(files.iter().map(|&(path, text)| (path, text)), true);
    Box::leak(Box::new(Host { fs: Box::new(fs), cwd: cwd.to_string() }))
}

fn options(type_roots: Option<&str>) -> P<CompilerOptions> {
    P::new(CompilerOptions {
        module_resolution: ModuleResolutionKind::BUNDLER,
        type_roots: type_roots.map(|root| vec![root.to_string()]),
        trace_resolution: Tristate::True,
        ..Default::default()
    })
}

fn format_trace(trace: &[DiagAndArgs]) -> Vec<String> {
    trace
        .iter()
        .map(|entry| {
            let args: Vec<&dyn std::fmt::Display> =
                entry.args.iter().map(|arg| arg as &dyn std::fmt::Display).collect();
            entry.message.localize(&args)
        })
        .collect()
}

fn resolve_type(files: &[(&str, &str)], cwd: &str, name: &str) -> (String, Vec<String>) {
    let resolver = new_resolver(ResolverOptions::new(
        host(files, cwd),
        options(Some(&format!("{cwd}/types"))),
    ));
    let (resolved, trace) = resolver.resolve_type_reference_directive(
        name,
        &format!("{cwd}/a.ts"),
        ModuleKind::COMMON_JS,
        None,
    );
    (resolved.resolved_file_name.to_string(), format_trace(&trace))
}

fn resolve_module(files: &[(&str, &str)], name: &str) -> (String, Vec<String>) {
    let resolver = new_resolver(ResolverOptions::new(host(files, "C:/app"), options(None)));
    let (resolved, trace) =
        resolver.resolve_module_name(name, "C:/app/a.ts", ModuleKind::COMMON_JS, None).unwrap();
    (resolved.resolved_file_name.to_string(), format_trace(&trace))
}

// Before the fix, the candidate was `C:`, and the file probe `C:.d.ts` is not an absolute path: the file system
// panicked with "vfs: path \"C:.d.ts\" is not absolute".
#[test]
fn type_reference_to_a_drive_root_probes_the_root_file_first() {
    let (resolved, trace) = resolve_type(
        &[("C:/app/types/keep", ""), ("C:/.d.ts", "declare const right: number;")],
        "C:/app",
        "C:",
    );
    assert_eq!(resolved, "C:/.d.ts");
    assert_eq!(
        trace,
        [
            "======== Resolving type reference directive 'C:', containing file 'C:/app/a.ts', root directory 'C:/app/types'. ========",
            "Resolving with primary search path 'C:/app/types'.",
            "File 'C:/.d.ts' exists - use it as a name resolution result.",
            "Resolving real path for 'C:/.d.ts', result 'C:/.d.ts'.",
            "======== Type reference directive 'C:' was successfully resolved to 'C:/.d.ts', primary: true. ========",
        ]
    );
}

#[test]
fn type_reference_to_a_drive_root_falls_back_to_the_root_directory() {
    let (resolved, trace) = resolve_type(
        &[("C:/app/types/keep", ""), ("C:/index.d.ts", "declare const right: number;")],
        "C:/app",
        "C:",
    );
    assert_eq!(resolved, "C:/index.d.ts");
    assert_eq!(
        trace,
        [
            "======== Resolving type reference directive 'C:', containing file 'C:/app/a.ts', root directory 'C:/app/types'. ========",
            "Resolving with primary search path 'C:/app/types'.",
            "File 'C:/.d.ts' does not exist.",
            "File 'C:/package.json' does not exist.",
            "File 'C:/index.d.ts' exists - use it as a name resolution result.",
            "Resolving real path for 'C:/index.d.ts', result 'C:/index.d.ts'.",
            "======== Type reference directive 'C:' was successfully resolved to 'C:/index.d.ts', primary: true. ========",
        ]
    );
}

// The secondary lookup treats `C:` as a relative module name: its candidate is the directory `C:/`.
#[test]
fn unresolved_type_reference_to_a_drive_root() {
    let (resolved, trace) = resolve_type(&[("C:/app/types/keep", "")], "C:/app", "C:");
    assert_eq!(resolved, "");
    assert_eq!(
        trace,
        [
            "======== Resolving type reference directive 'C:', containing file 'C:/app/a.ts', root directory 'C:/app/types'. ========",
            "Resolving with primary search path 'C:/app/types'.",
            "File 'C:/.d.ts' does not exist.",
            "File 'C:/package.json' does not exist.",
            "File 'C:/index.d.ts' does not exist.",
            "Looking up in 'node_modules' folder, initial location 'C:/app'.",
            "Loading module as file / folder, candidate module location 'C:/', target file types: Declaration.",
            "File 'C:/package.json' does not exist according to earlier cached lookups.",
            "File 'C:/index.d.ts' does not exist.",
            "======== Type reference directive 'C:' was not resolved. ========",
        ]
    );
}

// The vfstest file systems of Go and tsrs cannot hold files under `//server/`, so the UNC cases check the candidate.
// The expected values are Go's `getCandidateFromTypeRoot` results for the same type roots and names.
#[test]
fn type_root_candidates_keep_the_root_separator() {
    for (type_root, name, directory_only, expected) in [
        ("C:/app/types", "C:", false, "C:/"),
        ("C:/app/types", "C:/", true, "C:/"),
        ("C:/app/types", "C:\\", true, "C:/"),
        ("C:/app/types", "../../..", false, "C:/"),
        ("//server/app/types", "//server", false, "//server/"),
        ("//server/app/types", "//server/", true, "//server/"),
        ("//server/app/types", "../../../..", false, "//server/"),
        ("/app/types", "/", true, "/"),
    ] {
        assert_eq!(
            resolve_path_for_module(type_root, name, directory_only),
            expected,
            "{type_root:?} + {name:?}"
        );
    }
}

#[test]
fn relative_import_of_a_drive_root_is_a_directory_lookup() {
    let (resolved, trace) =
        resolve_module(&[("C:/app/keep", ""), ("C:/index.d.ts", "export {};")], "C:");
    assert_eq!(resolved, "C:/index.d.ts");
    assert_eq!(
        trace,
        [
            "======== Resolving module 'C:' from 'C:/app/a.ts'. ========",
            "Explicitly specified module resolution kind: 'Bundler'.",
            "Resolving in CJS mode with conditions 'require', 'types'.",
            "Loading module as file / folder, candidate module location 'C:/', target file types: TypeScript, JavaScript, Declaration, JSON.",
            "File 'C:/package.json' does not exist.",
            "File 'C:/index.ts' does not exist.",
            "File 'C:/index.tsx' does not exist.",
            "File 'C:/index.d.ts' exists - use it as a name resolution result.",
            "======== Module name 'C:' was successfully resolved to 'C:/index.d.ts'. ========",
        ]
    );
}
