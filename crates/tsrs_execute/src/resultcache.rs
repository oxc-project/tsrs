// tsrs-only: the `--fast` result cache (notes/fast-result-cache.md).
//
// A `--fast --noEmit` command-line check records every file system observation that can influence its output (each
// answer the OS file system gave, beneath the bundled-library layer) and the bytes it wrote to stdout. The next run
// with the same command line, working directory, binary and relevant environment asks the same questions again
// (stats, directory listings, realpaths; file contents only for files changed too recently to trust their time
// stamps) and, when every answer matches, writes the recorded output and exits with the recorded status without
// building a program. The compiler is a function of those answers, so the same answers give the same output. Any
// mismatch, unreadable entry or run that does not qualify runs normally, and a qualifying run records again.
//
// `TSRS_RESULT_CACHE=0` turns it off; `TSRS_RESULT_CACHE=log` prints one line to stderr saying what it did.
// `TSRS_RESULT_CACHE_DIR` moves the entries out of the user cache directory.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use tsrs_core::CompilerOptions;
use tsrs_core::collections::SyncMap;
use tsrs_core::tspath;
use tsrs_vfs::{Entries, FS, FileInfo, bundled, osvfs};
use xxhash_rust::xxh3::{Xxh3Default, xxh3_128};

use crate::execute;
use crate::sys::OsSys;
use crate::tsc::{CommandLineResult, ExitStatus, ReadWriteCloser, System};

/// Changes the entry format; entries of another version never match.
const MAGIC: &[u8; 8] = b"tsrsrc01";
/// A file whose modification or change time is this close to the start of the recording run (or later) may change
/// again within the same time stamp tick, so its content is hashed instead of trusting its time stamps (git's "racy
/// git" rule). Two seconds covers the coarsest common granularity (FAT); APFS, ext4 and xfs tick in nanoseconds or
/// kernel ticks.
const RACY_WINDOW: Duration = Duration::from_secs(2);
const MAX_ENTRIES: usize = 64;
const MAX_BYTES: u64 = 256 << 20;
/// `TSRS_*` variables that may be set without turning the cache off. Every other one is an experiment or a
/// measurement switch (statistics on stderr, files written outside the VFS, alternative algorithms): a run with one
/// set is a run someone wants to happen.
const ALLOWED_TSRS_ENV: &[&str] =
    &["TSRS_FAST", "TSRS_LIB_PATH", "TSRS_RESULT_CACHE", "TSRS_RESULT_CACHE_DIR"];
/// Environment variables that change a check's output (fast mode, the library directory, color) or its schedule.
const KEYED_ENV: &[&str] =
    &["TSRS_FAST", "TSRS_LIB_PATH", "FORCE_COLOR", "NO_COLOR", "RAYON_NUM_THREADS"];

/// Set by `perform_compilation` when its program may be recorded.
static RECORDABLE: AtomicBool = AtomicBool::new(false);

/// Called by `perform_compilation` (the only compilation a recorded run may reach): whether this check's output can
/// be replayed. Not with timings in the output, a trace or profile request, content mappers (external processes) or
/// emitted files.
pub(crate) fn note_compilation(options: &CompilerOptions, testing: bool) {
    let recordable = !testing
        && tsrs_core::fast::enabled()
        && options.no_emit.is_true()
        && !options.diagnostics_enabled()
        && options.generate_trace.is_empty()
        && options.generate_cpu_profile.is_empty()
        && options.pprof_dir.is_empty()
        && !options.run_external_code.is_true();
    // Relaxed: stored and loaded on the command line's own thread.
    RECORDABLE.store(recordable, Ordering::Relaxed);
}

/// The `tsrs` command line over the OS: replays a recorded result when one applies, else runs `execute::command_line`
/// (recording it when it qualifies).
pub fn command_line(sys: &'static OsSys, args: Vec<String>) -> CommandLineResult {
    let log = match std::env::var("TSRS_RESULT_CACHE").as_deref() {
        Ok("0") => return execute::command_line(sys, args),
        Ok("log") => true,
        _ => false,
    };
    let session = match Session::open(sys, &args) {
        Ok(session) => session,
        Err(reason) => {
            if log && !reason.is_empty() {
                eprintln!("tsrs result cache: off ({reason})");
            }
            return execute::command_line(sys, args);
        }
    };
    session.run(sys, args, log)
}

struct Session {
    dir: PathBuf,
    key: u128,
}

