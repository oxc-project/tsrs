// tsrs-only: a stale cached subscription descriptor must never close an unrelated file after descriptor reuse.

use std::os::fd::IntoRawFd;
use std::sync::Arc;

use super::{DirEntry, EntriesID, KqueueBackend, KqueueSubscription, new_kqueue_backend};
use crate::debounce::Debounce;
use crate::pathcompare::BytePathComparer;
use crate::watcher::{DirWatch, WatcherImpl};

fn open_fd() -> i32 {
    std::fs::File::open("/dev/null").unwrap().into_raw_fd()
}

fn is_open(fd: i32) -> bool {
    // SAFETY: F_GETFD only queries the descriptor; the isolated child owns every descriptor tested here.
    unsafe { libc::fcntl(fd, libc::F_GETFD) >= 0 }
}

fn insert_watch(backend: &KqueueBackend, watch: &Arc<DirWatch>, map_id: EntriesID) -> i32 {
    let fd = open_fd();
    let path = watch.dir.clone();
    let mut st = backend.mu.lock().unwrap();
    st.entry_maps.insert(
        map_id,
        rustc_hash::FxHashMap::from_iter([(
            path.clone(),
            DirEntry {
                path: path.clone(),
                watch_path: path.clone(),
                is_dir: false,
                state: Some(fd),
            },
        )]),
    );
    st.fd_to_entry.insert(fd, (map_id, path.clone()));
    st.subs_by_path.entry(path.clone()).or_default().push(KqueueSubscription {
        id: backend.next_id(),
        dir_watch: Arc::clone(watch),
        path,
        entries: map_id,
    });
    fd
}

#[test]
fn closing_watches_owns_only_current_entry_descriptors() {
    const CHILD: &str = "kqueue-descriptor-child";
    if !std::env::args().any(|arg| arg == CHILD) {
        // Descriptor reuse is controlled in a fresh process, without concurrent watchers or directory reads.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "kqueue::tests::closing_watches_owns_only_current_entry_descriptors",
                "--nocapture",
                CHILD,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed;"));
        return;
    }

    let db = Debounce::new_without_loop_for_test();
    let watch = DirWatch::new(
        "/watched".into(),
        "/watched".into(),
        &db,
        BytePathComparer::default(),
        None,
        false,
    );
    let mut ownership_results = Vec::new();
    for shutdown in [false, true] {
        let backend = new_kqueue_backend();
        let old_fd = insert_watch(&backend, &watch, 1);
        backend.mu.lock().unwrap().close_entry_locked(1, &watch.dir);
        let sentinel = open_fd();
        assert_eq!(sentinel, old_fd, "the unrelated file must reuse the closed descriptor");
        let replacement = open_fd();
        {
            let mut st = backend.mu.lock().unwrap();
            st.set_entry_state(1, &watch.dir, Some(replacement));
            st.fd_to_entry.insert(replacement, (1, watch.dir.clone()));
        }
        if shutdown {
            backend.close_subscriptions();
        } else {
            backend.close_watch(&watch).unwrap();
        }
        let sentinel_open = is_open(sentinel);
        let replacement_open = is_open(replacement);
        if sentinel_open {
            super::close_fd(sentinel);
        }
        if replacement_open {
            super::close_fd(replacement);
        }
        ownership_results.push((shutdown, sentinel_open, replacement_open));
    }

    let overlapping = DirWatch::new(
        watch.dir.clone(),
        watch.dir.clone(),
        &db,
        BytePathComparer::default(),
        None,
        true,
    );
    let backend = new_kqueue_backend();
    let first = insert_watch(&backend, &watch, 1);
    let second = insert_watch(&backend, &overlapping, 2);
    backend.close_watch(&watch).unwrap();
    let first_open = is_open(first);
    let second_open = is_open(second);
    backend.close_watch(&overlapping).unwrap();
    let second_leaked = is_open(second);
    if first_open {
        super::close_fd(first);
    }
    if second_leaked {
        super::close_fd(second);
    }
    assert_eq!(
        ownership_results,
        [(false, true, false), (true, true, false)],
        "watch close and backend shutdown must preserve reused descriptors and close replacements; \
         overlap: first_open={first_open}, second_open={second_open}, second_leaked={second_leaked}"
    );
    assert!(!first_open, "closing one overlapping watch leaked its descriptor");
    assert!(second_open, "closing one overlapping watch closed the other's descriptor");
    assert!(!second_leaked);
}
