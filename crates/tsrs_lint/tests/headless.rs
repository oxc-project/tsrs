use std::fmt::Write as _;
use std::io::Write;
use std::process::{Command, Stdio};

use tsrs_core::json::{self, Value};
use tsrs_lint::protocol;

fn run(payload: &Value, flags: &[&str]) -> (i32, Vec<(u8, Value)>) {
    run_raw(&json::marshal(payload).unwrap(), flags)
}

fn run_raw(input: &str, flags: &[&str]) -> (i32, Vec<(u8, Value)>) {
    run_raw_with_threads(input, flags, None)
}

fn run_raw_with_threads(
    input: &str,
    flags: &[&str],
    threads: Option<&str>,
) -> (i32, Vec<(u8, Value)>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsrs-lint"));
    if let Some(threads) = threads {
        command.env("RAYON_NUM_THREADS", threads);
    }
    let mut child = command
        .arg("headless")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let mut frames = Vec::new();
    let mut bytes = output.stdout.as_slice();
    while !bytes.is_empty() {
        assert!(bytes.len() >= 5, "truncated header");
        let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert!(bytes.len() >= 5 + len, "truncated payload");
        frames.push((
            bytes[4],
            json::unmarshal(std::str::from_utf8(&bytes[5..5 + len]).unwrap()).unwrap(),
        ));
        bytes = &bytes[5 + len..];
    }
    (output.status.code().unwrap(), frames)
}

#[test]
fn raw_json_unpaired_surrogates_emit_one_error_frame_and_fail() {
    for input in [
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\udbff"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\udc00"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\udfff"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800x\udc00"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800\n\udc00"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800\u0041"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800\ud800"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\udc00\ud800"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\udc00\udfff"}}"#,
        r#"{"version":2,"source_overrides":{"a.ts":"\ud800\\udc00"}}"#,
        r#"{"version":2,"source_overrides":{"\udc00":""}}"#,
        r#"{"version":2,"configs":[{"rules":[{"name":"await-thenable","options":["\ud800"]}]}]}"#,
        r#"{"version":2,"configs":[{"rules":[{"name":"await-thenable","options":{"\udc00":true}}]}]}"#,
        r#"{"version":2,"ignored":{"nested":["\ud800"]}}"#,
        r#"{"version":2,"\udc00":null}"#,
        r#"{"files":[{"file_path":"\ud800"}]}"#,
    ] {
        let (status, frames) = run_raw(input, &[]);
        assert_eq!(status, 1, "{input}: {frames:?}");
        assert_eq!(frames.len(), 1, "{input}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8, "{input}");
        let Value::Object(error) = &frames[0].1 else { panic!() };
        assert_eq!(error.len(), 1, "{input}");
        let Value::String(message) = &error["error"] else { panic!() };
        assert!(
            message
                .starts_with("error parsing config: jsontext: invalid surrogate pair at offset "),
            "{input}: {message}"
        );
    }
}

#[test]
fn raw_json_valid_unicode_and_textual_surrogates_succeed() {
    for input in [
        r#"{"version":2,"source_overrides":{"\ud83d\ude00.ts":"const s = \"\\ud800\\udc00\"; // �\ufffdé中\ud83d\ude00"}}"#,
        r#"{"version":2,"ignored":{"\ud800\udc00":["\udbff\udfff","\"\\\/\b\f\n\r\t"]}}"#,
        r#"{"version":2,"configs":[{"rules":[{"name":"await-thenable","options":{"\ud800\udc00":"\udbff\udfff","text":"\\ud800"}}]}]}"#,
    ] {
        assert_eq!(run_raw(input, &[]), (0, Vec::new()), "{input}");
    }
}

#[test]
fn empty_payload_and_debug_timing_frame() {
    let payload = json::unmarshal(r#"{"version":2}"#).unwrap();
    assert_eq!(run(&payload, &[]), (0, Vec::new()));
    assert_eq!(
        run(&payload, &["--debug", "timings"]),
        (0, vec![(2, protocol::object([("rules", Value::Array(Vec::new()))]))])
    );
}

#[test]
fn strict_boolean_expressions_preserves_utf8_options_predicates_state_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/strict_boolean_expressions/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-strict-boolean-regressions");
    let mut strict_files = Vec::new();
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let strict = case.get("strict") != Some(&Value::Bool(false));
        let file =
            if strict { base.join(format!("{name}.ts")) } else { base.join(name).join("file.ts") }
                .to_string_lossy()
                .into_owned();
        if strict {
            strict_files.push(Value::String(file.clone()));
        }
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("strict-boolean-expressions".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        if !strict {
            overrides.insert(
                base.join(name).join("tsconfig.json").to_string_lossy().into_owned(),
                Value::String(r#"{"compilerOptions":{"strict":false,"target":"esnext","types":[]},"files":["file.ts"]}"#.into()),
            );
        }
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["diagnostics"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(format!(
            r#"{{"compilerOptions":{{"strict":true,"target":"esnext","types":[]}},"files":{}}}"#,
            json::marshal(&Value::Array(strict_files)).unwrap(),
        )),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn related_getter_setter_pairs_preserves_names_order_utf8_options_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/related_getter_setter_pairs/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base =
        std::env::current_dir().unwrap().join("target/lint-related-getter-setter-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("related-getter-setter-pairs".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["diagnostics"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext"},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn unbound_method_preserves_utf8_labels_bindings_options_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/unbound_method/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-unbound-method-regressions");
    std::fs::create_dir_all(&base).unwrap();
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("unbound-method".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        let Value::Object(files) = &case["files"] else { panic!() };
        for (name, source) in files {
            overrides.insert(base.join(name).to_string_lossy().into_owned(), source.clone());
        }
        expected_by_file.push((file, case["diagnostics"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext","module":"commonjs","lib":["esnext","dom"],"types":[]},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn unbound_method_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unbound-method-invalid/a.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "false",
        "[]",
        "1",
        r#""invalid""#,
        r#"{"ignoreStatic":1}"#,
        r#"{"ignoreStatic":[]}"#,
        r#"{"ignoreStatic":{}}"#,
        r#"{"ignoreStatic":"true"}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("unbound-method".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {};".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn unknown_rules_and_bad_versions_emit_error_frames() {
    for (input, error) in [
        (
            r#"{"version":3}"#,
            "error parsing config: unsupported version `3`: expected `unset` or `2`",
        ),
        (
            r#"{"version":2,"configs":[{"file_paths":["a.ts"],"rules":[{"name":"unknown-rule"}]}]}"#,
            "error running linter: unknown rule: unknown-rule",
        ),
    ] {
        let (status, frames) = run(&json::unmarshal(input).unwrap(), &[]);
        assert_eq!(status, 1);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, 0);
        assert_eq!(frames[0].1, protocol::object([("error", Value::String(error.into()))]));
    }
}

#[test]
fn catch_callback_unknown_preserves_utf8_options_branches_constraints_and_suggestion_flags() {
    let Value::Array(cases) = json::unmarshal(include_str!(
        "fixtures/use_unknown_in_catch_callback_variable/regressions.json"
    ))
    .unwrap() else {
        panic!()
    };
    let base =
        std::env::current_dir().unwrap().join("target/lint-catch-callback-unknown-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("use-unknown-in-catch-callback-variable".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["diagnostics"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    if !flags.contains(&"-fix-suggestions") {
                        diagnostic.shift_remove("suggestions");
                    }
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(count, 19);
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn require_array_sort_compare_preserves_utf8_constraints_per_file_options_and_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-require-array-sort-compare");
    let source = include_str!("fixtures/require_array_sort_compare/edge_cases.ts");
    let Value::Object(expected) =
        json::unmarshal(include_str!("fixtures/require_array_sort_compare/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut expected_by_file = Vec::new();
    for (index, (options, expected_key)) in [
        ("null", "default"),
        ("{}", "default"),
        (r#"{"ignoreStringArrays":false}"#, "required"),
        (r#"{"ignoreStringArrays":null}"#, "default"),
        (r#"{"ignoreStringArrays":true,"unknown":42}"#, "default"),
    ]
    .into_iter()
    .enumerate()
    {
        let file = base.join(format!("{index}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("require-array-sort-compare".into())),
                    ("options", json::unmarshal(options).unwrap()),
                ])]),
            ),
        ]));
        overrides.push((file.clone(), Value::String(source.into())));
        expected_by_file.push((file, &expected[expected_key]));
    }
    overrides.push((
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]}}"#.into()),
    ));
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn require_array_sort_compare_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-require-array-sort-compare-invalid/a.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "false",
        "[]",
        "1",
        r#""invalid""#,
        r#"{"ignoreStringArrays":1}"#,
        r#"{"ignoreStringArrays":[]}"#,
        r#"{"ignoreStringArrays":{}}"#,
        r#"{"ignoreStringArrays":"true"}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("require-array-sort-compare".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {};".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn restrict_template_expressions_preserves_utf8_options_constraints_and_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-restrict-template-expressions");
    let source = include_str!("fixtures/restrict_template_expressions/edge_cases.ts");
    let Value::Array(options) = json::unmarshal(include_str!(
        "fixtures/restrict_template_expressions/edge_cases.options.json"
    ))
    .unwrap() else {
        panic!()
    };
    let Value::Array(expected) = json::unmarshal(include_str!(
        "fixtures/restrict_template_expressions/edge_cases.diagnostics.json"
    ))
    .unwrap() else {
        panic!()
    };
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    for (index, options) in options.into_iter().enumerate() {
        let file = base.join(format!("case-{index}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("restrict-template-expressions".into())),
                    ("options", options),
                ])]),
            ),
        ]));
        overrides.push((file, Value::String(source.into())));
    }
    overrides.push((
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"include":["*.ts"]}"#.into()),
    ));
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    let expected: Vec<_> = expected
        .into_iter()
        .map(|diagnostic| {
            let Value::Object(mut diagnostic) = diagnostic else { panic!() };
            let Value::String(file) = &diagnostic["file_path"] else { panic!() };
            diagnostic.insert(
                "file_path".into(),
                Value::String(base.join(file).to_string_lossy().into_owned()),
            );
            (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
        })
        .collect();
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        assert_eq!(frames.len(), expected.len(), "{flags:?}");
        for (index, (actual, expected)) in frames.iter().zip(&expected).enumerate() {
            assert_eq!(actual, expected, "{flags:?}: diagnostic {index}");
        }
    }
}

#[test]
fn restrict_template_expressions_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-restrict-template-expressions/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "42",
        "[]",
        r#""invalid""#,
        r#"{"allow":true}"#,
        r#"{"allow":[false]}"#,
        r#"{"allowAny":1}"#,
        r#"{"allowArray":"true"}"#,
        r#"{"allowBoolean":[]}"#,
        r#"{"allowNever":{}}"#,
        r#"{"allowNullish":0}"#,
        r#"{"allowNumber":"false"}"#,
        r#"{"allowRegExp":1}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("restrict-template-expressions".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("const value = `${1}`;".into())))
                        .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn strict_boolean_expressions_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-strict-boolean-expressions-invalid/a.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        r"true",
        r"false",
        r"[]",
        r"1",
        r#""invalid""#,
        r#"{"allowAny":1}"#,
        r#"{"allowAny":[]}"#,
        r#"{"allowAny":{}}"#,
        r#"{"allowAny":"true"}"#,
        r#"{"allowNullableBoolean":1}"#,
        r#"{"allowNullableBoolean":[]}"#,
        r#"{"allowNullableBoolean":{}}"#,
        r#"{"allowNullableBoolean":"true"}"#,
        r#"{"allowNullableEnum":1}"#,
        r#"{"allowNullableEnum":[]}"#,
        r#"{"allowNullableEnum":{}}"#,
        r#"{"allowNullableEnum":"true"}"#,
        r#"{"allowNullableNumber":1}"#,
        r#"{"allowNullableNumber":[]}"#,
        r#"{"allowNullableNumber":{}}"#,
        r#"{"allowNullableNumber":"true"}"#,
        r#"{"allowNullableObject":1}"#,
        r#"{"allowNullableObject":[]}"#,
        r#"{"allowNullableObject":{}}"#,
        r#"{"allowNullableObject":"true"}"#,
        r#"{"allowNullableString":1}"#,
        r#"{"allowNullableString":[]}"#,
        r#"{"allowNullableString":{}}"#,
        r#"{"allowNullableString":"true"}"#,
        r#"{"allowNumber":1}"#,
        r#"{"allowNumber":[]}"#,
        r#"{"allowNumber":{}}"#,
        r#"{"allowNumber":"true"}"#,
        r#"{"allowString":1}"#,
        r#"{"allowString":[]}"#,
        r#"{"allowString":{}}"#,
        r#"{"allowString":"true"}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("strict-boolean-expressions".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {};".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn prefer_reduce_type_parameter_keeps_utf8_trivia_constraints_options_and_fix_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-reduce-type-parameter");
    let file = base.join("virtual.ts").to_string_lossy().into_owned();
    let source = include_str!("fixtures/prefer_reduce_type_parameter/edge_cases.ts");
    let Value::Array(expected) = json::unmarshal(include_str!(
        "fixtures/prefer_reduce_type_parameter/edge_cases.diagnostics.json"
    ))
    .unwrap() else {
        panic!()
    };
    for options in [
        Value::Null,
        Value::String("ignored".into()),
        Value::Bool(false),
        Value::Integer(42),
        Value::Array(Vec::new()),
        protocol::object([("unknown", Value::Bool(true))]),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-reduce-type-parameter".into())),
                            ("options", options.clone()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file.clone(), Value::String(source.into())),
                        (
                            base.join("tsconfig.json").to_string_lossy().into_owned(),
                            Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"files":["virtual.ts"]}"#.into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{options:?}: {flags:?}: {frames:?}");
            let expected: Vec<_> = expected
                .iter()
                .map(|diagnostic| {
                    let Value::Object(mut diagnostic) = diagnostic.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    if !flags.contains(&"-fix") {
                        diagnostic.shift_remove("fixes");
                    }
                    (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
                })
                .collect();
            assert_eq!(frames, expected, "{options:?}: {flags:?}");
        }
    }
}

#[test]
fn prefer_return_this_type_preserves_utf8_type_identity_return_boundaries_and_flags() {
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut files = Vec::new();
    for (name, source, options) in [
        (
            "first.ts",
            include_str!("fixtures/prefer_return_this_type/first.ts"),
            Value::String("ignored".into()),
        ),
        (
            "second.ts",
            include_str!("fixtures/prefer_return_this_type/second.ts"),
            Value::Array(vec![Value::Bool(false), Value::Integer(42)]),
        ),
    ] {
        let file = std::env::current_dir()
            .unwrap()
            .join(format!("target/lint-return-this-type/{name}"))
            .to_string_lossy()
            .into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-return-this-type".into())),
                    ("options", options),
                ])]),
            ),
        ]));
        overrides.push((file.clone(), Value::String(source.into())));
        files.push((name, file));
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let Value::Array(mut expected) =
            json::unmarshal(include_str!("fixtures/prefer_return_this_type/diagnostics.json"))
                .unwrap()
        else {
            panic!()
        };
        for diagnostic in &mut expected {
            let Value::Object(diagnostic) = diagnostic else { panic!() };
            let file = files
                .iter()
                .find(|(name, _)| diagnostic["file_path"] == Value::String((*name).into()))
                .unwrap();
            diagnostic.insert("file_path".into(), Value::String(file.1.clone()));
            if !flags.contains(&"-fix") {
                diagnostic.shift_remove("fixes");
            }
        }
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(
            frames,
            expected.into_iter().map(|diagnostic| (1, diagnostic)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn only_throw_error_preserves_utf8_ranges_options_and_rethrow_boundaries() {
    let strict = r#"{"allowThrowingAny":false,"allowThrowingUnknown":false}"#;
    let thenable = "export {}; declare const p: { then(cb: (value: unknown) => void, reject?: (reason: unknown) => void): void; catch(cb: (reason: unknown) => void): void; }; ";
    let cases = [
        (
            "export {}; const café = 1; throw café; throw undefined;".to_owned(),
            "null",
            vec![("object", "café;", 5), ("undef", "throw undefined;", 16)],
        ),
        ("export {}; declare const v: any; throw v;".to_owned(), "null", vec![]),
        ("export {}; declare const v: any; throw v;".to_owned(), strict, vec![("object", "v;", 1)]),
        (
            "export {}; declare const v: unknown; throw v;".to_owned(),
            strict,
            vec![("object", "v;", 1)],
        ),
        ("export {}; try {} catch (é) { throw é; }".to_owned(), strict, vec![]),
        (
            "export {}; try {} catch (é) { throw é; }".to_owned(),
            r#"{"allowThrowingAny":false,"allowThrowingUnknown":false,"allowRethrowing":false}"#,
            vec![("object", "é;", 2)],
        ),
        (format!("{thenable}p.catch(é => {{ throw é; }});"), strict, vec![]),
        (format!("{thenable}p.then(() => {{}}, é => {{ throw é; }});"), strict, vec![]),
        (format!("{thenable}p.catch(é => {{ throw (é); }});"), strict, vec![("object", "(é);", 4)]),
        (
            format!("{thenable}p.catch(function (é) {{ throw é; }});"),
            strict,
            vec![("object", "é;", 2)],
        ),
        (
            format!("{thenable}p.then(...([] as any[]), é => {{ throw é; }});"),
            strict,
            vec![("object", "é;", 2)],
        ),
        (
            "export {}; throw undefined;".to_owned(),
            r#"{"allow":["undefined"],"allowRethrowing":null,"allowThrowingAny":null,"allowThrowingUnknown":null}"#,
            vec![],
        ),
    ];
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut expected = Vec::new();
    for (index, (source, options, diagnostics)) in cases.into_iter().enumerate() {
        let file = std::env::current_dir()
            .unwrap()
            .join(format!("target/lint-only-throw-error/{index}.ts"))
            .to_string_lossy()
            .into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("only-throw-error".into())),
                    ("options", json::unmarshal(options).unwrap()),
                ])]),
            ),
        ]));
        for (id, needle, length) in diagnostics {
            let pos = source.rfind(needle).unwrap();
            expected.push((file.clone(), id, pos, pos + length));
        }
        overrides.push((file, Value::String(source)));
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), expected.len(), "{frames:?}");
        for (file, id, pos, end) in &expected {
            let diagnostic = frames
                .iter()
                .find_map(|(kind, value)| {
                    assert_eq!(*kind, 1);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone())
                        && diagnostic["range"]
                            == protocol::object([
                                ("pos", Value::Number(*pos as f64)),
                                ("end", Value::Number(*end as f64)),
                            ]))
                    .then_some(diagnostic)
                })
                .unwrap_or_else(|| {
                    panic!("expected diagnostic for {file}:{pos}-{end}: {frames:?}")
                });
            assert_eq!(diagnostic["rule"], Value::String("only-throw-error".into()));
            assert_eq!(
                diagnostic["message"],
                protocol::object([
                    ("id", Value::String((*id).into())),
                    (
                        "description",
                        Value::String(
                            if *id == "undef" {
                                "Do not throw undefined."
                            } else {
                                "Expected an error object to be thrown."
                            }
                            .into()
                        )
                    ),
                ])
            );
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn only_throw_error_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-only-throw-error/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        r#"{"allow":true}"#,
        r#"{"allowRethrowing":"true"}"#,
        r#"{"allowThrowingAny":1}"#,
        r#"{"allowThrowingUnknown":[]}"#,
        r#"{"allow":[false]}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("only-throw-error".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("throw 1;".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, 0);
    }
}

