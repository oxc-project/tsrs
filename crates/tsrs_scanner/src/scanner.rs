//! Scanner API notes for consumers (parser, checker):
//!
//! Text and positions. The scanner scans a `&'static str` (for the parser, the file's source text: leaked,
//! embedded, or copied into the current arena or region; other callers pass their own static or `alloc_str` text).
//! Positions are unsigned byte offsets. Scanner EOF is represented separately from rune values.
//!
//! Token values. `token_value()` returns `&'static str`. When the Go scanner would produce a
//! substring of the source text (the overwhelmingly common case: identifiers, keywords, simple
//! strings, numbers that are already canonical) this is a slice of the source text and costs
//! nothing. When the value is computed (escape sequences, numeric separators, normalized numbers,
//! template literals containing `\r`) it is allocated with `alloc_str` in the current allocation
//! target (the thread arena, or the entered region), where the parser also allocates nodes, so the
//! parser can store token values in nodes directly without copying.
//!
//! Errors (Go `ErrorCallback` / `SetOnError`). Go's scanner calls a callback synchronously. In
//! Rust the parser owns the scanner and is itself a `&mut self` state machine, so the scanner
//! instead *buffers* errors: `set_on_error(true)` enables reporting (Go: a non-nil callback),
//! each reported error is appended to an internal `Vec<ScanError>`, and the owner drains it
//! with `take_errors()` (or checks `has_errors()` first, which is the cheap fast path).
//! To preserve Go's diagnostic order exactly, the owner must drain right after every scanner
//! call that may report (`scan`, every `re_scan_*`, `scan_jsx_*`, `scan_jsdoc_*`, ...), before it
//! reports any diagnostics of its own. Message arguments are pre-formatted into `String`s
//! (`ScanError::args`); pass them on as `&[&dyn Display]`.
//!
//! Speculation (Go `Mark` / `Rewind`). `mark()` saves the token state and directive count;
//! `rewind(state)` restores the token state and truncates the directive buffer. These snapshots
//! are for speculation within the current text. Across temporary text changes (`JSDoc` parsing),
//! use `take_state()` / `restore_state()` to move the buffer with the state. This preserves Go's
//! saved directive slice across `set_text` without copying its entries.
//!
//! Buffered errors are not part of either snapshot (in Go they are the callback's business).
//! The owner drains after each scanner call and truncates its own diagnostics on rewind.

use std::borrow::Cow;
use std::fmt::{Display, Write as _};
use std::sync::OnceLock;

use rustc_hash::FxHashMap;
use tsrs_ast as ast;
use tsrs_ast::{
    CommentDirective, CommentDirectiveKind, CommentRange, Kind, Node, NodeFlags, SourceFile,
    SourceFileLike, TokenFlags,
};
use tsrs_core::{
    ArenaStringBuilder, LanguageVariant, P, ScriptTarget, TextRange, UTF16Offset, alloc_str, jsnum,
    stringutil::{self, Rune},
};
use tsrs_diagnostics as diagnostics;
use tsrs_diagnostics::Message;

