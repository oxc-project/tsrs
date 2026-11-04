//! The opt-in lint result cache (notes/perf-lint-result-cache.md). With the fast preset and `TSRS_LINT_CACHE` set to
//! a directory, each program's frames are stored per file next to the program's incremental state (a tsbuildinfo).
//! A later run replays a file's frames while TypeScript's incremental builder would keep the file's semantic
//! diagnostics, so a file is linted again when its text changes, a file it references changes its declaration
//! signature, or a change reaches the global scope, the libraries or the compiler options.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use tsrs_ast::SourceFile;
use tsrs_compiler::{Context, Program};
use tsrs_core::{CompilerOptions, P, Tristate, json, tspath};
use tsrs_incremental::{BuildInfo, BuildInfoReader};
use tsrs_tsoptions::ParsedCommandLine;
use xxhash_rust::xxh3::xxh3_128;

use crate::engine::Options;
use crate::payload::Payload;

const MAGIC: &[u8] = b"tsrs-lint-cache 1\n";
const HASH_LEN: usize = 16;

pub struct Cache {
    directory: PathBuf,
    // The payload flags that change every file's frames; part of each cache file's name, so that runs with and
    // without them keep separate files.
    flags: String,
    // The linter build, checked against a cache file's contents with the program's compiler options.
    binary: String,
    // The hash of each file's rules and their options, by file name as in the payload.
    rules: FxHashMap<String, [u8; HASH_LEN]>,
    stats: bool,
}

/// One program's cache file, read: the old state and the frames of the files it still holds.
pub struct ProgramCache<'a> {
    cache: &'a Cache,
    file: PathBuf,
    fingerprint: [u8; HASH_LEN],
    data: Vec<u8>,
    // By file path: the ranges in `data` of the file's rules hash and of its frames.
    entries: FxHashMap<String, (Range<usize>, Range<usize>)>,
    // Present on a warm run, after the changes since the old state have been applied.
    incremental: Option<P<tsrs_incremental::Program>>,
}

impl Cache {
    /// `None` unless the fast preset is on, `TSRS_LINT_CACHE` names a directory and the payload overrides no source.
    pub fn new(payload: &Payload, options: Options) -> Option<Cache> {
        if !tsrs_core::fast::enabled() || !payload.source_overrides.is_empty() {
            return None;
        }
        let directory = std::env::var_os("TSRS_LINT_CACHE").filter(|value| !value.is_empty())?;
        let exe = std::env::current_exe().and_then(std::fs::metadata).ok();
        let mut rules = FxHashMap::default();
        for config in &payload.configs {
            let mut text = String::new();
            for rule in &config.rules {
                text.push_str(&rule.name);
                text.push('\0');
                text.push_str(&json::marshal(&rule.options).ok()?);
                text.push('\0');
            }
            let hash = xxh3_128(text.as_bytes()).to_le_bytes();
            for path in &config.file_paths {
                rules.insert(tspath::normalize_slashes(path), hash);
            }
        }
        Some(Cache {
            directory: directory.into(),
            flags: format!(
                "{} {} {} {} {}",
                options.fix,
                options.fix_suggestions,
                options.suppress_program_diagnostics,
                payload.report_syntactic,
                payload.report_semantic
            ),
            binary: format!(
                "{}\0{:?}\0{:?}",
                env!("CARGO_PKG_VERSION"),
                exe.as_ref().map(std::fs::Metadata::len),
                exe.and_then(|metadata| metadata.modified().ok())
            ),
            rules,
            stats: std::env::var("TSRS_LINT_CACHE_STATS").is_ok_and(|value| value == "1"),
        })
    }