#[test]
fn source_overrides_support_inferred_type_diagnostics() {
    let cwd = std::env::current_dir().unwrap();
    let path = cwd.join("target/lint-inferred/virtual.ts").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(path.clone())])),
                ("rules", Value::Array(Vec::new())),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(
                std::iter::once((path, Value::String("const value: string = 1;".into()))).collect(),
            ),
        ),
        ("report_semantic", Value::Bool(true)),
    ]);
    let mut payload = payload;
    let Value::Object(fields) = &mut payload else { panic!() };
    let Some(Value::Array(configs)) = fields.get_mut("configs") else { panic!() };
    let Value::Object(config) = &mut configs[0] else { panic!() };
    let Some(Value::Array(paths)) = config.get_mut("file_paths") else { panic!() };
    paths.push(paths[0].clone());
    let (status, frames) = run(&payload, &[]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1, "duplicate paths must execute once: {frames:?}");
    assert!(
        frames.iter().any(|(kind, value)| {
            let Value::Object(diagnostic) = value else {
                return false;
            };
            let Some(Value::Object(message)) = diagnostic.get("message") else {
                return false;
            };
            *kind == 1 && message.get("id") == Some(&Value::String("TS2322".into()))
        }),
        "{frames:?}"
    );
}

#[test]
fn jsconfig_controls_javascript_checking() {
    let base = std::env::current_dir().unwrap().join("target/lint-jsconfig");
    let file = base.join("a.js").to_string_lossy().into_owned();
    let config = base.join("jsconfig.json").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(vec![protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            ("rules", Value::Array(Vec::new())),
        ])])),
        ("source_overrides", Value::Object([
            (file, Value::String("/** @type {string} */ const x = 1;".into())),
            (config, Value::String(r#"{"compilerOptions":{"checkJs":true,"noEmit":true},"files":["a.js"]}"#.into())),
        ].into_iter().collect())),
        ("report_semantic", Value::Bool(true)),
    ]);
    let (status, frames) = run(&payload, &[]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1, "{frames:?}");
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    let Value::Object(message) = &diagnostic["message"] else { panic!() };
    assert_eq!(message["id"], Value::String("TS2322".into()));
}

#[test]
fn excluded_files_use_inferred_programs_and_node_modules_stops_config_search() {
    for (relative, config, code, expected_count) in [
        ("a.ts", r#"{"files":["b.ts"]}"#, "const x: string = 1;", 1),
        (
            "node_modules/pkg/a.js",
            r#"{"compilerOptions":{"checkJs":true,"noEmit":true},"files":["node_modules/pkg/a.js"]}"#,
            "/** @type {string} */ const x = 1;",
            0,
        ),
    ] {
        let base = std::env::current_dir().unwrap().join("target/lint-config-boundaries");
        let file = base.join(relative).to_string_lossy().into_owned();
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    ("rules", Value::Array(Vec::new())),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file, Value::String(code.into())),
                        (
                            base.join("b.ts").to_string_lossy().into_owned(),
                            Value::String("export {};".into()),
                        ),
                        (
                            base.join("tsconfig.json").to_string_lossy().into_owned(),
                            Value::String(config.into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("report_semantic", Value::Bool(true)),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), expected_count, "{frames:?}");
    }
}

#[test]
fn unrelated_invalid_config_does_not_capture_excluded_files() {
    for config in [
        r#"{"compilerOptions":{"badOption":true},"files":["b.ts"]}"#,
        r#"{"files":[]}"#,
        r#"{"compilerOptions":"#,
        r#"{"files":["a.ts"],"compilerOptions":"#,
    ] {
        let base = std::env::current_dir().unwrap().join("target/lint-invalid-config");
        let file = base.join("a.ts").to_string_lossy().into_owned();
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    ("rules", Value::Array(Vec::new())),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file, Value::String("const x: string = 1;".into())),
                        (
                            base.join("b.ts").to_string_lossy().into_owned(),
                            Value::String("export {};".into()),
                        ),
                        (
                            base.join("tsconfig.json").to_string_lossy().into_owned(),
                            Value::String(config.into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
            ("report_semantic", Value::Bool(true)),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(message["id"], Value::String("TS2322".into()));
    }
}

#[test]
fn await_thenable_obeys_last_file_configuration_and_suggestion_flags() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-await/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let code = "const greeting = '😀'; await 1;";
    let configured = protocol::object([
        ("file_paths", Value::Array(vec![Value::String(file.clone())])),
        (
            "rules",
            Value::Array(vec![protocol::object([(
                "name",
                Value::String("await-thenable".into()),
            )])]),
        ),
    ]);
    let disabled = protocol::object([
        ("file_paths", Value::Array(vec![Value::String(file.clone())])),
        ("rules", Value::Array(Vec::new())),
    ]);
    let mut payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(vec![configured.clone(), disabled.clone()])),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(code.into()))).collect()),
        ),
    ]);
    assert_eq!(run(&payload, &[]), (0, Vec::new()));
    let Value::Object(fields) = &mut payload else { panic!() };
    fields.insert("configs".into(), Value::Array(vec![disabled, configured]));
    for flags in [&[][..], &["-fix-suggestions"][..]] {
        let (status, frames) = run(&payload, flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let Value::Object(range) = &diagnostic["range"] else { panic!() };
        assert_eq!(range["pos"], Value::Number(code.find("await").unwrap() as f64));
        assert_eq!(diagnostic.contains_key("suggestions"), !flags.is_empty());
    }
}

#[test]
fn timing_counts_rule_initialization_and_listener_calls() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-timing/virtual.ts")
        .to_string_lossy()
        .into_owned();
    for (code, calls) in [("", 1.0), ("await 1;", 2.0)] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([(
                            "name",
                            Value::String("await-thenable".into()),
                        )])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(code.into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-debug", "timings"]);
        assert_eq!(status, 0);
        let (kind, Value::Object(timing)) = frames.last().unwrap() else { panic!() };
        assert_eq!(*kind, 2);
        let Value::Array(rules) = &timing["rules"] else { panic!() };
        assert_eq!(rules.len(), 1);
        let Value::Object(rule) = &rules[0] else { panic!() };
        assert_eq!(rule["rule_name"], Value::String("await-thenable".into()));
        assert_eq!(rule["calls"], Value::Number(calls));
    }
}

#[test]
fn consistent_return_preserves_nested_function_state_and_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-return/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let scenarios = [
        (
            "async function outer(flag: boolean): Promise<number> { function inner(flag: boolean): number { if (flag) return 1; return; } await 1; if (flag) return 2; return; }",
            false,
            3,
        ),
        (
            "function f(flag: boolean): number | undefined { if (flag) return 1; return void 0; }",
            false,
            0,
        ),
        (
            "function f(flag: boolean): number | undefined { if (flag) return 1; return void 0; }",
            true,
            1,
        ),
    ];
    for (code, treat_undefined, count) in scenarios {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![
                            protocol::object([("name", Value::String("await-thenable".into()))]),
                            protocol::object([
                                ("name", Value::String("consistent-return".into())),
                                (
                                    "options",
                                    protocol::object([(
                                        "treatUndefinedAsUnspecified",
                                        Value::Bool(treat_undefined),
                                    )]),
                                ),
                            ]),
                        ]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(code.into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), count, "{frames:?}");
        if count == 3 {
            let descriptions = frames
                .iter()
                .map(|(_, frame)| {
                    let Value::Object(diagnostic) = frame else { panic!() };
                    let Value::Object(message) = &diagnostic["message"] else { panic!() };
                    message["description"].clone()
                })
                .collect::<Vec<_>>();
            assert!(
                descriptions
                    .contains(&Value::String("Function 'inner' expected a return value.".into()))
            );
            assert!(descriptions.contains(&Value::String(
                "Async function 'outer' expected a return value.".into()
            )));
        }
    }
}

#[test]
fn consistent_type_exports_resolves_overrides_and_both_fix_modes() {
    let base = std::env::current_dir().unwrap().join("target/lint-export-overrides");
    std::fs::create_dir_all(&base).unwrap();
    let file = base.join("a.ts").to_string_lossy().into_owned();
    let types = base.join("types.ts").to_string_lossy().into_owned();
    let source = "export { T, V } from './types';";
    for inline in [false, true] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("consistent-type-exports".into())),
                            (
                                "options",
                                protocol::object([(
                                    "fixMixedExportsWithInlineTypeSpecifier",
                                    Value::Bool(inline),
                                )]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file.clone(), Value::String(source.into())),
                        (
                            types.clone(),
                            Value::String("export interface T {} export const V = 1;".into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix"]);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(message["id"], Value::String("singleExportIsType".into()));
        let Value::Array(fixes) = &diagnostic["fixes"] else { panic!() };
        assert_eq!(fixes.len(), 1);
        let Value::Object(fix) = &fixes[0] else { panic!() };
        let expected = if inline {
            "type "
        } else {
            "export type { T } from './types';\nexport { V } from './types';"
        };
        assert_eq!(fix["text"], Value::String(expected.into()));
    }
}

#[test]
fn dot_notation_accepts_go_regex_syntax_for_ascii_property_names() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-dot-regex/virtual.ts")
        .to_string_lossy()
        .into_owned();
    for (pattern, count) in [
        (r"^\Qa\E$", 0),
        (r"^(?P<1>a)$", 0),
        (r"^[a&&b]$", 0),
        (r"^[^^]$", 0),
        (r"^\141$", 0),
        (r"^[[a]$", 0),
        (r"^[[:alpha:]]$", 0),
        (r"^a{01}$", 1),
        (r"^a{1, 2}$", 1),
        (r"^a{1,0002}$", 1),
        (r"^a{foo|a$", 0),
        (r"^[\d-a]$", 0),
        (r"^[\w-a]$", 0),
        (r"^[\pL-a]$", 0),
        (r"^[\p{L}-a]$", 0),
        (r"^[\d-\w]$", 0),
        (r"^\p{^L}$", 1),
        (r"^\P{^L}$", 0),
        (r"^\p{Cs}$", 1),
        (r"^\P{Cs}$", 0),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("dot-notation".into())),
                            (
                                "options",
                                protocol::object([("allowPattern", Value::String(pattern.into()))]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("foo['a'];".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 0, "{pattern}: {frames:?}");
        assert_eq!(frames.len(), count, "{pattern}: {frames:?}");
    }
}

#[test]
fn no_array_delete_obeys_suggestion_flags_and_utf8_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-array-delete/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "declare const 数组: number[]; delete ((数组[0]));";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-array-delete".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        assert_eq!(
            diagnostic["range"],
            protocol::object([
                ("pos", Value::Number(source.find("delete").unwrap() as f64)),
                ("end", Value::Number((source.find("delete").unwrap() + 6) as f64)),
            ])
        );
        assert!(!diagnostic.contains_key("fixes"));
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            let Value::Array(fixes) = &suggestion["fixes"] else { panic!() };
            assert_eq!(fixes.len(), 3);
            let mut result = source.to_owned();
            for fix in fixes.iter().rev() {
                let Value::Object(fix) = fix else { panic!() };
                let Value::Object(range) = &fix["range"] else { panic!() };
                let Value::Number(pos) = range["pos"] else { panic!() };
                let Value::Number(end) = range["end"] else { panic!() };
                let Value::String(text) = &fix["text"] else { panic!() };
                result.replace_range(pos as usize..end as usize, text);
            }
            assert_eq!(result, "declare const 数组: number[];  ((数组.splice(0, 1)));");
        } else {
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn no_base_to_string_keeps_options_and_recursive_memos_per_file() {
    let root = std::env::current_dir().unwrap().join("target/lint-base-string");
    let files =
        ["checked.ts", "ignored.ts"].map(|name| root.join(name).to_string_lossy().into_owned());
    let source = "export {}; declare const 值: unknown; String(值); type Loop = Loop[]; declare const loop: Loop; String(loop); loop.join();";
    let configs = files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-base-to-string".into())),
                        (
                            "options",
                            protocol::object([
                                ("checkUnknown", Value::Bool(index == 0)),
                                ("ignoredTypeNames", Value::Array(vec![Value::Null])),
                            ]),
                        ),
                    ])]),
                ),
            ])
        })
        .collect();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        (
            "source_overrides",
            Value::Object(
                files.iter().map(|file| (file.clone(), Value::String(source.into()))).collect(),
            ),
        ),
    ]);
    let (status, frames) = run(&payload, &[]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    assert_eq!(diagnostic["file_path"], Value::String(files[0].clone()));
    let Value::Object(message) = &diagnostic["message"] else { panic!() };
    assert_eq!(message["id"], Value::String("baseToString".into()));
    assert_eq!(message["description"], Value::String("'值' may use Object's default stringification format ('[object Object]') when stringified.".into()));
}

#[test]
fn no_confusing_void_expression_separates_fixes_and_suggestions() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-void/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(vec![protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            ("rules", Value::Array(vec![protocol::object([
                ("name", Value::String("no-confusing-void-expression".into())),
                ("options", protocol::object([("ignoreVoidOperator", Value::Bool(true))])),
            ])])),
        ])])),
        ("source_overrides", Value::Object(std::iter::once((file, Value::String("declare function effect(): void; const 值 = effect(); const arrow = () => effect();".into()))).collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 2);
        for (index, (_, value)) in frames.iter().enumerate() {
            let Value::Object(diagnostic) = value else { panic!() };
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(
                message["id"],
                Value::String(
                    if index == 0 {
                        "invalidVoidExprWrapVoid"
                    } else {
                        "invalidVoidExprArrowWrapVoid"
                    }
                    .into()
                )
            );
            assert_eq!(diagnostic.contains_key("fixes"), index == 1 && flags.contains(&"-fix"));
            assert_eq!(
                diagnostic.contains_key("suggestions"),
                index == 0 && flags.contains(&"-fix-suggestions")
            );
        }
    }
}

#[test]
fn no_deprecated_preserves_unicode_reasons_and_nullable_allow_specifiers() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-deprecated/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "/** @deprecated use 新 instead */ function 旧() {} 旧;";
    for (allow, count) in
        [(Value::Array(vec![Value::Null]), 1), (Value::Array(vec![Value::String("旧".into())]), 0)]
    {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-deprecated".into())),
                            ("options", protocol::object([("allow", allow)])),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), count);
        if count != 0 {
            let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(
                message["description"],
                Value::String("`旧` is deprecated. use 新 instead".into())
            );
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn no_deprecated_distinguishes_shorthand_aliases_from_binding_keys() {
    let base = std::env::current_dir().unwrap().join("target/lint-deprecated-shorthand-binding");
    let file = base.join("file.ts").to_string_lossy().into_owned();
    let source = "import { foo } from \"./lib\";\nconst { foo: bar } = { foo };\n";
    let overrides = [
        (file.clone(), source),
        (
            base.join("lib.ts").to_string_lossy().into_owned(),
            "/** @deprecated old */\nexport const foo = 1;",
        ),
        (
            base.join("tsconfig.json").to_string_lossy().into_owned(),
            r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]}}"#,
        ),
    ];
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-deprecated".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(
                overrides
                    .into_iter()
                    .map(|(path, text)| (path, Value::String(text.into())))
                    .collect(),
            ),
        ),
    ]);
    let mut expected = json::unmarshal(
        r#"{"kind":0,"message":{"id":"deprecatedWithReason","description":"`foo` is deprecated. old"},"range":{"pos":52,"end":55},"rule":"no-deprecated"}"#,
    )
    .unwrap();
    let Value::Object(ref mut diagnostic) = expected else { panic!() };
    diagnostic.insert("file_path".into(), Value::String(file));
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames, vec![(1, expected.clone())], "{flags:?}");
    }
}

#[test]
fn no_deprecated_preserves_bigint_property_names() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-deprecated-bigint/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "interface X {\n/** @deprecated one n */\n'1n': number;\n/** @deprecated one */\n'1': number;\n}\ndeclare const x: X; x[1n]; const key = 1n; const object: X = { [key]: 0 };";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-deprecated".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    let (status, frames) = run(&payload, &[]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 2);
    for (_, value) in frames {
        let Value::Object(diagnostic) = value else { panic!() };
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(message["description"], Value::String("`1n` is deprecated. one n".into()));
    }
}

#[test]
fn no_duplicate_type_constituents_obeys_options_and_fix_flags_with_unicode() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-duplicates/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "type U = '值' | '值'; type I = number & number;";
    for ignore_unions in [false, true] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-duplicate-type-constituents".into())),
                            (
                                "options",
                                protocol::object([("ignoreUnions", Value::Bool(ignore_unions))]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0);
            assert_eq!(frames.len(), if ignore_unions { 1 } else { 2 });
            for (_, value) in &frames {
                let Value::Object(diagnostic) = value else { panic!() };
                assert_eq!(diagnostic.contains_key("fixes"), flags.contains(&"-fix"));
                assert!(!diagnostic.contains_key("suggestions"));
                assert!(diagnostic.contains_key("labeled_ranges"));
            }
            if !ignore_unions {
                let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
                let Value::Array(labels) = &diagnostic["labeled_ranges"] else { panic!() };
                let Value::Object(label) = &labels[0] else { panic!() };
                assert_eq!(
                    label["label"],
                    Value::String("Type ''值'' is first declared here.".into())
                );
                assert_eq!(label["range"], json::unmarshal("{\"pos\":9,\"end\":14}").unwrap());
            }
        }
    }
}

#[test]
fn no_duplicate_type_constituents_uses_go_token_ranges_inside_jsdoc() {
    let root = std::env::current_dir().unwrap().join("target/lint-duplicates-jsdoc");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"allowJs":true,"checkJs":true,"noEmit":true},"include":["*.js"]}"#,
    )
    .unwrap();
    std::fs::write(root.join("virtual.js"), "").unwrap();
    let file = root.join("virtual.js").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-duplicate-type-constituents".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(
                std::iter::once((
                    file,
                    Value::String("/** @type {string |\n * string} */ let x;".into()),
                ))
                .collect(),
            ),
        ),
    ]);
    let (status, frames) = run(&payload, &["-fix"]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    let range = json::unmarshal("{\"pos\":21,\"end\":29}").unwrap();
    assert_eq!(diagnostic["range"], range);
    let Value::Array(fixes) = &diagnostic["fixes"] else { panic!() };
    let Value::Object(fix) = &fixes[0] else { panic!() };
    assert_eq!(fix["range"], range);
}

