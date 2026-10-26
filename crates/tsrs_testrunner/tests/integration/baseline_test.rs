use tsrs_testrunner::baseline::*;

#[test]
fn keys_and_classes() {
    let exp = "a.ts(1,5): error TS2322: Type 'x' is not assignable.\r\n  chained line\r\nerror TS5023: Unknown.\r\n\r\n\r\n==== a.ts (1 errors) ====\r\n    let x";
    let act = "a.ts(1,5): error TS2322: Type 'y' is not assignable.\r\nerror TS5023: Unknown.\r\n\r\n\r\n==== a.ts (1 errors) ====\r\n    let x";
    assert_eq!(
        diagnostic_keys(exp),
        vec!["(,) error TS5023".to_string(), "a.ts(1,5) error TS2322".to_string()]
    );
    assert_eq!(classify(Some(exp), act), Class::Codes);
    assert_eq!(classify(Some(exp), exp), Class::Pass);
    assert_eq!(classify(None, NO_CONTENT), Class::Pass);
    assert_eq!(classify(Some(NO_CONTENT), NO_CONTENT), Class::Codes);
    assert_eq!(classify(None, act), Class::Fail);
}
