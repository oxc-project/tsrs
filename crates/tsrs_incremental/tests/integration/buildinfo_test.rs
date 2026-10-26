use tsrs_core::ModuleKind;
use tsrs_core::json::JsonMap;
use tsrs_core::json::Value;
use tsrs_incremental::snapshot::FileEmitKind;
use tsrs_incremental::snapshot::FileInfo;

use tsrs_incremental::buildinfo::*;

// `marshal` streams what `marshal_json` builds; a field written in the wrong place, or omitted or kept against
// `marshal_json`'s rules, would change the tsbuildinfo bytes.
#[test]
fn marshal_streams_the_marshal_json_document() {
    let info = |version: &str,
                signature: &str,
                affects_global_scope: bool,
                implied_node_format: ModuleKind| {
        new_build_info_file_info(&FileInfo {
            version: version.into(),
            signature: signature.into(),
            affects_global_scope,
            implied_node_format,
        })
    };
    let diagnostic = BuildInfoDiagnostic {
        file: 2,
        pos: 3,
        end: 9,
        code: 2322,
        message_text: "Type \"a\\b\"\n is not \u{1}assignable.".into(),
        message_key: "k".into(),
        message_args: vec!["x".into()],
        message_chain: vec![BuildInfoDiagnostic {
            code: 1,
            message_text: "chain".into(),
            ..Default::default()
        }],
        reports_unnecessary: true,
        ..Default::default()
    };
    let mut options = JsonMap::new();
    options.insert("strict".to_string(), Value::Bool(true));
    options.insert("target".to_string(), Value::Number(99.0));
    options.insert("lib".to_string(), Value::Array(vec![Value::String("lib.es2022.d.ts".into())]));
    let full = BuildInfo {
        version: "7.1.0".into(),
        errors: true,
        check_pending: true,
        root: vec![
            BuildInfoRoot { start: 1, end: 3, ..Default::default() },
            BuildInfoRoot { start: 5, end: 5, ..Default::default() },
        ],
        package_jsons: Some(vec!["../package.json".into()]),
        missing_package_jsons: Some(vec![]),
        content_mapper_identities: None,
        tsrs_canonical_check_signature: "go4-v1:0123456789abcdef0123456789abcdef".into(),
        tsrs_performance: "canonical,lazy".into(),
        file_names: vec!["lib.d.ts".into(), "../src/a.ts".into(), "../src/ü \"q\".ts".into()],
        file_infos: vec![
            info("v1", "v1", false, ModuleKind::COMMON_JS),
            info("v2", "", true, ModuleKind::ES_NEXT),
            info("v3", "s3", false, ModuleKind::ES_NEXT),
        ],
        file_infos_non_nil: true,
        file_ids_list: vec![vec![1, 2], vec![3], vec![1, 2, 3, 123_456_789]],
        options: Some(options),
        referenced_map: vec![
            BuildInfoReferenceMapEntry { file_id: 2, file_id_list_id: 1 },
            BuildInfoReferenceMapEntry { file_id: 3, file_id_list_id: 3 },
        ],
        semantic_diagnostics_per_file: vec![
            BuildInfoSemanticDiagnostic { file_id: 1, diagnostics: None },
            BuildInfoSemanticDiagnostic {
                file_id: 0,
                diagnostics: Some(BuildInfoDiagnosticsOfFile {
                    file_id: 2,
                    diagnostics: vec![diagnostic.clone()],
                }),
            },
        ],
        emit_diagnostics_per_file: vec![BuildInfoDiagnosticsOfFile {
            file_id: 3,
            diagnostics: vec![diagnostic],
        }],
        change_file_set: vec![2, 3],
        affected_files_pending_emit: vec![BuildInfoFilePendingEmit {
            file_id: 2,
            emit_kind: FileEmitKind::Js,
        }],
        latest_changed_dts_file: "../out/a.d.ts".into(),
        emit_signatures: vec![BuildInfoEmitSignature {
            file_id: 2,
            signature: "s".into(),
            differs_only_in_dts_map: false,
            differs_in_options: true,
        }],
        resolved_root: vec![BuildInfoResolvedRoot { resolved: 2, root: 1 }],
        semantic_errors: true,
    };
    let empty =
        BuildInfo { version: "7.1.0".into(), file_infos_non_nil: true, ..Default::default() };
    let bare = BuildInfo { version: "7.1.0".into(), ..Default::default() };
    for build_info in [full, empty, bare] {
        let expected = tsrs_core::json::marshal(&build_info.marshal_json()).unwrap();
        assert_eq!(build_info.marshal(), expected);
        if build_info.tsrs_canonical_check_signature.is_empty() {
            assert_eq!(
                BuildInfo::unmarshal(&expected).unwrap().tsrs_canonical_check_signature,
                build_info.tsrs_canonical_check_signature
            );
        } else {
            assert!(
                BuildInfo::unmarshal(&expected).is_err(),
                "default history rejects fast caches"
            );
        }
    }
}
