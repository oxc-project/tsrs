use tsrs_core::TextRange;

use super::keys::{StoredListKey, StoredModifierListKey, StoredNodeKey};
use super::memory::{AstBufferMemory, BufferMemory};
use super::{
    AstStore, AstStoreBuilder, AstStoreError, AstStoreId, AstStoreLease, ListIndex, ListKey,
    ModifierListIndex, ModifierListKey, NodeKey, NodeView,
};
use crate::ModifierFlags;

#[derive(Clone, Copy)]
pub(super) struct ElementSpan {
    start: u32,
    len: u32,
}

impl ElementSpan {
    const MISSING: Self = Self { start: u32::MAX, len: 0 };

    const fn is_missing(self) -> bool {
        self.start == u32::MAX
    }

    fn elements(self, buffer: &[StoredNodeKey]) -> &[StoredNodeKey] {
        if self.is_missing() {
            return &buffer[..0];
        }
        let start = self.start as usize;
        &buffer[start..start + self.len as usize]
    }
}

#[derive(Default)]
pub(super) struct ListColumns {
    ranges: Vec<TextRange>,
    spans: Vec<ElementSpan>,
    modifier_ranges: Vec<TextRange>,
    modifier_spans: Vec<ElementSpan>,
    modifier_flags: Vec<ModifierFlags>,
    elements: Vec<StoredNodeKey>,
}

#[derive(Clone, Copy)]
pub(super) struct ListCheckpoint {
    lists: usize,
    modifiers: usize,
    elements: usize,
}

impl ListColumns {
    pub(super) fn checkpoint(&self) -> ListCheckpoint {
        ListCheckpoint {
            lists: self.ranges.len(),
            modifiers: self.modifier_ranges.len(),
            elements: self.elements.len(),
        }
    }

    pub(super) fn restore(&mut self, checkpoint: ListCheckpoint, release: bool) {
        self.ranges.truncate(checkpoint.lists);
        self.spans.truncate(checkpoint.lists);
        self.modifier_ranges.truncate(checkpoint.modifiers);
        self.modifier_spans.truncate(checkpoint.modifiers);
        self.modifier_flags.truncate(checkpoint.modifiers);
        self.elements.truncate(checkpoint.elements);
        if release {
            self.ranges.shrink_to_fit();
            self.spans.shrink_to_fit();
            self.modifier_ranges.shrink_to_fit();
            self.modifier_spans.shrink_to_fit();
            self.modifier_flags.shrink_to_fit();
            self.elements.shrink_to_fit();
        }
    }
}

impl BufferMemory for ListColumns {
    fn buffer_memory(&self) -> AstBufferMemory {
        self.ranges.buffer_memory()
            + self.spans.buffer_memory()
            + self.modifier_ranges.buffer_memory()
            + self.modifier_spans.buffer_memory()
            + self.modifier_flags.buffer_memory()
            + self.elements.buffer_memory()
    }
}

impl ListColumns {
    pub(super) fn raw_nodes<'a>(
        &'a self,
        span: ElementSpan,
        store: AstStoreId,
        foreign: &'a [NodeKey],
    ) -> NodeSliceView<'a> {
        NodeSliceView { store, elements: span.elements(&self.elements), foreign }
    }

    fn list(&self, store: AstStoreId, key: ListKey) -> Result<usize, AstStoreError> {
        check_store(store, key.store())?;
        let offset = key.index().offset();
        if offset >= self.ranges.len() {
            return Err(AstStoreError::MissingList(key.index()));
        }
        Ok(offset)
    }

    fn modifiers(&self, store: AstStoreId, key: ModifierListKey) -> Result<usize, AstStoreError> {
        check_store(store, key.store())?;
        let offset = key.index().offset();
        if offset >= self.modifier_ranges.len() {
            return Err(AstStoreError::MissingModifierList(key.index()));
        }
        Ok(offset)
    }
}

fn check_store(expected: AstStoreId, actual: AstStoreId) -> Result<(), AstStoreError> {
    if expected != actual {
        return Err(AstStoreError::WrongStore { expected, actual });
    }
    Ok(())
}

impl AstStoreBuilder {
    pub(super) fn encode_modifiers(
        &mut self,
        key: ModifierListKey,
    ) -> Result<StoredModifierListKey, AstStoreError> {
        if key.store() == self.id {
            self.lists.modifiers(self.id, key)?;
            return Ok(StoredModifierListKey::local(key.index()));
        }
        if let Some(&encoded) = self.foreign_modifier_index.get(&key) {
            return Ok(encoded);
        }
        let encoded = StoredModifierListKey::foreign(self.foreign_modifiers.len())?;
        self.foreign_modifiers.reserve(1);
        self.foreign_modifier_index.reserve(1);
        self.foreign_modifier_index.insert(key, encoded);
        self.foreign_modifiers.push(key);
        Ok(encoded)
    }

