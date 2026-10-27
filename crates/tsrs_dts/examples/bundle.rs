//! Bundle declarations from the command line and write them to disk. The benchmark in `bench/dts/` runs this
//! binary against rolldown-plugin-dts; the crate itself has no CLI.
//!
//! `bundle --config <tsconfig> --out-dir <dir> [--reuse-diagnostics] <name.d.ts>=<entry>...`
//!
//! One entry builds a single bundle; several build facades and a shared bundle. A JSON summary is printed to
//! stdout; diagnostics and errors go to stderr with exit status 1.
#[cfg(not(miri))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tsrs_dts::{BuildOptions, OutputMode};
use tsrs_vfs::FS;

struct Args {
    config: String,
    out_dir: PathBuf,
    reuse_diagnostics: bool,
    entries: BTreeMap<String, String>,
}

fn absolute(path: &str) -> Result<String, String> {
    let path = std::path::absolute(path).map_err(|error| format!("{path}: {error}"))?;
    Ok(tsrs_core::tspath::normalize_slashes(&path.to_string_lossy()))
}

fn parse() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let (mut config, mut out_dir, mut reuse_diagnostics) = (None, None, false);
    let mut entries = BTreeMap::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(absolute(&args.next().ok_or("--config needs a path")?)?),
            "--out-dir" => {
                out_dir = Some(PathBuf::from(args.next().ok_or("--out-dir needs a path")?))
            }
            "--reuse-diagnostics" => reuse_diagnostics = true,
            _ => {
                let (name, entry) =
                    arg.split_once('=').ok_or_else(|| format!("unexpected argument: {arg}"))?;
                entries.insert(name.to_owned(), absolute(entry)?);
            }
        }
    }
    if entries.is_empty() {
        return Err("usage: bundle --config <tsconfig> --out-dir <dir> [--reuse-diagnostics] <name.d.ts>=<entry>...".into());
    }
    Ok(Args {
        config: config.ok_or("--config is required")?,
        out_dir: out_dir.ok_or("--out-dir is required")?,
        reuse_diagnostics,
        entries,
    })
}

fn run() -> Result<String, String> {
    let args = parse()?;
    let fs: Arc<dyn FS> = Arc::new(tsrs_vfs::bundled::wrap_fs(tsrs_vfs::osvfs::fs()));
    let (file_name, entry) = args.entries.first_key_value().expect("at least one entry");
    let options = BuildOptions {
        config_path: args.config.clone(),
        entry: entry.clone(),
        output: OutputMode::Bundle { file_name: file_name.clone() },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: false,
        reuse_diagnostics: args.reuse_diagnostics,
    };
    let started = Instant::now();
    let result = if args.entries.len() == 1 {
        tsrs_dts::build(&options, fs)
    } else {
        tsrs_dts::build_entries(&options, &args.entries, fs)
    }
    .map_err(|error| error.to_string())?;
    let build_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut bytes = 0;
    for (name, text) in &result.files {
        let path = args.out_dir.join(Path::new(name));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
        bytes += text.len();
    }
    Ok(format!(
        r#"{{"build_ms":{build_ms:.3},"files":{},"bytes":{bytes},"emitted_declarations":{},"retained_statements":{},"removed_statements":{}}}"#,
        result.files.len(),
        result.emitted_declarations,
        result.retained_statements,
        result.removed_statements,
    ))
}

fn main() {
    #[cfg(target_os = "linux")]
    tsrs_core::allocator::init_mimalloc();
    // Checking recurses deeply; use the CLI's stack size.
    let result = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(run)
        .expect("spawn bundle thread")
        .join()
        .expect("bundle thread panicked");
    match result {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
