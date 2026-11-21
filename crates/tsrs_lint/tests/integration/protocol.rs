use std::io::{self, Write};

use tsrs_core::json::{self, Value};
use tsrs_lint::payload::deserialize_payload;
use tsrs_lint::protocol::{
    self, Diagnostic, Fix, LabeledRange, MessageType, Range, RuleMessage, Suggestion,
};

#[test]
fn v1_conversion_and_v2_options() {
    let old = deserialize_payload(r#"{"files":[{"file_path":"a.ts","rules":["await-thenable"]}]}"#)
        .unwrap();
    let new = deserialize_payload(
        r#"{"version":2,"configs":[{"file_paths":["a.ts"],"rules":[{"name":"await-thenable"}]}]}"#,
    )
    .unwrap();
    assert_eq!(old, new);
    let payload = deserialize_payload(r#"{"version":2,"configs":[{"file_paths":["a.ts","b.ts"],"rules":[{"name":"no-unsafe-unary-minus","options":[{"ignoreVoid":false}]}]}],"source_overrides":{"a.ts":"é😀"},"report_syntactic":true,"report_semantic":true}"#).unwrap();
    assert_eq!(payload.configs[0].file_paths, ["a.ts", "b.ts"]);
    assert!(matches!(payload.configs[0].rules[0].options, Value::Array(_)));
    assert_eq!(payload.source_overrides["a.ts"], "é😀");
    assert!(payload.report_syntactic && payload.report_semantic);
}

#[test]
fn payload_rejects_unpaired_surrogates_in_all_json_strings() {
    for escape in [
        r"\ud800",
        r"\udbff",
        r"\udc00",
        r"\udfff",
        r"\ud800x\udc00",
        r"\ud800\n\udc00",
        r"\ud800\u0041",
        r"\ud800\ud800",
        r"\udc00\ud800",
        r"\udc00\udfff",
        r"\ud800\\udc00",
    ] {
        for input in [
            format!(r#"{{"version":2,"source_overrides":{{"a.ts":"{escape}"}}}}"#),
            format!(r#"{{"version":2,"source_overrides":{{"{escape}":""}}}}"#),
            format!(
                r#"{{"version":2,"configs":[{{"rules":[{{"name":"await-thenable","options":["{escape}"]}}]}}]}}"#
            ),
            format!(
                r#"{{"version":2,"configs":[{{"rules":[{{"name":"await-thenable","options":{{"{escape}":true}}}}]}}]}}"#
            ),
            format!(r#"{{"version":2,"ignored":{{"nested":["{escape}"]}}}}"#),
            format!(r#"{{"version":2,"{escape}":null}}"#),
            format!(r#"{{"files":[{{"file_path":"{escape}"}}]}}"#),
        ] {
            assert!(
                deserialize_payload(&input)
                    .unwrap_err()
                    .starts_with("jsontext: invalid surrogate pair"),
                "{input}"
            );
        }
    }
}

#[test]
fn payload_preserves_valid_unicode_and_textual_source_escapes() {
    let payload = deserialize_payload(
        r#"{"version":2,"configs":[{"file_paths":["\ud83d\ude00.ts"],"rules":[{"name":"await-thenable","options":{"\ud800\udc00":"\udbff\udfff","text":"\"\\\/\b\f\n\r\t"}}]}],"source_overrides":{"\ud83d\ude00.ts":"const s = \"\\ud800\\udc00\"; // �\ufffdé中\ud83d\ude00"},"ignored":{"\ud800\udc00":["\udbff\udfff"]}}"#,
    )
    .unwrap();
    assert_eq!(payload.configs[0].file_paths, ["😀.ts"]);
    assert_eq!(payload.source_overrides["😀.ts"], "const s = \"\\ud800\\udc00\"; // ��é中😀");
    let Value::Object(options) = &payload.configs[0].rules[0].options else { panic!() };
    assert_eq!(options["𐀀"], Value::String("\u{10FFFF}".into()));
    assert_eq!(options["text"], Value::String("\"\\/\x08\x0c\n\r\t".into()));
}

#[test]
fn rejects_invalid_payloads() {
    for input in [
        "{",
        "[]",
        r#"{"version":1}"#,
        r#"{"version":2.5}"#,
        r#"{"version":"2"}"#,
        r#"{"files":[]}"#,
        r#"{"version":2,"configs":{}}"#,
        r#"{"version":2,"report_semantic":"yes"}"#,
        r#"{"version":2,"source_overrides":{"a.ts":5}}"#,
    ] {
        assert!(deserialize_payload(input).is_err(), "{input}");
    }
    assert_eq!(deserialize_payload(r#"{"version":2}"#).unwrap().configs.len(), 0);
}

struct ShortWriter(Vec<u8>);

impl Write for ShortWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let len = bytes.len().min(2);
        self.0.extend_from_slice(&bytes[..len]);
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn framing_handles_short_writes_and_utf8_byte_lengths() {
    let mut writer = ShortWriter(Vec::new());
    protocol::write_error(&mut writer, "é😀").unwrap();
    assert_eq!(writer.0[4], MessageType::Error as u8);
    assert_eq!(u32::from_le_bytes(writer.0[..4].try_into().unwrap()) as usize, writer.0.len() - 5);
    assert_eq!(
        json::unmarshal(std::str::from_utf8(&writer.0[5..]).unwrap()).unwrap(),
        protocol::object([("error", Value::String("é😀".into()))])
    );
}

#[test]
fn diagnostic_omissions_labels_and_suggestions() {
    let diagnostic = Diagnostic {
        message: RuleMessage {
            id: "test".into(),
            description: "message".into(),
            help: String::new(),
        },
        ..Default::default()
    };
    let Value::Object(value) = diagnostic.to_json() else { panic!() };
    assert_eq!(value["file_path"], Value::Null);
    for key in ["range", "rule", "fixes", "suggestions", "labeled_ranges"] {
        assert!(!value.contains_key(key));
    }
    let diagnostic = Diagnostic {
        rule: Some("await-thenable".into()),
        range: Some(Range { pos: 0, end: 4 }),
        suggestions: vec![Suggestion {
            message: RuleMessage::default(),
            fixes: vec![Fix { text: String::new(), range: Range { pos: 0, end: 5 } }],
        }],
        ..diagnostic
    };
    let Value::Object(value) = diagnostic.to_json() else { panic!() };
    assert_eq!(value["kind"], Value::Integer(0));
    assert!(value.contains_key("suggestions"));
}

#[test]
fn borrowed_diagnostic_frames_match_value_encoding() {
    let controls: String = (0..32).map(char::from).collect();
    let unicode = format!("é中😀\u{2028}\u{2029}<>&\"\\/{controls}");
    for text in ["", "ordinary text", unicode.as_str()] {
        for mask in 0..128 {
            let message = RuleMessage {
                id: text.into(),
                description: text.into(),
                help: if mask & 8 == 0 { String::new() } else { text.into() },
            };
            let fixes = vec![
                Fix { text: String::new(), range: Range { pos: 0, end: 0 } },
                Fix { text: text.into(), range: Range { pos: 17, end: u32::MAX } },
            ];
            let diagnostic = Diagnostic {
                range: (mask & 1 != 0).then_some(Range { pos: u32::MAX, end: 0 }),
                message: message.clone(),
                file_path: (mask & 2 != 0).then(|| text.into()),
                rule: (mask & 4 != 0).then(|| text.into()),
                labeled_ranges: if mask & 16 == 0 {
                    vec![]
                } else {
                    vec![
                        LabeledRange { label: String::new(), range: Range { pos: 0, end: 0 } },
                        LabeledRange { label: text.into(), range: Range { pos: 1, end: u32::MAX } },
                    ]
                },
                fixes: if mask & 32 == 0 { vec![] } else { fixes.clone() },
                suggestions: if mask & 64 == 0 {
                    vec![]
                } else {
                    vec![
                        Suggestion { message: RuleMessage::default(), fixes: vec![] },
                        Suggestion { message, fixes },
                    ]
                },
            };
            let mut expected = Vec::new();
            protocol::write_message(&mut expected, MessageType::Diagnostic, &diagnostic.to_json())
                .unwrap();
            let mut actual = ShortWriter(Vec::new());
            protocol::write_diagnostic(&mut actual, &diagnostic).unwrap();
            assert_eq!(actual.0, expected, "mask {mask}, text {text:?}");
        }
    }
}

struct FailingWriter {
    bytes: Vec<u8>,
    remaining: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let count = bytes.len().min(self.remaining).min(2);
        self.bytes.extend_from_slice(&bytes[..count]);
        self.remaining -= count;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn borrowed_diagnostic_frames_preserve_write_errors() {
    let diagnostic = Diagnostic {
        message: RuleMessage {
            id: "id".into(), description: "é😀\n".into(), help: "help".into()
        },
        ..Default::default()
    };
    let value = diagnostic.to_json();
    let mut frame = Vec::new();
    protocol::write_message(&mut frame, MessageType::Diagnostic, &value).unwrap();
    for remaining in [0, 1, 4, 5, 6, frame.len() / 2, frame.len() - 1, frame.len()] {
        let mut before = FailingWriter { bytes: Vec::new(), remaining };
        let mut after = FailingWriter { bytes: Vec::new(), remaining };
        let before_error = protocol::write_message(&mut before, MessageType::Diagnostic, &value)
            .err()
            .map(|error| error.kind());
        let after_error =
            protocol::write_diagnostic(&mut after, &diagnostic).err().map(|error| error.kind());
        assert_eq!(after_error, before_error);
        assert_eq!(after.bytes, before.bytes);
        assert_eq!(after.bytes, frame[..remaining]);
    }
}

#[test]
fn inventory_is_complete_unique_and_implemented() {
    let mut names = tsrs_lint::registry::BASELINE_RULES.to_vec();
    assert_eq!(names.len(), 62);
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 62);
    for name in names {
        tsrs_lint::registry::require_implemented(name).unwrap();
        let config = tsrs_lint::payload::ConfiguredRule { name: name.into(), ..Default::default() };
        assert!(tsrs_lint::registry::create_rule(&config).is_ok(), "{name}");
    }
    assert_eq!(
        tsrs_lint::registry::require_implemented("absent").unwrap_err(),
        "unknown rule: absent"
    );
}

#[test]
fn empty_v1_and_null_errors_match_reference() {
    for input in ["{}", "null", r#"{"files":[]}"#] {
        assert_eq!(deserialize_payload(input).unwrap_err(), "V1 payload has no files");
    }
}
