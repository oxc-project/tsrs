use sha2::{Digest as _, Sha512};

#[path = "../../crates/tsrs_core/src/stringutil/js_case_generated.rs"]
mod compact;
#[expect(dead_code, reason = "Only range-table types are needed by the generated table")]
#[path = "../../crates/tsrs_core/src/stringutil/rangetable.rs"]
mod rangetable;

fn number(hash: &mut Sha512, value: u32) {
    hash.update(value.to_le_bytes());
}

fn count(hash: &mut Sha512, value: usize) {
    number(hash, u32::try_from(value).unwrap());
}

#[test]
fn all_mappings_and_properties_match_the_pinned_unicode_table() {
    // Digest of the original Go-derived Unicode 15.1 table at TypeScript
    // b85298b6a81f772d080b0455de0ca9d744cd6fd6. Counts, keys, lengths,
    // conditions and range fields are little-endian u32; strings are UTF-8 bytes.
    let mut hash = Sha512::new();
    let mappings = compact::SPECIAL_CASING_MAPPINGS;
    assert_eq!(mappings.len(), 2927);
    assert!(mappings.windows(2).all(|pair| pair[0].0 < pair[1].0));
    count(&mut hash, mappings.len());
    for &(key, mapping) in mappings {
        number(&mut hash, key);
        for bytes in [mapping.lower(), mapping.upper(), mapping.conditional_lower()] {
            count(&mut hash, bytes.len());
            hash.update(bytes);
        }
        number(
            &mut hash,
            u32::from(mapping.condition == compact::SpecialCasingCondition::FinalSigma),
        );
    }
    for table in [&compact::UNICODE_CASED_RANGES, &compact::UNICODE_CASE_IGNORABLE_RANGES] {
        count(&mut hash, table.latin_offset);
        count(&mut hash, table.r16.len());
        for range in table.r16 {
            for value in [range.lo, range.hi, range.stride] {
                number(&mut hash, u32::from(value));
            }
        }
        count(&mut hash, table.r32.len());
        for range in table.r32 {
            for value in [range.lo, range.hi, range.stride] {
                number(&mut hash, value);
            }
        }
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        concat!(
            "a3fe150422a1dc546d09b7ec55acb4b4830d86b2189c3c6fd1f75dd072c0f6ef4b51",
            "a92cd3bf89c07c3a99eb3e77ff02edac6324418852c6d4594848b4f9ef05"
        )
    );
}
