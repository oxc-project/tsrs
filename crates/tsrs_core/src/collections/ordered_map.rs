use rustc_hash::FxBuildHasher;
use std::hash::Hash;

pub type OrderedMap<K, V> = indexmap::IndexMap<K, V, FxBuildHasher>;

pub fn new_ordered_map_with_size_hint<K, V>(hint: usize) -> OrderedMap<K, V> {
    OrderedMap::with_capacity_and_hasher(hint, FxBuildHasher)
}

pub fn diff_ordered_maps<K: Hash + Eq, V: PartialEq>(
    m1: &OrderedMap<K, V>,
    m2: &OrderedMap<K, V>,
    on_added: impl FnMut(&K, &V),
    on_removed: impl FnMut(&K, &V),
    on_modified: impl FnMut(&K, &V, &V),
) {
    diff_ordered_maps_func(m1, m2, |a, b| a == b, on_added, on_removed, on_modified);
}

pub fn diff_ordered_maps_func<K: Hash + Eq, V>(
    m1: &OrderedMap<K, V>,
    m2: &OrderedMap<K, V>,
    mut equal_values: impl FnMut(&V, &V) -> bool,
    mut on_added: impl FnMut(&K, &V),
    mut on_removed: impl FnMut(&K, &V),
    mut on_modified: impl FnMut(&K, &V, &V),
) {
    for (k, v2) in m2 {
        if !m1.contains_key(k) {
            on_added(k, v2);
        }
    }
    for (k, v1) in m1 {
        if let Some(v2) = m2.get(k) {
            if !equal_values(v1, v2) {
                on_modified(k, v1, v2);
            }
        } else {
            on_removed(k, v1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad_int(n: i32) -> String {
        format!("{n:10}")
    }

    #[test]
    fn test_ordered_map() {
        const N: i32 = 1000;
        const START: i32 = 1;
        const END: i32 = START + N;

        let mut m: OrderedMap<i32, String> = OrderedMap::default();
        assert!(!m.contains_key(&1));
        // Seed the map with ascending keys and values for easier testing.
        m.extend((START..END).map(|i| (i, pad_int(i))));
        assert_eq!(m.len(), N as usize);
        // Attempt to overwrite existing keys in reverse order.
        m.extend((START..END).rev().map(|i| (i, pad_int(i))));
        assert_eq!(m.len(), N as usize);
        for i in START..END {
            assert_eq!(m.get(&i), Some(&pad_int(i)));
        }
        for (k, v) in &m {
            assert_eq!(*v, pad_int(*k));
        }
        let keys: Vec<i32> = m.keys().copied().collect();
        assert_eq!(keys.len(), N as usize);
        assert!(keys.is_sorted());
        let values: Vec<&String> = m.values().collect();
        assert!(values.is_sorted());
        assert_eq!(m.keys().next(), Some(&START));
        assert_eq!(m.values().next(), Some(&pad_int(START)));
        for i in START + 1..END {
            assert_eq!(m.shift_remove(&i), Some(pad_int(i)));
            assert!(!m.contains_key(&i));
            assert_eq!(m.get(&i), None);
            assert_eq!(m.shift_remove(&i), None);
        }
        assert_eq!(m.len(), 1);
        assert!(m.contains_key(&START));
        assert_eq!(m.shift_remove(&START), Some(pad_int(START)));
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn test_ordered_map_clone() {
        let mut m: OrderedMap<i32, &str> = OrderedMap::default();
        m.insert(1, "one");
        m.insert(2, "two");
        let clone = m.clone();
        assert_eq!(clone.len(), 2);
        m.shift_remove(&1);
        assert_eq!(m.len(), 1);
        assert_eq!(clone.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(clone.values().copied().collect::<Vec<_>>(), vec!["one", "two"]);
        m.clear();
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn test_diff() {
        let m1 = OrderedMap::from_iter([("a", 1), ("b", 2)]);
        let m2 = OrderedMap::from_iter([("b", 3), ("c", 4)]);
        let mut log = Vec::new();
        let log_cell = std::cell::RefCell::new(&mut log);
        diff_ordered_maps(
            &m1,
            &m2,
            |k, v| log_cell.borrow_mut().push(format!("+{k}{v}")),
            |k, v| log_cell.borrow_mut().push(format!("-{k}{v}")),
            |k, a, b| log_cell.borrow_mut().push(format!("~{k}{a}{b}")),
        );
        assert_eq!(log, vec!["+c4", "-a1", "~b23"]);
    }
}
