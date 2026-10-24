//! Owned file binding data. Published access is guarded; views borrow the
//! actual guard, while initial binding uses exclusive mutable access.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rustc_hash::FxHashMap;

use super::generated::BindingAnnotations;
use super::keys::{FOREIGN_EDGE, MAX_INDEX, StoredNodeKey};
use super::memory::{AstBufferMemory, AstMapMemory, BindingMemory, BufferMemory};
use super::text::{StoredText, TextColumns};
use super::{
    AstAccessError, AstStore, AstStoreBuilder, AstStoreError, AstStoreId, AstStoreLease, FlowIndex,
    FlowKey, FlowListIndex, FlowListKey, NodeKey, SymbolIndex, SymbolKey, TextInput,
};
use crate::{FlowFlags, SymbolFlags};

#[path = "symbol_tables.rs"]
mod tables;
pub use tables::{SymbolTableEntrySnapshot, SymbolTableView};
use tables::{SymbolTableExtra, SymbolTableRecord};

#[derive(Clone, Copy, Default)]
struct DeclarationSpan {
    start: u32,
    len: u32,
}

/// A saved declaration sequence, without a borrow or ownership of binding data.
///
/// Keep the binding owner alive separately, then use `BindingData::read_declarations`
/// under a fresh guard after callbacks which can force deferred lists. Existing
/// declaration elements are immutable; replacement/append installs a new span.
#[derive(Clone, Copy)]
pub struct DeclarationSnapshot {
    owner: AstStoreId,
    span: DeclarationSpan,
}

impl DeclarationSnapshot {
    pub const fn len(self) -> usize {
        self.span.len as usize
    }

    pub const fn is_empty(self) -> bool {
        self.span.len == 0
    }
}

struct SymbolRecord {
    flags: SymbolFlags,
    id: AtomicU32,
    name: StoredText,
    declarations: DeclarationSpan,
    parent: Option<SymbolIndex>,
    // High bit: value declaration is declarations[0]. Low bits: optional,
    // one-based auxiliary index. These are integer tags, never pointer bits.
    auxiliary: u32,
}

#[derive(Default)]
struct SymbolAuxiliary {
    value_declaration: Option<StoredNodeKey>,
    members: Option<super::SymbolTableIndex>,
    exports: Option<super::SymbolTableIndex>,
    export_symbol: Option<SymbolIndex>,
}

struct FlowRecord {
    flags: FlowFlags,
    node: Option<StoredNodeKey>,
    // Label flags select a FlowListIndex; other flows select a FlowIndex.
    // Only checked construction/setters write this integer domain.
    link: u32,
}

struct FlowListRecord {
    flow: FlowIndex,
    next: Option<FlowListIndex>,
}

#[derive(Default)]
pub(super) struct BindingChecks {
    pub(super) nodes: Vec<NodeKey>,
    pub(super) fragments: Vec<AstStoreId>,
}

/// One file's actual symbol/flow storage. Later syntax fragments use this same
/// identity. No legacy semantic pointer or unchecked cell enters this data.
pub struct BindingData {
    id: AstStoreId,
    pub(super) syntax_id: AstStoreId,
    pub(super) annotations: BindingAnnotations,
    source: Arc<str>,
    symbols: Vec<SymbolRecord>,
    auxiliary: Vec<SymbolAuxiliary>,
    tables: Vec<SymbolTableRecord>,
    table_extras: Vec<SymbolTableExtra>,
    declarations: Vec<StoredNodeKey>,
    flows: Vec<FlowRecord>,
    flow_lists: Vec<FlowListRecord>,
    pub(super) foreign: Vec<NodeKey>,
    foreign_index: FxHashMap<NodeKey, StoredNodeKey>,
    texts: TextColumns,
    pub(super) extensions: FxHashMap<AstStoreId, Arc<AstStore>>,
    pub(super) deferred: Option<Box<super::deferred::DeferredLists>>,
    pub(super) pending_checks: Option<Box<BindingChecks>>,
}

impl BindingData {
    pub fn auxiliary_len(&self) -> usize {
        self.auxiliary.len()
    }

