use tsrs_api_transport::strictjson::{decode_scan, fast_decode};

const SEEDS: &[&str] = &[
    r#"{"snapshot":1,"project":"/p/tsconfig.json","symbol":{"kind":0,"file":{"fileName":"/p/a.ts","path":"/p/a.ts","contentHash":"2bf6352bdccdb064eede8738b1391b80","parseOptionsKey":"0","scriptKind":3,"nodeId":"95"},"id":10}}"#,
    r#"{"snapshot":2,"project":"/p/tsconfig.json","types":[1,2,3],"flags":-7,"ok":true,"no":false,"z":null}"#,
    r#" [ {"a" : [ [] , {} , 0 , -0 , 123456789012345 ] } ] "#,
    r#"{"a":{"b":{"c":{"d":[null,true,false,"x"]}}}}"#,
];

/// Mutations of request-shaped documents: wherever the fast path accepts, the full scan accepts with
/// the same value (compared as marshaled text, which keeps member order) and no recorded literals.
#[test]
fn fast_path_accepts_only_what_the_scan_accepts_identically() {
    let marshal = |v: &tsrs_core::json::Value| tsrs_core::json::marshal(v).unwrap();
    for seed in SEEDS {
        let fast = fast_decode(seed.as_bytes(), |_| None)
            .unwrap_or_else(|| panic!("fast path declined {seed}"));
        assert_eq!(marshal(&fast), marshal(&decode_scan(seed.as_bytes()).ok().unwrap().value));
    }
    let alphabet = br#"{}[]":,-0123456789.eE+tfnrulsa \/u"#;
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % bound as u64) as usize
    };
    let (mut accepted, mut cases) = (0, 0);
    for round in 0..40_000 {
        let mut doc = SEEDS[round % SEEDS.len()].as_bytes().to_vec();
        for _ in 0..=next(3) {
            let at = next(doc.len() + 1);
            let c = alphabet[next(alphabet.len())];
            match next(4) {
                0 if at < doc.len() => {
                    doc.remove(at);
                }
                1 if at < doc.len() => doc[at] = c,
                2 if at < doc.len() => {
                    // Repeat a stretch, which duplicates member names and values.
                    let end = (at + 1 + next(24)).min(doc.len());
                    let stretch = doc[at..end].to_vec();
                    doc.splice(at..at, stretch);
                }
                _ => doc.insert(at, c),
            }
        }
        cases += 1;
        // Some member names are borrowed and some allocated, as with a request's field names.
        let names =
            |name: &str| ["snapshot", "project", "file", "a"].into_iter().find(|n| *n == name);
        let Some(fast) = fast_decode(&doc, names) else { continue };
        accepted += 1;
        let shown = String::from_utf8_lossy(&doc);
        let full = decode_scan(&doc).unwrap_or_else(|_| panic!("fast path accepted {shown}"));
        assert!(full.lexemes.is_empty(), "{shown}");
        assert_eq!(marshal(&fast), marshal(&full.value), "{shown}");
    }
    // Enough mutants stay valid for this to test acceptance, not only rejection.
    assert!(accepted * 20 > cases, "{accepted} of {cases}");
}

#[test]
fn fast_path_leaves_what_it_does_not_handle_to_the_scan() {
    for doc in [
        "",
        r#"{"a":"\u0041"}"#,
        r#"{"a":"é"}"#,
        r#"{"a":1.5}"#,
        r#"{"a":1e3}"#,
        r#"{"a":1234567890123456}"#,
        r#"{"a":1,"a":2}"#,
        r#"{"a":01}"#,
        "[1,]",
        r#"{"a":1}x"#,
    ] {
        assert!(fast_decode(doc.as_bytes(), |_| None).is_none(), "{doc}");
    }
    let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
    assert!(fast_decode(deep.as_bytes(), |_| None).is_none());
    let ok = format!("{}{}", "[".repeat(64), "]".repeat(64));
    assert!(fast_decode(ok.as_bytes(), |_| None).is_some());
}
