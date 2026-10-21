// The scan both the request parser and `marshal` run over string contents.

/// The length of the longest prefix of `b` whose bytes a JSON string holds as themselves: printable ASCII other
/// than `"` and `\`. It tests eight bytes at a time.
pub fn plain_ascii_len(b: &[u8]) -> usize {
    const ONES: u64 = u64::from_ne_bytes([0x01; 8]);
    const HIGH: u64 = u64::from_ne_bytes([0x80; 8]);
    let mut i = 0;
    while let Some(chunk) = b.get(i..i + 8) {
        let w = u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        // Each term sets a byte's high bit when the byte is a control character, `"`, `\` or not ASCII. The
        // subtractions can also set the bits of bytes above the first one that matches (their borrow carries
        // up), never below it, so the lowest set bit is exact.
        let quote = w ^ (ONES * u64::from(b'"'));
        let backslash = w ^ (ONES * u64::from(b'\\'));
        let special = (w.wrapping_sub(ONES * 0x20) & !w)
            | (quote.wrapping_sub(ONES) & !quote)
            | (backslash.wrapping_sub(ONES) & !backslash)
            | w;
        let special = special & HIGH;
        if special != 0 {
            return i + (special.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }
    i + b[i..].iter().position(|&c| !is_plain(c)).unwrap_or(b.len() - i)
}

fn is_plain(c: u8) -> bool {
    (0x20..0x80).contains(&c) && c != b'"' && c != b'\\'
}

#[cfg(test)]
mod tests {
    use super::{is_plain, plain_ascii_len};

    /// Agrees with the byte-at-a-time definition for every byte value at every offset of an otherwise plain run.
    #[test]
    fn matches_bytewise_scan() {
        for len in 0..20 {
            for at in 0..=len {
                for c in 0..=255u8 {
                    let mut b = vec![b'a'; len];
                    if at < len {
                        b[at] = c;
                    }
                    // A second special byte after the first must not hide it.
                    if at + 1 < len {
                        b[at + 1] = b'"';
                    }
                    let want = b.iter().position(|&c| !is_plain(c)).unwrap_or(b.len());
                    assert_eq!(plain_ascii_len(&b), want, "{b:?}");
                }
            }
        }
    }
}