use crate::regexp::{RegExpParser, RegularExpressionFlags, char_code_to_reg_exp_flag};
use crate::utilities::token_is_identifier_or_keyword;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IdentifierVariant {
    Standard,
    #[expect(
        clippy::upper_case_acronyms,
        reason = "The name matches the upstream Go or protocol vocabulary."
    )]
    JSX,
    RegExpGroupName,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsxWhitespaceState {
    Leading,
    AllWhitespace,
    SeenNonWhitespace,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct EscapeSequenceScanningFlags: u32 {
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

/// A diagnostic reported by the scanner (Go: the arguments of one `ErrorCallback` call).
#[derive(Clone, Debug)]
pub struct ScanError {
    pub message: &'static Message,
    pub start: u32,
    pub length: u32,
    pub args: Vec<String>,
}

macro_rules! keywords {
    ($($text:literal => $kind:ident,)*) => {
        const KEYWORDS: &[(&str, Kind)] = &[$(($text, Kind::$kind),)*];
        pub static TEXT_TO_KEYWORD: &[(&str, Kind)] = KEYWORDS;
    };
}

// Go `textToKeyword` is a map. The port looks keywords up in a perfect hash over the first two bytes, the last
// byte and the length (every keyword has at least two bytes); KEYWORD_HASH_MUL was searched for the current
// keyword list, and the table is built at compile time, which fails if two keywords collide.
const KEYWORD_HASH_MUL: u32 = 0xae85_26c7;
const NO_KEYWORD: u8 = u8::MAX;

#[inline]
const fn keyword_hash(b: &[u8]) -> usize {
    let key = b[0] as u32
        | ((b[1] as u32) << 8)
        | ((b[b.len() - 1] as u32) << 16)
        | ((b.len() as u32) << 24);
    (key.wrapping_mul(KEYWORD_HASH_MUL) >> 24) as usize
}

static KEYWORD_TABLE: [u8; 256] = {
    let mut table = [NO_KEYWORD; 256];
    let mut i = 0;
    while i < KEYWORDS.len() {
        let h = keyword_hash(KEYWORDS[i].0.as_bytes());
        assert!(table[h] == NO_KEYWORD, "keyword hash collision: search a new KEYWORD_HASH_MUL");
        table[h] = i as u8;
        i += 1;
    }
    table
};

/// Go `textToKeyword[s]` (zero value `KindUnknown` when absent).
#[inline]
pub(crate) fn text_to_keyword(s: &str) -> Kind {
    let b = s.as_bytes();
    if b.len() < 2 {
        return Kind::Unknown;
    }
    let i = KEYWORD_TABLE[keyword_hash(b)];
    if i != NO_KEYWORD {
        let (text, kind) = KEYWORDS[i as usize];
        if text.as_bytes() == b {
            return kind;
        }
    }
    Kind::Unknown
}

keywords! {
    "abstract" => AbstractKeyword,
    "accessor" => AccessorKeyword,
    "any" => AnyKeyword,
    "as" => AsKeyword,
    "asserts" => AssertsKeyword,
    "assert" => AssertKeyword,
    "bigint" => BigIntKeyword,
    "boolean" => BooleanKeyword,
    "break" => BreakKeyword,
    "case" => CaseKeyword,
    "catch" => CatchKeyword,
    "class" => ClassKeyword,
    "continue" => ContinueKeyword,
    "const" => ConstKeyword,
    "constructor" => ConstructorKeyword,
    "debugger" => DebuggerKeyword,
    "declare" => DeclareKeyword,
    "default" => DefaultKeyword,
    "defer" => DeferKeyword,
    "delete" => DeleteKeyword,
    "do" => DoKeyword,
    "else" => ElseKeyword,
    "enum" => EnumKeyword,
    "export" => ExportKeyword,
    "extends" => ExtendsKeyword,
    "false" => FalseKeyword,
    "finally" => FinallyKeyword,
    "for" => ForKeyword,
    "from" => FromKeyword,
    "function" => FunctionKeyword,
    "get" => GetKeyword,
    "if" => IfKeyword,
    "immediate" => ImmediateKeyword,
    "implements" => ImplementsKeyword,
    "import" => ImportKeyword,
    "in" => InKeyword,
    "infer" => InferKeyword,
    "instanceof" => InstanceOfKeyword,
    "interface" => InterfaceKeyword,
    "intrinsic" => IntrinsicKeyword,
    "is" => IsKeyword,
    "keyof" => KeyOfKeyword,
    "let" => LetKeyword,
    "module" => ModuleKeyword,
    "namespace" => NamespaceKeyword,
    "never" => NeverKeyword,
    "new" => NewKeyword,
    "null" => NullKeyword,
    "number" => NumberKeyword,
    "object" => ObjectKeyword,
    "package" => PackageKeyword,
    "private" => PrivateKeyword,
    "protected" => ProtectedKeyword,
    "public" => PublicKeyword,
    "override" => OverrideKeyword,
    "out" => OutKeyword,
    "readonly" => ReadonlyKeyword,
    "require" => RequireKeyword,
    "global" => GlobalKeyword,
    "return" => ReturnKeyword,
    "satisfies" => SatisfiesKeyword,
    "set" => SetKeyword,
    "source" => SourceKeyword,
    "static" => StaticKeyword,
    "string" => StringKeyword,
    "super" => SuperKeyword,
    "switch" => SwitchKeyword,
    "symbol" => SymbolKeyword,
    "this" => ThisKeyword,
    "throw" => ThrowKeyword,
    "true" => TrueKeyword,
    "try" => TryKeyword,
    "type" => TypeKeyword,
    "typeof" => TypeOfKeyword,
    "undefined" => UndefinedKeyword,
    "unique" => UniqueKeyword,
    "unknown" => UnknownKeyword,
    "using" => UsingKeyword,
    "var" => VarKeyword,
    "void" => VoidKeyword,
    "while" => WhileKeyword,
    "with" => WithKeyword,
    "yield" => YieldKeyword,
    "async" => AsyncKeyword,
    "await" => AwaitKeyword,
    "of" => OfKeyword,
}

static TEXT_TO_PUNCTUATION: &[(&str, Kind)] = &[
    ("{", Kind::OpenBraceToken),
    ("}", Kind::CloseBraceToken),
    ("(", Kind::OpenParenToken),
    (")", Kind::CloseParenToken),
    ("[", Kind::OpenBracketToken),
    ("]", Kind::CloseBracketToken),
    (".", Kind::DotToken),
    ("...", Kind::DotDotDotToken),
    (";", Kind::SemicolonToken),
    (",", Kind::CommaToken),
    ("<", Kind::LessThanToken),
    (">", Kind::GreaterThanToken),
    ("<=", Kind::LessThanEqualsToken),
    (">=", Kind::GreaterThanEqualsToken),
    ("==", Kind::EqualsEqualsToken),
    ("!=", Kind::ExclamationEqualsToken),
    ("===", Kind::EqualsEqualsEqualsToken),
    ("!==", Kind::ExclamationEqualsEqualsToken),
    ("=>", Kind::EqualsGreaterThanToken),
    ("+", Kind::PlusToken),
    ("-", Kind::MinusToken),
    ("**", Kind::AsteriskAsteriskToken),
    ("*", Kind::AsteriskToken),
    ("/", Kind::SlashToken),
    ("%", Kind::PercentToken),
    ("++", Kind::PlusPlusToken),
    ("--", Kind::MinusMinusToken),
    ("<<", Kind::LessThanLessThanToken),
    ("</", Kind::LessThanSlashToken),
    (">>", Kind::GreaterThanGreaterThanToken),
    (">>>", Kind::GreaterThanGreaterThanGreaterThanToken),
    ("&", Kind::AmpersandToken),
    ("|", Kind::BarToken),
    ("^", Kind::CaretToken),
    ("!", Kind::ExclamationToken),
    ("~", Kind::TildeToken),
    ("&&", Kind::AmpersandAmpersandToken),
    ("||", Kind::BarBarToken),
    ("?", Kind::QuestionToken),
    ("??", Kind::QuestionQuestionToken),
    ("?.", Kind::QuestionDotToken),
    (":", Kind::ColonToken),
    ("=", Kind::EqualsToken),
    ("+=", Kind::PlusEqualsToken),
    ("-=", Kind::MinusEqualsToken),
    ("*=", Kind::AsteriskEqualsToken),
    ("**=", Kind::AsteriskAsteriskEqualsToken),
    ("/=", Kind::SlashEqualsToken),
    ("%=", Kind::PercentEqualsToken),
    ("<<=", Kind::LessThanLessThanEqualsToken),
    (">>=", Kind::GreaterThanGreaterThanEqualsToken),
    (">>>=", Kind::GreaterThanGreaterThanGreaterThanEqualsToken),
    ("&=", Kind::AmpersandEqualsToken),
    ("|=", Kind::BarEqualsToken),
    ("^=", Kind::CaretEqualsToken),
    ("||=", Kind::BarBarEqualsToken),
    ("&&=", Kind::AmpersandAmpersandEqualsToken),
    ("??=", Kind::QuestionQuestionEqualsToken),
    ("@", Kind::AtToken),
    ("#", Kind::HashToken),
    ("`", Kind::BacktickToken),
];

fn text_to_token() -> &'static FxHashMap<&'static str, Kind> {
    static M: OnceLock<FxHashMap<&'static str, Kind>> = OnceLock::new();
    M.get_or_init(|| {
        let mut m = FxHashMap::default();
        m.extend(TEXT_TO_PUNCTUATION.iter().map(|&(text, kind)| (text, kind)));
        m.extend(TEXT_TO_KEYWORD.iter().map(|&(text, kind)| (text, kind)));
        m
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScannerState {
    pub(crate) pos: u32, // Current position in text (and ending position of current token)
    full_start_pos: u32, // Starting position of current token including preceding whitespace
    pub(crate) token_start: u32, // Starting position of non-whitespace part of current token
    token: Kind,         // Kind of current token
    pub(crate) token_value: &'static str, // Parsed value of current token
    token_flags: TokenFlags, // Flags for current token
    comment_directives_len: usize, // Snapshot of the number of comment directives collected so far
    skip_jsdoc_leading_asterisks: u32, // Leading asterisks to skip when scanning types inside JSDoc. Should be 0 outside JSDoc
}

/// An owning snapshot for temporary text changes, unlike a `mark()` snapshot.
#[must_use]
pub struct SavedScannerState {
    state: ScannerState,
    comment_directives: Vec<CommentDirective>,
}

#[derive(Default)]
pub struct Scanner {
    pub(crate) text: &'static str,
    end: u32,
    language_variant: LanguageVariant,
    script_target: ScriptTarget,
    on_error: bool,
    errors: Vec<ScanError>,
    skip_trivia: bool,
    pub(crate) state: ScannerState,
    comment_directives: Vec<CommentDirective>,

    number_cache: FxHashMap<&'static str, &'static str>,
    hex_number_cache: FxHashMap<&'static str, &'static str>,
    hex_digit_cache: FxHashMap<&'static str, &'static str>,
}

// Like Oxc's byte handlers, dispatch out of the token loop so simple tokens do not
// pay for the stack frame of numeric, comment, and Unicode scanning. None is the
// old `continue 'scan` for skipped trivia; Some returns the token kind directly.
type ByteHandler = fn(&mut Scanner) -> Option<Kind>;

static BYTE_HANDLERS: [ByteHandler; 256] = {
    let mut table = [Scanner::scan_byte_default as ByteHandler; 256];
    let mut byte = 0;
    while byte < table.len() {
        table[byte] = match byte as u8 {
            0x09 | 0x0B | 0x0C | 0x20 => Scanner::scan_byte_whitespace,
            0x0A | 0x0D => Scanner::scan_byte_line_break,
            0x21 /* ! */ => Scanner::scan_byte_exclamation,
            0x22 | 0x27 /* " ' */ => Scanner::scan_byte_string,
            0x60 /* ` */ => Scanner::scan_byte_template,
            0x25 /* % */ => Scanner::scan_byte_percent,
            0x26 /* & */ => Scanner::scan_byte_ampersand,
            0x28 /* ( */ => Scanner::scan_byte_open_paren,
            0x29 /* ) */ => Scanner::scan_byte_close_paren,
            0x2A /* * */ => Scanner::scan_byte_asterisk,
            0x2B /* + */ => Scanner::scan_byte_plus,
            0x2C /* , */ => Scanner::scan_byte_comma,
            0x2D /* - */ => Scanner::scan_byte_minus,
            0x2E /* . */ => Scanner::scan_byte_dot,
            0x2F /* / */ => Scanner::scan_byte_slash,
            0x30..=0x39 /* 0-9 */ => Scanner::scan_byte_number,
            0x3A /* : */ => Scanner::scan_byte_colon,
            0x3B /* ; */ => Scanner::scan_byte_semicolon,
            0x3C /* < */ => Scanner::scan_byte_less_than,
            0x3D /* = */ => Scanner::scan_byte_equals,
            0x3E /* > */ => Scanner::scan_byte_greater_than,
            0x3F /* ? */ => Scanner::scan_byte_question,
            0x5B /* [ */ => Scanner::scan_byte_open_bracket,
            0x5D /* ] */ => Scanner::scan_byte_close_bracket,
            0x5E /* ^ */ => Scanner::scan_byte_caret,
            0x7B /* { */ => Scanner::scan_byte_open_brace,
            0x7C /* | */ => Scanner::scan_byte_bar,
            0x7D /* } */ => Scanner::scan_byte_close_brace,
            0x7E /* ~ */ => Scanner::scan_byte_tilde,
            0x40 /* @ */ => Scanner::scan_byte_at,
            0x5C /* \ */ => Scanner::scan_byte_identifier_slow,
            0x23 /* # */ => Scanner::scan_byte_hash,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => Scanner::scan_byte_identifier,
            _ => Scanner::scan_byte_default,
        };
        byte += 1;
    }
    table
};

const RUNE_SELF: Rune = 0x80;

/// `[A-Za-z0-9_$]` by byte.
static ASCII_IDENTIFIER_PART: [bool; 256] = {
    let mut t = [false; 256];
    let mut b = 0;
    while b < 128 {
        let c = b as u8;
        t[b] = c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
        b += 1;
    }
    t
};

static ASCII_JSDOC_COMMENT_TEXT: [[bool; 256]; 2] = {
    let mut tables = [[false; 256]; 2];
    let mut byte = 0;
    while byte < 128 {
        let is_text = !matches!(byte as u8, b'\r' | b'\n' | b'`');
        tables[0][byte] = is_text && !matches!(byte as u8, b'{' | b'@');
        tables[1][byte] = is_text;
        byte += 1;
    }
    tables
};
pub(crate) const RUNE_ERROR: Rune = 0xFFFD;

static STRING_LITERAL_END: [[bool; 256]; 2] = {
    let mut table = [[false; 256]; 2];
    let mut i = 0;
    while i < table.len() {
        table[i][b'\\' as usize] = true;
        table[i][b'\r' as usize] = true;
        table[i][b'\n' as usize] = true;
        i += 1;
    }
    table[0][b'\'' as usize] = true;
    table[1][b'"' as usize] = true;
    table
};

/// Go `utf8.DecodeRuneInString` over raw bytes: `(RuneError, 0)` for empty input,
/// `(RuneError, 1)` for an invalid encoding.
#[inline]
pub(crate) fn decode_rune(b: &[u8]) -> (Rune, usize) {
    let Some(&b0) = b.first() else {
        return (RUNE_ERROR, 0);
    };
    if b0 < 0x80 {
        return (u32::from(b0), 1);
    }
    decode_rune_slow(b)
}

#[cold]
fn decode_rune_slow(b: &[u8]) -> (Rune, usize) {
    let b0 = b[0] as u32;
    let cont = |i: usize, lo: u8, hi: u8| -> Option<u32> {
        let c = *b.get(i)?;
        if c < lo || c > hi {
            return None;
        }
        Some((c & 0x3F) as u32)
    };
    let (r, size) = match b0 {
        0xC2..=0xDF => match cont(1, 0x80, 0xBF) {
            Some(c1) => (((b0 & 0x1F) << 6) | c1, 2),
            None => (RUNE_ERROR, 1),
        },
        0xE0..=0xEF => {
            let (lo, hi) = match b0 {
                0xE0 => (0xA0, 0xBF),
                0xED => (0x80, 0x9F),
                _ => (0x80, 0xBF),
            };
            match (cont(1, lo, hi), cont(2, 0x80, 0xBF)) {
                (Some(c1), Some(c2)) => (((b0 & 0x0F) << 12) | (c1 << 6) | c2, 3),
                _ => (RUNE_ERROR, 1),
            }
        }
        0xF0..=0xF4 => {
            let (lo, hi) = match b0 {
                0xF0 => (0x90, 0xBF),
                0xF4 => (0x80, 0x8F),
                _ => (0x80, 0xBF),
            };
            match (cont(1, lo, hi), cont(2, 0x80, 0xBF), cont(3, 0x80, 0xBF)) {
                (Some(c1), Some(c2), Some(c3)) => {
                    (((b0 & 0x07) << 18) | (c1 << 12) | (c2 << 6) | c3, 4)
                }
                _ => (RUNE_ERROR, 1),
            }
        }
        _ => (RUNE_ERROR, 1),
    };
    (r, size)
}

/// Go `utf8.DecodeLastRuneInString`.
fn decode_last_rune(b: &[u8]) -> (Rune, usize) {
    let end = b.len();
    if end == 0 {
        return (RUNE_ERROR, 0);
    }
    let last = b[end - 1];
    if last < 0x80 {
        return (u32::from(last), 1);
    }
    let lim = end.saturating_sub(4);
    let mut start = end - 1;
    while start > lim {
        if b[start] & 0xC0 != 0x80 {
            break;
        }
        start -= 1;
    }
    let (r, size) = decode_rune(&b[start..end]);
    if start + size != end {
        return (RUNE_ERROR, 1);
    }
    (r, size)
}

/// Go `string(r)`: invalid code points (surrogates, out of range) become U+FFFD.
#[inline]
fn rune_to_char(r: Rune) -> char {
    char::from_u32(r).unwrap_or('\u{FFFD}')
}

#[inline]
pub(crate) fn rune_string(r: Rune) -> String {
    rune_to_char(r).to_string()
}

#[path = "scanner_impl.rs"]
mod owned;
pub use owned::{
    OwnedScanner, OwnedScannerState, SavedOwnedScannerState, SavedScannerStateImpl, ScannerImpl,
    ScannerStateImpl,
};

include!("scanner_legacy_generated.rs");

/// Go `strconv.ParseInt(s, base, 32)` ignoring the error (clamped on overflow).
fn parse_int_32(s: &str, base: u32) -> i64 {
    match i64::from_str_radix(s, base) {
        Ok(v) => v.clamp(i32::MIN as i64, i32::MAX as i64),
        Err(_) => {
            if !s.is_empty() && s.bytes().all(|b| (b as char).is_digit(base)) {
                i32::MAX as i64
            } else {
                0
            }
        }
    }
}

/// Go `strconv.ParseInt(s, base, 64)` ignoring the error (clamped on overflow).
fn parse_int_64(s: &str, base: u32) -> i64 {
    match i64::from_str_radix(s, base) {
        Ok(v) => v,
        Err(_) => {
            if !s.is_empty() && s.bytes().all(|b| (b as char).is_digit(base)) {
                i64::MAX
            } else {
                0
            }
        }
    }
}

// hasJSDocTag reports whether text starts with one of the given tag names followed
// by a valid JSDoc tag terminator (whitespace, '}', '*', or end-of-string).
fn has_jsdoc_tag(text: &[u8], tags: &[&str]) -> bool {
    if tags.iter().any(|tag| {
        let tag = tag.as_bytes();
        if !text.starts_with(tag) {
            return false;
        }
        if text.len() == tag.len() {
            return true;
        }
        let ch = text[tag.len()];
        ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' || ch == b'}' || ch == b'*'
    }) {
        return true;
    }
    false
}

#[inline]
pub fn get_identifier_token(str: &str) -> Kind {
    let b = str.as_bytes();
    if b.len() >= 2 && b.len() <= 12 && b[0] >= b'a' && b[0] <= b'z' {
        let keyword = text_to_keyword(str);
        if keyword != Kind::Unknown {
            return keyword;
        }
    }
    Kind::Identifier
}

pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    !(s.char_indices().into_iter().any(|(i, ch)| {
        i == 0 && !is_identifier_start(ch as Rune) || i != 0 && !is_identifier_part(ch as Rune)
    }))
}

// Section 6.1.4
pub(crate) fn is_word_character(ch: Rune) -> bool {
    stringutil::is_ascii_letter(ch) || stringutil::is_digit(ch) || ch == '_' as Rune
}

#[inline]
pub fn is_identifier_start(ch: Rune) -> bool {
    stringutil::is_ascii_letter(ch)
        || ch == '_' as Rune
        || ch == '$' as Rune
        || ch >= RUNE_SELF && stringutil::is_unicode_identifier_start(ch)
}

#[inline]
pub fn is_identifier_part(ch: Rune) -> bool {
    is_identifier_part_ex(ch, LanguageVariant::Standard)
}

#[inline]
pub(crate) fn is_identifier_part_ex(ch: Rune, language_variant: LanguageVariant) -> bool {
    is_word_character(ch)
        || ch == '$' as Rune
        || ch >= RUNE_SELF && stringutil::is_unicode_identifier_part(ch)
        || language_variant == LanguageVariant::JSX && ch == '-' as Rune // ":" is part of JSXNamespacedName, but not JSXIdentifier.
}

fn token_to_text() -> &'static [&'static str] {
    static T: OnceLock<Vec<&'static str>> = OnceLock::new();
    T.get_or_init(|| {
        // Go fills this from the textToToken map; no two texts share a kind, so the tables give the same result.
        let mut result = vec![""; Kind::Count as usize];
        for &(text, kind) in TEXT_TO_PUNCTUATION.iter().chain(TEXT_TO_KEYWORD) {
            result[kind as usize] = text;
        }
        result
    })
}

pub fn token_to_string(token: Kind) -> &'static str {
    token_to_text()[token as usize]
}

pub fn string_to_token(s: &str) -> Kind {
    match text_to_token().get(s) {
        Some(&kind) => kind,
        None => Kind::Unknown,
    }
}

pub fn get_viable_keyword_suggestions() -> Vec<&'static str> {
    let mut result = Vec::with_capacity(TEXT_TO_KEYWORD.len());
    result.extend(
        TEXT_TO_KEYWORD
            .iter()
            .filter_map(|&(text, _)| if text.len() > 2 { Some(text) } else { None }),
    );
    result
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SkipTriviaOptions {
    pub stop_after_line_break: bool,
    pub stop_at_comments: bool,
    pub in_jsdoc: bool,
}

pub fn skip_trivia(text: &str, pos: u32) -> u32 {
    skip_trivia_ex(text, pos, None)
}

pub fn skip_trivia_ex(text: &str, pos: u32, options: Option<&SkipTriviaOptions>) -> u32 {
    let default_options = SkipTriviaOptions::default();
    let options = options.unwrap_or(&default_options);

    let bytes = text.as_bytes();
    let text_len = bytes.len() as u32;
    let mut pos = pos;
    let mut can_consume_star = false;
    // Keep in sync with couldStartTrivia
    loop {
        if pos >= text_len {
            return pos;
        }
        let (ch, size) = decode_rune(&bytes[pos as usize..]);
        match ch {
            0x0D | 0x0A => {
                if ch == '\r' as Rune && pos + 1 < text_len && bytes[(pos + 1) as usize] == b'\n' {
                    pos += 1;
                }
                pos += 1;
                if options.stop_after_line_break {
                    return pos;
                }
                can_consume_star = options.in_jsdoc;
                continue;
            }
            0x09 | 0x0B | 0x0C | 0x20 => {
                pos += 1;
                continue;
            }
            0x2F /* / */ => {
                if !options.stop_at_comments && pos + 1 < text_len {
                    if bytes[(pos + 1) as usize] == b'/' {
                        pos += 2;
                        while pos < text_len {
                            let (ch, size) = decode_rune(&bytes[pos as usize..]);
                            if stringutil::is_line_break(ch) {
                                break;
                            }
                            pos += size as u32;
                        }
                        can_consume_star = false;
                        continue;
                    }
                    if bytes[(pos + 1) as usize] == b'*' {
                        pos += 2;
                        while pos < text_len {
                            if bytes[pos as usize] == b'*' && (pos + 1 < text_len) && bytes[(pos + 1) as usize] == b'/' {
                                pos += 2;
                                break;
                            }
                            let (_, size) = decode_rune(&bytes[pos as usize..]);
                            pos += size as u32;
                        }
                        can_consume_star = false;
                        continue;
                    }
                }
            }
            0x3C | 0x7C | 0x3D | 0x3E /* < | = > */ => {
                if is_conflict_marker_trivia(text, pos) {
                    pos = scan_conflict_marker_trivia(text, pos, None);
                    can_consume_star = false;
                    continue;
                }
            }
            0x23 /* # */ => {
                if pos == 0 && is_shebang_trivia(text, pos) {
                    pos = scan_shebang_trivia(text, pos);
                    can_consume_star = false;
                    continue;
                }
            }
            0x2A /* * */ => {
                if can_consume_star {
                    pos += 1;
                    can_consume_star = false;
                    continue;
                }
            }
            _ => {
                if ch > MAX_ASCII_CHARACTER as Rune && stringutil::is_white_space_like(ch) {
                    pos += size as u32;
                    continue;
                }
            }
        }
        return pos;
    }
}

// All conflict markers consist of the same character repeated seven times.  If it is
// a <<<<<<< or >>>>>>> marker then it is also followed by a space.
const MERGE_CONFLICT_MARKER_LENGTH: u32 = "<<<<<<<".len() as u32;
const MAX_ASCII_CHARACTER: u8 = 127;

fn is_conflict_marker_trivia(text: &str, pos: u32) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len() as u32;

    // Fast reject: a conflict marker is the same byte repeated seven times. If the
    // second byte differs (the overwhelmingly common case for `<`, `>`, `=`, `|`
    // tokens), it cannot be a marker, so skip the line-start check entirely.
    if pos + 1 >= len || bytes[(pos + 1) as usize] != bytes[pos as usize] {
        return false;
    }

    // Conflict markers must be at the start of a line.
    let mut at_line_start =
        pos == 0 || stringutil::is_line_break(u32::from(bytes[(pos - 1) as usize]));
    if !at_line_start && pos >= 2 {
        let (prev, _) = decode_last_rune(&bytes[..(pos - 2) as usize]);
        at_line_start = stringutil::is_line_break(prev);
    }
    if at_line_start {
        let ch = bytes[pos as usize];

        if (pos + MERGE_CONFLICT_MARKER_LENGTH) < len {
            if (0..MERGE_CONFLICT_MARKER_LENGTH).any(|i| bytes[(pos + i) as usize] != ch) {
                return false;
            }

            return ch == b'=' || bytes[(pos + MERGE_CONFLICT_MARKER_LENGTH) as usize] == b' ';
        }
    }

    false
}

