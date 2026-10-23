// testutil_test.go + the helpers of watcher_test.go. Go's retry wrapper re-runs a failing test body up to
// three times with scaled timeouts (macOS event delivery stalls under load); here a body returns
// `Err(message)` where Go calls Fatal, and `run_with_retry` re-runs it. Go's `testingT.TempDir` / `Cleanup` are
// `TestT::temp_dir` / `TestT::cleanup` (run when the attempt ends).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::event::{Event, EventKind};
use crate::watcher::{
    Error, Watch, WatchCallback, WatchOption, Watcher, all_watchers, fsevents, kqueue,
};

pub type TestResult = Result<(), String>;

macro_rules! fatalf {
    ($($arg:tt)*) => { return Err(format!($($arg)*)) };
}
pub(crate) use fatalf;

// testutil_test.go:49
const RETRY_ATTEMPTS: usize = 3;

// testutil_test.go:55
fn retry_timeout_scale(attempt: usize) -> u32 {
    match attempt {
        1 => 1,
        2 => 5,
        _ => 15,
    }
}

pub struct TestT {
    pub(crate) attempt: usize,
    cleanups: Vec<Box<dyn FnOnce()>>,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl TestT {
    pub(crate) fn new(attempt: usize) -> TestT {
        TestT { attempt, cleanups: Vec::new() }
    }

    pub(crate) fn cleanup(&mut self, f: impl FnOnce() + 'static) {
        self.cleanups.push(Box::new(f));
    }

    // Go `t.TempDir()`.
    pub(crate) fn temp_dir(&mut self) -> String {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tsrs-fswatch-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        let s = dir.to_string_lossy().into_owned();
        let remove = s.clone();
        self.cleanup(move || {
            let _ = std::fs::remove_dir_all(&remove);
        });
        s
    }
}

impl Drop for TestT {
    fn drop(&mut self) {
        while let Some(f) = self.cleanups.pop() {
            f();
        }
    }
}

// testutil_test.go:166
pub fn run_with_retry(name: &str, body: &dyn Fn(&mut TestT) -> TestResult) {
    let mut last = String::new();
    for attempt in 1..=RETRY_ATTEMPTS {
        let mut t = TestT::new(attempt);
        match body(&mut t) {
            Ok(()) => return,
            Err(msg) => {
                eprintln!("{name}: attempt {attempt}/{RETRY_ATTEMPTS} failed: {msg}");
                last = msg;
            }
        }
    }
    panic!("{name}: retry: gave up after {RETRY_ATTEMPTS} attempts: {last}");
}

// watcher_test.go:97
// runForEachWatcher runs fn for every available watcher (in parallel, like Go's parallel subtests).
pub fn run_for_each_watcher(
    name: &str,
    f: impl Fn(&mut TestT, &'static dyn Watcher) -> TestResult + Sync,
) {
    let watchers: Vec<&'static dyn Watcher> =
        all_watchers().into_iter().filter(|w| w.available()).collect();
    std::thread::scope(|s| {
        for w in watchers {
            let f = &f;
            s.spawn(move || run_with_retry(&format!("{}/{}", name, w.name()), &|t| f(t, w)));
        }
    });
}

pub fn is_slow(w: &dyn Watcher) -> bool {
    std::ptr::addr_eq(w, fsevents()) || std::ptr::addr_eq(w, kqueue())
}

// watcher_test.go:50
pub fn watcher_event_timeout(t: &TestT, w: &dyn Watcher) -> Duration {
    let base = if is_slow(w) { Duration::from_secs(2) } else { Duration::from_secs(1) };
    base * retry_timeout_scale(t.attempt)
}

// watcher_test.go:111
// newTmpDir creates a fresh temp dir, resolves any symlinks in the path so
// it matches what backends report, and registers cleanup.
pub fn new_tmp_dir(t: &mut TestT) -> String {
    let d = t.temp_dir();
    std::fs::canonicalize(&d).unwrap().to_string_lossy().into_owned()
}

static NAME_COUNTER: AtomicU64 = AtomicU64::new(0);

// watcher_test.go:136
pub fn sub_path(dir: &str) -> String {
    let n = NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
    format!("{dir}/test{n}{nanos}")
}

// watcher_test.go:167
pub fn settle_sleep(w: &dyn Watcher) -> Duration {
    if is_slow(w) { Duration::from_millis(300) } else { Duration::from_millis(60) }
}

// watcher_test.go:177
fn pre_subscribe_sleep(w: &dyn Watcher) -> Duration {
    if is_slow(w) { Duration::from_millis(50) } else { Duration::ZERO }
}

pub type Subscription = (Arc<RecordingWatcher>, Arc<dyn Watch>);

// watcher_test.go:159
pub fn subscribe_for(
    t: &mut TestT,
    dir: &str,
    w: &'static dyn Watcher,
) -> Result<Subscription, String> {
    subscribe_for_opts(t, dir, w, vec![WatchOption::Recursive])
}

// watcher_test.go:185
pub fn subscribe_file_for(
    t: &mut TestT,
    path: &str,
    w: &'static dyn Watcher,
) -> Result<Subscription, String> {
    std::thread::sleep(pre_subscribe_sleep(w));
    let r = new_recorder(t, Some(w));
    let sub: Arc<dyn Watch> =
        Arc::from(w.watch_file(path, r.callback()).map_err(|e| format!("subscribeFile: {e}"))?);
    let s = std::sync::Arc::clone(&sub);
    t.cleanup(move || {
        let _ = s.close();
    });
    std::thread::sleep(settle_sleep(w));
    Ok((r, sub))
}

// watcher_test.go:202
pub fn subscribe_for_opts(
    t: &mut TestT,
    dir: &str,
    w: &'static dyn Watcher,
    opts: Vec<WatchOption>,
) -> Result<Subscription, String> {
    std::thread::sleep(pre_subscribe_sleep(w));
    let r = new_recorder(t, Some(w));
    let sub: Arc<dyn Watch> = Arc::from(
        w.watch_directory(dir, r.callback(), opts).map_err(|e| format!("subscribe: {e}"))?,
    );
    let s = std::sync::Arc::clone(&sub);
    t.cleanup(move || {
        let _ = s.close();
    });
    std::thread::sleep(settle_sleep(w));
    Ok((r, sub))
}

// ----- recordingWatcher --------------------------------------------------

pub struct RecordingWatcher {
    deadline: Duration,
    pub(crate) mu: Mutex<RecordingState>,
    cond: Condvar,
}

#[derive(Default)]
pub struct RecordingState {
    pub(crate) buf: Vec<Event>,
    pub(crate) errs: Vec<Error>,
}

// watcher_test.go:229
pub fn new_recorder(t: &TestT, w: Option<&'static dyn Watcher>) -> Arc<RecordingWatcher> {
    let deadline = match w {
        Some(w) => watcher_event_timeout(t, w),
        None => Duration::from_secs(1) * retry_timeout_scale(t.attempt),
    };
    Arc::new(RecordingWatcher {
        deadline,
        mu: Mutex::new(RecordingState::default()),
        cond: Condvar::new(),
    })
}

impl RecordingWatcher {
    // watcher_test.go:239
    pub(crate) fn deadline(&self) -> Duration {
        self.deadline
    }

    // watcher_test.go:256
    pub(crate) fn callback(self: &Arc<Self>) -> WatchCallback {
        let r = std::sync::Arc::clone(self);
        Arc::new(move |events: Vec<Event>, err: Option<Error>| {
            let mut st = r.mu.lock().unwrap();
            if let Some(err) = err {
                st.errs.push(err);
            }
            st.buf.extend(events);
            r.cond.notify_all();
        })
    }

    // watcher_test.go:268
    // next blocks for up to d for at least one event, then drains and returns
    // everything that has accumulated.
    pub(crate) fn next(&self, d: Duration) -> Vec<Event> {
        let st = self.mu.lock().unwrap();
        let (mut st, _) = self.cond.wait_timeout_while(st, d, |st| st.buf.is_empty()).unwrap();
        std::mem::take(&mut st.buf)
    }

    // watcher_test.go:293
    pub(crate) fn drain_quiet(&self, d: Duration) -> Vec<Event> {
        self.mu.lock().unwrap().buf.clear();
        std::thread::sleep(d);
        std::mem::take(&mut self.mu.lock().unwrap().buf)
    }

    // watcher_test.go:346
    pub(crate) fn wait_for_event(&self, d: Duration, pred: impl Fn(&Event) -> bool) -> Vec<Event> {
        let deadline = Instant::now() + d;
        let mut st = self.mu.lock().unwrap();
        loop {
            if st.buf.iter().any(&pred) {
                return std::mem::take(&mut st.buf);
            }
            let now = Instant::now();
            if now >= deadline {
                return std::mem::take(&mut st.buf);
            }
            st = self.cond.wait_timeout(st, deadline - now).unwrap().0;
        }
    }

    // watcher_test.go:385
    pub(crate) fn wait_for_all(&self, d: Duration, want: &[WantEvent]) -> Vec<Event> {
        if want.is_empty() {
            return Vec::new();
        }
        let deadline = Instant::now() + d;
        let mut collected = Vec::new();
        let mut st = self.mu.lock().unwrap();
        loop {
            collected.append(&mut st.buf);
            if have_all(&collected, want) {
                return collected;
            }
            let now = Instant::now();
            if now >= deadline {
                return collected;
            }
            st = self.cond.wait_timeout(st, deadline - now).unwrap().0;
        }
    }

    pub(crate) fn take_errs(&self) -> Vec<Error> {
        std::mem::take(&mut self.mu.lock().unwrap().errs)
    }

    pub(crate) fn err_count(&self) -> usize {
        self.mu.lock().unwrap().errs.len()
    }
}

// ----- assertion helpers -------------------------------------------------

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct WantEvent(pub(crate) i32, pub(crate) String);

pub fn want(kind: EventKind, path: &str) -> WantEvent {
    WantEvent(kind.0, path.to_string())
}

// watcher_test.go:422
fn have_all(got: &[Event], want: &[WantEvent]) -> bool {
    want.iter().all(|w| got.iter().any(|e| e.kind.0 == w.0 && e.path == w.1))
}

pub fn to_want_events(events: &[Event]) -> Vec<WantEvent> {
    events.iter().map(|e| WantEvent(e.kind.0, e.path.clone())).collect()
}

// watcher_test.go:443
pub fn expect_event_set(r: &RecordingWatcher, want: Vec<WantEvent>) -> Result<Vec<Event>, String> {
    let got = r.wait_for_all(r.deadline(), &want);
    assert_event_set(&got, want)?;
    Ok(got)
}

// watcher_test.go:454
pub fn expect_event_sequence(
    r: &RecordingWatcher,
    want: &[WantEvent],
) -> Result<Vec<Event>, String> {
    let got = r.wait_for_all(r.deadline(), want);
    assert_event_sequence(&got, want)?;
    Ok(got)
}

// watcher_test.go:465
pub fn expect_contains(
    r: &RecordingWatcher,
    kind: EventKind,
    path: &str,
) -> Result<Vec<Event>, String> {
    let d = r.deadline();
    let got = r.wait_for_event(d, |e| e.kind == kind && e.path == path);
    if !contains_event(&got, kind, path) {
        fatalf!("expected event {} {} within {:?}, got {:?}", kind, path, d, to_want_events(&got));
    }
    Ok(got)
}

// watcher_test.go:477
pub fn expect_no_buffered_events(r: &RecordingWatcher, msg: &str) -> TestResult {
    let got = std::mem::take(&mut r.mu.lock().unwrap().buf);
    if !got.is_empty() {
        fatalf!("{}, got {:?}", msg, to_want_events(&got));
    }
    Ok(())
}

// watcher_test.go:488
pub fn assert_no_events_for_path(got: &[Event], path: &str, msg: &str) -> TestResult {
    let got = filter_events_for_paths(got, &[path]);
    if !got.is_empty() {
        fatalf!("{} {}, got {:?}", msg, path, to_want_events(&got));
    }
    Ok(())
}

// watcher_test.go:513
pub fn assert_event_set(got: &[Event], mut want: Vec<WantEvent>) -> TestResult {
    let mut got_w = to_want_events(&filter_to_wanted_paths(got, &want));
    got_w.sort();
    want.sort();
    if got_w != want {
        fatalf!("event mismatch\nwant: {:?}\n got: {:?}", want, got_w);
    }
    Ok(())
}

// watcher_test.go:532
pub fn assert_event_sequence(got: &[Event], want: &[WantEvent]) -> TestResult {
    let got_w = to_want_events(&filter_to_wanted_paths(got, want));
    if got_w != want {
        fatalf!("event sequence mismatch\nwant: {:?}\n got: {:?}", want, got_w);
    }
    Ok(())
}

// watcher_test.go:542
fn filter_to_wanted_paths(got: &[Event], want: &[WantEvent]) -> Vec<Event> {
    got.iter().filter(|e| want.iter().any(|w| w.1 == e.path)).cloned().collect()
}

// watcher_test.go:569
pub fn contains_event(got: &[Event], typ: EventKind, path: &str) -> bool {
    got.iter().any(|e| e.kind == typ && e.path == path)
}

// watcher_test.go:820
pub fn filter_events_for_paths(events: &[Event], paths: &[&str]) -> Vec<Event> {
    events.iter().filter(|e| paths.contains(&e.path.as_str())).cloned().collect()
}

pub fn noop_callback() -> WatchCallback {
    Arc::new(|_: Vec<Event>, _: Option<Error>| {})
}