    pub fn new(ast: &mut AstStoreBuilder) -> Result<Self, AstStoreError> {
        if let Some(owner) = ast.binding_owner {
            return Err(AstStoreError::BindingOwnerAlreadyAssigned(owner));
        }
        let id = AstStoreId::allocate()?;
        ast.binding_owner = Some(id);
        Ok(Self {
            id,
            syntax_id: ast.id,
            annotations: BindingAnnotations::default(),
            source: Arc::clone(&ast.source),
            symbols: Vec::new(),
            auxiliary: Vec::new(),
            tables: Vec::new(),
            table_extras: Vec::new(),
            declarations: Vec::new(),
            flows: Vec::new(),
            flow_lists: Vec::new(),
            foreign: Vec::new(),
            foreign_index: FxHashMap::default(),
            texts: TextColumns::default(),
            extensions: FxHashMap::default(),
            deferred: None,
            pending_checks: None,
        })
    }

    pub const fn id(&self) -> AstStoreId {
        self.id
    }

    /// Counts this namespace's storage without counting shared source/fragment
    /// bodies again. Standard hash map backing is reported as a separate bound.
    pub fn memory(&self) -> BindingMemory {
        let (annotations, annotation_maps) = self.annotations.memory();
        let (deferred, deferred_maps) = self.deferred.as_ref().map_or_else(
            || (AstBufferMemory::default(), AstMapMemory::default()),
            |deferred| {
                let (buffers, maps) = deferred.memory();
                (AstBufferMemory::object(size_of_val(&**deferred)) + buffers, maps)
            },
        );
        let validation =
            self.pending_checks.as_ref().map_or_else(AstBufferMemory::default, |checks| {
                AstBufferMemory::object(size_of_val(&**checks))
                    + checks.nodes.buffer_memory()
                    + checks.fragments.buffer_memory()
            });
        BindingMemory {
            inline_bytes: size_of::<Self>(),
            symbols: self.symbols.buffer_memory()
                + self.auxiliary.buffer_memory()
                + self.declarations.buffer_memory(),
            tables: self.table_memory(),
            annotations,
            flows: self.flows.buffer_memory() + self.flow_lists.buffer_memory(),
            text: self.texts.buffer_memory(),
            foreign: self.foreign.buffer_memory(),
            deferred,
            validation,
            maps: annotation_maps
                + deferred_maps
                + AstMapMemory::of(&self.foreign_index)
                + AstMapMemory::of(&self.extensions),
            retained_fragments: self.extensions.len(),
            source_bytes: self.source.len(),
        }
    }

    pub(super) fn check_node_binding(
        &self,
        node: &super::NodeView<'_>,
        tag: super::PayloadTag,
    ) -> Result<(), AstStoreError> {
        node.check_payload(tag)?;
        self.check_owner(node.binding_owner.ok_or(AstStoreError::UnassignedBindingOwner)?)
    }

    pub fn fragment_builder(&self) -> Result<AstStoreBuilder, AstStoreError> {
        let mut ast = AstStoreBuilder::new(Arc::clone(&self.source))?;
        ast.binding_owner = Some(self.id);
        Ok(ast)
    }

    /// Retain private syntax before publishing binding edges that name it.
    /// Initial publication validates all retained dependencies together; later
    /// writes validate only new fragments before releasing the file gate.
    pub fn retain_fragment(
        &mut self,
        mut store: AstStore,
    ) -> Result<AstStoreLease, AstAccessError> {
        match store.binding_owner {
            Some(owner) if owner != self.id => {
                return Err(AstAccessError::UnpermittedOwner(store.id()));
            }
            None => store.binding_owner = Some(self.id),
            Some(_) => {}
        }
        let id = store.id();
        if id == self.syntax_id || self.extensions.contains_key(&id) {
            return Err(AstAccessError::DuplicateStore(id));
        }
        self.extensions.reserve(1);
        if let Some(checks) = &mut self.pending_checks {
            checks.fragments.push(id);
        }
        let store = Arc::new(store);
        self.extensions.insert(id, Arc::clone(&store));
        Ok(AstStoreLease::from(store))
    }

    pub(super) fn symbol_index(&self, key: SymbolKey) -> Result<SymbolIndex, AstStoreError> {
        self.check_owner(key.store())?;
        self.symbols.get(key.index().offset()).ok_or(AstStoreError::MissingSymbol(key.index()))?;
        Ok(key.index())
    }

