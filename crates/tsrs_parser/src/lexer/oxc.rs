//! The `oxc_lexer` token stream and the tokens the parser takes from it unchanged.
//!
//! `oxc_lexer` tokenizes the text a chunk at a time with its own context rules, so its stream can disagree
//! with TypeScript's scanner (which the parser drives token by token): it decides regular expressions and template
//! continuations itself, fuses or splits `<` and `>` differently, recovers from unterminated literals differently
//! and reports no TypeScript diagnostics. TypeScript's scanner is context-free at a token start, though: its token
//! there depends only on the text that follows. So a token is taken from `oxc_lexer` only when it starts exactly
//! where TypeScript's next token starts and its text alone shows that TypeScript would scan the same kind, end,
//! value and flags with no diagnostic. Anything else is scanned by `scan.rs`, until the two streams meet again.

use std::cell::Cell;

use oxc_lexer::{LexOptions, Lexer as OxcLexer, PAD, TokenKind};
use tsrs_ast::{Kind, TokenFlags};
use tsrs_core::LanguageVariant;
use tsrs_scanner::get_identifier_token;

use super::Lexer;
use super::trivia::{ASCII_IDENTIFIER_PART, is_conflict_marker_trivia};

/// An `oxc_lexer` lexer and the zero-padded copy of the chunk it reads, kept for the next chunk lexed on this
/// thread: each file has its own parser, and the lexer's buffers grow with its input. `Lexer::lex` writes the same
/// tokens as `lex_utf8_arena` (whose extra work is copying value lanes and diagnostics out), into buffers it reuses.
struct Scratch {
    lexer: OxcLexer,
    padded: Vec<u8>,
}

thread_local! {
    static SCRATCH: Cell<Option<Box<Scratch>>> = const { Cell::new(None) };
}

/// A chunk ends at the first line break at least this far past its start (or this far, without one), so that
/// `oxc_lexer`'s buffers, several times the size of its input, stay small however large the file is.
const CHUNK_LEN: u32 = 64 * 1024;

/// A token that ends this close to a chunk's cut may be a prefix of a longer one, or `oxc_lexer` may have chosen
/// its kind without the text past the cut, so it is left to the scanner.
const CUT_GUARD: u32 = 4;

/// The `oxc_lexer` tokens of the text (or of a window of it) up to the last chunk lexed, copied out of the lexer.
///
/// A chunk starts where TypeScript's scanner starts a token, and `oxc_lexer` lexes it as if it were a whole
/// program. That matters only to tokens whose meaning depends on what precedes them (regular expressions, template
/// continuations, JSX), which `take_oxc_token` checks against the text like any other token. The tokens of earlier
/// chunks are kept, so that the parser's speculation across a cut does not lex a chunk again.
#[derive(Default)]
pub(super) struct OxcTokens {
    /// The region lexed: `set_window`'s, or the whole text.
    window: Option<(u32, u32)>,
    /// A token that starts outside `lo..hi` lexes the next chunk from it: `lo` is the region's start once a chunk
    /// is lexed, `hi` the last chunk's cut (`u32::MAX` once the region is lexed).
    lo: u32,
    hi: u32,
    /// Tokens of the last chunk that end after `guard` are not taken (and are dropped with the next chunk).
    guard: u32,
    /// Token starts in text order, then `u32::MAX`.
    starts: Vec<u32>,
    ends: Vec<u32>,
    kinds: Vec<TokenKind>,
}

impl OxcTokens {
    pub(super) fn reset(&mut self) {
        self.window = None;
        self.clear();
    }

    fn clear(&mut self) {
        self.lo = 0;
        self.hi = 0;
        self.guard = 0;
        self.starts.clear();
        self.ends.clear();
        self.kinds.clear();
    }

    /// Lexes only `start..end` of the text: a parser that reads one region (a lazy member list) does not pay for
    /// the whole file. `start` must be a token boundary.
    pub(super) fn set_window(&mut self, start: u32, end: u32) {
        self.clear();
        self.window = Some((start, end));
    }

    /// Whether a token at `pos` needs no new chunk. The first scan lexes, so that a parser that only reads `JSDoc`
    /// never does.
    #[inline]
    pub(super) fn covers(&self, pos: u32) -> bool {
        self.lo <= pos && pos < self.hi
    }

