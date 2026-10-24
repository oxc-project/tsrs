//! Shared scanner algorithm; tools/gen-scanner.mjs specializes the legacy methods.

#![forbid(unsafe_code)]

use self::regexp::{RegExpParser, RegularExpressionFlags, char_code_to_reg_exp_flag};
use super::*;
use crate::text::{OwnedScannerText, ScannerText, ScannerTextBuilder};

mod regexp {
    include!("regexp_impl.rs");
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScannerStateImpl<T> {
    pub(crate) pos: u32, // Current position in text (and ending position of current token)
    pub(crate) full_start_pos: u32, // Starting position of current token including preceding whitespace
    pub(crate) token_start: u32,    // Starting position of non-whitespace part of current token
    pub(crate) token: Kind,         // Kind of current token
    pub(crate) token_value: T,      // Parsed value of current token
    pub(crate) token_flags: TokenFlags, // Flags for current token
    pub(crate) comment_directives_len: usize, // Snapshot of the number of comment directives collected so far
    pub(crate) skip_jsdoc_leading_asterisks: u32, // Leading asterisks to skip when scanning types inside JSDoc. Should be 0 outside JSDoc
}

/// An owning snapshot for temporary text changes, unlike a `mark()` snapshot.
#[must_use]
pub struct SavedScannerStateImpl<T> {
    state: ScannerStateImpl<T>,
    comment_directives: Vec<CommentDirective>,
}

#[derive(Default)]
pub struct ScannerImpl<'src, S: ScannerText<'src>> {
    pub(crate) text: &'src str,
    pub(crate) end: u32,
    language_variant: LanguageVariant,
    script_target: ScriptTarget,
    on_error: bool,
    errors: Vec<ScanError>,
    skip_trivia: bool,
    pub(crate) state: ScannerStateImpl<S::Text>,
    comment_directives: Vec<CommentDirective>,

    number_cache: FxHashMap<S::Text, S::Text>,
    hex_number_cache: FxHashMap<S::Text, S::Text>,
    hex_digit_cache: FxHashMap<S::Text, S::Text>,
}

pub type OwnedScanner<'src> = ScannerImpl<'src, OwnedScannerText>;
pub type OwnedScannerState<'src> = ScannerStateImpl<Cow<'src, str>>;
pub type SavedOwnedScannerState<'src> = SavedScannerStateImpl<Cow<'src, str>>;

impl<'src> ScannerImpl<'src, OwnedScannerText> {
    /// Move the current value to its consumer, leaving the scanner's value empty.
    ///
    /// Kind, range and flags are unchanged. Previously saved snapshots retain
    /// their independent values. Source slices still borrow the scanner input;
    /// cooked strings transfer their allocation without cloning it.
    pub fn take_token_value(&mut self) -> Cow<'src, str> {
        std::mem::take(&mut self.state.token_value)
    }
}

