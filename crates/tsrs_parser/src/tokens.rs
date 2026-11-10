//! The parser's token source. Source text is scanned by `Lexer` (`oxc_lexer`, with a TypeScript-compatible
//! fallback); the inside of a `JSDoc` comment is scanned by `tsrs_scanner::Scanner`, between `enter_jsdoc` and
//! `exit_jsdoc`. Every other method goes to whichever is active.

use tsrs_ast::{CommentDirective, Kind, TokenFlags};
use tsrs_core::{LanguageVariant, TextRange};
use tsrs_scanner::{SavedScannerState, ScanError, Scanner, ScannerState};

use crate::lexer::{LexState, Lexer};

pub struct Tokens {
    lexer: Lexer,
    jsdoc: Scanner,
    /// The number of `JSDoc` comments being parsed: one, or more when one is parsed inside another.
    jsdoc_depth: u32,
}

/// A `mark` snapshot of the active scanner.
#[derive(Clone, Copy)]
pub enum TokensState {
    Lex(LexState),
    Jsdoc(ScannerState),
}

/// What `exit_jsdoc` restores: the enclosing `JSDoc` comment's text and state, if there is one.
#[must_use]
pub struct SavedJsdoc(Option<(&'static str, SavedScannerState)>);

macro_rules! active {
    ($self:ident.$method:ident($($arg:expr),*)) => {
        if $self.jsdoc_depth != 0 {
            $self.jsdoc.$method($($arg),*)
        } else {
            $self.lexer.$method($($arg),*)
        }
    };
}

impl Tokens {
    pub fn new() -> Tokens {
        Tokens { lexer: Lexer::default(), jsdoc: Scanner::new(), jsdoc_depth: 0 }
    }

    pub fn reset(&mut self) {
        self.lexer.reset();
        self.jsdoc.reset();
        self.jsdoc_depth = 0;
    }

    /// Sets the source text. Scanner diagnostics are always buffered (`take_errors`).
    pub fn set_text(&mut self, text: &'static str) {
        self.lexer.set_text(text);
        self.jsdoc.set_on_error(true);
    }

    pub fn set_language_variant(&mut self, language_variant: LanguageVariant) {
        self.lexer.set_language_variant(language_variant);
        self.jsdoc.set_language_variant(language_variant);
    }

    /// Lexes only `start..end` of the source text with `oxc_lexer`, for a parser that reads just that region.
    pub fn set_oxc_window(&mut self, start: u32, end: u32) {
        self.lexer.set_oxc_window(start, end);
    }

    /// Scans `text` (a `JSDoc` comment without its closing `*/`) from `pos` until `exit_jsdoc`.
    pub fn enter_jsdoc(&mut self, text: &'static str, pos: u32) -> SavedJsdoc {
        let saved = (self.jsdoc_depth != 0).then(|| (self.jsdoc.text(), self.jsdoc.take_state()));
        self.jsdoc_depth += 1;
        self.jsdoc.set_text(text);
        self.jsdoc.reset_pos(pos);
        SavedJsdoc(saved)
    }

    pub fn exit_jsdoc(&mut self, saved: SavedJsdoc) {
        self.jsdoc_depth -= 1;
        if let Some((text, state)) = saved.0 {
            self.jsdoc.set_text(text);
            self.jsdoc.restore_state(state);
        }
    }

    pub fn text(&self) -> &'static str {
        active!(self.text())
    }

    pub fn token(&self) -> Kind {
        active!(self.token())
    }

    pub fn token_flags(&self) -> TokenFlags {
        active!(self.token_flags())
    }

    pub fn token_full_start(&self) -> u32 {
        active!(self.token_full_start())
    }

    pub fn token_start(&self) -> u32 {
        active!(self.token_start())
    }

    pub fn token_end(&self) -> u32 {
        active!(self.token_end())
    }

    pub fn token_text(&self) -> &'static str {
        active!(self.token_text())
    }

    pub fn token_value(&self) -> &'static str {
        active!(self.token_value())
    }

    pub fn token_range(&self) -> TextRange {
        active!(self.token_range())
    }

    pub fn comment_directives(&self) -> &[CommentDirective] {
        active!(self.comment_directives())
    }

    pub fn mark(&self) -> TokensState {
        if self.jsdoc_depth != 0 {
            TokensState::Jsdoc(self.jsdoc.mark())
        } else {
            TokensState::Lex(self.lexer.mark())
        }
    }

    pub fn rewind(&mut self, state: TokensState) {
        match state {
            TokensState::Lex(state) => {
                debug_assert!(self.jsdoc_depth == 0, "a source-text mark rewound inside JSDoc");
                self.lexer.rewind(state);
            }
            TokensState::Jsdoc(state) => {
                debug_assert!(self.jsdoc_depth != 0, "a JSDoc mark rewound outside JSDoc");
                self.jsdoc.rewind(state);
            }
        }
    }

    #[inline]
    pub fn has_errors(&self) -> bool {
        active!(self.has_errors())
    }

    pub fn take_errors(&mut self) -> Vec<ScanError> {
        active!(self.take_errors())
    }

    pub fn reset_pos(&mut self, pos: u32) {
        active!(self.reset_pos(pos));
    }

    pub fn reset_token_state(&mut self, pos: u32) {
        active!(self.reset_token_state(pos));
    }

    /// Only `JSDoc` types skip leading asterisks.
    pub fn set_skip_jsdoc_leading_asterisks(&mut self, skip: bool) {
        debug_assert!(self.jsdoc_depth != 0, "leading asterisks are skipped only inside JSDoc");
        if self.jsdoc_depth != 0 {
            self.jsdoc.set_skip_jsdoc_leading_asterisks(skip);
        }
    }

    pub fn has_unicode_escape(&self) -> bool {
        active!(self.has_unicode_escape())
    }

    pub fn has_extended_unicode_escape(&self) -> bool {
        active!(self.has_extended_unicode_escape())
    }

    pub fn has_preceding_line_break(&self) -> bool {
        active!(self.has_preceding_line_break())
    }

    pub fn has_preceding_jsdoc_comment(&self) -> bool {
        active!(self.has_preceding_jsdoc_comment())
    }

    pub fn has_preceding_jsdoc_leading_asterisks(&self) -> bool {
        self.jsdoc_depth != 0 && self.jsdoc.has_preceding_jsdoc_leading_asterisks()
    }

    pub fn has_preceding_jsdoc_with_deprecated_tag(&self) -> bool {
        active!(self.has_preceding_jsdoc_with_deprecated_tag())
    }

    pub fn has_preceding_jsdoc_with_see_or_link(&self) -> bool {
        active!(self.has_preceding_jsdoc_with_see_or_link())
    }

    pub fn scan(&mut self) -> Kind {
        active!(self.scan())
    }

    pub fn re_scan_less_than_token(&mut self) -> Kind {
        active!(self.re_scan_less_than_token())
    }

    pub fn re_scan_greater_than_token(&mut self) -> Kind {
        active!(self.re_scan_greater_than_token())
    }

    pub fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
        active!(self.re_scan_template_token(is_tagged_template))
    }

    pub fn re_scan_asterisk_equals_token(&mut self) -> Kind {
        active!(self.re_scan_asterisk_equals_token())
    }

    /// The parser does not validate regular expressions (the checker does), so this reports no errors in them.
    pub fn re_scan_slash_token(&mut self) -> Kind {
        if self.jsdoc_depth != 0 {
            self.jsdoc.re_scan_slash_token(false)
        } else {
            self.lexer.re_scan_slash_token()
        }
    }

    pub fn re_scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> Kind {
        active!(self.re_scan_jsx_token(allow_multiline_jsx_text))
    }

    pub fn re_scan_question_token(&mut self) -> Kind {
        active!(self.re_scan_question_token())
    }

    pub fn scan_jsx_token(&mut self) -> Kind {
        active!(self.scan_jsx_token())
    }

    pub fn scan_jsx_identifier(&mut self) -> Kind {
        active!(self.scan_jsx_identifier())
    }

    pub fn scan_jsx_attribute_value(&mut self) -> Kind {
        active!(self.scan_jsx_attribute_value())
    }

    pub fn scan_jsdoc_token(&mut self) -> Kind {
        debug_assert!(self.jsdoc_depth != 0);
        self.jsdoc.scan_jsdoc_token()
    }

    pub fn scan_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> Kind {
        debug_assert!(self.jsdoc_depth != 0);
        self.jsdoc.scan_jsdoc_comment_text_token(in_backticks)
    }

    pub fn re_scan_hash_token(&mut self) -> Kind {
        debug_assert!(self.jsdoc_depth != 0);
        self.jsdoc.re_scan_hash_token()
    }

    pub fn can_follow_jsdoc_at(&self) -> bool {
        debug_assert!(self.jsdoc_depth != 0);
        self.jsdoc.can_follow_jsdoc_at()
    }
}