fn scan_conflict_marker_trivia(
    text: &str,
    pos: u32,
    report_error: Option<&mut dyn FnMut(&'static Message, u32, u32)>,
) -> u32 {
    if let Some(report_error) = report_error {
        report_error(
            &diagnostics::MERGE_CONFLICT_MARKER_ENCOUNTERED,
            pos,
            MERGE_CONFLICT_MARKER_LENGTH,
        );
    }
    let bytes = text.as_bytes();
    let mut pos = pos;
    let (mut ch, mut size) = decode_rune(&bytes[pos as usize..]);
    let length = bytes.len() as u32;

    if ch == '<' as Rune || ch == '>' as Rune {
        while pos < length && !stringutil::is_line_break(ch) {
            pos += size as u32;
            (ch, size) = decode_rune(&bytes[pos as usize..]);
        }
    } else {
        assert!(
            !(ch != '|' as Rune && ch != '=' as Rune),
            "Assertion failed: ch must be either '|' or '='"
        );
        // Consume everything from the start of a ||||||| or ======= marker to the start
        // of the next ======= or >>>>>>> marker.
        while pos < length {
            let current_char = bytes[pos as usize];
            if (current_char == b'=' || current_char == b'>')
                && u32::from(current_char) != ch
                && is_conflict_marker_trivia(text, pos)
            {
                break;
            }

            pos += 1;
        }
    }

    pos
}

fn is_shebang_trivia(text: &str, pos: u32) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    assert!(pos == 0, "Shebangs check must only be done at the start of the file");
    bytes[0] == b'#' && bytes[1] == b'!'
}

