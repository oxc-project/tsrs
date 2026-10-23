// Port of tsc/internal/lsp/server_contentmapper_internal_test.go. TestContentMapperLoggerRequiresTrace is not
// ported: the server has no content mapper logger yet.

use tsrs_core::collections::OrderedMap;
use tsrs_core::tspath;
use tsrs_lsproto as lsproto;

use crate::lsptestutil::{acknowledge_registrations, new_lsp_client, test_server_options};
use crate::server::parse_content_mapper_contributions;

#[test]
fn set_content_mapper_contributions_accepts_empty_startup_handshake() {
    let (client, close_client) = new_lsp_client(
        test_server_options("/home/projects", &[]),
        Some(acknowledge_registrations()),
    );
    let (initialize, _) =
        client.send_request(lsproto::INITIALIZE_INFO, lsproto::InitializeParams::default());
    assert!(initialize.error.is_none(), "{:?}", initialize.error);
    client.send_notification(lsproto::INITIALIZED_INFO, lsproto::InitializedParams::default());
    client.server.init_complete().wait();
    let (response, _) = client.send_request(
        lsproto::CUSTOM_SET_CONTENT_MAPPER_CONTRIBUTIONS_INFO,
        lsproto::SetContentMapperContributionsParams::default(),
    );
    assert!(response.error.is_none(), "{:?}", response.error);
    let _ = close_client.close();
}

fn inferred_project_contribution(name: &str) -> lsproto::InferredProjectContentMapperContribution {
    lsproto::InferredProjectContentMapperContribution {
        options: None,
        manifest: lsproto::ContentMapperManifest {
            name: name.to_string(),
            exec: vec![name.to_string()],
            ..Default::default()
        },
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "The contribution pointer-slice element is nullable; this helper constructs non-nil test cases."
)]
fn contribution(
    contributor_id: &str,
    extension: &str,
    inferred_project_contribution: Option<lsproto::InferredProjectContentMapperContribution>,
) -> Option<lsproto::ContentMapperContribution> {
    Some(lsproto::ContentMapperContribution {
        contributor_id: contributor_id.to_string(),
        extensions: vec![extension.to_string()],
        inferred_project_contribution,
    })
}

