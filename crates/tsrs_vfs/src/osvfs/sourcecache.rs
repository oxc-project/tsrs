//! tsrs-only, `--fast` command-line compilations: file text served from a persistent cache instead of an
//! open + read + close per file (notes/perf-source-cache.md).
//!
//! On macOS with an Endpoint Security client every `open` is authorized by it, and the system tops out near 110k opens
//! a second however many threads ask, while `stat` scales. The cache keeps the bytes of each file the previous run
//! read, keyed by path and validated by the file's identity: device, inode, size, and modification and status-change
//! times in nanoseconds. A hit is one `stat` of the file and a copy out of the cache file, which the run maps once
//! (`pread` from many threads serializes on the file's vnode lock on macOS).
//!
//! - An entry is stored only when the file's modification and status-change times are more than `SETTLED` older than
//!   the moment its read started (git's "racy" rule, with the file system's timestamp granularity taken as unknown up
//!   to that margin). Any later write to the file then gives it a newer status-change time, so the identity no longer
//!   matches, even when the write keeps the size and restores the modification time (`touch -r`).
//! - One cache file per project, `<user cache dir>/tsrs/source-cache/<hash of the key>.bin` (never inside the
//!   project), written whole to a temporary file and renamed over the old one, never written in place: two runs at
//!   once each map one complete file. The header, index and paths carry one checksum and each entry's bytes another; a
//!   file that does not verify is ignored, and an entry that does not is read from disk.
//! - Rewritten after the output, and only when the run read at least 1/32 of its files from disk (any, with nothing
//!   cached yet) or left a quarter of the cached entries unused; unused entries are dropped then. Otherwise the mapping
//!   is released once the program is built, so the check pass does not keep its pages.
//! - A cache directory over `DIRECTORY_LIMIT` loses its least recently written cache files.
//!
//! `TSRS_SOURCE_CACHE=0` turns it off; `TSRS_SOURCE_CACHE_DIR` moves it.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
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
            old: RwLock::new(old),
            pending: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
        .is_ok()
}

/// Ends the cached reads once the program is built; later reads go to disk.
///
/// Records the `--extendedDiagnostics` rows (reads the cache served and reads it did not) and, unless the cache is to
/// be rewritten, releases it.
pub fn end_reads(diagnostics: bool) {
    let Some(cache) = CACHE.get() else { return };
    cache.closed.store(true, Ordering::SeqCst);
    // Relaxed: the reads that counted are done (the program is built); only the totals are read.
    let hits = cache.hits.load(Ordering::Relaxed);
    // Relaxed: as above.
    let misses = cache.misses.load(Ordering::Relaxed);
    tsrs_core::phases::count(diagnostics, "Program:   source cache hits", hits);
    tsrs_core::phases::count(diagnostics, "Program:   source cache misses", misses);
    if !cache.wants_rewrite() {
        *cache.old.write().unwrap() = None;
        cache.pending.lock().unwrap().clear();
    }
}

/// Writes the cache for the next run when this one changed enough of it. Call after the output is written.
pub fn finish() {
    let Some(cache) = CACHE.get() else { return };
    cache.closed.store(true, Ordering::SeqCst);
    if cache.wants_rewrite() {
        // A cache that cannot be written only makes the next run slower.
        if cache.write().is_ok() {
            cache.evict();
        }
    }
}

