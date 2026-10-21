// The members of a JSON object, in insertion order (Go's encoding/json/v2 keeps document order).
//
// Most objects the API reads and writes have a handful of members, for which hashing every key on
// insert and lookup costs more than comparing it with each member. A map keeps its entries in a vector,
// searches it linearly while small, and builds a hash index of the entries once it outgrows
// `LINEAR_MAX`, so large objects (tsconfig files, file system maps) keep constant-time lookups.
//
// The API mirrors the `IndexMap` methods JSON code uses. Equality, like `IndexMap`'s, ignores order.

use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};

use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;

use super::Value;

/// Maps up to this many members are searched linearly.
const LINEAR_MAX: usize = 8;

#[derive(Clone, Default)]
pub struct JsonMap {
    entries: Vec<(String, Value)>,
    /// Positions in `entries`, by key hash, once the map has more than `LINEAR_MAX` members.
    /// Boxed, so a small map (and a `Value`) stays small.
    index: Option<Box<HashTable<usize>>>,
}

fn hash<Q: Hash + ?Sized>(key: &Q) -> u64 {
    FxBuildHasher.hash_one(key)
}

impl JsonMap {
    pub fn new() -> JsonMap {
        JsonMap::default()
    }

    pub fn with_capacity(capacity: usize) -> JsonMap {
        JsonMap { entries: Vec::with_capacity(capacity), index: None }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index = None;
    }

    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    /// The position of `key`'s member.
    pub fn get_index_of<Q>(&self, key: &Q) -> Option<usize>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        match &self.index {
            None => self.entries.iter().position(|(k, _)| k.borrow() == key),
            Some(index) => index.find(hash(key), |&i| self.entries[i].0.borrow() == key).copied(),
        }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&Value>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get_index_of(key).map(|i| &self.entries[i].1)
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut Value>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get_index_of(key).map(|i| &mut self.entries[i].1)
    }

    pub fn get_key_value<Q>(&self, key: &Q) -> Option<(&String, &Value)>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get_index_of(key).map(|i| (&self.entries[i].0, &self.entries[i].1))
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.get_index_of(key).is_some()
    }

    pub fn get_index(&self, i: usize) -> Option<(&String, &Value)> {
        self.entries.get(i).map(|(k, v)| (k, v))
    }

    pub fn get_index_mut(&mut self, i: usize) -> Option<(&String, &mut Value)> {
        self.entries.get_mut(i).map(|(k, v)| (&*k, v))
    }

    pub fn first(&self) -> Option<(&String, &Value)> {
        self.get_index(0)
    }

    pub fn last(&self) -> Option<(&String, &Value)> {
        self.entries.last().map(|(k, v)| (k, v))
    }

    /// Sets `key`'s value: a new key goes last, an existing one keeps its position. Returns the old value.
    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.insert_full(key, value).1
    }

    /// `insert`, also returning the member's position.
    pub fn insert_full(&mut self, key: String, value: Value) -> (usize, Option<Value>) {
        if let Some(i) = self.get_index_of(key.as_str()) {
            return (i, Some(std::mem::replace(&mut self.entries[i].1, value)));
        }
        (self.push(key, value), None)
    }

    /// Appends a member known to be new.
    fn push(&mut self, key: String, value: Value) -> usize {
        let i = self.entries.len();
        self.entries.push((key, value));
        match &mut self.index {
            Some(index) => {
                let entries = &self.entries;
                index.insert_unique(hash(entries[i].0.as_str()), i, |&j| {
                    hash(entries[j].0.as_str())
                });
            }
            None if self.entries.len() > LINEAR_MAX => self.reindex(),
            None => {}
        }
        i
    }

    /// A map of `entries` in order, or `None` when two have the same key.
    pub fn from_distinct(entries: Vec<(String, Value)>) -> Option<JsonMap> {
        let mut map = JsonMap { entries, index: None };
        if map.entries.len() <= LINEAR_MAX {
            for (i, (k, _)) in map.entries.iter().enumerate() {
                if map.entries[..i].iter().any(|(earlier, _)| earlier == k) {
                    return None;
                }
            }
            return Some(map);
        }
        map.reindex();
        let index = map.index.as_ref()?;
        // `reindex` indexes every position: a duplicate shows as a key that finds an earlier position.
        for (i, (k, _)) in map.entries.iter().enumerate() {
            if index.find(hash(k.as_str()), |&j| map.entries[j].0 == *k) != Some(&i) {
                return None;
            }
        }
        Some(map)
    }

    /// Rebuilds the hash index after entries moved (none while the map is small).
    fn reindex(&mut self) {
        if self.entries.len() <= LINEAR_MAX {
            self.index = None;
            return;
        }
        let mut index = HashTable::with_capacity(self.entries.len());
        for (i, (k, _)) in self.entries.iter().enumerate() {
            let entries = &self.entries;
            index.insert_unique(hash(k.as_str()), i, |&j| hash(entries[j].0.as_str()));
        }
        self.index = Some(Box::new(index));
    }

    /// Removes `key`'s member, moving the later members up by one.
    pub fn shift_remove<Q>(&mut self, key: &Q) -> Option<Value>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let i = self.get_index_of(key)?;
        Some(self.shift_remove_index(i)?.1)
    }

    pub fn shift_remove_index(&mut self, i: usize) -> Option<(String, Value)> {
        if i >= self.entries.len() {
            return None;
        }
        let entry = self.entries.remove(i);
        if self.index.is_some() {
            self.reindex();
        }
        Some(entry)
    }

    /// Removes `key`'s member, moving the last member into its place.
    pub fn swap_remove<Q>(&mut self, key: &Q) -> Option<Value>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let i = self.get_index_of(key)?;
        let (_, value) = self.entries.swap_remove(i);
        if self.index.is_some() {
            self.reindex();
        }
        Some(value)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&String, &mut Value) -> bool) {
        let len = self.entries.len();
        self.entries.retain_mut(|(k, v)| keep(k, v));
        if self.entries.len() != len && self.index.is_some() {
            self.reindex();
        }
    }

    pub fn entry(&mut self, key: String) -> Entry<'_> {
        match self.get_index_of(key.as_str()) {
            Some(index) => Entry::Occupied(OccupiedEntry { map: self, index }),
            None => Entry::Vacant(VacantEntry { map: self, key }),
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter(self.entries.iter())
    }

    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut(self.entries.iter_mut())
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &Value> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = &mut Value> + ExactSizeIterator {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    pub fn into_keys(self) -> impl DoubleEndedIterator<Item = String> + ExactSizeIterator {
        self.entries.into_iter().map(|(k, _)| k)
    }

    pub fn into_values(self) -> impl DoubleEndedIterator<Item = Value> + ExactSizeIterator {
        self.entries.into_iter().map(|(_, v)| v)
    }

    /// Stable sort by a key computed once per member.
    pub fn sort_by_cached_key<T: Ord>(&mut self, mut sort_key: impl FnMut(&String, &Value) -> T) {
        self.entries.sort_by_cached_key(|(k, v)| sort_key(k, v));
        if self.index.is_some() {
            self.reindex();
        }
    }

    /// Stable sort by member name.
    pub fn sort_keys(&mut self) {
        self.entries.sort_by(|(a, _), (b, _)| a.cmp(b));
        if self.index.is_some() {
            self.reindex();
        }
    }

    /// Stable sort.
    pub fn sort_by(
        &mut self,
        mut compare: impl FnMut(&String, &Value, &String, &Value) -> std::cmp::Ordering,
    ) {
        self.entries.sort_by(|(ka, va), (kb, vb)| compare(ka, va, kb, vb));
        if self.index.is_some() {
            self.reindex();
        }
    }

    pub fn reverse(&mut self) {
        self.entries.reverse();
        if self.index.is_some() {
            self.reindex();
        }
    }
}

