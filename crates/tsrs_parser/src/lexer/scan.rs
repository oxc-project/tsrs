//! The TypeScript-compatible scanner behind every token `oxc_lexer` does not supply: trivia that the parser must
//! see (comments, conflict markers), tokens whose value, flags or diagnostics need TypeScript's rules, and every
//! rescan.

use tsrs_ast::{CommentDirective, CommentDirectiveKind, Kind, TokenFlags};
use tsrs_core::{ArenaStringBuilder, LanguageVariant, TextRange, stringutil};
use tsrs_diagnostics as diagnostics;
use tsrs_scanner::{get_identifier_token, is_identifier_part};

use super::trivia::{
    RUNE_ERROR, decode_last_rune, decode_rune, is_conflict_marker_trivia, scan_conflict_marker_trivia,
};
use super::{IdentifierVariant, JsxWhitespaceState, Lexer};

const RUNE_SELF: u8 = 0x80;

impl Lexer {
    /// Scans one token at `self.state.pos` (not whitespace), or skips one piece of trivia and returns `None`.
    pub(super) fn scan_compat(&mut self, byte: u8) -> Option<Kind> {
        let c1 = self.char_at(1);
        let c2 = self.char_at(2);
        let (token, len) = match byte {
            b'!' => match (c1, c2) {
                (Some(b'='), Some(b'=')) => (Kind::ExclamationEqualsEqualsToken, 3),
                (Some(b'='), _) => (Kind::ExclamationEqualsToken, 2),
                _ => (Kind::ExclamationToken, 1),
            },
            b'"' | b'\'' => {
                self.state.token_value = self.scan_string(false);
                return Some(Kind::StringLiteral);
            }
            b'`' => return Some(self.scan_template_and_set_token_value(false)),
            b'%' => match c1 {
                Some(b'=') => (Kind::PercentEqualsToken, 2),
                _ => (Kind::PercentToken, 1),
            },
            b'&' => match (c1, c2) {
                (Some(b'&'), Some(b'=')) => (Kind::AmpersandAmpersandEqualsToken, 3),
                (Some(b'&'), _) => (Kind::AmpersandAmpersandToken, 2),
                (Some(b'='), _) => (Kind::AmpersandEqualsToken, 2),
                _ => (Kind::AmpersandToken, 1),
            },
            b'(' => (Kind::OpenParenToken, 1),
            b')' => (Kind::CloseParenToken, 1),
            b'*' => match (c1, c2) {
                (Some(b'='), _) => (Kind::AsteriskEqualsToken, 2),
                (Some(b'*'), Some(b'=')) => (Kind::AsteriskAsteriskEqualsToken, 3),
                (Some(b'*'), _) => (Kind::AsteriskAsteriskToken, 2),
                _ => (Kind::AsteriskToken, 1),
            },
            b'+' => match c1 {
                Some(b'=') => (Kind::PlusEqualsToken, 2),
                Some(b'+') => (Kind::PlusPlusToken, 2),
                _ => (Kind::PlusToken, 1),
            },
            b',' => (Kind::CommaToken, 1),
            b'-' => match c1 {
                Some(b'=') => (Kind::MinusEqualsToken, 2),
                Some(b'-') => (Kind::MinusMinusToken, 2),
                _ => (Kind::MinusToken, 1),
            },
            b'.' => match (c1, c2) {
                (Some(b'0'..=b'9'), _) => return Some(self.scan_number()),
                (Some(b'.'), Some(b'.')) => (Kind::DotDotDotToken, 3),
                _ => (Kind::DotToken, 1),
            },
            b'/' => return self.scan_slash_or_comment(),
            b'0'..=b'9' => return Some(self.scan_numeric_literal()),
            b':' => (Kind::ColonToken, 1),
            b';' => (Kind::SemicolonToken, 1),
            b'<' => {
                if c1 == Some(b'<') && is_conflict_marker_trivia(self.text, self.state.pos) {
                    return self.skip_conflict_marker();
                }
                match (c1, c2) {
                    (Some(b'<'), Some(b'=')) => (Kind::LessThanLessThanEqualsToken, 3),
                    (Some(b'<'), _) => (Kind::LessThanLessThanToken, 2),
                    (Some(b'='), _) => (Kind::LessThanEqualsToken, 2),
                    (Some(b'/'), next)
                        if self.language_variant == LanguageVariant::JSX && next != Some(b'*') =>
                    {
                        (Kind::LessThanSlashToken, 2)
                    }
                    _ => (Kind::LessThanToken, 1),
                }
            }
            b'=' => {
                if c1 == Some(b'=') && is_conflict_marker_trivia(self.text, self.state.pos) {
                    return self.skip_conflict_marker();
                }
                match (c1, c2) {
                    (Some(b'='), Some(b'=')) => (Kind::EqualsEqualsEqualsToken, 3),
                    (Some(b'='), _) => (Kind::EqualsEqualsToken, 2),
                    (Some(b'>'), _) => (Kind::EqualsGreaterThanToken, 2),
                    _ => (Kind::EqualsToken, 1),
                }
            }
            b'>' => {
                if c1 == Some(b'>') && is_conflict_marker_trivia(self.text, self.state.pos) {
                    return self.skip_conflict_marker();
                }
                (Kind::GreaterThanToken, 1)
            }
            b'?' => match (c1, c2) {
                (Some(b'.'), next) if !next.is_some_and(|ch| ch.is_ascii_digit()) => {
                    (Kind::QuestionDotToken, 2)
                }
                (Some(b'?'), Some(b'=')) => (Kind::QuestionQuestionEqualsToken, 3),
                (Some(b'?'), _) => (Kind::QuestionQuestionToken, 2),
                _ => (Kind::QuestionToken, 1),
            },
            b'[' => (Kind::OpenBracketToken, 1),
            b']' => (Kind::CloseBracketToken, 1),
            b'^' => match c1 {
                Some(b'=') => (Kind::CaretEqualsToken, 2),
                _ => (Kind::CaretToken, 1),
            },
            b'{' => (Kind::OpenBraceToken, 1),
            b'|' => {
                if c1 == Some(b'|') && is_conflict_marker_trivia(self.text, self.state.pos) {
                    return self.skip_conflict_marker();
                }
                match (c1, c2) {
                    (Some(b'|'), Some(b'=')) => (Kind::BarBarEqualsToken, 3),
                    (Some(b'|'), _) => (Kind::BarBarToken, 2),
                    (Some(b'='), _) => (Kind::BarEqualsToken, 2),
                    _ => (Kind::BarToken, 1),
                }
            }
            b'}' => (Kind::CloseBraceToken, 1),
            b'~' => (Kind::TildeToken, 1),
            b'@' => (Kind::AtToken, 1),
            b'#' => return self.scan_hash(),
            b'\\' => return Some(self.scan_identifier_or_invalid()),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => return Some(self.scan_ascii_identifier()),
            _ => return self.scan_other(),
        };
        self.state.pos += len;
        Some(token)
    }