impl Session {
    /// The cache directory and the key of this invocation, or why the cache does not apply (empty when the command
    /// is not a fast-mode check at all).
    fn open(sys: &OsSys, args: &[String]) -> Result<Session, &'static str> {
        if !cfg!(unix) {
            return Err("");
        }
        let fast = args.iter().any(|a| a.eq_ignore_ascii_case("--fast"))
            || std::env::var("TSRS_FAST").as_deref() == Ok("1");
        if !fast {
            return Err("");
        }
        // `--build` and `--watch` never reach a recordable compilation; the rest read or write files outside the VFS
        // or hide arguments from this check.
        if args.first().is_some_and(|a| {
            matches!(a.to_ascii_lowercase().as_str(), "-b" | "--b" | "-build" | "--build")
        }) {
            return Err("--build");
        }
        for (i, arg) in args.iter().enumerate() {
            match arg.to_ascii_lowercase().as_str() {
                "-w" | "--watch" => return Err("--watch"),
                "--checkercostcache" => return Err("--checkerCostCache"),
                "--checkerassignment" if args.get(i + 1).is_some_and(|v| v.starts_with("file:")) => {
                    return Err("--checkerAssignment file:");
                }
                _ if arg.starts_with('@') => return Err("response file"),
                _ => {}
            }
        }
        for (name, _) in std::env::vars_os() {
            if let Some(name) = name.to_str()
                && name.starts_with("TSRS_")
                && !ALLOWED_TSRS_ENV.contains(&name)
            {
                return Err("a TSRS_ environment variable is set");
            }
        }
        let dir = cache_dir().ok_or("no cache directory")?;
        let key = invocation_key(sys, args).ok_or("cannot identify the executable")?;
        Ok(Session { dir, key })
    }

    fn entry_path(&self) -> PathBuf {
        self.dir.join(format!("{:032x}", self.key))
    }

    fn run(self, sys: &'static OsSys, args: Vec<String>, log: bool) -> CommandLineResult {
        let start = Instant::now();
        if let Ok(bytes) = std::fs::read(self.entry_path()) {
            match Entry::decode(&bytes, self.key) {
                None => {
                    if log {
                        eprintln!("tsrs result cache: miss (unreadable entry)");
                    }
                }
                Some(entry) => match validate(&entry.records) {
                    Ok(()) => {
                        sys.write(entry.output);
                        if log {
                            eprintln!(
                                "tsrs result cache: hit ({} observations checked in {:.1} ms)",
                                entry.records.len(),
                                start.elapsed().as_secs_f64() * 1e3
                            );
                        }
                        return CommandLineResult { watcher: None, status: entry.status };
                    }
                    Err(changed) => {
                        if log {
                            let record = &entry.records[changed];
                            eprintln!(
                                "tsrs result cache: miss ({} {} changed)",
                                record.kind.name(),
                                record.path
                            );
                        }
                    }
                },
            }
        }

        let recorder: &'static Recorder = Box::leak(Box::new(Recorder::new(osvfs::fs())));
        let recording: &'static RecordingSys = Box::leak(Box::new(RecordingSys {
            os: sys,
            fs: Arc::new(bundled::wrap_fs(recorder)),
            output: Mutex::new(String::new()),
            recorder,
        }));
        let result = execute::command_line(recording, args);
        // The user sees the output before the entry is written.
        sys.flush();
        // Relaxed: stored by `note_compilation` on this thread.
        let recordable = RECORDABLE.load(Ordering::Relaxed);
        let reason = if !recordable {
            "not a --fast --noEmit check without timings"
        } else if recorder.spoiled.load(Ordering::Relaxed) {
            // Relaxed: the compilation's threads have joined.
            "a file was written or changed during the run"
        } else if result.status as i32 > ExitStatus::DiagnosticsPresentOutputsGenerated as i32 {
            "unexpected exit status"
        } else {
            ""
        };
        if !reason.is_empty() {
            if log {
                eprintln!("tsrs result cache: not recorded ({reason})");
            }
            return result;
        }
        let records = recorder.records();
        let output = recording.output.lock().unwrap();
        let bytes = Entry::encode(self.key, result.status, &output, &records);
        let saved = save(&self.dir, &self.entry_path(), &bytes);
        if log {
            let hashed = records.iter().filter(|r| r.kind == Kind::ReadContent).count();
            match saved {
                Ok(()) => eprintln!(
                    "tsrs result cache: recorded {} observations ({hashed} by content), {} bytes",
                    records.len(),
                    bytes.len()
                ),
                Err(err) => eprintln!("tsrs result cache: not recorded ({err})"),
            }
        }
        result
    }
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("TSRS_RESULT_CACHE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    Some(Path::new(&osvfs::user_cache_dir()?).join("tsrs").join("result-cache"))
}

