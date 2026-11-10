//! Identifiers, string and template literals, escape sequences and numeric literals, scanned with TypeScript's
//! rules for values, flags and diagnostics.

use std::borrow::Cow;
use std::fmt::Write as _;

use tsrs_ast::{Kind, TokenFlags};
use tsrs_core::{
    ArenaStringBuilder, LanguageVariant, alloc_str, jsnum,
    stringutil::{self, Rune},
};
use tsrs_diagnostics as diagnostics;
use tsrs_scanner::{is_identifier_part, is_identifier_start};

use super::trivia::{
    decode_rune,
    STRING_LITERAL_END, is_identifier_part_ex, parse_int_32, parse_int_64, rune_string, rune_to_char,
};
use super::{EscapeSequenceScanningFlags, IdentifierVariant, Lexer};

const RUNE_SELF: Rune = 0x80;

impl Lexer {
    pub(super) fn scan_identifier(
        &mut self,
        prefix_length: u32,
        variant: IdentifierVariant,
    ) -> bool {
        let start = self.state.pos;
        self.state.pos += prefix_length;
        let identifier_start = self.state.pos;
        let ch = self.char();
        // Fast path for simple ASCII identifiers
        if variant != IdentifierVariant::Jsx
            && ch.is_some_and(|ch| {
                stringutil::is_ascii_letter(u32::from(ch)) || ch == b'_' || ch == b'$'
            })
        {
            self.state.pos += 1;
            self.scan_ascii_identifier_parts();
            let ch = self.char();
            if ch.is_none_or(|ch| ch < RUNE_SELF as u8 && ch != b'\\') {
                self.state.token_value = self.slice(start, self.state.pos);
                return true;
            }
            self.state.pos = identifier_start;
        }
        let (mut ch, mut size) = self.char_and_size();
        if is_identifier_start(ch) {
            let language_variant = if variant == IdentifierVariant::Jsx {
                LanguageVariant::JSX
            } else {
                LanguageVariant::Standard
            };
            loop {
                self.state.pos += size;
                (ch, size) = self.char_and_size();
                if !is_identifier_part_ex(ch, language_variant) {
                    break;
                }
            }
            self.state.token_value = self.slice(start, self.state.pos);
            if ch == '\\' as Rune {
                let parts = self.scan_identifier_parts(variant);
                self.state.token_value = self.concat_value(self.state.token_value, parts);
            }
            return true;
        }
        if ch == '\\' as Rune
            && let Some(escaped) = self.scan_identifier_escape(is_identifier_start)
        {
            let mut value =
                ArenaStringBuilder::with_capacity((self.state.pos - start) as usize * 2);
            value.push_str(self.slice(start, identifier_start));
            value.push(rune_to_char(escaped));
            value.push_str(self.scan_identifier_parts(variant));
            self.state.token_value = value.into_str();
            return true;
        }
        false
    }

