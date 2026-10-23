use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod regressions;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FLAGS: &[&str] = &["--noEmit", "--incremental", "false", "--pretty", "false"];
const ASSIGNMENTS: &[&str] = &["go", "locality", "default", "random"];
const REPETITIONS: usize = 3;
const USAGE: &str = "tsrs_determinism <plan|snapshot|check|regressions> [--projects all|a,b] [--tsrs BIN]\n\
    [--work-dir DIR] [--snapshots-dir DIR] [--out-dir DIR] [--timeout SECONDS]";

#[derive(Deserialize)]
struct Input {
    name: String,
    cwd: PathBuf,
    config: PathBuf,
    identity: Value,
    #[serde(default)]
    regression: bool,
}

#[derive(Deserialize)]
struct Reference {
    binary: PathBuf,
    provenance: Provenance,
}

#[derive(Clone, Deserialize, Serialize)]
struct Provenance {
    package: String,
    version: String,
    native_package: String,
    native_version: String,
    binary_sha256: String,
}

#[derive(Deserialize, Serialize)]
struct Snapshot {
    format_version: u32,
    typescript_commit: String,
    reference: Provenance,
    input: Value,
    exit_code: i32,
    diagnostics: Vec<String>,
}

#[derive(Serialize)]
struct Outcome {
    command: Vec<String>,
    exit_code: Option<i32>,
    failure: Option<String>,
    diagnostics: Vec<String>,
}

#[derive(Serialize)]
struct Case {
    project: String,
    label: String,
    passed: bool,
    failure: Option<String>,
}

struct Settings {
    root: PathBuf,
    work: PathBuf,
    snapshots: PathBuf,
    out: PathBuf,
    tsrs: PathBuf,
    timeout: Duration,
    max: usize,
    version: String,
    commit: String,
}

pub fn main(args: &[String]) -> Result<()> {
    let Some((mode, rest)) = args.split_first() else { return Err(USAGE.into()) };
    if matches!(mode.as_str(), "--help" | "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    if !matches!(mode.as_str(), "plan" | "snapshot" | "check" | "regressions") {
        return Err(USAGE.into());
    }
    let options = options(rest)?;
    let root = repo_root()?;
    let selected = options.get("--projects").map_or("all", String::as_str);
    let names = if mode == "regressions" {
        regressions::selection(&root, selected)?
    } else {
        selection(&root, selected)?
    };
    if mode == "plan" {
        println!("{}", serde_json::to_string(&names)?);
        return Ok(());
    }
    let version = cargo_pin(&root, "nightly")?;
    let package: Value = read_json(&root.join("package.json"))?;
    if package["devDependencies"]["typescript"] != version {
        return Err(
            "root typescript devDependency must exactly match Cargo.toml typescript.nightly".into(),
        );
    }
    let seconds = options.get("--timeout").map_or(Ok(900), |s| s.parse::<u64>())?;
    if seconds == 0 {
        return Err("--timeout must be positive".into());
    }
    let timeout = Duration::from_secs(seconds);
    if Instant::now().checked_add(timeout).is_none() {
        return Err("--timeout is too large".into());
    }
    let settings = Settings {
        work: option_path(&root, &options, "--work-dir", "bench/.work"),
        snapshots: option_path(&root, &options, "--snapshots-dir", "tasks/determinism/snapshots"),
        out: option_path(
            &root,
            &options,
            "--out-dir",
            if mode == "regressions" {
                "target/determinism/regressions"
            } else {
                "target/determinism"
            },
        ),
        tsrs: option_path(&root, &options, "--tsrs", "target/debug/tsrs"),
        timeout,
        max: std::thread::available_parallelism()?.get().min(256),
        commit: cargo_pin(&root, "commit")?,
        root,
        version,
    };
    if mode == "regressions" {
        regressions::run(&settings, &names)
    } else {
        run(&settings, mode, &names)
    }
}

fn options(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut args = args.iter();
    while let Some(name) = args.next() {
        if !matches!(
            name.as_str(),
            "--projects" | "--tsrs" | "--work-dir" | "--snapshots-dir" | "--out-dir" | "--timeout"
        ) {
            return Err(format!("unknown option {name}; {USAGE}").into());
        }
        let value = args
            .next()
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("missing value for {name}"))?;
        if result.insert(name.clone(), value.clone()).is_some() {
            return Err(format!("duplicate option {name}").into());
        }
    }
    Ok(result)
}