#[test]
fn no_floating_promises_gates_suggestions_and_preserves_unicode_insertion_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-floating/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "declare const 等待: Promise<void>; 等待;";
    for ignore_void in [false, true] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-floating-promises".into())),
                            (
                                "options",
                                protocol::object([("ignoreVoid", Value::Bool(ignore_void))]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0);
            assert_eq!(frames.len(), 1);
            let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
            assert!(!diagnostic.contains_key("fixes"));
            if flags.contains(&"-fix-suggestions") {
                let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
                assert_eq!(suggestions.len(), if ignore_void { 2 } else { 1 });
                for suggestion in suggestions {
                    let Value::Object(suggestion) = suggestion else { panic!() };
                    let Value::Array(fixes) = &suggestion["fixes"] else { panic!() };
                    assert_eq!(fixes.len(), 1);
                    let Value::Object(fix) = &fixes[0] else { panic!() };
                    let position = source.rfind("等待").unwrap();
                    assert_eq!(
                        fix["range"],
                        json::unmarshal(&format!("{{\"pos\":{position},\"end\":{position}}}"))
                            .unwrap()
                    );
                }
            } else {
                assert!(!diagnostic.contains_key("suggestions"));
            }
        }
    }
}

#[test]
fn no_for_in_array_checks_structural_union_members_and_header_byte_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-for-in/virtual.ts")
        .to_string_lossy()
        .into_owned();
    for (length_type, count) in [("3", 1), ("string", 0)] {
        let source = format!(
            "declare const arr: {{ [index: number]: unknown; length: {length_type} }} | {{ foo: string }}; for (const 索引 in arr) /*body*/ {{}}"
        );
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([(
                            "name",
                            Value::String("no-for-in-array".into()),
                        )])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.clone()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), count);
        if count != 0 {
            let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
            let start = source.find("for (").unwrap();
            let end = source.find(") /*body*/").unwrap() + 1;
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!("{{\"pos\":{start},\"end\":{end}}}")).unwrap()
            );
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn no_implied_eval_reports_unicode_handlers_and_preserves_local_shadowing() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-implied/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "setTimeout('旧();'); new Function('return 1'); setTimeout(() => {}); function run(setTimeout: (handler: string) => void) { setTimeout('local'); }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-implied-eval".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 2);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    let start = source.find("'旧();'").unwrap();
    let end = start + "'旧();'".len();
    assert_eq!(
        diagnostic["range"],
        json::unmarshal(&format!("{{\"pos\":{start},\"end\":{end}}}")).unwrap()
    );
    for (_, value) in frames {
        let Value::Object(diagnostic) = value else { panic!() };
        assert!(!diagnostic.contains_key("fixes"));
        assert!(!diagnostic.contains_key("suggestions"));
    }
}

#[test]
fn no_meaningless_void_operator_separates_void_fixes_and_never_suggestions() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-meaningless/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "declare function 无(): void; declare function neverReturns(): never; void /*keep*/无(); void neverReturns();";
    for check_never in [false, true] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-meaningless-void-operator".into())),
                            (
                                "options",
                                protocol::object([("checkNever", Value::Bool(check_never))]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0);
            assert_eq!(frames.len(), if check_never { 2 } else { 1 });
            for (index, (_, value)) in frames.iter().enumerate() {
                let Value::Object(diagnostic) = value else { panic!() };
                assert_eq!(diagnostic.contains_key("fixes"), index == 0 && flags.contains(&"-fix"));
                assert_eq!(
                    diagnostic.contains_key("suggestions"),
                    index == 1 && flags.contains(&"-fix-suggestions")
                );
                if index == 0 && flags.contains(&"-fix") {
                    let Value::Array(fixes) = &diagnostic["fixes"] else { panic!() };
                    let Value::Object(fix) = &fixes[0] else { panic!() };
                    let start = source.find("void /*keep*/").unwrap();
                    let end = start + 4;
                    assert_eq!(
                        fix["range"],
                        json::unmarshal(&format!("{{\"pos\":{start},\"end\":{end}}}")).unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn no_misused_promises_preserves_conditional_dedup_and_void_option_fallbacks() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-misused/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 待つ: Promise<void>; declare function take(fn: () => void): void; if (待つ && 待つ) {} take(async () => {}); const object = { ...待つ };";
    for (options, count) in [
        ("{}", 4),
        (r#"{"checksVoidReturn":{"arguments":false}}"#, 3),
        (r#"{"checksConditionals":false,"checksSpreads":false,"checksVoidReturn":false}"#, 0),
        (r#"{"checksVoidReturn":{"arguments":false,"properties":"bad"}}"#, 4),
        (r#"{"checksVoidReturn":"unexpected"}"#, 3),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-misused-promises".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 0, "{options}: {frames:?}");
        assert_eq!(frames.len(), count, "{options}: {frames:?}");
        for (_, value) in frames {
            let Value::Object(diagnostic) = value else { panic!() };
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn no_misused_spread_gates_await_suggestions_and_keeps_unicode_byte_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-spread/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 等待: Promise<void>; const object = { ...等待 };";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-misused-spread".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        assert!(!diagnostic.contains_key("fixes"));
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
            assert_eq!(suggestions.len(), 1);
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            let position = source.rfind("等待").unwrap();
            assert_eq!(
                suggestion["fixes"],
                json::unmarshal(&format!(
                    r#"[{{"text":"await ","range":{{"pos":{position},"end":{position}}}}}]"#
                ))
                .unwrap()
            );
        } else {
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn no_mixed_enums_uses_first_merged_definition_and_unicode_member_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-enums/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; enum 果物 { 林檎 = 'a' } enum 果物 { 梨, 桃 = 0 }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-mixed-enums".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    let pos = source.find('梨').unwrap();
    let end = pos + '梨'.len_utf8();
    assert_eq!(
        diagnostic["range"],
        json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
    );
    assert!(!diagnostic.contains_key("fixes"));
    assert!(!diagnostic.contains_key("suggestions"));
}

#[test]
fn no_redundant_type_constituents_quotes_aliased_literals_and_unicode_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-redundant/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; type 組 = 'x\u{a0}\u{61c}'; type U = 組 | string;";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-redundant-type-constituents".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 1);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    let pos = source.rfind('組').unwrap();
    let end = pos + '組'.len_utf8();
    assert_eq!(
        diagnostic["range"],
        json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
    );
    assert_eq!(
        diagnostic["message"],
        protocol::object([
            ("id", Value::String("literalOverridden".into())),
            (
                "description",
                Value::String(
                    r#""\"x\u00a0\u061c\"" is overridden by string in this union type."#.into()
                )
            ),
        ])
    );
    assert!(!diagnostic.contains_key("fixes"));
    assert!(!diagnostic.contains_key("suggestions"));
}

#[test]
fn no_redundant_type_constituents_uses_pinned_surrogate_semantics() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-redundant/virtual.ts")
        .to_string_lossy()
        .into_owned();
    for (literal, expected) in [
        (r"\uD800", "�"),
        (r"\uDC00", "�"),
        (r"\uD800\uDC00", "𐀀"),
        (r"\u{D800}", "�"),
        (r"\u{DC00}", "�"),
        (r"\u{D800}\u{DC00}", "��"),
        (r"\uD800\u{DC00}", "��"),
        (r"\u{D800}\uDC00", "��"),
    ] {
        for direct in [false, true] {
            let constituent = if direct { format!("\"{literal}\"") } else { "A".into() };
            let source =
                format!("export {{}}; type A = \"{literal}\"; type X = {constituent} | string;");
            let payload = protocol::object([
                ("version", Value::Integer(2)),
                (
                    "configs",
                    Value::Array(vec![protocol::object([
                        ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                        (
                            "rules",
                            Value::Array(vec![protocol::object([(
                                "name",
                                Value::String("no-redundant-type-constituents".into()),
                            )])]),
                        ),
                    ])]),
                ),
                (
                    "source_overrides",
                    Value::Object(std::iter::once((file.clone(), Value::String(source))).collect()),
                ),
            ]);
            let (status, frames) = run(&payload, &[]);
            assert_eq!(status, 0);
            assert_eq!(frames.len(), 1);
            let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            let text = if direct { expected.into() } else { format!("\"\\\"{expected}\\\"\"") };
            assert_eq!(
                message["description"],
                Value::String(format!("{text} is overridden by string in this union type.")),
                "{literal}, direct={direct}"
            );
        }
    }
}

#[test]
fn boolean_literal_compare_initializes_on_empty_files_without_strict_null_checks() {
    let directory = std::env::current_dir().unwrap().join("target/lint-boolean-empty");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":false},"files":["empty.ts"]}"#,
    )
    .unwrap();
    std::fs::write(directory.join("empty.ts"), "").unwrap();
    let file = directory.join("empty.ts").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unnecessary-boolean-literal-compare".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(String::new()))).collect()),
        ),
    ]);
    let (status, frames) = run(&payload, &["-debug", "timings"]);
    assert_eq!(status, 0);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].0, 1);
    let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
    assert_eq!(diagnostic["range"], json::unmarshal(r#"{"pos":0,"end":0}"#).unwrap());
    let Value::Object(message) = &diagnostic["message"] else { panic!() };
    assert_eq!(message["id"], Value::String("noStrictNullCheck".into()));
    assert_eq!(frames[1].0, 2);
    let Value::Object(timing_frame) = &frames[1].1 else { panic!() };
    let Value::Array(timings) = &timing_frame["rules"] else { panic!() };
    let Value::Object(timing) = &timings[0] else { panic!() };
    assert_eq!(timing["calls"], Value::Number(1.0));
}

#[test]
fn no_unnecessary_condition_keeps_unicode_help_and_loop_option_fallbacks() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-condition/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 文字: string; if (文字 === null) {} while (true) {} const x = { foo: 1 }; x?.foo;";
    for (mode, count) in [
        (r#""never""#, 3),
        ("true", 2),
        (r#""only-allowed-literals""#, 2),
        ("false", 3),
        ("23", 3),
        (r#""unexpected""#, 3),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-unnecessary-condition".into())),
                            (
                                "options",
                                protocol::object([(
                                    "allowConstantLoopConditions",
                                    json::unmarshal(mode).unwrap(),
                                )]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), count, "{mode}: {frames:?}");
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let pos = source.find("文字 === null").unwrap();
        let end = pos + "文字 === null".len();
        assert_eq!(
            diagnostic["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        assert!(!diagnostic.contains_key("labeled_ranges"));
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(
            message["help"],
            Value::String(
                "The left side has type `string`, and the right side has type `null`.".into()
            )
        );
        for (_, value) in frames {
            let Value::Object(diagnostic) = value else { panic!() };
            assert!(!diagnostic.contains_key("fixes"));
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            if message["id"] == Value::String("neverOptionalChain".into()) {
                let pos = source.find("?.").unwrap();
                let end = pos + 2;
                let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
                assert_eq!(suggestions.len(), 1);
                let Value::Object(suggestion) = &suggestions[0] else { panic!() };
                assert_eq!(
                    suggestion["fixes"],
                    json::unmarshal(&format!(
                        r#"[{{"text":".","range":{{"pos":{pos},"end":{end}}}}}]"#
                    ))
                    .unwrap()
                );
            } else {
                assert!(!diagnostic.contains_key("suggestions"));
            }
        }
    }
}

#[test]
fn no_unnecessary_qualifier_resets_nested_suppression_and_keeps_utf8_fix_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-qualifier/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; namespace 名 { export const 値 = 1; export const r = 名.値; export namespace 内 { export const 値 = 2; export const r = 名.内.値; } export const t = 名.値; }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unnecessary-qualifier".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 3);
        let positions = [
            (source.find("名.値").unwrap(), "名."),
            (source.find("名.内.値").unwrap(), "名.内."),
            (source.rfind("名.値").unwrap(), "名."),
        ];
        for ((_, value), (pos, prefix)) in frames.into_iter().zip(positions) {
            let Value::Object(diagnostic) = value else { panic!() };
            let end = pos + prefix.len() - 1;
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            assert!(!diagnostic.contains_key("suggestions"));
            if flags.contains(&"-fix") {
                let end = pos + prefix.len();
                assert_eq!(
                    diagnostic["fixes"],
                    json::unmarshal(&format!(
                        r#"[{{"text":"","range":{{"pos":{pos},"end":{end}}}}}]"#
                    ))
                    .unwrap()
                );
            } else {
                assert!(!diagnostic.contains_key("fixes"));
            }
        }
    }
}

#[test]
fn template_expression_preserves_dollar_boundaries_comments_and_utf8_ranges() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-template/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; const x = `é${'$'}{😀}`; const untouched = `${/*keep*/ 'x'}`;";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unnecessary-template-expression".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let pos = source.find("${'$'}").unwrap();
        let end = pos + 6;
        assert_eq!(
            diagnostic["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        assert!(!diagnostic.contains_key("suggestions"));
        if flags.contains(&"-fix") {
            let quote = pos + 2;
            let after_quote = pos + 5;
            assert_eq!(diagnostic["fixes"], json::unmarshal(&format!(r#"[{{"text":"","range":{{"pos":{pos},"end":{quote}}}}},{{"text":"","range":{{"pos":{after_quote},"end":{end}}}}},{{"text":"\\$","range":{{"pos":{quote},"end":{after_quote}}}}}]"#)).unwrap());
        } else {
            assert!(!diagnostic.contains_key("fixes"));
        }
    }
}

#[test]
fn template_expression_handles_unterminated_utf8_literals_like_pinned_go() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-template/virtual.ts")
        .to_string_lossy()
        .into_owned();
    for source in [
        "export {}; const v = `a${\"é",
        "export {}; const v = `a${`é",
        "export {}; const v = `a${\"😀",
        "export {}; type X = `a${\"é",
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([(
                            "name",
                            Value::String("no-unnecessary-template-expression".into()),
                        )])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{source:?}: {frames:?}");
            assert_eq!(
                frames.len(),
                usize::from(!flags.contains(&"-fix")),
                "{source:?}: {frames:?}"
            );
            if let Some((_, Value::Object(diagnostic))) = frames.first() {
                let pos = source.find("${").unwrap();
                let end = source.len() + 1;
                assert_eq!(
                    diagnostic["range"],
                    json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
                );
                assert!(!diagnostic.contains_key("fixes"));
            }
        }
    }
}

#[test]
fn type_conversion_gates_suggestions_and_keeps_unicode_labels_and_replacements() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-conversion/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 数: number; +数;";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unnecessary-type-conversion".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"]] {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0);
        assert_eq!(frames.len(), 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        let pos = source.rfind('+').unwrap();
        let end = pos + 1;
        assert_eq!(
            diagnostic["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        assert!(!diagnostic.contains_key("fixes"));
        let Value::Array(labels) = &diagnostic["labeled_ranges"] else { panic!() };
        let Value::Object(label) = &labels[0] else { panic!() };
        assert_eq!(
            label["label"],
            Value::String("This expression already has type 'number'.".into())
        );
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
            assert_eq!(suggestions.len(), 2);
            for (suggestion, replacement) in suggestions.iter().zip(["数", "数 satisfies number"])
            {
                let Value::Object(suggestion) = suggestion else { panic!() };
                let end = pos + 1 + "数".len();
                assert_eq!(
                    suggestion["fixes"],
                    json::unmarshal(&format!(
                        r#"[{{"text":"{replacement}","range":{{"pos":{pos},"end":{end}}}}}]"#
                    ))
                    .unwrap()
                );
            }
        } else {
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn default_type_arguments_gate_fixes_and_preserve_constructor_inference() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-type-arguments/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; type 名<T = string> = T; type 双<A, B = string> = [A, B]; type X = 名<string>; type Y = 双<number, /*値*/ string>; class C<T = string> { constructor(value: T) {} } new C<string>('値');";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unnecessary-type-arguments".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 2, "{frames:?}");
        for ((_, diagnostic), (argument, removal)) in frames
            .iter()
            .zip([("名<string>", "<string>"), ("双<number, /*値*/ string>", ", /*値*/ string")])
        {
            let Value::Object(diagnostic) = diagnostic else { panic!() };
            let base = source.find(argument).unwrap();
            let pos = base + argument.rfind("string").unwrap();
            let end = pos + "string".len();
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            assert!(!diagnostic.contains_key("suggestions"));
            if flags.contains(&"-fix") {
                let pos = base + argument.find(removal).unwrap();
                let end = pos + removal.len();
                assert_eq!(
                    diagnostic["fixes"],
                    json::unmarshal(&format!(
                        r#"[{{"text":"","range":{{"pos":{pos},"end":{end}}}}}]"#
                    ))
                    .unwrap()
                );
            } else {
                assert!(!diagnostic.contains_key("fixes"));
            }
        }
    }
}

#[test]
fn type_parameter_suggestions_replace_unicode_signature_and_body_references() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-type-parameters/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; function 函<型 extends string | number>(值: 型[]): void { const body: 型 = 值[0]; }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unnecessary-type-parameters".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        assert!(!diagnostic.contains_key("fixes"));
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(
            message["description"],
            Value::String("Type parameter 型 is used only once in the function signature.".into())
        );
        let Value::Array(labels) = &diagnostic["labeled_ranges"] else { panic!() };
        let Value::Object(label) = &labels[0] else { panic!() };
        let pos = source.find("型[]").unwrap();
        let end = pos + "型".len();
        assert_eq!(
            label["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            let body_pos = source.find("型 =").unwrap();
            let body_end = body_pos + "型".len();
            let remove_pos = source.find('<').unwrap();
            let remove_end = source.find('>').unwrap() + 1;
            assert_eq!(suggestion["fixes"], json::unmarshal(&format!(r#"[{{"text":"(string | number)","range":{{"pos":{pos},"end":{end}}}}},{{"text":"string | number","range":{{"pos":{body_pos},"end":{body_end}}}}},{{"text":"","range":{{"pos":{remove_pos},"end":{remove_end}}}}}]"#)).unwrap());
        } else {
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn type_assertion_fixes_match_go_comment_removal_and_unicode_and_skip_unassigned_variables() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-type-assertion/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 名: string; const 値 = 名 as /*保*/ string; 名!; let 未: string; 未!;";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unnecessary-type-assertion".into())),
                        (
                            "options",
                            protocol::object([("typesToIgnore", Value::Array(vec![Value::Null]))]),
                        ),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 2, "{frames:?}");
        let Value::Object(assertion) = &frames[0].1 else { panic!() };
        let Value::Object(nonnull) = &frames[1].1 else { panic!() };
        for diagnostic in [assertion, nonnull] {
            assert!(!diagnostic.contains_key("suggestions"));
            assert!(!diagnostic.contains_key("labeled_ranges"));
        }
        if flags.contains(&"-fix") {
            let keyword_pos = source.find(" as").unwrap();
            let type_pos = source.find("/*保*/ string").unwrap() + "/*保*/ ".len();
            let type_end = type_pos + "string".len();
            assert_eq!(
                assertion["fixes"],
                json::unmarshal(&format!(
                    r#"[{{"text":"","range":{{"pos":{keyword_pos},"end":{type_end}}}}}]"#
                ))
                .unwrap()
            );
            let pos = source.find('!').unwrap();
            let end = pos + 1;
            assert_eq!(
                nonnull["fixes"],
                json::unmarshal(&format!(r#"[{{"text":"","range":{{"pos":{pos},"end":{end}}}}}]"#))
                    .unwrap()
            );
        } else {
            assert!(!assertion.contains_key("fixes"));
            assert!(!nonnull.contains_key("fixes"));
        }
    }
}

#[test]
fn semantic_reporting_reuses_the_lint_checker_in_serial_and_parallel_runs() {
    let base = std::env::current_dir().unwrap().join("target/lint-assertion-semantic");
    let files: Vec<_> = (0..2)
        .map(|index| base.join(format!("{index}.ts")).to_string_lossy().into_owned())
        .collect();
    let source = "export {}; declare function factory<T = unknown>(): T; declare function takeString(value: string): void; const a = factory() as string; takeString(a); declare const n: number; n as number; const wrong: string = 1;";
    let mut overrides: tsrs_core::collections::OrderedMap<_, _> =
        files.iter().map(|file| (file.clone(), Value::String(source.into()))).collect();
    overrides.insert(base.join("tsconfig.json").to_string_lossy().into_owned(), Value::String(r#"{"compilerOptions":{"strict":true,"noLib":true,"types":[]},"files":["0.ts","1.ts"]}"#.into()));
    for semantic in [false, true] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            ("report_semantic", Value::Bool(semantic)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    (
                        "file_paths",
                        Value::Array(files.iter().cloned().map(Value::String).collect()),
                    ),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([(
                            "name",
                            Value::String("no-unnecessary-type-assertion".into()),
                        )])]),
                    ),
                ])]),
            ),
            ("source_overrides", Value::Object(overrides.clone())),
        ]);
        let input = json::marshal(&payload).unwrap();
        for threads in ["1", "2"] {
            for flags in
                [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
            {
                let (status, frames) = run_raw_with_threads(&input, &flags, Some(threads));
                assert_eq!(status, 0, "{frames:?}");
                for file in &files {
                    let diagnostics: Vec<_> = frames
                        .iter()
                        .filter_map(|(_, value)| match value {
                            Value::Object(diagnostic)
                                if diagnostic.get("file_path")
                                    == Some(&Value::String(file.clone())) =>
                            {
                                Some(diagnostic)
                            }
                            _ => None,
                        })
                        .collect();
                    let lint: Vec<_> = diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.contains_key("rule"))
                        .collect();
                    let expressions: &[&str] = if semantic {
                        &["factory() as string", "n as number"]
                    } else {
                        &["n as number"]
                    };
                    assert_eq!(
                        lint.len(),
                        expressions.len(),
                        "{semantic} {threads} {flags:?}: {frames:?}"
                    );
                    for (diagnostic, expression) in lint.iter().zip(expressions) {
                        let start = source.find(expression).unwrap();
                        let end = start + expression.len();
                        assert_eq!(
                            diagnostic["range"],
                            json::unmarshal(&format!(r#"{{"pos":{start},"end":{end}}}"#)).unwrap()
                        );
                        let Value::Object(message) = &diagnostic["message"] else { panic!() };
                        assert_eq!(message["id"], Value::String("unnecessaryAssertion".into()));
                        if flags.contains(&"-fix") {
                            let start = start + expression.find(" as ").unwrap();
                            assert_eq!(
                                diagnostic["fixes"],
                                json::unmarshal(&format!(
                                    r#"[{{"text":"","range":{{"pos":{start},"end":{end}}}}}]"#
                                ))
                                .unwrap()
                            );
                        } else {
                            assert!(!diagnostic.contains_key("fixes"));
                        }
                        assert!(!diagnostic.contains_key("suggestions"));
                    }
                    let compiler: Vec<_> = diagnostics
                        .iter()
                        .filter(|diagnostic| !diagnostic.contains_key("rule"))
                        .collect();
                    assert_eq!(compiler.len(), usize::from(semantic), "{frames:?}");
                    if let Some(diagnostic) = compiler.first() {
                        let Value::Object(message) = &diagnostic["message"] else { panic!() };
                        assert_eq!(message["id"], Value::String("TS2322".into()));
                    }
                }
            }
        }
    }
}

#[test]
fn default_assignments_keep_unicode_ranges_and_separate_fixes_from_suggestions() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-default-assignment/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; const obj = { 名: 1 }; const { 名 = 2 } = obj; function f(値: string | undefined = undefined) {}";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-useless-default-assignment".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 2, "{frames:?}");
        let Value::Object(property) = &frames[0].1 else { panic!() };
        let Value::Object(parameter) = &frames[1].1 else { panic!() };
        let pos = source.find("名 = 2").unwrap() + "名 = ".len();
        let end = pos + 1;
        assert_eq!(
            property["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        assert!(!property.contains_key("fixes"));
        assert!(!parameter.contains_key("fixes"));
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &property["suggestions"] else { panic!() };
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            let pos = source.find("名 = 2").unwrap() + "名".len();
            assert_eq!(
                suggestion["fixes"],
                json::unmarshal(&format!(r#"[{{"text":"","range":{{"pos":{pos},"end":{end}}}}}]"#))
                    .unwrap()
            );
        } else {
            assert!(!property.contains_key("suggestions"));
        }
        if flags.contains(&"-fix-suggestions") {
            let pos = source.find(" = undefined").unwrap();
            let end = pos + " = undefined".len();
            let name_end = source.find("値:").unwrap() + "値".len();
            assert_eq!(parameter["suggestions"], json::unmarshal(&format!(r#"[{{"message":{{"id":"useOptionalSyntax","description":"Use the `?` optional syntax instead."}},"fixes":[{{"text":"","range":{{"pos":{pos},"end":{end}}}}},{{"text":"?","range":{{"pos":{name_end},"end":{name_end}}}}}]}}]"#)).unwrap());
        } else {
            assert!(!parameter.contains_key("suggestions"));
        }
    }
}

#[test]
fn unsafe_arguments_keep_unicode_ranges_and_consume_variadic_tuple_and_template_parameters() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-argument/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 値: any; declare const 組: [any, ...string[]]; declare function f(x: number, ...rest: string[]): void; f(値); f(...組, 値); declare function tag(s: TemplateStringsArray, x: number): void; tag`${値}`; declare function safe(x: unknown): void; safe(値);";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-argument".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 4, "{frames:?}");
        for ((_, value), (needle, offset, width, id)) in frames.iter().zip([
            ("f(値)", 2, "値".len(), "unsafeArgument"),
            ("...組", 0, "...組".len(), "unsafeTupleSpread"),
            (", 値)", 2, "値".len(), "unsafeArgument"),
            ("${値}", 2, "値".len(), "unsafeArgument"),
        ]) {
            let Value::Object(diagnostic) = value else { panic!() };
            let pos = source.find(needle).unwrap() + offset;
            let end = pos + width;
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String(id.into()));
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
        let Value::Object(last_tuple_argument) = &frames[2].1 else { panic!() };
        let Value::Object(message) = &last_tuple_argument["message"] else { panic!() };
        assert_eq!(
            message["description"],
            Value::String(
                "Unsafe argument of type any assigned to a parameter of type string.".into()
            )
        );
    }
}

#[test]
fn unsafe_assignments_keep_unicode_nested_pattern_and_generic_ranges_without_fixes() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-assignment/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const obj: { 名: any }; const { 名: 値 } = obj; declare const gen: Set<any>; const 安: Set<number> = gen; declare const 危: any; const 未知: unknown = 危;";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-assignment".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 2, "{frames:?}");
        for ((_, value), (needle, id, description)) in frames.iter().zip([
            (
                "値",
                "unsafeArrayPatternFromTuple",
                "Unsafe array destructuring of a tuple element with an any value.",
            ),
            (
                "安: Set<number> = gen",
                "unsafeAssignment",
                "Unsafe assignment between incompatible types.",
            ),
        ]) {
            let Value::Object(diagnostic) = value else { panic!() };
            let pos = source.find(needle).unwrap();
            let end = pos + needle.len();
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String(id.into()));
            assert_eq!(message["description"], Value::String(description.into()));
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn unsafe_calls_keep_unicode_call_new_and_template_ranges_without_fixes() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-call/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 呼: any; 呼(); new 呼(); 呼`字`; declare const 函: Function; 函();";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-call".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 4, "{frames:?}");
        for ((_, value), (needle, width, id, description)) in frames.iter().zip([
            ("呼()", "呼".len(), "unsafeCall", "Unsafe call of a(n) `any` typed value."),
            (
                "new 呼()",
                "new 呼()".len(),
                "unsafeNew",
                "Unsafe construction of a(n) `any` typed value.",
            ),
            (
                "呼`",
                "呼".len(),
                "unsafeTemplateTag",
                "Unsafe use of a(n) `any` typed template tag.",
            ),
            ("函()", "函".len(), "unsafeCall", "Unsafe call of a(n) `Function` typed value."),
        ]) {
            let Value::Object(diagnostic) = value else { panic!() };
            let pos = source.find(needle).unwrap();
            let end = pos + width;
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String(id.into()));
            assert_eq!(message["description"], Value::String(description.into()));
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn enum_comparisons_keep_unicode_labels_and_gate_quoted_member_suggestions() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-enum-comparison/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; enum 枚 { '空 白' = '値' } declare const 値: 枚; 値 === '値'; switch (値) { case '値': break; }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-enum-comparison".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 2, "{frames:?}");
        let Value::Object(comparison) = &frames[0].1 else { panic!() };
        let Value::Object(case) = &frames[1].1 else { panic!() };
        assert!(!comparison.contains_key("fixes"));
        assert!(!case.contains_key("fixes"));
        assert!(!case.contains_key("suggestions"));
        let pos = source.find("値 ===").unwrap();
        let end = pos + "値 === '値'".len();
        assert_eq!(
            comparison["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
        );
        let Value::Array(labels) = &comparison["labeled_ranges"] else { panic!() };
        assert_eq!(labels.len(), 2);
        let Value::Object(label) = &labels[0] else { panic!() };
        let left_end = pos + "値".len();
        assert_eq!(
            label["range"],
            json::unmarshal(&format!(r#"{{"pos":{pos},"end":{left_end}}}"#)).unwrap()
        );
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &comparison["suggestions"] else { panic!() };
            assert_eq!(suggestions.len(), 1);
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            let pos = pos + "値 === ".len();
            assert_eq!(
                suggestion["fixes"],
                json::unmarshal(&format!(
                    r#"[{{"text":"枚['空 白']","range":{{"pos":{pos},"end":{end}}}}}]"#
                ))
                .unwrap()
            );
        } else {
            assert!(!comparison.contains_key("suggestions"));
        }
    }
}

