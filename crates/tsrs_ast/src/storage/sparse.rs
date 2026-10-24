use super::memory::{AstBufferMemory, BufferMemory};

/// Construction-only optional fields with low occupancy. Entries are appended
/// in table-element order; absent fields allocate no per-node slot or bitmap.
pub(super) struct SparseColumn<T> {
    entries: Vec<(u32, T)>,
}

impl<T> Default for SparseColumn<T> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<T> BufferMemory for SparseColumn<T> {
    fn buffer_memory(&self) -> AstBufferMemory {
        self.entries.buffer_memory()
    }
}

impl<T: Copy> SparseColumn<T> {
    pub(super) fn restore(&mut self, len: usize, release: bool) {
        let count = self.entries.partition_point(|&(index, _)| (index as usize) < len);
        self.entries.truncate(count);
        if release {
            self.entries.shrink_to_fit();
        }
    }

    pub(super) fn reserve(&mut self, value: Option<T>) {
        if value.is_some() {
            self.entries.reserve(1);
        }
    }

    pub(super) fn push(&mut self, element: usize, value: Option<T>) {
        if let Some(value) = value {
            // The checked table address limits elements to 20 bits.
            let element = u32::try_from(element).expect("checked node table element");
            debug_assert!(self.entries.last().is_none_or(|&(last, _)| last < element));
            self.entries.push((element, value));
        }
    }

    pub(super) fn get(&self, element: usize) -> Option<T> {
        self.entries
            .binary_search_by_key(&element, |&(index, _)| index as usize)
            .ok()
            .map(|index| self.entries[index].1)
    }
}