/// Everything outside the file system that the output depends on: the binary (its version and the identity of the
/// executable file, which carries the embedded libraries), the arguments, the working directory, the environment
/// variables the compiler reads, whether stdout is a terminal (the default of `--pretty`), the parallelism that sets
/// the default checker count and the file system's case sensitivity.
fn invocation_key(sys: &OsSys, args: &[String]) -> Option<u128> {
    let mut h = Xxh3Default::new();
    h.update(MAGIC);
    add_str(&mut h, env!("TSRS_RELEASE_VERSION"));
    let exe = std::env::current_exe().ok()?;
    add_str(&mut h, &exe.to_string_lossy());
    h.update(&fingerprint_of(&exe)?.digest.to_le_bytes());
    add_str(&mut h, sys.get_current_directory());
    h.update(&args.len().to_le_bytes());
    for arg in args {
        add_str(&mut h, arg);
    }
    for name in KEYED_ENV {
        add_opt_str(&mut h, std::env::var(name).ok().as_deref());
    }
    let term_dumb = std::env::var("TERM").as_deref() == Ok("dumb");
    let parallelism = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    h.update(&[
        u8::from(term_dumb),
        u8::from(sys.write_output_is_tty()),
        u8::from(osvfs::fs().use_case_sensitive_file_names()),
    ]);
    h.update(&parallelism.to_le_bytes());
    Some(h.digest128())
}

fn add_str(h: &mut Xxh3Default, s: &str) {
    h.update(&s.len().to_le_bytes());
    h.update(s.as_bytes());
}

fn add_opt_str(h: &mut Xxh3Default, s: Option<&str>) {
    match s {
        None => h.update(&[0]),
        Some(s) => {
            h.update(&[1]);
            add_str(h, s);
        }
    }
}

// What the compiler observed, as the kind of question and a digest of the answer.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Kind {
    FileExists,
    DirectoryExists,
    Stat,
    Entries,
    Realpath,
    /// A file read whose content is vouched for by its device, inode, size, mode and modification and change times.
    ReadUnchanged,
    /// A file read that failed or whose time stamps are too recent to trust: the digest of the text.
    ReadContent,
}

const KINDS: [Kind; 7] = [
    Kind::FileExists,
    Kind::DirectoryExists,
    Kind::Stat,
    Kind::Entries,
    Kind::Realpath,
    Kind::ReadUnchanged,
    Kind::ReadContent,
];

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::FileExists => "file exists",
            Kind::DirectoryExists => "directory exists",
            Kind::Stat => "stat",
            Kind::Entries => "directory listing",
            Kind::Realpath => "realpath",
            Kind::ReadUnchanged | Kind::ReadContent => "file",
        }
    }
}

fn digest_bool(b: bool) -> u128 {
    u128::from(b)
}

fn digest_text(text: Option<&str>) -> u128 {
    match text {
        None => 0,
        // Never 0: `xxh3_128` of the empty text is not 0, and a collision with it is a 2^-128 event.
        Some(text) => xxh3_128(text.as_bytes()),
    }
}