impl PartialEq for JsonMap {
    /// The same members with equal values, in any order (as `IndexMap`'s equality).
    fn eq(&self, other: &JsonMap) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k.as_str()) == Some(v))
    }
}

impl std::fmt::Debug for JsonMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<Q> std::ops::Index<&Q> for JsonMap
where
    String: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
{
    type Output = Value;

    /// Panics when `key` is not a member.
    fn index(&self, key: &Q) -> &Value {
        self.get(key).expect("JsonMap: key not found")
    }
}

impl Extend<(String, Value)> for JsonMap {
    fn extend<I: IntoIterator<Item = (String, Value)>>(&mut self, members: I) {
        for (k, v) in members {
            self.insert(k, v);
        }
    }
}

impl FromIterator<(String, Value)> for JsonMap {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(members: I) -> JsonMap {
        let mut map = JsonMap::new();
        map.extend(members);
        map
    }
}

impl<const N: usize> From<[(String, Value); N]> for JsonMap {
    fn from(members: [(String, Value); N]) -> JsonMap {
        members.into_iter().collect()
    }
}

pub struct Iter<'a>(std::slice::Iter<'a, (String, Value)>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a String, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(k, v)| (k, v))
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl Clone for Iter<'_> {
    fn clone(&self) -> Self {
        Iter(self.0.clone())
    }
}

pub struct IterMut<'a>(std::slice::IterMut<'a, (String, Value)>);

impl<'a> Iterator for IterMut<'a> {
    type Item = (&'a String, &'a mut Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, v)| (&*k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for IterMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(k, v)| (&*k, v))
    }
}

impl ExactSizeIterator for IterMut<'_> {}

pub struct IntoIter(std::vec::IntoIter<(String, Value)>);

impl Iterator for IntoIter {
    type Item = (String, Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl ExactSizeIterator for IntoIter {}

impl IntoIterator for JsonMap {
    type Item = (String, Value);
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        IntoIter(self.entries.into_iter())
    }
}

impl<'a> IntoIterator for &'a JsonMap {
    type Item = (&'a String, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut JsonMap {
    type Item = (&'a String, &'a mut Value);
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

pub enum Entry<'a> {
    Occupied(OccupiedEntry<'a>),
    Vacant(VacantEntry<'a>),
}

impl<'a> Entry<'a> {
    pub fn key(&self) -> &str {
        match self {
            Entry::Occupied(e) => e.key(),
            Entry::Vacant(e) => e.key(),
        }
    }

    pub fn or_insert(self, default: Value) -> &'a mut Value {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default),
        }
    }

    pub fn or_insert_with(self, default: impl FnOnce() -> Value) -> &'a mut Value {
        match self {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(default()),
        }
    }