fn repo_root() -> Result<PathBuf> {
    for dir in std::env::current_dir()?.ancestors() {
        if dir.join("tasks/determinism/Cargo.toml").is_file()
            && dir.join("bench/projects.json").is_file()
        {
            return Ok(dir.canonicalize()?);
        }
    }
    Err("run from the tsrs repository or one of its subdirectories".into())
}

fn selection(root: &Path, selection: &str) -> Result<Vec<String>> {
    let catalog: Value = read_json(&root.join("bench/projects.json"))?;
    let projects = catalog["projects"].as_array().ok_or("missing benchmark projects")?;
    let all: Vec<String> = projects
        .iter()
        .map(|p| p["name"].as_str().map(str::to_owned).ok_or("missing project name"))
        .collect::<std::result::Result<_, _>>()?;
    let selected: BTreeSet<_> = selection.split(',').collect();
    if selection != "all"
        && (selected.len() != selection.split(',').count()
            || selected.iter().any(|n| !all.iter().any(|a| a == n)))
    {
        return Err("select unique benchmark names or all".into());
    }
    let names: Vec<_> =
        all.into_iter().filter(|n| selection == "all" || selected.contains(n.as_str())).collect();
    if names.is_empty() {
        return Err("no benchmark projects selected".into());
    }
    Ok(names)
}

fn cargo_pin(root: &Path, key: &str) -> Result<String> {
    let cargo = fs::read_to_string(root.join("Cargo.toml"))?;
    let table = cargo
        .split("[workspace.metadata.typescript]")
        .nth(1)
        .ok_or("missing TypeScript metadata")?;
    table
        .lines()
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
        })
        .ok_or_else(|| format!("missing TypeScript {key}").into())
}