#[test]
fn unsafe_member_access_keeps_chain_state_and_optional_options_per_file() {
    let root = std::env::current_dir().unwrap().join("target/lint-member-access");
    let source = "export {}; declare const 对: any; declare const 键: any; 对.名.次; 对[键].末; 对?.[键]; 对?.名.尾;";
    let files: Vec<_> = ["denied.ts", "allowed.ts"]
        .iter()
        .map(|name| root.join(name).to_string_lossy().into_owned())
        .collect();
    let configs = files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-member-access".into())),
                        (
                            "options",
                            protocol::object([("allowOptionalChaining", Value::Bool(index == 1))]),
                        ),
                    ])]),
                ),
            ])
        })
        .collect();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        (
            "source_overrides",
            Value::Object(
                files.iter().map(|file| (file.clone(), Value::String(source.into()))).collect(),
            ),
        ),
    ]);
    let first = source.find("名.次").unwrap();
    let computed = source.find("键].末").unwrap();
    let optional_computed = source.find("键];").unwrap();
    let optional_name = source.find("名.尾").unwrap();
    let tail = source.find("尾;").unwrap();
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 10, "{frames:?}");
        for (file, expected) in [
            (
                &files[0],
                vec![
                    (first, "unsafeMemberExpression"),
                    (computed, "unsafeMemberExpression"),
                    (computed, "unsafeComputedMemberAccess"),
                    (optional_computed, "unsafeMemberExpression"),
                    (optional_computed, "unsafeComputedMemberAccess"),
                    (optional_name, "unsafeMemberExpression"),
                ],
            ),
            (
                &files[1],
                vec![
                    (first, "unsafeMemberExpression"),
                    (computed, "unsafeMemberExpression"),
                    (computed, "unsafeComputedMemberAccess"),
                    (tail, "unsafeMemberExpression"),
                ],
            ),
        ] {
            let diagnostics: Vec<_> = frames
                .iter()
                .filter_map(|(_, frame)| {
                    let Value::Object(diagnostic) = frame else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone())).then_some(diagnostic)
                })
                .collect();
            assert_eq!(diagnostics.len(), expected.len());
            for (diagnostic, (pos, id)) in diagnostics.into_iter().zip(expected) {
                let end = pos + 3;
                assert_eq!(
                    diagnostic["range"],
                    json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
                );
                let Value::Object(message) = &diagnostic["message"] else { panic!() };
                assert_eq!(message["id"], Value::String(id.into()));
                assert!(!diagnostic.contains_key("fixes"));
                assert!(!diagnostic.contains_key("suggestions"));
            }
        }
    }
}

#[test]
fn unsafe_returns_keep_contextual_unknown_generic_and_unicode_ranges_without_fixes() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-return/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; const 安全: () => unknown = () => (null as any); const 危険 = () => (null as any); function 配列(){return [] as any[];} async function 非同期(){return Promise.resolve(null as any);} const 集合: () => Set<string> = () => new Set<any>();";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-return".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 4, "{frames:?}");
        for ((_, value), (needle, id, description)) in frames.iter().zip([
            ("(null as any); function", "unsafeReturn", "Unsafe return of a value of type `any`."),
            ("return [] as any[];", "unsafeReturn", "Unsafe return of a value of type `any[]`."),
            (
                "return Promise.resolve(null as any);",
                "unsafeReturn",
                "Unsafe return of a value of type `Promise<any>`.",
            ),
            (
                "new Set<any>()",
                "unsafeReturnAssignment",
                "Unsafe return of type `Set<any>` from function with return type `Set<string>`.",
            ),
        ]) {
            let Value::Object(diagnostic) = value else { panic!() };
            let pos = source.find(needle).unwrap();
            let end = pos
                + if needle.starts_with("(null") { "(null as any)".len() } else { needle.len() };
            assert_eq!(
                diagnostic["range"],
                json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
            );
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String(id.into()));
            assert_eq!(message["description"], Value::String(description.into()));
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn unsafe_type_assertions_keep_unicode_narrowing_and_generic_ranges_without_fixes() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-type-assertion/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 値: unknown; 値 as any; declare const 数: number | string; 数 as number; function 関数<型>(入: unknown) { return 入 as 型; }";
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("no-unsafe-type-assertion".into())),
                        ("options", Value::String("ignored".into())),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.into()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 3, "{frames:?}");
        for ((_, value), (needle, id, description)) in frames.iter().zip([
            ("値 as any", "unsafeToAnyTypeAssertion", "Unsafe assertion to `any` detected."),
            ("数 as number", "unsafeTypeAssertion", "Unsafe type assertion: type 'number' is more narrow than the original type."),
            ("入 as 型", "unsafeToUnconstrainedTypeAssertion", "Unsafe type assertion: '型' could be instantiated with an arbitrary type which could be unrelated to the original type."),
        ]) {
            let Value::Object(diagnostic) = value else { panic!() };
            let pos = source.find(needle).unwrap();
            let end = pos + needle.len();
            assert_eq!(diagnostic["range"], json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap());
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String(id.into()));
            assert_eq!(message["description"], Value::String(description.into()));
            assert!(!diagnostic.contains_key("fixes"));
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn unsafe_unary_minus_keeps_unicode_union_and_constraint_ranges_without_fixes() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-unsafe-unary-minus/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let source = "export {}; declare const 数: number | string; declare const 真: boolean; declare const 任: any; -数; -真; -任; +真; function 否<型 extends string>(値: 型) { return -値; }";
    for options in [
        Value::Null,
        Value::Bool(false),
        Value::Integer(42),
        Value::String("ignored".into()),
        Value::Array(vec![Value::Null]),
        protocol::object([("ignored", Value::Bool(true))]),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("no-unsafe-unary-minus".into())),
                            ("options", options),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String(source.into()))).collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{frames:?}");
            assert_eq!(frames.len(), 3, "{frames:?}");
            for ((kind, value), (needle, ty)) in
                frames.iter().zip([("-数", "string"), ("-真", "false"), ("-値", "string")])
            {
                let pos = source.find(needle).unwrap();
                let end = pos + needle.len();
                assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                let expected = protocol::Diagnostic {
                        range: Some(protocol::Range {
                            pos: pos.try_into().unwrap(),
                            end: end.try_into().unwrap(),
                        }),
                        message: protocol::RuleMessage {
                            id: "unaryMinus".into(),
                            description: format!(
                                "Argument of unary negation should be assignable to number | bigint but is {ty} instead."
                            ),
                            ..Default::default()
                        },
                        file_path: Some(file.clone()),
                        rule: Some("no-unsafe-unary-minus".into()),
                        ..Default::default()
                    }
                    .to_json();
                assert_eq!(*value, json::unmarshal(&json::marshal(&expected).unwrap()).unwrap(),);
            }
        }
    }
}

