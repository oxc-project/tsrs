//! The parser's tokens for source text (everything but `JSDoc`, which `tsrs_scanner::Scanner` scans; see
//! `crate::tokens`).
//!
//! `oxc_lexer` tokenizes the text in chunks, as `scan` reaches them. `scan` takes each token from that stream when
//! TypeScript's scanner would produce the same token at the same position (`oxc.rs`), and otherwise scans it with
//! the TypeScript-compatible scanner in `scan.rs` and `literals.rs`. That scanner also skips the trivia the parser
//! needs to see (comments, which set token flags and comment directives, and conflict markers), does every rescan
//! and reports TypeScript's scanner diagnostics.
//!
//! The API follows `tsrs_scanner::Scanner` (see its module notes on token values, buffered errors and
//! speculation), minus what only `JSDoc` scanning or the language service uses.

mod literals;
mod oxc;
mod scan;
mod trivia;

use std::fmt::Display;

use tsrs_ast::{CommentDirective, Kind, TokenFlags};
use tsrs_core::{ArenaStringBuilder, LanguageVariant, TextRange, alloc_str, stringutil::Rune};
use tsrs_diagnostics::Message;
use tsrs_scanner::ScanError;

use trivia::{ASCII_IDENTIFIER_PART, decode_rune};

#[derive(Clone, Copy, PartialEq, Eq)]
enum IdentifierVariant {
    Standard,
    Jsx,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsxWhitespaceState {
    Leading,
    AllWhitespace,
    SeenNonWhitespace,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    struct EscapeSequenceScanningFlags: u32 {
        const String = 1 << 0;
        const ReportErrors = 1 << 1;
        const RegularExpression = 1 << 2;
        const AnnexB = 1 << 3;
        const AnyUnicodeMode = 1 << 4;
        const AtomEscape = 1 << 5;
        const ReportInvalidEscapeErrors = Self::RegularExpression.bits() | Self::ReportErrors.bits();
        const AllowExtendedUnicodeEscape = Self::String.bits() | Self::AnyUnicodeMode.bits();
    }
}

/// A `mark` snapshot: the token state and the number of comment directives collected so far.
#[derive(Clone, Copy, Debug, Default)]
pub struct LexState {
    /// Current position in text (and ending position of current token)
    pos: u32,
    /// Starting position of current token including preceding whitespace
    full_start_pos: u32,
    /// Starting position of non-whitespace part of current token
    token_start: u32,
    token: Kind,
    token_value: &'static str,
    token_flags: TokenFlags,
    comment_directives_len: usize,
    /// A hint for finding the `oxc_lexer` token at a position: the first one at or after the last position
    /// looked up.
    oxc_index: u32,
}

#[derive(Default)]
pub struct Lexer {
    text: &'static str,
    end: u32,
    language_variant: LanguageVariant,
    errors: Vec<ScanError>,
    state: LexState,
    comment_directives: Vec<CommentDirective>,
    oxc: oxc::OxcTokens,
}

impl Lexer {
    pub fn reset(&mut self) {
        self.text = "";
        self.end = 0;
        self.language_variant = LanguageVariant::default();
        self.errors.clear();
        self.state = LexState::default();
        self.comment_directives.clear();
        self.oxc.reset();
    }

    pub fn set_text(&mut self, text: &'static str) {
        self.text = text;
        self.end = text.len() as u32;
        self.state = LexState::default();
        self.comment_directives.clear();
        self.oxc.reset();
    }

    /// Lexes only `start..end` with `oxc_lexer` (`OxcTokens::set_window`).
    pub fn set_oxc_window(&mut self, start: u32, end: u32) {
        self.oxc.set_window(start, end);
    }

    pub fn set_language_variant(&mut self, language_variant: LanguageVariant) {
        self.language_variant = language_variant;
    }

    pub fn text(&self) -> &'static str {
        self.text
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

    pub fn token_text(&self) -> &'static str {
        &self.text[self.state.token_start as usize..self.state.pos as usize]
    }