fn digest_info(info: Option<&FileInfo>) -> u128 {
    let mut h = Xxh3Default::new();
    let Some(info) = info else {
        return 0;
    };
    add_str(&mut h, &info.name);
    h.update(&info.size.to_le_bytes());
    h.update(&info.mode.bits().to_le_bytes());
    let mod_time =
        info.mod_time.map(|t| t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.update(&mod_time.unwrap_or(u128::MAX).to_le_bytes());
    h.digest128()
}

fn digest_entries(entries: &Entries) -> u128 {
    let mut h = Xxh3Default::new();
    for list in [&entries.files, &entries.directories] {
        h.update(&list.len().to_le_bytes());
        for name in list {
            add_str(&mut h, name);
        }
    }
    match &entries.symlinks {
        None => h.update(&[0]),
        Some(symlinks) => {
            let mut names: Vec<&String> = symlinks.iter().collect();
            names.sort_unstable();
            h.update(&names.len().to_le_bytes());
            for name in names {
                add_str(&mut h, name);
            }
        }
    }
    h.digest128()
}

struct Fingerprint {
    digest: u128,
    /// The later of the modification and change times, in nanoseconds since the epoch.
    changed: i128,
}

/// The fingerprint of the file the OS file system opens for `path` (the same normalization `osvfs` applies).
fn fingerprint(path: &str) -> Option<Fingerprint> {
    let normalized = tspath::normalize_path_cow(path);
    let root = tspath::get_root_length(&normalized);
    let rest = tspath::remove_trailing_directory_separator(&normalized[root..]);
    if root == 0 || rest.is_empty() {
        return None;
    }
    fingerprint_of(Path::new(&format!("{}{rest}", &normalized[..root])))
}

#[cfg(unix)]
fn fingerprint_of(path: &Path) -> Option<Fingerprint> {
    use std::os::unix::fs::MetadataExt;
    let md = std::fs::metadata(path).ok()?;
    let mtime = i128::from(md.mtime()) * 1_000_000_000 + i128::from(md.mtime_nsec());
    let ctime = i128::from(md.ctime()) * 1_000_000_000 + i128::from(md.ctime_nsec());
    let mut h = Xxh3Default::new();
    for v in [md.dev(), md.ino(), md.size(), u64::from(md.mode())] {
        h.update(&v.to_le_bytes());
    }
    h.update(&mtime.to_le_bytes());
    h.update(&ctime.to_le_bytes());
    Some(Fingerprint { digest: h.digest128(), changed: mtime.max(ctime) })
}

// Without a change time a fingerprint cannot see a content edit that restores the modification time: every read is
// hashed. (`Session::open` also keeps the cache off there.)
#[cfg(not(unix))]
fn fingerprint_of(_path: &Path) -> Option<Fingerprint> {
    None
}

/// The answer `fs` gives now, digested the way it was recorded.
fn answer(fs: &dyn FS, kind: Kind, path: &str) -> Option<u128> {
    Some(match kind {
        Kind::FileExists => digest_bool(fs.file_exists(path)),
        Kind::DirectoryExists => digest_bool(fs.directory_exists(path)),
        Kind::Stat => digest_info(fs.stat(path).as_ref()),
        Kind::Entries => digest_entries(&fs.get_accessible_entries(path)),
        Kind::Realpath => xxh3_128(fs.realpath(path).as_bytes()),
        Kind::ReadUnchanged => fingerprint(path)?.digest,
        Kind::ReadContent => digest_text(fs.read_file(path).as_deref()),
    })
}

/// The OS file system, recording each distinct question and its answer. A question asked twice with different
/// answers (a file changed during the run) or any write spoils the recording.
struct Recorder {
    inner: &'static dyn FS,
    racy_after: i128,
    seen: [SyncMap<String, u128>; KINDS.len()],
    spoiled: AtomicBool,
}

impl Recorder {
    fn new(inner: &'static dyn FS) -> Recorder {
        let start = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_sub(RACY_WINDOW);
        Recorder {
            inner,
            racy_after: start.as_nanos() as i128,
            seen: std::array::from_fn(|_| SyncMap::default()),
            spoiled: AtomicBool::new(false),
        }
    }

    fn observe(&self, kind: Kind, path: &str, digest: u128) {
        let seen = &self.seen[kind as usize];
        let first = match seen.load(path) {
            Some(first) => first,
            None => seen.load_or_store(path.to_string(), digest).0,
        };
        if first != digest {
            self.spoil();
        }
    }

    fn spoil(&self) {
        // Relaxed: read after the compilation's threads have joined.
        self.spoiled.store(true, Ordering::Relaxed);
    }

    /// The recorded questions, project files before `node_modules` (an edit is found sooner), each group by path.
    fn records(&self) -> Vec<Record<'static>> {
        let (mut project, mut dependencies) = (Vec::new(), Vec::new());
        for kind in KINDS {
            self.seen[kind as usize].range(|path, &digest| {
                let group =
                    if path.contains("/node_modules/") { &mut dependencies } else { &mut project };
                group.push(Record { kind, path: Cow::Owned(path.clone()), digest });
                true
            });
        }
        for group in [&mut project, &mut dependencies] {
            group.sort_unstable_by(|a, b| (&a.path, a.kind as u8).cmp(&(&b.path, b.kind as u8)));
        }
        project.append(&mut dependencies);
        project
    }
}

