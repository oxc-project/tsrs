// tsrs-only: the file system a resident check server's runs read through (daemon/mod.rs). It records every query a
// run makes with its answer. The next request asks each of them again (`validate`): if every answer is the same, a
// cold run would see the same file system and print the same output; if not, the new answers serve the rebuild
// without asking the system twice.
//
// A file's content is not read again to validate it: its `Stamp` (device, inode, size, mode, modification and
// change times) stands for it. The kernel sets the change time on every write, rename and attribute change, and no
// system call sets it back, so a file whose stamp is unchanged was not written. One gap: a write within the same
// clock tick as the stamp that keeps the size leaves the stamp as it was (coarse file system clocks tick every few
// milliseconds on Linux, every second on HFS+). A stamp taken less than `RACY_NS` after the file's newest time is
// therefore "racy" and not trusted: the next validation reads that file again and compares its hash, as git does for
// files changed in the second its index was written.

use std::cell::Cell;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::time::SystemTime;

use rayon::prelude::*;
use tsrs_core::collections::SyncMap;
use tsrs_vfs::{Entries, FS, FileInfo};

/// How long after a file's newest time a stamp of it is not trusted (see the module comment).
const RACY_NS: i128 = 2_000_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    dev: u64,
    ino: u64,
    size: u64,
    mode: u32,
    mtime: i128,
    ctime: i128,
}

/// A file's or directory's `Stamp` (`None`: it does not exist or cannot be read), and whether it was racy when taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fingerprint {
    stamp: Option<Stamp>,
    racy: bool,
}

fn now_ns() -> i128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i128::try_from(d.as_nanos()).unwrap_or(i128::MAX))
}

/// Stamps `path` (following symlinks, as reads and listings do).
pub fn fingerprint(path: &str) -> Fingerprint {
    let taken = now_ns();
    let stamp = std::fs::metadata(path).ok().map(|md| Stamp {
        dev: md.dev(),
        ino: md.ino(),
        size: md.size(),
        mode: md.mode(),
        mtime: i128::from(md.mtime()) * 1_000_000_000 + i128::from(md.mtime_nsec()),
        ctime: i128::from(md.ctime()) * 1_000_000_000 + i128::from(md.ctime_nsec()),
    });
    let racy = stamp.is_some_and(|s| s.mtime.max(s.ctime) + RACY_NS > taken);
    Fingerprint { stamp, racy }
}

impl Fingerprint {
    /// Whether a later stamp `now` shows that the file has not changed since this one was taken.
    fn vouches_for(&self, now: &Fingerprint) -> bool {
        !self.racy && self.stamp.is_some() && self.stamp == now.stamp
    }
}

/// The answer to a `read_file`: the stamp taken just before reading and a hash of what was read (`None`: the read
/// failed).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReadAnswer {
    fingerprint: Fingerprint,
    hash: Option<u128>,
}

impl ReadAnswer {
    /// Whether both reads returned the same text.
    pub fn same_text(&self, other: &ReadAnswer) -> bool {
        self.hash.is_some() && self.hash == other.hash
    }
}

fn hash(text: &str) -> u128 {
    xxhash_rust::xxh3::xxh3_128(text.as_bytes())
}

/// The answer to a `get_accessible_entries`, with the directory's stamp.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Listing {
    fingerprint: Fingerprint,
    entries: Entries,
    /// The stamp vouches for the listing: no entry is a symlink (whose target can change type) and every entry is
    /// a file or a directory. A dangling symlink is left out of the listing, and its target can appear without the
    /// directory changing.
    stable: bool,
}

fn list(inner: &dyn FS, path: &str) -> Listing {
    let fingerprint = fingerprint(path);
    let entries = inner.get_accessible_entries(path);
    let all = std::fs::read_dir(path).map_or(usize::MAX, Iterator::count);
    let stable = entries.symlinks.as_ref().is_some_and(rustc_hash::FxHashSet::is_empty)
        && all == entries.files.len() + entries.directories.len();
    Listing { fingerprint, entries, stable }
}

/// What a run asked the file system and what it answered, by kind of query and path.
#[derive(Default)]
pub struct Answers {
    file_exists: SyncMap<String, bool>,
    directory_exists: SyncMap<String, bool>,
    stats: SyncMap<String, Option<FileInfo>>,
    realpaths: SyncMap<String, String>,
    listings: SyncMap<String, Arc<Listing>>,
    reads: SyncMap<String, ReadAnswer>,
}

