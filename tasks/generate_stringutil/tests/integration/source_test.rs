use tsrs_generate_stringutil::source::*;

#[test]
fn rejects_malformed_data_and_invalid_scalars() {
    assert_eq!(mapping("module.exports=new Map([[931,[962]]])").unwrap()[&931], "ς");
    assert_eq!(mapping("module.exports=new Map([[65,97]])").unwrap()[&65], "a");
    for input in [
        "module.exports=new Map([[65,97],[65,98]])",
        "module.exports=new Map([[55296,97]])",
        "module.exports=new Map([[65,[1114112]]])",
        "module.exports=new Map([[65,1.5]])",
        "module.exports=new Map([[65,-1]])",
        "module.exports=new Map(truncated)",
    ] {
        assert!(mapping(input).is_err(), "{input}");
    }
    assert!(verify_integrity(b"modified archive").is_err());
}

#[test]
fn decodes_range_gaps_and_rejects_truncation() {
    let wrap = |data: &str| format!("module.exports=require('../../decode-ranges.js')('{data}')");
    assert_eq!(code_points(&wrap("ABAC")).unwrap(), [0, 1, 3, 4, 5]);
    assert_eq!(code_points(&wrap("hBA")).unwrap(), [65]);
    assert_eq!(code_points(&wrap("wQAA")).unwrap(), [1024]);
    assert_eq!(code_points(&wrap("4QAAA")).unwrap(), [65536]);
    for data in ["?A", "h", "wQ", "A"] {
        assert!(code_points(&wrap(data)).is_err(), "{data}");
    }
}
