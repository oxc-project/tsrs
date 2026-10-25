use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Result, cargo_pin, option_path};

pub(super) const OPTIONS: &[&str] = &[
    "--mode",
    "--tsrs-test",
    "--testdata",
    "--out-dir",
    "--list",
    "--filter",
    "--jobs",
    "--default-reps",
    "--seeds",
    "--timeout",
];
const FOCUSED: &str = "testdata/checker-determinism.txt";
const CHECKERS: [usize; 2] = [4, 8];

enum History {
    Go,
    Default { checkers: Option<usize>, assignment: Option<String> },
}

struct Settings {
    root: PathBuf,
    tsrs_test: PathBuf,
    testdata: PathBuf,
    args: Vec<String>,
    narrowed: bool,
}

pub(super) fn run(root: &Path, options: &BTreeMap<String, String>) -> Result<()> {
    let mode = options.get("--mode").map_or("all", String::as_str);
    if !matches!(mode, "all" | "go" | "default" | "determinism") {
        return Err("--mode must be all, go, default or determinism".into());
    }
    let repetitions = positive(options, "--default-reps", 10)?;
    let seeds = positive(options, "--seeds", 8)?;
    let timeout = positive(options, "--timeout", 120)?;
    let results = option_path(root, options, "--out-dir", "target/test-results");
    let mut args: Vec<String> =
        ["run", "--suite", "all", "--baselines", "all", "--timeout"].map(String::from).into();
    args.push(timeout.to_string());
    if let Some(list) = options.get("--list") {
        args.extend(["--list".into(), root.join(list).to_string_lossy().into_owned()]);
    }
    for name in ["--filter", "--jobs"] {
        if let Some(value) = options.get(name) {
            args.extend([name.into(), value.clone()]);
        }
    }
    let settings = Settings {
        root: root.to_path_buf(),
        tsrs_test: option_path(root, options, "--tsrs-test", "target/debug/tsrs-test"),
        testdata: option_path(root, options, "--testdata", "ts-ref/tsc/testdata"),
        narrowed: options.contains_key("--list") || options.contains_key("--filter"),
        args,
    };
    check_pin(root, &settings.testdata)?;

    let mut failed = Vec::new();
    let mut runs = 0;
    let mut run = |label: String, directory: PathBuf, history: History| -> Result<()> {
        runs += 1;
        if !invoke(&settings, &label, &directory, history)? {
            failed.push(label);
        }
        Ok(())
    };
    if matches!(mode, "all" | "go") {
        run("go".into(), results.clone(), History::Go)?;
    }
    if matches!(mode, "all" | "default") {
        let history = History::Default { checkers: None, assignment: None };
        run("default".into(), suffixed(&results, "-default"), history)?;
    }
    if matches!(mode, "all" | "determinism") {
        let sweep = suffixed(&results, "-determinism");
        for checkers in CHECKERS {
            let variants = (1..=repetitions)
                .map(|rep| (format!("default-{checkers}-{rep}"), None))
                .chain((1..=seeds).map(|seed| {
                    (format!("random-{checkers}-{seed}"), Some(format!("random:{seed}")))
                }));
            for (label, assignment) in variants {
                let directory = sweep.join(&label);
                run(label, directory, History::Default { checkers: Some(checkers), assignment })?;
            }
        }
    }
    println!("conformance: {runs} runs, {} failed; results in {}", failed.len(), results.display());
    if !failed.is_empty() {
        return Err(format!("conformance runs failed: {}", failed.join(", ")).into());
    }
    Ok(())
}

fn invoke(settings: &Settings, label: &str, results: &Path, history: History) -> Result<bool> {
    println!("conformance: {label}");
    let mut command = Command::new(&settings.tsrs_test);
    command
        .args(&settings.args)
        .current_dir(&settings.root)
        .env("TSRS_TESTDATA", &settings.testdata)
        .env("TSRS_TEST_RESULTS", results);
    match history {
        History::Go => {
            command
                .env("TSRS_FAST", "0")
                .env("TS_TEST_PROGRAM_SINGLE_THREADED", "true")
                .env("TSRS_HISTORY", "go")
                .env("TSRS_CHECKER_ASSIGNMENT", "go");
        }
        History::Default { checkers, assignment } => {
            command
                .env("TSRS_FAST", "1")
                .env("TS_TEST_PROGRAM_SINGLE_THREADED", "false")
                .env("TSRS_HISTORY", "canonical");
            match assignment {
                Some(assignment) => command.env("TSRS_CHECKER_ASSIGNMENT", assignment),
                None => command.env_remove("TSRS_CHECKER_ASSIGNMENT"),
            };
            if let Some(checkers) = checkers {
                command.args(["--checkers".into(), checkers.to_string()]);
                if !settings.narrowed {
                    command.arg("--list").arg(settings.root.join(FOCUSED));
                }
            }
        }
    }
    let status =
        command.status().map_err(|error| format!("{}: {error}", settings.tsrs_test.display()))?;
    if !status.success() {
        eprintln!("conformance: {label} failed ({status})");
    }
    Ok(status.success())
}

fn check_pin(root: &Path, testdata: &Path) -> Result<()> {
    let commit = cargo_pin(root, "commit")?;
    let output =
        Command::new("git").arg("-C").arg(testdata).args(["rev-parse", "HEAD"]).output()?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != commit {
        return Err(format!(
            "{} is not a checkout of the TypeScript source pin {commit}",
            testdata.display()
        )
        .into());
    }
    Ok(())
}

fn positive(options: &BTreeMap<String, String>, name: &str, default: usize) -> Result<usize> {
    match options.get(name).map_or(Ok(default), |value| value.parse::<usize>()) {
        Ok(value) if value > 0 => Ok(value),
        _ => Err(format!("{name} must be a positive integer").into()),
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(suffix);
    PathBuf::from(name)
}