#[test]
fn non_nullable_assertions_keep_utf8_precedence_constraints_and_suggestion_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-non-nullable-type-assertion");
    let file = base.join("virtual.ts").to_string_lossy().into_owned();
    let source = "export {};\n\
declare const 値: string | null;\n\
値 /* 保持 */ as string;\n\
<string>値;\n\
declare function nullable(): Promise<string | null>;\n\
async function f() { return <string> /* 保持 */ await nullable(); }\n\
function constrained<T extends string>(items: ArrayLike<T>) { return items[0] as T; }\n\
function unconstrained<T>(items: ArrayLike<T>) { return items[0] as T; }\n\
function nullableConstraint<T extends string | null>(items: ArrayLike<T>) { return items[0] as T; }\n\
function unknownConstraint<T extends unknown>(items: ArrayLike<T>) { return items[0] as T; }\n\
function anyConstraint<T extends any>(items: ArrayLike<T>) { return items[0] as T; }\n\
declare const union: string | number | undefined;\n\
union as string | number;\n\
union as string;\n\
declare const alias: {value: string} | null;\n\
alias as {value: string};\n\
declare const loose: any;\n\
loose as string;\n\
値 as unknown;\n\
[] as const;\n";
    for options in [Value::Null, Value::String("ignored".into())] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("non-nullable-type-assertion-style".into())),
                            ("options", options),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file.clone(), Value::String(source.into())),
                        (
                            base.join("tsconfig.json").to_string_lossy().into_owned(),
                            Value::String(r#"{"compilerOptions":{"strict":true,"noUncheckedIndexedAccess":true,"target":"esnext"},"files":["virtual.ts"]}"#.into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{frames:?}");
            let expected: Vec<_> = [
                ("", "値 /* 保持 */ as string", "値", false),
                ("", "<string>値", "値", false),
                ("", "<string> /* 保持 */ await nullable()", "await nullable()", true),
                ("function constrained", "items[0] as T", "items[0]", false),
                ("function unknownConstraint", "items[0] as T", "items[0]", false),
                ("function anyConstraint", "items[0] as T", "items[0]", false),
                ("", "union as string | number", "union", false),
            ]
            .into_iter()
            .map(|(scope, needle, expression, parenthesize)| {
                let scope_pos = source.find(scope).unwrap();
                let pos = scope_pos + source[scope_pos..].find(needle).unwrap();
                let end = pos + needle.len();
                let expression_pos = pos + needle.rfind(expression).unwrap();
                let expression_end = expression_pos + expression.len();
                let message = protocol::RuleMessage {
                    id: "preferNonNullAssertion".into(),
                    description: "Use a ! assertion to more succinctly remove null and undefined from the type.".into(),
                    ..Default::default()
                };
                let mut fixes = vec![protocol::Fix {
                    range: protocol::Range { pos: expression_end as u32, end: end as u32 },
                    text: String::new(),
                }];
                if parenthesize {
                    fixes.push(protocol::Fix {
                        range: protocol::Range { pos: expression_pos as u32, end: expression_pos as u32 },
                        text: "(".into(),
                    });
                }
                fixes.push(protocol::Fix {
                    range: protocol::Range { pos: expression_end as u32, end: expression_end as u32 },
                    text: if parenthesize { ")!" } else { "!" }.into(),
                });
                let suggestions = if flags.contains(&"-fix-suggestions") {
                    vec![protocol::Suggestion { message: message.clone(), fixes }]
                } else {
                    Vec::new()
                };
                let diagnostic = protocol::Diagnostic {
                    rule: Some("non-nullable-type-assertion-style".into()),
                    file_path: Some(file.clone()),
                    range: Some(protocol::Range { pos: pos as u32, end: end as u32 }),
                    message,
                    suggestions,
                    ..Default::default()
                }.to_json();
                (1, json::unmarshal(&json::marshal(&diagnostic).unwrap()).unwrap())
            })
            .collect();
            assert_eq!(frames, expected);
        }
    }
}

#[test]
fn only_throw_error_package_mismatch_resolves_virtual_dependency() {
    let root =
        std::env::temp_dir().join(format!("tsrs-lint-only-throw-package-{}", std::process::id()));
    let package = root.join("node_modules/errors-like");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"target":"ESNext","module":"NodeNext","moduleResolution":"NodeNext","types":[]},"files":["main.ts"]}"#,
    )
    .unwrap();
    let file = root.join("main.ts").to_string_lossy().into_owned();
    std::fs::write(&file, "").unwrap();
    let source = "import { createError } from 'errors-like'; throw createError();";
    for allowed_package in ["errors", "errors-like"] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            ("report_semantic", Value::Bool(true)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("only-throw-error".into())),
                            (
                                "options",
                                protocol::object([
                                    ("allowThrowingAny", Value::Bool(false)),
                                    ("allowThrowingUnknown", Value::Bool(false)),
                                    (
                                        "allow",
                                        Value::Array(vec![protocol::object([
                                            ("from", Value::String("package".into())),
                                            ("name", Value::String("ErrorLike".into())),
                                            ("package", Value::String(allowed_package.into())),
                                        ])]),
                                    ),
                                ]),
                            ),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file.clone(), Value::String(source.into())),
                        (
                            package.join("package.json").to_string_lossy().into_owned(),
                            Value::String(r#"{"name":"errors-like","version":"1.0.0","types":"index.d.ts"}"#.into()),
                        ),
                        (
                            package.join("index.d.ts").to_string_lossy().into_owned(),
                            Value::String("export interface ErrorLike { message: string; } export function createError(): ErrorLike;".into()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{frames:?}");
            assert_eq!(frames.len(), usize::from(allowed_package == "errors"), "{frames:?}");
            if let Some((_, Value::Object(diagnostic))) = frames.first() {
                assert_eq!(diagnostic["rule"], Value::String("only-throw-error".into()));
                let Value::Object(message) = &diagnostic["message"] else { panic!() };
                assert_eq!(message["id"], Value::String("object".into()));
                let pos = source.find("createError()").unwrap();
                let end = pos + "createError()".len();
                assert_eq!(
                    diagnostic["range"],
                    json::unmarshal(&format!(r#"{{"pos":{pos},"end":{end}}}"#)).unwrap()
                );
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn prefer_find_preserves_utf8_comments_and_independent_suggestion_flags() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-find/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let prefix = "export {}; const 名 = 'filter'; const 零 = -0n; declare const 数组: number[]; ";
    let expression =
        "(true ? 数组[名](x => x) : (0, 数组.filter(x => x))) /* 汉 [ . */\u{2003}[零]";
    let source = format!("{prefix}{expression};");
    let expected = format!(
        "{prefix}(true ? 数组[\"find\"](x => x) : (0, 数组.find(x => x))) /* 汉 [ . */\u{2003};"
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("prefer-find".into())),
                        ("options", Value::Bool(true)),
                    ])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file, Value::String(source.clone()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0].0, 1);
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        assert_eq!(
            diagnostic["message"],
            protocol::object([
                ("id", Value::String("preferFind".into())),
                (
                    "description",
                    Value::String("Prefer .find(...) instead of .filter(...)[0].".into())
                ),
            ])
        );
        assert_eq!(
            diagnostic["range"],
            protocol::object([
                ("pos", Value::Number(prefix.len() as f64)),
                ("end", Value::Number((prefix.len() + expression.len()) as f64)),
            ])
        );
        assert!(!diagnostic.contains_key("fixes"));
        assert!(!diagnostic.contains_key("labeled_ranges"));
        if flags.contains(&"-fix-suggestions") {
            let Value::Array(suggestions) = &diagnostic["suggestions"] else { panic!() };
            assert_eq!(suggestions.len(), 1);
            let Value::Object(suggestion) = &suggestions[0] else { panic!() };
            assert_eq!(
                suggestion["message"],
                protocol::object([
                    ("id", Value::String("preferFindSuggestion".into())),
                    (
                        "description",
                        Value::String("Use .find(...) instead of .filter(...)[0].".into())
                    ),
                ])
            );
            let Value::Array(fixes) = &suggestion["fixes"] else { panic!() };
            assert_eq!(fixes.len(), 3);
            let mut result = source.clone();
            for fix in fixes.iter().rev() {
                let Value::Object(fix) = fix else { panic!() };
                let Value::Object(range) = &fix["range"] else { panic!() };
                let Value::Number(pos) = range["pos"] else { panic!() };
                let Value::Number(end) = range["end"] else { panic!() };
                let Value::String(text) = &fix["text"] else { panic!() };
                result.replace_range(pos as usize..end as usize, text);
            }
            assert_eq!(result, expected);
        } else {
            assert!(!diagnostic.contains_key("suggestions"));
        }
    }
}

#[test]
fn prefer_find_static_coercions_and_const_resolution_stay_per_file() {
    let root = std::env::current_dir().unwrap().join("target/lint-prefer-find-static");
    let scenarios = [
        ("const zero = +false; const alias = zero; [1].filter(Boolean).at(alias);", 1),
        ("const zero = 2; const alias = zero; [1].filter(Boolean).at(alias);", 0),
        ("const first = second; const second = first; [1].filter(Boolean).at(first);", 0),
        ("let zero = 0; [1].filter(Boolean)[zero];", 0),
        (
            "[1].filter(Boolean).at('1e309'); [1].filter(Boolean).at('1e-999'); [1].filter(Boolean).at('0_0'); [1].filter(Boolean).at('-0x0');",
            4,
        ),
        (
            "[1].filter(Boolean).at('1_0'); [1].filter(Boolean).at(0n); [1].filter(Boolean).at(Symbol.for('0')); [1].filter(Boolean).at(Infinity);",
            0,
        ),
        (
            "[1].filter(Boolean).at('0x-1'); [1].filter(Boolean).at('0x+1'); [1].filter(Boolean).at('0b-1'); [1].filter(Boolean).at('0o+1');",
            0,
        ),
        (
            "[1].filter(Boolean).at('+0x+1'); [1].filter(Boolean).at('-0x-1'); [1].filter(Boolean).at('0x-0'); [1].filter(Boolean).at('0x+0');",
            4,
        ),
        (
            "[1].filter(Boolean).at(null); [1].filter(Boolean).at(undefined); [1].filter(Boolean).at(false); [1].filter(Boolean).at(+null);",
            4,
        ),
        (
            "[1].filter(Boolean)[false]; [1].filter(Boolean)[null]; [1].filter(Boolean)[undefined]; [1].filter(Boolean)[NaN]; [1].filter(Boolean)['00'];",
            0,
        ),
        (
            "[1].filter(Boolean)[-0x0n]; [1].filter(Boolean)[0o0n]; [1].filter(Boolean)[0b0n]; [1].filter(Boolean)[0n as bigint];",
            4,
        ),
        (
            "[1].filter(Boolean)[+0n]; [1].filter(Boolean)[0 satisfies number]; [1].filter(Boolean).at(void 0);",
            0,
        ),
        ("declare const values: number[] & {tag: true}; values.filter(Boolean)[0];", 0),
        ("declare const values: number[] | readonly number[]; values.filter(Boolean)[0];", 1),
    ];
    let paths: Vec<_> = (0..scenarios.len())
        .map(|index| root.join(format!("case-{index}.ts")).to_string_lossy().into_owned())
        .collect();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(
                paths
                    .iter()
                    .enumerate()
                    .map(|(index, path)| {
                        protocol::object([
                            ("file_paths", Value::Array(vec![Value::String(path.clone())])),
                            (
                                "rules",
                                Value::Array(vec![protocol::object([
                                    ("name", Value::String("prefer-find".into())),
                                    (
                                        "options",
                                        if index % 2 == 0 {
                                            Value::String("ignored".into())
                                        } else {
                                            Value::Array(vec![Value::Integer(1)])
                                        },
                                    ),
                                ])]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "source_overrides",
            Value::Object(
                paths
                    .iter()
                    .zip(scenarios)
                    .map(|(path, (source, _))| {
                        (path.clone(), Value::String(format!("export {{}}; {source}")))
                    })
                    .collect(),
            ),
        ),
    ]);
    let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
    assert_eq!(status, 0, "{frames:?}");
    for (path, (source, expected)) in paths.iter().zip(scenarios) {
        let count = frames
            .iter()
            .filter(|(_, frame)| {
                let Value::Object(diagnostic) = frame else { panic!() };
                diagnostic["file_path"] == Value::String(path.clone())
            })
            .count();
        assert_eq!(count, expected, "{source}: {frames:?}");
    }
}

#[test]
fn prefer_includes_preserves_pinned_edges_utf8_options_and_fix_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-includes");
    let first = base.join("first.ts").to_string_lossy().into_owned();
    let second = base.join("second.ts").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(first.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-includes".into())),
                            ("options", Value::Array(vec![Value::Integer(42)])),
                        ])]),
                    ),
                ]),
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(second.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-includes".into())),
                            ("options", Value::String("invalid".into())),
                        ])]),
                    ),
                ]),
            ]),
        ),
        (
            "source_overrides",
            Value::Object(
                [
                    (
                        first,
                        Value::String(include_str!("fixtures/prefer_includes/first.ts").into()),
                    ),
                    (
                        second,
                        Value::String(include_str!("fixtures/prefer_includes/second.ts").into()),
                    ),
                    (
                        base.join("tsconfig.json").to_string_lossy().into_owned(),
                        Value::String(
                            r#"{"compilerOptions":{"target":"esnext","strict":true},"files":["first.ts","second.ts"]}"#.into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        ),
    ]);
    let Value::Array(expected) =
        json::unmarshal(include_str!("fixtures/prefer_includes/diagnostics.json")).unwrap()
    else {
        panic!()
    };
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        let mut actual: Vec<_> = frames
            .into_iter()
            .map(|(kind, value)| {
                assert_eq!(kind, 1);
                value
            })
            .collect();
        let mut expected = expected.clone();
        for diagnostic in &mut expected {
            let Value::Object(fields) = diagnostic else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            fields.insert(
                "file_path".into(),
                Value::String(base.join(file).to_string_lossy().into_owned()),
            );
            if !flags.contains(&"-fix") {
                fields.shift_remove("fixes");
            }
        }
        let key = |value: &Value| {
            let Value::Object(fields) = value else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            let Value::Object(range) = &fields["range"] else { panic!() };
            let Value::Number(pos) = range["pos"] else { panic!() };
            (file.clone(), pos as u32)
        };
        actual.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(actual, expected, "flags: {flags:?}");
    }
}

#[test]
fn prefer_includes_keeps_pinned_negated_unicode_category_validation() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-includes-categories/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let patterns = [
        r"\\[\PC\\]",
        r"\\[\PL\\]",
        r"\\[\PM\\]",
        r"\\[\PN\\]",
        r"\\[\PP\\]",
        r"\\[\PS\\]",
        r"\\[\PZ\\]",
        r"\\[\PQ\\]",
        r"\\[a-\PP\\]",
        r"\\[\P\\]",
    ];
    let mut source = String::from("export {}; declare const 字: string; ");
    for (index, pattern) in patterns.iter().enumerate() {
        let literal = json::marshal(&Value::String((*pattern).into())).unwrap();
        write!(source, "const p{index} = new RegExp({literal}); p{index}.test(字); ").unwrap();
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("prefer-includes".into()),
                    )])]),
                ),
            ])]),
        ),
        (
            "source_overrides",
            Value::Object(std::iter::once((file.clone(), Value::String(source.clone()))).collect()),
        ),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 7, "{frames:?}");
        for (index, (kind, value)) in frames.iter().enumerate() {
            let needle = format!("p{index}.test(字)");
            let pos: u32 = source.find(&needle).unwrap().try_into().unwrap();
            let end = pos + u32::try_from(needle.len()).unwrap();
            let argument_pos = pos + u32::try_from(format!("p{index}.test(").len()).unwrap();
            let argument_end = argument_pos + u32::try_from("字".len()).unwrap();
            let fixes = if flags.contains(&"-fix") {
                vec![
                    protocol::Fix {
                        range: protocol::Range { pos, end: argument_pos },
                        text: String::new(),
                    },
                    protocol::Fix {
                        range: protocol::Range { pos: argument_end, end },
                        text: String::new(),
                    },
                    protocol::Fix {
                        range: protocol::Range { pos: argument_end, end: argument_end },
                        text: format!(".includes('{}')", patterns[index]),
                    },
                ]
            } else {
                Vec::new()
            };
            let expected = protocol::Diagnostic {
                range: Some(protocol::Range { pos, end }),
                message: protocol::RuleMessage {
                    id: "preferStringIncludes".into(),
                    description: "Use `String#includes()` method with a string instead.".into(),
                    ..Default::default()
                },
                fixes,
                file_path: Some(file.clone()),
                rule: Some("prefer-includes".into()),
                ..Default::default()
            }
            .to_json();
            assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
            assert_eq!(*value, json::unmarshal(&json::marshal(&expected).unwrap()).unwrap());
        }
    }
}

#[test]
fn prefer_optional_chain_preserves_unicode_comments_flags_and_file_state() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-optional-chain");
    let source = "export {}; const 絵 = '😀'; declare const 値: { 子: number } | undefined; 値 && /* 保持 */ 値.子; 値 != null && 値.子;";
    let any_source = "export {}; declare const 値: any; 値 && 値.子;";
    let mut overrides = vec![(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(r#"{"compilerOptions":{"strict":true},"include":["*.ts"]}"#.into()),
    )];
    let mut files = Vec::new();
    let configs = [
        ("default", source, Value::Null),
        (
            "unsafe",
            source,
            protocol::object([(
                "allowPotentiallyUnsafeFixesThatModifyTheReturnTypeIKnowWhatImDoing",
                Value::Bool(true),
            )]),
        ),
        (
            "defaults",
            source,
            json::unmarshal(r#"{"checkAny":null,"checkBigInt":null,"checkBoolean":null,"checkNumber":null,"checkString":null,"checkUnknown":null,"requireNullish":null,"allowPotentiallyUnsafeFixesThatModifyTheReturnTypeIKnowWhatImDoing":null,"ignored":42}"#).unwrap(),
        ),
        (
            "nullish",
            any_source,
            protocol::object([("requireNullish", Value::Bool(true))]),
        ),
        (
            "any",
            any_source,
            protocol::object([("checkAny", Value::Bool(false))]),
        ),
    ]
    .into_iter()
    .map(|(name, source, options)| {
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        files.push(file.clone());
        overrides.push((file.clone(), Value::String(source.into())));
        protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file)])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-optional-chain".into())),
                    ("options", options),
                ])]),
            ),
        ])
    })
    .collect();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for (fix, suggestions) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut flags = Vec::new();
        if fix {
            flags.push("-fix");
        }
        if suggestions {
            flags.push("-fix-suggestions");
        }
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 6, "{frames:?}");
        for (file_index, file) in files.iter().enumerate().take(3) {
            for (needle, replacement, suggestion) in [
                ("値 && /* 保持 */ 値.子", "/* 保持 */ 値?.子", false),
                ("値 != null && 値.子", "値?.子", file_index != 1),
            ] {
                let pos = source.find(needle).unwrap();
                let range = protocol::Range {
                    pos: pos.try_into().unwrap(),
                    end: (pos + needle.len()).try_into().unwrap(),
                };
                let fixes = vec![protocol::Fix { text: replacement.into(), range }];
                let expected = protocol::Diagnostic {
                    range: Some(range),
                    message: protocol::RuleMessage {
                        id: "preferOptionalChain".into(),
                        description: "Prefer using an optional chain expression instead, as it's more concise and easier to read.".into(),
                        ..Default::default()
                    },
                    file_path: Some(file.clone()),
                    rule: Some("prefer-optional-chain".into()),
                    fixes: if !suggestion && fix { fixes.clone() } else { Vec::new() },
                    suggestions: if suggestion && suggestions {
                        vec![protocol::Suggestion {
                            message: protocol::RuleMessage {
                                id: "optionalChainSuggest".into(),
                                description: "Change to an optional chain.".into(),
                                ..Default::default()
                            },
                            fixes,
                        }]
                    } else { Vec::new() },
                    ..Default::default()
                }.to_json();
                let expected = json::unmarshal(&json::marshal(&expected).unwrap()).unwrap();
                assert!(
                    frames
                        .iter()
                        .any(|(kind, value)| *kind == protocol::MessageType::Diagnostic as u8
                            && *value == expected),
                    "{flags:?}: missing {expected:?} in {frames:?}"
                );
            }
        }
    }
}

