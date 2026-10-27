use super::*;

fn compare_apis(
    inputs: &Files,
    options: &BuildOptions,
    case: &Case,
    expected: &tsrs_dts::BuildResult,
    link: &tsrs_dts::LinkOptions,
) {
    use tsrs_dts::{DeclarationContext, DeclarationInputs};
    let mut inputs = inputs.clone();
    let mut config = tsrs_core::json::unmarshal(&inputs[&options.config_path]).unwrap();
    let tsrs_core::json::Value::Object(ref mut object) = config else { panic!() };
    let Some(tsrs_core::json::Value::Object(compiler)) = object.get_mut("compilerOptions") else {
        panic!()
    };
    for (key, value) in [
        ("declaration", true),
        ("emitDeclarationOnly", true),
        ("noEmit", false),
        ("noCheck", false),
        ("noEmitOnError", true),
        ("incremental", false),
        ("composite", false),
    ] {
        compiler.insert(key, tsrs_core::json::Value::Bool(value));
    }
    for key in ["outDir", "declarationDir", "outFile", "tsBuildInfoFile"] {
        compiler.shift_remove(key);
    }
    inputs.insert(options.config_path.clone(), tsrs_core::json::marshal(&config).unwrap());
    let (fs, writes) = memory_fs(&inputs);
    let host = crate::program::HostProgram::new(Arc::clone(&fs), &options.config_path);
    let context = tsrs_compiler::Context::background();
    let borrowed =
        tsrs_dts::build_from_program(host.program, &context, link, Arc::clone(&fs)).unwrap();
    assert_eq!(expected.files, borrowed.files, "{} borrowed Program", case.key);
    let capture = tsrs_dts::emit_declarations(host.program, &context).unwrap();
    let settings = DeclarationContext::from_program(host.program);
    // The capture and settings must remain usable after the source Program dies.
    drop(host);
    let linked = tsrs_dts::link_declarations(
        DeclarationInputs { captured: capture, context: settings, fs },
        link,
    )
    .unwrap();
    assert_eq!(expected.files, linked.files, "{} captured declarations", case.key);
    assert_eq!(expected.retained_statements, linked.retained_statements);
    assert_eq!(expected.removed_statements, linked.removed_statements);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
}

fn diagnostics_match(case: &Case, messages: &[String]) {
    let codes: BTreeSet<_> =
        messages.iter().filter_map(|message| message.split(':').next()).collect();
    assert_eq!(
        codes,
        case.expected.iter().copied().collect(),
        "{} diagnostics changed: {messages:?}",
        case.key
    );
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "TypeScript recognizes module extensions with case-sensitive spellings"
)]
fn output_name(entry: &str, index: Option<usize>) -> String {
    let extension = if entry.ends_with(".mts") {
        ".d.mts"
    } else if entry.ends_with(".cts") {
        ".d.cts"
    } else {
        ".d.ts"
    };
    format!("{}{extension}", index.map_or_else(|| "index".into(), |index| format!("entry{index}")))
}

