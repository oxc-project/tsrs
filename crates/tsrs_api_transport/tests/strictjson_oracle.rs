// Decoder strictness against a Go binary built at the pinned commit (tests/fixtures/go_json_oracle_main.go.txt,
// inputs from go_json_oracle_cases.mjs): duplicate member names, unpaired surrogate escapes and invalid
// UTF-8 are rejected with jsontext's error text; valid input (including escaped astral pairs) decodes.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde_json::Value;
use tsrs_api_transport::callbackfs::{CallbackConfig, CallbackFs};
use tsrs_api_transport::jsonrpc::decode_message;
use tsrs_api_transport::{Caller, TransportError};
use tsrs_vfs::FS;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

struct Fixed(Vec<u8>);

impl Caller for Fixed {
    fn call(&self, _method: &str, _params: Option<&[u8]>) -> Result<Vec<u8>, TransportError> {
        Ok(self.0.clone())
    }
    fn notify(&self, _: &str, _: Option<&[u8]>) -> Result<(), TransportError> {
        Ok(())
    }
}

fn read_file_via_callback(response: Vec<u8>) -> Result<Option<String>, String> {
    let fs = CallbackFs::new(
        Arc::new(tsrs_vfs::osvfs::fs()),
        &CallbackConfig::parse(&["readFile"]).unwrap(),
        None,
    );
    fs.set_connection(Arc::new(Fixed(response)));
    catch_unwind(AssertUnwindSafe(|| fs.read_file("/x.ts")))
        .map_err(|p| p.downcast_ref::<String>().cloned().unwrap_or_default())
}

#[test]
fn decoders_match_pinned_go() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/go_json_oracle.json")).unwrap();
    let (mut rejected, mut accepted) = (0, 0);
    for case in &cases {
        let input = hex(case["input"].as_str().unwrap());
        let shown = String::from_utf8_lossy(&input).into_owned();
        let go_err = case["error"].as_str().unwrap();
        match case["kind"].as_str().unwrap() {
            "requestfs" => {
                let ours = tsrs_api_transport::strictjson::validate(&input);
                if go_err.is_empty() {
                    assert!(ours.is_ok(), "{shown}: {ours:?}");
                } else {
                    assert_eq!(ours.unwrap_err(), go_err, "{shown}");
                }
            }
            "readFile" => {
                let ours = read_file_via_callback(input.clone());
                if go_err.is_empty() {
                    let content = ours.unwrap_or_else(|e| panic!("{shown}: {e}")).unwrap();
                    assert_eq!(
                        content.as_bytes(),
                        &hex(case["hex"].as_str().unwrap())[..],
                        "{shown}"
                    );
                } else {
                    assert_eq!(
                        ours.unwrap_err(),
                        go_err.strip_prefix("response: ").unwrap(),
                        "{shown}"
                    );
                }
            }
            "jsonrpc" => {
                let ours = decode_message(&input);
                if go_err.is_empty() {
                    assert!(ours.is_ok(), "{shown}: {ours:?}");
                } else {
                    assert_eq!(ours.unwrap_err().to_string(), go_err, "{shown}");
                }
            }
            other => panic!("unknown case kind {other}"),
        }
        if go_err.is_empty() {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    assert_eq!((accepted, rejected), (10, 28));
}

#[test]
fn deep_nesting_is_bounded() {
    let deep = "[".repeat(20000);
    let err = tsrs_api_transport::strictjson::validate(deep.as_bytes()).unwrap_err();
    assert!(err.contains("exceeded max depth"), "{err}");
    let ok = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
    assert!(tsrs_api_transport::strictjson::validate(ok.as_bytes()).is_ok());
}

/// Grammar errors: every input of `go_syntax_oracle.json` (decoded by the pinned Go binary into `any`) is
/// accepted or rejected exactly as Go does, with Go's error text (pointer and offset rules included).
#[test]
fn syntax_errors_match_pinned_go_text() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/go_syntax_oracle.json")).unwrap();
    let mut mismatches = Vec::new();
    mismatches.extend(cases.iter().filter_map(|case| {
        let input = hex(case["input"].as_str().unwrap());
        let go = case["error"].as_str().unwrap();
        let ours = tsrs_api_transport::strictjson::validate(&input).err().unwrap_or_default();
        if ours == go {
            None
        } else {
            Some(format!("{:?}\n   go: {go}\n  ours: {ours}", String::from_utf8_lossy(&input)))
        }
    }));
    assert!(cases.len() >= 100, "{}", cases.len());
    assert!(
        mismatches.is_empty(),
        "{} of {} differ:\n{}",
        mismatches.len(),
        cases.len(),
        mismatches.join("\n")
    );
}

