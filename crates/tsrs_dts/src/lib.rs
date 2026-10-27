//! A new declaration linker built exclusively on tsrs compiler infrastructure.
//! Compilation, declaration emission, binding, symbol resolution and printing
//! are provided by tsrs. All intermediate and final artifacts remain in memory.
#[doc(hidden)]
pub mod diagnostics;
mod emit;
mod graph;
mod host;
mod link;
mod maps;
mod output;
mod project;
mod references;
mod session;

pub use session::BuildSession;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use tsrs_vfs::FS;

pub use emit::{CapturedDeclarations, emit_declarations};
pub use link::{
    DeclarationContext, DeclarationInputs, LinkOptions, build_from_program, link_declarations,
};

pub type Files = BTreeMap<String, String>;

#[derive(Debug)]
#[expect(
    clippy::error_impl_error,
    reason = "preserve the public Error type used by declaration-linker callers"
)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Bundle { file_name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    pub config_path: String,
    /// Absolute source entry for `build`. `build_entries` ignores it.
    pub entry: String,
    /// Output for `build`. `build_entries` names outputs by its entry keys.
    pub output: OutputMode,
    /// Path used by tsrs to find its standard libraries. For bundled libraries
    /// use `tsrs_vfs::bundled::lib_path()` with a bundled filesystem wrapper.
    pub default_library_path: String,
    /// Allow configured content mappers to execute external processes. Disabled
    /// by default at the API boundary; mapper builds bypass session caches.
    pub run_external_code: bool,
    /// Reuse diagnostics within this source-to-declaration build. UMD, project
    /// references, mappers and skipped library checking use full checks instead.
    pub reuse_diagnostics: bool,
}

#[derive(Clone, Debug)]
pub struct BuildResult {
    /// Final bundled artifacts.
    pub files: Files,
    pub emitted_declarations: usize,
    pub retained_statements: usize,
    pub removed_statements: usize,
}

/// Compile a source project, emit declarations in memory, bind those declarations
/// with tsrs, calculate symbol reachability and print final outputs with tsrs.
///
/// The supplied filesystem is read-only to this pipeline. Nothing is written,
/// including the final artifacts; the caller owns final output I/O.
pub fn build(options: &BuildOptions, fs: Arc<dyn FS>) -> Result<BuildResult> {
    build_impl(options, None, fs, None).map(Arc::unwrap_or_clone)
}

/// Link multiple public entries through one compiler and symbol graph. Keys are
/// output declaration basenames; values are absolute source entry paths.
/// `options.entry` and `options.output` are ignored.
pub fn build_entries(
    options: &BuildOptions,
    entries: &BTreeMap<String, String>,
    fs: Arc<dyn FS>,
) -> Result<BuildResult> {
    validate_entries(entries)?;
    build_impl(options, Some(entries), fs, None).map(Arc::unwrap_or_clone)
}

/// Output names starting with this prefix are reserved for the shared bundle
/// (`output::SHARED_BUNDLE`) and its ambient, companion and map files.
const SHARED_PREFIX: &str = "__shared.";

fn validate_entries(entries: &BTreeMap<String, String>) -> Result<()> {
    if entries.is_empty() {
        return Err(Error("at least one public entry is required".into()));
    }
    for name in entries.keys() {
        if !tsrs_core::tspath::is_declaration_file_name(name)
            || name.contains(['/', '\\', ':'])
            || std::path::Path::new(name).components().count() != 1
            || name.starts_with(SHARED_PREFIX)
        {
            return Err(Error(format!("invalid or reserved entry output name: {name}")));
        }
    }
    Ok(())
}

fn validate_output_names(entries: &BTreeMap<String, String>, case_sensitive: bool) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    for name in entries.keys() {
        let key = tsrs_core::tspath::get_canonical_file_name(name, case_sensitive);
        if key.starts_with(SHARED_PREFIX) || !names.insert(key) {
            return Err(Error(format!("duplicate or reserved entry output name: {name}")));
        }
    }
    Ok(())
}

fn build_impl(
    options: &BuildOptions,
    entries: Option<&BTreeMap<String, String>>,
    fs: Arc<dyn FS>,
    link_cache: Option<&mut session::LinkCache>,
) -> Result<Arc<BuildResult>> {
    // Workaround for caller-thread allocations that escape nested checker/emit
    // scratch scopes: without an enclosing region they fall back to the thread's
    // leak arena (tsrs_core::arena module docs), retaining memory across builds.
    // This root outlives both Programs and their diagnostic snapshots.

    // TODO: Give escaping allocations owners that reclaim them after each build,
    // then remove this root. See docs/DTS.md, "Workarounds and constraints".
    let invocation = tsrs_core::arena::Region::new_scratch(1 << 20);
    let _invocation_scope = invocation.enter();
    let fs = references::compile(options, fs)?;
    let source = project::source_program(options, Arc::clone(&fs))?;
    let reuse = options.reuse_diagnostics && diagnostics::eligible(source.program());
    let diagnostics = reuse.then(|| diagnostics::Diagnostics::new(source.program()));
    let context = tsrs_compiler::Context::background();
    let captured = if let Some(diagnostics) = &diagnostics {
        diagnostics
            .check(source.program(), || project::source_program(options, Arc::clone(&fs)))?;
        emit::emit_checked_declarations(source.program(), &context)?
    } else {
        emit_declarations(source.program(), &context)?
    };
    let context = DeclarationContext::from_program(source.program());
    let link_options = LinkOptions {
        entries: entries.cloned().unwrap_or_else(|| {
            let OutputMode::Bundle { file_name } = &options.output;
            BTreeMap::from([(file_name.clone(), options.entry.clone())])
        }),
    };
    let link = |fs| {
        link::link(&captured, &context, &link_options, fs, entries.is_some(), diagnostics.as_ref())
    };
    let Some(cache) = link_cache else {
        return link(fs).map(Arc::new);
    };
    let key = session::LinkKey::new(
        options,
        entries,
        &captured,
        source.program(),
        fs.use_case_sensitive_file_names(),
    );
    if let Some(result) = cache.reuse(&key, &*fs) {
        return Ok(result);
    }
    let tracking = Arc::new(tsrs_vfs::trackingvfs::FS::new(Arc::clone(&fs)));
    let result = Arc::new(link(Arc::clone(&tracking) as Arc<dyn FS>)?);
    cache.store(key, Arc::clone(&result), &*fs, tracking.seen_files());
    Ok(result)
}