    pub(super) fn flow_index(&self, key: FlowKey) -> Result<FlowIndex, AstStoreError> {
        self.check_owner(key.store())?;
        self.flows.get(key.index().offset()).ok_or(AstStoreError::MissingFlow(key.index()))?;
        Ok(key.index())
    }

    pub(super) fn flow_list_index(&self, key: FlowListKey) -> Result<FlowListIndex, AstStoreError> {
        self.check_owner(key.store())?;
        self.flow_lists
            .get(key.index().offset())
            .ok_or(AstStoreError::MissingFlowList(key.index()))?;
        Ok(key.index())
    }

    fn check_owner(&self, actual: AstStoreId) -> Result<(), AstStoreError> {
        if actual != self.id {
            return Err(AstStoreError::WrongStore { expected: self.id, actual });
        }
        Ok(())
    }

    pub(super) fn encode_node(&mut self, key: NodeKey) -> Result<StoredNodeKey, AstStoreError> {
        self.track_node(key);
        // Syntax keys are validated by their actual syntax owner when followed.
        if key.store() == self.syntax_id {
            return Ok(StoredNodeKey::local(key.index()));
        }
        if let Some(&stored) = self.foreign_index.get(&key) {
            return Ok(stored);
        }
        let stored = StoredNodeKey::foreign(self.foreign.len())?;
        self.foreign.reserve(1);
        self.foreign_index.reserve(1);
        self.foreign_index.insert(key, stored);
        self.foreign.push(key);
        Ok(stored)
    }

    pub(super) fn track_node(&mut self, key: NodeKey) {
        if let Some(checks) = &mut self.pending_checks {
            checks.nodes.push(key);
        }
    }

    pub(super) fn node_edges(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.declarations
            .iter()
            .copied()
            .chain(self.flows.iter().filter_map(|flow| flow.node))
            .chain(self.auxiliary.iter().filter_map(|auxiliary| auxiliary.value_declaration))
            .map(|key| key.resolve(self.syntax_id, &self.foreign))
    }

    fn declaration_elements(&self, span: DeclarationSpan) -> &[StoredNodeKey] {
        &self.declarations[span.start as usize..span.start as usize + span.len as usize]
    }

