//! Owned AST columns and owner-borrowed views for the AST storage migration.
//!
//! This backend is not selected by production factories yet. Internal edges stay
//! four bytes; public keys qualify their indexes with the owning store identity.

#![forbid(unsafe_code)]

mod binding;
mod children;
mod deferred;
mod generated;
mod keys;
mod lists;
mod mapped_source;
mod memory;
mod owners;
mod source_file;
mod sparse;
mod speculation;
mod text;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use rustc_hash::FxHashMap;
use tsrs_core::TextRange;

use crate::{Kind, NodeFlags};

pub use binding::{
    BindingData, BindingLease, BindingRead, BindingWrite, DeclarationSnapshot, FlowListView,
    FlowView, SymbolTableEntrySnapshot, SymbolTableView, SymbolView,
};
pub use children::{AstReader, ChildCursor, ChildEdge};
pub use deferred::{
    DeferredListLease, FileBindingContext, LazyBindPhase, LazyBindSession, LazyBindingContext,
    LazyListHooks, LazyListSpec, LazyListState, MemberListKind,
};
pub use generated::*;
use keys::StoredModifierListKey;
pub use keys::{AstStoreId, NodeIndex, NodeKey};
pub use keys::{
    FlowIndex, FlowKey, FlowListIndex, FlowListKey, SymbolIndex, SymbolKey, SymbolTableIndex,
    SymbolTableKey,
};
pub use keys::{ListIndex, ListKey, ModifierListIndex, ModifierListKey};
use keys::{NODE_TABLE_CAPACITY, NodeTableIndex, StoredListKey, StoredNodeKey};
use lists::ListColumns;
pub use lists::{ListView, ModifierListView, NodeSliceView};
pub use mapped_source::{
    MappedSourceBundle, MappedSourceBundleMemory, MappedSourceFile, MappedSourceFileView,
    MappedSourceOutput, OwnedMappedDiagnosticDirective,
};
use memory::BufferMemory;
pub use memory::{
    AstBufferMemory, AstMapMemory, AstStoreMemory, BindingMemory, SourceFileInfoMemory,
};
pub use owners::{
    AstAccessError, AstDirectory, AstRead, BindSession, BindingAttempt, FileAstOwner, FileAstPhase,
    ParsedAstGuard,
};
pub use source_file::{
    SourceFileDirectives, SourceFileId, SourceFileInfo, SourceFileInfoDraft, SourceFileInfoError,
    SourceFileParseResults,
};
pub use speculation::{
    AstScope, RollbackBuffers, ScopeError, ScopedChildCursor, ScopedChildEdge, ScopedList,
    ScopedListView, ScopedModifiers, ScopedModifiersView, ScopedNode, ScopedNodeSlice,
    ScopedNodeView,
};
use text::TextColumns;
pub use text::{TextInput, TextListView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AstStoreError {
    StoreIdExhausted,
    NodeCapacity,
    ForeignCapacity,
    ListCapacity,
    ModifierListCapacity,
    ListElementCapacity,
    OwnedTextCapacity,
    SymbolCapacity,
    SymbolTableCapacity,
    FlowCapacity,
    FlowListCapacity,
    DeclarationCapacity,
    MissingSymbol(SymbolIndex),
    MissingSymbolTable(SymbolTableIndex),
    MissingFlow(FlowIndex),
    MissingFlowList(FlowListIndex),
    FlowLabelWithAntecedent,
    NotFlowLabel,
    FlowFlagClassificationChanged,
    BindingOwnerAlreadyAssigned(AstStoreId),
    UnassignedBindingOwner,
    InvalidTextRange(TextRange),
    UnsupportedPayload(Kind),
    UnsupportedPayloadTag(PayloadTag),
    WrongPayload { expected: PayloadTag, actual: PayloadTag },
    WrongStore { expected: AstStoreId, actual: AstStoreId },
    MissingNode(NodeIndex),
    MissingList(ListIndex),
    MissingModifierList(ModifierListIndex),
    ListLastNodeMismatch { expected: Option<NodeKey>, actual: Option<NodeKey> },
}

#[derive(Default)]
struct CommonColumns {
    kinds: Vec<Kind>,
    flags: Vec<NodeFlags>,
    ranges: Vec<TextRange>,
    parents: Vec<Option<StoredNodeKey>>,
    ids: Vec<AtomicU32>,
    comma_markers: Vec<u64>,
}

impl CommonColumns {
    fn restore(&mut self, len: usize, release: bool) {
        self.kinds.truncate(len);
        self.flags.truncate(len);
        self.ranges.truncate(len);
        self.parents.truncate(len);
        self.ids.truncate(len);
        self.comma_markers.truncate(len.div_ceil(64));
        let remainder = len % 64;
        if remainder != 0 {
            *self.comma_markers.last_mut().expect("nonempty marker prefix") &=
                (1_u64 << remainder) - 1;
        }
        if release {
            self.kinds.shrink_to_fit();
            self.flags.shrink_to_fit();
            self.ranges.shrink_to_fit();
            self.parents.shrink_to_fit();
            self.ids.shrink_to_fit();
            self.comma_markers.shrink_to_fit();
        }
    }

    fn len(&self) -> usize {
        self.flags.len()
    }

    fn reserve_node(&mut self, variable_kind: bool) {
        // Reserve every column before changing any lengths. A failed reserve
        // leaves an unpublished builder with consistent column lengths.
        if variable_kind {
            self.kinds.reserve(1);
        }
        self.flags.reserve(1);
        self.ranges.reserve(1);
        self.parents.reserve(1);
        self.ids.reserve(1);
        if self.len().is_multiple_of(64) {
            self.comma_markers.reserve(1);
        }
    }
}

impl BufferMemory for CommonColumns {
    fn buffer_memory(&self) -> AstBufferMemory {
        self.kinds.buffer_memory()
            + self.flags.buffer_memory()
            + self.ranges.buffer_memory()
            + self.parents.buffer_memory()
            + self.ids.buffer_memory()
            + self.comma_markers.buffer_memory()
    }
}

/// Exclusive construction storage; finishing consumes all mutable access.
pub struct AstStoreBuilder {
    id: AstStoreId,
    binding_owner: Option<AstStoreId>,
    source: Arc<str>,
    foreign: Vec<NodeKey>,
    foreign_index: FxHashMap<NodeKey, StoredNodeKey>,
    foreign_lists: Vec<ListKey>,
    foreign_list_index: FxHashMap<ListKey, StoredListKey>,
    foreign_modifiers: Vec<ModifierListKey>,
    foreign_modifier_index: FxHashMap<ModifierListKey, StoredModifierListKey>,
    lists: ListColumns,
    payloads: PayloadColumns,
    last_tables: [Option<NodeTableIndex>; TAG_COUNT],
    texts: TextColumns,
    text_count: usize,
    node_count: usize,
}

impl AstStoreBuilder {
    pub fn new(source: impl Into<Arc<str>>) -> Result<Self, AstStoreError> {
        let id = AstStoreId::allocate()?;
        Ok(Self {
            id,
            binding_owner: None,
            source: source.into(),
            foreign: Vec::new(),
            foreign_index: FxHashMap::default(),
            foreign_lists: Vec::new(),
            foreign_list_index: FxHashMap::default(),
            foreign_modifiers: Vec::new(),
            foreign_modifier_index: FxHashMap::default(),
            lists: ListColumns::default(),
            payloads: PayloadColumns::default(),
            last_tables: [None; TAG_COUNT],
            texts: TextColumns::default(),
            text_count: 0,
            node_count: 0,
        })
    }

    pub const fn id(&self) -> AstStoreId {
        self.id
    }

    pub const fn text_count(&self) -> usize {
        self.text_count
    }

    pub const fn node_count(&self) -> usize {
        self.node_count
    }

    /// Allocate common columns. Typed factories will also supply payload columns.
    pub fn new_node(
        &mut self,
        kind: Kind,
        flags: NodeFlags,
        range: TextRange,
    ) -> Result<NodeKey, AstStoreError> {
        let index = self.prepare_node(PayloadTag::CommonOnly)?;
        Ok(self.push_node(index, kind, flags, range, PayloadTag::CommonOnly))
    }

    fn prepare_node(&mut self, tag: PayloadTag) -> Result<NodeIndex, AstStoreError> {
        let limit = NODE_TABLE_CAPACITY;
        let table = match self.last_tables[tag as usize] {
            Some(table) if self.payloads.common(table).len() < limit => table,
            _ => {
                let table = self.payloads.new_table(tag)?;
                self.last_tables[tag as usize] = Some(table);
                table
            }
        };
        let index = NodeIndex::in_table(table, self.payloads.common(table).len())?;
        self.payloads.common_mut(table).reserve_node(tag.fixed_kind().is_none());
        Ok(index)
    }

    fn push_node(
        &mut self,
        index: NodeIndex,
        kind: Kind,
        flags: NodeFlags,
        range: TextRange,
        payload: PayloadTag,
    ) -> NodeKey {
        let common = self.payloads.common_mut(index.table());
        debug_assert_eq!(common.len(), index.element());
        if payload.fixed_kind().is_none() {
            common.kinds.push(kind);
        }
        common.flags.push(flags);
        common.ranges.push(range);
        common.parents.push(None);
        common.ids.push(AtomicU32::new(0));
        if index.element().is_multiple_of(64) {
            common.comma_markers.push(0);
        }
        self.node_count += 1;
        NodeKey::new(self.id, index)
    }

    pub fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstStoreError> {
        let (common, tag) = self.payloads.node(self.id, key)?;
        Ok(NodeView {
            key,
            offset: key.index().element(),
            tag,
            source: &self.source,
            common,
            foreign: &self.foreign,
            foreign_lists: &self.foreign_lists,
            foreign_modifiers: &self.foreign_modifiers,
            binding_owner: self.binding_owner,
            payloads: &self.payloads,
            texts: &self.texts,
            lists: &self.lists,
        })
    }

    pub fn set_flags(&mut self, key: NodeKey, flags: NodeFlags) -> Result<(), AstStoreError> {
        self.payloads.node(self.id, key)?;
        self.payloads.common_mut(key.index().table()).flags[key.index().element()] = flags;
        Ok(())
    }

    pub fn set_range(&mut self, key: NodeKey, range: TextRange) -> Result<(), AstStoreError> {
        self.payloads.node(self.id, key)?;
        self.payloads.common_mut(key.index().table()).ranges[key.index().element()] = range;
        Ok(())
    }

    pub fn set_parent(
        &mut self,
        key: NodeKey,
        parent: Option<NodeKey>,
    ) -> Result<(), AstStoreError> {
        self.payloads.node(self.id, key)?;
        let parent = parent.map(|parent| self.encode(parent)).transpose()?;
        self.payloads.common_mut(key.index().table()).parents[key.index().element()] = parent;
        Ok(())
    }

    pub fn mark_trailing_comma(&mut self, key: NodeKey) -> Result<(), AstStoreError> {
        self.payloads.node(self.id, key)?;
        let offset = key.index().element();
        self.payloads.common_mut(key.index().table()).comma_markers[offset / 64] |=
            1 << (offset % 64);
        Ok(())
    }

    fn encode(&mut self, key: NodeKey) -> Result<StoredNodeKey, AstStoreError> {
        if key.store() == self.id {
            self.payloads.node(self.id, key)?;
            return Ok(StoredNodeKey::local(key.index()));
        }
        if let Some(&encoded) = self.foreign_index.get(&key) {
            return Ok(encoded);
        }
        let encoded = StoredNodeKey::foreign(self.foreign.len())?;
        self.foreign.reserve(1);
        self.foreign_index.reserve(1);
        self.foreign_index.insert(key, encoded);
        self.foreign.push(key);
        Ok(encoded)
    }

    fn payload_offset(&self, key: NodeKey, expected: PayloadTag) -> Result<usize, AstStoreError> {
        let (_, actual) = self.payloads.node(self.id, key)?;
        if actual != expected {
            return Err(AstStoreError::WrongPayload { expected, actual });
        }
        Ok(key.index().element())
    }

    fn check_bindings(&self, bindings: &BindingData) -> Result<(), AstStoreError> {
        let expected = self.binding_owner.ok_or(AstStoreError::UnassignedBindingOwner)?;
        if bindings.id() != expected {
            return Err(AstStoreError::WrongStore { expected, actual: bindings.id() });
        }
        Ok(())
    }

    pub fn finish(self) -> AstStore {
        AstStore {
            id: self.id,
            binding_owner: self.binding_owner,
            source: self.source,
            foreign: self.foreign,
            foreign_lists: self.foreign_lists,
            foreign_modifiers: self.foreign_modifiers,
            lists: self.lists,
            payloads: self.payloads,
            texts: self.texts,
            text_count: self.text_count,
            node_count: self.node_count,
        }
    }
}

/// Published columns. Ordinary fields have no mutation API.
pub struct AstStore {
    id: AstStoreId,
    binding_owner: Option<AstStoreId>,
    source: Arc<str>,
    foreign: Vec<NodeKey>,
    foreign_lists: Vec<ListKey>,
    foreign_modifiers: Vec<ModifierListKey>,
    lists: ListColumns,
    payloads: PayloadColumns,
    texts: TextColumns,
    text_count: usize,
    node_count: usize,
}

impl AstStore {
    pub const fn id(&self) -> AstStoreId {
        self.id
    }

    pub const fn text_count(&self) -> usize {
        self.text_count
    }

    pub const fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstStoreError> {
        let (common, tag) = self.payloads.node(self.id, key)?;
        Ok(NodeView {
            key,
            offset: key.index().element(),
            tag,
            source: &self.source,
            common,
            foreign: &self.foreign,
            foreign_lists: &self.foreign_lists,
            foreign_modifiers: &self.foreign_modifiers,
            binding_owner: self.binding_owner,
            payloads: &self.payloads,
            texts: &self.texts,
            lists: &self.lists,
        })
    }
}

/// Retains published storage. Views borrow this lease rather than a static pointer.
#[derive(Clone)]
pub struct AstStoreLease(Arc<AstStore>);

impl AstStoreLease {
    pub fn id(&self) -> AstStoreId {
        self.0.id
    }

    pub fn downgrade(&self) -> std::sync::Weak<AstStore> {
        Arc::downgrade(&self.0)
    }

    pub fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstStoreError> {
        self.0.node(key)
    }
}

impl From<AstStore> for AstStoreLease {
    fn from(store: AstStore) -> Self {
        Self(Arc::new(store))
    }
}

impl From<Arc<AstStore>> for AstStoreLease {
    fn from(store: Arc<AstStore>) -> Self {
        Self(store)
    }
}

pub struct NodeView<'a> {
    key: NodeKey,
    offset: usize,
    tag: PayloadTag,
    source: &'a str,
    common: &'a CommonColumns,
    foreign: &'a [NodeKey],
    foreign_lists: &'a [ListKey],
    foreign_modifiers: &'a [ModifierListKey],
    binding_owner: Option<AstStoreId>,
    payloads: &'a PayloadColumns,
    texts: &'a TextColumns,
    lists: &'a ListColumns,
}