fn scan_shebang_trivia(text: &str, pos: u32) -> u32 {
    let bytes = text.as_bytes();
    let mut pos = pos + 2;
    while (pos as usize) < bytes.len() {
        let (ch, size) = decode_rune(&bytes[pos as usize..]);
        if stringutil::is_line_break(ch) {
            break;
        }
        pos += size as u32;
    }
    pos
}

pub fn get_shebang(text: &str) -> &str {
    if !is_shebang_trivia(text, 0) {
        return "";
    }

    let end = scan_shebang_trivia(text, 0);
    &text[..end as usize]
}

pub fn get_scanner_for_source_file(source_file: P<SourceFile>, pos: u32) -> Scanner {
    let mut s = Scanner::new();
    s.text = source_file.text();
    s.state.pos = pos;
    s.end = s.text.len() as u32;
    s.language_variant = source_file.language_variant();
    s.scan();
    s
}

pub fn scan_token_at_position(source_file: P<SourceFile>, pos: u32) -> Kind {
    let s = get_scanner_for_source_file(source_file, pos);
    s.state.token
}

pub fn get_range_of_token_at_position(source_file: P<SourceFile>, pos: u32) -> TextRange {
    let s = get_scanner_for_source_file(source_file, pos);
    TextRange::new(s.state.token_start, s.state.pos)
}