    /// Lexes the chunk that starts at `pos`, a token start past the last chunk.
    #[cold]
    pub(super) fn lex_chunk(&mut self, text: &str, language_variant: LanguageVariant, pos: u32) {
        let (region_start, region_end) = self.window.unwrap_or((0, text.len() as u32));
        if pos < region_start {
            return;
        }
        debug_assert!(pos >= self.hi, "chunks are lexed in text order");
        self.starts.pop();
        let kept = self.ends.partition_point(|&end| end <= self.guard);
        self.starts.truncate(kept);
        self.ends.truncate(kept);
        self.kinds.truncate(kept);
        self.lo = region_start;
        if pos >= region_end {
            self.hi = u32::MAX;
            self.starts.push(u32::MAX);
            return;
        }
        let cut = chunk_end(text, pos, region_end);
        (self.hi, self.guard) =
            if cut == region_end { (u32::MAX, u32::MAX) } else { (cut, cut - CUT_GUARD) };
        let source = &text.as_bytes()[pos as usize..cut as usize];
        let options = LexOptions {
            ts: true,
            jsx: language_variant == LanguageVariant::JSX,
            source_type_module: true,
            strip_utf8_bom: false,
            validate_utf8: false,
            ..LexOptions::default()
        };
        let mut scratch = SCRATCH.take().unwrap_or_else(|| {
            Box::new(Scratch { lexer: OxcLexer::new(), padded: Vec::new() })
        });
        let Scratch { lexer, padded } = &mut *scratch;
        padded.clear();
        padded.extend_from_slice(source);
        padded.resize(source.len() + PAD, 0);
        let count = lexer.lex(padded, source.len(), options);
        let spans = &lexer.spans[..count];
        self.kinds.extend_from_slice(&lexer.kinds()[..count]);
        self.starts.reserve(count + 1);
        self.starts.extend(spans.iter().map(|span| span.start + pos));
        self.starts.push(u32::MAX);
        self.ends.extend(spans.iter().map(|span| span.end + pos));
        SCRATCH.set(Some(scratch));
    }

    /// The index of the first token that starts at or after `pos`.
    pub(super) fn index_at_or_after(&self, pos: u32) -> u32 {
        self.starts.partition_point(|&start| start < pos) as u32
    }
}

/// The end of the chunk that starts at `start`: just past a line break at least `CHUNK_LEN` on, else
/// `CHUNK_LEN` on (backed up to a character boundary), else `region_end`.
fn chunk_end(text: &str, start: u32, region_end: u32) -> u32 {
    let target = start.saturating_add(CHUNK_LEN);
    if target >= region_end {
        return region_end;
    }
    let search_end = target.saturating_add(CHUNK_LEN).min(region_end);
    if let Some(offset) = memchr::memchr(b'\n', &text.as_bytes()[target as usize..search_end as usize]) {
        return target + offset as u32 + 1;
    }
    let mut cut = target as usize;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    cut as u32
}

impl Lexer {
    /// The index of the `oxc_lexer` token that starts at `pos`, moving the lookup hint there.
    #[inline]
    fn oxc_token_at(&mut self, pos: u32) -> Option<usize> {
        let starts = &self.oxc.starts;
        if starts.is_empty() {
            return None;
        }
        let mut index = self.state.oxc_index as usize;
        if index >= starts.len() || index > 0 && starts[index - 1] >= pos {
            index = self.oxc.index_at_or_after(pos) as usize;
        }
        while starts[index] < pos {
            index += 1;
        }
        self.state.oxc_index = index as u32;
        (starts[index] == pos).then_some(index)
    }