impl FS for Recorder {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.inner.use_case_sensitive_file_names()
    }

    fn file_exists(&self, path: &str) -> bool {
        let exists = self.inner.file_exists(path);
        self.observe(Kind::FileExists, path, digest_bool(exists));
        exists
    }

    fn read_file(&self, path: &str) -> Option<String> {
        // Stat first: an edit after the stat changes the change time, so the next run cannot match.
        let before = fingerprint(path);
        let text = self.inner.read_file(path);
        match (&text, before) {
            (Some(_), Some(before)) if before.changed < self.racy_after => {
                self.observe(Kind::ReadUnchanged, path, before.digest);
            }
            _ => self.observe(Kind::ReadContent, path, digest_text(text.as_deref())),
        }
        text
    }

    fn write_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.spoil();
        self.inner.write_file(path, data)
    }

    fn append_file(&self, path: &str, data: &str) -> Result<(), String> {
        self.spoil();
        self.inner.append_file(path, data)
    }

    fn remove(&self, path: &str) -> Result<(), String> {
        self.spoil();
        self.inner.remove(path)
    }

    fn chtimes(&self, path: &str, a_time: SystemTime, m_time: SystemTime) -> Result<(), String> {
        self.spoil();
        self.inner.chtimes(path, a_time, m_time)
    }

    fn directory_exists(&self, path: &str) -> bool {
        let exists = self.inner.directory_exists(path);
        self.observe(Kind::DirectoryExists, path, digest_bool(exists));
        exists
    }

    fn get_accessible_entries(&self, path: &str) -> Entries {
        let entries = self.inner.get_accessible_entries(path);
        self.observe(Kind::Entries, path, digest_entries(&entries));
        entries
    }

    fn stat(&self, path: &str) -> Option<FileInfo> {
        let info = self.inner.stat(path);
        self.observe(Kind::Stat, path, digest_info(info.as_ref()));
        info
    }

    fn realpath(&self, path: &str) -> String {
        let real = self.inner.realpath(path);
        self.observe(Kind::Realpath, path, xxh3_128(real.as_bytes()));
        real
    }
}

/// The OS system with the recorder as its file system and a copy of everything written to stdout.
struct RecordingSys {
    os: &'static OsSys,
    fs: Arc<dyn FS>,
    output: Mutex<String>,
    recorder: &'static Recorder,
}

impl System for RecordingSys {
    fn fs(&self) -> Arc<dyn FS> {
        Arc::clone(&self.fs)
    }

    fn default_library_path(&self) -> &str {
        self.os.default_library_path()
    }

    fn get_current_directory(&self) -> &str {
        self.os.get_current_directory()
    }

    fn write(&self, text: &str) {
        self.output.lock().unwrap().push_str(text);
        self.os.write(text);
    }

    fn flush(&self) {
        self.os.flush();
    }

    fn write_output_is_tty(&self) -> bool {
        self.os.write_output_is_tty()
    }

    fn get_environment_variable(&self, name: &str) -> Option<String> {
        self.os.get_environment_variable(name)
    }

    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn std::io::Write + Send>>,
    ) -> Result<ReadWriteCloser, String> {
        self.recorder.spoil();
        self.os.spawn(command, dir, stderr)
    }

    fn write_error(&self, text: &str) {
        self.recorder.spoil();
        self.os.write_error(text);
    }

    fn now(&self) -> Instant {
        self.os.now()
    }

    fn since_start(&self) -> Duration {
        self.os.since_start()
    }
}

struct Record<'a> {
    kind: Kind,
    path: Cow<'a, str>,
    digest: u128,
}

struct Entry<'a> {
    status: ExitStatus,
    output: &'a str,
    records: Vec<Record<'a>>,
}

// Entry layout (little endian): MAGIC, key (16), status (1), output length (8) and bytes, record count (8), then per
// record its kind (1), path length (4) and bytes, digest (16); last, the xxh3-128 of everything before it (16).
impl<'a> Entry<'a> {
    fn encode(key: u128, status: ExitStatus, output: &str, records: &[Record<'_>]) -> Vec<u8> {
        let size = records.iter().map(|r| 21 + r.path.len()).sum::<usize>() + output.len() + 64;
        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&key.to_le_bytes());
        out.push(status as u8);
        out.extend_from_slice(&(output.len() as u64).to_le_bytes());
        out.extend_from_slice(output.as_bytes());
        out.extend_from_slice(&(records.len() as u64).to_le_bytes());
        for r in records {
            out.push(r.kind as u8);
            out.extend_from_slice(&(r.path.len() as u32).to_le_bytes());
            out.extend_from_slice(r.path.as_bytes());
            out.extend_from_slice(&r.digest.to_le_bytes());
        }
        let checksum = xxh3_128(&out);
        out.extend_from_slice(&checksum.to_le_bytes());
        out
    }