impl<'src, S: ScannerText<'src>> ScannerImpl<'src, S> {
    const BYTE_HANDLERS: [fn(&mut Self) -> Option<Kind>; 256] = {
        let mut table = [Self::scan_byte_default as fn(&mut Self) -> Option<Kind>; 256];
        let mut byte = 0;
        while byte < table.len() {
            table[byte] = match byte as u8 {
            0x09 | 0x0B | 0x0C | 0x20 => Self::scan_byte_whitespace,
            0x0A | 0x0D => Self::scan_byte_line_break,
            0x21 /* ! */ => Self::scan_byte_exclamation,
            0x22 | 0x27 /* " ' */ => Self::scan_byte_string,
            0x60 /* ` */ => Self::scan_byte_template,
            0x25 /* % */ => Self::scan_byte_percent,
            0x26 /* & */ => Self::scan_byte_ampersand,
            0x28 /* ( */ => Self::scan_byte_open_paren,
            0x29 /* ) */ => Self::scan_byte_close_paren,
            0x2A /* * */ => Self::scan_byte_asterisk,
            0x2B /* + */ => Self::scan_byte_plus,
            0x2C /* , */ => Self::scan_byte_comma,
            0x2D /* - */ => Self::scan_byte_minus,
            0x2E /* . */ => Self::scan_byte_dot,
            0x2F /* / */ => Self::scan_byte_slash,
            0x30..=0x39 /* 0-9 */ => Self::scan_byte_number,
            0x3A /* : */ => Self::scan_byte_colon,
            0x3B /* ; */ => Self::scan_byte_semicolon,
            0x3C /* < */ => Self::scan_byte_less_than,
            0x3D /* = */ => Self::scan_byte_equals,
            0x3E /* > */ => Self::scan_byte_greater_than,
            0x3F /* ? */ => Self::scan_byte_question,
            0x5B /* [ */ => Self::scan_byte_open_bracket,
            0x5D /* ] */ => Self::scan_byte_close_bracket,
            0x5E /* ^ */ => Self::scan_byte_caret,
            0x7B /* { */ => Self::scan_byte_open_brace,
            0x7C /* | */ => Self::scan_byte_bar,
            0x7D /* } */ => Self::scan_byte_close_brace,
            0x7E /* ~ */ => Self::scan_byte_tilde,
            0x40 /* @ */ => Self::scan_byte_at,
            0x5C /* \ */ => Self::scan_byte_identifier_slow,
            0x23 /* # */ => Self::scan_byte_hash,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => Self::scan_byte_identifier,
            _ => Self::scan_byte_default,
        };
            byte += 1;
        }
        table
    };

    fn default_scanner() -> Self {
        Self { skip_trivia: true, ..Default::default() }
    }

    pub fn new() -> Self {
        Self::default_scanner()
    }

    pub fn reset(&mut self) {
        let mut number_cache = std::mem::take(&mut self.number_cache);
        let mut hex_number_cache = std::mem::take(&mut self.hex_number_cache);
        let mut hex_digit_cache = std::mem::take(&mut self.hex_digit_cache);
        number_cache.clear();
        hex_number_cache.clear();
        hex_digit_cache.clear();
        *self = Self::default_scanner();
        self.number_cache = number_cache;
        self.hex_number_cache = hex_number_cache;
        self.hex_digit_cache = hex_digit_cache;
    }

    pub fn text(&self) -> &'src str {
        self.text
    }

    /// A number cache entry outlives a rolled-back parse (notes/mem-recycle.md): pin the arena unless both strings are
    /// slices of the source text (or empty), which no rewind can discard. Lazily parsed declaration-file member lists
    /// are discarded by a rewind too (notes/mem-lazy-dts-members.md), and most numbers are source slices.
    fn pin_unless_source(&self, key: &str, value: &str) {
        S::pin_unless_source(self.text, key, value);
    }

    pub fn token(&self) -> Kind {
        self.state.token
    }

    pub fn token_flags(&self) -> TokenFlags {
        self.state.token_flags
    }

    pub fn token_full_start(&self) -> u32 {
        self.state.full_start_pos
    }

    pub fn token_start(&self) -> u32 {
        self.state.token_start
    }

    pub fn token_end(&self) -> u32 {
        self.state.pos
    }

    pub fn token_text(&self) -> &'src str {
        &self.text[self.state.token_start as usize..self.state.pos as usize]
    }

    pub fn token_value(&self) -> S::Text {
        self.state.token_value.clone()
    }

    pub fn token_value_ref(&self) -> &str {
        &self.state.token_value
    }

    /// A zero-copy AST text descriptor, only when the current value is a slice
    /// of this input. Cooked values and static recovery spellings return None.
    pub fn token_value_source_range(&self) -> Option<TextRange> {
        let value = self.token_value_ref();
        let start = (value.as_ptr() as usize).checked_sub(self.text.as_ptr() as usize)?;
        let end = start.checked_add(value.len())?;
        self.text.get(start..end)?;
        let start = u32::try_from(start).ok()?;
        let end = u32::try_from(end).ok()?;
        (end <= tsrs_core::SourcePos::MAX).then(|| TextRange::new(start, end))
    }

    pub fn token_range(&self) -> TextRange {
        TextRange::new(self.state.token_start, self.state.pos)
    }

    pub fn comment_directives(&self) -> &[CommentDirective] {
        &self.comment_directives[..self.state.comment_directives_len]
    }

    /// Saves a snapshot for speculation within the current text.
    pub fn mark(&self) -> ScannerStateImpl<S::Text> {
        S::snapshot(&self.state)
    }

    /// Restores a snapshot without changing text. Use `restore_state` after `set_text`.
    pub fn rewind(&mut self, state: ScannerStateImpl<S::Text>) {
        debug_assert!(
            state.comment_directives_len <= self.comment_directives.len(),
            "rewind requires the saved directive buffer; use take_state across text changes"
        );
        self.state = state;
        self.comment_directives.truncate(self.state.comment_directives_len);
    }

    /// Moves the token state and directive buffer out before a temporary text change.
    pub fn take_state(&mut self) -> SavedScannerStateImpl<S::Text> {
        SavedScannerStateImpl {
            state: std::mem::take(&mut self.state),
            comment_directives: std::mem::take(&mut self.comment_directives),
        }
    }

    /// Restores the owned snapshot after the caller has restored the scanner's text.
    pub fn restore_state(&mut self, saved: SavedScannerStateImpl<S::Text>) {
        self.state = saved.state;
        self.comment_directives = saved.comment_directives;
    }

    /// Go: `onError != nil`. Enables buffering of scanner diagnostics (see the module docs).
    pub fn set_on_error(&mut self, enabled: bool) {
        self.on_error = enabled;
    }

    #[inline]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Drains the buffered scanner diagnostics in report order.
    pub fn take_errors(&mut self) -> Vec<ScanError> {
        std::mem::take(&mut self.errors)
    }

    pub fn reset_pos(&mut self, pos: u32) {
        self.state.pos = pos;
        self.state.full_start_pos = pos;
        self.state.token_start = pos;
    }

    pub fn reset_token_state(&mut self, pos: u32) {
        self.reset_pos(pos);
        self.state.token = Kind::Unknown;
        self.state.token_value = "".into();
        self.state.token_flags = TokenFlags::None;
    }

    pub fn set_skip_jsdoc_leading_asterisks(&mut self, skip: bool) {
        if skip {
            self.state.skip_jsdoc_leading_asterisks += 1;
        } else {
            self.state.skip_jsdoc_leading_asterisks -= 1;
        }
    }

    pub fn set_skip_trivia(&mut self, skip: bool) {
        self.skip_trivia = skip;
    }

    pub fn has_unicode_escape(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::UnicodeEscape)
    }

    pub fn has_extended_unicode_escape(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::ExtendedUnicodeEscape)
    }

    pub fn has_preceding_line_break(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingLineBreak)
    }

    pub fn has_preceding_jsdoc_comment(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocComment)
    }

    pub fn has_preceding_jsdoc_leading_asterisks(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocLeadingAsterisks)
    }

    pub fn has_preceding_jsdoc_with_deprecated_tag(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithDeprecated)
    }

    pub fn has_preceding_jsdoc_with_see_or_link(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithSeeOrLink)
    }

    // scanJSDocCommentForTags scans a JSDoc comment for @deprecated, @see, and @link tags,
    // setting the appropriate token flags. Called during scanning when a JSDoc comment is detected.
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

    pub fn set_text(&mut self, text: &'src str) {
        self.text = text;
        self.end = text.len() as u32;
        self.state = ScannerStateImpl::default();
        self.comment_directives.clear();
    }

    pub fn set_language_variant(&mut self, language_variant: LanguageVariant) {
        self.language_variant = language_variant;
    }

    pub fn set_script_target(&mut self, script_target: ScriptTarget) {
        self.script_target = script_target;
    }

    pub(crate) fn language_version(&self) -> ScriptTarget {
        if self.script_target == ScriptTarget::NONE {
            return ScriptTarget::LATEST;
        }
        self.script_target
    }

    pub(crate) fn error(&mut self, diagnostic: &'static Message) {
        self.error_at(diagnostic, self.state.pos, 0, &[]);
    }

    pub(crate) fn error_at(
        &mut self,
        diagnostic: &'static Message,
        pos: u32,
        length: u32,
        args: &[&dyn Display],
    ) {
        if self.on_error {
            self.errors.push(ScanError {
                message: diagnostic,
                start: pos,
                length,
                args: args.iter().map(std::string::ToString::to_string).collect(),
            });
        }
    }

    // NOTE: this only returns the current byte. It must be checked against
    // utf8.RuneSelf to verify that a call to charAndSize is not needed.
    #[inline]
    pub(crate) fn char(&self) -> Option<u8> {
        self.text.as_bytes().get(self.state.pos as usize).copied()
    }

    // NOTE: this only returns the byte at the offset.
    #[inline]
    pub(crate) fn char_at(&self, offset: u32) -> Option<u8> {
        self.state
            .pos
            .checked_add(offset)
            .and_then(|p| self.text.as_bytes().get(p as usize))
            .copied()
    }

    #[inline]
    pub(crate) fn char_and_size(&self) -> (Rune, u32) {
        // Fast path: a single ASCII byte.
        if self.state.pos < self.end {
            let b = self.text.as_bytes()[self.state.pos as usize];
            if b < 0x80 {
                return (u32::from(b), 1);
            }
        }
        let (r, size) =
            decode_rune(&self.text.as_bytes()[(self.state.pos as usize).min(self.text.len())..]);
        (r, size as u32)
    }

    // scanASCIIWhile advances s.pos over the longest run of ASCII bytes for which
    // pred returns true. It stops at end-of-text, the first non-ASCII byte, or the
    // first byte where pred is false.
    #[inline]
    fn scan_ascii_while(&mut self, pred: impl Fn(u8) -> bool) {
        let text = &self.text.as_bytes()[self.state.pos as usize..self.end as usize];
        let mut i = 0;
        while i < text.len() {
            let b = text[i];
            if b >= 0x80 || !pred(b) {
                break;
            }
            i += 1;
        }
        self.state.pos += i as u32;
    }

    #[inline]
    fn scan_ascii_identifier_parts(&mut self) {
        let text = &self.text.as_bytes()[self.state.pos as usize..self.end as usize];
        let (batches, tail) = text.as_chunks::<32>();
        let mut scanned = 0;
        for batch in batches {
            for (i, &byte) in batch.iter().enumerate() {
                if !ASCII_IDENTIFIER_PART[byte as usize] {
                    self.state.pos += (scanned + i) as u32;
                    return;
                }
            }
            scanned += batch.len();
        }
        for &byte in tail {
            if !ASCII_IDENTIFIER_PART[byte as usize] {
                break;
            }
            scanned += 1;
        }
        self.state.pos += scanned as u32;
    }

    #[inline]
    fn scan_ascii_jsdoc_comment_text(&mut self, in_backticks: bool) {
        let text = &self.text.as_bytes()[self.state.pos as usize..];
        let table = &ASCII_JSDOC_COMMENT_TEXT[in_backticks as usize];
        let (batches, tail) = text.as_chunks::<32>();
        let mut scanned = 0;
        for batch in batches {
            for (i, &byte) in batch.iter().enumerate() {
                if !table[byte as usize] {
                    self.state.pos += (scanned + i) as u32;
                    return;
                }
            }
            scanned += batch.len();
        }
        for &byte in tail {
            if !table[byte as usize] {
                break;
            }
            scanned += 1;
        }
        self.state.pos += scanned as u32;
    }

    #[inline]
    pub(crate) fn slice(&self, start: u32, end: u32) -> &'src str {
        &self.text[start as usize..end as usize]
    }

    /// Go `a + b` for two token-value fragments: when `b` directly follows `a` in the source
    /// text the result is the covering text slice, otherwise it is arena-allocated.
    fn concat_value(&self, a: S::Text, b: &str) -> S::Text {
        if b.is_empty() {
            return a;
        }
        if a.is_empty() {
            return self.intern_value(b);
        }
        let base = self.text.as_ptr() as usize;
        let text_end = base + self.text.len();
        let a_start = a.as_ptr() as usize;
        let b_start = b.as_ptr() as usize;
        if a_start >= base && a_start + a.len() == b_start && b_start + b.len() <= text_end {
            let start = a_start - base;
            return (&self.text[start..start + a.len() + b.len()]).into();
        }
        let mut s = S::Builder::with_capacity(a.len() + b.len());
        s.push_str(&a);
        s.push_str(b);
        s.finish()
    }

    /// Returns `s` as a `'static` value without copying if it already is a slice of the text.
    fn intern_value(&self, s: &str) -> S::Text {
        let base = self.text.as_ptr() as usize;
        let p = s.as_ptr() as usize;
        if p >= base && p + s.len() <= base + self.text.len() {
            let start = p - base;
            return (&self.text[start..start + s.len()]).into();
        }
        S::copy_text(s)
    }

    pub fn scan(&mut self) -> Kind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::None;
        loop {
            let mut pos = self.state.pos;
            let bytes = self.text.as_bytes();
            let byte = loop {
                if pos >= self.end {
                    self.state.pos = pos;
                    self.state.token_start = pos;
                    self.state.token = Kind::EndOfFile;
                    return Kind::EndOfFile;
                }
                let byte = bytes[pos as usize];
                if self.skip_trivia {
                    // Keep ASCII trivia out of the function table: even a single space
                    // otherwise pays for the frame of the Unicode trivia path.
                    match byte {
                        b' ' | b'\t' | 0x0B | 0x0C => {}
                        b'\r' | b'\n' => self.state.token_flags |= TokenFlags::PrecedingLineBreak,
                        _ => break byte,
                    }
                    pos += 1;
                } else {
                    break byte;
                }
            };
            self.state.pos = pos;
            self.state.token_start = pos;
            if let Some(token) = Self::BYTE_HANDLERS[byte as usize](self) {
                self.state.token = token;
                return token;
            }
        }
    }

    #[cold]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_whitespace(&mut self) -> Option<Kind> {
        debug_assert!(!self.skip_trivia);
        self.state.pos += 1;
        loop {
            let (ch, size) = self.char_and_size();
            if !stringutil::is_white_space_single_line(ch) {
                break;
            }
            self.state.pos += size;
        }
        Some(Kind::WhitespaceTrivia)
    }

    #[cold]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_line_break(&mut self) -> Option<Kind> {
        debug_assert!(!self.skip_trivia);
        let ch = self.char();
        self.state.token_flags |= TokenFlags::PrecedingLineBreak;
        if ch == Some(b'\r') && self.char_at(1) == Some(b'\n') {
            self.state.pos += 2;
        } else {
            self.state.pos += 1;
        }
        Some(Kind::NewLineTrivia)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_exclamation(&mut self) -> Option<Kind> {
        let token = if self.char_at(1) == Some(b'=') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::ExclamationEqualsEqualsToken
            } else {
                self.state.pos += 2;
                Kind::ExclamationEqualsToken
            }
        } else {
            self.state.pos += 1;
            Kind::ExclamationToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_string(&mut self) -> Option<Kind> {
        self.state.token_value = self.scan_string(false /*jsxAttributeString*/);
        Some(Kind::StringLiteral)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_template(&mut self) -> Option<Kind> {
        Some(self.scan_template_and_set_token_value(false /*shouldEmitInvalidEscapeError*/))
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_percent(&mut self) -> Option<Kind> {
        let token = if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            Kind::PercentEqualsToken
        } else {
            self.state.pos += 1;
            Kind::PercentToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_ampersand(&mut self) -> Option<Kind> {
        let next = self.char_at(1);
        let token = if next == Some(b'&') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::AmpersandAmpersandEqualsToken
            } else {
                self.state.pos += 2;
                Kind::AmpersandAmpersandToken
            }
        } else if next == Some(b'=') {
            self.state.pos += 2;
            Kind::AmpersandEqualsToken
        } else {
            self.state.pos += 1;
            Kind::AmpersandToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_open_paren(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::OpenParenToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_close_paren(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::CloseParenToken)
    }

    fn scan_byte_asterisk(&mut self) -> Option<Kind> {
        let next = self.char_at(1);
        let token = if next == Some(b'=') {
            self.state.pos += 2;
            Kind::AsteriskEqualsToken
        } else if next == Some(b'*') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::AsteriskAsteriskEqualsToken
            } else {
                self.state.pos += 2;
                Kind::AsteriskAsteriskToken
            }
        } else {
            self.state.pos += 1;
            if self.state.skip_jsdoc_leading_asterisks != 0
                && !self.state.token_flags.intersects(TokenFlags::PrecedingJSDocLeadingAsterisks)
                && self.state.token_flags.intersects(TokenFlags::PrecedingLineBreak)
            {
                self.state.token_flags |= TokenFlags::PrecedingJSDocLeadingAsterisks;
                return None;
            }
            Kind::AsteriskToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_plus(&mut self) -> Option<Kind> {
        let next = self.char_at(1);
        let token = if next == Some(b'=') {
            self.state.pos += 2;
            Kind::PlusEqualsToken
        } else if next == Some(b'+') {
            self.state.pos += 2;
            Kind::PlusPlusToken
        } else {
            self.state.pos += 1;
            Kind::PlusToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_comma(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::CommaToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_minus(&mut self) -> Option<Kind> {
        let next = self.char_at(1);
        let token = if next == Some(b'=') {
            self.state.pos += 2;
            Kind::MinusEqualsToken
        } else if next == Some(b'-') {
            self.state.pos += 2;
            Kind::MinusMinusToken
        } else {
            self.state.pos += 1;
            Kind::MinusToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_dot(&mut self) -> Option<Kind> {
        let next = self.char_at(1);
        let token = if next.is_some_and(|ch| stringutil::is_digit(u32::from(ch))) {
            self.scan_number()
        } else if next == Some(b'.') && self.char_at(2) == Some(b'.') {
            self.state.pos += 3;
            Kind::DotDotDotToken
        } else {
            self.state.pos += 1;
            Kind::DotToken
        };
        Some(token)
    }

    fn scan_byte_slash(&mut self) -> Option<Kind> {
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

            if self.skip_trivia {
                return None;
            }
            return Some(Kind::SingleLineCommentTrivia);
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

                if ch1 == '*' as Rune && self.char_at(1) == Some(b'/') {
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
                self.scan_jsdoc_comment_for_tags(
                    self.slice(self.state.token_start, self.state.pos),
                );
            }

            self.process_comment_directive(last_line_start, self.state.pos, true);

            if !comment_closed {
                self.error(&diagnostics::ASTERISK_SLASH_EXPECTED);
            }

            if self.skip_trivia {
                return None;
            }

            if !comment_closed {
                self.state.token_flags |= TokenFlags::Unterminated;
            }
            return Some(Kind::MultiLineCommentTrivia);
        }
        let token = if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            Kind::SlashEqualsToken
        } else {
            self.state.pos += 1;
            Kind::SlashToken
        };
        Some(token)
    }

    // After the first line break, only the closing delimiter and final line start remain relevant.
    // Keep the final line start for comment directives; CRLF ends after LF, like the scalar loop.
    fn scan_multi_line_comment_tail(&mut self) -> (bool, u32) {
        let start = self.state.pos as usize;
        debug_assert!(start <= self.end as usize);
        debug_assert!(
            self.text[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| stringutil::is_line_break(ch as Rune))
        );
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

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_number(&mut self) -> Option<Kind> {
        let ch = self.char();
        let token = 'digit: {
            if ch == Some(b'0') {
                if self.char_at(1) == Some(b'X') || self.char_at(1) == Some(b'x') {
                    let start = self.state.pos;
                    self.state.pos += 2;
                    let mut digits = self.scan_hex_digits(1, true, true);
                    if digits.is_empty() {
                        self.error(&diagnostics::HEXADECIMAL_DIGIT_EXPECTED);
                        digits = "0".into();
                    }
                    let raw_text = self.slice(start, self.state.pos);
                    if raw_text.starts_with("0x") && &raw_text[2..] == digits.as_ref() {
                        self.state.token_value = raw_text.into();
                    } else if let Some(cached_value) = self.hex_number_cache.get(digits.as_ref()) {
                        self.state.token_value = cached_value.clone();
                    } else {
                        let mut value = S::Builder::with_capacity(2 + digits.len());
                        value.push_str("0x");
                        value.push_str(&digits);
                        self.state.token_value = value.finish();
                        self.pin_unless_source(&digits, &self.state.token_value);
                        self.hex_number_cache.insert(digits, self.state.token_value.clone());
                    }
                    self.state.token_flags |= TokenFlags::HexSpecifier;
                    break 'digit self.scan_big_int_suffix();
                }
                if self.char_at(1) == Some(b'B') || self.char_at(1) == Some(b'b') {
                    self.state.pos += 2;
                    let (value, has_digits) = self.scan_binary_or_octal_digits(2, "0b");
                    if has_digits {
                        self.state.token_value = value;
                    } else {
                        self.error(&diagnostics::BINARY_DIGIT_EXPECTED);
                        self.state.token_value = "0b0".into();
                    }
                    self.state.token_flags |= TokenFlags::BinarySpecifier;
                    break 'digit self.scan_big_int_suffix();
                }
                if self.char_at(1) == Some(b'O') || self.char_at(1) == Some(b'o') {
                    self.state.pos += 2;
                    let (value, has_digits) = self.scan_binary_or_octal_digits(8, "0o");
                    if has_digits {
                        self.state.token_value = value;
                    } else {
                        self.error(&diagnostics::OCTAL_DIGIT_EXPECTED);
                        self.state.token_value = "0o0".into();
                    }
                    self.state.token_flags |= TokenFlags::OctalSpecifier;
                    break 'digit self.scan_big_int_suffix();
                }
            }
            self.scan_number()
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_colon(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::ColonToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_semicolon(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::SemicolonToken)
    }

    fn scan_byte_less_than(&mut self) -> Option<Kind> {
        if self.char_at(1) == Some(b'<') && is_conflict_marker_trivia(self.text, self.state.pos) {
            self.state.pos = self.scan_conflict_marker_trivia_reporting(self.state.pos);
            if self.skip_trivia {
                return None;
            }
            return Some(Kind::ConflictMarkerTrivia);
        }
        let token = if self.char_at(1) == Some(b'<') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::LessThanLessThanEqualsToken
            } else {
                self.state.pos += 2;
                Kind::LessThanLessThanToken
            }
        } else if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            Kind::LessThanEqualsToken
        } else if self.language_variant == LanguageVariant::JSX
            && self.char_at(1) == Some(b'/')
            && self.char_at(2) != Some(b'*')
        {
            self.state.pos += 2;
            Kind::LessThanSlashToken
        } else {
            self.state.pos += 1;
            Kind::LessThanToken
        };
        Some(token)
    }

    fn scan_byte_equals(&mut self) -> Option<Kind> {
        if self.char_at(1) == Some(b'=') && is_conflict_marker_trivia(self.text, self.state.pos) {
            self.state.pos = self.scan_conflict_marker_trivia_reporting(self.state.pos);
            if self.skip_trivia {
                return None;
            }
            return Some(Kind::ConflictMarkerTrivia);
        }
        let token = if self.char_at(1) == Some(b'=') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::EqualsEqualsEqualsToken
            } else {
                self.state.pos += 2;
                Kind::EqualsEqualsToken
            }
        } else if self.char_at(1) == Some(b'>') {
            self.state.pos += 2;
            Kind::EqualsGreaterThanToken
        } else {
            self.state.pos += 1;
            Kind::EqualsToken
        };
        Some(token)
    }

    fn scan_byte_greater_than(&mut self) -> Option<Kind> {
        if self.char_at(1) == Some(b'>') && is_conflict_marker_trivia(self.text, self.state.pos) {
            self.state.pos = self.scan_conflict_marker_trivia_reporting(self.state.pos);
            if self.skip_trivia {
                return None;
            }
            return Some(Kind::ConflictMarkerTrivia);
        }
        self.state.pos += 1;
        Some(Kind::GreaterThanToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_question(&mut self) -> Option<Kind> {
        let token = if self.char_at(1) == Some(b'.')
            && !self.char_at(2).is_some_and(|ch| stringutil::is_digit(u32::from(ch)))
        {
            self.state.pos += 2;
            Kind::QuestionDotToken
        } else if self.char_at(1) == Some(b'?') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::QuestionQuestionEqualsToken
            } else {
                self.state.pos += 2;
                Kind::QuestionQuestionToken
            }
        } else {
            self.state.pos += 1;
            Kind::QuestionToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_open_bracket(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::OpenBracketToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_close_bracket(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::CloseBracketToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_caret(&mut self) -> Option<Kind> {
        let token = if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            Kind::CaretEqualsToken
        } else {
            self.state.pos += 1;
            Kind::CaretToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_open_brace(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::OpenBraceToken)
    }

    fn scan_byte_bar(&mut self) -> Option<Kind> {
        if self.char_at(1) == Some(b'|') && is_conflict_marker_trivia(self.text, self.state.pos) {
            self.state.pos = self.scan_conflict_marker_trivia_reporting(self.state.pos);
            if self.skip_trivia {
                return None;
            }
            return Some(Kind::ConflictMarkerTrivia);
        }
        let token = if self.char_at(1) == Some(b'|') {
            if self.char_at(2) == Some(b'=') {
                self.state.pos += 3;
                Kind::BarBarEqualsToken
            } else {
                self.state.pos += 2;
                Kind::BarBarToken
            }
        } else if self.char_at(1) == Some(b'=') {
            self.state.pos += 2;
            Kind::BarEqualsToken
        } else {
            self.state.pos += 1;
            Kind::BarToken
        };
        Some(token)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_close_brace(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::CloseBraceToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_tilde(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::TildeToken)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_at(&mut self) -> Option<Kind> {
        self.state.pos += 1;
        Some(Kind::AtToken)
    }

    fn scan_byte_identifier(&mut self) -> Option<Kind> {
        debug_assert!(matches!(self.char(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'$' | b'_')));
        let start = self.state.pos;
        self.state.pos += 1;
        self.scan_ascii_identifier_parts();
        let ch = self.char();
        if ch.is_none_or(|ch| ch < RUNE_SELF as u8 && ch != b'\\') {
            self.state.token_value = self.slice(start, self.state.pos).into();
            return Some(get_identifier_token(&self.state.token_value));
        }
        // The generic scanner handles a Unicode continuation or escape from the
        // original start, including malformed escapes and diagnostic positions.
        self.state.pos = start;
        self.scan_byte_identifier_slow()
    }

    #[cold]
    #[expect(
        clippy::unnecessary_wraps,
        reason = "BYTE_HANDLERS requires Option<Kind>; other byte handlers return None to continue scanning trivia."
    )]
    fn scan_byte_identifier_slow(&mut self) -> Option<Kind> {
        let token = if self.scan_identifier(0, IdentifierVariant::Standard) {
            get_identifier_token(&self.state.token_value)
        } else {
            self.scan_invalid_character();
            Kind::Unknown
        };
        Some(token)
    }

    fn scan_byte_hash(&mut self) -> Option<Kind> {
        let token = 'hash: {
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
                break 'hash Kind::Unknown;
            }
            if !self.scan_identifier(1, IdentifierVariant::Standard) {
                self.error_at(&diagnostics::INVALID_CHARACTER, self.state.pos - 1, 1, &[]);
                self.state.token_value = "#".into();
            }
            Kind::PrivateIdentifier
        };
        Some(token)
    }

    fn scan_byte_default(&mut self) -> Option<Kind> {
        let token = 'default: {
            if self.scan_identifier(0, IdentifierVariant::Standard) {
                break 'default get_identifier_token(&self.state.token_value);
            }
            let (mut ch, mut size) = self.char_and_size();
            if ch == RUNE_ERROR {
                self.error_at(&diagnostics::FILE_APPEARS_TO_BE_BINARY, 0, 0, &[]);
                self.state.pos = self.text.len() as u32;
                break 'default Kind::NonTextFileMarkerTrivia;
            }
            if stringutil::is_white_space_single_line(ch) {
                self.state.pos += size;

                // If we get here and it's not 0x0085 (nextLine), then we're handling non-ASCII whitespace.
                // Handle skipTrivia like we do in the space case above.
                if ch == 0x0085 || self.skip_trivia {
                    return None;
                }

                loop {
                    (ch, size) = self.char_and_size();
                    if !stringutil::is_white_space_single_line(ch) {
                        break;
                    }
                    self.state.pos += size;
                }
                return Some(Kind::WhitespaceTrivia);
            }
            if stringutil::is_line_break(ch) {
                self.state.token_flags |= TokenFlags::PrecedingLineBreak;
                self.state.pos += size;
                return None;
            }
            self.scan_invalid_character();
            Kind::Unknown
        };
        Some(token)
    }

    fn scan_conflict_marker_trivia_reporting(&mut self, pos: u32) -> u32 {
        let text = self.text;
        scan_conflict_marker_trivia(
            text,
            pos,
            Some(&mut |diag, pos, length| self.error_at(diag, pos, length, &[])),
        )
    }

    fn process_comment_directive(&mut self, start: u32, end: u32, multiline: bool) {
        let text = self.text.as_bytes();
        let end_u = end as usize;
        // Skip starting slashes and whitespace
        let mut pos = start as usize;
        if multiline {
            // Skip whitespace
            while pos < end_u && (text[pos] == b' ' || text[pos] == b'\t') {
                pos += 1;
            }
            // Skip combinations of / and *
            while pos < end_u && (text[pos] == b'/' || text[pos] == b'*') {
                pos += 1;
            }
        } else {
            // Skip opening //
            pos += 2;
            // Skip another / if present
            while pos < end_u && text[pos] == b'/' {
                pos += 1;
            }
        }
        // Skip whitespace
        while pos < end_u && (text[pos] == b' ' || text[pos] == b'\t') {
            pos += 1;
        }
        // Directive must start with '@'
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

    pub fn re_scan_less_than_token(&mut self) -> Kind {
        if self.state.token == Kind::LessThanLessThanToken {
            self.state.pos = self.state.token_start + 1;
            self.state.token = Kind::LessThanToken;
        }
        self.state.token
    }

    pub fn re_scan_greater_than_token(&mut self) -> Kind {
        if self.state.token == Kind::GreaterThanToken {
            self.re_scan_greater_than_token_inner();
        }
        self.state.token
    }

    fn re_scan_greater_than_token_inner(&mut self) {
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

    pub fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
        self.state.pos = self.state.token_start;
        self.state.token = self.scan_template_and_set_token_value(!is_tagged_template);
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

    /// Go `ReScanSlashToken(reportErrors ...bool)`; pass `false` for the no-argument form.
    pub fn re_scan_slash_token(&mut self, report_errors: bool) -> Kind {
        let should_report_errors = report_errors;
        if self.state.token == Kind::SlashToken || self.state.token == Kind::SlashEqualsToken {
            let text = self.text.as_bytes();
            let end = self.end;
            // Quickly get to the end of regex such that we know the flags
            let start_of_reg_exp_body = self.state.token_start + 1;
            let mut p = start_of_reg_exp_body;
            let mut in_escape = false;
            let mut named_capture_groups = false;
            // Although nested character classes are allowed in Unicode Sets mode,
            // an unescaped slash is nevertheless invalid even in a character class in any Unicode mode.
            // This is indicated by Section 12.9.5 Regular Expression Literals of the specification,
            // where nested character classes are not considered at all. (A `[` RegularExpressionClassChar
            // does nothing in a RegularExpressionClass, and a `]` always closes the class.)
            // Additionally, parsing nested character classes will misinterpret regexes like `/[[]/`
            // as unterminated, consuming characters beyond the slash. (This even applies to `/[[]/v`,
            // which should be parsed as a well-terminated regex with an incomplete character class.)
            // Thus we must not handle nested character classes in the first pass.
            let mut in_character_class = false;
            loop {
                // If we reach the end of a file, or hit a newline, then this is an unterminated
                // regex. Report error and return what we have so far.
                if p >= end {
                    self.state.token_flags |= TokenFlags::Unterminated;
                    break;
                }
                let ch = u32::from(text[p as usize]);
                if stringutil::is_line_break(ch) {
                    self.state.token_flags |= TokenFlags::Unterminated;
                    break;
                } else if in_escape {
                    // Parsing an escape character;
                    // reset the flag and just advance to the next char.
                    in_escape = false;
                } else if ch == '/' as Rune && !in_character_class {
                    // A slash within a character class is permissible,
                    // but in general it signals the end of the regexp literal.
                    break;
                } else if ch == '[' as Rune {
                    in_character_class = true;
                } else if ch == '\\' as Rune {
                    in_escape = true;
                } else if ch == ']' as Rune {
                    in_character_class = false;
                } else if !in_character_class
                    && ch == '(' as Rune
                    && p + 1 < end
                    && text[(p + 1) as usize] == b'?'
                    && p + 2 < end
                    && text[(p + 2) as usize] == b'<'
                    && (p + 3 >= end
                        || (text[(p + 3) as usize] != b'=' && text[(p + 3) as usize] != b'!'))
                {
                    named_capture_groups = true;
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
                                // We encountered an unbalanced bracket outside a character class. Treat this position as the end of regex.
                                break;
                            }
                        }
                    }
                    p += 1;
                }
                // Whitespaces and semicolons at the end are not likely to be part of the regex
                while p > start_of_reg_exp_body {
                    let (ch, size) = decode_last_rune(&text[..p as usize]);
                    if stringutil::is_white_space_like(ch) || ch == ';' as Rune {
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
                // Consume the slash character
                p += 1;
                let mut reg_exp_flags = RegularExpressionFlags::None;
                while p < end {
                    let (ch, size) = decode_rune(&text[p as usize..]);
                    let size = size as u32;
                    if ch == RUNE_ERROR || !is_identifier_part(ch) {
                        break;
                    }
                    if should_report_errors {
                        match char_code_to_reg_exp_flag(ch) {
                            None => self.error_at(
                                &diagnostics::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                                p,
                                size,
                                &[],
                            ),
                            Some(flag) if reg_exp_flags.intersects(flag) => {
                                self.error_at(
                                    &diagnostics::DUPLICATE_REGULAR_EXPRESSION_FLAG,
                                    p,
                                    size,
                                    &[],
                                );
                            }
                            Some(flag)
                                if (reg_exp_flags | flag)
                                    .contains(RegularExpressionFlags::AnyUnicodeMode) =>
                            {
                                self.error_at(
                                    &diagnostics::THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY,
                                    p,
                                    size,
                                    &[],
                                );
                            }
                            Some(flag) => {
                                reg_exp_flags |= flag;
                                self.check_regular_expression_flag_availability(flag, p, size);
                            }
                        }
                    }
                    p += size;
                }
                if should_report_errors {
                    self.state.pos = start_of_reg_exp_body;
                    let save_end = self.end;
                    let save_token_pos = self.state.token_start;
                    let save_token_flags = self.state.token_flags;
                    self.end = end_of_reg_exp_body;
                    let scanner = std::mem::take(self);
                    let mut parser = RegExpParser::new(
                        scanner,
                        end_of_reg_exp_body,
                        reg_exp_flags.intersects(RegularExpressionFlags::AnyUnicodeMode),
                        reg_exp_flags.intersects(RegularExpressionFlags::UnicodeSets),
                        true,
                        named_capture_groups,
                    );
                    parser.run();
                    *self = parser.into_scanner();
                    self.end = save_end;
                    self.state.pos = p;
                    self.state.token_start = save_token_pos;
                    self.state.token_flags = save_token_flags;
                } else {
                    self.state.pos = p;
                }
            }

            self.state.pos = p;
            self.state.token_value = self.slice(self.state.token_start, self.state.pos).into();
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

    pub fn re_scan_hash_token(&mut self) -> Kind {
        if self.state.token == Kind::PrivateIdentifier {
            self.state.pos = self.state.token_start + 1;
            self.state.token = Kind::HashToken;
        }
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

    pub fn scan_jsx_token(&mut self) -> Kind {
        self.scan_jsx_token_ex(true /*allowMultilineJsxText*/)
    }

    pub fn scan_jsx_token_ex(&mut self, allow_multiline_jsx_text: bool) -> Kind {
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
            // First non-whitespace character on this line.
            let mut whitespace_state = JsxWhitespaceState::Leading;
            // These initial values are special because the first line is:
            // firstNonWhitespace = 0 to indicate that we want leading whitespace
            loop {
                let (ch, size) = self.char_and_size();
                if size == 0 || ch == '{' as Rune {
                    break;
                }
                if ch == '<' as Rune {
                    if is_conflict_marker_trivia(self.text, self.state.pos) {
                        self.state.pos = self.scan_conflict_marker_trivia_reporting(self.state.pos);
                        self.state.token = Kind::ConflictMarkerTrivia;
                        return self.state.token;
                    }
                    break;
                }
                if ch == '>' as Rune {
                    self.error_at(
                        &diagnostics::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                        self.state.pos,
                        1,
                        &[],
                    );
                } else if ch == '}' as Rune {
                    self.error_at(
                        &diagnostics::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                        self.state.pos,
                        1,
                        &[],
                    );
                }
                // FirstNonWhitespace is 0, then we only see whitespaces so far. If we see a linebreak, we want to ignore that whitespaces.
                // i.e (- : whitespace)
                //      <div>----
                //      </div> becomes <div></div>
                //
                //      <div>----</div> becomes <div>----</div>
                if stringutil::is_line_break(ch) && whitespace_state == JsxWhitespaceState::Leading
                {
                    whitespace_state = JsxWhitespaceState::AllWhitespace;
                } else if !allow_multiline_jsx_text
                    && stringutil::is_line_break(ch)
                    && whitespace_state == JsxWhitespaceState::SeenNonWhitespace
                {
                    // Stop JsxText on each line during formatting. This allows the formatter to
                    // indent each line correctly.
                    break;
                } else if !stringutil::is_white_space_like(ch) {
                    whitespace_state = JsxWhitespaceState::SeenNonWhitespace;
                }
                self.state.pos += size;
            }
            self.state.token_value = self.slice(self.state.full_start_pos, self.state.pos).into();
            self.state.token = Kind::JsxText;
            if whitespace_state == JsxWhitespaceState::AllWhitespace {
                self.state.token = Kind::JsxTextAllWhiteSpaces;
            }
        }
        self.state.token
    }

    // Scans a JSX identifier; these differ from normal identifiers in that they allow dashes
    pub fn scan_jsx_identifier(&mut self) -> Kind {
        if token_is_identifier_or_keyword(self.state.token) {
            // An identifier or keyword has already been parsed - check for a `-` or a single instance of `:` and then append it and
            // everything after it to the token
            // Do note that this means that `scanJsxIdentifier` effectively _mutates_ the visible token without advancing to a new token
            // Any caller should be expecting this behavior and should only read the pos or token value after calling it.
            // Here scanIdentifierParts is reused to ensure Unicode escapes are handled.
            let parts = self.scan_identifier_parts(IdentifierVariant::JSX);
            self.state.token_value = self.concat_value(self.state.token_value.clone(), &parts);
            self.state.token = get_identifier_token(&self.state.token_value);
        }
        self.state.token
    }

    pub fn scan_jsx_attribute_value(&mut self) -> Kind {
        self.state.full_start_pos = self.state.pos;
        // Skip whitespace between '=' and the value so tokenStart lands on the
        // opening quote, not on trivia.
        let (mut ch, mut size) = self.char_and_size();
        while size > 0 && stringutil::is_white_space_like(ch) {
            self.state.pos += size;
            (ch, size) = self.char_and_size();
        }
        self.state.token_start = self.state.pos;
        match self.char() {
            Some(0x22 | 0x27) => {
                self.state.token_value = self.scan_string(true /*jsxAttributeString*/);
                self.state.token = Kind::StringLiteral;
                self.state.token
            }
            // If this scans anything other than `{`, it's a parse error.
            _ => self.scan(),
        }
    }

    pub fn re_scan_jsx_attribute_value(&mut self) -> Kind {
        self.state.pos = self.state.full_start_pos;
        self.state.token_start = self.state.full_start_pos;
        self.scan_jsx_attribute_value()
    }

    /** In addition to the usual `JSDoc` ast.Kinds, can also return ast.KindJSDocCommentTextToken */
    pub fn scan_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> Kind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::None;
        if self.state.pos as usize >= self.text.len() {
            self.state.token = Kind::EndOfFile;
            return self.state.token;
        }
        self.state.token_start = self.state.pos;
        let text = self.text.as_bytes();
        while (self.state.pos as usize) < self.text.len() {
            self.scan_ascii_jsdoc_comment_text(in_backticks);
            if self.state.pos as usize == self.text.len() {
                break;
            }
            let (ch, size) = self.char_and_size();
            if stringutil::is_line_break(ch) || ch == '`' as Rune {
                break;
            }
            if !in_backticks {
                if ch == '{' as Rune {
                    break;
                } else if ch == '@' as Rune {
                    // @ doesn't start a new tag inside ``, and elsewhere, only after whitespace and before identifier
                    let (previous, _) = decode_last_rune(&text[..self.state.pos as usize]);
                    if stringutil::is_white_space_single_line(previous) {
                        let (next, _) = decode_rune(&text[(self.state.pos + size) as usize..]);
                        if is_identifier_start(next) {
                            break;
                        }
                    }
                }
            }
            self.state.pos += size;
        }
        if self.state.pos == self.state.token_start {
            return self.scan_jsdoc_token();
        }
        self.state.token_value = self.slice(self.state.token_start, self.state.pos).into();
        self.state.token = Kind::JSDocCommentTextToken;
        self.state.token
    }

    // Peek at the character at the current scanner position (expected to be right after '@')
    // and return true if a JSDoc tag can follow. Identifier starts indicate a tag name.
    // Whitespace, newlines, and EOF are also accepted to support incomplete tags for code completion.
    pub fn can_follow_jsdoc_at(&self) -> bool {
        if self.state.pos as usize >= self.text.len() {
            return true;
        }
        let (ch, _) = decode_rune(&self.text.as_bytes()[self.state.pos as usize..]);
        is_identifier_start(ch)
            || stringutil::is_white_space_single_line(ch)
            || stringutil::is_line_break(ch)
    }

    pub fn scan_jsdoc_token(&mut self) -> Kind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::None;
        if self.state.pos as usize >= self.text.len() {
            self.state.token = Kind::EndOfFile;
            return self.state.token;
        }

        self.state.token_start = self.state.pos;
        let (ch, size) = self.char_and_size();
        self.state.pos += size;
        let token = match ch {
            0x09 | 0x0B | 0x0C | 0x20 => {
                let (mut ch2, mut size2) = self.char_and_size();
                while size2 > 0 && stringutil::is_white_space_single_line(ch2) {
                    self.state.pos += size2;
                    (ch2, size2) = self.char_and_size();
                }
                Some(Kind::WhitespaceTrivia)
            }
            0x40 /* @ */ => Some(Kind::AtToken),
            0x0D | 0x0A => {
                if ch == '\r' as Rune && self.char() == Some(b'\n') {
                    self.state.pos += 1;
                }
                self.state.token_flags |= TokenFlags::PrecedingLineBreak;
                Some(Kind::NewLineTrivia)
            }
            0x2A /* * */ => Some(Kind::AsteriskToken),
            0x7B /* { */ => Some(Kind::OpenBraceToken),
            0x7D /* } */ => Some(Kind::CloseBraceToken),
            0x5B /* [ */ => Some(Kind::OpenBracketToken),
            0x5D /* ] */ => Some(Kind::CloseBracketToken),
            0x28 /* ( */ => Some(Kind::OpenParenToken),
            0x29 /* ) */ => Some(Kind::CloseParenToken),
            0x3C /* < */ => Some(Kind::LessThanToken),
            0x3E /* > */ => Some(Kind::GreaterThanToken),
            0x3D /* = */ => Some(Kind::EqualsToken),
            0x2C /* , */ => Some(Kind::CommaToken),
            0x2E /* . */ => Some(Kind::DotToken),
            0x60 /* ` */ => Some(Kind::BacktickToken),
            0x23 /* # */ => Some(Kind::HashToken),
            _ => None,
        };
        if let Some(token) = token {
            self.state.token = token;
            return self.state.token;
        }

        self.state.pos = self.state.token_start;
        if self.scan_identifier(0, IdentifierVariant::JSX) {
            self.state.token = get_identifier_token(&self.state.token_value);
            return self.state.token;
        }
        self.state.pos = self.state.token_start + size;
        self.state.token = Kind::Unknown;
        self.state.token
    }

    pub(crate) fn scan_identifier(
        &mut self,
        prefix_length: u32,
        variant: IdentifierVariant,
    ) -> bool {
        let start = self.state.pos;
        self.state.pos += prefix_length;
        let identifier_start = self.state.pos;
        let ch = self.char();
        // Fast path for simple ASCII identifiers
        if variant != IdentifierVariant::JSX
            && ch.is_some_and(|ch| {
                stringutil::is_ascii_letter(u32::from(ch)) || ch == b'_' || ch == b'$'
            })
        {
            self.state.pos += 1;
            self.scan_ascii_identifier_parts();
            let ch = self.char();
            if ch.is_none_or(|ch| ch < RUNE_SELF as u8 && ch != b'\\') {
                self.state.token_value = self.slice(start, self.state.pos).into();
                return true;
            }
            self.state.pos = identifier_start;
        }
        let (mut ch, mut size) = self.char_and_size();
        if is_identifier_start(ch) {
            let language_variant = if variant == IdentifierVariant::JSX {
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
            self.state.token_value = self.slice(start, self.state.pos).into();
            if ch == '\\' as Rune {
                let parts = self.scan_identifier_parts(variant);
                self.state.token_value = self.concat_value(self.state.token_value.clone(), &parts);
            }
            return true;
        }
        if ch == '\\' as Rune
            && let Some(escaped) = self.scan_identifier_escape(
                is_identifier_start,
                variant == IdentifierVariant::RegExpGroupName,
            )
        {
            let mut value = S::Builder::with_capacity((self.state.pos - start) as usize * 2);
            value.push_str(self.slice(start, identifier_start));
            value.push(rune_to_char(escaped));
            value.push_str(&self.scan_identifier_parts(variant));
            self.state.token_value = value.finish();
            return true;
        }
        false
    }

    fn scan_identifier_parts(&mut self, variant: IdentifierVariant) -> S::Text {
        let mut sb: Option<S::Builder> = None;
        let mut start = self.state.pos;
        let language_variant = if variant == IdentifierVariant::JSX {
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
                if let Some(escaped) = self.scan_identifier_escape(
                    |ch| is_identifier_part_ex(ch, language_variant),
                    variant == IdentifierVariant::RegExpGroupName,
                ) {
                    let so_far = (escape_start - start) as usize;
                    let sb =
                        sb.get_or_insert_with(|| S::Builder::with_capacity((so_far * 2).max(16)));
                    sb.push_str(self.slice(start, escape_start));
                    sb.push(rune_to_char(escaped));
                    start = self.state.pos;
                    continue;
                }
            }
            break;
        }
        match sb {
            None => self.slice(start, self.state.pos).into(),
            Some(mut sb) => {
                sb.push_str(self.slice(start, self.state.pos));
                sb.finish()
            }
        }
    }

    fn scan_identifier_escape(
        &mut self,
        is_valid: impl Fn(u32) -> bool,
        allow_surrogate_pair_escape: bool,
    ) -> Option<u32> {
        let escaped = self.peek_unicode_escape()?;
        if is_valid(escaped) {
            return self.scan_unicode_escape(true);
        }
        if allow_surrogate_pair_escape
            && self.char_at(2) != Some(b'{')
            && stringutil::is_high_surrogate(escaped)
        {
            // Unlike normal identifiers, group names in regular expressions, whether in Unicode mode or not,
            // accept \u HexLeadSurrogate \u HexTrailSurrogate as part of RegExpIdentifierName.
            // See https://github.com/tc39/ecma262/pull/1869 for the change.
            let saved_pos = self.state.pos;
            let saved_token_flags = self.state.token_flags;
            let _ = self.scan_unicode_escape(false);
            // scanLowSurrogateEscape also accepts the braced form used in string literals,
            // but RegExpIdentifierName does not allow it.
            if self.char_at(2) != Some(b'{')
                && let Some(code_point) = self.scan_low_surrogate_escape(escaped)
                && is_valid(code_point)
            {
                return Some(code_point);
            }
            self.state.pos = saved_pos;
            self.state.token_flags = saved_token_flags;
        }
        None
    }

    fn scan_string(&mut self, jsx_attribute_string: bool) -> S::Text {
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
            return if start == end { "" } else { self.slice(start, end) }.into();
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
    fn scan_string_slow(&mut self, quote: u8, mut start: u32, table: &[bool; 256]) -> S::Text {
        // Keep the original first-quote capacity estimate on the allocating path.
        let rest = &self.text.as_bytes()[self.state.pos as usize..];
        let capacity = memchr::memchr(quote, rest)
            .map_or(16, |offset| (self.state.pos - start) as usize + offset);
        let mut sb = S::Builder::with_capacity(capacity);
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
        sb.finish()
    }

    fn scan_template_and_set_token_value(
        &mut self,
        should_emit_invalid_escape_error: bool,
    ) -> Kind {
        let started_with_backtick = self.char() == Some(b'`');
        self.state.pos += 1;
        let mut start = self.state.pos;
        // Go collects the parts in a slice and joins them; a single untouched part is a text slice.
        let mut parts: Option<S::Builder> = None;
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
                let buf =
                    parts.get_or_insert_with(|| S::Builder::with_capacity((so_far * 2).max(16)));
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
                let buf =
                    parts.get_or_insert_with(|| S::Builder::with_capacity((so_far * 2).max(16)));
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

    fn finish_template_parts(&mut self, parts: &mut Option<S::Builder>, start: u32, end: u32) {
        self.state.token_value = match parts.take() {
            None => self.slice(start, end).into(),
            Some(mut buf) => {
                buf.push_str(self.slice(start, end));
                buf.finish()
            }
        };
    }

    pub(crate) fn scan_escape_sequence(
        &mut self,
        flags: EscapeSequenceScanningFlags,
    ) -> Cow<'src, str> {
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
    pub(crate) fn scan_unicode_escape(
        &mut self,
        should_emit_invalid_escape_error: bool,
    ) -> Option<u32> {
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
        let hex_value = parse_int_32(&hex_digits, 16);
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

    fn scan_number(&mut self) -> Kind {
        let mut start = self.state.pos;
        let fixed_part: Cow<'src, str>;
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
                    let mut value = S::Builder::with_capacity(digits.len());
                    write!(value, "{val}").unwrap();
                    self.state.token_value = value.finish();
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
        let mut fractional_part: Cow<'src, str> = Cow::Borrowed("");
        let mut exponent_preamble = "";
        let mut exponent_part: Cow<'src, str> = Cow::Borrowed("");
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
            let mut value = S::Builder::with_capacity(capacity);
            value.push_str(&fixed_part);
            if !fractional_part.is_empty() {
                value.push('.');
                value.push_str(&fractional_part);
            }
            if !exponent_part.is_empty() {
                value.push_str(exponent_preamble);
                value.push_str(&exponent_part);
            }
            self.state.token_value = value.finish();
        } else {
            self.state.token_value = self.slice(start, end).into();
        }
        if self.state.token_flags.intersects(TokenFlags::ContainsLeadingZero) {
            self.error_at(
                &diagnostics::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED,
                start,
                self.state.pos - start,
                &[],
            );
            self.state.token_value = Self::canonical_number(self.state.token_value.clone());
            return Kind::NumericLiteral;
        }

        let result = if fixed_part_end == self.state.pos {
            self.scan_big_int_suffix()
        } else {
            self.state.token_value = Self::canonical_number(self.state.token_value.clone());
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
    fn canonical_number(v: S::Text) -> S::Text {
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
        let s = crate::number::parse_literal(&v).unwrap_or_else(|| jsnum::from_string(&v)).string();
        if s == v.as_ref() {
            return v;
        }
        S::copy_text(&s)
    }

    fn scan_number_fragment(&mut self) -> Cow<'src, str> {
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

    fn scan_digits(&mut self) -> (&'src str, bool) {
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

    pub(crate) fn scan_hex_digits(
        &mut self,
        min_count: u32,
        scan_as_many_as_possible: bool,
        can_have_separators: bool,
    ) -> S::Text {
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
            return "".into();
        }
        let digits = self.slice(start, self.state.pos);
        let has_separator = memchr::memchr(b'_', digits.as_bytes()).is_some();
        let has_uppercase = digits.bytes().any(|b| b.is_ascii_uppercase());
        if !has_separator && !has_uppercase {
            return digits.into();
        }
        if let Some(cached) = self.hex_digit_cache.get(digits) {
            cached.clone()
        } else {
            let original = digits;
            let has_separator = self.state.token_flags.intersects(TokenFlags::ContainsSeparator);
            let has_uppercase = digits.bytes().any(|b| b.is_ascii_uppercase());
            let digits = if has_separator || has_uppercase {
                let mut result = S::Builder::with_capacity(digits.len());
                for byte in digits.bytes() {
                    if byte != b'_' {
                        result.push(byte.to_ascii_lowercase() as char);
                    }
                }
                result.finish()
            } else {
                digits.into()
            };
            self.pin_unless_source(original, &digits);
            self.hex_digit_cache.insert(original.into(), digits.clone());
            digits
        }
    }

    fn scan_binary_or_octal_digits(&mut self, base: u8, prefix: &str) -> (S::Text, bool) {
        let mut sb = S::Builder::with_capacity(16);
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
        (sb.finish(), has_digits)
    }

    fn scan_big_int_suffix(&mut self) -> Kind {
        if self.char() == Some(b'n') {
            // Go: tokenValue += "n"; when the value is a text slice directly followed by the `n`
            // this is again a text slice.
            let n = self.slice(self.state.pos, self.state.pos + 1);
            self.state.token_value = self.concat_value(self.state.token_value.clone(), n);
            if self.state.token_flags.intersects(TokenFlags::BinaryOrOctalSpecifier) {
                self.state.token_value =
                    S::copy_text(&(jsnum::parse_pseudo_big_int(&self.state.token_value) + "n"));
            }
            self.state.pos += 1;
            return Kind::BigIntLiteral;
        }
        if let Some(cached) = self.number_cache.get(self.state.token_value.as_ref()) {
            self.state.token_value = cached.clone();
        } else {
            let original = self.state.token_value.clone();
            let token_value = Self::canonical_number(original.clone());
            if token_value != original {
                self.pin_unless_source(&original, &token_value);
                self.number_cache.insert(original, token_value.clone());
            }
            self.state.token_value = token_value;
        }
        Kind::NumericLiteral
    }

    fn scan_invalid_character(&mut self) {
        let (_, size) = self.char_and_size();
        self.error_at(&diagnostics::INVALID_CHARACTER, self.state.pos, size, &[]);
        self.state.pos += size;
        self.state.token = Kind::Unknown;
    }
}