// The bytes of `full` (an OS path) as `fs::read` returns them, or None when the cache is off.
pub(crate) fn read(full: &str) -> Option<io::Result<Vec<u8>>> {
    // Relaxed: set once by the thread that built the program, after the reads that use the cache.
    CACHE.get().filter(|cache| !cache.closed.load(Ordering::Relaxed)).map(|cache| cache.read(full))
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

// A private read-only mapping of a whole cache file.
struct Mapping {
    ptr: NonNull<u8>,
    len: usize,
}

// SAFETY: read-only memory, shared between threads as a `&[u8]` is.
unsafe impl Send for Mapping {}
// SAFETY: as for Send.
unsafe impl Sync for Mapping {}

impl Mapping {
    fn new(file: &File, len: usize) -> Option<Mapping> {
        // SAFETY: maps `len` bytes of an open file for reading at an address the system picks; checked below.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        Some(Mapping { ptr: NonNull::new(ptr.cast())?, len })
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: `ptr .. ptr + len` stays mapped and readable until `drop`. Cache files are replaced by rename and
        // never written in place, so the bytes do not change while mapped (a program that truncated one by hand
        // would make reads of the lost pages fault).
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: the mapping `new` made; no slice of it outlives `self`.
        unsafe { libc::munmap(self.ptr.as_ptr().cast(), self.len) };
    }
}

struct Entry {
    path: std::ops::Range<usize>,
    data: std::ops::Range<usize>,
    id: Identity,
    hash: u64,
}

// The previous run's cache file, mapped, with its index decoded.
struct CacheFile {
    map: Mapping,
    entries: Vec<Entry>,
    seen: Box<[AtomicBool]>,
}

impl CacheFile {
    // Layout, little-endian: header | key | entries | paths | data. The header's checksum covers key, entries and
    // paths; each entry's covers its data. Entries are sorted by path.
    fn load(path: &Path, key: &str) -> Option<CacheFile> {
        let file = File::open(path).ok()?;
        let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
        if len < HEADER_LEN {
            return None;
        }
        let map = Mapping::new(&file, len)?;
        let bytes = map.bytes();
        if &bytes[..8] != MAGIC || u64_at(bytes, 24) != len as u64 {
            return None;
        }
        let count = u32_at(bytes, 8) as usize;
        let key_len = u32_at(bytes, 12) as usize;
        let paths_len = u32_at(bytes, 16) as usize;
        let index = HEADER_LEN + key_len..HEADER_LEN + key_len + count.checked_mul(ENTRY_LEN)?;
        let paths = index.end..index.end.checked_add(paths_len)?;
        let meta = bytes.get(HEADER_LEN..paths.end)?;
        if xxh3_64(meta) != u64_at(bytes, 32) || &meta[..key_len] != key.as_bytes() {
            return None;
        }
        let mut entries: Vec<Entry> = Vec::with_capacity(count);
        for e in bytes[index].as_chunks::<ENTRY_LEN>().0 {
            let start = paths.start + u32_at(e, 0) as usize;
            let data = usize::try_from(u64_at(e, 8)).ok()?;
            let id = Identity {
                dev: u64_at(e, 16),
                ino: u64_at(e, 24),
                size: u64_at(e, 32),
                mtime: u64_at(e, 40) as i64,
                ctime: u64_at(e, 48) as i64,
            };
            let entry = Entry {
                path: start..start + u32_at(e, 4) as usize,
                data: data..data.checked_add(usize::try_from(id.size).ok()?)?,
                id,
                hash: u64_at(e, 56),
            };
            let in_bounds = entry.path.end <= paths.end
                && entry.data.start >= paths.end
                && entry.data.end <= len;
            let sorted = entries
                .last()
                .is_none_or(|prev| bytes[prev.path.clone()] < bytes[entry.path.clone()]);
            if !in_bounds || !sorted {
                return None;
            }
            entries.push(entry);
        }
        let seen = entries.iter().map(|_| AtomicBool::new(false)).collect();
        Some(CacheFile { map, entries, seen })
    }

    fn path(&self, entry: &Entry) -> &[u8] {
        &self.map.bytes()[entry.path.clone()]
    }

    fn find(&self, full: &str) -> Option<usize> {
        self.entries.binary_search_by(|e| self.path(e).cmp(full.as_bytes())).ok()
    }

    // The bytes cached at `index` if `md` (the file's stat now) still describes them.
    fn get(&self, index: usize, md: &fs::Metadata) -> Option<Vec<u8>> {
        let entry = &self.entries[index];
        if !md.is_file() || Identity::of(md) != Some(entry.id) {
            return None;
        }
        let bytes = &self.map.bytes()[entry.data.clone()];
        if xxh3_64(bytes) != entry.hash {
            return None;
        }
        // Relaxed: read after the reads are done (`wants_rewrite`, `write`).
        self.seen[index].store(true, Ordering::Relaxed);
        Some(bytes.to_vec())
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
    // None once released (`end_reads`).
    old: RwLock<Option<CacheFile>>,
    pending: Mutex<Vec<Pending>>,
    closed: AtomicBool,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl SourceCache {
    fn read(&self, full: &str) -> io::Result<Vec<u8>> {
        {
            let old = self.old.read().unwrap();
            if let Some(old) = &*old
                && let Some(index) = old.find(full)
                && let Ok(md) = fs::metadata(full)
                && let Some(bytes) = old.get(index, &md)
            {
                // Relaxed: counters read after the reads are done.
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(bytes);
            }
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

    fn wants_rewrite(&self) -> bool {
        let pending = self.pending.lock().unwrap().len() as u64;
        // Relaxed: the reads are done.
        let reads = self.hits.load(Ordering::Relaxed) + self.misses.load(Ordering::Relaxed);
        match &*self.old.read().unwrap() {
            None => pending > 0,
            Some(old) => {
                // Relaxed: as above.
                let unseen = old.seen.iter().filter(|s| !s.load(Ordering::Relaxed)).count();
                pending * 32 >= reads.max(1) || unseen * 4 > old.entries.len()
            }
        }
    }

    fn write(&self) -> io::Result<()> {
        enum Source<'a> {
            Old(&'a Entry),
            New(&'a Pending),
        }
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        let old = self.old.read().unwrap();
        // Every file the run read that may be stored, in path order; one read from disk wins over a cached copy of
        // the same path (it was read twice and changed in between).
        let mut kept: Vec<(&[u8], Identity, Source<'_>)> =
            pending.iter().map(|p| (p.path.as_bytes(), p.id, Source::New(p))).collect();
        if let Some(old) = &*old {
            kept.extend(
                old.entries
                    .iter()
                    .zip(&old.seen)
                    // Relaxed: the reads are done.
                    .filter(|(_, seen)| seen.load(Ordering::Relaxed))
                    .map(|(e, _)| (old.path(e), e.id, Source::Old(e))),
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
            for (_, _, source) in &kept {
                out.write_all(match source {
                    Source::New(p) => &p.bytes,
                    Source::Old(e) => &old.as_ref().unwrap().map.bytes()[e.data.clone()],
                })?;
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
    // files that a run stopped while writing left an hour ago or more.
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