// server_contentmapper_internal_test.go:26
#[test]
fn test_parse_content_mapper_contributions() {
    let mut options = OrderedMap::default();
    options.insert("mode".to_string(), lsproto::Value::String("embedded".to_string()));
    let mut output_extensions = OrderedMap::default();
    output_extensions.insert(".vue".to_string(), ".js".to_string());
    let mut contributions = parse_content_mapper_contributions(&[
        contribution(
            "publisher.extension",
            ".vue",
            Some(lsproto::InferredProjectContentMapperContribution {
                options: Some(options),
                manifest: lsproto::ContentMapperManifest {
                    name: "Vue mapper".to_string(),
                    version: Some("2.3.4".to_string()),
                    exec: vec!["node".to_string(), "mapper.js".to_string()],
                    cwd: Some("/workspace/mapper".to_string()),
                    compiler_options: Some(vec!["strict".to_string()]),
                    output_extensions: Some(output_extensions),
                    ..Default::default()
                },
            }),
        ),
        contribution("publisher.extension", ".svelte", None),
    ])
    .unwrap();
    assert_eq!(contributions.mappers.len(), 1);
    assert_eq!(contributions.extensions, [".vue"]);
    let mapper = &mut contributions.mappers[0];
    assert_eq!(mapper.identity(), "publisher.extension[0] (Vue mapper@2.3.4)");
    assert_eq!(mapper.package_directory, "/workspace/mapper");
    let vue_to_js = std::iter::once((".vue".to_string(), ".js".to_string())).collect();
    assert_eq!(mapper.manifest.default_output_extensions, vue_to_js);
    assert_eq!(mapper.definition.output_extensions, vue_to_js);
    mapper.manifest.default_output_extensions.insert(".vue".to_string(), ".cjs".to_string());
    assert_eq!(mapper.definition.output_extensions[".vue"], ".js");
    assert_eq!(mapper.definition.options, r#"{"mode":"embedded"}"#);
}

// server_contentmapper_internal_test.go:69
#[test]
fn test_parse_content_mapper_contributions_rejects_conflicting_inline_mappers() {
    let err = parse_content_mapper_contributions(&[
        contribution("first", ".vue", Some(inferred_project_contribution("first"))),
        contribution("second", ".vue", Some(inferred_project_contribution("second"))),
    ])
    .unwrap_err();
    assert!(err.to_string().contains(r#"both claim extension ".vue""#), "{err}");
}

// server_contentmapper_internal_test.go:81
#[test]
fn test_parse_content_mapper_contributions_uses_case_insensitive_extensions() {
    let err = parse_content_mapper_contributions(&[
        contribution("first", ".vue", Some(inferred_project_contribution("first"))),
        contribution("second", ".VUE", Some(inferred_project_contribution("second"))),
    ])
    .unwrap_err();
    assert!(err.to_string().contains(r#"both claim extension ".VUE""#), "{err}");

    let err =
        parse_content_mapper_contributions(&[contribution("built-in", ".TS", None)]).unwrap_err();
    assert!(err.to_string().contains(r#"invalid extension ".TS""#), "{err}");
}

// server_contentmapper_internal_test.go:99
#[test]
fn test_parse_content_mapper_contributions_defaults_options_to_object() {
    let contributions = parse_content_mapper_contributions(&[contribution(
        "publisher.extension",
        ".vue",
        Some(inferred_project_contribution("mapper")),
    )])
    .unwrap();
    assert_eq!(contributions.mappers[0].definition.options, "{}");
}

// server_contentmapper_internal_test.go:112
#[test]
fn test_parse_content_mapper_contributions_compound_extensions() {
    for extension in [".y.z", ".component.tsx.vue"] {
        let contributions = parse_content_mapper_contributions(&[contribution(
            "mapper",
            extension,
            Some(inferred_project_contribution("mapper")),
        )])
        .unwrap();
        assert_eq!(contributions.extensions, [extension]);
    }
    for native_extension in tspath::ALL_SUPPORTED_EXTENSIONS_WITH_JSON.iter().flat_map(|e| e.iter())
    {
        for extension in [
            format!(".component{native_extension}"),
            format!(".component{}", native_extension.to_uppercase()),
        ] {
            let err =
                parse_content_mapper_contributions(&[contribution("mapper", &extension, None)])
                    .unwrap_err();
            assert!(err.to_string().contains("invalid extension"), "{extension}: {err}");
        }
    }
}

fn output_extensions_contribution(
    extension: &str,
    output_extensions: &[(&str, &str)],
    compiler_options: Option<Vec<String>>,
) -> Option<lsproto::ContentMapperContribution> {
    let mut inferred_project = inferred_project_contribution("mapper");
    inferred_project.manifest.compiler_options = compiler_options;
    inferred_project.manifest.output_extensions = Some(
        output_extensions
            .iter()
            .map(|(source, output)| (source.to_string(), output.to_string()))
            .collect(),
    );
    contribution("publisher.extension", extension, Some(inferred_project))
}

// server_contentmapper_internal_test.go:135
#[test]
fn test_parse_content_mapper_contributions_rejects_invalid_output_extensions() {
    for compiler_options in [None, Some(vec![]), Some(vec!["strict".to_string()])] {
        let err = parse_content_mapper_contributions(&[output_extensions_contribution(
            ".vue",
            &[(".vue", "js")],
            compiler_options,
        )])
        .unwrap_err();
        assert!(err.to_string().contains("invalid output extension mapping"), "{err}");
    }
}

// server_contentmapper_internal_test.go:153
#[test]
fn test_parse_content_mapper_contributions_rejects_extension_paths() {
    for extension in [".vue/foo", r".vue\foo"] {
        let err = parse_content_mapper_contributions(&[contribution("mapper", extension, None)])
            .unwrap_err();
        assert!(err.to_string().contains("invalid extension"), "{err}");
        for output_extensions in [[(extension, ".js")], [(".vue", extension)]] {
            let err = parse_content_mapper_contributions(&[output_extensions_contribution(
                ".vue",
                &output_extensions,
                None,
            )])
            .unwrap_err();
            assert!(err.to_string().contains("invalid output extension mapping"), "{err}");
        }
    }
}

// Not a Go test. Go decodes the options as `map[string]any`, and json.Marshal writes map keys sorted.
#[test]
fn contributed_options_are_marshaled_with_sorted_keys() {
    let options = tsrs_core::json::unmarshal(r#"{"z":1,"a":{"y":[{"d":1,"c":2}],"b":3}}"#).unwrap();
    let lsproto::Value::Object(options) = options else { panic!("not an object") };
    let mut inferred_project = inferred_project_contribution("mapper");
    inferred_project.options =
        Some(options.iter().map(|(name, value)| (name.to_string(), value.clone())).collect());
    let contributions = parse_content_mapper_contributions(&[contribution(
        "publisher.extension",
        ".vue",
        Some(inferred_project),
    )])
    .unwrap();
    assert_eq!(
        contributions.mappers[0].definition.options,
        r#"{"a":{"b":3,"y":[{"c":2,"d":1}]},"z":1}"#
    );
}

#[test]
fn null_contribution_reaches_contributor_validation() {
    let params = lsproto::unmarshal::<lsproto::SetContentMapperContributionsParams>(
        br#"{"contributions":[null],"openDocuments":[]}"#,
    )
    .unwrap();
    let err = parse_content_mapper_contributions(&params.contributions).unwrap_err();
    assert_eq!(err.to_string(), "content mapper contribution requires a contributorId");
}

#[test]
fn contributed_extensions_use_go_simple_lowercase() {
    let err = parse_content_mapper_contributions(&[
        contribution("first", ".İ", Some(inferred_project_contribution("first"))),
        contribution("second", ".i", Some(inferred_project_contribution("second"))),
    ])
    .unwrap_err();
    assert_eq!(err.to_string(), r#"content mapper contributions both claim extension ".i""#);

    let result = parse_content_mapper_contributions(&[
        contribution("first", ".İ", Some(inferred_project_contribution("first"))),
        contribution("second", ".i\u{0307}", Some(inferred_project_contribution("second"))),
    ])
    .unwrap();
    assert_eq!(result.extensions, [".i\u{0307}", ".İ"]);
}

#[test]
fn contributed_mapper_errors_use_go_quoting() {
    for (character, quoted) in [
        ('\u{2066}', r"\u2066"),
        ('\u{2069}', r"\u2069"),
        ('\u{0378}', r"\u0378"),
        ('\u{e000}', r"\ue000"),
        ('\u{100000}', r"\U00100000"),
    ] {
        let err = parse_content_mapper_contributions(&[contribution(
            &format!("mapper{character}"),
            ".ts",
            None,
        )])
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                r#"content mapper contribution "mapper{quoted}[0]" has invalid extension ".ts""#
            )
        );
    }
}

#[test]
fn contributed_mapper_directory_is_normalized() {
    for (cwd, expected) in [
        ("/project/src/../lib/", "/project/lib"),
        (r"c:\project\src\..\lib\", "c:/project/lib"),
        ("file:///project/src/../lib/", "file:///project/lib"),
        ("c:/project/..", "c:/"),
        ("//server", "//server/"),
    ] {
        let mut inferred_project = inferred_project_contribution("mapper");
        inferred_project.manifest.cwd = Some(cwd.to_string());
        let contributions = parse_content_mapper_contributions(&[contribution(
            "mapper",
            ".vue",
            Some(inferred_project),
        )])
        .unwrap();
        assert_eq!(contributions.mappers[0].package_directory, expected, "{cwd}");
    }
}
