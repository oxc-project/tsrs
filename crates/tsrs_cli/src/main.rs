// Miri (which cannot run mimalloc) and `system-alloc` builds (sanitizers, which see only the system allocator's
// blocks) keep the system allocator.
#[cfg(not(any(feature = "system-alloc", miri)))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

use tsrs_cli::{api, lsp, signals};
use tsrs_execute::{execute, sys, tsc};

// TSRS_MEM_SPLIT (tsrs_core::memsplit): mimalloc's view of its heap, every page of every thread.
#[cfg(not(feature = "system-alloc"))]
fn mimalloc_heap_stats() -> tsrs_core::memsplit::HeapStats {
    #[repr(C)]
    struct HeapArea {
        blocks: *mut std::ffi::c_void,
        reserved: usize,
        committed: usize,
        used: usize,
        block_size: usize,
        full_block_size: usize,
        reserved1: *mut std::ffi::c_void,
    }
    unsafe extern "C" {
        fn mi_heap_visit_blocks(
            heap: *mut std::ffi::c_void,
            visit_blocks: bool,
            visitor: extern "C" fn(
                *const std::ffi::c_void,
                *const HeapArea,
                *mut std::ffi::c_void,
                usize,
                *mut std::ffi::c_void,
            ) -> bool,
            arg: *mut std::ffi::c_void,
        ) -> bool;
    }
    extern "C" fn visit(
        _heap: *const std::ffi::c_void,
        area: *const HeapArea,
        _block: *mut std::ffi::c_void,
        _size: usize,
        arg: *mut std::ffi::c_void,
    ) -> bool {
        // SAFETY: mimalloc passes a valid area and our `arg`, a `HeapStats`.
        let (area, stats) = unsafe { (&*area, &mut *arg.cast::<tsrs_core::memsplit::HeapStats>()) };
        stats.pages += 1;
        stats.capacity += area.committed;
        stats.used += area.used * area.full_block_size;
        // The page's header lies in front of its blocks, inside the same 4 KiB page.
        let start = area.blocks.addr() & !4095;
        stats.resident +=
            tsrs_core::memsplit::resident(start, area.blocks.addr() + area.reserved - start);
        true
    }
    let mut stats = tsrs_core::memsplit::HeapStats::default();
    // SAFETY: a null heap is the main heap; the visitor only reads the areas. The callers' other threads are parked.
    unsafe { mi_heap_visit_blocks(std::ptr::null_mut(), false, visit, (&raw mut stats).cast()) };
    stats
}

#[cfg(not(feature = "system-alloc"))]
fn mimalloc_collect(force: bool) {
    unsafe extern "C" {
        fn mi_collect(force: bool);
    }
    // SAFETY: collects the calling thread's heap; no arguments to get wrong.
    #[cfg(not(miri))]
    unsafe {
        mi_collect(force);
    };
    #[cfg(miri)]
    let _ = force;
}

#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
unsafe extern "C" {
    fn mi_option_set(option: std::ffi::c_int, value: std::ffi::c_long);
}

// mimalloc v3's option numbers (`mimalloc.h`); `libmimalloc-sys2`'s constants are v2's (37 for allow_thp).
#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
const MI_OPTION_ALLOW_THP: std::ffi::c_int = 43;
#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
const MI_OPTION_ARENA_RESERVE: std::ffi::c_int = 23;

#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
static FAST_HEAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// `--fast` on Linux with glibc gives mimalloc's heap transparent huge pages too (notes/perf-fast-heap-thp.md; the
// build keeps them off for memory, notes/mem-no-thp.md). mimalloc fixes that policy in its own constructor (priority
// 101), before `main` can read the command line, so this priority-100 constructor sets the options first.
#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
#[used]
#[unsafe(link_section = ".init_array.00100")]
static FAST_HEAP_HUGE_PAGES: extern "C" fn(
    std::ffi::c_int,
    *const *const std::ffi::c_char,
    *const *const std::ffi::c_char,
) = fast_heap_huge_pages;

