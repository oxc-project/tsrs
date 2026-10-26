use tsrs_core::collections::syncmap::*;

#[test]
fn test_sync_map() {
    let m: SyncMap<&str, i32> = SyncMap::default();
    assert_eq!(m.load_or_store("a", 1), (1, false));
    assert_eq!(m.load_or_store("a", 2), (1, true));
    assert_eq!(m.load(&"a"), Some(1));
    m.store("b", 3);
    m.store("b", 4);
    assert_eq!(m.load(&"b"), Some(4));
    assert_eq!(m.size(), 2);
    let c = m.clone_map();
    m.delete(&"a");
    assert_eq!(m.load(&"a"), None);
    assert_eq!(c.load(&"a"), Some(1));
}

#[test]
fn borrowed_lookup_hashes_like_the_key() {
    let m: SyncMap<String, u32> = SyncMap::default();
    for i in 0..1000 {
        m.store(format!("/dir/{i}/file.ts"), i);
    }
    assert!((0..1000).all(|i| m.load(format!("/dir/{i}/file.ts").as_str()) == Some(i)));
    assert_eq!(m.load("/dir/none"), None);
}

#[test]
fn shards_cover_every_key() {
    let m: SyncMap<u64, u64> = SyncMap::default();
    for i in 0..10_000 {
        m.store(i, i * 2);
    }
    assert_eq!(m.size(), 10_000);
    assert!((0..10_000).all(|i| m.load(&i) == Some(i * 2)));
    let mut keys = m.keys();
    keys.sort_unstable();
    assert_eq!(keys, (0..10_000).collect::<Vec<_>>());
    assert_eq!(m.to_map().len(), 10_000);
    let mut seen = 0;
    m.range(|_, _| {
        seen += 1;
        true
    });
    assert_eq!(seen, 10_000);
    m.clear();
    assert_eq!(m.size(), 0);
}
