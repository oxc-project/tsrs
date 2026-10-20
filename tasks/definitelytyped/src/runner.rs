use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::api::Result;
use crate::check::{CompilerOutcome, PackageResult, check_package};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    dt_commit: String,
    typescript_commit: String,
    packages: Vec<String>,
}

pub fn main(args: &[String]) -> Result<()> {
    let Some((command, rest)) = args.split_first() else {
        return Err(usage().into());
    };
    match command.as_str() {
        "plan" => plan(rest),
        "run" => run(rest),
        "worker" => worker(rest),
        "report" => report(rest),
        "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage().into()),
    }
}

fn usage() -> &'static str {
    "tsrs_definitelytyped plan --dt DIR --out manifest.json [--package NAME]\n\
     tsrs_definitelytyped run --manifest manifest.json --dt DIR --tsgo BIN --tsrs BIN --shard INDEX/COUNT --jobs N --out DIR\n\
     tsrs_definitelytyped report --manifest manifest.json --results DIR --out report.md"
}

fn option<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    let position =
        args.iter().position(|arg| arg == name).ok_or_else(|| format!("missing {name}"))?;
    args.get(position + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for {name}").into())
}

fn optional<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|position| args.get(position + 1))
        .map(String::as_str)
}

fn canonical(path: &str) -> Result<PathBuf> {
    Ok(Path::new(path).canonicalize()?)
}

fn git_commit(path: &Path) -> Result<String> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(path).output()?;
    if !output.status.success() {
        return Err(format!("git rev-parse failed in {}", path.display()).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn pinned_typescript_commit() -> Result<String> {
    let manifest = include_str!("../../../Cargo.toml");
    let section =
        manifest.split("[workspace.metadata.typescript]").nth(1).ok_or("TypeScript pin absent")?;
    let line = section
        .lines()
        .find(|line| line.trim_start().starts_with("commit = "))
        .ok_or("TypeScript commit absent")?;
    Ok(line.split('"').nth(1).ok_or("invalid TypeScript commit")?.to_string())
}

fn discover(dt: &Path) -> Result<Vec<String>> {
    let types = dt.join("types");
    if !types.is_dir() {
        return Err(format!("{} has no types directory", dt.display()).into());
    }
    let mut packages = BTreeSet::new();
    for entry in fs::read_dir(types)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        packages.insert(name.clone());
        for version in fs::read_dir(entry.path())? {
            let version = version?;
            if !version.file_type()?.is_dir() {
                continue;
            }
            let dir_name = version.file_name().to_string_lossy().into_owned();
            if is_version_directory(&dir_name) {
                packages.insert(format!("{name}/{dir_name}"));
            }
        }
    }
    Ok(packages.into_iter().collect())
}

fn is_version_directory(name: &str) -> bool {
    let Some(version) = name.strip_prefix('v') else { return false };
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or("");
    let minor = parts.next();
    !major.is_empty()
        && major.bytes().all(|b| b.is_ascii_digit())
        && minor.is_none_or(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
}

fn plan(args: &[String]) -> Result<()> {
    let dt = canonical(option(args, "--dt")?)?;
    let out = Path::new(option(args, "--out")?);
    let mut packages = discover(&dt)?;
    if let Some(package) = optional(args, "--package") {
        if !packages.iter().any(|name| name == package) {
            return Err(format!("package {package} not found").into());
        }
        packages.retain(|name| name == package);
    }
    if packages.is_empty() {
        return Err("DefinitelyTyped checkout contains no packages".into());
    }
    let manifest = Manifest {
        dt_commit: git_commit(&dt)?,
        typescript_commit: pinned_typescript_commit()?,
        packages,
    };
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, serde_json::to_vec_pretty(&manifest)?)?;
    println!("planned {} packages at {}", manifest.packages.len(), manifest.dt_commit);
    Ok(())
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn shard_packages(manifest: &Manifest, index: usize, count: usize) -> Vec<(usize, String)> {
    let mut groups = BTreeMap::<String, Vec<(usize, String)>>::new();
    for (package_index, name) in manifest.packages.iter().enumerate() {
        groups
            .entry(name.split('/').next().unwrap_or("").to_string())
            .or_default()
            .push((package_index, name.clone()));
    }
    groups
        .into_values()
        .enumerate()
        .filter(|(group, _)| group % count == index)
        .flat_map(|(_, packages)| packages)
        .collect()
}

fn parse_shard(value: &str) -> Result<(usize, usize)> {
    let (index, count) = value.split_once('/').ok_or("shard must be INDEX/COUNT")?;
    let index = index.parse::<usize>()?;
    let count = count.parse::<usize>()?;
    if count == 0 || index >= count {
        return Err("invalid shard index or count".into());
    }
    Ok((index, count))
}

fn run(args: &[String]) -> Result<()> {
    let manifest = read_manifest(Path::new(option(args, "--manifest")?))?;
    let dt = canonical(option(args, "--dt")?)?;
    if git_commit(&dt)? != manifest.dt_commit {
        return Err("DefinitelyTyped commit differs from manifest".into());
    }
    if manifest.typescript_commit != pinned_typescript_commit()? {
        return Err("TypeScript pin differs from manifest".into());
    }
    let tsgo = canonical(option(args, "--tsgo")?)?;
    let tsrs = canonical(option(args, "--tsrs")?)?;
    let (shard, shard_count) = parse_shard(option(args, "--shard")?)?;
    let jobs = option(args, "--jobs")?.parse::<usize>()?;
    if jobs == 0 || jobs > 32 {
        return Err("jobs must be between 1 and 32".into());
    }
    let out = canonical_or_create(option(args, "--out")?)?.join(format!("shard-{shard}"));
    fs::create_dir_all(&out)?;
    let packages = Arc::new(shard_packages(&manifest, shard, shard_count));
    let next = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let packages = Arc::clone(&packages);
            let next = Arc::clone(&next);
            let errors = Arc::clone(&errors);
            let out = out.clone();
            let dt = dt.clone();
            let tsgo = tsgo.clone();
            let tsrs = tsrs.clone();
            scope.spawn(move || {
                loop {
                    // Only uniqueness matters: package data is immutable and scoped
                    // threads join before their results are read.
                    let position = next.fetch_add(1, Ordering::Relaxed);
                    let Some((number, package)) = packages.get(position) else { break };
                    let result = out.join(format!("{number}.json"));
                    let stage = out.join(format!("{number}.stage"));
                    let reference = out.join(format!("{number}.reference.json"));
                    if let Err(error) =
                        run_worker(&dt, &tsgo, &tsrs, package, &result, &stage, &reference)
                    {
                        errors.lock().unwrap().push(format!("{package}: {error}"));
                    }
                }
            });
        }
    });
    let errors = errors.lock().unwrap();
    if !errors.is_empty() {
        return Err(errors.join("\n").into());
    }
    println!("shard {shard}/{shard_count}: {} package results", packages.len());
    Ok(())
}