    fn declaration_nodes(
        &self,
        span: DeclarationSpan,
    ) -> impl ExactSizeIterator<Item = NodeKey> + DoubleEndedIterator + '_ {
        self.declaration_elements(span)
            .iter()
            .map(|&node| node.resolve(self.syntax_id, &self.foreign))
    }

    /// Reacquire a saved sequence from its actual binding owner. The iterator
    /// borrows this data/guard, not the snapshot, and cannot survive a write.
    pub fn read_declarations(
        &self,
        snapshot: DeclarationSnapshot,
    ) -> Result<impl ExactSizeIterator<Item = NodeKey> + DoubleEndedIterator + '_, AstStoreError>
    {
        self.check_owner(snapshot.owner)?;
        Ok(self.declaration_nodes(snapshot.span))
    }

    fn symbol_auxiliary(&self, symbol: usize) -> Option<&SymbolAuxiliary> {
        let index = self.symbols[symbol].auxiliary & MAX_INDEX;
        (index != 0).then(|| &self.auxiliary[(index - 1) as usize])
    }

    fn symbol_auxiliary_mut(
        &mut self,
        symbol: usize,
    ) -> Result<&mut SymbolAuxiliary, AstStoreError> {
        let mut index = self.symbols[symbol].auxiliary & MAX_INDEX;
        if index == 0 {
            index = u32::try_from(self.auxiliary.len())
                .ok()
                .and_then(|index| index.checked_add(1))
                .filter(|&index| index <= MAX_INDEX)
                .ok_or(AstStoreError::SymbolCapacity)?;
            self.auxiliary.push(SymbolAuxiliary::default());
            self.symbols[symbol].auxiliary |= index;
        }
        Ok(&mut self.auxiliary[(index - 1) as usize])
    }

    pub fn new_symbol(
        &mut self,
        flags: SymbolFlags,
        name: TextInput<'_>,
    ) -> Result<SymbolKey, AstStoreError> {
        let index = SymbolIndex::from_offset(self.symbols.len())?;
        let name = self.texts.encode(&self.source, name)?;
        self.symbols.push(SymbolRecord {
            flags,
            id: AtomicU32::new(0),
            name,
            declarations: DeclarationSpan::default(),
            parent: None,
            auxiliary: 0,
        });
        Ok(SymbolKey::new(self.id, index))
    }

    pub fn symbol(&self, key: SymbolKey) -> Result<SymbolView<'_>, AstStoreError> {
        let index = self.symbol_index(key)?;
        Ok(SymbolView { data: self, index })
    }

    pub fn set_symbol_flags(
        &mut self,
        key: SymbolKey,
        flags: SymbolFlags,
    ) -> Result<(), AstStoreError> {
        let index = self.symbol_index(key)?;
        self.symbols[index.offset()].flags = flags;
        Ok(())
    }

    pub fn set_symbol_parent(
        &mut self,
        key: SymbolKey,
        parent: Option<SymbolKey>,
    ) -> Result<(), AstStoreError> {
        let index = self.symbol_index(key)?;
        let parent = parent.map(|parent| self.symbol_index(parent)).transpose()?;
        self.symbols[index.offset()].parent = parent;
        Ok(())
    }

    pub fn set_symbol_declarations(
        &mut self,
        key: SymbolKey,
        nodes: &[NodeKey],
    ) -> Result<(), AstStoreError> {
        let index = self.symbol_index(key)?.offset();
        let old_value = self.symbol(key)?.value_declaration();
        let start = u32::try_from(self.declarations.len())
            .map_err(|_| AstStoreError::DeclarationCapacity)?;
        let len = u32::try_from(nodes.len()).map_err(|_| AstStoreError::DeclarationCapacity)?;
        start.checked_add(len).ok_or(AstStoreError::DeclarationCapacity)?;
        self.declarations.reserve(nodes.len());
        for &node in nodes {
            match self.encode_node(node) {
                Ok(node) => self.declarations.push(node),
                Err(error) => {
                    self.declarations.truncate(start as usize);
                    return Err(error);
                }
            }
        }
        // Allocate a needed value-declaration tail before changing the span.
        if old_value.is_some() && old_value != nodes.first().copied() {
            self.symbol_auxiliary_mut(index)?;
        }
        self.symbols[index].declarations = DeclarationSpan { start, len };
        self.set_symbol_value_declaration(key, old_value)
    }

    pub fn append_symbol_declarations(
        &mut self,
        key: SymbolKey,
        nodes: &[NodeKey],
    ) -> Result<(), AstStoreError> {
        let index = self.symbol_index(key)?.offset();
        if nodes.is_empty() {
            return Ok(());
        }
        let old = self.symbols[index].declarations;
        let old_value = self.symbol(key)?.value_declaration();
        let buffer_end = u32::try_from(self.declarations.len())
            .map_err(|_| AstStoreError::DeclarationCapacity)?;
        let added = u32::try_from(nodes.len()).map_err(|_| AstStoreError::DeclarationCapacity)?;
        let len = old.len.checked_add(added).ok_or(AstStoreError::DeclarationCapacity)?;
        let copy_old = old.len != 0 && old.start + old.len != buffer_end;
        let start = if old.len == 0 || copy_old { buffer_end } else { old.start };
        start.checked_add(len).ok_or(AstStoreError::DeclarationCapacity)?;
        self.declarations.reserve(if copy_old { len } else { added } as usize);
        if copy_old {
            self.declarations
                .extend_from_within(old.start as usize..old.start as usize + old.len as usize);
        }
        for &node in nodes {
            match self.encode_node(node) {
                Ok(node) => self.declarations.push(node),
                Err(error) => {
                    self.declarations.truncate(buffer_end as usize);
                    return Err(error);
                }
            }
        }
        // Extending the final span never rewrites existing elements, so saved
        // snapshots keep their original prefix. Interleaved spans copy directly
        // within the compact buffer, without a temporary Vec of qualified keys.
        self.symbols[index].declarations = DeclarationSpan { start, len };
        self.set_symbol_value_declaration(key, old_value)
    }

    pub fn set_symbol_value_declaration(
        &mut self,
        key: SymbolKey,
        node: Option<NodeKey>,
    ) -> Result<(), AstStoreError> {
        let index = self.symbol_index(key)?.offset();
        let node = node.map(|node| self.encode_node(node)).transpose()?;
        let first = self.declaration_elements(self.symbols[index].declarations).first().copied();
        let is_first = node.is_some()
            && node.map(|node| node.resolve(self.syntax_id, &self.foreign))
                == first.map(|node| node.resolve(self.syntax_id, &self.foreign));
        if is_first {
            self.symbols[index].auxiliary |= FOREIGN_EDGE;
            if self.symbol_auxiliary(index).is_some() {
                self.symbol_auxiliary_mut(index)?.value_declaration = None;
            }
        } else {
            if node.is_some() || self.symbol_auxiliary(index).is_some() {
                self.symbol_auxiliary_mut(index)?.value_declaration = node;
            }
            self.symbols[index].auxiliary &= MAX_INDEX;
        }
        Ok(())
    }

    pub fn new_flow(
        &mut self,
        flags: FlowFlags,
        node: Option<NodeKey>,
        antecedent: Option<FlowKey>,
    ) -> Result<FlowKey, AstStoreError> {
        if flags.intersects(FlowFlags::Label) && antecedent.is_some() {
            return Err(AstStoreError::FlowLabelWithAntecedent);
        }
        let index = FlowIndex::from_offset(self.flows.len())?;
        let antecedent = antecedent.map(|key| self.flow_index(key)).transpose()?;
        let node = node.map(|node| self.encode_node(node)).transpose()?;
        self.flows.push(FlowRecord {
            flags,
            node,
            link: antecedent.map_or(0, |index| (index.offset() + 1) as u32),
        });
        Ok(FlowKey::new(self.id, index))
    }

    pub fn flow(&self, key: FlowKey) -> Result<FlowView<'_>, AstStoreError> {
        let index = self.flow_index(key)?;
        Ok(FlowView { data: self, index })
    }

    pub fn mark_flow_referenced(&mut self, key: FlowKey) -> Result<(), AstStoreError> {
        let index = self.flow_index(key)?;
        let flags = &mut self.flows[index.offset()].flags;
        *flags |= if flags.intersects(FlowFlags::Referenced) {
            FlowFlags::Shared
        } else {
            FlowFlags::Referenced
        };
        Ok(())
    }

    pub fn set_flow_node(
        &mut self,
        key: FlowKey,
        node: Option<NodeKey>,
    ) -> Result<(), AstStoreError> {
        let index = self.flow_index(key)?;
        let node = node.map(|node| self.encode_node(node)).transpose()?;
        self.flows[index.offset()].node = node;
        Ok(())
    }

    pub fn set_flow_antecedents(
        &mut self,
        key: FlowKey,
        list: Option<FlowListKey>,
    ) -> Result<(), AstStoreError> {
        let index = self.flow_index(key)?;
        if !self.flows[index.offset()].flags.intersects(FlowFlags::Label) {
            return Err(AstStoreError::NotFlowLabel);
        }
        let list = list.map(|key| self.flow_list_index(key)).transpose()?;
        self.flows[index.offset()].link = list.map_or(0, |index| (index.offset() + 1) as u32);
        Ok(())
    }

    pub fn new_flow_list(
        &mut self,
        flow: FlowKey,
        next: Option<FlowListKey>,
    ) -> Result<FlowListKey, AstStoreError> {
        let index = FlowListIndex::from_offset(self.flow_lists.len())?;
        let flow = self.flow_index(flow)?;
        let next = next.map(|key| self.flow_list_index(key)).transpose()?;
        self.flow_lists.push(FlowListRecord { flow, next });
        Ok(FlowListKey::new(self.id, index))
    }

    pub fn flow_list(&self, key: FlowListKey) -> Result<FlowListView<'_>, AstStoreError> {
        let index = self.flow_list_index(key)?;
        Ok(FlowListView { data: self, index })
    }

    pub fn set_flow_list_next(
        &mut self,
        key: FlowListKey,
        next: Option<FlowListKey>,
    ) -> Result<(), AstStoreError> {
        let index = self.flow_list_index(key)?;
        let next = next.map(|key| self.flow_list_index(key)).transpose()?;
        self.flow_lists[index.offset()].next = next;
        Ok(())
    }

    pub fn publish(mut self, syntax: AstStore) -> Result<BindingLease, AstAccessError> {
        if syntax.id() != self.syntax_id {
            return Err(AstStoreError::WrongStore {
                expected: self.syntax_id,
                actual: syntax.id(),
            }
            .into());
        }
        let session =
            super::LazyBindSession::new(&syntax, &mut self, super::LazyBindPhase::Initial);
        session.validate_dependencies(
            &syntax.foreign,
            &syntax.foreign_lists,
            &syntax.foreign_modifiers,
        )?;
        session.validate_binding_data()?;
        session.commit()?;
        Ok(Self::publish_shared(self, Arc::new(syntax)))
    }

    pub(super) fn publish_shared(mut self, syntax: Arc<AstStore>) -> BindingLease {
        debug_assert_eq!(self.syntax_id, syntax.id());
        debug_assert_eq!(syntax.binding_owner, Some(self.id));
        self.mark_unbound_lazy_lists();
        BindingLease(Arc::new(BindingOwner {
            base: syntax,
            data: RwLock::new(self),
            failed: AtomicBool::new(false),
        }))
    }
}