    fn skip_conflict_marker(&mut self) -> Option<Kind> {
        let text = self.text;
        self.state.pos = scan_conflict_marker_trivia(
            text,
            self.state.pos,
            Some(&mut |diag, pos, length| self.error_at(diag, pos, length, &[])),
        );
        None
    }

    fn scan_slash_or_comment(&mut self) -> Option<Kind> {
        // Single-line comment
        if self.char_at(1) == Some(b'/') {
            self.state.pos += 2;
            loop {
                self.scan_ascii_while(|b| b != b'\n' && b != b'\r');
                let (ch1, size) = self.char_and_size();
                if size == 0 || stringutil::is_line_break(ch1) {
                    break;
                }
                self.state.pos += size;
            }
            self.process_comment_directive(self.state.token_start, self.state.pos, false);
            return None;
        }
        // Multi-line comment
        if self.char_at(1) == Some(b'*') {
            self.state.pos += 2;
            let is_jsdoc = self.char() == Some(b'*') && self.char_at(1) != Some(b'/');

            let mut comment_closed = false;
            let mut last_line_start = self.state.token_start;
            loop {
                self.scan_ascii_while(|b| b != b'*' && b != b'\n' && b != b'\r');
                let (ch1, size) = self.char_and_size();
                if size == 0 {
                    break;
                }
                if ch1 == u32::from(b'*') && self.char_at(1) == Some(b'/') {
                    self.state.pos += 2;
                    comment_closed = true;
                    break;
                }
                self.state.pos += size;
                if stringutil::is_line_break(ch1) {
                    self.state.token_flags |= TokenFlags::PrecedingLineBreak;
                    (comment_closed, last_line_start) = self.scan_multi_line_comment_tail();
                    break;
                }
            }

            if is_jsdoc {
                self.state.token_flags |= TokenFlags::PrecedingJSDocComment;
                self.scan_jsdoc_comment_for_tags(self.slice(self.state.token_start, self.state.pos));
            }
            self.process_comment_directive(last_line_start, self.state.pos, true);
            if !comment_closed {
                self.error(&diagnostics::ASTERISK_SLASH_EXPECTED);
            }
            return None;
        }
        if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            return Some(Kind::SlashEqualsToken);
        }
        self.state.pos += 1;
        Some(Kind::SlashToken)
    }

    // After the first line break, only the closing delimiter and final line start remain relevant.
    // Keep the final line start for comment directives; CRLF ends after LF, like the scalar loop.
    fn scan_multi_line_comment_tail(&mut self) -> (bool, u32) {
        let start = self.state.pos as usize;
        let remaining = &self.text.as_bytes()[start..self.end as usize];
        let closing = memchr::memmem::find(remaining, b"*/");
        let comment_closed = closing.is_some();
        let body = &remaining[..closing.unwrap_or(remaining.len())];
        let mut search_end = body.len();
        let mut last_line_start = start;
        while let Some(i) = memchr::memrchr3(b'\r', b'\n', 0xE2, &body[..search_end]) {
            if body[i] != 0xE2 {
                last_line_start = start + i + 1;
                break;
            }
            if matches!(body.get(i..i + 3), Some([0xE2, 0x80, 0xA8 | 0xA9])) {
                last_line_start = start + i + 3;
                break;
            }
            search_end = i;
        }
        self.state.pos = (start + body.len()) as u32;
        if comment_closed {
            self.state.pos += 2;
        }
        (comment_closed, last_line_start as u32)
    }

    fn scan_jsdoc_comment_for_tags(&mut self, comment_text: &str) {
        let mut comment_text = comment_text.as_bytes();
        loop {
            let Some(i) = memchr::memchr(b'@', comment_text) else {
                return;
            };
            comment_text = &comment_text[i + 1..];
            if !self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithDeprecated)
                && has_jsdoc_tag(comment_text, &["deprecated"])
            {
                self.state.token_flags |= TokenFlags::PrecedingJSDocWithDeprecated;
            }
            if !self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithSeeOrLink)
                && has_jsdoc_tag(comment_text, &["see", "link", "linkcode", "linkplain"])
            {
                self.state.token_flags |= TokenFlags::PrecedingJSDocWithSeeOrLink;
            }
            if self.state.token_flags.contains(
                TokenFlags::PrecedingJSDocWithDeprecated | TokenFlags::PrecedingJSDocWithSeeOrLink,
            ) {
                return;
            }
        }
    }

    fn process_comment_directive(&mut self, start: u32, end: u32, multiline: bool) {
        let text = self.text.as_bytes();
        let end_u = end as usize;
        let mut pos = start as usize;
        if multiline {
            while pos < end_u && (text[pos] == b' ' || text[pos] == b'\t') {
                pos += 1;
            }
            while pos < end_u && (text[pos] == b'/' || text[pos] == b'*') {
                pos += 1;
            }
        } else {
            pos += 2;
            while pos < end_u && text[pos] == b'/' {
                pos += 1;
            }
        }
        while pos < end_u && (text[pos] == b' ' || text[pos] == b'\t') {
            pos += 1;
        }
        if !(pos < end_u && text[pos] == b'@') {
            return;
        }
        pos += 1;
        let rest = &text[pos..];
        let kind = if rest.starts_with(b"ts-expect-error") {
            CommentDirectiveKind::ExpectError
        } else if rest.starts_with(b"ts-ignore") {
            CommentDirectiveKind::Ignore
        } else {
            return;
        };
        self.comment_directives.push(CommentDirective { loc: TextRange::new(start, end), kind });
        self.state.comment_directives_len += 1;
    }

    fn scan_numeric_literal(&mut self) -> Kind {
        if self.char() == Some(b'0') {
            match self.char_at(1) {
                Some(b'X' | b'x') => {
                    let start = self.state.pos;
                    self.state.pos += 2;
                    let mut digits = self.scan_hex_digits(1, true, true);
                    if digits.is_empty() {
                        self.error(&diagnostics::HEXADECIMAL_DIGIT_EXPECTED);
                        digits = "0";
                    }
                    let raw_text = self.slice(start, self.state.pos);
                    if raw_text.starts_with("0x") && &raw_text[2..] == digits {
                        self.state.token_value = raw_text;
                    } else {
                        let mut value = ArenaStringBuilder::with_capacity(2 + digits.len());
                        value.push_str("0x");
                        value.push_str(digits);
                        self.state.token_value = value.into_str();
                    }
                    self.state.token_flags |= TokenFlags::HexSpecifier;
                    return self.scan_big_int_suffix();
                }
                Some(b'B' | b'b') => {
                    self.state.pos += 2;
                    let (value, has_digits) = self.scan_binary_or_octal_digits(2, "0b");
                    if has_digits {
                        self.state.token_value = value;
                    } else {
                        self.error(&diagnostics::BINARY_DIGIT_EXPECTED);
                        self.state.token_value = "0b0";
                    }
                    self.state.token_flags |= TokenFlags::BinarySpecifier;
                    return self.scan_big_int_suffix();
                }
                Some(b'O' | b'o') => {
                    self.state.pos += 2;
                    let (value, has_digits) = self.scan_binary_or_octal_digits(8, "0o");
                    if has_digits {
                        self.state.token_value = value;
                    } else {
                        self.error(&diagnostics::OCTAL_DIGIT_EXPECTED);
                        self.state.token_value = "0o0";
                    }
                    self.state.token_flags |= TokenFlags::OctalSpecifier;
                    return self.scan_big_int_suffix();
                }
                _ => {}
            }
        }
        self.scan_number()
    }

    fn scan_ascii_identifier(&mut self) -> Kind {
        let start = self.state.pos;
        self.state.pos += 1;
        self.scan_ascii_identifier_parts();
        let ch = self.char();
        if ch.is_none_or(|ch| ch < RUNE_SELF && ch != b'\\') {
            self.state.token_value = self.slice(start, self.state.pos);
            return get_identifier_token(self.state.token_value);
        }
        // The generic scanner handles a Unicode continuation or escape from the
        // original start, including malformed escapes and diagnostic positions.
        self.state.pos = start;
        self.scan_identifier_or_invalid()
    }

    #[cold]
    fn scan_identifier_or_invalid(&mut self) -> Kind {
        if self.scan_identifier(0, IdentifierVariant::Standard) {
            get_identifier_token(self.state.token_value)
        } else {
            self.scan_invalid_character();
            Kind::Unknown
        }
    }

    fn scan_hash(&mut self) -> Option<Kind> {
        if self.char_at(1) == Some(b'!') {
            if self.state.pos == 0 {
                self.state.pos += 2;
                let (mut ch, mut size) = self.char_and_size();
                while size > 0 && !stringutil::is_line_break(ch) {
                    self.state.pos += size;
                    (ch, size) = self.char_and_size();
                }
                return None;
            }
            self.error_at(
                &diagnostics::X_CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE,
                self.state.pos,
                2,
                &[],
            );
            self.state.pos += 2;
            return Some(Kind::Unknown);
        }
        if !self.scan_identifier(1, IdentifierVariant::Standard) {
            self.error_at(&diagnostics::INVALID_CHARACTER, self.state.pos - 1, 1, &[]);
            self.state.token_value = "#";
        }
        Some(Kind::PrivateIdentifier)
    }

    #[cold]
    fn scan_other(&mut self) -> Option<Kind> {
        if self.scan_identifier(0, IdentifierVariant::Standard) {
            return Some(get_identifier_token(self.state.token_value));
        }
        let (ch, size) = self.char_and_size();
        if ch == RUNE_ERROR {
            self.error_at(&diagnostics::FILE_APPEARS_TO_BE_BINARY, 0, 0, &[]);
            self.state.pos = self.text.len() as u32;
            return Some(Kind::NonTextFileMarkerTrivia);
        }
        if stringutil::is_white_space_single_line(ch) {
            self.state.pos += size;
            return None;
        }
        if stringutil::is_line_break(ch) {
            self.state.token_flags |= TokenFlags::PrecedingLineBreak;
            self.state.pos += size;
            return None;
        }
        self.scan_invalid_character();
        Some(Kind::Unknown)
    }

    pub fn re_scan_less_than_token(&mut self) -> Kind {
        if self.state.token == Kind::LessThanLessThanToken {
            self.state.pos = self.state.token_start + 1;
            self.state.token = Kind::LessThanToken;
        }
        self.state.token
    }

    pub fn re_scan_greater_than_token(&mut self) -> Kind {
        if self.state.token == Kind::GreaterThanToken {
            self.state.pos = self.state.token_start + 1;
            if self.char() == Some(b'>') {
                if self.char_at(1) == Some(b'>') {
                    if self.char_at(2) == Some(b'=') {
                        self.state.pos += 3;
                        self.state.token = Kind::GreaterThanGreaterThanGreaterThanEqualsToken;
                    } else {
                        self.state.pos += 2;
                        self.state.token = Kind::GreaterThanGreaterThanGreaterThanToken;
                    }
                } else if self.char_at(1) == Some(b'=') {
                    self.state.pos += 2;
                    self.state.token = Kind::GreaterThanGreaterThanEqualsToken;
                } else {
                    self.state.pos += 1;
                    self.state.token = Kind::GreaterThanGreaterThanToken;
                }
            } else if self.char() == Some(b'=') {
                self.state.pos += 1;
                self.state.token = Kind::GreaterThanEqualsToken;
            }
        }
        self.state.token
    }

    pub fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
        self.state.pos = self.state.token_start;
        self.state.token = match self.take_oxc_template_continuation() {
            Some(token) => token,
            None => self.scan_template_and_set_token_value(!is_tagged_template),
        };
        self.state.token
    }

    pub fn re_scan_asterisk_equals_token(&mut self) -> Kind {
        assert!(
            self.state.token == Kind::AsteriskEqualsToken,
            "'ReScanAsteriskEqualsToken' should only be called on a '*='"
        );
        self.state.pos = self.state.token_start + 1;
        self.state.token = Kind::EqualsToken;
        self.state.token
    }

    pub fn re_scan_question_token(&mut self) -> Kind {
        assert!(
            self.state.token == Kind::QuestionQuestionToken,
            "'reScanQuestionToken' should only be called on a '??'"
        );
        self.state.pos = self.state.token_start + 1;
        self.state.token = Kind::QuestionToken;
        self.state.token
    }

    /// TypeScript's `reScanSlashToken()` without error reporting: the parser never validates the regular expression
    /// body (the checker does), it only finds its end and reports an unterminated literal.
    pub fn re_scan_slash_token(&mut self) -> Kind {
        if self.state.token == Kind::SlashToken || self.state.token == Kind::SlashEqualsToken {
            let text = self.text.as_bytes();
            let end = self.end;
            let start_of_reg_exp_body = self.state.token_start + 1;
            let mut p = start_of_reg_exp_body;
            let mut in_escape = false;
            // An unescaped slash ends the literal even inside a nested character class (see TypeScript's
            // `reScanSlashToken`), so the first pass does not track nesting.
            let mut in_character_class = false;
            loop {
                if p >= end {
                    self.state.token_flags |= TokenFlags::Unterminated;
                    break;
                }
                let ch = u32::from(text[p as usize]);
                if stringutil::is_line_break(ch) {
                    self.state.token_flags |= TokenFlags::Unterminated;
                    break;
                } else if in_escape {
                    in_escape = false;
                } else if ch == u32::from(b'/') && !in_character_class {
                    break;
                } else if ch == u32::from(b'[') {
                    in_character_class = true;
                } else if ch == u32::from(b'\\') {
                    in_escape = true;
                } else if ch == u32::from(b']') {
                    in_character_class = false;
                }
                p += 1;
            }

            let end_of_reg_exp_body = p;
            if self.state.token_flags.intersects(TokenFlags::Unterminated) {
                // Search for the nearest unbalanced bracket for better recovery. Since the expression is
                // invalid anyways, we take nested square brackets into consideration for the best guess.
                p = start_of_reg_exp_body;
                in_escape = false;
                let mut character_class_depth = 0;
                let mut in_decimal_quantifier = false;
                let mut group_depth = 0;
                while p < end_of_reg_exp_body {
                    let ch = text[p as usize];
                    if in_escape {
                        in_escape = false;
                    } else if ch == b'\\' {
                        in_escape = true;
                    } else if ch == b'[' {
                        character_class_depth += 1;
                    } else if ch == b']' && character_class_depth != 0 {
                        character_class_depth -= 1;
                    } else if character_class_depth == 0 {
                        if ch == b'{' {
                            in_decimal_quantifier = true;
                        } else if ch == b'}' && in_decimal_quantifier {
                            in_decimal_quantifier = false;
                        } else if !in_decimal_quantifier {
                            if ch == b'(' {
                                group_depth += 1;
                            } else if ch == b')' && group_depth != 0 {
                                group_depth -= 1;
                            } else if ch == b')' || ch == b']' || ch == b'}' {
                                // An unbalanced bracket outside a character class ends the regex.
                                break;
                            }
                        }
                    }
                    p += 1;
                }
                // Whitespaces and semicolons at the end are not likely to be part of the regex
                while p > start_of_reg_exp_body {
                    let (ch, size) = decode_last_rune(&text[..p as usize]);
                    if stringutil::is_white_space_like(ch) || ch == u32::from(b';') {
                        p -= size as u32;
                    } else {
                        break;
                    }
                }
                self.error_at(
                    &diagnostics::UNTERMINATED_REGULAR_EXPRESSION_LITERAL,
                    self.state.token_start,
                    p - self.state.token_start,
                    &[],
                );
            } else {
                // Consume the slash character and the flags
                p += 1;
                while p < end {
                    let (ch, size) = decode_rune(&text[p as usize..]);
                    if ch == RUNE_ERROR || !is_identifier_part(ch) {
                        break;
                    }
                    p += size as u32;
                }
            }

            self.state.pos = p;
            self.state.token_value = self.slice(self.state.token_start, self.state.pos);
            self.state.token = Kind::RegularExpressionLiteral;
        }
        self.state.token
    }

    pub fn re_scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> Kind {
        self.state.pos = self.state.full_start_pos;
        self.state.token_start = self.state.full_start_pos;
        self.state.token = self.scan_jsx_token_ex(allow_multiline_jsx_text);
        self.state.token
    }

    pub fn scan_jsx_token(&mut self) -> Kind {
        self.scan_jsx_token_ex(true /*allowMultilineJsxText*/)
    }

    fn scan_jsx_token_ex(&mut self, allow_multiline_jsx_text: bool) -> Kind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_start = self.state.pos;
        let Some(ch) = self.char() else {
            self.state.token = Kind::EndOfFile;
            return self.state.token;
        };
        if ch == b'<' {
            if self.char_at(1) == Some(b'/') {
                self.state.pos += 2;
                self.state.token = Kind::LessThanSlashToken;
            } else {
                self.state.pos += 1;
                self.state.token = Kind::LessThanToken;
            }
        } else if ch == b'{' {
            self.state.pos += 1;
            self.state.token = Kind::OpenBraceToken;
        } else {
            // Leading means only whitespace so far on the first line: a following line break drops it.
            let mut whitespace_state = JsxWhitespaceState::Leading;
            loop {
                let (ch, size) = self.char_and_size();
                if size == 0 || ch == u32::from(b'{') {
                    break;
                }
                if ch == u32::from(b'<') {
                    if is_conflict_marker_trivia(self.text, self.state.pos) {
                        let text = self.text;
                        self.state.pos = scan_conflict_marker_trivia(
                            text,
                            self.state.pos,
                            Some(&mut |diag, pos, length| self.error_at(diag, pos, length, &[])),
                        );
                        self.state.token = Kind::ConflictMarkerTrivia;
                        return self.state.token;
                    }
                    break;
                }
                if ch == u32::from(b'>') {
                    self.error_at(
                        &diagnostics::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                        self.state.pos,
                        1,
                        &[],
                    );
                } else if ch == u32::from(b'}') {
                    self.error_at(
                        &diagnostics::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                        self.state.pos,
                        1,
                        &[],
                    );
                }
                if stringutil::is_line_break(ch) && whitespace_state == JsxWhitespaceState::Leading
                {
                    whitespace_state = JsxWhitespaceState::AllWhitespace;
                } else if !allow_multiline_jsx_text
                    && stringutil::is_line_break(ch)
                    && whitespace_state == JsxWhitespaceState::SeenNonWhitespace
                {
                    // Stop JsxText on each line during formatting, so the formatter can indent each line.
                    break;
                } else if !stringutil::is_white_space_like(ch) {
                    whitespace_state = JsxWhitespaceState::SeenNonWhitespace;
                }
                self.state.pos += size;
            }
            self.state.token_value = self.slice(self.state.full_start_pos, self.state.pos);
            self.state.token = Kind::JsxText;
            if whitespace_state == JsxWhitespaceState::AllWhitespace {
                self.state.token = Kind::JsxTextAllWhiteSpaces;
            }
        }
        self.state.token
    }

    /// Scans a JSX identifier; these differ from normal identifiers in that they allow dashes. This extends the
    /// current identifier or keyword token in place.
    pub fn scan_jsx_identifier(&mut self) -> Kind {
        if self.state.token >= Kind::Identifier {
            let parts = self.scan_identifier_parts(IdentifierVariant::Jsx);
            self.state.token_value = self.concat_value(self.state.token_value, parts);
            self.state.token = get_identifier_token(self.state.token_value);
        }
        self.state.token
    }

    pub fn scan_jsx_attribute_value(&mut self) -> Kind {
        self.state.full_start_pos = self.state.pos;
        // Skip whitespace between '=' and the value so tokenStart lands on the opening quote, not on trivia.
        let (mut ch, mut size) = self.char_and_size();
        while size > 0 && stringutil::is_white_space_like(ch) {
            self.state.pos += size;
            (ch, size) = self.char_and_size();
        }
        self.state.token_start = self.state.pos;
        match self.char() {
            Some(b'"' | b'\'') => {
                self.state.token_value = self.scan_string(true /*jsxAttributeString*/);
                self.state.token = Kind::StringLiteral;
                self.state.token
            }
            // If this scans anything other than `{`, it's a parse error.
            _ => self.scan(),
        }
    }
}

// hasJSDocTag reports whether text starts with one of the given tag names followed
// by a valid JSDoc tag terminator (whitespace, '}', '*', or end-of-string).
fn has_jsdoc_tag(text: &[u8], tags: &[&str]) -> bool {
    tags.iter().any(|tag| {
        let tag = tag.as_bytes();
        if !text.starts_with(tag) {
            return false;
        }
        if text.len() == tag.len() {
            return true;
        }
        let ch = text[tag.len()];
        ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' || ch == b'}' || ch == b'*'
    })
}
