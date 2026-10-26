#[test]
fn go_kind_text() {
    assert_eq!(
        tsrs_api::printing::go_kind_text("unexpected node: TypeLiteral"),
        "unexpected node: KindTypeLiteral"
    );
    assert_eq!(
        tsrs_api::printing::go_kind_text("unexpected node: KindTypeLiteral"),
        "unexpected node: KindTypeLiteral"
    );
    assert_eq!(tsrs_api::printing::go_kind_text("index out of range: 3"), "index out of range: 3");
}