impl<'a> NodeView<'a> {
    fn check_payload(&self, expected: PayloadTag) -> Result<(), AstStoreError> {
        let actual = self.tag;
        if actual != expected {
            return Err(AstStoreError::WrongPayload { expected, actual });
        }
        Ok(())
    }
    pub const fn key(&self) -> NodeKey {
        self.key
    }

    pub fn kind(&self) -> Kind {
        self.tag.fixed_kind().unwrap_or_else(|| self.common.kinds[self.offset])
    }

    pub fn flags(&self) -> NodeFlags {
        self.common.flags[self.offset]
    }

    pub fn range(&self) -> TextRange {
        self.common.ranges[self.offset]
    }

    pub fn parent(&self) -> Option<NodeKey> {
        self.common.parents[self.offset]
            .map(|parent| parent.resolve(self.key.store(), self.foreign))
    }

    pub fn has_trailing_comma_marker(&self) -> bool {
        self.common.comma_markers[self.offset / 64] & (1 << (self.offset % 64)) != 0
    }

    pub fn source_text(&self) -> Option<&'a str> {
        let range = self.range();
        self.source.get(range.source_pos()? as usize..range.source_end()? as usize)
    }

    /// Assigns the encoded compiler id lazily; storage keys do not allocate it.
    pub fn get_or_assign_id(&self, allocate: impl FnOnce() -> NonZeroU32) -> NonZeroU32 {
        let cell = &self.common.ids[self.offset];
        // Relaxed: the id is immutable and publishes no other AST state.
        if let Some(id) = NonZeroU32::new(cell.load(Ordering::Relaxed)) {
            return id;
        }
        let proposed = allocate();
        // Relaxed: only the winning immutable id is needed, as in get_node_id.
        let id = cell
            .compare_exchange(0, proposed.get(), Ordering::Relaxed, Ordering::Relaxed)
            .unwrap_or_else(|id| id);
        NonZeroU32::new(if id == 0 { proposed.get() } else { id })
            .expect("node ids are assigned nonzero values")
    }
}