    /// Takes the `oxc_lexer` token at `token_start` when TypeScript's scanner would scan the same token there.
    #[inline]
    pub(super) fn take_oxc_token(&mut self) -> Option<Kind> {
        let pos = self.state.token_start;
        let index = self.oxc_token_at(pos)?;
        let end = self.oxc.ends[index];
        if end > self.oxc.guard {
            return None;
        }
        let bytes = self.text.as_bytes();
        let text = &bytes[pos as usize..end as usize];
        let next = bytes.get(end as usize).copied();
        let token = match self.oxc.kinds[index] {
            TokenKind::Ident => {
                if !is_ascii_identifier(text) || !ends_identifier(next) {
                    return None;
                }
                self.state.token_value = self.slice(pos, end);
                get_identifier_token(self.state.token_value)
            }
            kind if kind.is_keyword() => {
                if !is_ascii_identifier(text) || !ends_identifier(next) {
                    return None;
                }
                self.state.token_value = self.slice(pos, end);
                get_identifier_token(self.state.token_value)
            }
            TokenKind::PrivateIdent => {
                if !is_ascii_identifier(&text[1..]) || !ends_identifier(next) {
                    return None;
                }
                self.state.token_value = self.slice(pos, end);
                Kind::PrivateIdentifier
            }
            TokenKind::Number => {
                // A decimal integer of at most 15 digits without a leading zero is its own canonical spelling.
                let plain = text == b"0"
                    || text.len() <= 15
                        && matches!(text.first(), Some(b'1'..=b'9'))
                        && text.iter().all(u8::is_ascii_digit);
                if !plain || !ends_identifier(next) || next == Some(b'.') {
                    return None;
                }
                self.state.token_value = self.slice(pos, end);
                Kind::NumericLiteral
            }
            TokenKind::String => {
                let quote = text[0];
                if text.len() < 2
                    || text[text.len() - 1] != quote
                    || memchr::memchr3(b'\\', b'\r', b'\n', &text[1..text.len() - 1]).is_some()
                {
                    return None;
                }
                if quote == b'\'' {
                    self.state.token_flags |= TokenFlags::SingleQuote;
                }
                self.state.token_value =
                    if text.len() == 2 { "" } else { self.slice(pos + 1, end - 1) };
                Kind::StringLiteral
            }
            TokenKind::TemplateNoSub => {
                if text.len() < 2 || text[text.len() - 1] != b'`' {
                    return None;
                }
                self.plain_template_value(pos + 1, end - 1)?;
                Kind::NoSubstitutionTemplateLiteral
            }
            TokenKind::TemplateHead => {
                if text.len() < 3 || !text.ends_with(b"${") {
                    return None;
                }
                self.plain_template_value(pos + 1, end - 2)?;
                Kind::TemplateHead
            }
            kind => self.punctuator(kind, pos, next)?,
        };
        self.state.pos = end;
        Some(token)
    }

    /// Takes the `oxc_lexer` template middle or tail at `token_start` (a `}`) for `re_scan_template_token`.
    pub(super) fn take_oxc_template_continuation(&mut self) -> Option<Kind> {
        let pos = self.state.token_start;
        let index = self.oxc_token_at(pos)?;
        let end = self.oxc.ends[index];
        if end > self.oxc.guard {
            return None;
        }
        let text = &self.text.as_bytes()[pos as usize..end as usize];
        let token = match self.oxc.kinds[index] {
            TokenKind::TemplateMiddle if text.len() >= 3 && text.ends_with(b"${") => {
                self.plain_template_value(pos + 1, end - 2)?;
                Kind::TemplateMiddle
            }
            TokenKind::TemplateTail if text.len() >= 2 && text[text.len() - 1] == b'`' => {
                self.plain_template_value(pos + 1, end - 1)?;
                Kind::TemplateTail
            }
            _ => return None,
        };
        self.state.pos = end;
        Some(token)
    }

    /// Sets the value of a template part without escapes or carriage returns, which is its text.
    fn plain_template_value(&mut self, start: u32, end: u32) -> Option<()> {
        let part = self.slice(start, end);
        if memchr::memchr2(b'\\', b'\r', part.as_bytes()).is_some() {
            return None;
        }
        self.state.token_value = part;
        Some(())
    }