#[test]
fn prefer_optional_chain_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-optional-chain-invalid/virtual.ts")
        .to_string_lossy()
        .into_owned();
    let mut options = vec![
        Value::Bool(false),
        Value::Integer(1),
        Value::String("invalid".into()),
        Value::Array(Vec::new()),
    ];
    options.extend(
        [
            "allowPotentiallyUnsafeFixesThatModifyTheReturnTypeIKnowWhatImDoing",
            "checkAny",
            "checkBigInt",
            "checkBoolean",
            "checkNumber",
            "checkString",
            "checkUnknown",
            "requireNullish",
        ]
        .into_iter()
        .map(|name| protocol::object([(name, Value::Integer(1))])),
    );
    for options in options {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-optional-chain".into())),
                            ("options", options),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {};".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn prefer_optional_chain_keeps_parenthesized_guards_and_invalid_utf8_fix_records() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/prefer_optional_chain/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-optional-chain-regressions");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    let config = base.join("tsconfig.json").to_string_lossy().into_owned();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            ("configs", Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                ("rules", Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-optional-chain".into())),
                    ("options", case["options"].clone()),
                ])])),
            ])])),
            ("source_overrides", Value::Object([
                (file.clone(), case["source"].clone()),
                (config.clone(), Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext"},"files":["a.ts"]}"#.into())),
            ].into_iter().collect())),
        ]);
        let Value::Array(expected) = &case["expected"] else { panic!() };
        for (flags, expected) in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
                .iter()
                .zip(expected)
        {
            let (status, frames) = run(&payload, flags);
            assert_eq!(status, 0, "{flags:?}: {case:?}: {frames:?}");
            let Value::Array(expected) = expected else { panic!() };
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
                })
                .collect();
            assert_eq!(frames, expected, "{flags:?}: {case:?}");
        }
    }
}

fn prefer_nullish_payload(
    base: &std::path::Path,
    cases: &[(&str, &str, Value)],
    strict: bool,
) -> Value {
    let files: Vec<_> =
        cases.iter().map(|(name, _, _)| base.join(name).to_string_lossy().into_owned()).collect();
    let configs = cases
        .iter()
        .zip(&files)
        .map(|((_, _, options), file)| {
            protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([
                        ("name", Value::String("prefer-nullish-coalescing".into())),
                        ("options", options.clone()),
                    ])]),
                ),
            ])
        })
        .collect();
    let mut overrides: tsrs_core::collections::OrderedMap<_, _> = cases
        .iter()
        .zip(&files)
        .map(|((_, source, _), file)| (file.clone(), Value::String((*source).into())))
        .collect();
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            json::marshal(&protocol::object([
                (
                    "compilerOptions",
                    protocol::object([
                        ("strict", Value::Bool(strict)),
                        ("target", Value::String("esnext".into())),
                    ]),
                ),
                (
                    "files",
                    Value::Array(
                        cases.iter().map(|(name, _, _)| Value::String((*name).into())).collect(),
                    ),
                ),
            ]))
            .unwrap(),
        ),
    );
    protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ])
}

#[test]
fn prefer_nullish_coalescing_keeps_utf8_ranges_and_independent_suggestion_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-nullish-flags");
    let file = base.join("virtual.ts").to_string_lossy().into_owned();
    let source = "export {};\n\
declare const 値: string | null | undefined;\n\
const α = 値 || \"雪\";\n\
function update(値: string | null | undefined) { 値 ||= \"雪\"; }\n\
const β = 値 == null ? \"雪\" : 値;\n\
function initialize(対象: string | null | undefined) { if (対象 == null) { (((対象))) = \"雪\"; } }\n\
function generic<T extends string | null>(value: T) { return value || \"雪\"; }\n";
    let payload = prefer_nullish_payload(&base, &[("virtual.ts", source, Value::Null)], true);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        let expected: Vec<_> = [
            (
                "||",
                "preferNullishOverOr",
                "Prefer using nullish coalescing operator (`??`) instead of a logical or (`||`), as it is a safer operator.",
                "??",
            ),
            (
                "||=",
                "preferNullishOverOr",
                "Prefer using nullish coalescing operator (`??=`) instead of a logical assignment (`||=`), as it is a safer operator.",
                "??=",
            ),
            (
                "値 == null ? \"雪\" : 値",
                "preferNullishOverTernary",
                "Prefer using nullish coalescing operator (`??`) instead of a ternary expression, as it is simpler to read.",
                "値 ?? \"雪\"",
            ),
            (
                "if (対象 == null) { (((対象))) = \"雪\"; }",
                "preferNullishOverAssignment",
                "Prefer using nullish coalescing operator (`??=`) instead of an assignment expression, as it is simpler to read.",
                "(対象) ??= \"雪\";",
            ),
        ]
        .into_iter()
        .map(|(needle, id, description, replacement)| {
            let pos = source.find(needle).unwrap();
            let range = protocol::Range {
                pos: pos as u32,
                end: (pos + needle.len()) as u32,
            };
            let diagnostic = protocol::Diagnostic {
                rule: Some("prefer-nullish-coalescing".into()),
                file_path: Some(file.clone()),
                range: Some(range),
                message: protocol::RuleMessage {
                    id: id.into(),
                    description: description.into(),
                    ..Default::default()
                },
                suggestions: if flags.contains(&"-fix-suggestions") {
                    vec![protocol::Suggestion {
                        message: protocol::RuleMessage {
                            id: "suggestNullishCoalescing".into(),
                            description: "Change to the nullish coalescing operator.".into(),
                            ..Default::default()
                        },
                        fixes: vec![protocol::Fix {
                            range,
                            text: replacement.into(),
                        }],
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            }
            .to_json();
            (1, json::unmarshal(&json::marshal(&diagnostic).unwrap()).unwrap())
        })
        .collect();
        assert_eq!(frames, expected);
    }
}

#[test]
fn prefer_nullish_coalescing_keeps_per_file_options_context_and_raw_generic_types() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-nullish-context");
    let source = "export {};\n\
declare const 値: string | null;\n\
declare const branded: (string & { readonly tag: unique symbol }) | null;\n\
declare const opaque: unknown;\n\
const direct = 値 || \"雪\";\n\
const intersection = branded || \"雪\";\n\
const unknownValue = opaque || \"雪\";\n\
if (値 || \"雪\") {}\n\
Boolean(値 || \"雪\");\n\
Boolean(値 ? 値 : \"雪\");\n\
Boolean((値 ? 値 : \"雪\"));\n\
const mixed = (値 || \"雪\") && true;\n\
function local(Boolean: (value: unknown) => boolean) { return Boolean(値 || \"雪\"); }\n\
function generic<T extends string | null>(value: T) { return value || \"雪\"; }\n\
function partial<T extends string | null | undefined>(value: T) { return value === null ? \"雪\" : value; }\n";
    let default_lines = &[5, 6, 7, 9, 10, 11, 12, 13, 15][..];
    let cases = [
        ("a.ts", "null", default_lines),
        ("b.ts", "{}", default_lines),
        (
            "c.ts",
            r#"{"ignoreBooleanCoercion":true,"ignoreMixedLogicalExpressions":true}"#,
            &[5, 6, 7, 10, 13, 15][..],
        ),
        (
            "d.ts",
            r#"{"ignoreConditionalTests":false,"ignoreBooleanCoercion":true}"#,
            &[5, 6, 7, 8, 10, 12, 13, 15][..],
        ),
        ("e.ts", r#"{"ignorePrimitives":{"string":true}}"#, &[15][..]),
        ("f.ts", r#"{"ignorePrimitives":true}"#, &[15][..]),
        ("g.ts", r#"{"ignorePrimitives":{"string":true,"number":"invalid"}}"#, default_lines),
        ("h.ts", r#"{"ignorePrimitives":["invalid"]}"#, default_lines),
        ("i.ts", r#"{"ignorePrimitives":{"String":true}}"#, default_lines),
        ("j.ts", r#"{"ignoreTernaryTests":true}"#, &[5, 6, 7, 9, 12, 13][..]),
        (
            "k.ts",
            r#"{"ignoreConditionalTests":null,"ignoreBooleanCoercion":null,"ignoreIfStatements":null,"ignoreMixedLogicalExpressions":null,"ignoreTernaryTests":null,"ignorePrimitives":null,"ignoredOption":true}"#,
            default_lines,
        ),
        ("l.ts", r#"{"ignorePrimitives":42}"#, default_lines),
        ("m.ts", r#"{"ignorePrimitives":"invalid"}"#, default_lines),
    ];
    let input_cases: Vec<_> = cases
        .iter()
        .map(|(name, options, _)| (*name, source, json::unmarshal(options).unwrap()))
        .collect();
    let payload = prefer_nullish_payload(&base, &input_cases, true);
    let (status, frames) = run(&payload, &[]);
    assert_eq!(status, 0, "{frames:?}");
    let mut expected_count = 0;
    for (name, _, lines) in cases {
        let file = base.join(name).to_string_lossy().into_owned();
        let actual: Vec<_> = frames
            .iter()
            .filter(|(_, diagnostic)| {
                matches!(diagnostic, Value::Object(fields) if fields.get("file_path") == Some(&Value::String(file.clone())))
            })
            .cloned()
            .collect();
        let expected: Vec<_> = lines
            .iter()
            .map(|line| {
                let line_index = line - 1;
                let line_text = source.lines().nth(line_index).unwrap();
                let offset: usize = source.split_inclusive('\n').take(line_index).map(str::len).sum();
                let (needle, id, description) = if matches!(line, 10 | 11) {
                    (
                        "値 ? 値 : \"雪\"",
                        "preferNullishOverTernary",
                        "Prefer using nullish coalescing operator (`??`) instead of a ternary expression, as it is simpler to read.",
                    )
                } else if *line == 15 {
                    (
                        "value === null ? \"雪\" : value",
                        "preferNullishOverTernary",
                        "Prefer using nullish coalescing operator (`??`) instead of a ternary expression, as it is simpler to read.",
                    )
                } else {
                    (
                        "||",
                        "preferNullishOverOr",
                        "Prefer using nullish coalescing operator (`??`) instead of a logical or (`||`), as it is a safer operator.",
                    )
                };
                let pos = offset + line_text.find(needle).unwrap();
                let diagnostic = protocol::Diagnostic {
                    rule: Some("prefer-nullish-coalescing".into()),
                    file_path: Some(file.clone()),
                    range: Some(protocol::Range {
                        pos: pos as u32,
                        end: (pos + needle.len()) as u32,
                    }),
                    message: protocol::RuleMessage {
                        id: id.into(),
                        description: description.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }.to_json();
                (1, json::unmarshal(&json::marshal(&diagnostic).unwrap()).unwrap())
            })
            .collect();
        assert_eq!(actual, expected, "{name}");
        expected_count += lines.len();
    }
    assert_eq!(frames.len(), expected_count);
}

#[test]
fn prefer_nullish_coalescing_reports_strictness_per_file_and_rejects_invalid_options() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-nullish-invalid");
    let payload = prefer_nullish_payload(
        &base,
        &[("a.ts", "export {};", Value::Null), ("b.ts", "export {};", Value::Null)],
        false,
    );
    let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
    assert_eq!(status, 0, "{frames:?}");
    let expected: Vec<_> = ["a.ts", "b.ts"].into_iter().map(|name| {
        let diagnostic = protocol::Diagnostic {
            rule: Some("prefer-nullish-coalescing".into()),
            file_path: Some(base.join(name).to_string_lossy().into_owned()),
            range: Some(protocol::Range { pos: 0, end: 0 }),
            message: protocol::RuleMessage {
                id: "noStrictNullCheck".into(),
                description: "This rule requires the `strictNullChecks` compiler option to be turned on to function correctly.".into(),
                ..Default::default()
            },
            ..Default::default()
        }.to_json();
        (1, json::unmarshal(&json::marshal(&diagnostic).unwrap()).unwrap())
    }).collect();
    assert_eq!(frames, expected);
    for options in [r#""invalid""#, "42", "[]", "true"].into_iter().chain([
        r#"{"ignoreBooleanCoercion":"invalid"}"#,
        r#"{"ignoreConditionalTests":42}"#,
        r#"{"ignoreIfStatements":[]}"#,
        r#"{"ignoreMixedLogicalExpressions":{}}"#,
        r#"{"ignoreTernaryTests":"invalid"}"#,
    ]) {
        let payload = prefer_nullish_payload(
            &base,
            &[("a.ts", "export {};", json::unmarshal(options).unwrap())],
            true,
        );
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, 0);
        let Value::Object(fields) = &frames[0].1 else { panic!() };
        let Some(Value::String(error)) = fields.get("error") else { panic!() };
        assert!(error.contains("prefer-nullish-coalescing:"));
    }
}

#[test]
fn prefer_promise_reject_errors_preserves_utf8_options_and_executor_boundaries() {
    let cases = [
        (
            r"export {}; const café = '😀'; (Promise['reject'])(café);",
            r"null",
            vec![r"(Promise['reject'])(café)"],
        ),
        (
            r"export {}; const clé: 'reject' = 'reject'; Promise[clé]?.('é');",
            r"null",
            vec![r"Promise[clé]?.('é')"],
        ),
        (
            r"export {}; new Promise(((résout, rejette) => { (() => (rejette)('😀'))(); function nested() { rejette(1); } const alias = rejette; alias('x'); function inner(rejette: (x: unknown) => void) { rejette(2); } }));",
            r"null",
            vec![r"(rejette)('😀')", r"rejette(1)"],
        ),
        (
            r"export {}; new Promise(undefined, (résout, rejette) => rejette('é'));",
            r"null",
            vec![r"rejette('é')"],
        ),
        (
            r"export {}; class P extends Promise<number> {} new P((résout, rejette) => rejette('é')); P.reject('é');",
            r"null",
            vec![r"P.reject('é')"],
        ),
        (
            r"export {}; declare const é: Error | Readonly<Error>; Promise.reject(é);",
            r"null",
            vec![r"Promise.reject(é)"],
        ),
        (
            r"export {}; function f<T extends Readonly<Readonly<Error>>>(é: T) { Promise.reject(é); }",
            r"null",
            vec![],
        ),
        (
            r"export {}; type Readonly<T> = { reason: T }; declare const é: Readonly<Error>; Promise.reject(é);",
            r"null",
            vec![r"Promise.reject(é)"],
        ),
        (
            r"export {}; const café = 1; Promise.reject(café); Promise.reject(undefined); Promise.reject();",
            r#"{"allow":["number","undefined"]}"#,
            vec![r"Promise.reject(café)", r"Promise.reject()"],
        ),
        (
            r"export {}; declare const é: any; declare const u: unknown; Promise.reject(é); Promise.reject(u); Promise.reject();",
            r"null",
            vec![r"Promise.reject(é)", r"Promise.reject(u)", r"Promise.reject()"],
        ),
        (
            r"export {}; declare const é: any; declare const u: unknown; Promise.reject(é); Promise.reject(u); Promise.reject();",
            r#"{"allowThrowingAny":true,"allowEmptyReject":true}"#,
            vec![r"Promise.reject(u)"],
        ),
        (
            r"export {}; declare const é: any; declare const u: unknown; Promise.reject(é); Promise.reject(u); Promise.reject();",
            r#"{"allowThrowingUnknown":true}"#,
            vec![r"Promise.reject(é)", r"Promise.reject()"],
        ),
        (
            r"export {}; declare const é: any; declare const u: unknown; Promise.reject(é); Promise.reject(u); Promise.reject();",
            r#"{"allow":null,"allowEmptyReject":null,"allowThrowingAny":null,"allowThrowingUnknown":null}"#,
            vec![r"Promise.reject(é)", r"Promise.reject(u)", r"Promise.reject()"],
        ),
        (
            r"export {}; function f<T extends PromiseConstructor>(p: T) { p.reject('😀'); }",
            r"null",
            vec![r"p.reject('😀')"],
        ),
    ];
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut expected = Vec::new();
    for (index, (source, options, calls)) in cases.into_iter().enumerate() {
        let file = std::env::current_dir()
            .unwrap()
            .join(format!("target/lint-prefer-promise-reject-errors/{index}.ts"))
            .to_string_lossy()
            .into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-promise-reject-errors".into())),
                    ("options", json::unmarshal(options).unwrap()),
                ])]),
            ),
        ]));
        for call in calls {
            let pos = source.rfind(call).unwrap();
            expected.push(protocol::object([
                ("kind", Value::Number(0.0)),
                ("file_path", Value::String(file.clone())),
                ("rule", Value::String("prefer-promise-reject-errors".into())),
                (
                    "range",
                    protocol::object([
                        ("pos", Value::Number(pos as f64)),
                        ("end", Value::Number((pos + call.len()) as f64)),
                    ]),
                ),
                (
                    "message",
                    protocol::object([
                        ("id", Value::String("rejectAnError".into())),
                        (
                            "description",
                            Value::String(
                                "Expected the Promise rejection reason to be an Error.".into(),
                            ),
                        ),
                    ]),
                ),
            ]));
        }
        overrides.push((file, Value::String(source.into())));
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), expected.len(), "{frames:?}");
        for diagnostic in &expected {
            assert!(frames.contains(&(1, diagnostic.clone())), "{diagnostic:?}: {frames:?}");
        }
    }
}

#[test]
fn prefer_promise_reject_errors_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-promise-reject-errors/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        r#"{"allow":true}"#,
        r#"{"allowEmptyReject":"true"}"#,
        r#"{"allowThrowingAny":1}"#,
        r#"{"allowThrowingUnknown":[]}"#,
        r#"{"allow":[false]}"#,
        r#"{"allow":[{"from":"lib","name":1}]}"#,
        r#"{"allow":[{"from":"package","package":false}]}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-promise-reject-errors".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("Promise.reject(1);".into())))
                        .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, 0);
    }
}