    pub fn or_default(self) -> &'a mut Value {
        self.or_insert_with(Value::default)
    }

    #[must_use]
    pub fn and_modify(self, f: impl FnOnce(&mut Value)) -> Entry<'a> {
        match self {
            Entry::Occupied(mut e) => {
                f(e.get_mut());
                Entry::Occupied(e)
            }
            Entry::Vacant(e) => Entry::Vacant(e),
        }
    }
}

pub struct OccupiedEntry<'a> {
    map: &'a mut JsonMap,
    index: usize,
}

impl<'a> OccupiedEntry<'a> {
    pub fn key(&self) -> &str {
        &self.map.entries[self.index].0
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn get(&self) -> &Value {
        &self.map.entries[self.index].1
    }

    pub fn get_mut(&mut self) -> &mut Value {
        &mut self.map.entries[self.index].1
    }

    pub fn into_mut(self) -> &'a mut Value {
        &mut self.map.entries[self.index].1
    }

    /// Replaces the value, returning the old one.
    pub fn insert(&mut self, value: Value) -> Value {
        std::mem::replace(self.get_mut(), value)
    }

    pub fn shift_remove(self) -> Value {
        self.map.shift_remove_index(self.index).map(|(_, v)| v).unwrap_or_default()
    }
}

pub struct VacantEntry<'a> {
    map: &'a mut JsonMap,
    key: String,
}

impl<'a> VacantEntry<'a> {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn into_key(self) -> String {
        self.key
    }

    pub fn insert(self, value: Value) -> &'a mut Value {
        let i = self.map.push(self.key, value);
        &mut self.map.entries[i].1
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonMap, LINEAR_MAX};
    use crate::json::Value;

    fn n(i: usize) -> Value {
        Value::Number(i as f64)
    }

    /// Lookups, replacement, removal and order agree with `IndexMap` on both sides of `LINEAR_MAX`.
    #[test]
    fn matches_indexmap_small_and_large() {
        for size in [0, 1, LINEAR_MAX - 1, LINEAR_MAX, LINEAR_MAX + 1, 3 * LINEAR_MAX] {
            let mut ours = JsonMap::new();
            let mut theirs = crate::collections::OrderedMap::<String, Value>::default();
            for i in 0..size {
                let key = format!("k{}", (i * 7) % (size + 3));
                assert_eq!(ours.insert(key.clone(), n(i)), theirs.insert(key, n(i)));
            }
            for i in 0..size + 3 {
                let key = format!("k{i}");
                assert_eq!(ours.get(key.as_str()), theirs.get(key.as_str()), "{size} {key}");
                assert_eq!(ours.get_index_of(key.as_str()), theirs.get_index_of(key.as_str()));
            }
            for i in (0..size + 3).step_by(3) {
                let key = format!("k{i}");
                assert_eq!(ours.shift_remove(key.as_str()), theirs.shift_remove(key.as_str()));
            }
            let order: Vec<_> = ours.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let want: Vec<_> = theirs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            assert_eq!(order, want, "{size}");
            for (k, v) in &theirs {
                assert_eq!(ours.get(k.as_str()), Some(v), "{size} {k}");
            }
            ours.reverse();
            theirs.reverse();
            ours.sort_keys();
            theirs.sort_keys();
            assert!(ours.iter().eq(theirs.iter()), "{size}");
            for (k, v) in &theirs {
                assert_eq!(ours.get(k.as_str()), Some(v), "{size} {k}");
            }
        }
    }

    /// `from_distinct` builds the same map as inserting, and rejects a repeated key at any size and position.
    #[test]
    fn from_distinct_rejects_duplicates() {
        for size in [1, 2, LINEAR_MAX, LINEAR_MAX + 1, 3 * LINEAR_MAX] {
            let entries: Vec<_> = (0..size).map(|i| (format!("k{i}"), n(i))).collect();
            let map = JsonMap::from_distinct(entries.clone()).expect("distinct keys");
            assert_eq!(map, entries.iter().cloned().collect::<JsonMap>());
            for (k, v) in &entries {
                assert_eq!(map.get(k.as_str()), Some(v));
            }
            for from in 0..size {
                for to in 0..=size {
                    let mut dup = entries.clone();
                    dup.insert(to, (format!("k{from}"), n(99)));
                    assert!(JsonMap::from_distinct(dup).is_none(), "{size} {from} {to}");
                }
            }
        }
    }

    #[test]
    fn equality_ignores_order() {
        let a: JsonMap = [("a".to_string(), n(1)), ("b".to_string(), n(2))].into();
        let b: JsonMap = [("b".to_string(), n(2)), ("a".to_string(), n(1))].into();
        let c: JsonMap = [("a".to_string(), n(1))].into();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