    pub(super) fn encode_list(&mut self, key: ListKey) -> Result<StoredListKey, AstStoreError> {
        if key.store() == self.id {
            self.lists.list(self.id, key)?;
            return Ok(StoredListKey::local(key.index()));
        }
        if let Some(&encoded) = self.foreign_list_index.get(&key) {
            return Ok(encoded);
        }
        let encoded = StoredListKey::foreign(self.foreign_lists.len())?;
        self.foreign_lists.reserve(1);
        self.foreign_list_index.reserve(1);
        self.foreign_list_index.insert(key, encoded);
        self.foreign_lists.push(key);
        Ok(encoded)
    }

    pub(super) fn append_list_elements_iter(
        &mut self,
        nodes: impl ExactSizeIterator<Item = NodeKey>,
    ) -> Result<ElementSpan, AstStoreError> {
        // Empty spans always use zero, so MAX is available for the parser's
        // missing-list sentinel without reducing the element-buffer capacity.
        if nodes.len() == 0 {
            return Ok(ElementSpan { start: 0, len: 0 });
        }
        let start = u32::try_from(self.lists.elements.len())
            .map_err(|_| AstStoreError::ListElementCapacity)?;
        let len = u32::try_from(nodes.len()).map_err(|_| AstStoreError::ListElementCapacity)?;
        start.checked_add(len).ok_or(AstStoreError::ListElementCapacity)?;
        self.lists.elements.reserve(nodes.len());
        for node in nodes {
            match self.encode(node) {
                Ok(node) => self.lists.elements.push(node),
                Err(error) => {
                    // Unused foreign entries are harmless. No list identity has
                    // been exposed, and no existing list span was changed.
                    self.lists.elements.truncate(start as usize);
                    return Err(error);
                }
            }
        }
        Ok(ElementSpan { start, len })
    }

    pub fn new_node_list(
        &mut self,
        range: TextRange,
        nodes: &[NodeKey],
    ) -> Result<ListKey, AstStoreError> {
        self.new_node_list_iter(range, nodes.iter().copied())
    }

    pub(super) fn new_node_list_iter(
        &mut self,
        range: TextRange,
        nodes: impl ExactSizeIterator<Item = NodeKey>,
    ) -> Result<ListKey, AstStoreError> {
        let index = ListIndex::from_offset(self.lists.ranges.len())?;
        self.lists.ranges.reserve(1);
        self.lists.spans.reserve(1);
        let span = self.append_list_elements_iter(nodes)?;
        self.lists.ranges.push(range);
        self.lists.spans.push(span);
        Ok(ListKey::new(self.id, index))
    }

    /// Parser recovery distinguishes a missing opening token from an empty list.
    pub fn new_missing_node_list(&mut self, range: TextRange) -> Result<ListKey, AstStoreError> {
        let index = ListIndex::from_offset(self.lists.ranges.len())?;
        self.lists.ranges.reserve(1);
        self.lists.spans.reserve(1);
        self.lists.ranges.push(range);
        self.lists.spans.push(ElementSpan::MISSING);
        Ok(ListKey::new(self.id, index))
    }

    /// The factory computes flags from its node views in Go evaluation order.
    pub fn new_modifier_list(
        &mut self,
        range: TextRange,
        nodes: &[NodeKey],
        flags: ModifierFlags,
    ) -> Result<ModifierListKey, AstStoreError> {
        self.new_modifier_list_iter(range, nodes.iter().copied(), flags)
    }

    pub(super) fn new_modifier_list_iter(
        &mut self,
        range: TextRange,
        nodes: impl ExactSizeIterator<Item = NodeKey>,
        flags: ModifierFlags,
    ) -> Result<ModifierListKey, AstStoreError> {
        let index = ModifierListIndex::from_offset(self.lists.modifier_ranges.len())?;
        self.lists.modifier_ranges.reserve(1);
        self.lists.modifier_spans.reserve(1);
        self.lists.modifier_flags.reserve(1);
        let span = self.append_list_elements_iter(nodes)?;
        self.lists.modifier_ranges.push(range);
        self.lists.modifier_spans.push(span);
        self.lists.modifier_flags.push(flags);
        Ok(ModifierListKey::new(self.id, index))
    }

    pub fn set_list_range(&mut self, key: ListKey, range: TextRange) -> Result<(), AstStoreError> {
        let offset = self.lists.list(self.id, key)?;
        self.lists.ranges[offset] = range;
        Ok(())
    }

    pub fn set_modifier_list_range(
        &mut self,
        key: ModifierListKey,
        range: TextRange,
    ) -> Result<(), AstStoreError> {
        let offset = self.lists.modifiers(self.id, key)?;
        self.lists.modifier_ranges[offset] = range;
        Ok(())
    }

    /// Clone identity and span metadata while sharing the immutable element span.
    pub fn clone_node_list(&mut self, key: ListKey) -> Result<ListKey, AstStoreError> {
        let offset = self.lists.list(self.id, key)?;
        let index = ListIndex::from_offset(self.lists.ranges.len())?;
        self.lists.ranges.reserve(1);
        self.lists.spans.reserve(1);
        self.lists.ranges.push(self.lists.ranges[offset]);
        self.lists.spans.push(self.lists.spans[offset]);
        Ok(ListKey::new(self.id, index))
    }