    pub(super) fn scan_identifier_parts(&mut self, variant: IdentifierVariant) -> &'static str {
        let mut sb: Option<ArenaStringBuilder> = None;
        let mut start = self.state.pos;
        let language_variant = if variant == IdentifierVariant::Jsx {
            LanguageVariant::JSX
        } else {
            LanguageVariant::Standard
        };
        loop {
            let (ch, size) = self.char_and_size();
            if is_identifier_part_ex(ch, language_variant) {
                self.state.pos += size;
                continue;
            }
            if ch == '\\' as Rune {
                let escape_start = self.state.pos;
                if let Some(escaped) =
                    self.scan_identifier_escape(|ch| is_identifier_part_ex(ch, language_variant))
                {
                    let so_far = (escape_start - start) as usize;
                    let sb = sb.get_or_insert_with(|| {
                        ArenaStringBuilder::with_capacity((so_far * 2).max(16))
                    });
                    sb.push_str(self.slice(start, escape_start));
                    sb.push(rune_to_char(escaped));
                    start = self.state.pos;
                    continue;
                }
            }
            break;
        }
        match sb {
            None => self.slice(start, self.state.pos),
            Some(mut sb) => {
                sb.push_str(self.slice(start, self.state.pos));
                sb.into_str()
            }
        }
    }

    fn scan_identifier_escape(&mut self, is_valid: impl Fn(u32) -> bool) -> Option<u32> {
        let escaped = self.peek_unicode_escape()?;
        if is_valid(escaped) {
            return self.scan_unicode_escape(true);
        }
        None
    }

    pub(super) fn scan_string(&mut self, jsx_attribute_string: bool) -> &'static str {
        let quote = self.char().expect("scan_string starts at a quote");
        if quote == b'\'' {
            self.state.token_flags |= TokenFlags::SingleQuote;
        }
        self.state.pos += 1;
        let start = self.state.pos;
        let table = &STRING_LITERAL_END[usize::from(quote == b'"')];
        if jsx_attribute_string {
            let rest = &self.text.as_bytes()[start as usize..];
            if let Some(str_len) = memchr::memchr(quote, rest) {
                self.state.pos += str_len as u32;
            } else {
                self.state.pos = self.text.len() as u32;
            }
        } else {
            self.scan_string_text(table);
        }
        if self.char() == Some(quote) {
            let end = self.state.pos;
            self.state.pos += 1;
            return if start == end { "" } else { self.slice(start, end) };
        }
        self.scan_string_slow(quote, start, table)
    }

    #[inline]
    fn scan_string_text(&mut self, table: &[bool; 256]) {
        let text = &self.text.as_bytes()[self.state.pos as usize..];
        let (batches, tail) = text.as_chunks::<32>();
        let mut scanned = 0;
        for batch in batches {
            for (i, &byte) in batch.iter().enumerate() {
                if table[byte as usize] {
                    self.state.pos += (scanned + i) as u32;
                    return;
                }
            }
            scanned += batch.len();
        }
        for &byte in tail {
            if table[byte as usize] {
                break;
            }
            scanned += 1;
        }
        self.state.pos += scanned as u32;
    }

    #[cold]
    #[inline(never)]
    fn scan_string_slow(&mut self, quote: u8, mut start: u32, table: &[bool; 256]) -> &'static str {
        // Keep the original first-quote capacity estimate on the allocating path.
        let rest = &self.text.as_bytes()[self.state.pos as usize..];
        let capacity = memchr::memchr(quote, rest)
            .map_or(16, |offset| (self.state.pos - start) as usize + offset);
        let mut sb = ArenaStringBuilder::with_capacity(capacity);
        loop {
            sb.push_str(self.slice(start, self.state.pos));
            let Some(ch) = self.char() else {
                self.state.token_flags |= TokenFlags::Unterminated;
                self.error(&diagnostics::UNTERMINATED_STRING_LITERAL);
                break;
            };
            if ch == quote {
                self.state.pos += 1;
                break;
            }
            if ch == b'\\' {
                let escaped = self.scan_escape_sequence(
                    EscapeSequenceScanningFlags::String | EscapeSequenceScanningFlags::ReportErrors,
                );
                sb.push_str(&escaped);
                start = self.state.pos;
                self.scan_string_text(table);
                continue;
            }
            debug_assert!(ch == b'\n' || ch == b'\r');
            self.state.token_flags |= TokenFlags::Unterminated;
            self.error(&diagnostics::UNTERMINATED_STRING_LITERAL);
            break;
        }
        sb.into_str()
    }

    pub(super) fn scan_template_and_set_token_value(
        &mut self,
        should_emit_invalid_escape_error: bool,
    ) -> Kind {
        let started_with_backtick = self.char() == Some(b'`');
        self.state.pos += 1;
        let mut start = self.state.pos;
        // Go collects the parts in a slice and joins them; a single untouched part is a text slice.
        let mut parts: Option<ArenaStringBuilder> = None;
        let token;
        loop {
            self.scan_ascii_while(|b| b != b'`' && b != b'$' && b != b'\\' && b != b'\r');
            let ch = self.char();
            if ch.is_none() || ch == Some(b'`') {
                let part_end = self.state.pos;
                if ch == Some(b'`') {
                    self.state.pos += 1;
                } else {
                    self.state.token_flags |= TokenFlags::Unterminated;
                    self.error(&diagnostics::UNTERMINATED_TEMPLATE_LITERAL);
                }
                self.finish_template_parts(&mut parts, start, part_end);
                token = if started_with_backtick {
                    Kind::NoSubstitutionTemplateLiteral
                } else {
                    Kind::TemplateTail
                };
                break;
            }
            if ch == Some(b'$') && self.char_at(1) == Some(b'{') {
                let part_end = self.state.pos;
                self.state.pos += 2;
                self.finish_template_parts(&mut parts, start, part_end);
                token =
                    if started_with_backtick { Kind::TemplateHead } else { Kind::TemplateMiddle };
                break;
            }
            if ch == Some(b'\\') {
                let so_far = (self.state.pos - start) as usize;
                let buf = parts
                    .get_or_insert_with(|| ArenaStringBuilder::with_capacity((so_far * 2).max(16)));
                buf.push_str(self.slice(start, self.state.pos));
                let flags = EscapeSequenceScanningFlags::String
                    | if should_emit_invalid_escape_error {
                        EscapeSequenceScanningFlags::ReportErrors
                    } else {
                        EscapeSequenceScanningFlags::empty()
                    };
                let escaped = self.scan_escape_sequence(flags);
                parts.as_mut().unwrap().push_str(&escaped);
                start = self.state.pos;
                continue;
            }
            // Speculated ECMAScript 6 Spec 11.8.6.1:
            // <CR><LF> and <CR> LineTerminatorSequences are normalized to <LF> for Template Values
            if ch == Some(b'\r') {
                let so_far = (self.state.pos - start) as usize;
                let buf = parts
                    .get_or_insert_with(|| ArenaStringBuilder::with_capacity((so_far * 2).max(16)));
                buf.push_str(self.slice(start, self.state.pos));
                self.state.pos += 1;
                if self.char() == Some(b'\n') {
                    self.state.pos += 1;
                }
                parts.as_mut().unwrap().push('\n');
                start = self.state.pos;
                continue;
            }
            self.state.pos += 1;
        }
        token
    }

    fn finish_template_parts(
        &mut self,
        parts: &mut Option<ArenaStringBuilder>,
        start: u32,
        end: u32,
    ) {
        self.state.token_value = match parts.take() {
            None => self.slice(start, end),
            Some(mut buf) => {
                buf.push_str(self.slice(start, end));
                buf.into_str()
            }
        };
    }

    fn scan_escape_sequence(
        &mut self,
        flags: EscapeSequenceScanningFlags,
    ) -> Cow<'static, str> {
        let start = self.state.pos;
        self.state.pos += 1;
        let Some(ch) = self.char() else {
            self.error(&diagnostics::UNEXPECTED_END_OF_TEXT);
            return Cow::Borrowed("");
        };
        self.state.pos += 1;
        match ch {
            0x30..=0x37 /* 0-7 */ => {
                if ch == b'0' {
                    // Although '0' preceding any digit is treated as LegacyOctalEscapeSequence,
                    // '\08' should separately be interpreted as '\0' + '8'.
                    if !self.char().is_some_and(|ch| stringutil::is_digit(u32::from(ch))) {
                        return Cow::Borrowed("\x00");
                    }
                }
                if ch <= b'3' {
                    // '\01', '\011'
                    // '\1', '\17', '\177'
                    if self.char().is_some_and(|ch| stringutil::is_octal_digit(u32::from(ch))) {
                        self.state.pos += 1;
                    }
                }
                // '\17', '\177'
                // '\4', '\47' but not '\477'
                if self.char().is_some_and(|ch| stringutil::is_octal_digit(u32::from(ch))) {
                    self.state.pos += 1;
                }
                // '\47'
                self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
                if flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                    let code = u32::from_str_radix(self.slice(start + 1, self.state.pos), 8).unwrap_or(0);
                    let arg = format!("\\x{code:02x}");
                    if flags.intersects(EscapeSequenceScanningFlags::RegularExpression)
                        && !flags.intersects(EscapeSequenceScanningFlags::AtomEscape)
                        && ch != b'0'
                    {
                        self.error_at(
                            &diagnostics::OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_IF_THIS_WAS_INTENDED_AS_AN_ESCAPE_SEQUENCE_USE_THE_SYNTAX_0_INSTEAD,
                            start,
                            self.state.pos - start,
                            &[&arg],
                        );
                    } else {
                        self.error_at(&diagnostics::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX_0, start, self.state.pos - start, &[&arg]);
                    }
                    return Cow::Owned(rune_string(code));
                }
                Cow::Borrowed(self.slice(start, self.state.pos))
            }
            0x38 | 0x39 /* 8 9 */ => {
                // the invalid '\8' and '\9'
                self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
                if flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                    if flags.intersects(EscapeSequenceScanningFlags::RegularExpression) && !flags.intersects(EscapeSequenceScanningFlags::AtomEscape) {
                        self.error_at(
                            &diagnostics::DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS,
                            start,
                            self.state.pos - start,
                            &[],
                        );
                    } else {
                        let text = self.slice(start, self.state.pos);
                        self.error_at(&diagnostics::ESCAPE_SEQUENCE_0_IS_NOT_ALLOWED, start, self.state.pos - start, &[&text]);
                    }
                    return Cow::Owned(rune_string(u32::from(ch)));
                }
                Cow::Borrowed(self.slice(start, self.state.pos))
            }
            0x62 /* b */ => Cow::Borrowed("\u{8}"),
            0x74 /* t */ => Cow::Borrowed("\t"),
            0x6E /* n */ => Cow::Borrowed("\n"),
            0x76 /* v */ => Cow::Borrowed("\u{b}"),
            0x66 /* f */ => Cow::Borrowed("\u{c}"),
            0x72 /* r */ => Cow::Borrowed("\r"),
            0x27 /* ' */ => Cow::Borrowed("'"),
            0x22 /* " */ => Cow::Borrowed("\""),
            0x75 /* u */ => {
                // '\uDDDD' and '\u{DDDDDD}'
                let extended = self.char() == Some(b'{');
                self.state.pos -= 2;
                let code_point = self.scan_unicode_escape(flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors));
                if extended {
                    if !flags.intersects(EscapeSequenceScanningFlags::AllowExtendedUnicodeEscape) {
                        self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
                        if flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                            self.error_at(
                                &diagnostics::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICODE_SETS_V_FLAG_IS_SET,
                                start,
                                self.state.pos - start,
                                &[],
                            );
                        }
                    }
                    let Some(code_point) = code_point else {
                        return Cow::Borrowed(self.slice(start, self.state.pos));
                    };
                    // In string literals, a high surrogate \u{...} followed by a low
                    // surrogate escape forms a single code point, exactly as adjacent
                    // UTF-16 code units would in a JavaScript string.
                    if !flags.intersects(EscapeSequenceScanningFlags::RegularExpression) && stringutil::is_high_surrogate(code_point)
                        && let Some(combined) = self.scan_low_surrogate_escape(code_point) {
                            return Cow::Owned(rune_string(combined));
                        }
                    return Cow::Owned(stringutil::encode_js_string_rune(code_point));
                }
                let Some(code_point) = code_point else {
                    return Cow::Borrowed(self.slice(start, self.state.pos));
                };
                if stringutil::is_high_surrogate(code_point) {
                    if !flags.intersects(EscapeSequenceScanningFlags::RegularExpression) {
                        // Combine \uHigh followed by any low surrogate escape (\uLow or
                        // \u{Low}) into a single code point in string literals, matching
                        // how adjacent UTF-16 code units pair in a JavaScript string.
                        if let Some(combined) = self.scan_low_surrogate_escape(code_point) {
                            return Cow::Owned(rune_string(combined));
                        }
                    } else if flags.intersects(EscapeSequenceScanningFlags::AnyUnicodeMode)
                        && self.char() == Some(b'\\')
                        && self.char_at(1) == Some(b'u')
                        && self.char_at(2) != Some(b'{')
                    {
                        // In regex AnyUnicodeMode, combine \uHigh\uLow so scanClassRanges
                        // can compare the pair numerically. In non-unicode regex mode they
                        // are separate atoms, and extended \u{...} escapes never combine.
                        let saved_pos = self.state.pos;
                        let next_code_point = self.scan_unicode_escape(flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors));
                        if let Some(next_code_point) = next_code_point.filter(|&ch| stringutil::is_low_surrogate(ch)) {
                            return Cow::Owned(rune_string(stringutil::surrogate_pair_to_code_point(code_point, next_code_point)));
                        }
                        self.state.pos = saved_pos;
                    }
                }
                // Lone surrogate: encode as CESU-8 so it survives losslessly. In a
                // non-unicode regex this also lets scanClassRanges compare it numerically.
                Cow::Owned(stringutil::encode_js_string_rune(code_point))
            }
            0x78 /* x */ => {
                // '\xDD'
                while self.state.pos < start + 4 {
                    if !self.char().is_some_and(|ch| stringutil::is_hex_digit(u32::from(ch))) {
                        self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
                        if flags.intersects(EscapeSequenceScanningFlags::ReportInvalidEscapeErrors) {
                            self.error(&diagnostics::HEXADECIMAL_DIGIT_EXPECTED);
                        }
                        return Cow::Borrowed(self.slice(start, self.state.pos));
                    }
                    self.state.pos += 1;
                }
                self.state.token_flags |= TokenFlags::HexEscape;
                let escaped_value = u32::from_str_radix(self.slice(start + 2, self.state.pos), 16).unwrap_or(0);
                Cow::Owned(rune_string(escaped_value))
            }
            // when encountering a LineContinuation (i.e. a backslash and a line terminator sequence),
            // the line terminator is interpreted to be "the empty code unit sequence".
            0x0D /* \r */ => {
                if self.char() == Some(b'\n') {
                    self.state.pos += 1;
                }
                Cow::Borrowed("")
            }
            0x0A /* \n */ => Cow::Borrowed(""),
            _ => {
                let mut ch = u32::from(ch);
                // ch was read as a single byte; for multi-byte UTF-8 characters,
                // we need to decode the full rune and advance past all its bytes.
                if ch >= RUNE_SELF {
                    self.state.pos -= 1; // back up past the single-byte advance
                    let (r, size) = decode_rune(&self.text.as_bytes()[self.state.pos as usize..]);
                    ch = r;
                    self.state.pos += size as u32;
                }
                // LineContinuation: a backslash followed by a line terminator is "the empty code unit sequence".
                if ch == 0x2028 || ch == 0x2029 {
                    return Cow::Borrowed("");
                }
                if flags.intersects(EscapeSequenceScanningFlags::AnyUnicodeMode)
                    || flags.intersects(EscapeSequenceScanningFlags::RegularExpression)
                        && !flags.intersects(EscapeSequenceScanningFlags::AnnexB)
                        && is_identifier_part(ch)
                {
                    self.error_at(&diagnostics::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION, start, self.state.pos - start, &[]);
                }
                if ch < RUNE_SELF {
                    // Single ASCII characters are always slices of the text.
                    return Cow::Borrowed(self.slice(self.state.pos - 1, self.state.pos));
                }
                Cow::Owned(rune_string(ch))
            }
        }
    }

    // Known to be at \u
    fn scan_unicode_escape(&mut self, should_emit_invalid_escape_error: bool) -> Option<u32> {
        self.state.pos += 2;
        let start = self.state.pos;
        let extended = self.char() == Some(b'{');

        let hex_digits = if extended {
            self.state.pos += 1;
            self.scan_hex_digits(1, true, false)
        } else {
            self.state.token_flags |= TokenFlags::UnicodeEscape;
            self.scan_hex_digits(4, false, false)
        };
        if hex_digits.is_empty() {
            self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
            if should_emit_invalid_escape_error {
                self.error(&diagnostics::HEXADECIMAL_DIGIT_EXPECTED);
            }
            return None;
        }
        let hex_value = parse_int_32(hex_digits, 16);
        if extended {
            let mut is_invalid_extended_escape = false;
            if hex_value > 0x0010_FFFF {
                if should_emit_invalid_escape_error {
                    self.error_at(
                        &diagnostics::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10_FFFF_INCLUSIVE,
                        start + 1,
                        self.state.pos - start - 1,
                        &[],
                    );
                }
                is_invalid_extended_escape = true;
            }
            if self.state.pos >= self.end {
                if should_emit_invalid_escape_error {
                    self.error(&diagnostics::UNEXPECTED_END_OF_TEXT);
                }
                is_invalid_extended_escape = true;
            } else if self.char() == Some(b'}') {
                self.state.pos += 1;
            } else {
                if should_emit_invalid_escape_error {
                    self.error(&diagnostics::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE);
                }
                is_invalid_extended_escape = true;
            }
            if is_invalid_extended_escape {
                self.state.token_flags |= TokenFlags::ContainsInvalidEscape;
                return None;
            }
            self.state.token_flags |= TokenFlags::ExtendedUnicodeEscape;
        }
        u32::try_from(hex_value).ok()
    }

    // scanLowSurrogateEscape attempts to consume a low-surrogate Unicode escape
    // (either '\uLow' or '\u{Low}') immediately following an already-scanned high
    // surrogate and combine them into a single supplementary code point. This
    // mirrors how adjacent UTF-16 code units form a surrogate pair in a JavaScript
    // string, regardless of which escape syntax produced each half. On success it
    // returns the combined code point; otherwise it restores the scanner
    // position and returns None.
    fn scan_low_surrogate_escape(&mut self, high: u32) -> Option<u32> {
        if self.char() != Some(b'\\') || self.char_at(1) != Some(b'u') {
            return None;
        }
        let saved_pos = self.state.pos;
        let saved_token_flags = self.state.token_flags;
        // Speculatively scan the escape with diagnostics suppressed: if it isn't a
        // low surrogate we rewind below, and the caller re-scans the same escape and
        // reports any error then, so reporting here would duplicate diagnostics.
        let low = self.scan_unicode_escape(false);
        if low.is_some_and(stringutil::is_low_surrogate) {
            return Some(stringutil::surrogate_pair_to_code_point(high, low.unwrap()) as u32);
        }
        self.state.pos = saved_pos;
        self.state.token_flags = saved_token_flags;
        None
    }

    // Current character is known to be a backslash. Check for Unicode escape of the form '\uXXXX'
    // or '\u{XXXXXX}' and return the code point if a valid Unicode escape is found.
    fn peek_unicode_escape(&mut self) -> Option<u32> {
        if self.char_at(1) == Some(b'u') {
            let save_pos = self.state.pos;
            let save_token_flags = self.state.token_flags;
            let code_point = self.scan_unicode_escape(false);
            self.state.pos = save_pos;
            self.state.token_flags = save_token_flags;
            return code_point;
        }
        None
    }

    pub(super) fn scan_number(&mut self) -> Kind {
        let mut start = self.state.pos;
        let fixed_part: Cow<'static, str>;
        if self.char() == Some(b'0') {
            self.state.pos += 1;
            if self.char() == Some(b'_') {
                self.state.token_flags |=
                    TokenFlags::ContainsSeparator | TokenFlags::ContainsInvalidSeparator;
                self.error_at(
                    &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                    self.state.pos,
                    1,
                    &[],
                );
                self.state.pos = start;
                fixed_part = self.scan_number_fragment();
            } else {
                let (digits, is_octal) = self.scan_digits();
                if digits.is_empty() {
                    fixed_part = Cow::Borrowed("0");
                } else if !is_octal {
                    self.state.token_flags |= TokenFlags::ContainsLeadingZero;
                    fixed_part = Cow::Borrowed(digits);
                } else {
                    let val = parse_int_64(digits, 8);
                    let mut value = ArenaStringBuilder::with_capacity(digits.len());
                    write!(value, "{val}").unwrap();
                    self.state.token_value = value.into_str();
                    self.state.token_flags |= TokenFlags::Octal;
                    let with_minus = self.state.token == Kind::MinusToken;
                    let literal = format!("{}0o{:o}", if with_minus { "-" } else { "" }, val);
                    if with_minus {
                        start -= 1;
                    }
                    self.error_at(
                        &diagnostics::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX_0,
                        start,
                        self.state.pos - start,
                        &[&literal],
                    );
                    return Kind::NumericLiteral;
                }
            }
        } else {
            fixed_part = self.scan_number_fragment();
        }
        let fixed_part_end = self.state.pos;
        let mut fractional_part: Cow<'static, str> = Cow::Borrowed("");
        let mut exponent_preamble = "";
        let mut exponent_part: Cow<'static, str> = Cow::Borrowed("");
        if self.char() == Some(b'.') {
            self.state.pos += 1;
            fractional_part = self.scan_number_fragment();
        }
        let mut end = self.state.pos;
        if self.char() == Some(b'E') || self.char() == Some(b'e') {
            self.state.pos += 1;
            self.state.token_flags |= TokenFlags::Scientific;
            if self.char() == Some(b'+') || self.char() == Some(b'-') {
                self.state.pos += 1;
            }
            let start_numeric_part = self.state.pos;
            exponent_part = self.scan_number_fragment();
            if exponent_part.is_empty() {
                self.error(&diagnostics::DIGIT_EXPECTED);
            } else {
                exponent_preamble = self.slice(end, start_numeric_part);
                end = self.state.pos;
            }
        }
        if self.state.token_flags.intersects(TokenFlags::ContainsSeparator) {
            let capacity = fixed_part.len()
                + usize::from(!fractional_part.is_empty())
                + fractional_part.len()
                + exponent_preamble.len()
                + exponent_part.len();
            let mut value = ArenaStringBuilder::with_capacity(capacity);
            value.push_str(&fixed_part);
            if !fractional_part.is_empty() {
                value.push('.');
                value.push_str(&fractional_part);
            }
            if !exponent_part.is_empty() {
                value.push_str(exponent_preamble);
                value.push_str(&exponent_part);
            }
            self.state.token_value = value.into_str();
        } else {
            self.state.token_value = self.slice(start, end);
        }
        if self.state.token_flags.intersects(TokenFlags::ContainsLeadingZero) {
            self.error_at(
                &diagnostics::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED,
                start,
                self.state.pos - start,
                &[],
            );
            self.state.token_value = Self::canonical_number(self.state.token_value);
            return Kind::NumericLiteral;
        }

        let result = if fixed_part_end == self.state.pos {
            self.scan_big_int_suffix()
        } else {
            self.state.token_value = Self::canonical_number(self.state.token_value);
            Kind::NumericLiteral
        };
        let (ch, _) = self.char_and_size();
        if is_identifier_start(ch) {
            let id_start = self.state.pos;
            let id = self.scan_identifier_parts(IdentifierVariant::Standard);
            if result != Kind::BigIntLiteral
                && id.len() == 1
                && self.text.as_bytes()[id_start as usize] == b'n'
            {
                if self.state.token_flags.intersects(TokenFlags::Scientific) {
                    self.error_at(
                        &diagnostics::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION,
                        start,
                        self.state.pos - start,
                        &[],
                    );
                    return result;
                }
                if fixed_part_end < id_start {
                    self.error_at(
                        &diagnostics::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER,
                        start,
                        self.state.pos - start,
                        &[],
                    );
                    return result;
                }
            }
            self.error_at(
                &diagnostics::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                id_start,
                self.state.pos - id_start,
                &[],
            );
            self.state.pos = id_start;
        }
        result
    }

    /// Go `jsnum.FromString(v).String()`, reusing `v` when it is already canonical.
    fn canonical_number(v: &'static str) -> &'static str {
        // Every decimal integer with at most 15 digits is a safe integer. Its scanner spelling is canonical when it
        // has no leading zero, so neither conversion nor a temporary formatting String is needed. Longer integers,
        // fractions and exponents still go through Number::string for ECMAScript rounding and formatting.
        let bytes = v.as_bytes();
        if bytes == b"0"
            || bytes.len() <= 15
                && matches!(bytes.first(), Some(b'1'..=b'9'))
                && bytes[1..].iter().all(u8::is_ascii_digit)
        {
            return v;
        }
        let s = tsrs_scanner::number::parse_literal(v).unwrap_or_else(|| jsnum::from_string(v)).string();
        if s == v {
            return v;
        }
        alloc_str(&s)
    }

    fn scan_number_fragment(&mut self) -> Cow<'static, str> {
        let mut start = self.state.pos;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        let mut result = String::new();
        loop {
            let before = self.state.pos;
            self.scan_ascii_while(|b| b.is_ascii_digit());
            if self.state.pos > before {
                allow_separator = true;
                is_previous_token_separator = false;
            }
            let ch = self.char();
            if ch == Some(b'_') {
                self.state.token_flags |= TokenFlags::ContainsSeparator;
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                    result.push_str(self.slice(start, self.state.pos));
                } else {
                    self.state.token_flags |= TokenFlags::ContainsInvalidSeparator;
                    if is_previous_token_separator {
                        self.error_at(
                            &diagnostics::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
                            self.state.pos,
                            1,
                            &[],
                        );
                    } else {
                        self.error_at(
                            &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                            self.state.pos,
                            1,
                            &[],
                        );
                    }
                }
                self.state.pos += 1;
                start = self.state.pos;
                continue;
            }
            break;
        }
        if is_previous_token_separator {
            self.state.token_flags |= TokenFlags::ContainsInvalidSeparator;
            self.error_at(
                &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                self.state.pos - 1,
                1,
                &[],
            );
        }
        if result.is_empty() {
            return Cow::Borrowed(self.slice(start, self.state.pos));
        }
        result.push_str(self.slice(start, self.state.pos));
        Cow::Owned(result)
    }

    fn scan_digits(&mut self) -> (&'static str, bool) {
        let start = self.state.pos;
        let mut is_octal = true;
        while self.char().is_some_and(|ch| stringutil::is_digit(u32::from(ch))) {
            if !self.char().is_some_and(|ch| stringutil::is_octal_digit(u32::from(ch))) {
                is_octal = false;
            }
            self.state.pos += 1;
        }
        (self.slice(start, self.state.pos), is_octal)
    }

    pub(super) fn scan_hex_digits(
        &mut self,
        min_count: u32,
        scan_as_many_as_possible: bool,
        can_have_separators: bool,
    ) -> &'static str {
        let mut digit_count = 0;
        let start = self.state.pos;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        while digit_count < min_count || scan_as_many_as_possible {
            let ch = self.char();
            if ch.is_some_and(|ch| stringutil::is_hex_digit(u32::from(ch))) {
                allow_separator = can_have_separators;
                is_previous_token_separator = false;
                digit_count += 1;
            } else if can_have_separators && ch == Some(b'_') {
                self.state.token_flags |= TokenFlags::ContainsSeparator;
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                } else if is_previous_token_separator {
                    self.error_at(
                        &diagnostics::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
                        self.state.pos,
                        1,
                        &[],
                    );
                } else {
                    self.error_at(
                        &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                        self.state.pos,
                        1,
                        &[],
                    );
                }
            } else {
                break;
            }
            self.state.pos += 1;
        }
        if is_previous_token_separator {
            self.error_at(
                &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                self.state.pos - 1,
                1,
                &[],
            );
        }
        if digit_count < min_count {
            return "";
        }
        let digits = self.slice(start, self.state.pos);
        let has_separator = memchr::memchr(b'_', digits.as_bytes()).is_some();
        let has_uppercase = digits.bytes().any(|b| b.is_ascii_uppercase());
        if !has_separator && !has_uppercase {
            return digits;
        }
        let mut result = ArenaStringBuilder::with_capacity(digits.len());
        for byte in digits.bytes() {
            if byte != b'_' {
                result.push(byte.to_ascii_lowercase() as char);
            }
        }
        result.into_str()
    }

    pub(super) fn scan_binary_or_octal_digits(&mut self, base: u8, prefix: &str) -> (&'static str, bool) {
        let mut sb = ArenaStringBuilder::with_capacity(16);
        sb.push_str(prefix);
        let mut has_digits = false;
        let mut allow_separator = false;
        let mut is_previous_token_separator = false;
        loop {
            let ch = self.char();
            if ch.is_some_and(|ch| ch.is_ascii_digit() && ch - b'0' < base) {
                sb.push(ch.unwrap() as char);
                has_digits = true;
                allow_separator = true;
                is_previous_token_separator = false;
            } else if ch == Some(b'_') {
                self.state.token_flags |= TokenFlags::ContainsSeparator;
                if allow_separator {
                    allow_separator = false;
                    is_previous_token_separator = true;
                } else if is_previous_token_separator {
                    self.error_at(
                        &diagnostics::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED,
                        self.state.pos,
                        1,
                        &[],
                    );
                } else {
                    self.error_at(
                        &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                        self.state.pos,
                        1,
                        &[],
                    );
                }
            } else {
                break;
            }
            self.state.pos += 1;
        }
        if is_previous_token_separator {
            self.error_at(
                &diagnostics::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE,
                self.state.pos - 1,
                1,
                &[],
            );
        }
        (sb.into_str(), has_digits)
    }

    pub(super) fn scan_big_int_suffix(&mut self) -> Kind {
        if self.char() == Some(b'n') {
            // Go: tokenValue += "n"; when the value is a text slice directly followed by the `n`
            // this is again a text slice.
            let n = self.slice(self.state.pos, self.state.pos + 1);
            self.state.token_value = self.concat_value(self.state.token_value, n);
            if self.state.token_flags.intersects(TokenFlags::BinaryOrOctalSpecifier) {
                self.state.token_value =
                    alloc_str(&(jsnum::parse_pseudo_big_int(self.state.token_value) + "n"));
            }
            self.state.pos += 1;
            return Kind::BigIntLiteral;
        }
        self.state.token_value = Self::canonical_number(self.state.token_value);
        Kind::NumericLiteral
    }

    pub(super) fn scan_invalid_character(&mut self) {
        let (_, size) = self.char_and_size();
        self.error_at(&diagnostics::INVALID_CHARACTER, self.state.pos, size, &[]);
        self.state.pos += size;
        self.state.token = Kind::Unknown;
    }
}
