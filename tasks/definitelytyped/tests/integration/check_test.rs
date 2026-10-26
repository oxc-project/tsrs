use tsrs_definitelytyped::check::*;

#[test]
fn assertions_follow_upstream_line_rules() {
    let (assertions, duplicates) = parse_assertions(
        "// $ExpectType number\nfoo; // $ExpectType string\n  // $ExpectType boolean\nbar; // $ExpectType null\n",
    );
    assert!(assertions.is_empty());
    assert_eq!(duplicates, vec![1, 3]);
}

#[test]
fn assertion_after_url_uses_the_comment_delimiter() {
    let (assertions, duplicates) =
        parse_assertions("const url = \"https://example.test\"; // $ExpectType string\n");
    assert_eq!(assertions.get(&0).map(String::as_str), Some("string"));
    assert_eq!(duplicates, Vec::<usize>::new());
}

#[test]
fn utf16_positions_account_for_astral_text() {
    let text = "a😀b\n";
    assert_eq!(utf16_to_byte(text, 3), 5);
    assert_eq!(byte_to_utf16(text, 5), 3);
    assert_eq!(line_of_utf16(text, 4), 0);
    let index = TextIndex::new(text);
    assert_eq!(index.byte_at(3), 5);
    assert_eq!(index.utf16_at(5), 3);
    assert_eq!(index.line_at(6), 0);
}

#[test]
fn expected_type_normalizes_unions_and_readonly_arrays() {
    assert_eq!(normalize_type("string | number"), normalize_type("number | string"));
    assert_eq!(normalize_type("readonly string[]"), normalize_type("ReadonlyArray<string>"));
    assert_eq!(normalize_type("Array<string | number>"), normalize_type("Array<number | string>"));
    assert_eq!(
        normalize_type("{ b: string; a: number }"),
        normalize_type("{ a: number; b: string }")
    );
    assert_eq!(normalize_type("'hello'"), normalize_type("\"hello\""));
}

#[test]
fn lint_disable_allows_line_and_rule_scopes() {
    assert_eq!(lint_disable("/* eslint-disable */"), Some(3));
    assert_eq!(lint_disable("// eslint-disable-next-line"), None);
    assert_eq!(lint_disable("// eslint-disable rule"), None);
}

#[test]
fn typescript_version_ranges_parse_like_expect_error_rules() {
    let version =
        tsrs_core::semver::try_parse_version(&pinned_typescript_version().unwrap()).unwrap();
    assert!(tsrs_core::semver::try_parse_version_range(">=7.0").unwrap().test(&version));
    assert!(!tsrs_core::semver::try_parse_version_range(">=7.2").unwrap().test(&version));
}