    /// The TypeScript kind of a punctuator, when TypeScript's scanner would scan the same characters as one token.
    /// `>` is always one token in TypeScript's `scan` (the parser rescans it), and a run of the same character at
    /// a line start may be a conflict marker.
    fn punctuator(&self, kind: TokenKind, pos: u32, next: Option<u8>) -> Option<Kind> {
        let not_conflict = || !is_conflict_marker_trivia(self.text, pos);
        let token = match kind {
            TokenKind::LBrace => Kind::OpenBraceToken,
            TokenKind::RBrace => Kind::CloseBraceToken,
            TokenKind::LParen => Kind::OpenParenToken,
            TokenKind::RParen => Kind::CloseParenToken,
            TokenKind::LBracket => Kind::OpenBracketToken,
            TokenKind::RBracket => Kind::CloseBracketToken,
            TokenKind::Dot if !next.is_some_and(|ch| ch.is_ascii_digit()) => Kind::DotToken,
            TokenKind::Ellipsis => Kind::DotDotDotToken,
            TokenKind::Semi => Kind::SemicolonToken,
            TokenKind::Comma => Kind::CommaToken,
            TokenKind::Colon => Kind::ColonToken,
            TokenKind::Question if !matches!(next, Some(b'?' | b'.')) => Kind::QuestionToken,
            TokenKind::OptionalChain if !next.is_some_and(|ch| ch.is_ascii_digit()) => {
                Kind::QuestionDotToken
            }
            TokenKind::Nullish if next != Some(b'=') => Kind::QuestionQuestionToken,
            TokenKind::NullishEq => Kind::QuestionQuestionEqualsToken,
            TokenKind::Arrow => Kind::EqualsGreaterThanToken,
            TokenKind::Lt if !matches!(next, Some(b'<' | b'=' | b'/')) => Kind::LessThanToken,
            TokenKind::Le => Kind::LessThanEqualsToken,
            TokenKind::LShift if next != Some(b'=') && not_conflict() => {
                Kind::LessThanLessThanToken
            }
            TokenKind::LShiftEq if not_conflict() => Kind::LessThanLessThanEqualsToken,
            TokenKind::Gt if next != Some(b'>') => Kind::GreaterThanToken,
            TokenKind::Eq if !matches!(next, Some(b'=' | b'>')) => Kind::EqualsToken,
            TokenKind::EqEq if next != Some(b'=') && not_conflict() => Kind::EqualsEqualsToken,
            TokenKind::EqEqEq if not_conflict() => Kind::EqualsEqualsEqualsToken,
            TokenKind::Bang if next != Some(b'=') => Kind::ExclamationToken,
            TokenKind::BangEq if next != Some(b'=') => Kind::ExclamationEqualsToken,
            TokenKind::BangEqEq => Kind::ExclamationEqualsEqualsToken,
            TokenKind::Plus if !matches!(next, Some(b'+' | b'=')) => Kind::PlusToken,
            TokenKind::PlusPlus => Kind::PlusPlusToken,
            TokenKind::PlusEq => Kind::PlusEqualsToken,
            TokenKind::Minus if !matches!(next, Some(b'-' | b'=')) => Kind::MinusToken,
            TokenKind::MinusMinus => Kind::MinusMinusToken,
            TokenKind::MinusEq => Kind::MinusEqualsToken,
            TokenKind::Star if !matches!(next, Some(b'*' | b'=')) => Kind::AsteriskToken,
            TokenKind::StarStar if next != Some(b'=') => Kind::AsteriskAsteriskToken,
            TokenKind::StarEq => Kind::AsteriskEqualsToken,
            TokenKind::StarStarEq => Kind::AsteriskAsteriskEqualsToken,
            TokenKind::Slash if !matches!(next, Some(b'/' | b'*' | b'=')) => Kind::SlashToken,
            TokenKind::SlashEq => Kind::SlashEqualsToken,
            TokenKind::Percent if next != Some(b'=') => Kind::PercentToken,
            TokenKind::PercentEq => Kind::PercentEqualsToken,
            TokenKind::Amp if !matches!(next, Some(b'&' | b'=')) => Kind::AmpersandToken,
            TokenKind::AmpAmp if next != Some(b'=') => Kind::AmpersandAmpersandToken,
            TokenKind::AmpEq => Kind::AmpersandEqualsToken,
            TokenKind::AmpAmpEq => Kind::AmpersandAmpersandEqualsToken,
            TokenKind::Pipe if !matches!(next, Some(b'|' | b'=')) => Kind::BarToken,
            TokenKind::PipePipe if next != Some(b'=') && not_conflict() => Kind::BarBarToken,
            TokenKind::PipeEq => Kind::BarEqualsToken,
            TokenKind::PipePipeEq if not_conflict() => Kind::BarBarEqualsToken,
            TokenKind::Caret if next != Some(b'=') => Kind::CaretToken,
            TokenKind::CaretEq => Kind::CaretEqualsToken,
            TokenKind::Tilde => Kind::TildeToken,
            TokenKind::At => Kind::AtToken,
            _ => return None,
        };
        Some(token)
    }
}

/// Whether `text` is a non-empty run of `[A-Za-z0-9_$]`, so TypeScript's identifier value is the text itself.
#[inline]
fn is_ascii_identifier(text: &[u8]) -> bool {
    !text.is_empty() && text.iter().all(|&byte| ASCII_IDENTIFIER_PART[byte as usize])
}

/// Whether TypeScript's identifier or number scan stops before `next`: it continues into identifier parts,
/// escapes and (with the Unicode tables) any non-ASCII character.
#[inline]
fn ends_identifier(next: Option<u8>) -> bool {
    next.is_none_or(|byte| byte < 0x80 && byte != b'\\' && !ASCII_IDENTIFIER_PART[byte as usize])
}
