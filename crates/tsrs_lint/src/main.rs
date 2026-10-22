#![expect(
    clippy::multiple_crate_versions,
    reason = "regress 0.11 uses hashbrown 0.16; the compiler uses hashbrown 0.17"
)]

use std::io::{self, Read, Write};

use tsrs_lint::{engine, payload, protocol};

#[cfg(not(miri))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

fn main() {
    #[cfg(all(target_os = "linux", not(miri)))]
    tsrs_core::allocator::init_mimalloc();
    let result = std::thread::Builder::new()
        .name("tsrs-lint".into())
        .stack_size(512 * 1024 * 1024)
        .spawn(run);
    let code = match result {
        Ok(thread) => {
            if let Ok(code) = thread.join() {
                code
            } else {
                eprintln!("tsrs-lint panicked");
                1
            }
        }
        Err(error) => {
            eprintln!("could not start tsrs-lint: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> i32 {
    let mut options = engine::Options {
        suppress_program_diagnostics: std::env::var(
            "OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS",
        )
        .is_ok_and(|value| value == "true"),
        ..Default::default()
    };
    let mut headless = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "headless" | "--headless" => headless = true,
            "-fix" | "--fix" => options.fix = true,
            "-fix-suggestions" | "--fix-suggestions" => options.fix_suggestions = true,
            "-debug" | "--debug" => {
                if args.next().as_deref() == Some("timings") {
                    options.debug_timings = true;
                } else {
                    eprintln!("--debug requires timings");
                    return 1;
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: tsrs-lint headless [--fix] [--fix-suggestions] [--debug timings]"
                );
                return 0;
            }
            _ => {
                eprintln!("unknown option: {arg}");
                return 1;
            }
        }
    }
    if !headless {
        eprintln!("tsrs-lint requires headless");
        return 1;
    }
    let mut writer = io::BufWriter::new(io::stdout().lock());
    let result = (|| {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("error reading from stdin: {e}"))?;
        let payload = payload::deserialize_payload(&input)
            .map_err(|e| format!("error parsing config: {e}"))?;
        let cwd =
            std::env::current_dir().map_err(|e| format!("error getting current directory: {e}"))?;
        engine::run(&payload, options, &cwd.to_string_lossy(), &mut writer)
            .map_err(|e| format!("error running linter: {e}"))
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            if let Err(write_error) = protocol::write_error(&mut writer, &error) {
                eprintln!("could not write error: {write_error}");
            }
            1
        }
    };
    if let Err(error) = writer.flush() {
        eprintln!("could not flush output: {error}");
        return 1;
    }
    code
}
