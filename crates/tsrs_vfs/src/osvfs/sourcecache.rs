//! tsrs-only, `--fast` command-line compilations: file text served from a persistent cache instead of an
//! open + read + close per file (notes/perf-source-cache.md).
//!
//! On macOS with an Endpoint Security client every `open` is authorized by it, and the system tops out near 110k opens
//! a second however many threads ask; `stat` and `pread` on an open descriptor are not intercepted. The cache keeps the
//! bytes of each file the previous run read, keyed by path and validated by the file's identity: device, inode, size,
//! and modification and status-change times in nanoseconds. A hit is one `stat` of the file and one `pread` from the
//! cache file, which the run opens once.
//!
//! - An entry is stored only when the file's modification and status-change times are more than `SETTLED` older than
//!   the moment its read started (git's "racy" rule, with the file system's timestamp granularity taken as unknown up
//!   to that margin). Any later write to the file then gives it a newer status-change time, so the identity no longer
//!   matches, even when the write keeps the size and restores the modification time (`touch -r`).
//! - One cache file per project, `<user cache dir>/tsrs/source-cache/<hash of the key>.bin` (never inside the
//!   project), written whole to a temporary file and renamed over the old one, so two runs at once each read one
//!   complete file. The header, index and paths carry one checksum and each entry's bytes another; a file that does
//!   not verify is ignored, and an entry that does not is read from disk.
//! - Written after the output, and only when the run stored files it read from disk for at least 1/32 of its reads
//!   (any, with nothing cached yet) or left a quarter of the cached entries unused; unused entries are dropped then.
//! - A cache directory over `DIRECTORY_LIMIT` loses its least recently written cache files.
//!
//! `TSRS_SOURCE_CACHE=0` turns it off; `TSRS_SOURCE_CACHE_DIR` moves it.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use xxhash_rust::xxh3::xxh3_64;

// The format version is the last byte.
const MAGIC: &[u8; 8] = b"tsrssrc1";
// magic, entry count u32, key length u32, paths length u32, zero u32, file length u64, checksum u64.
const HEADER_LEN: usize = 40;
// path offset u32, path length u32, data offset u64, then the identity (device, inode, size, mtime, ctime: u64 /
// i64) and the data's checksum u64.
const ENTRY_LEN: usize = 64;
// A file modified less than this before its read began is not stored: a write in the same timestamp tick could leave
// its identity unchanged.
const SETTLED: Duration = Duration::from_secs(2);
// The bytes one cache file stores at most; entries past it in path order are left out.
const CACHE_LIMIT: u64 = 1 << 30;
// The size of the cache directory past which the least recently written cache files are removed.
const DIRECTORY_LIMIT: u64 = 4 << 30;

static CACHE: OnceLock<SourceCache> = OnceLock::new();

