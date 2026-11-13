// Frees the process's memory after its caller has seen it exit (notes/perf-exit-teardown.md).
//
// When a process exits, the kernel tears its address space down inside `exit_group`, on one core, before the parent's
// `wait` returns: 10-75 ms for the bench catalog's checks, 2-5% of their wall time, most of it the mimalloc heap's
// 4 KiB pages. Just before a batch compile exits, `hand_off` starts a helper process that shares the address space
// (`clone` with `CLONE_VM`, without `CLONE_THREAD`). The process's own exit then only drops a reference to the memory,
// and the kernel tears it down when the helper exits a moment later, after the caller's `wait` has returned. Every
// checker thread ran in this process, so `wait4` still reports their CPU time and the peak RSS; only the teardown's
// system time moves to the helper, which init reaps.
//
// The helper shares memory and, without `CLONE_SETTLS`, the calling thread's thread pointer. It runs on a static
// stack of its own and makes only raw system calls (no libc, no Rust std, no allocation, nothing that can panic), so
// it reads and writes nothing but that stack: libc's `errno` and every other thread-local live behind the thread
// pointer. It starts with every signal blocked, so no handler of this process runs on its stack. It closes every
// descriptor except the read end of a pipe whose only write end this process holds, then blocks reading it; the
// kernel closes the write end in this process's exit, after every thread has dropped the memory (`do_exit` runs
// `exit_mm` before `exit_files`), and the helper exits. That holds however this process exits, by a signal too.
// Before `hand_off` returns, it waits for the helper to have closed its descriptors, so a pipe on stdout or stderr
// closes when this process exits, not when the helper does.

/// The `TSRS_EXIT_HELPER` setting: `0` turns the helper off, `1` starts it after every batch compile whatever the
/// mode and size; unset or anything else starts it in fast mode when the process is large enough to gain.
const ENV: &str = "TSRS_EXIT_HELPER";

/// The resident size below which a fast-mode exit starts no helper.
///
/// The helper pays at every size measured (`--version`, 7.5 MiB: 3.2 -> 2.8 ms), but below this are only runs that
/// load no default library (`--version`, `--help`, option errors), where it saves under half a millisecond; the
/// smallest check with the default library is 56 MiB and saves 3.7 ms (notes/perf-exit-teardown.md).
pub const MIN_RESIDENT: usize = 32 << 20;

/// Called on the main thread after a batch compile (not a watch session, `--lsp` or `--api`) has written and flushed
/// all of its output, right before the process exits. Returns whether it started a helper.
pub fn at_exit() -> bool {
    if !imp::SUPPORTED {
        return false;
    }
    match std::env::var_os(ENV).as_deref().and_then(std::ffi::OsStr::to_str) {
        Some("0") => return false,
        Some("1") => {}
        _ => {
            if !tsrs_core::fast::enabled() || tsrs_core::memsplit::process_memory() < MIN_RESIDENT {
                return false;
            }
        }
    }
    imp::hand_off()
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(any(feature = "system-alloc", miri))
)))]
mod imp {
    // Linux on other architectures needs its own `clone_helper` and `syscall3`; macOS and Windows have no way to share
    // an address space with another process (notes/perf-exit-teardown.md). Sanitizer builds (`system-alloc`)
    // instrument `helper`, and the instrumentation keeps per-thread state behind the thread pointer the helper shares;
    // Miri runs no inline assembly.
    pub(super) const SUPPORTED: bool = false;