pub fn get_token_pos_of_node(
    node: P<Node>,
    source_file: P<SourceFile>,
    include_jsdoc: bool,
) -> u32 {
    // With nodes that have no width (i.e. 'Missing' nodes), we actually *don't*
    // want to skip trivia because this will launch us forward to the next token.
    if ast::node_is_missing(Some(node)) {
        return node.pos().expect_source("parsed nodes have source positions");
    }
    if ast::is_jsdoc_node(node) || node.kind() == Kind::JsxText {
        // JsxText cannot actually contain comments, even though the scanner will think it sees comments
        return skip_trivia_ex(
            source_file.text(),
            node.pos().expect_source("parsed nodes have source positions"),
            Some(&SkipTriviaOptions { stop_at_comments: true, ..Default::default() }),
        );
    }
    if include_jsdoc {
        let jsdoc = node.jsdoc(Some(source_file.get()));
        if !jsdoc.is_empty() {
            return get_token_pos_of_node(jsdoc[0], source_file, false /*includeJSDoc*/);
        }
    }
    skip_trivia_ex(
        source_file.text(),
        node.pos().expect_source("parsed nodes have source positions"),
        Some(&SkipTriviaOptions {
            in_jsdoc: node.flags().intersects(NodeFlags::JSDoc),
            ..Default::default()
        }),
    )
}