/// Serves reads of the OS file system from the cache of the project named by `key`; returns whether it is on.
///
/// `key` is the config file, or the directory and root files of a run without one. The CLI calls this once, before
/// the program is built, in `--fast` compilations that neither watch nor build; the language server and the API never
/// do.
pub fn install(key: &str) -> bool {
    if std::env::var_os("TSRS_SOURCE_CACHE").is_some_and(|v| v == "0") {
        return false;
    }
    let dir = match std::env::var_os("TSRS_SOURCE_CACHE_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => match super::os::user_cache_dir() {
            Some(dir) => Path::new(&dir).join("tsrs/source-cache"),
            None => return false,
        },
    };
    let target = dir.join(format!("{:016x}.bin", xxh3_64(key.as_bytes())));
    let old = CacheFile::load(&target, key);
    CACHE
        .set(SourceCache {
            dir,
            target,
            key: key.to_string(),
            old,
            pending: Mutex::new(Vec::new()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
        .is_ok()
}

/// The `--extendedDiagnostics` rows: reads the cache served and reads it did not.
pub fn report_counts(enabled: bool) {
    if let Some(cache) = CACHE.get() {
        // Relaxed: the reads that counted are done (the program is built); only the totals are read.
        let hits = cache.hits.load(Ordering::Relaxed);
        // Relaxed: as above.
        let misses = cache.misses.load(Ordering::Relaxed);
        tsrs_core::phases::count(enabled, "Program:   source cache hits", hits);
        tsrs_core::phases::count(enabled, "Program:   source cache misses", misses);
    }
}

/// Writes the cache for the next run when this one changed enough of it. Call after the output is written.
pub fn finish() {
    if let Some(cache) = CACHE.get() {
        // A cache that cannot be written is only a slower next run.
        let _ = cache.finish();
    }
}

// The bytes of `full` (an OS path) as `fs::read` returns them, or None when the cache is off.
pub(crate) fn read(full: &str) -> Option<io::Result<Vec<u8>>> {
    CACHE.get().map(|cache| cache.read(full))
}

// What `stat` says about a file, the part that changes when the file does.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    size: u64,
    mtime: i64,
    ctime: i64,
}

impl Identity {
    fn of(md: &fs::Metadata) -> Option<Identity> {
        let ns = |sec: i64, nsec: i64| sec.checked_mul(1_000_000_000)?.checked_add(nsec);
        Some(Identity {
            dev: md.dev(),
            ino: md.ino(),
            size: md.size(),
            mtime: ns(md.mtime(), md.mtime_nsec())?,
            ctime: ns(md.ctime(), md.ctime_nsec())?,
        })
    }

    // Changed more than `SETTLED` before `start` (nanoseconds since the epoch, read before the file was opened). A file
    // system that keeps no status-change time is never trusted.
    fn settled(&self, start: i64) -> bool {
        let limit = start - SETTLED.as_nanos() as i64;
        self.ctime > 0 && self.mtime < limit && self.ctime < limit
    }

    fn encode(&self, out: &mut Vec<u8>) {
        for v in [self.dev, self.ino, self.size, self.mtime as u64, self.ctime as u64] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

struct Entry {
    path: std::ops::Range<usize>,
    data: u64,
    id: Identity,
    hash: u64,
}

// The previous run's cache file: its index in memory, its data read on demand from the descriptor (the file is
// replaced by rename, never written in place, so the descriptor keeps the bytes the index describes).
struct CacheFile {
    file: File,
    paths: Box<[u8]>,
    entries: Vec<Entry>,
    seen: Box<[AtomicBool]>,
}

impl CacheFile {
    // Layout, little-endian: header | key | entries | paths | data. The header's checksum covers key, entries and
    // paths; each entry's covers its data. Entries are sorted by path.
    fn load(path: &Path, key: &str) -> Option<CacheFile> {
        let file = File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        let mut header = [0u8; HEADER_LEN];
        file.read_exact_at(&mut header, 0).ok()?;
        if &header[..8] != MAGIC || u64_at(&header, 24) != len {
            return None;
        }
        let count = u32_at(&header, 8) as usize;
        let key_len = u32_at(&header, 12) as usize;
        let paths_len = u32_at(&header, 16) as usize;
        let index = key_len..key_len + count.checked_mul(ENTRY_LEN)?;
        let meta_len = index.end.checked_add(paths_len)?;
        let data_start = (HEADER_LEN + meta_len) as u64;
        if data_start > len {
            return None;
        }
        let mut meta = vec![0u8; meta_len];
        file.read_exact_at(&mut meta, HEADER_LEN as u64).ok()?;
        if xxh3_64(&meta) != u64_at(&header, 32) || &meta[..key_len] != key.as_bytes() {
            return None;
        }
        let paths: Box<[u8]> = meta[index.end..].into();
        let mut entries = Vec::with_capacity(count);
        for e in meta[index].as_chunks::<ENTRY_LEN>().0 {
            let start = u32_at(e, 0) as usize;
            let path = start..start + u32_at(e, 4) as usize;
            let entry = Entry {
                path,
                data: u64_at(e, 8),
                id: Identity {
                    dev: u64_at(e, 16),
                    ino: u64_at(e, 24),
                    size: u64_at(e, 32),
                    mtime: u64_at(e, 40) as i64,
                    ctime: u64_at(e, 48) as i64,
                },
                hash: u64_at(e, 56),
            };
            let in_bounds = entry.path.end <= paths.len()
                && entry.data >= data_start
                && entry.data.checked_add(entry.id.size).is_some_and(|end| end <= len);
            let sorted = entries
                .last()
                .is_none_or(|prev: &Entry| paths[prev.path.clone()] < paths[entry.path.clone()]);
            if !in_bounds || !sorted {
                return None;
            }
            entries.push(entry);
        }
        let seen = entries.iter().map(|_| AtomicBool::new(false)).collect();
        Some(CacheFile { file, paths, entries, seen })
    }

    fn find(&self, path: &str) -> Option<usize> {
        self.entries.binary_search_by(|e| self.paths[e.path.clone()].cmp(path.as_bytes())).ok()
    }

    // The entry's bytes, when they are what was stored.
    fn data(&self, entry: &Entry) -> Option<Vec<u8>> {
        let mut bytes = vec![0u8; usize::try_from(entry.id.size).ok()?];
        self.file.read_exact_at(&mut bytes, entry.data).ok()?;
        (xxh3_64(&bytes) == entry.hash).then_some(bytes)
    }
}

// A file this run read from disk that may be stored.
struct Pending {
    path: String,
    id: Identity,
    bytes: Vec<u8>,
}

struct SourceCache {
    dir: PathBuf,
    target: PathBuf,
    key: String,
    old: Option<CacheFile>,
    pending: Mutex<Vec<Pending>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl SourceCache {
    fn read(&self, full: &str) -> io::Result<Vec<u8>> {
        if let Some(old) = &self.old
            && let Some(index) = old.find(full)
            && let Ok(md) = fs::metadata(full)
            && md.is_file()
            && Identity::of(&md) == Some(old.entries[index].id)
            && let Some(bytes) = old.data(&old.entries[index])
        {
            // Relaxed: flags and counters read only after the reads are done (`report_counts`, `finish`).
            old.seen[index].store(true, Ordering::Relaxed);
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(bytes);
        }
        // Relaxed: as above.
        self.misses.fetch_add(1, Ordering::Relaxed);
        // `fs::read`, keeping the opened file's identity.
        let start = now_ns();
        let mut file = File::open(full)?;
        let md = file.metadata()?;
        let mut bytes = Vec::with_capacity(usize::try_from(md.len()).unwrap_or(0));
        file.read_to_end(&mut bytes)?;
        if md.is_file()
            && let Some(id) = Identity::of(&md)
            && id.size == bytes.len() as u64
            && id.settled(start)
        {
            self.pending.lock().unwrap().push(Pending {
                path: full.to_string(),
                id,
                bytes: bytes.clone(),
            });
        }
        Ok(bytes)
    }

    fn finish(&self) -> io::Result<()> {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        // Relaxed: the reads are done.
        let reads = self.hits.load(Ordering::Relaxed) + self.misses.load(Ordering::Relaxed);
        let rewrite = match &self.old {
            None => !pending.is_empty(),
            Some(old) => {
                // Relaxed: as above.
                let unseen = old.seen.iter().filter(|s| !s.load(Ordering::Relaxed)).count();
                pending.len() as u64 * 32 >= reads.max(1) || unseen * 4 > old.entries.len()
            }
        };
        if !rewrite {
            return Ok(());
        }
        self.write(&pending)?;
        self.evict();
        Ok(())
    }

    fn write(&self, pending: &[Pending]) -> io::Result<()> {
        enum Source<'a> {
            Old(&'a Entry),
            New(&'a Pending),
        }
        // Every entry the run read, in path order; a file read from disk wins over a cached copy of the same path
        // (it was read twice and changed in between).
        let mut kept: Vec<(&[u8], Identity, Source<'_>)> =
            pending.iter().map(|p| (p.path.as_bytes(), p.id, Source::New(p))).collect();
        if let Some(old) = &self.old {
            kept.extend(
                old.entries
                    .iter()
                    .zip(&old.seen)
                    // Relaxed: the reads are done.
                    .filter(|(_, seen)| seen.load(Ordering::Relaxed))
                    .map(|(e, _)| (&old.paths[e.path.clone()], e.id, Source::Old(e))),
            );
        }
        kept.sort_by(|a, b| a.0.cmp(b.0));
        kept.dedup_by(|later, earlier| later.0 == earlier.0);
        let mut total = 0u64;
        kept.retain(|(_, id, _)| {
            total += id.size;
            total <= CACHE_LIMIT
        });

        let key = self.key.as_bytes();
        let paths_len: usize = kept.iter().map(|k| k.0.len()).sum();
        let mut meta = Vec::with_capacity(key.len() + kept.len() * ENTRY_LEN + paths_len);
        meta.extend_from_slice(key);
        let mut path_off = 0u32;
        let mut data_off = (HEADER_LEN + key.len() + kept.len() * ENTRY_LEN + paths_len) as u64;
        for (path, id, source) in &kept {
            let hash = match source {
                Source::Old(e) => e.hash,
                Source::New(p) => xxh3_64(&p.bytes),
            };
            meta.extend_from_slice(&path_off.to_le_bytes());
            meta.extend_from_slice(&(path.len() as u32).to_le_bytes());
            meta.extend_from_slice(&data_off.to_le_bytes());
            id.encode(&mut meta);
            meta.extend_from_slice(&hash.to_le_bytes());
            path_off += path.len() as u32;
            data_off += id.size;
        }
        for (path, _, _) in &kept {
            meta.extend_from_slice(path);
        }
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        for v in [kept.len(), key.len(), paths_len, 0] {
            header.extend_from_slice(&(v as u32).to_le_bytes());
        }
        header.extend_from_slice(&data_off.to_le_bytes());
        header.extend_from_slice(&xxh3_64(&meta).to_le_bytes());

        fs::create_dir_all(&self.dir)?;
        let temp = self.target.with_extension(format!("{}.tmp", std::process::id()));
        let written = (|| {
            let mut out = BufWriter::with_capacity(1 << 20, File::create(&temp)?);
            out.write_all(&header)?;
            out.write_all(&meta)?;
            let mut buffer = Vec::new();
            for (_, id, source) in &kept {
                match source {
                    Source::New(p) => out.write_all(&p.bytes)?,
                    Source::Old(e) => {
                        buffer.resize(id.size as usize, 0);
                        self.old.as_ref().unwrap().file.read_exact_at(&mut buffer, e.data)?;
                        out.write_all(&buffer)?;
                    }
                }
            }
            out.into_inner().map_err(io::IntoInnerError::into_error)?;
            fs::rename(&temp, &self.target)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written
    }

    // Keeps the directory under `DIRECTORY_LIMIT`: removes the least recently written cache files, and temporary
    // files a run that stopped while writing left behind an hour ago or more.
    fn evict(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        let now = SystemTime::now();
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(md) = entry.metadata() else { continue };
            let modified = md.modified().unwrap_or(now);
            match path.extension().and_then(|e| e.to_str()) {
                Some("bin") => files.push((modified, md.len(), path)),
                Some("tmp")
                    if now.duration_since(modified).is_ok_and(|age| age.as_secs() >= 3600) =>
                {
                    let _ = fs::remove_file(&path);
                }
                _ => {}
            }
        }
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
        for (_, len, path) in files {
            if total <= DIRECTORY_LIMIT {
                break;
            }
            if path != self.target && fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}
