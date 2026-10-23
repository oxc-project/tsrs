// The threads that run asynchronous request work (Go: `go func() { doAsyncWork() }()` in dispatchLoop, a fresh
// goroutine per request). As in Go, a job never waits for a thread: when every worker is busy (often blocked on a
// checker, e.g. behind the one diagnostics checker), `go` starts another, so a request that needs no checker or a
// free one does not queue behind them. Up to `max(4, available_parallelism)` workers stay for the rest of the
// process and wait for jobs; one started beyond that exits as soon as it finds no job, handing back its thread arena
// and slab (tsrs_core ptr `ReleaseAtExit`, arena `CurrentSlab`), so the number of threads follows the number of
// requests in flight, like Go's goroutines. Each has a 512 MB stack (the checker recurses as deeply as in the CLI;
// Go's goroutine stacks grow without a fixed limit); the reservation is virtual.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, OnceLock};

pub const WORKER_STACK_SIZE: usize = 512 << 20;

type Job = Box<dyn FnOnce() + Send>;

struct WorkerPool {
    state: Mutex<WorkerPoolState>,
    cond: Condvar,
    // Workers that stay when there is no job.
    max_idle_threads: usize,
}

struct WorkerPoolState {
    jobs: VecDeque<Job>,
    threads: usize,
    idle: usize,
}

fn get_pool() -> &'static WorkerPool {
    static POOL: OnceLock<WorkerPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let parallelism = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        WorkerPool {
            state: Mutex::new(WorkerPoolState { jobs: VecDeque::new(), threads: 0, idle: 0 }),
            cond: Condvar::new(),
            max_idle_threads: parallelism.max(4),
        }
    })
}

// Go `go f()` for request work.
pub fn go(f: impl FnOnce() + Send + 'static) {
    let pool = get_pool();
    let mut state = pool.state.lock().unwrap();
    state.jobs.push_back(Box::new(f));
    // Each idle worker takes one queued job (a notified one still counts as idle until it wakes).
    if state.jobs.len() > state.idle {
        state.threads += 1;
        let n = state.threads;
        drop(state);
        let spawned = std::thread::Builder::new()
            .name(format!("lsp-worker-{n}"))
            .stack_size(WORKER_STACK_SIZE)
            .spawn(|| worker_loop(get_pool()));
        if let Err(err) = spawned {
            // Out of threads or address space: the job waits for a running worker, as it did with a fixed pool.
            let mut state = pool.state.lock().unwrap();
            state.threads -= 1;
            assert!(state.threads > 0, "failed to spawn a language server worker thread: {err}");
        }
    } else {
        drop(state);
        pool.cond.notify_one();
    }
}

fn worker_loop(pool: &'static WorkerPool) {
    let mut state = pool.state.lock().unwrap();
    loop {
        if let Some(job) = state.jobs.pop_front() {
            drop(state);
            // Go's GOMAXPROCS: the thread starts at once, but runs once a slot is free (tsrs_core `runslots`).
            run_or_exit(|| tsrs_core::runslots::run(job));
            state = pool.state.lock().unwrap();
            continue;
        }
        if state.threads > pool.max_idle_threads {
            state.threads -= 1;
            return;
        }
        state.idle += 1;
        state = pool.cond.wait(state).unwrap();
        state.idle -= 1;
    }
}

#[cfg(test)]
pub fn max_idle_threads() -> usize {
    get_pool().max_idle_threads
}

#[cfg(test)]
pub fn threads() -> usize {
    get_pool().state.lock().unwrap().threads
}

// An unrecovered panic in a goroutine ends a Go program (exit status 2, after the runtime prints the panic); the
// panic hook has already printed the message and location.
pub fn run_or_exit(f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        std::process::exit(2);
    }
}