fn get_error_range_for_arrow_function(source_file: P<SourceFile>, node: P<Node>) -> TextRange {
    let pos = skip_trivia(
        source_file.text(),
        node.pos().expect_source("parsed nodes have source positions"),
    );
    let body = node.body();
    if let Some(body) = body
        && body.kind() == Kind::Block
    {
        let start_line = get_ecma_line_of_position(
            &*source_file,
            body.pos().expect_source("parsed nodes have source positions"),
        );
        let end_line = get_ecma_line_of_position(
            &*source_file,
            body.end().expect_source("parsed nodes have source positions"),
        );
        if start_line < end_line {
            // The arrow function spans multiple lines, make the error span be the first line, inclusive.
            let end =
                get_ecma_end_line_position(source_file, start_line).map_or(pos, |end| end + 1);
            return TextRange::new(pos, end);
        }
    }
    TextRange::new(pos, node.end().expect_source("parsed nodes have source positions"))
}

fn find_originating_jsdoc_satisfies_tag(
    source_file: P<SourceFile>,
    node: P<Node>,
) -> Option<P<Node>> {
    let target_type = node.as_satisfies_expression().type_;
    if !target_type.flags().intersects(NodeFlags::Reparsed) {
        return None;
    }
    let mut current = node.parent();
    while let Some(cur) = current {
        current = cur.parent();
        if !cur.flags().intersects(NodeFlags::HasJSDoc) {
            continue;
        }
        let mut first_satisfies_tag: Option<P<Node>> = None;
        for &js_doc in cur.eager_jsdoc(Some(source_file.get())) {
            if let Some(tags) = js_doc.as_jsdoc().tags {
                for &tag in tags.nodes() {
                    if !ast::is_jsdoc_satisfies_tag(tag) {
                        continue;
                    }
                    if first_satisfies_tag.is_none() {
                        first_satisfies_tag = Some(tag);
                    }
                    let type_expr = tag.as_jsdoc_satisfies_tag().type_expression;
                    if let Some(t) = type_expr.type_node()
                        && t.loc() == target_type.loc()
                    {
                        return Some(tag);
                    }
                }
            }
        }
        return first_satisfies_tag;
    }
    None
}