    pub(super) const fn hand_off() -> bool {
        false
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(any(feature = "system-alloc", miri))
))]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(super) const SUPPORTED: bool = true;

    // The helper's frame and its calls use a few hundred bytes, unoptimized; this is many times that. It lies in .bss:
    // only the pages the helper touches become resident.
    const STACK_SIZE: usize = 16 << 10;

    #[repr(C, align(16))]
    struct Stack([u8; STACK_SIZE]);

    // Written only by the helper, through `&raw mut`; this process never reads it.
    static mut STACK: Stack = Stack([0; STACK_SIZE]);
    static STARTED: AtomicBool = AtomicBool::new(false);

    pub(super) fn hand_off() -> bool {
        // One helper per process: a second one would share the stack.
        if STARTED.swap(true, Ordering::SeqCst) {
            return false;
        }
        // The init process of a PID namespace (a container's command) does not finish exiting until every process in
        // the namespace is gone, the helper included: nothing to gain.
        if std::process::id() == 1 {
            return false;
        }
        let mut wake = [0; 2];
        let mut ready = [0; 2];
        // SAFETY: plain system calls on descriptors this function creates and owns. The signal mask is the calling
        // thread's and is restored before the block ends. `clone_helper`'s contract: `STARTED` makes this the only
        // helper, so the stack is unused, and `helper` touches nothing but it.
        unsafe {
            if libc::pipe2(wake.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
                return false;
            }
            if libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
                libc::close(wake[0]);
                libc::close(wake[1]);
                return false;
            }
            // The kernel's 64-bit signal set, every signal: libc's `sigfillset` and `pthread_sigmask` leave its own
            // internal signals out.
            let all = u64::MAX;
            let mut old = 0u64;
            let mask = |set: *const u64, old: *mut u64| {
                libc::syscall(
                    libc::SYS_rt_sigprocmask,
                    libc::SIG_SETMASK,
                    set,
                    old,
                    size_of::<u64>(),
                )
            };
            mask(&raw const all, &raw mut old);
            let top = (&raw mut STACK).cast::<u8>().add(STACK_SIZE);
            let pid = clone_helper(wake[0] as usize, top);
            mask(&raw const old, std::ptr::null_mut());
            libc::close(wake[0]);
            libc::close(ready[1]);
            if pid < 0 {
                libc::close(wake[1]);
                libc::close(ready[0]);
                return false;
            }
            // The helper's copy of `ready[1]` closes with the rest of its descriptors (or with the helper, if it
            // exits early): wait for that, so this process's stdout and stderr have no other holder when it exits.
            // The helper reaches it in a few system calls; the timeout covers a helper that is stopped.
            let mut poll = libc::pollfd { fd: ready[0], events: libc::POLLIN, revents: 0 };
            loop {
                if libc::poll(&raw mut poll, 1, 1000) >= 0
                    || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    break;
                }
            }
            libc::close(ready[0]);
        }
        // `wake[1]` stays open: the kernel closes it when this process exits, which wakes the helper.
        true
    }

    const CLONE_FLAGS: usize = libc::CLONE_VM as usize | libc::SIGCHLD as usize;
    const SYS_CLOSE_RANGE: usize = libc::SYS_close_range as usize;
    const SYS_READ: usize = libc::SYS_read as usize;

    /// The helper process. It holds the address space until this process has exited, then exits and frees it.
    ///
    /// It shares memory and the thread pointer with the thread that cloned it, so it must not use libc, Rust std,
    /// the heap, thread-locals or anything that can panic: it runs on `STACK` and makes only raw system calls.
    extern "C" fn helper(wake: usize) -> ! {
        // SAFETY: raw system calls with integer arguments and a pointer into this frame; none touches memory
        // outside `STACK`.
        unsafe {
            // Every descriptor but `wake` (which is never above `i32::MAX`): stdout, stderr and the ready pipe among them.
            // Without `close_range` (before Linux 5.9) the helper exits now, and this process frees its own memory.
            let below = wake == 0 || syscall3(SYS_CLOSE_RANGE, 0, wake.wrapping_sub(1), 0) == 0;
            if below && syscall3(SYS_CLOSE_RANGE, wake.wrapping_add(1), u32::MAX as usize, 0) == 0 {
                let mut byte = 0u8;
                // Returns 0 (end of file) once the last write end, in this process's descriptor table, is closed.
                while syscall3(SYS_READ, wake, (&raw mut byte).addr(), 1) == -(libc::EINTR as isize)
                {
                }
            }
            exit_group()
        }
    }

    /// Starts `helper(wake)` in a new process that shares this address space, on the stack that ends at `top`.
    /// Returns the helper's pid, or a negative errno.
    ///
    /// # Safety
    ///
    /// `top` is 16-byte aligned and ends a stack that nothing else uses for the helper's lifetime.
    #[cfg(target_arch = "x86_64")]
    unsafe fn clone_helper(wake: usize, top: *mut u8) -> isize {
        let ret: isize;
        // SAFETY: the parent only gets `rax` back and the `syscall` clobbers. The child (`rax` = 0) starts on `top`
        // with every other register as the parent left it, and never returns here: it calls `helper`, which exits.
        unsafe {
            std::arch::asm!(
                "syscall",
                "test rax, rax",
                "jnz 2f",
                "xor ebp, ebp",
                "mov rdi, r12",
                "call {helper}",
                "ud2",
                "2:",
                helper = sym helper,
                inlateout("rax") libc::SYS_clone as usize => ret,
                in("rdi") CLONE_FLAGS,
                in("rsi") top,
                in("rdx") 0usize, // parent tid
                in("r10") 0usize, // child tid
                in("r8") 0usize,  // tls
                in("r12") wake,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        ret
    }

    /// Starts `helper(wake)` in a new process that shares this address space, on the stack that ends at `top`.
    /// Returns the helper's pid, or a negative errno.
    ///
    /// # Safety
    ///
    /// `top` is 16-byte aligned and ends a stack that nothing else uses for the helper's lifetime.
    #[cfg(target_arch = "aarch64")]
    unsafe fn clone_helper(wake: usize, top: *mut u8) -> isize {
        let ret: isize;
        // SAFETY: the parent only gets `x0` back. The child (`x0` = 0) starts on `top` with every other register as
        // the parent left it, and never returns here: it calls `helper`, which exits.
        unsafe {
            std::arch::asm!(
                "svc 0",
                "cbnz x0, 2f",
                "mov x29, xzr",
                "mov x0, x9",
                "bl {helper}",
                "brk 1",
                "2:",
                helper = sym helper,
                inlateout("x0") CLONE_FLAGS => ret,
                in("x1") top,
                in("x2") 0usize, // parent tid
                in("x3") 0usize, // tls
                in("x4") 0usize, // child tid
                in("x8") libc::SYS_clone as usize,
                in("x9") wake,
            );
        }
        ret
    }

    /// A system call that does not touch `errno`: the result, or a negative errno.
    ///
    /// # Safety
    ///
    /// The call must be sound with these arguments.
    #[cfg(target_arch = "x86_64")]
    unsafe fn syscall3(number: usize, a: usize, b: usize, c: usize) -> isize {
        let ret: isize;
        // SAFETY: the caller's contract; `syscall` clobbers only `rcx` and `r11`.
        unsafe {
            std::arch::asm!(
                "syscall",
                inlateout("rax") number => ret,
                in("rdi") a,
                in("rsi") b,
                in("rdx") c,
                lateout("rcx") _,
                lateout("r11") _,
                options(nostack),
            );
        }
        ret
    }

    /// A system call that does not touch `errno`: the result, or a negative errno.
    ///
    /// # Safety
    ///
    /// The call must be sound with these arguments.
    #[cfg(target_arch = "aarch64")]
    unsafe fn syscall3(number: usize, a: usize, b: usize, c: usize) -> isize {
        let ret: isize;
        // SAFETY: the caller's contract; `svc` returns in `x0` and preserves every other register.
        unsafe {
            std::arch::asm!(
                "svc 0",
                inlateout("x0") a => ret,
                in("x1") b,
                in("x2") c,
                in("x8") number,
                options(nostack),
            );
        }
        ret
    }

    /// Ends the helper without running anything of this process's exit (no atexit handlers, no stdio flush).
    fn exit_group() -> ! {
        // SAFETY: `exit_group` takes an integer and does not return.
        unsafe {
            #[cfg(target_arch = "x86_64")]
            std::arch::asm!(
                "syscall",
                in("rax") libc::SYS_exit_group as usize,
                in("rdi") 0usize,
                options(noreturn, nostack),
            );
            #[cfg(target_arch = "aarch64")]
            std::arch::asm!(
                "svc 0",
                in("x0") 0usize,
                in("x8") libc::SYS_exit_group as usize,
                options(noreturn, nostack),
            );
        }
    }
}