pub struct SymbolView<'a> {
    data: &'a BindingData,
    index: SymbolIndex,
}
impl<'a> SymbolView<'a> {
    fn record(&self) -> &SymbolRecord {
        &self.data.symbols[self.index.offset()]
    }
    pub const fn key(&self) -> SymbolKey {
        SymbolKey::new(self.data.id, self.index)
    }
    pub fn flags(&self) -> SymbolFlags {
        self.record().flags
    }
    pub fn name(&self) -> &'a str {
        self.record().name.resolve(&self.data.source, &self.data.texts)
    }
    pub fn parent(&self) -> Option<SymbolKey> {
        self.record().parent.map(|index| SymbolKey::new(self.data.id, index))
    }
    pub fn declarations(&self) -> impl Iterator<Item = NodeKey> + use<'a> {
        self.data.declaration_nodes(self.record().declarations)
    }
    pub fn declaration_snapshot(&self) -> DeclarationSnapshot {
        DeclarationSnapshot { owner: self.data.id, span: self.record().declarations }
    }
    pub fn value_declaration(&self) -> Option<NodeKey> {
        let record = self.record();
        let node = if record.auxiliary & FOREIGN_EDGE != 0 {
            self.data.declaration_elements(record.declarations).first().copied()
        } else {
            self.data
                .symbol_auxiliary(self.index.offset())
                .and_then(|auxiliary| auxiliary.value_declaration)
        };
        node.map(|node| node.resolve(self.data.syntax_id, &self.data.foreign))
    }
    pub fn get_or_assign_id(&self, allocate: impl FnOnce() -> NonZeroU32) -> NonZeroU32 {
        let cell = &self.record().id;
        // Relaxed: an immutable id, publishing no other binding data.
        if let Some(id) = NonZeroU32::new(cell.load(Ordering::Relaxed)) {
            return id;
        }
        let proposed = allocate();
        // Relaxed: only the winning id is published.
        let old = cell
            .compare_exchange(0, proposed.get(), Ordering::Relaxed, Ordering::Relaxed)
            .unwrap_or_else(|old| old);
        NonZeroU32::new(if old == 0 { proposed.get() } else { old })
            .expect("semantic ids are nonzero")
    }
}

