use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::{Input, REPETITIONS, Result, Settings, invoke, report, summary, write_json};

pub(super) const SEEDS: usize = 6;

pub(super) fn selection(root: &Path, selected: &str) -> Result<Vec<String>> {
    let mut all = Vec::new();
    for entry in fs::read_dir(root.join("testdata/regressions"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("tsconfig.json").is_file() {
            all.push(entry.file_name().into_string().map_err(|_| "invalid regression name")?);
        }
    }
    all.sort();
    let names: BTreeSet<_> = selected.split(',').collect();
    if selected != "all"
        && (names.len() != selected.split(',').count()
            || names.iter().any(|name| !all.iter().any(|a| a == name)))
    {
        return Err("select unique regression names or all".into());
    }
    all.retain(|name| selected == "all" || names.contains(name.as_str()));
    if all.is_empty() {
        return Err("no regression fixtures selected".into());
    }
    Ok(all)
}

pub(super) fn run(settings: &Settings, names: &[String]) -> Result<()> {
    let mut cases = Vec::new();
    summary(settings, "regressions", names.len(), &cases)?;
    for name in names {
        let cwd = settings.root.join("testdata/regressions").join(name);
        let input = Input {
            name: name.clone(),
            config: cwd.join("tsconfig.json"),
            cwd,
            identity: serde_json::Value::Null,
            regression: true,
        };
        let mut reference =
            invoke(settings, &input, &settings.tsrs, &["--singleThreaded".into()], "single")?;
        let mut failure = reference.failure.clone();
        let output = full_output(settings, &input, "single")?;
        // These fixtures include intentional canonical-history differences from Go. Keep their checked-in
        // expectations and compare schedules in the default history, as the previous determinism gate did.
        let expected = fs::read_to_string(input.cwd.join("expected.txt"))?.replace("\r\n", "\n");
        if failure.is_none() && output.trim_end_matches('\n') != expected.trim_end_matches('\n') {
            write_diff(settings, &input, "single", &expected, &output)?;
            failure =
                Some("single-threaded output differs from expected.txt; see single.diff".into());
            reference.failure.clone_from(&failure);
            write_json(&settings.out.join(name).join("single.json"), &reference)?;
        }
        report(&mut cases, &input, "single", failure.clone());
        if failure.is_none() {
            for count in [2, 4] {
                let variants = (1..=REPETITIONS)
                    .map(|rep| (format!("default-{count}-{rep}"), None))
                    .chain((1..=SEEDS).map(|seed| {
                        (format!("random-{count}-{seed}"), Some(format!("random:{seed}")))
                    }));
                for (label, assignment) in variants {
                    let mut flags = vec!["--checkers".into(), count.to_string()];
                    if let Some(assignment) = assignment {
                        flags.extend(["--checkerAssignment".into(), assignment]);
                    }
                    let mut outcome = invoke(settings, &input, &settings.tsrs, &flags, &label)?;
                    let mut failure = outcome.failure.clone();
                    let actual = full_output(settings, &input, &label)?;
                    if failure.is_none()
                        && (outcome.exit_code != reference.exit_code || actual != output)
                    {
                        write_diff(settings, &input, &label, &output, &actual)?;
                        failure = Some(format!(
                            "output or exit status differs from single-threaded (expected {:?}, actual {:?}); see {label}.diff",
                            reference.exit_code, outcome.exit_code
                        ));
                        outcome.failure.clone_from(&failure);
                        write_json(
                            &settings.out.join(name).join(format!("{label}.json")),
                            &outcome,
                        )?;
                    }
                    report(&mut cases, &input, &label, failure);
                }
            }
        }
        summary(settings, "regressions", names.len(), &cases)?;
    }
    let failed = cases.iter().filter(|case| !case.passed).count();
    println!("determinism regressions: {} cases, {failed} failed", cases.len());
    if failed > 0 {
        return Err(format!("{failed} regression cases failed").into());
    }
    Ok(())
}

fn full_output(settings: &Settings, input: &Input, label: &str) -> Result<String> {
    let bytes = fs::read(settings.out.join(&input.name).join(format!("{label}.log")))?;
    let output = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    // Normalize the fixture directory before comparing resolution traces, including symlinked workspaces.
    Ok(output
        .replace(input.cwd.to_string_lossy().as_ref(), "<dir>")
        .replace(input.cwd.canonicalize()?.to_string_lossy().as_ref(), "<dir>"))
}

fn write_diff(
    settings: &Settings,
    input: &Input,
    label: &str,
    expected: &str,
    actual: &str,
) -> Result<()> {
    fs::write(
        settings.out.join(&input.name).join(format!("{label}.diff")),
        similar::TextDiff::from_lines(expected, actual)
            .unified_diff()
            .context_radius(3)
            .header("expected", "actual")
            .to_string(),
    )?;
    Ok(())
}
