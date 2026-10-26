use rustc_hash::FxBuildHasher;

pub type OrderedSet<T> = indexmap::IndexSet<T, FxBuildHasher>;

pub fn new_ordered_set_with_size_hint<T>(hint: usize) -> OrderedSet<T> {
    OrderedSet::with_capacity_and_hasher(hint, FxBuildHasher)
}
