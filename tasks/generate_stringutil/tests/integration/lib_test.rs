use tsrs_generate_stringutil::*;

#[test]
fn pool_deduplicates_and_checks_utf8_byte_bounds() {
    let mut pool = Pool::default();
    let text = "i\u{307}\u{10428}";
    let slice = pool.insert(text).unwrap();
    assert_eq!(slice, Slice { offset: 0, length: 7 });
    assert_eq!(pool.insert(text).unwrap(), slice);
    assert_eq!(pool.insert("").unwrap(), Slice { offset: 7, length: 0 });
    assert!(pool.insert(&"x".repeat(256)).is_err());
    pool.validate().unwrap();

    let mut full = Pool::default();
    for i in 0..257 {
        full.insert(&format!("{i:x<255}")).unwrap();
    }
    assert_eq!(full.text.len(), usize::from(u16::MAX));
    assert!(full.insert("y").is_err());
    full.validate().unwrap();
}

#[test]
fn property_ranges_preserve_sparse_points() {
    assert_eq!(
        stride_ranges(&[0x41, 0x43, 0x45, 0xFFFE, 0xFFFF]),
        [Range { lo: 0x41, hi: 0x45, stride: 2 }, Range { lo: 0xFFFE, hi: 0xFFFF, stride: 1 }]
    );
    assert_eq!(stride_ranges(&[]), []);
}