#[test]
fn prefer_readonly_parameter_types_preserves_utf8_options_and_recursive_state() {
    let cases: [(&str, &str, &[&str]); 20] = [
        (
            "export {}; const 前 = 1; function f(/*é*/ café?: string[]) {}",
            "null",
            &["café?: string[]"],
        ),
        (
            "export {}; class C { constructor(public /*é*/ café: string[] = []) {} }",
            "null",
            &["café: string[] = []"],
        ),
        (
            "export {}; class C { constructor(public café: string[], naïve: string[]) {} }",
            "{\"checkParameterProperties\":false}",
            &["naïve: string[]"],
        ),
        (
            "export {}; class C { constructor(readonly café: string[]) {} }",
            "{\"checkParameterProperties\":null,\"ignoreInferredTypes\":null,\"treatMethodsAsReadonly\":null,\"allow\":null}",
            &["café: string[]"],
        ),
        ("export {}; interface M { method(): void } function f(café: M) {}", "null", &["café: M"]),
        (
            "export {}; interface M { method(): void } function f(café: M) {}",
            "{\"treatMethodsAsReadonly\":true}",
            &[],
        ),
        (
            "export {}; interface M { method(): void } function f(café: M) {}",
            "{\"treatMethodsAsReadonly\":null}",
            &["café: M"],
        ),
        ("export {}; function f(café = []) {}", "null", &["café = []"]),
        ("export {}; function f(café = []) {}", "{\"ignoreInferredTypes\":true}", &[]),
        ("export {}; function f(café = []) {}", "{\"ignoreInferredTypes\":null}", &["café = []"]),
        (
            "export {}; interface Loop {readonly next: Loop; readonly value: {field: number}} function f(café: Loop, naïve: Loop) {}",
            "null",
            &["café: Loop", "naïve: Loop"],
        ),
        (
            "export {}; interface Loop {readonly next: Loop} function f(café: Loop, naïve: Loop) {}",
            "null",
            &[],
        ),
        (
            "export {}; function f<T extends string[]>(café: T, naïve: readonly [string[]]) {}",
            "null",
            &["naïve: readonly [string[]]"],
        ),
        (
            "export {}; type Brand = string & {tag: number[]}; function f(café: Brand, naïve: Brand | number) {}",
            "null",
            &["naïve: Brand | number"],
        ),
        ("export {}; class C { #é: string[] = []; } function f(café: C) {}", "null", &[]),
        (
            "export {}; class C { set café(value: string[]) {} } type Construct = new(value: string[]) => C; function f(café: Readonly<{x: number[]}>) {}",
            "null",
            &["café: Readonly<{x: number[]}>"],
        ),
        (
            "export {}; function f({café}: {café: string[]}, ...naïve: readonly [string[]]) {}",
            "null",
            &["{café}: {café: string[]}", "...naïve: readonly [string[]]"],
        ),
        (
            "export {}; function f(café: {readonly [key: string]: {value: number}}) {}",
            "null",
            &["café: {readonly [key: string]: {value: number}}"],
        ),
        (
            "export {}; interface Allowed {value: number[]} function f(café: Allowed) {}",
            "{\"allow\":[\"Allowed\"]}",
            &[],
        ),
        (
            "export {}; interface Allowed {value: number[]} function f(café: Allowed) {}",
            "null",
            &["café: Allowed"],
        ),
    ];
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut expected = Vec::new();
    for (index, (source, options, parameters)) in cases.into_iter().enumerate() {
        let file = std::env::current_dir()
            .unwrap()
            .join(format!("target/lint-prefer-readonly-parameter-types/{index}.ts"))
            .to_string_lossy()
            .into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-readonly-parameter-types".into())),
                    ("options", json::unmarshal(options).unwrap()),
                ])]),
            ),
        ]));
        for parameter in parameters {
            let pos = source.rfind(parameter).unwrap();
            expected.push((
                1,
                protocol::object([
                    ("kind", Value::Number(0.0)),
                    ("file_path", Value::String(file.clone())),
                    ("rule", Value::String("prefer-readonly-parameter-types".into())),
                    (
                        "range",
                        protocol::object([
                            ("pos", Value::Number(pos as f64)),
                            ("end", Value::Number((pos + parameter.len()) as f64)),
                        ]),
                    ),
                    (
                        "message",
                        protocol::object([
                            ("id", Value::String("shouldBeReadonly".into())),
                            (
                                "description",
                                Value::String("Parameter should be a readonly type.".into()),
                            ),
                        ]),
                    ),
                ]),
            ));
        }
        overrides.push((file, Value::String(source.into())));
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), expected.len(), "{frames:?}");
        for diagnostic in &expected {
            assert!(frames.contains(diagnostic), "missing {diagnostic:?}: {frames:?}");
        }
    }
}

#[test]
fn prefer_readonly_parameter_types_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-readonly-parameter-types/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        r#"{"allow":true}"#,
        r#"{"allow":[false]}"#,
        r#"{"checkParameterProperties":"true"}"#,
        r#"{"ignoreInferredTypes":1}"#,
        r#"{"treatMethodsAsReadonly":[]}"#,
        r#"{"allow":[{"from":"lib","name":false}]}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-readonly-parameter-types".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((
                        file.clone(),
                        Value::String("function f(value: string[]) {}".into()),
                    ))
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, 0, "{options}: {frames:?}");
    }
}

#[test]
fn prefer_regexp_exec_preserves_utf8_options_flags_aliases_and_precedence() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-regexp-exec");
    let first = base.join("first.ts").to_string_lossy().into_owned();
    let second = base.join("second.ts").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(first.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-regexp-exec".into())),
                            ("options", Value::Array(vec![Value::Integer(42)])),
                        ])]),
                    ),
                ]),
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(second.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-regexp-exec".into())),
                            ("options", Value::String("invalid".into())),
                        ])]),
                    ),
                ]),
            ]),
        ),
        (
            "source_overrides",
            Value::Object(
                [
                    (
                        first,
                        Value::String(include_str!("fixtures/prefer_regexp_exec/first.ts").into()),
                    ),
                    (
                        second,
                        Value::String(include_str!("fixtures/prefer_regexp_exec/second.ts").into()),
                    ),
                    (
                        base.join("tsconfig.json").to_string_lossy().into_owned(),
                        Value::String(
                            r#"{"compilerOptions":{"target":"esnext","strict":true},"files":["first.ts","second.ts"]}"#.into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        ),
    ]);
    let Value::Array(expected) =
        json::unmarshal(include_str!("fixtures/prefer_regexp_exec/diagnostics.json")).unwrap()
    else {
        panic!()
    };
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        let mut actual: Vec<_> = frames
            .into_iter()
            .map(|(kind, value)| {
                assert_eq!(kind, 1);
                value
            })
            .collect();
        let mut expected = expected.clone();
        for diagnostic in &mut expected {
            let Value::Object(fields) = diagnostic else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            fields.insert(
                "file_path".into(),
                Value::String(base.join(file).to_string_lossy().into_owned()),
            );
            if !flags.contains(&"-fix") {
                fields.shift_remove("fixes");
            }
        }
        let key = |value: &Value| {
            let Value::Object(fields) = value else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            let Value::Object(range) = &fields["range"] else { panic!() };
            let Value::Number(pos) = range["pos"] else { panic!() };
            (file.clone(), pos as u32)
        };
        actual.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(actual, expected, "flags: {flags:?}");
    }
}

#[test]
fn prefer_regexp_exec_keeps_unicode_predicates_and_conditional_group_options() {
    let Value::Object(fixture) =
        json::unmarshal(include_str!("fixtures/prefer_regexp_exec/audit-regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-regexp-audit-regressions");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    let config = base.join("tsconfig.json").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(vec![protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            ("rules", Value::Array(vec![protocol::object([("name", Value::String("prefer-regexp-exec".into()))])])),
        ])])),
        ("source_overrides", Value::Object([
            (file.clone(), fixture["source"].clone()),
            (config, Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext"},"files":["a.ts"]}"#.into())),
        ].into_iter().collect())),
    ]);
    let Value::Array(diagnostics) = &fixture["diagnostics"] else { panic!() };
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let expected: Vec<_> = diagnostics
            .iter()
            .map(|value| {
                let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                diagnostic.insert("file_path".into(), Value::String(file.clone()));
                if !flags.contains(&"-fix") {
                    diagnostic.shift_remove("fixes");
                }
                (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
            })
            .collect();
        assert_eq!(frames, expected, "{flags:?}");
    }
}

#[test]
fn prefer_readonly_preserves_utf8_options_flags_and_per_file_scope() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/prefer_readonly/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-readonly-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("prefer-readonly".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["expected"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext"},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for (index, flags) in
        [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
            .iter()
            .enumerate()
    {
        let (status, frames) = run(&payload, flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            let Value::Array(expected) = &expected[index] else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn prefer_readonly_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-readonly-invalid/a.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        "1",
        r#""invalid""#,
        r#"{"onlyInlineLambdas":1}"#,
        r#"{"onlyInlineLambdas":[]}"#,
        r#"{"onlyInlineLambdas":"true"}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-readonly".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {};".into()))).collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
    }
}

#[test]
fn prefer_string_starts_ends_with_preserves_oracle_edges_options_and_fix_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-prefer-string-starts-ends-with");
    let first = base.join("first.ts").to_string_lossy().into_owned();
    let second = base.join("second.ts").to_string_lossy().into_owned();
    let mut payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(first.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-string-starts-ends-with".into())),
                            ("options", Value::Null),
                        ])]),
                    ),
                ]),
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(second.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-string-starts-ends-with".into())),
                            (
                                "options",
                                json::unmarshal(r#"{"allowSingleElementEquality":"always"}"#)
                                    .unwrap(),
                            ),
                        ])]),
                    ),
                ]),
            ]),
        ),
        (
            "source_overrides",
            Value::Object(
                [
                    (
                        first,
                        Value::String(
                            include_str!("fixtures/prefer_string_starts_ends_with/first.ts").into(),
                        ),
                    ),
                    (
                        second,
                        Value::String(
                            include_str!("fixtures/prefer_string_starts_ends_with/second.ts")
                                .into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        ),
    ]);
    let Value::Array(expected) =
        json::unmarshal(include_str!("fixtures/prefer_string_starts_ends_with/diagnostics.json"))
            .unwrap()
    else {
        panic!()
    };
    for (flags, options) in [
        (vec![], "null"),
        (vec!["-fix"], r#"{"allowSingleElementEquality":"never"}"#),
        (vec!["-fix-suggestions"], r#"{"allowSingleElementEquality":null,"ignored":true}"#),
        (vec!["-fix", "-fix-suggestions"], "{}"),
    ] {
        let Value::Object(payload_fields) = &mut payload else { panic!() };
        let Value::Array(configs) = payload_fields.get_mut("configs").unwrap() else { panic!() };
        let Value::Object(config) = &mut configs[0] else { panic!() };
        let Value::Array(rules) = config.get_mut("rules").unwrap() else { panic!() };
        let Value::Object(rule) = &mut rules[0] else { panic!() };
        rule.insert("options".into(), json::unmarshal(options).unwrap());
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        let mut actual: Vec<_> = frames
            .into_iter()
            .map(|(kind, value)| {
                assert_eq!(kind, 1);
                value
            })
            .collect();
        let mut expected = expected.clone();
        for diagnostic in &mut expected {
            let Value::Object(fields) = diagnostic else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            fields.insert(
                "file_path".into(),
                Value::String(base.join(file).to_string_lossy().into_owned()),
            );
            if !flags.contains(&"-fix") {
                fields.shift_remove("fixes");
            }
        }
        let key = |value: &Value| {
            let Value::Object(fields) = value else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            let Value::Object(range) = &fields["range"] else { panic!() };
            let Value::Number(pos) = range["pos"] else { panic!() };
            (file.clone(), pos as u32)
        };
        actual.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(actual, expected, "flags: {flags:?}");
    }
}

#[test]
fn prefer_string_starts_ends_with_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-prefer-string-starts-ends-with/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        "42",
        r#""always""#,
        r#"{"allowSingleElementEquality":true}"#,
        r#"{"allowSingleElementEquality":"sometimes"}"#,
        r#"{"allowSingleElementEquality":[]}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("prefer-string-starts-ends-with".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((
                        file.clone(),
                        Value::String("declare const s: string; s[0] === 'x';".into()),
                    ))
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "options: {options}; {frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0].0, 0);
        let Value::Object(error) = &frames[0].1 else { panic!() };
        let Value::String(message) = &error["error"] else { panic!() };
        assert!(message.contains("prefer-string-starts-ends-with"), "{message}");
    }
}

#[test]
fn promise_function_async_preserves_utf8_options_inheritance_and_independent_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/promise_function_async/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-promise-function-async");
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut diagnostics = Vec::new();
    let mut files = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let Value::Object(case) = case else { panic!() };
        let filename = format!("{index}.ts");
        let file = base.join(&filename).to_string_lossy().into_owned();
        files.push(Value::String(filename));
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("promise-function-async".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.push((file.clone(), case["source"].clone()));
        let Value::Array(expected) = &case["diagnostics"] else { panic!() };
        for diagnostic in expected {
            let Value::Object(mut diagnostic) = diagnostic.clone() else { panic!() };
            diagnostic.insert("file_path".into(), Value::String(file.clone()));
            diagnostics.push(diagnostic);
        }
    }
    overrides.push((
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            json::marshal(&protocol::object([
                (
                    "compilerOptions",
                    protocol::object([
                        ("target", Value::String("esnext".into())),
                        ("strict", Value::Bool(true)),
                        ("experimentalDecorators", Value::Bool(true)),
                        ("types", Value::Array(Vec::new())),
                    ]),
                ),
                ("files", Value::Array(files)),
            ]))
            .unwrap(),
        ),
    ));
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let expected: Vec<_> = diagnostics
            .iter()
            .cloned()
            .map(|mut diagnostic| {
                if !flags.contains(&"-fix") {
                    diagnostic.shift_remove("fixes");
                }
                if !flags.contains(&"-fix-suggestions") {
                    diagnostic.shift_remove("suggestions");
                }
                (1, Value::Object(diagnostic))
            })
            .collect();
        assert_eq!(frames.len(), expected.len(), "{flags:?}: {frames:?}");
        for diagnostic in &expected {
            assert!(frames.contains(diagnostic), "{flags:?}: missing {diagnostic:?}: {frames:?}");
        }
    }
}

#[test]
fn promise_function_async_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-promise-function-async/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        r#"{"allowAny":"true"}"#,
        r#"{"checkArrowFunctions":1}"#,
        r#"{"checkFunctionDeclarations":[]}"#,
        r#"{"checkFunctionExpressions":{}}"#,
        r#"{"checkMethodDeclarations":"false"}"#,
        r#"{"allowedPromiseNames":false}"#,
        r#"{"allowedPromiseNames":[1]}"#,
        r#"{"allowedPromiseNames":[{}]}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("promise-function-async".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("function café() {}".into())))
                        .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, 0, "{options}: {frames:?}");
    }
}

#[test]
fn require_await_preserves_utf8_scopes_types_options_and_suggestion_flags() {
    let base = std::env::current_dir().unwrap().join("target/lint-require-await");
    let first = base.join("first.ts").to_string_lossy().into_owned();
    let second = base.join("second.ts").to_string_lossy().into_owned();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(first.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("require-await".into())),
                            ("options", Value::Array(vec![Value::Integer(42)])),
                        ])]),
                    ),
                ]),
                protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(second.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("require-await".into())),
                            ("options", Value::String("invalid".into())),
                        ])]),
                    ),
                ]),
            ]),
        ),
        (
            "source_overrides",
            Value::Object(
                [
                    (
                        first,
                        Value::String(include_str!("fixtures/require_await/first.ts").into()),
                    ),
                    (
                        second,
                        Value::String(include_str!("fixtures/require_await/second.ts").into()),
                    ),
                    (
                        base.join("tsconfig.json").to_string_lossy().into_owned(),
                        Value::String(
                            r#"{"compilerOptions":{"target":"esnext","strict":true},"files":["first.ts","second.ts"]}"#.into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        ),
    ]);
    let Value::Array(expected) =
        json::unmarshal(include_str!("fixtures/require_await/diagnostics.json")).unwrap()
    else {
        panic!()
    };
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        let mut actual: Vec<_> = frames
            .into_iter()
            .map(|(kind, value)| {
                assert_eq!(kind, 1);
                value
            })
            .collect();
        let mut expected = expected.clone();
        for diagnostic in &mut expected {
            let Value::Object(fields) = diagnostic else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            fields.insert(
                "file_path".into(),
                Value::String(base.join(file).to_string_lossy().into_owned()),
            );
            if !flags.contains(&"-fix-suggestions") {
                fields.shift_remove("suggestions");
            }
        }
        let key = |value: &Value| {
            let Value::Object(fields) = value else { panic!() };
            let Value::String(file) = &fields["file_path"] else { panic!() };
            let Value::Object(range) = &fields["range"] else { panic!() };
            let Value::Number(pos) = range["pos"] else { panic!() };
            (file.clone(), pos as u32)
        };
        actual.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(actual, expected, "flags: {flags:?}");
    }
}

#[test]
fn restrict_plus_operands_preserves_utf8_labels_options_constraints_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/restrict_plus_operands/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base =
        std::env::current_dir().unwrap().join("target/lint-restrict-plus-operands-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("restrict-plus-operands".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["expected"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn restrict_plus_operands_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-restrict-plus-operands/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        "42",
        r#""invalid""#,
        r#"{"allowAny":"true"}"#,
        r#"{"allowBoolean":1}"#,
        r#"{"allowNullish":[]}"#,
        r#"{"allowNumberAndString":{}}"#,
        r#"{"allowRegExp":"false"}"#,
        r#"{"skipCompoundAssignments":0}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("restrict-plus-operands".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("export {}; 1 + 1;".into())))
                        .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
        let Value::Object(error) = &frames[0].1 else { panic!() };
        let Value::String(error) = &error["error"] else { panic!() };
        assert!(error.starts_with("error running linter: restrict-plus-operands:"), "{error}");
    }
}

#[test]
fn return_await_preserves_utf8_options_scopes_precedence_and_independent_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/return_await/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-return-await");
    let mut configs = Vec::new();
    let mut overrides = Vec::new();
    let mut diagnostics = Vec::new();
    let mut files = Vec::new();
    for (index, case) in cases.into_iter().enumerate() {
        let Value::Object(case) = case else { panic!() };
        let filename = format!("{index}.ts");
        let file = base.join(&filename).to_string_lossy().into_owned();
        files.push(Value::String(filename));
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("return-await".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.push((file.clone(), case["source"].clone()));
        let Value::Array(expected) = &case["diagnostics"] else { panic!() };
        for diagnostic in expected {
            let Value::Object(mut diagnostic) = diagnostic.clone() else { panic!() };
            diagnostic.insert("file_path".into(), Value::String(file.clone()));
            diagnostics.push(diagnostic);
        }
    }
    overrides.push((
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            json::marshal(&protocol::object([
                (
                    "compilerOptions",
                    protocol::object([
                        ("target", Value::String("esnext".into())),
                        ("strict", Value::Bool(true)),
                        ("experimentalDecorators", Value::Bool(true)),
                        ("types", Value::Array(Vec::new())),
                    ]),
                ),
                ("files", Value::Array(files)),
            ]))
            .unwrap(),
        ),
    ));
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides.into_iter().collect())),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let expected: Vec<_> = diagnostics
            .iter()
            .cloned()
            .map(|mut diagnostic| {
                if !flags.contains(&"-fix") {
                    diagnostic.shift_remove("fixes");
                }
                if !flags.contains(&"-fix-suggestions") {
                    diagnostic.shift_remove("suggestions");
                }
                (1, Value::Object(diagnostic))
            })
            .collect();
        assert_eq!(frames.len(), expected.len(), "{flags:?}: {frames:?}");
        for diagnostic in &expected {
            assert!(frames.contains(diagnostic), "{flags:?}: missing {diagnostic:?}: {frames:?}");
        }
    }
}