    pub fn token_value(&self) -> &'static str {
        self.state.token_value
    }

    pub fn token_range(&self) -> TextRange {
        TextRange::new(self.state.token_start, self.state.pos)
    }

    pub fn comment_directives(&self) -> &[CommentDirective] {
        &self.comment_directives[..self.state.comment_directives_len]
    }

    pub fn mark(&self) -> LexState {
        self.state
    }

    pub fn rewind(&mut self, state: LexState) {
        self.state = state;
        self.comment_directives.truncate(state.comment_directives_len);
    }

    #[inline]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn take_errors(&mut self) -> Vec<ScanError> {
        std::mem::take(&mut self.errors)
    }

    pub fn reset_pos(&mut self, pos: u32) {
        self.state.pos = pos;
        self.state.full_start_pos = pos;
        self.state.token_start = pos;
        self.state.oxc_index = self.oxc.index_at_or_after(pos);
    }

    pub fn reset_token_state(&mut self, pos: u32) {
        self.reset_pos(pos);
        self.state.token = Kind::Unknown;
        self.state.token_value = "";
        self.state.token_flags = TokenFlags::None;
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

    pub fn has_preceding_jsdoc_with_deprecated_tag(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithDeprecated)
    }

    pub fn has_preceding_jsdoc_with_see_or_link(&self) -> bool {
        self.state.token_flags.intersects(TokenFlags::PrecedingJSDocWithSeeOrLink)
    }

    pub fn scan(&mut self) -> Kind {
        self.state.full_start_pos = self.state.pos;
        self.state.token_flags = TokenFlags::None;
        loop {
            let bytes = self.text.as_bytes();
            let mut pos = self.state.pos;
            let byte = loop {
                if pos >= self.end {
                    self.state.pos = pos;
                    self.state.token_start = pos;
                    self.state.token = Kind::EndOfFile;
                    return Kind::EndOfFile;
                }
                let byte = bytes[pos as usize];
                match byte {
                    b' ' | b'\t' | 0x0B | 0x0C => {}
                    b'\r' | b'\n' => self.state.token_flags |= TokenFlags::PrecedingLineBreak,
                    _ => break byte,
                }
                pos += 1;
            };
            self.state.pos = pos;
            self.state.token_start = pos;
            if !self.oxc.covers(pos) {
                self.oxc.lex_chunk(self.text, self.language_variant, pos);
                self.state.oxc_index = self.oxc.index_at_or_after(pos);
            }
            let token = match self.take_oxc_token() {
                Some(token) => token,
                None => match self.scan_compat(byte) {
                    Some(token) => token,
                    None => continue,
                },
            };
            self.state.token = token;
            return token;
        }
    }

    fn error(&mut self, diagnostic: &'static Message) {
        self.error_at(diagnostic, self.state.pos, 0, &[]);
    }

    fn error_at(&mut self, diagnostic: &'static Message, pos: u32, length: u32, args: &[&dyn Display]) {
        self.errors.push(ScanError {
            message: diagnostic,
            start: pos,
            length,
            args: args.iter().map(std::string::ToString::to_string).collect(),
        });
    }

    // NOTE: this only returns the current byte. It must be checked against
    // utf8.RuneSelf to verify that a call to charAndSize is not needed.
    #[inline]
    fn char(&self) -> Option<u8> {
        self.text.as_bytes().get(self.state.pos as usize).copied()
    }

    // NOTE: this only returns the byte at the offset.
    #[inline]
    fn char_at(&self, offset: u32) -> Option<u8> {
        self.state.pos.checked_add(offset).and_then(|p| self.text.as_bytes().get(p as usize)).copied()
    }

    #[inline]
    fn char_and_size(&self) -> (Rune, u32) {
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

    /// Advances over the longest run of ASCII bytes for which `pred` holds.
    #[inline]
    fn scan_ascii_while(&mut self, pred: impl Fn(u8) -> bool) {
        let text = &self.text.as_bytes()[self.state.pos as usize..self.end as usize];
        let scanned = text.iter().position(|&b| b >= 0x80 || !pred(b)).unwrap_or(text.len());
        self.state.pos += scanned as u32;
    }

    #[inline]
    fn scan_ascii_identifier_parts(&mut self) {
        let text = &self.text.as_bytes()[self.state.pos as usize..self.end as usize];
        let scanned =
            text.iter().position(|&b| !ASCII_IDENTIFIER_PART[b as usize]).unwrap_or(text.len());
        self.state.pos += scanned as u32;
    }

    #[inline]
    fn slice(&self, start: u32, end: u32) -> &'static str {
        &self.text[start as usize..end as usize]
    }

    /// `a + b` for two token-value fragments: when `b` directly follows `a` in the source text the result is the
    /// covering text slice, otherwise it is arena-allocated.
    fn concat_value(&self, a: &'static str, b: &str) -> &'static str {
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
            return &self.text[start..start + a.len() + b.len()];
        }
        let mut s = ArenaStringBuilder::with_capacity(a.len() + b.len());
        s.push_str(a);
        s.push_str(b);
        s.into_str()
    }

    /// Returns `s` as a `'static` value without copying if it already is a slice of the text.
    fn intern_value(&self, s: &str) -> &'static str {
        let base = self.text.as_ptr() as usize;
        let p = s.as_ptr() as usize;
        if p >= base && p + s.len() <= base + self.text.len() {
            let start = p - base;
            return &self.text[start..start + s.len()];
        }
        alloc_str(s)
    }
}