    pub fn clone_modifier_list(
        &mut self,
        key: ModifierListKey,
    ) -> Result<ModifierListKey, AstStoreError> {
        let offset = self.lists.modifiers(self.id, key)?;
        let index = ModifierListIndex::from_offset(self.lists.modifier_ranges.len())?;
        self.lists.modifier_ranges.reserve(1);
        self.lists.modifier_spans.reserve(1);
        self.lists.modifier_flags.reserve(1);
        self.lists.modifier_ranges.push(self.lists.modifier_ranges[offset]);
        self.lists.modifier_spans.push(self.lists.modifier_spans[offset]);
        self.lists.modifier_flags.push(self.lists.modifier_flags[offset]);
        Ok(ModifierListKey::new(self.id, index))
    }
}

macro_rules! list_reads {
    () => {
        pub fn list(&self, key: ListKey) -> Result<ListView<'_>, AstStoreError> {
            let offset = self.lists.list(self.id, key)?;
            Ok(ListView {
                store: self.id,
                range: self.lists.ranges[offset],
                elements: self.lists.spans[offset].elements(&self.lists.elements),
                missing: self.lists.spans[offset].is_missing(),
                foreign: &self.foreign,
            })
        }

        pub fn modifier_list(
            &self,
            key: ModifierListKey,
        ) -> Result<ModifierListView<'_>, AstStoreError> {
            let offset = self.lists.modifiers(self.id, key)?;
            Ok(ModifierListView {
                list: ListView {
                    store: self.id,
                    range: self.lists.modifier_ranges[offset],
                    elements: self.lists.modifier_spans[offset].elements(&self.lists.elements),
                    missing: false,
                    foreign: &self.foreign,
                },
                flags: self.lists.modifier_flags[offset],
            })
        }
    };
}

impl AstStoreBuilder {
    list_reads!();
}

impl AstStore {
    list_reads!();
}

impl AstStoreLease {
    pub fn list(&self, key: ListKey) -> Result<ListView<'_>, AstStoreError> {
        self.0.list(key)
    }

    pub fn modifier_list(
        &self,
        key: ModifierListKey,
    ) -> Result<ModifierListView<'_>, AstStoreError> {
        self.0.modifier_list(key)
    }
}

pub struct ListView<'a> {
    store: AstStoreId,
    range: TextRange,
    elements: &'a [StoredNodeKey],
    missing: bool,
    foreign: &'a [NodeKey],
}

impl<'a> ListView<'a> {
    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub const fn is_missing(&self) -> bool {
        self.missing
    }

    pub fn get(&self, index: usize) -> Option<NodeKey> {
        self.elements.get(index).map(|key| key.resolve(self.store, self.foreign))
    }

    pub fn nodes(
        &self,
    ) -> impl ExactSizeIterator<Item = NodeKey> + DoubleEndedIterator + Clone + 'a + use<'a> {
        let store = self.store;
        let foreign = self.foreign;
        self.elements.iter().map(move |key| key.resolve(store, foreign))
    }

    /// Mirrors `NodeList`'s marker-or-end comparison. The caller resolves the last
    /// node through its retained owner, including lists with a foreign last node.
    pub fn has_trailing_comma(&self, last: Option<&NodeView<'_>>) -> Result<bool, AstStoreError> {
        let expected = self.nodes().next_back();
        let actual = last.map(NodeView::key);
        if expected != actual {
            return Err(AstStoreError::ListLastNodeMismatch { expected, actual });
        }
        Ok(last.is_some_and(|node| {
            node.has_trailing_comma_marker() || node.range().end() < self.range.end()
        }))
    }
}

pub struct ModifierListView<'a> {
    list: ListView<'a>,
    flags: ModifierFlags,
}

/// A raw schema slice, with no node-list identity or range metadata.
pub struct NodeSliceView<'a> {
    store: AstStoreId,
    elements: &'a [StoredNodeKey],
    foreign: &'a [NodeKey],
}

impl<'a> NodeSliceView<'a> {
    pub fn len(&self) -> usize {
        self.elements.len()
    }
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<NodeKey> {
        self.elements.get(index).map(|key| key.resolve(self.store, self.foreign))
    }
    pub fn nodes(
        &self,
    ) -> impl ExactSizeIterator<Item = NodeKey> + DoubleEndedIterator + Clone + use<'a> {
        let store = self.store;
        let foreign = self.foreign;
        self.elements.iter().map(move |key| key.resolve(store, foreign))
    }
}

impl<'a> ModifierListView<'a> {
    pub const fn list(&self) -> &ListView<'a> {
        &self.list
    }

    pub const fn modifier_flags(&self) -> ModifierFlags {
        self.flags
    }
}

const _: () = assert!(size_of::<ElementSpan>() == 8);