pub struct FlowView<'a> {
    data: &'a BindingData,
    index: FlowIndex,
}
impl FlowView<'_> {
    fn record(&self) -> &FlowRecord {
        &self.data.flows[self.index.offset()]
    }
    pub const fn key(&self) -> FlowKey {
        FlowKey::new(self.data.id, self.index)
    }
    pub fn flags(&self) -> FlowFlags {
        self.record().flags
    }
    pub fn node(&self) -> Option<NodeKey> {
        self.record().node.map(|node| node.resolve(self.data.syntax_id, &self.data.foreign))
    }
    pub fn antecedent(&self) -> Option<FlowKey> {
        let record = self.record();
        (!record.flags.intersects(FlowFlags::Label) && record.link != 0).then(|| {
            FlowKey::new(
                self.data.id,
                FlowIndex::from_offset((record.link - 1) as usize).expect("checked flow link"),
            )
        })
    }
    pub fn antecedents(&self) -> Option<FlowListKey> {
        let record = self.record();
        (record.flags.intersects(FlowFlags::Label) && record.link != 0).then(|| {
            FlowListKey::new(
                self.data.id,
                FlowListIndex::from_offset((record.link - 1) as usize)
                    .expect("checked flow-list link"),
            )
        })
    }
}

pub struct FlowListView<'a> {
    data: &'a BindingData,
    index: FlowListIndex,
}
impl FlowListView<'_> {
    pub const fn key(&self) -> FlowListKey {
        FlowListKey::new(self.data.id, self.index)
    }
    pub fn flow(&self) -> FlowKey {
        FlowKey::new(self.data.id, self.data.flow_lists[self.index.offset()].flow)
    }
    pub fn next(&self) -> Option<FlowListKey> {
        self.data.flow_lists[self.index.offset()]
            .next
            .map(|index| FlowListKey::new(self.data.id, index))
    }
}