impl Answers {
    /// Queries of all kinds.
    pub fn len(&self) -> usize {
        self.file_exists.size()
            + self.directory_exists.size()
            + self.stats.size()
            + self.realpaths.size()
            + self.listings.size()
            + self.reads.size()
    }

    pub fn has_read(&self, path: &str) -> bool {
        self.reads.load(path).is_some()
    }
}

/// The text of the files read outside a source-file parse (configuration and `package.json` files), with the hash
/// of what was read, so that a rebuild serves an unchanged one without reading it again. Kept across requests.
pub type Contents = SyncMap<String, (u128, Arc<str>)>;

thread_local! {
    /// Set by the server's compiler host around a source-file parse: the stamp it took before the read, so that
    /// `read_file` neither stamps the file again nor keeps its text (the parsed file has it).
    static SOURCE_READ: Cell<Option<Fingerprint>> = const { Cell::new(None) };
}

/// Runs `parse` (a compiler host's `get_source_file`) as the parse of a source file stamped `fingerprint`.
pub fn source_read<T>(fingerprint: Fingerprint, parse: impl FnOnce() -> T) -> T {
    SOURCE_READ.set(Some(fingerprint));
    let result = parse();
    // An embedded library file is parsed without a read.
    SOURCE_READ.set(None);
    result
}

/// The file system of one request: the operating system's, behind the answers validated for this request (`known`),
/// recording what this run asks (`run`).
pub struct RecordFs {
    inner: &'static dyn FS,
    known: Arc<Answers>,
    run: Arc<Answers>,
    contents: Arc<Contents>,
}

impl RecordFs {
    pub fn new(known: Arc<Answers>, contents: Arc<Contents>) -> RecordFs {
        RecordFs { inner: tsrs_vfs::osvfs::fs(), known, run: Arc::default(), contents }
    }

    /// What this run asked, for the next request to validate (once the run is over: nothing asks this file system
    /// again, though the program's host keeps it until the program is freed).
    pub fn record(&self) -> Arc<Answers> {
        Arc::clone(&self.run)
    }

    /// The answer for a read of `path` that this run already has or that was validated for it, recorded for this run.
    pub fn current_read(&self, path: &str) -> Option<ReadAnswer> {
        if let Some(answer) = self.run.reads.load(path) {
            return Some(answer);
        }
        let answer = self.known.reads.load(path)?;
        self.run.reads.store(path.to_string(), answer);
        Some(answer)
    }

    fn ask<V: Clone>(
        &self,
        pick: impl Fn(&Answers) -> &SyncMap<String, V>,
        path: &str,
        query: impl FnOnce() -> V,
    ) -> V {
        if let Some(v) = pick(&self.run).load(path) {
            return v;
        }
        let v = pick(&self.known).load(path).unwrap_or_else(query);
        pick(&self.run).store(path.to_string(), v.clone());
        v
    }
}

impl FS for RecordFs {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.inner.use_case_sensitive_file_names()
    }

    fn file_exists(&self, path: &str) -> bool {
        self.ask(|a| &a.file_exists, path, || self.inner.file_exists(path))
    }

    fn read_file(&self, path: &str) -> Option<String> {
        if let Some(fingerprint) = SOURCE_READ.take() {
            let text = self.inner.read_file(path);
            let answer = ReadAnswer { fingerprint, hash: text.as_deref().map(hash) };
            self.run.reads.store(path.to_string(), answer);
            return text;
        }
        if let Some(answer) = self.current_read(path)
            && let Some(h) = answer.hash
            && let Some((kept, text)) = self.contents.load(path)
            && kept == h
        {
            return Some(text.to_string());
        }
        let fingerprint = fingerprint(path);
        let text = self.inner.read_file(path);
        let h = text.as_deref().map(hash);
        if let (Some(h), Some(text)) = (h, &text) {
            self.contents.store(path.to_string(), (h, Arc::from(text.as_str())));
        }
        self.run.reads.store(path.to_string(), ReadAnswer { fingerprint, hash: h });
        text
    }

    fn write_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.inner.write_file(path, data)
    }

    fn append_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.inner.append_file(path, data)
    }

    fn remove(&self, path: &str) -> Result<(), String> {
        self.inner.remove(path)
    }

    fn chtimes(&self, path: &str, a_time: SystemTime, m_time: SystemTime) -> Result<(), String> {
        self.inner.chtimes(path, a_time, m_time)
    }

    fn directory_exists(&self, path: &str) -> bool {
        self.ask(|a| &a.directory_exists, path, || self.inner.directory_exists(path))
    }

    fn get_accessible_entries(&self, path: &str) -> Entries {
        let listing = self.ask(|a| &a.listings, path, || Arc::new(list(self.inner, path)));
        listing.entries.clone()
    }

    fn stat(&self, path: &str) -> Option<FileInfo> {
        self.ask(|a| &a.stats, path, || self.inner.stat(path))
    }

    fn realpath(&self, path: &str) -> String {
        self.ask(|a| &a.realpaths, path, || self.inner.realpath(path))
    }
}

