use crate::*;

// lsp_test.go:10
#[test]
fn test_unmarshal_completion_item() {
    const MESSAGE: &str = r#"{
    "label": "pageXOffset",
    "insertTextFormat": 1,
    "textEdit": {
        "newText": "pageXOffset",
        "insert": {
            "start": {
                "line": 4,
                "character": 0
            },
            "end": {
                "line": 4,
                "character": 4
            }
        },
        "replace": {
            "start": {
                "line": 4,
                "character": 0
            },
            "end": {
                "line": 4,
                "character": 4
            }
        }
    },
    "kind": 6,
    "sortText": "15",
    "commitCharacters": [
        ".",
        ",",
        ";"
    ]
}"#;

    let result = unmarshal::<CompletionItem>(MESSAGE.as_bytes()).unwrap();

    assert_eq!(
        result,
        CompletionItem {
            label: "pageXOffset".to_string(),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            text_edit: Some(TextEditOrInsertReplaceEdit {
                insert_replace_edit: Some(InsertReplaceEdit {
                    new_text: "pageXOffset".to_string(),
                    insert: Range {
                        start: Position { line: 4, character: 0 },
                        end: Position { line: 4, character: 4 }
                    },
                    replace: Range {
                        start: Position { line: 4, character: 0 },
                        end: Position { line: 4, character: 4 }
                    },
                }),
                ..Default::default()
            }),
            kind: Some(CompletionItemKind::VARIABLE),
            sort_text: Some("15".to_string()),
            commit_characters: Some(vec![".".to_string(), ",".to_string(), ";".to_string()]),
            ..Default::default()
        }
    );
}

// tsrs-only: `DocumentUri::check` rejects the URIs `file_name` cannot turn into a file name (Go panics on them, ending
// the session), and accepts those it can.
#[test]
fn check_rejects_uris_without_a_file_name() {
    for bad in ["", "not a uri", "%%%", "relative/path.ts", "file://", "file:///%zz", "1abc:x"] {
        assert!(DocumentUri::from(bad).check().is_err(), "{bad:?} passed");
    }
    for good in [
        "file:///a/b.ts",
        "file:///c%3A/a/b.ts",
        "file://server/share/a.ts",
        "untitled:Untitled-1",
        "vscode-notebook-cell:/a.ipynb#x",
        "foo://host/x",
        "foo://host",
        "bundled:///libs/lib.d.ts",
    ] {
        let uri = DocumentUri::from(good);
        uri.check().unwrap_or_else(|err| panic!("{good:?}: {err}"));
        assert!(
            tsrs_core::tspath::path_is_absolute(&uri.file_name()) || !good.starts_with("file:"),
            "{good:?}"
        );
    }
}

#[test]
fn test_dynamic_document_uri_roundtrip() {
    for uri in [
        "untitled:folder/../file.ts",
        "vscode-vfs://github/path//file.ts",
        "custom:/path/./file.ts/",
        "custom:",
        "custom:///path",
        "custom://authority",
        "custom://authority/",
        "custom:path/file.ts?rev=a/b#frag/c",
        "custom://authority/path/file.ts#frag/a",
        "custom:path\\file.ts",
        "custom:.git/file.ts",
        "custom:..hidden/file.ts",
        "custom://~ts-uri~/path",
        "custom://ts-nul-authority/path",
        "custom:~ts-uri-escape~file.ts",
        "custom:~ts-uri-escape~no-path",
        "custom://authority/~ts-uri-no-path~~",
        "custom:~ts-uri-spec~666f6f~/file.ts?x=1",
        "custom:folder/../~ts-uri~/café\\file.ts",
        "custom:name.ts\\",
        "custom:name..ts",
    ] {
        let document = DocumentUri::from(uri);
        let file_name = document.file_name();
        assert_eq!(DocumentUri::dynamic_file_name_to_document_uri(&file_name).0, uri, "{uri}");
    }

    assert_eq!(
        DocumentUri::from("custom:folder/../~ts-uri~/café\\file.ts").file_name(),
        "^/~ts-uri~/custom/ts-nul-authority/folder/~ts-uri-escape~2e2e~/~ts-uri~/~ts-uri-escape~636166c3a95c66696c65~.ts"
    );
    assert_eq!(
        DocumentUri::from("custom:~ts-uri-escape~dir.js/file.ts?x=1").file_name(),
        "^/~ts-uri~/custom/ts-nul-authority/~ts-uri-escape~7e74732d7572692d6573636170657e6469722e6a73~/~ts-uri-escape~66696c65003f783d31~.ts"
    );
    assert_ne!(
        DocumentUri::from("custom:name.ts\\").file_name(),
        DocumentUri::from("custom:name..ts").file_name()
    );
    assert!(
        DocumentUri::from("custom:~ts-uri-escape~types.d.css.ts")
            .file_name()
            .ends_with(".d.css.ts")
    );
}