fn option_path(
    root: &Path,
    options: &BTreeMap<String, String>,
    name: &str,
    default: &str,
) -> PathBuf {
    root.join(options.get(name).map_or(default, String::as_str))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn helper<T: serde::de::DeserializeOwned>(
    settings: &Settings,
    mode: &str,
    names: &[String],
) -> Result<T> {
    let output = Command::new("node")
        .arg(settings.root.join("tasks/determinism/setup.mjs"))
        .arg(mode)
        .arg(&settings.work)
        .arg(names.join(","))
        .output()?;
    if !output.status.success() {
        return Err(format!("setup {mode} failed: {}; install root dependencies with pnpm install --frozen-lockfile", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn run(settings: &Settings, mode: &str, names: &[String]) -> Result<()> {
    fs::create_dir_all(&settings.out)?;
    let inputs: Vec<Input> = helper(settings, "inputs", names)?;
    let reference = if mode == "snapshot" {
        let reference: Reference = helper(settings, "reference", names)?;
        validate_provenance(&reference.provenance, &settings.version)?;
        let output = Command::new(&reference.binary).arg("--version").output()?;
        if !output.status.success()
            || String::from_utf8_lossy(&output.stdout).trim()
                != format!("Version {}", settings.version)
        {
            return Err(
                "root native TypeScript compiler --version does not match Cargo.toml".into()
            );
        }
        Some(reference)
    } else {
        None
    };
    println!("determinism: {} projects, max={} checkers", inputs.len(), settings.max);
    let mut cases = Vec::new();
    summary(settings, mode, inputs.len(), &cases)?;
    for input in &inputs {
        if let Err(error) = prepare(settings, input) {
            report(&mut cases, input, "prepare", Some(error.to_string()));
        } else if let Some(reference) = &reference {
            record_snapshot(settings, input, reference, &mut cases)?;
        } else {
            check_project(settings, input, &mut cases)?;
        }
        summary(settings, mode, inputs.len(), &cases)?;
    }
    let failed = cases.iter().filter(|case| !case.passed).count();
    println!("determinism: {} cases, {failed} failed; {}", cases.len(), settings.out.display());
    if failed > 0 {
        return Err(format!("{failed} cases failed").into());
    }
    Ok(())
}

fn prepare(settings: &Settings, input: &Input) -> Result<()> {
    let directory = settings.out.join(&input.name);
    fs::create_dir_all(&directory)?;
    let log = fs::File::create(directory.join("prepare.log"))?;
    println!("preparing {}", input.name);
    let status = Command::new("node")
        .arg(settings.root.join("bench/bench.mjs"))
        .args(["prepare", "--projects", &input.name, "--work-dir"])
        .arg(&settings.work)
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .status()?;
    if !status.success() {
        return Err(format!(
            "preparation exited {status}; see {}/prepare.log",
            directory.display()
        )
        .into());
    }
    Ok(())
}

fn summary(settings: &Settings, mode: &str, projects: usize, cases: &[Case]) -> Result<()> {
    let per_project = match mode {
        "check" => ASSIGNMENTS.len() * 2 * REPETITIONS,
        "regressions" => 1 + 2 * (REPETITIONS + regressions::SEEDS),
        _ => 1,
    };
    write_json(
        &settings.out.join("summary.json"),
        &serde_json::json!({
            "mode": mode,
            "max_checkers": settings.max,
            "expected_cases": projects * per_project,
            "cases": cases,
        }),
    )
}

fn validate_provenance(reference: &Provenance, version: &str) -> Result<()> {
    if reference.package != "typescript"
        || reference.version != version
        || reference.native_version != version
        || !reference.native_package.starts_with("@typescript/typescript-")
        || reference.binary_sha256.len() != 64
        || !reference.binary_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("reference metadata does not match the pinned npm TypeScript release".into());
    }
    Ok(())
}

fn record_snapshot(
    settings: &Settings,
    input: &Input,
    reference: &Reference,
    cases: &mut Vec<Case>,
) -> Result<()> {
    let outcome =
        invoke(settings, input, &reference.binary, &["--singleThreaded".into()], "snapshot")?;
    let failure = outcome.failure.clone();
    if failure.is_none() {
        let snapshot = Snapshot {
            format_version: 1,
            typescript_commit: settings.commit.clone(),
            reference: reference.provenance.clone(),
            input: input.identity.clone(),
            exit_code: outcome.exit_code.ok_or("missing snapshot exit code")?,
            diagnostics: outcome.diagnostics,
        };
        write_json(&settings.snapshots.join(format!("{}.json", input.name)), &snapshot)?;
    }
    report(cases, input, "snapshot", failure);
    Ok(())
}

fn load_snapshot(settings: &Settings, input: &Input) -> Result<Snapshot> {
    let path = settings.snapshots.join(format!("{}.json", input.name));
    let snapshot: Snapshot = read_json(&path)
        .map_err(|e| format!("{}: {e}; run snapshot explicitly", path.display()))?;
    validate_provenance(&snapshot.reference, &settings.version)?;
    if snapshot.format_version != 1
        || snapshot.typescript_commit != settings.commit
        || snapshot.input != input.identity
        || !(0..=2).contains(&snapshot.exit_code)
    {
        return Err(
            format!("{} is stale or invalid; regenerate with snapshot", path.display()).into()
        );
    }
    Ok(snapshot)
}

fn check_project(settings: &Settings, input: &Input, cases: &mut Vec<Case>) -> Result<()> {
    let snapshot = match load_snapshot(settings, input) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            report(cases, input, "snapshot", Some(error.to_string()));
            return Ok(());
        }
    };
    for &assignment in ASSIGNMENTS {
        for (count_label, count) in [("4", 4), ("max", settings.max)] {
            for repetition in 1..=REPETITIONS {
                let label = format!("{assignment}-{count_label}-{repetition}");
                let mut flags = vec!["--checkers".into(), count.to_string()];
                if assignment != "default" {
                    flags.push("--checkerAssignment".into());
                    flags.push(if assignment == "random" {
                        format!("random:{repetition}")
                    } else {
                        assignment.into()
                    });
                }
                let mut outcome = invoke(settings, input, &settings.tsrs, &flags, &label)?;
                let mut failure = outcome.failure.clone();
                if failure.is_none()
                    && (outcome.exit_code != Some(snapshot.exit_code)
                        || outcome.diagnostics != snapshot.diagnostics)
                {
                    let diff = diagnostic_diff(&snapshot, &outcome);
                    fs::write(settings.out.join(&input.name).join(format!("{label}.diff")), &diff)?;
                    failure = Some(format!("diagnostics or exit status differ; see {label}.diff"));
                    outcome.failure.clone_from(&failure);
                    write_json(
                        &settings.out.join(&input.name).join(format!("{label}.json")),
                        &outcome,
                    )?;
                }
                report(cases, input, &label, failure);
            }
        }
    }
    Ok(())
}

fn report(cases: &mut Vec<Case>, input: &Input, label: &str, failure: Option<String>) {
    println!(
        "{} {} {label}{}",
        if failure.is_some() { "FAIL" } else { "pass" },
        input.name,
        failure.as_ref().map_or_else(String::new, |s| format!(": {s}"))
    );
    cases.push(Case {
        project: input.name.clone(),
        label: label.into(),
        passed: failure.is_none(),
        failure,
    });
}

fn diagnostic_diff(snapshot: &Snapshot, outcome: &Outcome) -> String {
    let expected = snapshot.diagnostics.join("\n") + "\n";
    let actual = outcome.diagnostics.join("\n") + "\n";
    format!(
        "exit: expected {}, actual {:?}\n{}",
        snapshot.exit_code,
        outcome.exit_code,
        similar::TextDiff::from_lines(&expected, &actual)
            .unified_diff()
            .context_radius(3)
            .header("expected", "actual")
    )
}

fn invoke(
    settings: &Settings,
    input: &Input,
    binary: &Path,
    flags: &[String],
    label: &str,
) -> Result<Outcome> {
    let directory = settings.out.join(&input.name);
    fs::create_dir_all(&directory)?;
    match fs::remove_file(directory.join(format!("{label}.diff"))) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let log_path = directory.join(format!("{label}.log"));
    let log = fs::File::create(&log_path)?;
    let args: Vec<String> = [
        vec!["-p".into(), input.config.to_string_lossy().into_owned()],
        if input.regression { &["--pretty", "false"][..] } else { FLAGS }
            .iter()
            .map(|s| (*s).into())
            .collect(),
        flags.to_vec(),
    ]
    .concat();
    let mut command = Command::new(binary);
    command
        .args(&args)
        .current_dir(&input.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["TSRS_", "TS_TEST_", "RAYON_", "MIMALLOC_", "MALLOC_", "Malloc"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            || ["GOGC", "GOMEMLIMIT", "GOMAXPROCS", "GODEBUG"].contains(&name.as_ref())
        {
            command.env_remove(key);
        }
    }
    command
        .env("RAYON_NUM_THREADS", settings.max.to_string())
        .env("GOMAXPROCS", settings.max.to_string());
    let mut outcome = Outcome {
        command: std::iter::once(binary.to_string_lossy().into_owned()).chain(args).collect(),
        exit_code: None,
        failure: None,
        diagnostics: Vec::new(),
    };
    match command.spawn() {
        Err(error) => outcome.failure = Some(format!("launch failed: {error}")),
        Ok(mut child) => {
            let deadline = Instant::now() + settings.timeout;
            loop {
                if let Some(status) = child.try_wait()? {
                    outcome.exit_code = status.code();
                    if !status.code().is_some_and(|code| (0..=2).contains(&code)) {
                        outcome.failure = Some(format!("abnormal exit: {status}"));
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    if let Err(error) = child.kill()
                        && child.try_wait()?.is_none()
                    {
                        return Err(error.into());
                    }
                    child.wait()?;
                    outcome.failure =
                        Some(format!("timeout after {} seconds", settings.timeout.as_secs()));
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let bytes = fs::read(log_path)?;
    let output = String::from_utf8_lossy(&bytes);
    if matches!(output, std::borrow::Cow::Owned(_)) {
        outcome.failure = Some("compiler output contains invalid UTF-8".into());
    }
    if output.lines().any(|line| {
        line.starts_with("panic: ")
            || line.starts_with("fatal error: ")
            || (line.starts_with("thread '") && line.contains("panicked at"))
    }) {
        outcome.failure = Some("compiler panic".into());
    }
    outcome.diagnostics = diagnostics(&normalize(settings, &output)?);
    if outcome.failure.is_none() && outcome.exit_code != Some(0) && outcome.diagnostics.is_empty() {
        outcome.failure = Some("nonzero exit without TypeScript diagnostics".into());
    }
    write_json(&directory.join(format!("{label}.json")), &outcome)?;
    Ok(outcome)
}

fn normalize(settings: &Settings, output: &str) -> Result<String> {
    let mut text = output.replace("\r\n", "\n");
    for (path, placeholder) in [(&settings.work, "<work>"), (&settings.root, "<repo>")] {
        text = text.replace(path.to_string_lossy().as_ref(), placeholder);
        if path.exists() {
            text = text.replace(path.canonicalize()?.to_string_lossy().as_ref(), placeholder);
        }
    }
    Ok(text)
}

fn diagnostics(output: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut inside = false;
    for line in output.lines() {
        if line.starts_with("error TS") || line.contains(": error TS") {
            result.push(line.to_owned());
            inside = true;
        } else if inside && line.starts_with([' ', '\t']) && !line.trim().is_empty() {
            result.push(line.to_owned());
        } else {
            inside = false;
        }
    }
    result
}