#[test]
fn return_await_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-return-await/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "false",
        "42",
        "[]",
        "{}",
        r#"["always"]"#,
        r#"{"option":"always"}"#,
        r#""""#,
        r#""ALWAYS""#,
        r#""unknown""#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("return-await".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((file.clone(), Value::String("function café() {}".into())))
                        .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, 0, "{options}: {frames:?}");
    }
}

#[test]
fn strict_void_return_preserves_utf8_contexts_options_scopes_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/strict_void_return/regressions.json")).unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-strict-void-return-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.tsx")).to_string_lossy().into_owned();
        configs.push(protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("strict-void-return".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]));
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["diagnostics"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext","jsx":"preserve"},"include":["*.tsx"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn strict_void_return_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-strict-void-return-invalid/file.ts")
        .to_string_lossy()
        .into_owned();
    for (options, error) in [
        ("[]", "strict-void-return: options must be an object"),
        ("true", "strict-void-return: options must be an object"),
        ("42", "strict-void-return: options must be an object"),
        (r#""invalid""#, "strict-void-return: options must be an object"),
        (r#"{"allowReturnAny":1}"#, "strict-void-return: allowReturnAny must be a boolean"),
        (r#"{"allowReturnAny":"true"}"#, "strict-void-return: allowReturnAny must be a boolean"),
        (r#"{"allowReturnAny":[]}"#, "strict-void-return: allowReturnAny must be a boolean"),
        (r#"{"allowReturnAny":{}}"#, "strict-void-return: allowReturnAny must be a boolean"),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("strict-void-return".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((
                        file.clone(),
                        Value::String("export {}; const café: () => void = () => 1;".into()),
                    ))
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, 0);
        let Value::Object(fields) = &frames[0].1 else { panic!() };
        let Value::String(actual) = &fields["error"] else { panic!() };
        assert!(actual.contains(error), "{options}: {actual}");
    }
}

#[test]
fn switch_exhaustiveness_check_preserves_utf8_constraints_options_and_flags() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/switch_exhaustiveness_check/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir()
        .unwrap()
        .join("target/lint-switch-exhaustiveness-check-regressions");
    let mut configs = Vec::new();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    let mut expected_by_file = Vec::new();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let Value::String(name) = &case["name"] else { panic!() };
        let file = base.join(format!("{name}.ts")).to_string_lossy().into_owned();
        let config = protocol::object([
            ("file_paths", Value::Array(vec![Value::String(file.clone())])),
            (
                "rules",
                Value::Array(vec![protocol::object([
                    ("name", Value::String("switch-exhaustiveness-check".into())),
                    ("options", case["options"].clone()),
                ])]),
            ),
        ]);
        if let Some(Value::String(error)) = case.get("error") {
            let payload = protocol::object([
                ("version", Value::Integer(2)),
                ("configs", Value::Array(vec![config])),
                (
                    "source_overrides",
                    Value::Object(std::iter::once((file, case["source"].clone())).collect()),
                ),
            ]);
            for flags in
                [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
            {
                let (status, frames) = run(&payload, &flags);
                assert_eq!(status, 1, "{flags:?}: {frames:?}");
                assert_eq!(frames.len(), 1, "{flags:?}: {frames:?}");
                assert_eq!(frames[0].0, 0);
                let Value::Object(fields) = &frames[0].1 else { panic!() };
                let Value::String(actual) = &fields["error"] else { panic!() };
                assert!(actual.contains(error), "{flags:?}: {actual}");
            }
            continue;
        }
        configs.push(config);
        overrides.insert(file.clone(), case["source"].clone());
        expected_by_file.push((file, case["expected"].clone()));
    }
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(
            r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"include":["*.ts"]}"#.into(),
        ),
    );
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("configs", Value::Array(configs)),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let mut count = 0;
        for (file, expected) in &expected_by_file {
            let Value::Array(expected) = expected else { panic!() };
            count += expected.len();
            let actual: Vec<_> = frames
                .iter()
                .filter_map(|(kind, value)| {
                    assert_eq!(*kind, protocol::MessageType::Diagnostic as u8);
                    let Value::Object(diagnostic) = value else { panic!() };
                    (diagnostic["file_path"] == Value::String(file.clone()))
                        .then_some(value.clone())
                })
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value.clone() else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    if !flags.contains(&"-fix-suggestions") {
                        diagnostic.shift_remove("suggestions");
                    }
                    Value::Object(diagnostic)
                })
                .collect();
            assert_eq!(actual, expected, "{flags:?}: {file}");
        }
        assert_eq!(frames.len(), count, "{flags:?}: {frames:?}");
    }
}

#[test]
fn switch_exhaustiveness_check_rejects_invalid_options() {
    let file = std::env::current_dir()
        .unwrap()
        .join("target/lint-switch-exhaustiveness-check/invalid.ts")
        .to_string_lossy()
        .into_owned();
    for options in [
        "true",
        "[]",
        "42",
        r#""invalid""#,
        r#"{"allowDefaultCaseForExhaustiveSwitch":"true"}"#,
        r#"{"considerDefaultExhaustiveForUnions":1}"#,
        r#"{"requireDefaultForNonUnion":[]}"#,
        r#"{"defaultCaseCommentPattern":false}"#,
        r#"{"defaultCaseCommentPattern":{}}"#,
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    (
                        "rules",
                        Value::Array(vec![protocol::object([
                            ("name", Value::String("switch-exhaustiveness-check".into())),
                            ("options", json::unmarshal(options).unwrap()),
                        ])]),
                    ),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    std::iter::once((
                        file.clone(),
                        Value::String("export {}; switch (true) {}".into()),
                    ))
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &["-fix", "-fix-suggestions"]);
        assert_eq!(status, 1, "{options}: {frames:?}");
        assert_eq!(frames.len(), 1, "{options}: {frames:?}");
        assert_eq!(frames[0].0, protocol::MessageType::Error as u8);
        let Value::Object(error) = &frames[0].1 else { panic!() };
        let Value::String(error) = &error["error"] else { panic!() };
        assert!(error.starts_with("error running linter: switch-exhaustiveness-check:"), "{error}");
    }
}

#[test]
fn all_pinned_rules_run_together_with_independent_fix_modes() {
    let base = std::env::current_dir().unwrap().join("target/lint-all-rules-integration");
    std::fs::create_dir_all(&base).unwrap();
    let file = base.join("a.ts").to_string_lossy().into_owned();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    overrides.insert(file.clone(), Value::String(include_str!("fixtures/all_rules/a.ts").into()));
    overrides.insert(
        base.join("tsconfig.json").to_string_lossy().into_owned(),
        Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","module":"commonjs","lib":["esnext","dom"],"types":[]},"include":["*.ts"]}"#.into()),
    );
    let rules = tsrs_lint::registry::BASELINE_RULES
        .iter()
        .map(|&name| protocol::object([("name", Value::String(name.into()))]))
        .collect();
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                ("rules", Value::Array(rules)),
            ])]),
        ),
        ("source_overrides", Value::Object(overrides)),
    ]);
    let Value::Array(expected) =
        json::unmarshal(include_str!("fixtures/all_rules/diagnostics.json")).unwrap()
    else {
        panic!()
    };
    assert_eq!(expected.len(), 4);
    for (flags, expected) in
        [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
            .into_iter()
            .zip(expected)
    {
        let (status, mut frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{flags:?}: {frames:?}");
        let Value::Array(expected) = expected else { panic!() };
        let mut expected: Vec<_> = expected
            .into_iter()
            .map(|value| {
                let Value::Object(mut diagnostic) = value else { panic!() };
                diagnostic.insert("file_path".into(), Value::String(file.clone()));
                (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
            })
            .collect();
        let order = |(_, value): &(u8, Value)| {
            let Value::Object(diagnostic) = value else { panic!() };
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            (
                json::marshal(&diagnostic["rule"]).unwrap(),
                json::marshal(&diagnostic["range"]).unwrap(),
                json::marshal(&message["id"]).unwrap(),
            )
        };
        frames.sort_by_key(&order);
        expected.sort_by_key(order);
        assert_eq!(frames, expected, "{flags:?}");
    }
}

#[test]
fn removed_options_link_to_upstream_guidance_only_for_program_diagnostics() {
    let base = std::env::current_dir().unwrap().join("target/lint-removed-options");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    for (options, enhanced) in [
        (r#"{"baseUrl":"."}"#, true),
        (r#"{"outFile":"out.js"}"#, true),
        (r#"{"Please remove it from your configuration.":true}"#, false),
    ] {
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            (
                "configs",
                Value::Array(vec![protocol::object([
                    ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                    ("rules", Value::Array(Vec::new())),
                ])]),
            ),
            (
                "source_overrides",
                Value::Object(
                    [
                        (file.clone(), Value::String("export {};".into())),
                        (
                            base.join("tsconfig.json").to_string_lossy().into_owned(),
                            Value::String(format!(
                                r#"{{"compilerOptions":{options},"files":["a.ts"]}}"#
                            )),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ),
        ]);
        let (status, frames) = run(&payload, &[]);
        assert_eq!(status, 0, "{frames:?}");
        assert!(!frames.is_empty(), "{options}");
        for (kind, diagnostic) in frames {
            assert_eq!(kind, 1);
            let Value::Object(diagnostic) = diagnostic else { panic!() };
            let Value::Object(message) = &diagnostic["message"] else { panic!() };
            assert_eq!(message["id"], Value::String("tsconfig-error".into()));
            let Value::String(help) = &message["help"] else { panic!() };
            assert_eq!(
                help.contains("https://github.com/oxc-project/tsgolint/issues/351"),
                enhanced,
                "{help}"
            );
        }
    }
}

#[test]
fn type_diagnostic_rendering_matches_go_utf8_truncation() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/diagnostic_rendering_utf8/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-type-rendering-utf8");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            ("configs", Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                ("rules", Value::Array(vec![protocol::object([("name", case["rule"].clone())])])),
            ])])),
            ("source_overrides", Value::Object([
                (file.clone(), case["source"].clone()),
                (base.join("tsconfig.json").to_string_lossy().into_owned(), Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"files":["a.ts"]}"#.into())),
            ].into_iter().collect())),
        ]);
        let Value::Array(expected) = &case["expected"] else { panic!() };
        for (flags, expected) in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
                .into_iter()
                .zip(expected)
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{:?} {flags:?}: {frames:?}", case["name"]);
            let Value::Array(expected) = expected else { panic!() };
            let expected: Vec<_> = expected
                .iter()
                .cloned()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
                })
                .collect();
            assert_eq!(frames, expected, "{:?} {flags:?}", case["name"]);
        }
    }
}

#[test]
fn type_parameter_reference_traversal_matches_go_terminal_reference_branch() {
    let Value::Array(cases) =
        json::unmarshal(include_str!("fixtures/type_parameter_references/regressions.json"))
            .unwrap()
    else {
        panic!()
    };
    let base = std::env::current_dir().unwrap().join("target/lint-type-parameter-references");
    let file = base.join("a.ts").to_string_lossy().into_owned();
    for case in cases {
        let Value::Object(case) = case else { panic!() };
        let payload = protocol::object([
            ("version", Value::Integer(2)),
            ("configs", Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                ("rules", Value::Array(vec![protocol::object([("name", Value::String("no-unnecessary-type-parameters".into()))])])),
            ])])),
            ("source_overrides", Value::Object([
                (file.clone(), case["source"].clone()),
                (base.join("tsconfig.json").to_string_lossy().into_owned(), Value::String(r#"{"compilerOptions":{"strict":true,"target":"esnext","types":[]},"files":["a.ts"]}"#.into())),
            ].into_iter().collect())),
        ]);
        let Value::Array(expected) = &case["expected"] else { panic!() };
        for (flags, expected) in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
                .into_iter()
                .zip(expected)
        {
            let (status, frames) = run(&payload, &flags);
            assert_eq!(status, 0, "{:?} {flags:?}: {frames:?}", case["name"]);
            let Value::Array(expected) = expected else { panic!() };
            let expected: Vec<_> = expected
                .iter()
                .cloned()
                .map(|value| {
                    let Value::Object(mut diagnostic) = value else { panic!() };
                    diagnostic.insert("file_path".into(), Value::String(file.clone()));
                    (protocol::MessageType::Diagnostic as u8, Value::Object(diagnostic))
                })
                .collect();
            assert_eq!(frames, expected, "{:?} {flags:?}", case["name"]);
        }
    }
}

#[test]
fn parallel_rules_and_type_checking_match_serial_for_independent_modules() {
    let base = std::env::current_dir().unwrap().join("target/lint-parallel-integration");
    let files = (0..24)
        .map(|index| base.join(format!("{index:02}.ts")).to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    for file in &files {
        overrides
            .insert(file.clone(), Value::String(include_str!("fixtures/all_rules/a.ts").into()));
    }
    let rules: Vec<_> = tsrs_lint::registry::BASELINE_RULES
        .iter()
        .copied()
        .map(|name| protocol::object([("name", Value::String(name.into()))]))
        .collect();
    for semantic in [false, true] {
        let input_for = |single_threaded| {
            let mut overrides = overrides.clone();
            overrides.insert(base.join("tsconfig.json").to_string_lossy().into_owned(), Value::String(
                format!(r#"{{"compilerOptions":{{"singleThreaded":{single_threaded},"strict":true,"target":"esnext","module":"commonjs","lib":["esnext","dom"],"types":[]}},"include":["*.ts"]}}"#)
            ));
            let payload = protocol::object([
                ("version", Value::Integer(2)),
                ("report_semantic", Value::Bool(semantic)),
                ("report_syntactic", Value::Bool(semantic)),
                (
                    "configs",
                    Value::Array(vec![protocol::object([
                        (
                            "file_paths",
                            Value::Array(files.iter().cloned().map(Value::String).collect()),
                        ),
                        ("rules", Value::Array(rules.clone())),
                    ])]),
                ),
                ("source_overrides", Value::Object(overrides.clone())),
            ]);
            json::marshal(&payload).unwrap()
        };
        let serial_input = input_for(true);
        let input = input_for(false);
        for flags in
            [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
        {
            let (serial_status, serial) = run_raw_with_threads(&serial_input, &flags, Some("1"));
            assert_eq!(serial_status, 0, "{serial:?}");
            assert_ne!(serial, [] as [(u8, tsrs_core::json::Value); 0]);
            for threads in ["1", "2", "4", "8", "12"] {
                let (status, parallel) = run_raw_with_threads(&input, &flags, Some(threads));
                assert_eq!(status, 0, "{parallel:?}");
                assert_eq!(
                    parallel, serial,
                    "semantic={semantic}, threads={threads}, flags={flags:?}"
                );
            }
        }
    }
}

#[test]
fn source_overrides_resolve_virtual_package_directories_and_glob_roots() {
    let base = std::env::current_dir().unwrap().join("target/lint-virtual-package-resolution");
    let file = base.join("src/a/b/c/d/file.ts").to_string_lossy().into_owned();
    let source = "import type { Fn } from 'virtual-return-types'; export const foo: Fn = () => new Set<any>();";
    let mut overrides = tsrs_core::collections::OrderedMap::default();
    for (path, text) in [
        ("src/a/b/c/d/file.ts", source),
        (
            "node_modules/return-types/package.json",
            r#"{"name":"return-types","types":"index.d.ts"}"#,
        ),
        ("node_modules/return-types/index.d.ts", "export type Fn = () => Set<string>;"),
        (
            "tsconfig.json",
            r#"{"compilerOptions":{"strict":true,"target":"ES2020","types":[],"paths":{"virtual-return-types":["./node_modules/return-types/index.d.ts"]}},"include":["src/**/*.ts"]}"#,
        ),
    ] {
        overrides
            .insert(base.join(path).to_string_lossy().into_owned(), Value::String(text.into()));
    }
    for index in 0..7 {
        overrides.insert(
            base.join(format!("src/a/b/c/d/control-{index}.ts")).to_string_lossy().into_owned(),
            Value::String("export {};".into()),
        );
    }
    let payload = protocol::object([
        ("version", Value::Integer(2)),
        ("report_semantic", Value::Bool(true)),
        (
            "configs",
            Value::Array(vec![protocol::object([
                ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                (
                    "rules",
                    Value::Array(vec![protocol::object([(
                        "name",
                        Value::String("no-unsafe-return".into()),
                    )])]),
                ),
            ])]),
        ),
        ("source_overrides", Value::Object(overrides)),
    ]);
    for flags in [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
    {
        let (status, frames) = run(&payload, &flags);
        assert_eq!(status, 0, "{frames:?}");
        assert_eq!(frames.len(), 1, "{frames:?}");
        let Value::Object(diagnostic) = &frames[0].1 else { panic!() };
        assert_eq!(diagnostic["file_path"], Value::String(file.clone()));
        assert_eq!(diagnostic["rule"], Value::String("no-unsafe-return".into()));
        let Value::Object(message) = &diagnostic["message"] else { panic!() };
        assert_eq!(message["id"], Value::String("unsafeReturnAssignment".into()));
    }
}

#[test]
fn unbound_method_accepts_untyped_this_without_panicking() {
    let base = std::env::current_dir().unwrap().join("target/lint-untyped-this");
    let file = base.join("file.ts").to_string_lossy().into_owned();
    for strict in [false, true] {
        for parameter in ["this", "this: any"] {
            let source = format!(
                "export {{}}; class C {{ property = function({parameter}) {{}}; method({parameter}) {{}} }} const c = new C(); const a = c.property; const b = c.method;"
            );
            let overrides = [
                (file.clone(), Value::String(source)),
                (base.join("tsconfig.json").to_string_lossy().into_owned(), Value::String(format!(r#"{{"compilerOptions":{{"strict":{strict},"noLib":true,"types":[]}},"files":["file.ts"]}}"#))),
            ].into_iter().collect();
            let payload = protocol::object([
                ("version", Value::Integer(2)),
                (
                    "configs",
                    Value::Array(vec![protocol::object([
                        ("file_paths", Value::Array(vec![Value::String(file.clone())])),
                        (
                            "rules",
                            Value::Array(vec![protocol::object([(
                                "name",
                                Value::String("unbound-method".into()),
                            )])]),
                        ),
                    ])]),
                ),
                ("source_overrides", Value::Object(overrides)),
            ]);
            for flags in
                [vec![], vec!["-fix"], vec!["-fix-suggestions"], vec!["-fix", "-fix-suggestions"]]
            {
                let (status, frames) = run(&payload, &flags);
                assert_eq!(status, 0, "strict={strict}, parameter={parameter}: {frames:?}");
                assert_eq!(frames.len(), 2, "{frames:?}");
                for (kind, value) in frames {
                    assert_eq!(kind, 1);
                    let Value::Object(diagnostic) = value else { panic!() };
                    assert_eq!(diagnostic["rule"], Value::String("unbound-method".into()));
                    let Value::Object(message) = &diagnostic["message"] else { panic!() };
                    assert_eq!(message["id"], Value::String("unbound".into()));
                }
            }
        }
    }
}