struct BindingOwner {
    base: Arc<AstStore>,
    data: RwLock<BindingData>,
    failed: AtomicBool,
}

/// Retains binding data and the syntax it names, without a syntax-to-binding
/// ownership edge. There is one write gate for the whole file binding namespace.
#[derive(Clone)]
pub struct BindingLease(Arc<BindingOwner>);
impl BindingLease {
    pub fn downgrade(&self) -> std::sync::Weak<impl Send + Sync + use<>> {
        Arc::downgrade(&self.0)
    }

    pub fn root_ast(&self) -> AstStoreLease {
        AstStoreLease::from(Arc::clone(&self.0.base))
    }
    pub fn read(&self) -> Result<BindingRead<'_>, AstAccessError> {
        let data = tsrs_core::runslots::blocking(|| self.0.data.read())
            .map_err(|_| AstAccessError::Failed)?;
        if self.is_failed() {
            return Err(AstAccessError::Failed);
        }
        Ok(BindingRead { data })
    }
    pub fn write(&self) -> Result<BindingWrite<'_>, AstAccessError> {
        let mut data = tsrs_core::runslots::blocking(|| self.0.data.write())
            .map_err(|_| AstAccessError::Failed)?;
        if self.is_failed() {
            return Err(AstAccessError::Failed);
        }
        data.pending_checks.get_or_insert_with(Box::default);
        Ok(BindingWrite { owner: &self.0, data, completed: false })
    }
    pub(super) fn is_failed(&self) -> bool {
        // Acquire: observes the terminal cancellation/failure marker.
        self.0.failed.load(Ordering::Acquire)
    }
}

pub struct BindingRead<'a> {
    data: RwLockReadGuard<'a, BindingData>,
}
impl BindingRead<'_> {
    pub fn data(&self) -> &BindingData {
        &self.data
    }
    pub fn extension(&self, store: AstStoreId) -> Result<AstStoreLease, AstAccessError> {
        self.data
            .extensions
            .get(&store)
            .map(|store| AstStoreLease::from(Arc::clone(store)))
            .ok_or(AstAccessError::UnknownStore(store))
    }
    pub(super) fn fragments(&self) -> impl Iterator<Item = AstStoreLease> + '_ {
        self.data.extensions.values().map(|store| AstStoreLease::from(Arc::clone(store)))
    }
}

/// Cancellation or panic after taking exclusive authority is terminal. Existing
/// readers finish before mutation; later readers cannot observe partial changes.
#[must_use]
pub struct BindingWrite<'a> {
    owner: &'a BindingOwner,
    data: RwLockWriteGuard<'a, BindingData>,
    completed: bool,
}
impl BindingWrite<'_> {
    pub fn data(&self) -> &BindingData {
        &self.data
    }
    pub fn data_mut(&mut self) -> &mut BindingData {
        &mut self.data
    }
    pub fn publish_fragment(&mut self, store: AstStore) -> Result<AstStoreLease, AstAccessError> {
        self.data.retain_fragment(store)
    }
    pub fn complete(mut self) -> Result<(), AstAccessError> {
        let session = super::LazyBindSession::new(
            &*self.owner.base,
            &mut self.data,
            super::LazyBindPhase::Late,
        );
        session.commit()?;
        self.completed = true;
        Ok(())
    }
}
impl Drop for BindingWrite<'_> {
    fn drop(&mut self) {
        if !self.completed {
            // Release: reject this version before the write guard is released.
            // The RwLock guards all actual data; this marker grants no access.
            self.owner.failed.store(true, Ordering::Release);
        }
    }
}

const _: () = {
    assert!(size_of::<SymbolRecord>() == 32);
    assert!(size_of::<FlowRecord>() == 12);
    assert!(size_of::<FlowListRecord>() == 8);
};