pub fn get_error_range_for_node(source_file: P<SourceFile>, node: P<Node>) -> TextRange {
    let mut error_node = Some(node);
    match node.kind() {
        Kind::SourceFile => {
            let pos = skip_trivia(source_file.text(), 0);
            if pos as usize == source_file.text().len() {
                return TextRange::new(0, 0);
            }
            return get_range_of_token_at_position(source_file, pos);
        }
        // This list is a work in progress. Add missing node kinds to improve their error spans
        Kind::FunctionDeclaration
        | Kind::MethodDeclaration
        | Kind::VariableDeclaration
        | Kind::BindingElement
        | Kind::ClassDeclaration
        | Kind::InterfaceDeclaration
        | Kind::ModuleDeclaration
        | Kind::EnumDeclaration
        | Kind::EnumMember
        | Kind::FunctionExpression
        | Kind::GetAccessor
        | Kind::SetAccessor
        | Kind::TypeAliasDeclaration
        | Kind::JSTypeAliasDeclaration
        | Kind::PropertyDeclaration
        | Kind::PropertySignature
        | Kind::NamespaceImport => {
            if (node.kind() == Kind::FunctionDeclaration || node.kind() == Kind::MethodDeclaration)
                && node.flags().intersects(NodeFlags::Reparsed)
            {
                error_node = Some(node);
            } else {
                error_node = ast::get_name_of_declaration(Some(node));
            }
        }
        Kind::ClassExpression => {
            error_node = node.name();
        }
        Kind::ArrowFunction => {
            return get_error_range_for_arrow_function(source_file, node);
        }
        Kind::CaseClause | Kind::DefaultClause => {
            let start = skip_trivia(
                source_file.text(),
                node.pos().expect_source("parsed nodes have source positions"),
            );
            let mut end = node.end().expect_source("parsed nodes have source positions");
            let statements = node.statements();
            if !statements.is_empty() {
                end = statements[0].pos().expect_source("parsed nodes have source positions");
            }
            return TextRange::new(start, end);
        }
        Kind::ReturnStatement | Kind::YieldExpression => {
            let pos = skip_trivia(
                source_file.text(),
                node.pos().expect_source("parsed nodes have source positions"),
            );
            return get_range_of_token_at_position(source_file, pos);
        }
        Kind::SatisfiesExpression => {
            if let Some(js_doc_satisfies_tag) =
                find_originating_jsdoc_satisfies_tag(source_file, node)
            {
                let pos = skip_trivia(
                    source_file.text(),
                    js_doc_satisfies_tag
                        .tag_name()
                        .pos()
                        .expect_source("parsed nodes have source positions"),
                );
                return get_range_of_token_at_position(source_file, pos);
            }
            let pos = skip_trivia(
                source_file.text(),
                node.as_satisfies_expression()
                    .expression
                    .end()
                    .expect_source("parsed nodes have source positions"),
            );
            return get_range_of_token_at_position(source_file, pos);
        }
        Kind::Constructor => {
            if node.flags().intersects(NodeFlags::Reparsed) {
                error_node = Some(node);
            } else {
                let mut scanner = get_scanner_for_source_file(
                    source_file,
                    node.pos().expect_source("parsed nodes have source positions"),
                );
                let start = scanner.token_start();
                while scanner.token() != Kind::ConstructorKeyword
                    && scanner.token() != Kind::StringLiteral
                    && scanner.token() != Kind::EndOfFile
                {
                    scanner.scan();
                }
                return TextRange::new(start, scanner.token_end());
            }
        }
        _ => {}
    }
    let Some(error_node) = error_node else {
        // If we don't have a better node, then just set the error on the first token of
        // construct.
        return get_range_of_token_at_position(
            source_file,
            node.pos().expect_source("parsed nodes have source positions"),
        );
    };
    let mut pos = error_node.pos().expect_source("parsed nodes have source positions");
    if !ast::node_is_missing(Some(error_node)) && !ast::is_jsx_text(error_node) {
        pos = skip_trivia(source_file.text(), pos);
    }
    TextRange::new(pos, error_node.end().expect_source("parsed nodes have source positions"))
}

pub fn compute_line_of_position(line_starts: &[u32], pos: u32) -> usize {
    let mut low: usize = 0;
    let mut high: usize = line_starts.len() - 1;
    while low <= high {
        let middle = low + ((high - low) >> 1);
        let value = line_starts[middle];
        match value.cmp(&pos) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => {
                if middle == 0 {
                    return 0;
                }
                high = middle - 1;
            }
            std::cmp::Ordering::Equal => return middle,
        }
    }
    low.saturating_sub(1)
}

pub fn get_ecma_line_starts<S: SourceFileLike + ?Sized>(source_file: &S) -> &[u32] {
    source_file.ecma_line_map()
}

pub fn get_ecma_line_of_position(source_file: &(impl SourceFileLike + ?Sized), pos: u32) -> usize {
    let line_map = get_ecma_line_starts(source_file);
    compute_line_of_position(line_map, pos)
}

// GetECMALineAndUTF16CharacterOfPosition returns the 0-based line number and the
// UTF-16 code unit offset from the start of that line for the given byte position.
// Uses ECMAScript line separators (LF, CR, CRLF, LS, PS).
pub fn get_ecma_line_and_utf16_character_of_position(
    source_file: &(impl SourceFileLike + ?Sized),
    pos: u32,
) -> (usize, UTF16Offset) {
    let line_map = get_ecma_line_starts(source_file);
    let line = compute_line_of_position(line_map, pos);
    let character =
        tsrs_core::utf16_len(&source_file.text()[line_map[line] as usize..pos as usize]);
    (line, character)
}

// GetECMALineAndByteOffsetOfPosition returns the 0-based line number and the
// raw UTF-8 byte offset from the start of that line for the given byte position.
// Uses ECMAScript line separators (LF, CR, CRLF, LS, PS).
// Unlike GetECMALineAndUTF16CharacterOfPosition, the offset is in bytes, not UTF-16 code units.
pub fn get_ecma_line_and_byte_offset_of_position(
    source_file: &(impl SourceFileLike + ?Sized),
    pos: u32,
) -> (usize, u32) {
    let line_map = get_ecma_line_starts(source_file);
    let line = compute_line_of_position(line_map, pos);
    let byte_offset = pos - line_map[line];
    (line, byte_offset)
}

pub fn get_ecma_end_line_position(source_file: P<SourceFile>, line: usize) -> Option<u32> {
    let text = source_file.text().as_bytes();
    let mut pos = get_ecma_line_starts(&*source_file)[line];
    loop {
        let (ch, size) = decode_rune(&text[pos as usize..]);
        if size == 0 || stringutil::is_line_break(ch) {
            return pos.checked_sub(1);
        }
        pos += size as u32;
    }
}

// GetECMAPositionOfLineAndUTF16Character converts a 0-based line number and UTF-16
// code unit character offset back to an absolute byte position in the source text.
// Uses ECMAScript line separators.

// GetECMAPositionOfLineAndByteOffset converts a 0-based line number and byte offset
// from line start back to an absolute byte position in the source text.
// Uses ECMAScript line separators.
pub fn get_ecma_position_of_line_and_byte_offset(
    source_file: &(impl SourceFileLike + ?Sized),
    line: usize,
    byte_offset: u32,
) -> u32 {
    compute_position_of_line_and_byte_offset(get_ecma_line_starts(source_file), line, byte_offset)
}

// ComputePositionOfLineAndByteOffset computes a byte position from a line and
// raw byte offset from the line start. This is a simple addition with validation.
fn compute_position_of_line_and_byte_offset(
    line_starts: &[u32],
    line: usize,
    byte_offset: u32,
) -> u32 {
    assert!(
        line < line_starts.len(),
        "Bad line number. Line: {}, lineStarts.length: {}.",
        line,
        line_starts.len()
    );
    line_starts[line] + byte_offset
}