pub(super) fn run(case: &Case, directory: &Path, consumer: PathBuf) {
    let mut inputs = read_inputs(directory);
    if case.key == "unexported_variable_treeshake" {
        // Adapt only the exact invalid upstream declaration; preserve imported bytes.

        // TODO: Remove this adapter when valid upstream declarations are available.
        // The assertion below guards the original input; see docs/DTS.md.
        let text = inputs.get_mut("/project/src/index.d.ts").unwrap();
        assert_eq!(text.trim(), "const a = 0;\n\nexport {}");
        text.insert_str(0, "declare ");
    }
    let mut environment = environment(case);
    let mut consumer_environment = Files::new();
    if case.nodenext {
        for (prefix, target) in [
            ("/project/src/environment/", &mut environment),
            ("/project/src/consumer-environment/", &mut consumer_environment),
        ] {
            let paths: Vec<_> =
                inputs.keys().filter(|path| path.starts_with(prefix)).cloned().collect();
            for path in paths {
                let text = inputs.remove(&path).unwrap();
                target.insert(format!("/project/{}", path.strip_prefix(prefix).unwrap()), text);
            }
        }
    }
    let contract = fs::read_to_string(consumer).ok();
    if !matches!(case.classification, "missing-environment" | "invalid-input") {
        assert!(
            contract.is_some()
                || !case.required_output.is_empty()
                || !case.forbidden_output.is_empty()
                || !case.occurrences.is_empty(),
            "{} has no output or consumer assertions",
            case.key
        );
    }
    let maps = case.key == "sourcemap_composition";
    // The paths mapping is an explicit adapter: no upstream tsconfig is shipped
    // with this directory. All resolution is still performed by tsrs.

    // TODO: Use the original tsconfig when available, then remove this injected
    // mapping. See docs/DTS.md, "Fixture adaptations and coverage limits".
    let paths = if case.key.ends_with("/path-mapping") {
        r#", "paths":{"components/*":["/project/src/foo/bar/*"]}"#
    } else {
        ""
    };
    let mut compiler_config = format!(
        r#"{{"compilerOptions":{{"strict":true,"target":"es2022","module":"preserve","moduleResolution":"bundler","types":[],"allowJs":true,"jsx":"preserve"{paths}}},"files":[{}]}}"#,
        case.entries
            .iter()
            .map(|entry| format!("\"/project/src/{entry}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    if case.nodenext {
        compiler_config = compiler_config.replace(
            "\"module\":\"preserve\",\"moduleResolution\":\"bundler\"",
            "\"module\":\"nodenext\",\"moduleResolution\":\"nodenext\"",
        );
    }
    if maps {
        compiler_config = compiler_config.replace(
            "\"strict\":true",
            "\"strict\":true,\"declaration\":true,\"declarationMap\":true",
        );
    }
    if case.nodenext
        && let Some(config) = inputs.remove("/project/src/tsconfig.json")
    {
        compiler_config = config;
    }
    compiler_config = dependencies::configure(case.key, &compiler_config);
    let entries: Vec<_> = case
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let output = case.output.map_or_else(
                || output_name(entry, (case.entries.len() > 1).then_some(index)),
                str::to_owned,
            );
            (format!("/project/src/{entry}"), output)
        })
        .collect();
    let bind_entries = |contract: &str, generated: bool| {
        let mut contract = contract.to_owned();
        for (index, (source, output)) in entries.iter().enumerate() {
            let relative = if generated {
                format!("out/{output}")
            } else {
                source.strip_prefix("/project/").unwrap().to_owned()
            };
            contract =
                contract.replace(&format!("__ENTRY{index}__"), &runtime_specifier(&relative));
        }
        contract
    };
    let original_contract = contract.as_deref().map(|contract| bind_entries(contract, false));
    let generated_contract = contract.as_deref().map(|contract| bind_entries(contract, true));
    let mut originals = Vec::new();
    let mut input_diagnostics = Vec::new();
    for (source, _) in &entries {
        match inspect_with_config(
            inputs.clone(),
            &environment,
            source,
            if case.generated_consumer_only { None } else { original_contract.as_deref() },
            Some(&compiler_config),
        ) {
            Ok(facts) => originals.push(facts),
            Err(messages) => input_diagnostics.extend(messages),
        }
    }
    if !input_diagnostics.is_empty() {
        assert!(
            matches!(case.classification, "missing-environment" | "invalid-input"),
            "{} original: {input_diagnostics:?}",
            case.key
        );
        diagnostics_match(case, &input_diagnostics);
        return;
    }
    assert!(
        !matches!(case.classification, "missing-environment" | "invalid-input"),
        "{} original now passes; reclassify it",
        case.key
    );
    if case.nodenext {
        inputs.append(&mut environment);
    } else {
        inputs.extend(environment.clone());
    }
    let first = entries[0].0.clone();
    inputs.insert("/project/tsconfig.json".into(), compiler_config);
    let (fs, writes) = memory_fs(&inputs);
    let mut options = BuildOptions {
        config_path: "/project/tsconfig.json".into(),
        entry: first,
        output: OutputMode::Bundle {
            file_name: case
                .output
                .map_or_else(|| output_name(case.entries[0], None), str::to_owned),
        },
        default_library_path: tsrs_vfs::bundled::lib_path(),
        run_external_code: false,
        reuse_diagnostics: false,
    };
    let link = tsrs_dts::LinkOptions {
        entries: entries.iter().map(|(source, output)| (output.clone(), source.clone())).collect(),
    };
    let run_build = |options: &BuildOptions| {
        if case.entries.len() == 1 && !case.shared {
            build(options, Arc::clone(&fs))
        } else {
            tsrs_dts::build_entries(options, &link.entries, Arc::clone(&fs))
        }
    };
    let result = run_build(&options).unwrap_or_else(|error| panic!("{} build: {error}", case.key));
    if std::env::var_os("TSRS_DTS_COMPARE_APIS").is_some() {
        // Borrowed/captured single-entry APIs produce direct bundles, without facades.
        let direct;
        let expected = if case.shared && case.entries.len() == 1 {
            direct = build(&options, Arc::clone(&fs)).unwrap();
            &direct
        } else {
            &result
        };
        compare_apis(&inputs, &options, case, expected, &link);
    }
    if std::env::var_os("TSRS_DTS_COMPARE_REUSE").is_some() {
        options.reuse_diagnostics = true;
        let actual =
            run_build(&options).unwrap_or_else(|error| panic!("{} reuse: {error}", case.key));
        assert_eq!(result.files, actual.files, "{} diagnostic reuse output", case.key);
        assert_eq!(result.retained_statements, actual.retained_statements);
        assert_eq!(result.removed_statements, actual.removed_statements);
        assert_eq!(result.emitted_declarations, actual.emitted_declarations);
        options.reuse_diagnostics = false;
    }
    if case.repeat {
        // Rebuild with a fresh compiler/graph against the same inputs.
        assert_eq!(case.entries.len(), 1);
        assert_eq!(
            result.files,
            build(&options, fs).unwrap().files,
            "{} is nondeterministic",
            case.key
        );
    }
    if let Some(count) = case.files {
        assert_eq!(result.files.len(), count, "{} artifact count", case.key);
    }
    if !case.each_contains_any.is_empty() {
        assert!(
            result
                .files
                .values()
                .all(|text| case.each_contains_any.iter().any(|fragment| text.contains(fragment))),
            "{} per-artifact content changed: {:?}",
            case.key,
            result.files
        );
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    if case.removed {
        assert!(result.removed_statements > 0, "{} removed no statements", case.key);
    }
    if maps {
        check_maps(&result.files);
    }
    let generated: Files = result
        .files
        .into_iter()
        .map(|(name, text)| (format!("/project/out/{name}"), text))
        .collect();
    let text = generated.values().map(String::as_str).collect::<Vec<_>>().join("\n");
    for &(fragment, count) in case.occurrences {
        assert_eq!(
            text.matches(fragment).count(),
            count,
            "{} occurrence count for {fragment:?}:\n{text}",
            case.key
        );
    }
    for required in case.required_output {
        assert!(
            text.contains(required),
            "{} missing reviewed output {required:?}:\n{text}",
            case.key
        );
    }
    for forbidden in case.forbidden_output {
        assert!(
            !text.contains(forbidden),
            "{} retained forbidden output {forbidden:?}:\n{text}",
            case.key
        );
    }
    // Optional review artifact: capture in a fresh directory so stale companion
    // declarations cannot make an independent consumer check pass accidentally.
    if let Some(directory) = std::env::var_os("TSRS_DTS_REVIEW_OUTPUT")
        .filter(|_| case.key.starts_with("rolldown-plugin-dts/"))
    {
        let directory =
            PathBuf::from(directory).join(case.key.strip_prefix("rolldown-plugin-dts/").unwrap());
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::create_dir(&directory).expect("review output must be a fresh fixture directory");
        for (name, text) in &generated {
            let relative = name.strip_prefix("/project/out/").unwrap();
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }
    let generated_config = if case.nodenext {
        consumer_environment
            .entry("/project/package.json".into())
            .or_insert_with(|| r#"{"type":"module"}"#.into());
        Some(format!(
            r#"{{"compilerOptions":{{"strict":true,"target":"es2022","module":"nodenext","moduleResolution":"nodenext","types":[]}},"files":[{}]}}"#,
            entries
                .iter()
                .map(|(_, output)| tsrs_core::json::marshal_string(&format!(
                    "/project/out/{output}"
                )))
                .collect::<Vec<_>>()
                .join(",")
        ))
    } else {
        None
    };
    let mut output_diagnostics = Vec::new();
    for (index, (_, name)) in entries.iter().enumerate() {
        let fallback;
        let generated_config = if let Some(config) = generated_config.as_deref() {
            config
        } else {
            fallback = dependencies::configure(
                case.key,
                &config(&format!("/project/out/{name}"), contract.is_some()),
            );
            &fallback
        };
        match inspect_with_config(
            generated.clone(),
            if case.nodenext { &consumer_environment } else { &environment },
            &format!("/project/out/{name}"),
            generated_contract.as_deref(),
            Some(generated_config),
        ) {
            Ok(actual) => {
                assert_eq!(
                    actual.exports, originals[index].exports,
                    "{} public exports changed",
                    case.key
                );
                if let Some(count) = case.declarations {
                    assert_eq!(
                        actual.declarations, count,
                        "{} reviewed declaration count changed",
                        case.key
                    );
                }
            }
            Err(messages) => output_diagnostics.extend(messages),
        }
    }
    if output_diagnostics.is_empty() {
        assert_ne!(
            case.classification, "generated-invalid",
            "{} now passes; remove expected failure",
            case.key
        );
    } else {
        assert_eq!(
            case.classification, "generated-invalid",
            "{} generated: {output_diagnostics:?}",
            case.key
        );
        diagnostics_match(case, &output_diagnostics);
    }
}