/// Reads `--fast` as the option parser does (any case, one or two dashes, the last one wins, `--fast false` turns it
/// off; otherwise `TSRS_FAST=1`), except inside response files. An explicit `MIMALLOC_ALLOW_THP` wins, and `--lsp`,
/// `--api` and `--lint` keep the build's policy. Nothing here may allocate: the first allocation starts mimalloc.
#[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
extern "C" fn fast_heap_huge_pages(
    argc: std::ffi::c_int,
    argv: *const *const std::ffi::c_char,
    envp: *const *const std::ffi::c_char,
) {
    let c_str = |p: *const std::ffi::c_char| {
        // SAFETY: glibc's argument and environment strings are NUL-terminated and live as long as the process.
        unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes()
    };
    let mut fast = false;
    let mut reserve_set = false;
    for i in 0.. {
        // SAFETY: glibc's environment vector is NULL-terminated; this stops at its end.
        let var = unsafe { *envp.add(i) };
        if var.is_null() {
            break;
        }
        let var = c_str(var);
        if var.starts_with(b"MIMALLOC_ALLOW_THP=") {
            return;
        }
        fast |= var == b"TSRS_FAST=1";
        reserve_set |= var.starts_with(b"MIMALLOC_ARENA_RESERVE=");
    }
    // SAFETY: glibc passes `argc` argument pointers.
    let args = unsafe { std::slice::from_raw_parts(argv, usize::try_from(argc).unwrap_or(0)) };
    let mut args = args.iter().skip(1).map(|&p| c_str(p)).peekable();
    if args.peek().is_some_and(|a| matches!(*a, b"--lsp" | b"--api" | b"--lint")) {
        return;
    }
    while let Some(arg) = args.next() {
        let Some(name) = arg.strip_prefix(b"-") else { continue };
        let name = name.strip_prefix(b"-").unwrap_or(name);
        if name.eq_ignore_ascii_case(b"fast") {
            fast = args.next_if(|v| *v == b"false").is_none();
        }
    }
    if fast {
        // SAFETY: sets one integer option; mimalloc reads it when it starts, right after this constructor.
        unsafe { mi_option_set(MI_OPTION_ALLOW_THP, 1) };
        if !reserve_set {
            // 64 MiB arenas (in KiB), not 1 GiB: the checkers' heap is mapped after the parse phase, with huge pages.
            // SAFETY: sets the arena reservation size; mimalloc reads it when it starts, right after this constructor.
            unsafe { mi_option_set(MI_OPTION_ARENA_RESERVE, 64 * 1024) };
        }
        // Relaxed: written before `main` runs, read by `main` on the same thread.
        FAST_HEAP.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(all(target_os = "linux", not(any(feature = "system-alloc", miri))))]
    tsrs_core::allocator::init_mimalloc();
    // Huge pages stay off while the parse threads allocate: their partly used heap pages would be resident as whole
    // 2 MiB pages, about 0.5 GiB however small the program (notes/perf-fast-heap-thp.md).
    #[cfg(all(target_os = "linux", target_env = "gnu", not(any(feature = "system-alloc", miri))))]
    // Relaxed: see the store in `fast_heap_huge_pages`.
    if FAST_HEAP.load(std::sync::atomic::Ordering::Relaxed) {
        // SAFETY: sets one integer option; mimalloc reads it on each later OS allocation.
        unsafe { mi_option_set(MI_OPTION_ALLOW_THP, 0) };
        // SAFETY: sets one integer option at parse end, before any checker thread starts.
        tsrs_core::allocator::at_parse_end(|| unsafe { mi_option_set(MI_OPTION_ALLOW_THP, 1) });
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--lint` is the type-aware linter that oxlint runs (`tsrs-lint`, tools/tsgolint/README.md). It returns before
    // any compiler command-line setup, so it runs exactly as the `tsrs-lint` binary does.
    if args.first().map(String::as_str) == Some("--lint") {
        return std::process::ExitCode::from(tsrs_lint::cli::main(args[1..].to_vec()) as u8);
    }
    #[cfg(not(feature = "system-alloc"))]
    tsrs_core::memsplit::set_heap_hooks(mimalloc_heap_stats, mimalloc_collect);
    // main.go:21: `--lsp` runs the language server (its threads have their own stacks); `--api` runs the
    // native API server (docs/NODE_API.md).
    if args.first().map(String::as_str) == Some("--lsp") {
        signals::notify_interrupts();
        return std::process::ExitCode::from(lsp::run_lsp(&args[1..]) as u8);
    }
    if args.first().map(String::as_str) == Some("--api") {
        // Checking recurses deeply; serve requests on a large stack like the command line below.
        let rest = args[1..].to_vec();
        signals::notify_interrupts();
        let status = std::thread::Builder::new()
            .stack_size(512 << 20)
            .spawn(move || api::run_api(&rest))
            .unwrap()
            .join()
            .unwrap_or(1);
        return std::process::ExitCode::from(status as u8);
    }
    // Type checking recurses deeply; run on a thread with a large stack
    // (Go's goroutine stacks grow on demand).
    let status = std::thread::Builder::new()
        .name("tsrs".to_string())
        .stack_size(512 << 20)
        .spawn(move || {
            let sys: &'static sys::OsSys = Box::leak(Box::new(sys::new_system()));
            let result = execute::command_line(sys, args);
            tsc::System::flush(sys);
            tsrs_compiler::dump_site_counts();
            result.status
        })
        .unwrap()
        .join()
        .unwrap_or(tsc::ExitStatus::NotImplemented);
    // TSRS_UNION_CACHE_STATS / TSRS_UNION_CACHE=shadow: the union front cache's totals.
    tsrs_compiler::Checker::union_cache_finish();
    // TSRS_INFER_MEMO_STATS / TSRS_INFER_MEMO=shadow: the inference memo's totals.
    tsrs_compiler::Checker::infer_memo_finish();
    tsrs_core::memsplit::report("exit");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    // Return through the C runtime so LLVM's atexit handler writes PGO profiles on Windows too.
    std::process::ExitCode::from(status as u8)
}