/// readFile `value` decoding follows Go's json.Unmarshal into a string (absent, null, wrong kinds).
#[test]
fn read_file_value_kinds_match_pinned_go() {
    let cases = [
        (r#"{"kind":"value"}"#, Err("jsontext: unexpected EOF")),
        (r#"{"kind":"value","value":null}"#, Ok("")),
        (r#"{"kind":"value","value":5}"#, Err("json: cannot unmarshal JSON number into Go string")),
        (
            r#"{"kind":"value","value":true}"#,
            Err("json: cannot unmarshal JSON boolean into Go string"),
        ),
        (r#"{"kind":"value","value":[]}"#, Err("json: cannot unmarshal JSON array into Go string")),
        (
            r#"{"kind":"value","value":{}}"#,
            Err("json: cannot unmarshal JSON object into Go string"),
        ),
        (
            r#"{"kind":"value","value":-1.5e3}"#,
            Err("json: cannot unmarshal JSON number into Go string"),
        ),
    ];
    for (input, want) in cases {
        let got = read_file_via_callback(input.as_bytes().to_vec());
        match want {
            Ok(content) => {
                assert_eq!(got.as_ref().map(|c| c.as_deref()), Ok(Some(content)), "{input}");
            }
            Err(text) => assert_eq!(got.unwrap_err(), text, "{input}"),
        }
    }
}

/// `decode` is `validate` then `json::unmarshal` in one scan: the same error for every oracle input and edge
/// case, and the same value when both accept.
#[test]
fn decode_matches_validate_then_unmarshal() {
    use tsrs_api_transport::strictjson::{DecodeError, decode, validate};
    let mut inputs: Vec<Vec<u8>> = [
        include_str!("fixtures/go_json_oracle.json"),
        include_str!("fixtures/go_syntax_oracle.json"),
    ]
    .into_iter()
    .flat_map(|fixture| serde_json::from_str::<Vec<Value>>(fixture).unwrap())
    .map(|case| hex(case["input"].as_str().unwrap()))
    .collect();
    inputs.extend(
        [
            "",
            " null ",
            r#"{"a":1e400}"#,
            r#"{"a":1e400,"a":1}"#,
            r#"[1e400,"\ud800"]"#,
            r#"{"a":[-0,0.5,1e3,-12,123456789012345,1234567890123456,9007199254740993],"b":{}}"#,
            r#"{"a/b~c":{"d":[{"e":2.5}]},"x":[[],{}],"y":true,"z":false,"n":null}"#,
            r#"{"snapshot":1,"s":"😀 é","t":"\n"}"#,
        ]
        .map(|s| s.as_bytes().to_vec()),
    );
    for input in &inputs {
        let shown = String::from_utf8_lossy(input);
        let ours = decode(input);
        if let Err(want) = validate(input) {
            assert_eq!(ours.err(), Some(DecodeError::Invalid(want)), "{shown}");
            continue;
        }
        match tsrs_core::json::unmarshal(std::str::from_utf8(input).unwrap()) {
            Ok(want) => {
                let got = ours.unwrap_or_else(|e| panic!("{shown}: {e:?}")).value;
                // `Value`'s `==` ignores member order; the marshaled text does not.
                let marshal = |v: &tsrs_core::json::Value| tsrs_core::json::marshal(v).unwrap();
                assert_eq!((marshal(&got), &got), (marshal(&want), &want), "{shown}");
            }
            Err(want) => assert_eq!(ours.err(), Some(DecodeError::OutOfRange(want)), "{shown}"),
        }
    }
}

/// Only numbers whose f64 may differ from the literal keep their literal, under the JSON pointer of the number.
#[test]
fn decode_keeps_inexact_number_literals() {
    let decoded = tsrs_api_transport::strictjson::decode(
        br#"{"a":1e3,"b":[1,2.5,{"c":3}],"d":{"e":9007199254740993},"f":"9","g~/h":-0.0,"i":123456789012345}"#,
    )
    .unwrap_or_else(|_| panic!("valid"));
    let mut lexemes: Vec<_> = decoded.lexemes.iter().collect();
    lexemes.sort();
    assert_eq!(
        lexemes,
        [
            (&"/a".to_string(), &"1e3".to_string()),
            (&"/b/1".to_string(), &"2.5".to_string()),
            (&"/d/e".to_string(), &"9007199254740993".to_string()),
            (&"/g~0~1h".to_string(), &"-0.0".to_string()),
        ]
    );
}
