use tsrs_testrunner::test_case_parser;

use tsrs_testrunner::compiler_runner::{self, repo_root};
use tsrs_testrunner::options::*;

// For every variant: option parsing must succeed, the unsupported-option skip decision must agree with
// the Go harness (tools/oracle/testrunner `names`), and with a tsconfig.json the compiled/other split
// must reproduce the Go file order (`diags`). Skipped when the oracle files are absent.
#[test]
fn options_and_tsconfig_match_go_harness() {
    let scratch = repo_root().join("target/scratch/testrunner");
    let (Ok(names), Ok(diags)) = (
        std::fs::read_to_string(scratch.join("names.jsonl")),
        std::fs::read_to_string(scratch.join("diags.jsonl")),
    ) else {
        eprintln!("oracle files missing; skipping");
        return;
    };
    let mut go_skip = rustc_hash::FxHashMap::default();
    for line in names.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        go_skip.extend(v["variants"].as_array().into_iter().flatten().map(|variant| {
            let stem = compiler_runner::baseline_stem(variant["name"].as_str().unwrap());
            (format!("{}/{stem}", v["suite"].as_str().unwrap()), variant.get("skip").is_some())
        }));
    }
    let mut go_files = rustc_hash::FxHashMap::default();
    go_files.extend(diags.lines().map(|line| {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let stem = compiler_runner::baseline_stem(v["name"].as_str().unwrap());
        let files: Vec<String> = v["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect();
        (format!("{}/{stem}", v["suite"].as_str().unwrap()), files)
    }));
    let table = tsoptions_option_table();
    let mut problems = Vec::new();
    let mut checked = 0;
    for suite in compiler_runner::SUITES {
        for path in compiler_runner::enumerate_test_files(suite) {
            for item in compiler_runner::expand_test_file(suite, &path, &table).unwrap() {
                let content = compiler_runner::read_test_file(&path);
                let payload = test_case_parser::make_units_from_test(&content, &path);
                if payload.global_options.get("runexternalcode").is_some_and(|v| v == "true") {
                    continue;
                }
                let mut config =
                    compiler_runner::find_configuration(&content, &table, &item.config)
                        .unwrap()
                        .map(|c| c.config)
                        .unwrap_or_default();
                let ts_config = parse_test_ts_config(&payload);
                let split = compiler_runner::split_units(
                    &payload,
                    &mut config,
                    ts_config.map(|t| t.get().parsed_config.file_names.as_slice()),
                );
                checked += 1;
                match test_compiler_options(Some(&config), ts_config, &split.current_directory) {
                    Err(e) => problems.push(format!("{}: {e}", item.id())),
                    Ok((options, _)) => {
                        let skip = unsupported_reason(&options).is_some();
                        if go_skip.get(&item.id()).copied() != Some(skip) {
                            problems.push(format!(
                                "{}: skip={skip} ({:?})",
                                item.id(),
                                unsupported_reason(&options)
                            ));
                        }
                    }
                }
                if ts_config.is_some() {
                    let mine: Vec<String> = split
                        .ts_config_files
                        .iter()
                        .chain(&split.to_be_compiled)
                        .chain(&split.other_files)
                        .map(|f| f.unit_name.clone())
                        .collect();
                    if let Some(go) = go_files.get(&item.id())
                        && !go.is_empty()
                        && *go != mine
                    {
                        problems.push(format!("{}: files {mine:?} vs go {go:?}", item.id()));
                    }
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} of {checked} differ:\n{}",
        problems.len(),
        problems[..problems.len().min(15)].join("\n")
    );
    eprintln!("{checked} variants agree");
}
