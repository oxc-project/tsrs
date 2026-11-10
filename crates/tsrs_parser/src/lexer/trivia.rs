//! Stateless text helpers for the TypeScript-compatible scanner.

use tsrs_core::{
    LanguageVariant,
    stringutil::{self, Rune},
};
use tsrs_diagnostics as diagnostics;
use tsrs_diagnostics::Message;

pub(super) const RUNE_ERROR: Rune = 0xFFFD;
const RUNE_SELF: Rune = 0x80;

/// `[A-Za-z0-9_$]` by byte.
pub(super) static ASCII_IDENTIFIER_PART: [bool; 256] = {
    let mut t = [false; 256];
    let mut b = 0;
    while b < 128 {
        let c = b as u8;
        t[b] = c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
        b += 1;
    }
    t
};

/// The bytes that end the fast scan of a string literal body, by quote (`'` then `"`).
pub(super) static STRING_LITERAL_END: [[bool; 256]; 2] = {
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
pub(super) fn decode_rune(b: &[u8]) -> (Rune, usize) {
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
pub(super) fn decode_last_rune(b: &[u8]) -> (Rune, usize) {
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
pub(super) fn rune_to_char(r: Rune) -> char {
    char::from_u32(r).unwrap_or('\u{FFFD}')
}

#[inline]
pub(super) fn rune_string(r: Rune) -> String {
    rune_to_char(r).to_string()
}

/// Go `strconv.ParseInt(s, base, 32)` ignoring the error (clamped on overflow).
pub(super) fn parse_int_32(s: &str, base: u32) -> i64 {
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
pub(super) fn parse_int_64(s: &str, base: u32) -> i64 {
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

// Section 6.1.4
fn is_word_character(ch: Rune) -> bool {
    stringutil::is_ascii_letter(ch) || stringutil::is_digit(ch) || ch == u32::from(b'_')
}

#[inline]
pub(super) fn is_identifier_part_ex(ch: Rune, language_variant: LanguageVariant) -> bool {
    is_word_character(ch)
        || ch == u32::from(b'$')
        || ch >= RUNE_SELF && stringutil::is_unicode_identifier_part(ch)
        // ":" is part of JSXNamespacedName, but not JSXIdentifier.
        || language_variant == LanguageVariant::JSX && ch == u32::from(b'-')
}

// All conflict markers consist of the same character repeated seven times.  If it is
// a <<<<<<< or >>>>>>> marker then it is also followed by a space.
const MERGE_CONFLICT_MARKER_LENGTH: u32 = "<<<<<<<".len() as u32;

pub(super) fn is_conflict_marker_trivia(text: &str, pos: u32) -> bool {
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

pub(super) fn scan_conflict_marker_trivia(
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

