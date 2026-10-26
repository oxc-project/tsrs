use tsrs_core::json::Value;

#[test]
fn exact_literals_by_object_identity() {
    let raw = br#"{"snapshot":9007199254740993,"inner":{"snapshot":9007199254740992},"list":[{"snapshot":9007199254740995}]}"#;
    let Ok(decoded) = tsrs_api_transport::strictjson::decode(raw) else { panic!("valid") };
    let root = decoded.value;
    let _g = tsrs_api::predecode::enter_request(raw, decoded.lexemes, &root);
    assert_eq!(tsrs_api::predecode::exact_u64(&root, "snapshot"), Some(9_007_199_254_740_993));
    let Value::Object(o) = &root else { unreachable!() };
    assert_eq!(
        tsrs_api::predecode::exact_u64(o.get("inner").unwrap(), "snapshot"),
        Some(9_007_199_254_740_992)
    );
    let Value::Array(list) = o.get("list").unwrap() else { unreachable!() };
    assert_eq!(tsrs_api::predecode::exact_u64(&list[0], "snapshot"), Some(9_007_199_254_740_995));
    // A copy outside the tree has no recorded literal.
    let copy = o.get("inner").unwrap().clone();
    assert_eq!(tsrs_api::predecode::exact_u64(&copy, "snapshot"), None);
}

#[test]
fn raw_values_by_pointer() {
    let raw = br#"{"requests":[{"method":"a","params":{"x":1e3}},{"par\u0061ms": {"snapshot":9007199254740993} ,"method":"b"}]}"#;
    assert_eq!(
        tsrs_api::predecode::raw_value_at(raw, "/requests/0/params"),
        Some(&br#"{"x":1e3}"#[..])
    );
    assert_eq!(
        tsrs_api::predecode::raw_value_at(raw, "/requests/1/params"),
        Some(&br#"{"snapshot":9007199254740993}"#[..])
    );
    assert_eq!(tsrs_api::predecode::raw_value_at(raw, "/requests/2/params"), None);
    assert_eq!(tsrs_api::predecode::raw_value_at(raw, "/requests/0/nope"), None);
    let params = tsrs_api::predecode::batch_params(raw);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].as_deref(), Some(&br#"{"x":1e3}"#[..]));
    assert_eq!(params[1].as_deref(), Some(&br#"{"snapshot":9007199254740993}"#[..]));
    assert_eq!(
        tsrs_api::predecode::batch_params(br#"{"requests":[1,{"method":"ping"},null]}"#),
        vec![None, None, None]
    );
}