    /// The entry in `bytes` if it is whole, of this format and for `key`.
    fn decode(bytes: &'a [u8], key: u128) -> Option<Entry<'a>> {
        let (body, checksum) = bytes.split_last_chunk::<16>()?;
        if xxh3_128(body) != u128::from_le_bytes(*checksum) {
            return None;
        }
        let mut r = Reader { bytes: body };
        if r.take(MAGIC.len())? != MAGIC || r.u128()? != key {
            return None;
        }
        let status = match r.take(1)?[0] {
            0 => ExitStatus::Success,
            1 => ExitStatus::DiagnosticsPresentOutputsSkipped,
            2 => ExitStatus::DiagnosticsPresentOutputsGenerated,
            _ => return None,
        };
        let output_len = usize::try_from(r.u64()?).ok()?;
        let output = std::str::from_utf8(r.take(output_len)?).ok()?;
        let count = usize::try_from(r.u64()?).ok()?;
        let mut records = Vec::with_capacity(count.min(body.len() / 21));
        for _ in 0..count {
            let kind = *KINDS.get(usize::from(r.take(1)?[0]))?;
            let path_len = u32::from_le_bytes(*r.take(4)?.first_chunk()?) as usize;
            let path = std::str::from_utf8(r.take(path_len)?).ok()?;
            records.push(Record { kind, path: Cow::Borrowed(path), digest: r.u128()? });
        }
        r.bytes.is_empty().then_some(Entry { status, output, records })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.bytes.split_at_checked(n)?;
        self.bytes = tail;
        Some(head)
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(*self.take(8)?.first_chunk()?))
    }

    fn u128(&mut self) -> Option<u128> {
        Some(u128::from_le_bytes(*self.take(16)?.first_chunk()?))
    }
}

/// Asks every recorded question again on parallel threads; `Err` holds the index of a record whose answer changed.
fn validate(records: &[Record<'_>]) -> Result<(), usize> {
    const CHUNK: usize = 256;
    let fs = osvfs::fs();
    let chunks = records.len().div_ceil(CHUNK);
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(16)
        .min(chunks)
        .max(1);
    let next = AtomicUsize::new(0);
    let changed = AtomicUsize::new(usize::MAX);
    let work = || {
        loop {
            // Relaxed (both): a work counter and a stop flag; the scope's join publishes the result.
            let chunk = next.fetch_add(1, Ordering::Relaxed);
            if chunk >= chunks || changed.load(Ordering::Relaxed) != usize::MAX {
                return;
            }
            let start = chunk * CHUNK;
            for (i, r) in records[start..(start + CHUNK).min(records.len())].iter().enumerate() {
                if answer(fs, r.kind, &r.path) != Some(r.digest) {
                    // Relaxed: see above.
                    changed.fetch_min(start + i, Ordering::Relaxed);
                    return;
                }
            }
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..threads {
            scope.spawn(work);
        }
        work();
    });
    match changed.into_inner() {
        usize::MAX => Ok(()),
        index => Err(index),
    }
}

/// Writes the entry atomically (a temporary file renamed over it), then keeps the directory to the newest
/// `MAX_ENTRIES` entries and `MAX_BYTES`.
fn save(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    prune(dir);
    Ok(())
}

fn prune(dir: &Path) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut entries = Vec::new();
    for entry in read_dir.flatten() {
        let Ok(md) = entry.metadata() else {
            continue;
        };
        let modified = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let path = entry.path();
        // A temporary file left by a run that died before its rename.
        if path.extension().is_some_and(|e| e == "tmp") {
            if now.duration_since(modified).is_ok_and(|age| age > Duration::from_secs(3600)) {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        entries.push((modified, md.len(), path));
    }
    entries.sort_unstable_by_key(|&(modified, ..)| std::cmp::Reverse(modified));
    let mut total = 0;
    for (i, (_, len, path)) in entries.iter().enumerate() {
        total += len;
        if i >= MAX_ENTRIES || total > MAX_BYTES {
            let _ = std::fs::remove_file(path);
        }
    }
}
