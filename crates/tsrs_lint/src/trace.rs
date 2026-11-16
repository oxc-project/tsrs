//! `TSRS_LINT_TRACE=<path>`: a span per request phase and per file a checker runs, written as tab-separated lines
//! (start and end in seconds since the request started, thread, span name, detail) when the request ends. For
//! measuring where a lint run's time goes; nothing is recorded without the variable.

use std::fmt::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct Trace {
    path: String,
    start: Instant,
    spans: Mutex<Vec<(f64, f64, String, &'static str, String)>>,
}

fn trace() -> Option<&'static Trace> {
    static TRACE: OnceLock<Option<Trace>> = OnceLock::new();
    TRACE
        .get_or_init(|| {
            let path = std::env::var("TSRS_LINT_TRACE").ok().filter(|p| !p.is_empty())?;
            Some(Trace { path, start: Instant::now(), spans: Mutex::new(Vec::new()) })
        })
        .as_ref()
}

/// Starts the request's clock.
pub fn start() {
    let _ = trace();
}

/// A span that ends when it is dropped; `None` without the variable.
pub struct Span(Option<(Instant, &'static str, String)>);

pub fn span(name: &'static str, detail: impl FnOnce() -> String) -> Span {
    Span(trace().map(|_| (Instant::now(), name, detail())))
}

impl Drop for Span {
    fn drop(&mut self) {
        let (Some(trace), Some((start, name, detail))) = (trace(), self.0.take()) else {
            return;
        };
        let thread = std::thread::current();
        let thread = format!("{}{:?}", thread.name().unwrap_or(""), thread.id());
        let from = start.duration_since(trace.start).as_secs_f64();
        let to = trace.start.elapsed().as_secs_f64();
        trace.spans.lock().unwrap().push((from, to, thread, name, detail));
    }
}

/// Writes the spans.
pub fn finish() {
    let Some(trace) = trace() else {
        return;
    };
    let mut out = String::new();
    for (from, to, thread, name, detail) in trace.spans.lock().unwrap().iter() {
        let _ = writeln!(out, "{from:.6}\t{to:.6}\t{thread}\t{name}\t{detail}");
    }
    let _ = std::fs::write(&trace.path, out);
}

impl Span {
    /// Runs `f` inside the span.
    pub fn finish_with<T>(self, f: impl FnOnce() -> T) -> T {
        let result = f();
        drop(self);
        result
    }
}
