use codspeed::codspeed::CodSpeed;
use codspeed::walltime_results::WalltimeBenchmark;
use std::process::{Command, Stdio};
use std::time::Instant;
use tsrs_bench::{BenchmarkSystem, Compilation, default_invocation};

#[global_allocator]
static ALLOCATOR: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

// Walltime: ten one-shot compilations in child processes after the hooked one (a compilation keeps its arenas
// until exit, so sampling in one process would stack them). One sample gave CodSpeed no stdev or p-value.
const WALLTIME_SAMPLES: usize = 10;
const SAMPLE_ARG: &str = "--tsrs-walltime-sample";
const SAMPLE_PREFIX: &str = "tsrs-walltime-sample-ns=";

#[inline(never)]
#[expect(non_snake_case, reason = "CodSpeed recognizes this prefix as the profiling root")]
fn __codspeed_root_frame__compile(sys: &'static BenchmarkSystem, args: Vec<String>) -> Compilation {
    sys.compile(args)
}

/// Compilation times of `WALLTIME_SAMPLES` child processes, in nanoseconds.
fn walltime_samples() -> Vec<u128> {
    let exe = std::env::current_exe().expect("the benchmark's own path");
    (0..WALLTIME_SAMPLES)
        .map(|i| {
            let output = Command::new(&exe)
                .arg(SAMPLE_ARG)
                .stdin(Stdio::null())
                .stderr(Stdio::inherit())
                .output()
                .expect("start a walltime sample");
            assert!(output.status.success(), "walltime sample {i} failed: {}", output.status);
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .rev()
                .find_map(|line| line.strip_prefix(SAMPLE_PREFIX))
                .and_then(|ns| ns.trim().parse().ok())
                .unwrap_or_else(|| panic!("walltime sample {i} printed no time:\n{stdout}"))
        })
        .collect()
}

fn main() {
    #[cfg(all(target_os = "linux", not(miri)))]
    tsrs_core::allocator::init_mimalloc();
    for (name, _) in std::env::vars_os() {
        assert!(
            !name.as_encoded_bytes().starts_with(b"TSRS_") || name == "TSRS_BENCH_PROJECT",
            "unset compiler environment override {} before benchmarking",
            name.display()
        );
    }
    assert!(
        std::env::var_os("RAYON_NUM_THREADS").is_none(),
        "unset RAYON_NUM_THREADS to use the CLI's default parallelism"
    );
    let name = std::env::var("TSRS_BENCH_PROJECT")
        .expect("set TSRS_BENCH_PROJECT to the project to benchmark");
    // cargo-codspeed passes --bench only in walltime mode.
    let walltime = std::env::args_os().any(|arg| arg == "--bench");
    let sample = std::env::args_os().any(|arg| arg == SAMPLE_ARG);
    // Match the CLI's stack. The CodSpeed hooks measure exactly one compilation on this thread.
    std::thread::Builder::new()
        .name("tsrs-bench".to_string())
        .stack_size(512 << 20)
        .spawn(move || {
            let (sys, args) = default_invocation(&name);
            if sample {
                // A walltime sample: compile once, print the time.
                let start = Instant::now();
                let result = sys.compile(args);
                let elapsed = start.elapsed();
                result.assert_completed();
                println!("{SAMPLE_PREFIX}{}", elapsed.as_nanos());
                return;
            }
            let mut codspeed = CodSpeed::new();
            let uri = format!("{}::compiler_default::{name}", file!());
            codspeed.start_benchmark(&uri);
            let result = __codspeed_root_frame__compile(sys, args);
            codspeed.end_benchmark();
            result.assert_completed();
            if walltime {
                let times = walltime_samples();
                WalltimeBenchmark::collect_raw_walltime_results(
                    "tsrs",
                    format!("compiler_default::{name}"),
                    uri,
                    vec![1; times.len()],
                    times,
                    None,
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
