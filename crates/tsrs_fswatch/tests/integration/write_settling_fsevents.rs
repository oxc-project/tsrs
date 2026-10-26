// `set_write_settling` (FSEvents): a save that truncates the file and writes it later is not delivered while the
// file is empty. It runs in its own process: the setting is process-wide, and the other tests run in parallel threads.
#![cfg(target_os = "macos")]

use std::collections::BTreeSet;
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tsrs_fswatch::{fsevents, set_write_settling};

struct Recorder {
    seen: Mutex<Vec<(Instant, String)>>,
    cv: Condvar,
}

impl Recorder {
    // Forgets what was seen so far: FSEvents also delivers the events of the setup, which happened about 12 ms
    // before the stream started (they reach the stream after its start).
    fn clear(&self) {
        self.seen.lock().unwrap().clear();
    }

    fn wait_for(&self, count: usize, timeout: Duration) -> Vec<(Instant, String)> {
        let seen = self.seen.lock().unwrap();
        let (seen, _) = self.cv.wait_timeout_while(seen, timeout, |s| s.len() < count).unwrap();
        seen.clone()
    }
}

fn watch(
    dir: &std::path::Path,
    file: &std::path::Path,
) -> (Arc<Recorder>, Box<dyn tsrs_fswatch::Watch>) {
    let recorder = Arc::new(Recorder { seen: Mutex::new(Vec::new()), cv: Condvar::new() });
    let r = Arc::clone(&recorder);
    let file = file.to_path_buf();
    let watch = fsevents()
        .watch_directory(
            dir.to_str().unwrap(),
            Arc::new(move |events, _err| {
                if events.is_empty() {
                    return;
                }
                let content = std::fs::read_to_string(&file).unwrap_or_default();
                r.seen.lock().unwrap().push((Instant::now(), content));
                r.cv.notify_all();
            }),
            Vec::new(),
        )
        .unwrap();
    (recorder, watch)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tsrs-fswatch-settle-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

#[test]
fn write_settling() {
    if !crate::isolated::in_child("write_settling_fsevents::write_settling") {
        return;
    }
    set_write_settling(true);
    a_truncation_waits_for_the_data();
    a_complete_save_is_not_delayed();
    an_emptied_file_stops_holding_after_the_idle_time();
    a_burst_of_writes_delivers_every_path();
}

fn a_truncation_waits_for_the_data() {
    let dir = temp_dir("truncate");
    let file = dir.join("a.ts");
    std::fs::write(&file, "export const a = 0;\n").unwrap();
    let (recorder, watch) = watch(&dir, &file);
    // A quiet period first: without settling, the next event would fire at once (the debounce's leading edge).
    std::thread::sleep(Duration::from_millis(700));
    recorder.clear();

    let mut f = std::fs::File::create(&file).unwrap();
    assert!(
        recorder.wait_for(1, Duration::from_millis(300)).is_empty(),
        "fired while the file was empty"
    );
    f.write_all(b"export const a = 1;\n").unwrap();
    drop(f);
    let written = Instant::now();

    let seen = recorder.wait_for(2, Duration::from_millis(1500));
    assert_eq!(seen.len(), 1, "one batch for the whole save");
    let (at, content) = &seen[0];
    assert_eq!(content, "export const a = 1;\n");
    // At the write, not when the hold times out (`settleIdleTime`, 1 s).
    assert!(
        at.duration_since(written) < Duration::from_millis(300),
        "fired {:?} after the write",
        at.duration_since(written)
    );
    let _ = watch.close();
    let _ = std::fs::remove_dir_all(&dir);
}

fn a_complete_save_is_not_delayed() {
    let dir = temp_dir("complete");
    let file = dir.join("a.ts");
    std::fs::write(&file, "export const a = 0;\n").unwrap();
    let (recorder, watch) = watch(&dir, &file);
    std::thread::sleep(Duration::from_millis(700));
    recorder.clear();

    std::fs::write(&file, "export const a = 2;\n").unwrap();
    let written = Instant::now();
    let seen = recorder.wait_for(1, Duration::from_secs(2));
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].0.duration_since(written) < Duration::from_millis(100),
        "fired {:?} after the write",
        seen[0].0.duration_since(written)
    );
    let _ = watch.close();
    let _ = std::fs::remove_dir_all(&dir);
}

fn an_emptied_file_stops_holding_after_the_idle_time() {
    let dir = temp_dir("empty");
    let file = dir.join("a.ts");
    std::fs::write(&file, "export const a = 0;\n").unwrap();
    let (recorder, watch) = watch(&dir, &file);
    std::thread::sleep(Duration::from_millis(700));
    recorder.clear();

    std::fs::write(&file, "").unwrap();
    let written = Instant::now();
    let seen = recorder.wait_for(1, Duration::from_secs(5));
    assert_eq!(seen.len(), 1, "an empty file must not hold the callbacks for good");
    let waited = seen[0].0.duration_since(written);
    assert!(
        waited >= Duration::from_millis(900) && waited < Duration::from_millis(2500),
        "released after {waited:?}"
    );
    let _ = watch.close();
    let _ = std::fs::remove_dir_all(&dir);
}

fn a_burst_of_writes_delivers_every_path() {
    let dir = temp_dir("burst");
    let seen = Arc::new((Mutex::new(BTreeSet::new()), Condvar::new()));
    let callback_seen = Arc::clone(&seen);
    let watch = fsevents()
        .watch_directory(
            dir.to_str().unwrap(),
            Arc::new(move |events, _err| {
                if !events.is_empty() {
                    let (paths, ready) = &*callback_seen;
                    paths.lock().unwrap().extend(events.iter().map(|event| event.path.clone()));
                    ready.notify_all();
                }
            }),
            Vec::new(),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(700));
    seen.0.lock().unwrap().clear();

    // FSEvents can deliver the first file before the next write. That single-file native batch correctly fires
    // immediately; only a native batch with multiple files holds the leading edge. Verify OS delivery here;
    // fsevents_darwin_test::write_settling_holds_multi_file_native_batches injects controlled native batches.
    let mut expected = BTreeSet::new();
    for i in 0..20 {
        let path = dir.join(format!("f{i}.ts"));
        std::fs::write(&path, format!("export const f{i} = {i};\n")).unwrap();
        expected.insert(path.to_str().unwrap().to_string());
        std::thread::sleep(Duration::from_millis(5));
    }
    let (paths, ready) = &*seen;
    let paths = paths.lock().unwrap();
    let (paths, _) = ready
        .wait_timeout_while(paths, Duration::from_secs(5), |paths| *paths != expected)
        .unwrap();
    assert_eq!(*paths, expected, "every written path must be delivered");
    drop(paths);
    let _ = watch.close();
    let _ = std::fs::remove_dir_all(&dir);
}