fn canonical_or_create(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    fs::create_dir_all(path)?;
    Ok(path.canonicalize()?)
}

fn run_worker(
    dt: &Path,
    tsgo: &Path,
    tsrs: &Path,
    package: &str,
    result: &Path,
    stage: &Path,
    reference: &Path,
) -> Result<()> {
    let _ = fs::remove_file(result);
    let _ = fs::remove_file(stage);
    let _ = fs::remove_file(reference);
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("worker")
        .arg("--dt")
        .arg(dt)
        .arg("--tsgo")
        .arg(tsgo)
        .arg("--tsrs")
        .arg(tsrs)
        .arg("--package")
        .arg(package)
        .arg("--result")
        .arg(result)
        .arg("--stage")
        .arg(stage)
        .arg("--reference")
        .arg(reference)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() || !result.is_file() {
                return record_worker_failure(
                    package,
                    result,
                    stage,
                    reference,
                    format!("package worker exited {status}"),
                );
            }
            return Ok(());
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            // SAFETY: the child was started as a new Unix process group. The negative
            // PID addresses that group, including both compiler subprocesses.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return record_worker_failure(
                package,
                result,
                stage,
                reference,
                "package worker timed out after 180 seconds".to_string(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn record_worker_failure(
    package: &str,
    result: &Path,
    stage: &Path,
    reference: &Path,
    message: String,
) -> Result<()> {
    let failed = CompilerOutcome { error: Some(message), ..CompilerOutcome::default() };
    let mut record = PackageResult {
        package: package.to_string(),
        tsgo: CompilerOutcome::default(),
        tsrs: CompilerOutcome::default(),
        package_issues: Vec::new(),
    };
    if fs::read_to_string(stage).ok().as_deref() == Some("tsrs") {
        record.tsgo = fs::read(reference)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| CompilerOutcome {
                error: Some("reference result missing after worker failure".to_string()),
                ..CompilerOutcome::default()
            });
        record.tsrs = failed;
    } else {
        record.tsgo = failed;
    }
    fs::write(result, serde_json::to_vec_pretty(&record)?)?;
    Ok(())
}

fn worker(args: &[String]) -> Result<()> {
    let dt = canonical(option(args, "--dt")?)?;
    let tsgo = canonical(option(args, "--tsgo")?)?;
    let tsrs = canonical(option(args, "--tsrs")?)?;
    let package = option(args, "--package")?;
    let result = Path::new(option(args, "--result")?);
    let stage = Path::new(option(args, "--stage")?);
    let reference = Path::new(option(args, "--reference")?);
    let output = check_package(&dt, package, &tsgo, &tsrs, stage, reference);
    fs::write(result, serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}

fn report(args: &[String]) -> Result<()> {
    let manifest = read_manifest(Path::new(option(args, "--manifest")?))?;
    let results = Path::new(option(args, "--results")?);
    let out = Path::new(option(args, "--out")?);
    let mut identical = 0;
    let mut mismatches = Vec::new();
    let mut incomplete = Vec::new();
    let mut package_checks = Vec::new();
    let shard_dirs = if results.is_dir() {
        fs::read_dir(results)?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                entry.file_type().ok().filter(std::fs::FileType::is_dir).map(|_| entry.path())
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    for (number, package) in manifest.packages.iter().enumerate() {
        let mut found = Vec::new();
        for dir in &shard_dirs {
            let path = dir.join(format!("{number}.json"));
            if path.is_file() {
                found.push(path);
            }
        }
        if found.len() != 1 {
            incomplete.push(format!("{package}: expected one result, found {}", found.len()));
            continue;
        }
        let record: PackageResult = serde_json::from_slice(&fs::read(&found[0])?)?;
        if record.package != *package {
            incomplete.push(format!("{package}: result names {}", record.package));
            continue;
        }
        if !record.package_issues.is_empty() {
            package_checks.push(format!("{package}: {}", record.package_issues.join("; ")));
        }
        if record.tsgo.error.is_some() {
            incomplete.push(format!("{package}: reference failed: {:?}", record.tsgo.error));
        } else if record.tsrs.error.is_some() {
            mismatches.push(format!("{package}: tsrs failed: {:?}", record.tsrs.error));
        } else if record.tsgo == record.tsrs {
            identical += 1;
        } else {
            let kind = if record.tsgo.issues != record.tsrs.issues {
                "diagnostics"
            } else if record.tsgo.assertions != record.tsrs.assertions {
                "assertions"
            } else {
                "loaded files"
            };
            mismatches.push(format!("{package}: {kind} differ (tsgo {} issues/{} assertions, tsrs {} issues/{} assertions)",
                record.tsgo.issues.len(), record.tsgo.assertions.len(),
                record.tsrs.issues.len(), record.tsrs.assertions.len()));
        }
    }
    let mut markdown = format!(
        "# DefinitelyTyped conformance\n\n- DefinitelyTyped commit: `{}`\n- TypeScript commit: `{}`\n- Packages: {}\n- Identical: {}\n- Mismatched: {}\n- Incomplete: {}\n\n",
        manifest.dt_commit,
        manifest.typescript_commit,
        manifest.packages.len(),
        identical,
        mismatches.len(),
        incomplete.len()
    );
    for (title, rows) in [
        ("Mismatches", &mismatches),
        ("Incomplete", &incomplete),
        ("Package checks", &package_checks),
    ] {
        let _ = write!(markdown, "## {title}\n\n");
        if rows.is_empty() {
            markdown.push_str("None.\n\n");
        }
        for row in rows {
            let _ = writeln!(markdown, "- {row}");
        }
        if !rows.is_empty() {
            markdown.push('\n');
        }
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, markdown)?;
    println!(
        "{} identical, {} mismatched, {} incomplete",
        identical,
        mismatches.len(),
        incomplete.len()
    );
    if !incomplete.is_empty() {
        return Err("conformance run has incomplete packages".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_shards_stay_together() {
        assert!(is_version_directory("v2"));
        assert!(is_version_directory("v2.1"));
        assert!(!is_version_directory("v2.x"));
        let manifest = Manifest {
            dt_commit: String::new(),
            typescript_commit: String::new(),
            packages: vec!["a".into(), "a/v1".into(), "b".into()],
        };
        assert_eq!(shard_packages(&manifest, 0, 2).len(), 2);
        assert_eq!(shard_packages(&manifest, 1, 2).len(), 1);
    }
}