/// The outcome of `validate`: the answers as the file system gives them now, and the paths whose answers changed.
pub struct Validation {
    pub known: Answers,
    pub changed: Vec<String>,
}

enum Query {
    FileExists(String, bool),
    DirectoryExists(String, bool),
    Stat(String, Option<FileInfo>),
    Realpath(String, String),
    Listing(String, Arc<Listing>),
    Read(String, ReadAnswer),
}

fn collect<V: Clone>(
    map: &SyncMap<String, V>,
    make: impl Fn(String, V) -> Query,
    queries: &mut Vec<Query>,
) {
    map.range(|path, v| {
        queries.push(make(path.clone(), v.clone()));
        true
    });
}

/// Asks every query of `record` again, on the worker pool. Unchanged stamps vouch for reads and listings; a read
/// whose stamp changed (or was racy) is read again and compared by hash, so a file rewritten with the same text
/// (`touch`, a branch switch and back) counts as unchanged. `contents` gets the new text of the kept files.
pub fn validate(record: &Answers, contents: &Contents) -> Validation {
    let inner = tsrs_vfs::osvfs::fs();
    let mut queries = Vec::with_capacity(record.len());
    collect(&record.file_exists, Query::FileExists, &mut queries);
    collect(&record.directory_exists, Query::DirectoryExists, &mut queries);
    collect(&record.stats, Query::Stat, &mut queries);
    collect(&record.realpaths, Query::Realpath, &mut queries);
    collect(&record.listings, Query::Listing, &mut queries);
    collect(&record.reads, Query::Read, &mut queries);
    let known = Answers::default();
    let changed: Vec<String> = tsrs_compiler::worker_pool().install(|| {
        queries
            .into_par_iter()
            .filter_map(|query| {
                let (path, same) = match query {
                    Query::FileExists(path, old) => {
                        let now = inner.file_exists(&path);
                        known.file_exists.store(path.clone(), now);
                        (path, now == old)
                    }
                    Query::DirectoryExists(path, old) => {
                        let now = inner.directory_exists(&path);
                        known.directory_exists.store(path.clone(), now);
                        (path, now == old)
                    }
                    Query::Stat(path, old) => {
                        let now = inner.stat(&path);
                        let same = now == old;
                        known.stats.store(path.clone(), now);
                        (path, same)
                    }
                    Query::Realpath(path, old) => {
                        let now = inner.realpath(&path);
                        let same = now == old;
                        known.realpaths.store(path.clone(), now);
                        (path, same)
                    }
                    Query::Listing(path, old) => {
                        if old.stable && old.fingerprint.vouches_for(&fingerprint(&path)) {
                            known.listings.store(path.clone(), old);
                            (path, true)
                        } else {
                            let now = list(inner, &path);
                            let same = now.entries == old.entries;
                            known.listings.store(path.clone(), Arc::new(now));
                            (path, same)
                        }
                    }
                    Query::Read(path, old) => {
                        let now = fingerprint(&path);
                        if old.fingerprint.vouches_for(&now) && old.hash.is_some() {
                            known.reads.store(path.clone(), old);
                            (path, true)
                        } else {
                            let text = inner.read_file(&path);
                            let answer =
                                ReadAnswer { fingerprint: now, hash: text.as_deref().map(hash) };
                            if let (Some(h), Some(text)) = (answer.hash, &text)
                                && contents.load(&path).is_some()
                            {
                                contents.store(path.clone(), (h, Arc::from(text.as_str())));
                            }
                            known.reads.store(path.clone(), answer);
                            (path, answer.hash == old.hash)
                        }
                    }
                };
                (!same).then_some(path)
            })
            .collect()
    });
    let mut changed = changed;
    changed.sort();
    changed.dedup();
    Validation { known, changed }
}
