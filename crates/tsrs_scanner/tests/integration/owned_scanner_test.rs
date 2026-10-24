#![forbid(unsafe_code)]

use std::borrow::Cow;

use tsrs_ast::Kind;
use tsrs_core::{LanguageVariant, TextRange};

use tsrs_scanner::{OwnedScanner, Scanner};

#[test]
fn comment_ranges_borrow_temporary_source_and_preserve_newline_boundaries() {
    let source = String::from("#!/bin/tsrs\n/** doc */\r\n// line\nvalue /* tail */\n// next");
    let comments: Vec<_> = tsrs_scanner::get_leading_comment_ranges(&source, 0).collect();
    assert_eq!(comments.len(), 2);
    assert_eq!(&source[comments[0].pos() as usize..comments[0].end() as usize], "/** doc */");
    assert_eq!(&source[comments[1].pos() as usize..comments[1].end() as usize], "// line");
    assert!(comments.iter().all(|comment| comment.has_trailing_new_line));
    let end = source.find("value").unwrap() as u32 + 5;
    let trailing: Vec<_> = tsrs_scanner::get_trailing_comment_ranges(&source, end).collect();
    assert_eq!(trailing.len(), 1);
    assert_eq!(&source[trailing[0].pos() as usize..trailing[0].end() as usize], "/* tail */");
}

#[test]
fn owned_source_slices_and_cooked_values_survive_scanning_and_rewind() {
    let source = String::from("name 'escaped\\nvalue' 1_000 0xA_B 0b101n");
    let mut scanner = OwnedScanner::new();
    scanner.set_text(&source);
    assert_eq!(scanner.scan(), Kind::Identifier);
    assert!(matches!(scanner.token_value(), Cow::Borrowed("name")));
    assert_eq!(scanner.token_value_source_range(), Some(TextRange::new(0, 4)));
    assert_eq!(scanner.scan(), Kind::StringLiteral);
    assert!(matches!(scanner.token_value(), Cow::Owned(_)));
    assert_eq!(scanner.token_value_ref(), "escaped\nvalue");
    assert_eq!(scanner.token_value_source_range(), None);
    let saved = scanner.mark();
    let retained = scanner.token_value();
    for (kind, value) in
        [(Kind::NumericLiteral, "1000"), (Kind::NumericLiteral, "171"), (Kind::BigIntLiteral, "5n")]
    {
        assert_eq!(scanner.scan(), kind);
        assert_eq!(scanner.token_value_ref(), value);
    }
    scanner.rewind(saved);
    assert_eq!(scanner.token_value_ref(), "escaped\nvalue");
    assert_eq!(scanner.scan(), Kind::NumericLiteral);
    assert_eq!(scanner.token_value_ref(), "1000");
    drop(scanner);
    assert_eq!(retained, "escaped\nvalue");
}

#[test]
fn owned_token_handoff_moves_cooked_storage_and_keeps_prior_snapshots_independent() {
    let source = String::from("name 'escaped\\nvalue' tail");
    let mut scanner = OwnedScanner::new();
    scanner.set_text(&source);
    assert_eq!(scanner.scan(), Kind::Identifier);
    let borrowed = scanner.take_token_value();
    assert!(matches!(borrowed, Cow::Borrowed("name")));
    assert_eq!(scanner.scan(), Kind::StringLiteral);
    let allocation = scanner.token_value_ref().as_ptr();
    let saved = scanner.mark();
    let range = scanner.token_range();
    let flags = scanner.token_flags();
    let moved = scanner.take_token_value();
    assert!(matches!(moved, Cow::Owned(_)));
    assert_eq!(moved.as_ptr(), allocation, "the cooked string is moved, not cloned");
    assert_eq!(scanner.token_value_ref(), "");
    assert_eq!(scanner.token(), Kind::StringLiteral);
    assert_eq!(scanner.token_range(), range);
    assert_eq!(scanner.token_flags(), flags);
    assert_eq!(scanner.scan(), Kind::Identifier);
    assert_eq!(scanner.token_value_ref(), "tail");
    scanner.rewind(saved);
    assert_eq!(scanner.token_value_ref(), "escaped\nvalue");
    assert_eq!(scanner.scan(), Kind::Identifier);
    drop(scanner);
    assert_eq!(moved, "escaped\nvalue");
    assert_eq!(borrowed, "name");
}

