use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use tsrs_core::context::Context;

use tsrs_project::background::queue::*;

// queue_test.go:15 TestQueue/BasicEnqueue
#[test]
fn queue_basic_enqueue() {
    let q = new_queue();

    let executed = Arc::new(AtomicBool::new(false));
    let e = std::sync::Arc::clone(&executed);
    q.enqueue(&Context::background(), move |_ctx| {
        e.store(true, Ordering::SeqCst);
    });

    q.wait();
    q.close();

    assert!(executed.load(Ordering::SeqCst));
}

// queue_test.go:30 TestQueue/MultipleTasksExecution
#[test]
fn queue_multiple_tasks_execution() {
    let q = new_queue();

    let counter = Arc::new(AtomicUsize::new(0));
    let num_tasks = 10;

    for _ in 0..num_tasks {
        let c = std::sync::Arc::clone(&counter);
        q.enqueue(&Context::background(), move |_ctx| {
            c.fetch_add(1, Ordering::SeqCst);
        });
    }

    q.wait();
    q.close();

    assert_eq!(counter.load(Ordering::SeqCst), num_tasks);
}

// queue_test.go:49 TestQueue/NestedEnqueue
#[test]
fn queue_nested_enqueue() {
    let q = Arc::new(new_queue());

    let executed = Arc::new(Mutex::new(Vec::<String>::new()));

    let (q2, e) = (std::sync::Arc::clone(&q), std::sync::Arc::clone(&executed));
    q.enqueue(&Context::background(), move |ctx| {
        e.lock().unwrap().push("parent".to_string());

        let e2 = std::sync::Arc::clone(&e);
        q2.enqueue(ctx, move |_child_ctx| {
            e2.lock().unwrap().push("child".to_string());
        });
    });

    q.wait();
    q.close();

    assert_eq!(executed.lock().unwrap().len(), 2);
}

// queue_test.go:77 TestQueue/ClosedQueueRejectsNewTasks
#[test]
fn queue_closed_queue_rejects_new_tasks() {
    let q = new_queue();
    q.close();

    let executed = Arc::new(AtomicBool::new(false));
    let e = std::sync::Arc::clone(&executed);
    q.enqueue(&Context::background(), move |_ctx| {
        e.store(true, Ordering::SeqCst);
    });

    q.wait();

    assert!(!executed.load(Ordering::SeqCst), "Task should not execute after queue is closed");
}

// queue_test.go:92 TestQueue/CloseWaitsForActiveTasks
#[test]
fn queue_close_waits_for_active_tasks() {
    let q = Arc::new(new_queue());
    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    q.enqueue(&Context::background(), move |_| {
        started_tx.send(()).unwrap();
        finish_rx.recv().unwrap();
    });
    started_rx.recv().unwrap();

    let (close_started_tx, close_started_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    let q_for_close = Arc::clone(&q);
    let close_thread = std::thread::spawn(move || {
        close_started_tx.send(()).unwrap();
        q_for_close.close();
        closed_tx.send(()).unwrap();
    });

    close_started_rx.recv().unwrap();
    assert!(closed_rx.recv_timeout(Duration::from_millis(100)).is_err());
    finish_tx.send(()).unwrap();
    closed_rx.recv().unwrap();
    close_thread.join().unwrap();
}