    /// The cache file of the program of `config_path`, and `config` with the incremental state turned on, which the
    /// cache needs to build its state; `None` when the configuration turns `incremental` off.
    pub fn program_file(
        &self,
        config_path: &str,
        config: &mut ParsedCommandLine,
    ) -> Option<PathBuf> {
        let options = config.compiler_options().unwrap();
        if !options.is_incremental() {
            if options.incremental.is_false() {
                return None;
            }
            // `incremental` changes no diagnostic of a program with a configuration file.
            config.parsed_config.compiler_options =
                Some(P::new(CompilerOptions { incremental: Tristate::True, ..(*options).clone() }));
        }
        let name = xxh3_128(format!("{config_path}\0{}", self.flags).as_bytes());
        Some(self.directory.join(format!("{name:032x}")))
    }

    /// Reads the program's cache file. When it matches this linter and the program's options, applies the changes
    /// since its state; otherwise the run is cold.
    pub fn load(&self, file: PathBuf, program: &'static Program) -> ProgramCache<'_> {
        let fingerprint =
            xxh3_128(format!("{}\0{:?}", self.binary, *program.options()).as_bytes()).to_le_bytes();
        let mut cache = ProgramCache {
            cache: self,
            file,
            fingerprint,
            data: Vec::new(),
            entries: FxHashMap::default(),
            incremental: None,
        };
        let Ok(data) = std::fs::read(&cache.file) else { return cache };
        let Some((build_info, entries)) = parse(&data, &cache.fingerprint) else {
            return cache;
        };
        let Ok(build_info) = std::str::from_utf8(&data[build_info]) else { return cache };
        let Some(old) = tsrs_incremental::read_build_info_program(
            program.command_line(),
            &Reader(build_info),
            &**program.host(),
        ) else {
            return cache;
        };
        let incremental = tsrs_incremental::new_program(
            program,
            Some(old),
            tsrs_incremental::create_host(Arc::clone(program.host())),
            None,
            false,
        );
        incremental.collect_affected_files(&Context::default());
        cache.incremental = Some(incremental);
        cache.entries = entries;
        cache.data = data;
        cache
    }
}

impl ProgramCache<'_> {
    /// The stored frames of `file` (named `name` in the payload), if they still hold.
    pub fn replay(&self, file: P<SourceFile>, name: &str) -> Option<&[u8]> {
        self.incremental?.snapshot.semantic_diagnostics_per_file.load(file.path())?;
        let (rules, frames) = self.entries.get(file.path().as_str())?;
        (self.data[rules.clone()] == *self.cache.rules.get(name)?)
            .then(|| &self.data[frames.clone()])
    }

    /// Records the frames of the files linted in this run (`file`, payload name, frames) and writes the cache file.
    pub fn finish(
        self,
        program: &'static Program,
        linted: &[(P<SourceFile>, &str, &[u8])],
        replayed: usize,
    ) {
        if self.cache.stats {
            eprintln!("tsrs-lint cache: {} linted, {replayed} replayed", linted.len());
        }
        let incremental = self.incremental.unwrap_or_else(|| {
            tsrs_incremental::new_program(
                program,
                None,
                tsrs_incremental::create_host(Arc::clone(program.host())),
                None,
                false,
            )
        });
        let snapshot = incremental.snapshot;
        // An empty entry marks the file's state as current. Its semantic diagnostics are in its frames when the
        // payload asks for them, and the cache reads no others.
        for &(file, _, _) in linted {
            snapshot.semantic_diagnostics_per_file.store(file.path().clone(), Arc::default());
        }
        // The importers of a changed file have just been linted against its shape, so its declaration signature can
        // replace the file version that stands in for it: without that, the next edit of the file would invalidate
        // every file importing it, even one that keeps its declarations. In chunks of the watcher's size: a single
        // emit of every file of a large JavaScript program exhausted the arena address range.
        incremental.compute_placeholder_signatures(&Context::default(), 256);
        if !linted.is_empty() || snapshot.build_info_emit_pending.get() {
            self.write(program, incremental, linted);
        }
    }

