use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use tsrs_core::runslots;
use tsrs_ls::concurrent::run_concurrently;

#[test]
fn blocked_parallel_requests_release_their_execution_slots() {
    const CHILD: &str = "TSRS_TEST_PARALLEL_REQUEST_SLOTS";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "concurrent_test::blocked_parallel_requests_release_their_execution_slots",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let count = runslots::capacity();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let (started_tx, started_rx) = mpsc::channel();
    let mut parents = Vec::new();
    for _ in 0..count {
        let release_rx = Arc::clone(&release_rx);
        let started_tx = started_tx.clone();
        parents.push(std::thread::spawn(move || {
            runslots::run(|| {
                run_concurrently(
                    vec![(), ()],
                    &|()| {
                        assert!(
                            runslots::HOLDS_SLOT.with(std::cell::Cell::get),
                            "search work must hold an execution slot"
                        );
                        started_tx.send(()).unwrap();
                        runslots::blocking(|| release_rx.lock().unwrap().recv().unwrap());
                    },
                    &|work| work(),
                );
            });
        }));
    }
    for _ in 0..count * count.min(2) {
        started_rx.recv_timeout(Duration::from_secs(15)).expect("parallel search did not start");
    }
    let (done_tx, done_rx) = mpsc::channel();
    let independent = std::thread::spawn(move || runslots::run(|| done_tx.send(()).unwrap()));
    let ran = done_rx.recv_timeout(Duration::from_secs(15));
    for _ in 0..count * 2 {
        release_tx.send(()).unwrap();
    }
    for parent in parents {
        parent.join().unwrap();
    }
    independent.join().unwrap();
    ran.expect("checker-free work queued behind blocked parallel requests");
}
