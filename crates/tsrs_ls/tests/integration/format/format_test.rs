use tsrs_ls::format::api::*;

use tsrs_core::{ScriptKind, Tristate};

use crate::astnav::tokens_test::parse_for_test;
use crate::format::api_test::{apply_bulk_edits, settings};
use tsrs_ls::lsutil;

// format_test.go:15
#[test]
fn test_format_no_trailing_space() {
    let test_cases = [
        ("simple statement without trailing newline", "1;"),
        ("function call without trailing newline", "console.log('hello');"),
        ("if block on single line", "if (true) { }"),
        ("class declaration", "class A {\n    // Class Contents Go Here\n}"),
        (
            "class declaration with trailing newline",
            "class A {\n    // Class Contents Go Here\n}\n",
        ),
        ("empty block", "if (true) {}"),
        ("module declaration", "module M { }"),
        ("enum declaration", "enum E { A, B }"),
    ];

    for (name, text) in test_cases {
        let ctx = with_format_code_settings(
            &FormatContext::default(),
            settings(4, 4, 0, Tristate::True, |_| {}),
            "\n",
        );
        let source_file = parse_for_test("/test.ts", text, ScriptKind::TS);
        let edits = format_document(&ctx, source_file);
        let new_text = apply_bulk_edits(text, &edits);
        // Formatting should not add trailing whitespace at end of file
        for (i, line) in new_text.split('\n').enumerate() {
            let trimmed = line.trim_end_matches([' ', '\t']);
            assert_eq!(
                line,
                trimmed,
                "{name}: Formatter should not add trailing whitespace on line {}",
                i + 1
            );
        }
    }
}

// Upstream f9f8d01292: a template token may span lines even when it starts on the
// same line as the previous token. The spacing rule compares token starts.
#[test]
fn test_format_template_tokens_on_same_line() {
    for (space, input, expected) in [
        (Tristate::False, "const a = `\n    ${ 1 }${ 1 }\n`;", "const a = `\n    ${1}${1}\n`;"),
        (Tristate::True, "const a = `\n    ${1}${1}\n`;", "const a = `\n    ${ 1 }${ 1 }\n`;"),
    ] {
        let mut options = lsutil::get_default_format_code_settings();
        options.insert_space_after_opening_and_before_closing_template_string_braces = space;
        let ctx = with_format_code_settings(&FormatContext::default(), options, "\n");
        let source_file = parse_for_test("/test.ts", input, ScriptKind::TS);
        let actual = apply_bulk_edits(input, &format_document(&ctx, source_file));
        assert_eq!(actual, expected);
    }
}

// The source-33 Fourslash fixture changes the JSX attribute spacing for an
// unterminated string; keep that exact formatting result in the Rust suite.
#[test]
fn test_format_jsx_attribute_with_unterminated_string() {
    let input = "const x = <HangupButton customClass = 'ha\n";
    let ctx = with_format_code_settings(
        &FormatContext::default(),
        lsutil::get_default_format_code_settings(),
        "\n",
    );
    let source_file = parse_for_test("/a.tsx", input, ScriptKind::TSX);
    let actual = apply_bulk_edits(input, &format_document(&ctx, source_file));
    assert_eq!(actual, "const x = <HangupButton customClass='ha\n");
}