    fn write(
        &self,
        program: &'static Program,
        incremental: P<tsrs_incremental::Program>,
        linted: &[(P<SourceFile>, &str, &[u8])],
    ) {
        let Ok(build_info) = incremental.build_info() else { return };
        let build_info = build_info.marshal();
        let mut data = Vec::with_capacity(build_info.len() + self.data.len());
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&self.fingerprint);
        data.extend_from_slice(&[0; HASH_LEN]);
        let body = data.len();
        data.extend_from_slice(&(build_info.len() as u64).to_le_bytes());
        data.extend_from_slice(build_info.as_bytes());
        let linted: FxHashMap<&str, (&[u8], &[u8])> = linted
            .iter()
            .filter_map(|&(file, name, frames)| {
                Some((file.get().path().as_str(), (&self.cache.rules.get(name)?[..], frames)))
            })
            .collect();
        let semantic = &incremental.snapshot.semantic_diagnostics_per_file;
        for file in program.get_source_files() {
            let path = file.path().as_str();
            let entry = linted.get(path).copied().or_else(|| {
                let (rules, frames) = self.entries.get(path)?;
                semantic.load(file.path())?;
                Some((&self.data[rules.clone()], &self.data[frames.clone()]))
            });
            if let Some((rules, frames)) = entry {
                data.extend_from_slice(&(path.len() as u32).to_le_bytes());
                data.extend_from_slice(path.as_bytes());
                data.extend_from_slice(rules);
                data.extend_from_slice(&(frames.len() as u32).to_le_bytes());
                data.extend_from_slice(frames);
            }
        }
        let checksum = xxh3_128(&data[body..]).to_le_bytes();
        data[body - HASH_LEN..body].copy_from_slice(&checksum);
        // Written beside the file and renamed over it, so that a concurrent run reads the old file or the new one.
        let temporary = self.file.with_extension(format!("{}.tmp", std::process::id()));
        let result = std::fs::create_dir_all(&self.cache.directory)
            .and_then(|()| std::fs::write(&temporary, &data))
            .and_then(|()| std::fs::rename(&temporary, &self.file));
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            eprintln!("tsrs-lint: could not write the lint cache {}: {error}", self.file.display());
        }
    }
}

struct Reader<'a>(&'a str);

impl BuildInfoReader for Reader<'_> {
    fn read_build_info(&self, _: &ParsedCommandLine) -> Option<BuildInfo> {
        BuildInfo::unmarshal(self.0).ok()
    }
}

type Entries = FxHashMap<String, (Range<usize>, Range<usize>)>;

// The layout `write` produces: the magic line, the fingerprint, the checksum of the rest, the build info's length and
// text, then for each file its path's length and text, its rules hash and its frames' length and bytes. Hashes are
// xxh3-128 and lengths little-endian.
fn parse(data: &[u8], fingerprint: &[u8]) -> Option<(Range<usize>, Entries)> {
    let mut at = 0;
    if data[take(data, &mut at, MAGIC.len())?] != *MAGIC
        || data[take(data, &mut at, HASH_LEN)?] != *fingerprint
        || data[take(data, &mut at, HASH_LEN)?] != xxh3_128(&data[at..]).to_le_bytes()
    {
        return None;
    }
    let len = u64::from_le_bytes(data[take(data, &mut at, 8)?].try_into().ok()?);
    let build_info = take(data, &mut at, usize::try_from(len).ok()?)?;
    let mut entries = FxHashMap::default();
    while at < data.len() {
        let len = u32::from_le_bytes(data[take(data, &mut at, 4)?].try_into().ok()?) as usize;
        let path = std::str::from_utf8(&data[take(data, &mut at, len)?]).ok()?.to_owned();
        let rules = take(data, &mut at, HASH_LEN)?;
        let len = u32::from_le_bytes(data[take(data, &mut at, 4)?].try_into().ok()?) as usize;
        entries.insert(path, (rules, take(data, &mut at, len)?));
    }
    Some((build_info, entries))
}

fn take(data: &[u8], at: &mut usize, len: usize) -> Option<Range<usize>> {
    let range = *at..at.checked_add(len).filter(|&end| end <= data.len())?;
    *at = range.end;
    Some(range)
}
