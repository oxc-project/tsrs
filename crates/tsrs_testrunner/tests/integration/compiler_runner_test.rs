use tsrs_core::tspath;
use tsrs_testrunner::test_case_parser;

use tsrs_testrunner::compiler_runner::*;

// Checks unit parsing and file assembly for every test variant against the input files the Go harness
// produced (tools/oracle/testrunner `diags` output). Skipped when the oracle files are absent.
#[test]
fn units_match_go_harness() {
    let scratch = repo_root().join("target/scratch/testrunner");
    let Ok(diags) = std::fs::read_to_string(scratch.join("diags.jsonl")) else {
        eprintln!("oracle files missing; skipping");
        return;
    };
    let table = tsrs_testrunner::options::tsoptions_option_table();
    let mut checked = 0;
    let mut mismatches = Vec::new();
    let mut paths: rustc_hash::FxHashMap<String, String> = rustc_hash::FxHashMap::default();
    for suite in SUITES {
        paths.extend(
            enumerate_test_files(suite)
                .into_iter()
                .map(|p| (format!("{suite}/{}", tspath::get_base_file_name(&p)), p)),
        );
    }
    let variant_suffix = regex::Regex::new(r"\(.*\)").unwrap();
    for line in diags.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v.get("error").is_some() || v.get("files").is_none() {
            continue;
        }
        let suite = v["suite"].as_str().unwrap();
        let name = v["name"].as_str().unwrap();
        let file = variant_suffix.replace(name, "").into_owned();
        let path = &paths[&format!("{suite}/{file}")];
        let content = read_test_file(path);
        let stem = baseline_stem(name);
        let items = expand_test_file(suite, path, &table).unwrap();
        let item =
            items.iter().find(|i| i.name == stem).unwrap_or_else(|| panic!("no variant {stem}"));
        let mut config = find_configuration(&content, &table, &item.config)
            .unwrap()
            .map(|c| c.config)
            .unwrap_or_default();
        let payload = test_case_parser::make_units_from_test(&content, path);
        if payload.global_options.get("runexternalcode").is_some_and(|v| v == "true") {
            // content-mapped files are baselined with their transformed text
            continue;
        }
        let all_names: Vec<String> = payload
            .test_unit_data
            .iter()
            .map(|u| tspath::get_normalized_absolute_path(&u.name, &payload.current_directory))
            .collect();
        let has_ts_config = payload.ts_config_file_unit_data.is_some();
        let split =
            split_units(&payload, &mut config, if has_ts_config { Some(&all_names) } else { None });
        let mine: Vec<(String, String)> = split
            .ts_config_files
            .iter()
            .chain(&split.to_be_compiled)
            .chain(&split.other_files)
            .map(|f| (f.unit_name.clone(), f.content.clone()))
            .collect();
        let mut go: Vec<(String, String)> = v["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["name"].as_str().unwrap().to_string(),
                    f["content"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let mut mine = mine;
        if has_ts_config {
            mine[1..].sort();
            go[1..].sort();
        }
        checked += 1;
        if mine != go {
            mismatches.push(format!("{suite}/{name}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {checked} mismatched, e.g. {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(10)]
    );
    eprintln!("{checked} variants match");
}
