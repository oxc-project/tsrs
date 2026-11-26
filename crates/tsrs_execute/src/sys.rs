// Port of cmd/tsc/sys.go (osSys).

use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsrs_core::tspath;
use tsrs_vfs::{FS, bundled, osvfs};

use crate::tsc::System;

// Go's os.Stdout parks the goroutine on EAGAIN until the fd is writable, so fmt.Fprint appears to block.
// Rust returns WouldBlock on a non-blocking stdout pipe; poll until writable and retry so output is not dropped.
#[derive(Debug)]
struct StdoutWriter(std::io::Stdout);

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[cfg(unix)]
        {
            use std::io::ErrorKind;

            loop {
                match self.0.write(buf) {
                    Ok(n) => return Ok(n),
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => poll_out(&self.0)?,
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(not(unix))]
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::io::ErrorKind;

            loop {
                match self.0.flush() {
                    Ok(()) => return Ok(()),
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => poll_out(&self.0)?,
                    Err(e) => return Err(e),
                }
            }
        }
        #[cfg(not(unix))]
        self.0.flush()
    }
}

#[cfg(unix)]
fn poll_out(fd: &impl std::os::fd::AsFd) -> std::io::Result<()> {
    use rustix::event::{PollFd, PollFlags, poll};

    let mut fds = [PollFd::new(fd, PollFlags::OUT)];
    loop {
        match poll(&mut fds, None) {
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(err) => return Err(err.into()),
        }
    }
}

pub struct OsSys {
    writer: Mutex<std::io::BufWriter<StdoutWriter>>,
    fs: Arc<dyn FS>,
    default_library_path: String,
    cwd: String,
    start: Instant,
}

impl System for OsSys {
    fn since_start(&self) -> Duration {
        self.start.elapsed()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn fs(&self) -> Arc<dyn FS> {
        Arc::clone(&self.fs)
    }

    fn default_library_path(&self) -> &str {
        &self.default_library_path
    }

    fn get_current_directory(&self) -> &str {
        &self.cwd
    }

    fn write(&self, text: &str) {
        exit_on_broken_pipe(&self.writer.lock().unwrap().write_all(text.as_bytes()));
    }

    fn flush(&self) {
        exit_on_broken_pipe(&self.writer.lock().unwrap().flush());
    }

    fn write_output_is_tty(&self) -> bool {
        std::io::stdout().is_terminal()
    }

    fn get_environment_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    // sys.go:68
    fn spawn(
        &self,
        command: &[String],
        dir: &str,
        stderr: Option<Box<dyn Write + Send>>,
    ) -> Result<crate::tsc::ReadWriteCloser, String> {
        tsrs_contentmapper::spawn_process(command, dir, stderr)
    }
}

// Go's sys.Writer() is os.Stdout, and the Go runtime ends a program whose write to file descriptor 1 or 2 fails
// with EPIPE by SIGPIPE (os/signal: "If the program is not calling signal.Notify ... a write to a broken pipe on file
// descriptors 1 or 2 will cause the program to exit with a SIGPIPE signal"), so `tsc | head` stops tsc with status
// 141. Rust ignores SIGPIPE; this restores the default action and raises it. Other write errors are ignored, as Go's
// fmt.Fprint callers do.
fn exit_on_broken_pipe(result: &std::io::Result<()>) {
    if !matches!(result, Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe) {
        return;
    }
    #[cfg(unix)]
    // SAFETY: signal and raise take integer arguments only; the default action of SIGPIPE ends the process.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        libc::raise(libc::SIGPIPE);
    }
    std::process::exit(crate::tsc::ExitStatus::DiagnosticsPresentOutputsSkipped as i32);
}

pub fn new_system() -> OsSys {
    let cwd = match osvfs::getwd() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("Error getting current directory: {err}");
            std::process::exit(crate::tsc::ExitStatus::InvalidProjectOutputsSkipped as i32);
        }
    };

    OsSys {
        cwd: tspath::normalize_path(&cwd),
        fs: Arc::new(bundled::wrap_fs(osvfs::fs())),
        default_library_path: bundled::default_lib_path(),
        writer: Mutex::new(std::io::BufWriter::new(StdoutWriter(std::io::stdout()))),
        start: Instant::now(),
    }
}
