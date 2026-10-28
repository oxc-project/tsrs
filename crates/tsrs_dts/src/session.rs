use std::collections::BTreeMap;
use std::sync::Arc;

use tsrs_vfs::{Entries, FS, FileInfo, trackingvfs};

use crate::{BuildOptions, BuildResult, Result};

#[derive(PartialEq, Eq)]
struct Observation {
    exists: bool,
    text: Option<String>,
    directory: bool,
    entries: Entries,
    realpath: String,
    info: Option<FileInfo>,
}
impl Observation {
    fn read(fs: &dyn FS, path: &str) -> Self {
        let directory = fs.directory_exists(path);
        Self {
            exists: fs.file_exists(path),
            text: fs.read_file(path),
            directory,
            entries: if directory { fs.get_accessible_entries(path) } else { Entries::default() },
            realpath: fs.realpath(path),
            info: fs.stat(path),
        }
    }
}

/// A result with the key and filesystem observations it was built from.
struct Snapshot<K> {
    key: K,
    observations: BTreeMap<String, Observation>,
    result: Arc<BuildResult>,
}
impl<K: PartialEq> Snapshot<K> {
    fn new(key: K, result: Arc<BuildResult>, fs: &dyn FS, paths: Vec<String>) -> Self {
        let observations = paths
            .into_iter()
            .map(|path| {
                let observation = Observation::read(fs, &path);
                (path, observation)
            })
            .collect();
        Self { key, observations, result }
    }

    fn reuse(&self, key: &K, fs: &dyn FS) -> Option<Arc<BuildResult>> {
        (self.key == *key
            && self.observations.iter().all(|(path, old)| *old == Observation::read(fs, path)))
        .then(|| Arc::clone(&self.result))
    }
}

/// Inputs selecting a source build: options, multi-entry map and filesystem case sensitivity.
type BuildKey = (BuildOptions, Option<BTreeMap<String, String>>, bool);

#[derive(PartialEq, Eq)]
pub struct LinkKey {
    options: BuildOptions,
    entries: Option<BTreeMap<String, String>>,
    declarations: crate::Files,
    provenance: BTreeMap<String, String>,
    maps: BTreeMap<String, (Vec<String>, tsrs_sourcemap::RawSourceMap)>,
    compiler: tsrs_core::CompilerOptions,
    references: Vec<tsrs_core::ProjectReference>,
    case_sensitive: bool,
}
impl LinkKey {
    pub fn new(
        options: &BuildOptions,
        entries: Option<&BTreeMap<String, String>>,
        captured: &crate::CapturedDeclarations,
        program: &'static tsrs_compiler::Program,
        case_sensitive: bool,
    ) -> Self {
        Self {
            options: options.clone(),
            entries: entries.cloned(),
            declarations: captured.files.clone(),
            provenance: captured.source_to_declaration.clone(),
            maps: captured
                .declaration_maps
                .iter()
                .map(|(path, map)| {
                    (path.clone(), (map.input_source_file_names.clone(), map.source_map.clone()))
                })
                .collect(),
            compiler: program.options().get().clone(),
            references: program.command_line().project_references().to_vec(),
            case_sensitive,
        }
    }
}
#[derive(Default)]
pub struct LinkCache {
    saved: Option<Snapshot<LinkKey>>,
    hits: usize,
}
impl LinkCache {
    pub fn reuse(&mut self, key: &LinkKey, fs: &dyn FS) -> Option<Arc<BuildResult>> {
        let result = self.saved.as_ref()?.reuse(key, fs)?;
        self.hits += 1;
        Some(result)
    }
    pub fn store(
        &mut self,
        key: LinkKey,
        result: Arc<BuildResult>,
        fs: &dyn FS,
        paths: Vec<String>,
    ) {
        self.saved = Some(Snapshot::new(key, result, fs, paths));
    }
}

/// Serial build session caching owned results while filesystem inputs are unchanged.
///
/// Changed inputs are recompiled; identical declaration surfaces and linking
/// dependencies reuse the linked output. No compiler
/// arenas, symbol identities or diagnostics survive a build call.
/// The filesystem must remain stable during each call, including validation of
/// cached observations; callers may change it between calls.
#[derive(Default)]
pub struct BuildSession {
    cached: Option<Snapshot<BuildKey>>,
    cache_hits: usize,
    link_cache: LinkCache,
}
impl BuildSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cache_hits(&self) -> usize {
        self.cache_hits
    }

    pub fn link_cache_hits(&self) -> usize {
        self.link_cache.hits
    }

    pub fn clear(&mut self) {
        self.cached = None;
        self.link_cache.saved = None;
    }

    pub fn build(&mut self, options: &BuildOptions, fs: Arc<dyn FS>) -> Result<Arc<BuildResult>> {
        self.run(options, None, fs)
    }

    /// Like `crate::build_entries`: `options.entry` and `options.output` are ignored.
    pub fn build_entries(
        &mut self,
        options: &BuildOptions,
        entries: &BTreeMap<String, String>,
        fs: Arc<dyn FS>,
    ) -> Result<Arc<BuildResult>> {
        self.run(options, Some(entries), fs)
    }

    fn run(
        &mut self,
        options: &BuildOptions,
        entries: Option<&BTreeMap<String, String>>,
        fs: Arc<dyn FS>,
    ) -> Result<Arc<BuildResult>> {
        // Mapper processes can observe files and state outside the tracking VFS.
        // Do not reuse results whose complete dependency set we cannot validate.

        // TODO: Define a complete mapper dependency/state contract before enabling
        // cache reuse for mapper builds. See docs/DTS.md, "Content mapper cache bypass".
        if options.run_external_code {
            self.clear();
            if let Some(entries) = entries {
                crate::validate_entries(entries)?;
            }
            return crate::build_impl(options, entries, fs, None);
        }
        let key = (options.clone(), entries.cloned(), fs.use_case_sensitive_file_names());
        if let Some(result) = self.cached.as_ref().and_then(|cached| cached.reuse(&key, &*fs)) {
            self.cache_hits += 1;
            return Ok(result);
        }
        // Failures invalidate the previous success, including its dependency set.
        self.cached = None;
        let tracked = Arc::new(trackingvfs::FS::new(Arc::clone(&fs)));
        if let Some(entries) = entries {
            crate::validate_entries(entries)?;
        }
        let result = crate::build_impl(
            options,
            entries,
            Arc::clone(&tracked) as Arc<dyn FS>,
            Some(&mut self.link_cache),
        )
        .inspect_err(|_| self.link_cache.saved = None)?;
        self.cached = Some(Snapshot::new(key, Arc::clone(&result), &*fs, tracked.seen_files()));
        Ok(result)
    }
}