// ComputePositionOfLineAndUTF16Character converts a line and UTF-16 character offset
// back to a byte position. The character parameter is measured in UTF-16 code units.
// It scans from the line start to correctly handle multi-byte characters.
// When allowEdits is true, out-of-range values are clamped instead of panicking.
pub fn compute_position_of_line_and_utf16_character(
    line_starts: &[u32],
    line: usize,
    character: UTF16Offset,
    text: &str,
    allow_edits: bool,
) -> u32 {
    let mut line = line;
    if line >= line_starts.len() {
        if allow_edits {
            // Clamp line to nearest allowable value
            line = line_starts.len() - 1;
        } else {
            panic!("Bad line number. Line: {}, lineStarts.length: {}.", line, line_starts.len());
        }
    }

    let line_start = line_starts[line];
    let text_len = text.len() as u32;

    if character > 0 {
        // UTF-16 character offset: scan from line start counting UTF-16 code units.
        let mut line_end = text_len;
        if line + 1 < line_starts.len() {
            line_end = line_starts[line + 1];
        }
        let mut utf16_count: UTF16Offset = 0;
        let mut pos = line_start;
        while pos < line_end {
            if utf16_count >= character {
                break;
            }
            let (r, size) = decode_rune(&text.as_bytes()[pos as usize..]);
            utf16_count += if r >= 0x10000 { 2 } else { 1 };
            pos += size as u32;
        }
        if !allow_edits {
            assert!(
                !(pos == line_end && utf16_count < character),
                "Bad UTF-16 character offset. Line: {line}, character: {character}."
            );
            assert!(pos <= text_len);
            return pos;
        }
        if pos > text_len {
            return text_len;
        }
        return pos;
    }

    // Character is 0: line start position.
    let res = line_start;

    if allow_edits {
        if res > text_len {
            return text_len;
        }
        return res;
    }
    assert!(res <= text_len); // Allow single character overflow for trailing newline
    res
}

/// Go `(*ast.NodeFactory).NewCommentRange`; the factory argument of `GetLeadingCommentRanges` /
/// `GetTrailingCommentRanges` only served this constructor and is dropped.
fn new_comment_range(kind: Kind, pos: u32, end: u32, has_trailing_new_line: bool) -> CommentRange {
    CommentRange { text_range: TextRange::new(pos, end), kind, has_trailing_new_line }
}

pub fn get_leading_comment_ranges(text: &str, pos: u32) -> CommentRangeIter<'_> {
    iterate_comment_ranges(text, pos, false)
}

pub fn get_trailing_comment_ranges(text: &str, pos: u32) -> CommentRangeIter<'_> {
    iterate_comment_ranges(text, pos, true)
}

/*
Returns an iterator over each comment range following the provided position.
Single-line comment ranges include the leading double-slash characters but not the ending
line break. Multi-line comment ranges include the leading slash-asterisk and trailing
asterisk-slash characters.
*/
fn iterate_comment_ranges(text: &str, pos: u32, trailing: bool) -> CommentRangeIter<'_> {
    let mut pos = pos;
    let mut collecting = trailing;
    if pos == 0 {
        collecting = true;
        if is_shebang_trivia(text, pos) {
            pos = scan_shebang_trivia(text, pos);
        }
    }
    CommentRangeIter {
        text,
        pos,
        trailing,
        collecting,
        pending_pos: 0,
        pending_end: 0,
        pending_kind: Kind::Unknown,
        pending_has_trailing_new_line: false,
        has_pending_comment_range: false,
        done: false,
    }
}

/// The Go `iter.Seq[ast.CommentRange]` returned by `GetLeadingCommentRanges` /
/// `GetTrailingCommentRanges`, as an explicit state machine.
pub struct CommentRangeIter<'src> {
    text: &'src str,
    pos: u32,
    trailing: bool,
    collecting: bool,
    pending_pos: u32,
    pending_end: u32,
    pending_kind: Kind,
    pending_has_trailing_new_line: bool,
    has_pending_comment_range: bool,
    done: bool,
}

impl Iterator for CommentRangeIter<'_> {
    type Item = CommentRange;

    fn next(&mut self) -> Option<CommentRange> {
        if self.done {
            return None;
        }
        let text = self.text.as_bytes();
        let len = text.len() as u32;
        'scan: while self.pos < len {
            let (ch, size) = decode_rune(&text[self.pos as usize..]);
            match ch {
                0x0D | 0x0A => {
                    if ch == '\r' as Rune && self.pos + 1 < len && text[(self.pos + 1) as usize] == b'\n' {
                        self.pos += 1;
                    }
                    self.pos += 1;
                    if self.trailing {
                        break 'scan;
                    }

                    self.collecting = true;
                    if self.has_pending_comment_range {
                        self.pending_has_trailing_new_line = true;
                    }

                }
                0x09 | 0x0B | 0x0C | 0x20 => {
                    self.pos += 1;
                }
                0x2F /* / */ => {
                    let next_char = if self.pos + 1 < len { text[(self.pos + 1) as usize] } else { 0 };
                    let mut has_trailing_new_line = false;
                    if next_char == b'/' || next_char == b'*' {
                        let kind = if next_char == b'/' { Kind::SingleLineCommentTrivia } else { Kind::MultiLineCommentTrivia };

                        let start_pos = self.pos;
                        self.pos += 2;
                        if next_char == b'/' {
                            while self.pos < len {
                                let (c, s) = decode_rune(&text[self.pos as usize..]);
                                if stringutil::is_line_break(c) {
                                    has_trailing_new_line = true;
                                    break;
                                }
                                self.pos += s as u32;
                            }
                        } else if let Some(i) = memchr::memmem::find(&text[self.pos as usize..], b"*/") {
                            self.pos += i as u32 + 2;
                        } else {
                            self.pos = len;
                        }

                        if self.collecting {
                            let mut result = None;
                            if self.has_pending_comment_range {
                                result = Some(new_comment_range(
                                    self.pending_kind,
                                    self.pending_pos,
                                    self.pending_end,
                                    self.pending_has_trailing_new_line,
                                ));
                            }

                            self.pending_pos = start_pos;
                            self.pending_end = self.pos;
                            self.pending_kind = kind;
                            self.pending_has_trailing_new_line = has_trailing_new_line;
                            self.has_pending_comment_range = true;
                            if result.is_some() {
                                return result;
                            }
                        }

                        continue;
                    }
                    break 'scan;
                }
                _ => {
                    if ch > 0x7F && stringutil::is_white_space_like(ch) {
                        if self.has_pending_comment_range && stringutil::is_line_break(ch) {
                            self.pending_has_trailing_new_line = true;
                        }
                        self.pos += size as u32;
                        continue;
                    }
                    break 'scan;
                }
            }
        }

        self.done = true;
        if self.has_pending_comment_range {
            return Some(new_comment_range(
                self.pending_kind,
                self.pending_pos,
                self.pending_end,
                self.pending_has_trailing_new_line,
            ));
        }
        None
    }
}