#[test]
fn owned_snapshots_restore_cooked_text_and_directives_across_temporary_inputs() {
    let source = String::from("// @ts-ignore\n'first\\n' /* @ts-expect-error */ next");
    let temporary = String::from("// @ts-expect-error\n'second\\t'");
    let mut scanner = OwnedScanner::new();
    scanner.set_text(&source);
    assert_eq!(scanner.scan(), Kind::StringLiteral);
    assert_eq!(scanner.comment_directives().len(), 1);
    let saved = scanner.take_state();
    scanner.set_text(&temporary);
    assert_eq!(scanner.scan(), Kind::StringLiteral);
    assert_eq!(scanner.token_value_ref(), "second\t");
    scanner.set_text(&source);
    scanner.restore_state(saved);
    assert_eq!(scanner.token_value_ref(), "first\n");
    assert_eq!(scanner.comment_directives().len(), 1);
    assert_eq!(scanner.scan(), Kind::Identifier);
    assert_eq!(scanner.comment_directives().len(), 2);
}

macro_rules! token {
    ($scanner:expr) => {{
        let scanner = $scanner;
        (
            scanner.token(),
            scanner.token_range(),
            scanner.token_flags(),
            scanner.token_value_ref().to_owned(),
        )
    }};
}

#[test]
fn owned_jsx_and_jsdoc_rescans_match_the_existing_scanner() {
    let text = "tag-name 'a\\nb' text\n{value}";
    let source = text.to_owned();
    let mut legacy = Scanner::new();
    let mut owned = OwnedScanner::new();
    legacy.set_text(text);
    owned.set_text(&source);
    legacy.set_language_variant(LanguageVariant::JSX);
    owned.set_language_variant(LanguageVariant::JSX);
    assert_eq!(legacy.scan(), owned.scan());
    assert_eq!(legacy.scan_jsx_identifier(), owned.scan_jsx_identifier());
    assert_eq!(token!(&legacy), token!(&owned));
    assert_eq!(legacy.scan_jsx_attribute_value(), owned.scan_jsx_attribute_value());
    assert_eq!(token!(&legacy), token!(&owned));
    assert_eq!(legacy.scan_jsx_token(), owned.scan_jsx_token());
    assert_eq!(token!(&legacy), token!(&owned));

    let doc = " * {Foo\\u0062ar} `code`\r\n * @param x";
    let doc_source = doc.to_owned();
    legacy.set_text(doc);
    owned.set_text(&doc_source);
    loop {
        assert_eq!(legacy.scan_jsdoc_token(), owned.scan_jsdoc_token());
        assert_eq!(token!(&legacy), token!(&owned));
        if legacy.token() == Kind::EndOfFile {
            break;
        }
    }
}

#[test]
fn owned_regexp_names_quantifiers_and_errors_match_the_existing_scanner() {
    for text in [
        r"/(?<\u0061>x)\k<a>/u",
        r"/(?:(?<a>x)|(?<a>y))\k<aa>/u",
        r"/a{999999999999999999999999,1}/u",
        r"/[\q{ab|cd}&&\p{RGI_Emoji}]/v",
        r"/(?<\uD801\uDC00>x)\k<\uD801\uDC00>/u",
    ] {
        let source = text.to_owned();
        let mut legacy = Scanner::new();
        let mut owned = OwnedScanner::new();
        legacy.set_text(text);
        owned.set_text(&source);
        legacy.set_on_error(true);
        owned.set_on_error(true);
        assert_eq!(legacy.scan(), owned.scan());
        assert_eq!(legacy.re_scan_slash_token(true), owned.re_scan_slash_token(true));
        assert_eq!(token!(&legacy), token!(&owned));
        let errors = |scanner_errors: Vec<tsrs_scanner::ScanError>| {
            scanner_errors
                .into_iter()
                .map(|error| (error.message.code(), error.start, error.length, error.args))
                .collect::<Vec<_>>()
        };
        assert_eq!(errors(legacy.take_errors()), errors(owned.take_errors()));
    }
}
