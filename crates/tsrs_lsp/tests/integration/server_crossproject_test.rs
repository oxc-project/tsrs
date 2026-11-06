use tsrs_lsproto as lsproto;

use crate::lsptestutil::{acknowledge_registrations, new_lsp_client, test_server_options};

const CONSUMER: &str = "import \"../a/a\";\nimport \"../b/b\";\nlet value: Shared;\n";
const FILES: &[(&str, &str)] = &[
    (
        "/home/projects/tsconfig.json",
        r#"{"files":[],"references":[{"path":"a"},{"path":"b"},{"path":"p0"},{"path":"p1"},{"path":"q"}]}"#,
    ),
    (
        "/home/projects/a/tsconfig.json",
        r#"{"compilerOptions":{"composite":true,"declaration":true,"declarationMap":true,"outDir":"dist","noLib":true},"files":["a.ts"]}"#,
    ),
    (
        "/home/projects/b/tsconfig.json",
        r#"{"compilerOptions":{"composite":true,"declaration":true,"declarationMap":true,"outDir":"dist","noLib":true},"files":["b.ts"]}"#,
    ),
    ("/home/projects/a/a.ts", "interface Shared { a: string; }\n"),
    ("/home/projects/b/b.ts", "interface Shared { b: number; }\n"),
    (
        "/home/projects/p0/tsconfig.json",
        r#"{"compilerOptions":{"composite":true,"declaration":true,"declarationMap":true,"outDir":"dist","noLib":true},"files":["index.ts"],"references":[{"path":"../a"},{"path":"../b"}]}"#,
    ),
    (
        "/home/projects/p1/tsconfig.json",
        r#"{"compilerOptions":{"composite":true,"declaration":true,"declarationMap":true,"outDir":"dist","noLib":true},"files":["index.ts"],"references":[{"path":"../a"},{"path":"../b"}]}"#,
    ),
    (
        "/home/projects/q/tsconfig.json",
        r#"{"compilerOptions":{"composite":true,"declaration":true,"declarationMap":true,"outDir":"dist","noLib":true},"files":["index.ts"],"references":[{"path":"../b"}]}"#,
    ),
    ("/home/projects/p0/index.ts", CONSUMER),
    ("/home/projects/p1/index.ts", CONSUMER),
    ("/home/projects/q/index.ts", "import \"../b/b\";\nlet other: Shared;\n"),
];

#[test]
fn references_include_late_projects_of_each_merged_declaration() {
    check_merged_declarations(false);
}

#[test]
fn rename_includes_late_projects_of_each_merged_declaration() {
    check_merged_declarations(true);
}

fn check_merged_declarations(rename: bool) {
    let (client, close_client) = new_lsp_client(
        test_server_options("/home/projects", FILES),
        Some(acknowledge_registrations()),
    );
    let init = lsproto::InitializeParams {
        root_uri: lsproto::DocumentUriOrNull {
            document_uri: Some(lsproto::DocumentUri::from("file:///home/projects")),
        },
        ..Default::default()
    };
    let (msg, _) = client.send_request(lsproto::INITIALIZE_INFO, init);
    assert!(msg.error.is_none(), "{:?}", msg.error);
    client.send_notification(lsproto::INITIALIZED_INFO, lsproto::InitializedParams::default());
    client.server.init_complete().wait();
    let uri = lsproto::DocumentUri::from("file:///home/projects/p0/index.ts");
    client.send_notification(
        lsproto::TEXT_DOCUMENT_DID_OPEN_INFO,
        lsproto::DidOpenTextDocumentParams {
            text_document: lsproto::TextDocumentItem {
                uri: uri.clone(),
                language_id: lsproto::LanguageKind::TYPE_SCRIPT,
                version: 1,
                text: CONSUMER.to_string(),
            },
        },
    );
    let mut actual = Vec::new();
    if rename {
        let mut params = lsproto::RenameParams::default();
        params.text_document.uri = uri;
        params.position = lsproto::Position { line: 2, character: 12 };
        params.new_name = "Renamed".to_string();
        let (msg, result) = client.send_request(lsproto::TEXT_DOCUMENT_RENAME_INFO, params);
        assert!(msg.error.is_none(), "{:?}", msg.error);
        for (uri, edits) in result.unwrap().workspace_edit.unwrap().changes.unwrap() {
            for edit in edits {
                assert_eq!(edit.new_text, "Renamed");
                actual.push((uri.to_string(), edit.range));
            }
        }
    } else {
        let params = lsproto::ReferenceParams {
            text_document: lsproto::TextDocumentIdentifier { uri },
            position: lsproto::Position { line: 2, character: 12 },
            context: lsproto::ReferenceContext { include_declaration: true },
            ..Default::default()
        };
        let (msg, result) = client.send_request(lsproto::TEXT_DOCUMENT_REFERENCES_INFO, params);
        assert!(msg.error.is_none(), "{:?}", msg.error);
        actual.extend(
            result.unwrap().locations.unwrap().into_iter().map(|l| (l.uri.to_string(), l.range)),
        );
    }
    close_client.close().unwrap();
    actual.sort_by(|a, b| a.0.cmp(&b.0));
    let expected = [
        ("a/a.ts", 0, 10),
        ("b/b.ts", 0, 10),
        ("p0/index.ts", 2, 11),
        ("p1/index.ts", 2, 11),
        ("q/index.ts", 1, 11),
    ]
    .map(|(file, line, character)| {
        (
            format!("file:///home/projects/{file}"),
            lsproto::Range {
                start: lsproto::Position { line, character },
                end: lsproto::Position { line, character: character + 6 },
            },
        )
    });
    assert_eq!(actual, expected);
}
