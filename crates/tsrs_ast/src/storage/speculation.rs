//! Speculative syntax has private handles. Only a committed result or an explicit
//! retention barrier can create a stable key for an allocation made in a scope.

use std::marker::PhantomData;

use tsrs_core::TextRange;

use super::lists::ListCheckpoint;
use super::text::TextCheckpoint;
use super::{
    AstStoreBuilder, AstStoreError, ChildEdge, Kind, ListKey, ListView, ModifierListKey,
    ModifierListView, NodeFlags, NodeKey, NodeSliceView, NodeTableIndex, NodeView, PayloadTag,
    TAG_COUNT,
};
use crate::ModifierFlags;

type Brand<'brand> = PhantomData<&'brand mut &'brand ()>;

/// Reuse capacity for ordinary lookahead, or request capacity release when a
/// large captured syntax body is discarded. Allocator residency is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollbackBuffers {
    Reuse,
    Release,
}

/// The underlying error is private because it can contain speculative keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeError(AstStoreError);

impl ScopeError {
    pub(super) const fn wrap(error: AstStoreError) -> Self {
        Self(error)
    }
}

macro_rules! scoped_key {
    ($name:ident, $stable:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name<'brand> {
            key: $stable,
            brand: Brand<'brand>,
        }

        impl<'brand> $name<'brand> {
            pub(super) const fn wrap(key: $stable) -> Self {
                Self { key, brand: PhantomData }
            }

            pub(super) const fn raw(self) -> $stable {
                self.key
            }
        }
    };
}
scoped_key!(ScopedNode, NodeKey);
scoped_key!(ScopedList, ListKey);
scoped_key!(ScopedModifiers, ModifierListKey);

struct Checkpoint {
    lengths: Vec<usize>,
    last_tables: [Option<NodeTableIndex>; TAG_COUNT],
    lists: ListCheckpoint,
    texts: TextCheckpoint,
    foreign: usize,
    foreign_lists: usize,
    foreign_modifiers: usize,
}

impl Checkpoint {
    fn capture(writer: &AstStoreBuilder) -> Self {
        Self {
            lengths: writer.payloads.lengths(),
            last_tables: writer.last_tables,
            lists: writer.lists.checkpoint(),
            texts: writer.texts.checkpoint(),
            foreign: writer.foreign.len(),
            foreign_lists: writer.foreign_lists.len(),
            foreign_modifiers: writer.foreign_modifiers.len(),
        }
    }

    fn predates(&self, target: NodeKey) -> bool {
        self.lengths
            .get(target.index().table().offset())
            .is_some_and(|&len| target.index().element() < len)
    }
}

trait Journal {
    fn before_write(&mut self, writer: &AstStoreBuilder);
    fn pin_target(&mut self, target: NodeKey);
    fn pin_all(&mut self);
}

struct Frame<'parent> {
    parent: Option<&'parent mut dyn Journal>,
    checkpoint: Option<Box<Checkpoint>>,
    keep: bool,
    buffers: RollbackBuffers,
}

impl Frame<'_> {
    fn new(buffers: RollbackBuffers, parent: Option<&mut dyn Journal>) -> Frame<'_> {
        Frame { parent, checkpoint: None, keep: false, buffers }
    }
}

impl Journal for Frame<'_> {
    fn before_write(&mut self, writer: &AstStoreBuilder) {
        // All ancestors must observe the state before this write. Token-only
        // speculation leaves every checkpoint uninitialized and allocates none.
        if let Some(parent) = &mut self.parent {
            parent.before_write(writer);
        }
        if !self.keep && self.checkpoint.is_none() {
            self.checkpoint = Some(Box::new(Checkpoint::capture(writer)));
        }
    }

    fn pin_target(&mut self, target: NodeKey) {
        if self.checkpoint.as_ref().is_some_and(|checkpoint| checkpoint.predates(target)) {
            self.keep = true;
        }
        if let Some(parent) = &mut self.parent {
            parent.pin_target(target);
        }
    }

    fn pin_all(&mut self) {
        self.keep = true;
        if let Some(parent) = &mut self.parent {
            parent.pin_all();
        }
    }
}

/// Exclusive writer with freshly branded syntax handles. The writer and raw
/// keys stay private. Views borrow this access; recursion copies scoped handles.
pub struct AstScope<'brand, 'borrow> {
    pub(super) writer: &'borrow mut AstStoreBuilder,
    frame: Frame<'borrow>,
    brand: Brand<'brand>,
}

impl Drop for AstScope<'_, '_> {
    fn drop(&mut self) {
        if !self.frame.keep
            && let Some(checkpoint) = self.frame.checkpoint.take()
        {
            self.writer.restore(&checkpoint, self.frame.buffers);
        }
    }
}

impl AstStoreBuilder {
    fn restore(&mut self, checkpoint: &Checkpoint, buffers: RollbackBuffers) {
        let release = buffers == RollbackBuffers::Release;
        self.payloads.restore(&checkpoint.lengths, release);
        self.last_tables = checkpoint.last_tables;
        self.lists.restore(checkpoint.lists, release);
        self.texts.restore(checkpoint.texts, release);
        for key in self.foreign.drain(checkpoint.foreign..) {
            self.foreign_index.remove(&key);
        }
        for key in self.foreign_lists.drain(checkpoint.foreign_lists..) {
            self.foreign_list_index.remove(&key);
        }
        for key in self.foreign_modifiers.drain(checkpoint.foreign_modifiers..) {
            self.foreign_modifier_index.remove(&key);
        }
        if release {
            self.foreign.shrink_to_fit();
            self.foreign_index.shrink_to_fit();
            self.foreign_lists.shrink_to_fit();
            self.foreign_list_index.shrink_to_fit();
            self.foreign_modifiers.shrink_to_fit();
            self.foreign_modifier_index.shrink_to_fit();
        }
        // Parser allocation counters deliberately survive every rewind.
    }

    pub fn look_ahead<'borrow, R>(
        &'borrow mut self,
        buffers: RollbackBuffers,
        callback: impl for<'brand> FnOnce(&mut AstScope<'brand, 'borrow>) -> R,
    ) -> R {
        let mut scope =
            AstScope { writer: self, frame: Frame::new(buffers, None), brand: PhantomData };
        callback(&mut scope)
    }
}

impl<'brand> AstScope<'brand, '_> {
    pub fn memory(&self) -> super::AstStoreMemory {
        self.writer.memory()
    }

    pub fn has_checkpoint(&self) -> bool {
        self.frame.checkpoint.is_some()
    }

    pub fn node_table_lengths(&self) -> Vec<usize> {
        self.writer.node_table_lengths()
    }

    /// Lends construction access without creating another rollback boundary.
    /// The returned writer forwards every write/escape to this scope's journal;
    /// dropping it leaves rollback to the scope that owns the checkpoint.
    pub fn borrow_writer(&mut self) -> AstScope<'brand, '_> {
        let buffers = self.frame.buffers;
        let mut frame = Frame::new(buffers, Some(&mut self.frame));
        frame.keep = true;
        AstScope { writer: self.writer, frame, brand: PhantomData }
    }

    pub(super) fn before_write(&mut self) {
        self.frame.before_write(self.writer);
    }

    pub(super) fn pin_target(&mut self, key: NodeKey) {
        self.frame.pin_target(key);
    }

    pub fn look_ahead<'borrow, R>(
        &'borrow mut self,
        buffers: RollbackBuffers,
        callback: impl for<'inner> FnOnce(&mut AstScope<'inner, 'borrow>) -> R,
    ) -> R {
        let mut scope = AstScope {
            writer: self.writer,
            frame: Frame::new(buffers, Some(&mut self.frame)),
            brand: PhantomData,
        };
        callback(&mut scope)
    }

    pub fn new_node(
        &mut self,
        kind: Kind,
        flags: NodeFlags,
        range: TextRange,
    ) -> Result<ScopedNode<'brand>, ScopeError> {
        self.before_write();
        self.writer.new_node(kind, flags, range).map(ScopedNode::wrap).map_err(ScopeError::wrap)
    }

    pub fn new_token(
        &mut self,
        kind: Kind,
        flags: NodeFlags,
        range: TextRange,
    ) -> Result<ScopedNode<'brand>, ScopeError> {
        self.before_write();
        self.writer.new_token(kind, flags, range).map(ScopedNode::wrap).map_err(ScopeError::wrap)
    }

    pub fn clone_node(
        &mut self,
        key: ScopedNode<'brand>,
    ) -> Result<ScopedNode<'brand>, ScopeError> {
        self.before_write();
        self.writer.clone_node(key.raw()).map(ScopedNode::wrap).map_err(ScopeError::wrap)
    }

    pub fn node(&self, key: ScopedNode<'brand>) -> Result<ScopedNodeView<'brand, '_>, ScopeError> {
        self.writer
            .node(key.raw())
            .map(|inner| ScopedNodeView { inner, brand: PhantomData })
            .map_err(ScopeError::wrap)
    }

    pub fn set_flags(
        &mut self,
        key: ScopedNode<'brand>,
        flags: NodeFlags,
    ) -> Result<(), ScopeError> {
        self.writer.set_flags(key.raw(), flags).map_err(ScopeError::wrap)
    }

    pub fn set_range(
        &mut self,
        key: ScopedNode<'brand>,
        range: TextRange,
    ) -> Result<(), ScopeError> {
        self.writer.set_range(key.raw(), range).map_err(ScopeError::wrap)
    }

    pub fn set_parent(
        &mut self,
        key: ScopedNode<'brand>,
        parent: Option<ScopedNode<'brand>>,
    ) -> Result<(), ScopeError> {
        self.before_write();
        self.pin_target(key.raw());
        self.writer.set_parent(key.raw(), parent.map(ScopedNode::raw)).map_err(ScopeError::wrap)
    }

    pub fn mark_trailing_comma(&mut self, key: ScopedNode<'brand>) -> Result<(), ScopeError> {
        self.writer.mark_trailing_comma(key.raw()).map_err(ScopeError::wrap)
    }

    pub fn new_node_list(
        &mut self,
        range: TextRange,
        nodes: &[ScopedNode<'brand>],
    ) -> Result<ScopedList<'brand>, ScopeError> {
        self.before_write();
        self.writer
            .new_node_list_iter(range, nodes.iter().map(|key| key.raw()))
            .map(ScopedList::wrap)
            .map_err(ScopeError::wrap)
    }

    pub fn new_missing_node_list(
        &mut self,
        range: TextRange,
    ) -> Result<ScopedList<'brand>, ScopeError> {
        self.before_write();
        self.writer.new_missing_node_list(range).map(ScopedList::wrap).map_err(ScopeError::wrap)
    }

    pub fn new_modifier_list(
        &mut self,
        range: TextRange,
        nodes: &[ScopedNode<'brand>],
        flags: ModifierFlags,
    ) -> Result<ScopedModifiers<'brand>, ScopeError> {
        self.before_write();
        self.writer
            .new_modifier_list_iter(range, nodes.iter().map(|key| key.raw()), flags)
            .map(ScopedModifiers::wrap)
            .map_err(ScopeError::wrap)
    }

    pub fn list(&self, key: ScopedList<'brand>) -> Result<ScopedListView<'brand, '_>, ScopeError> {
        self.writer
            .list(key.raw())
            .map(|inner| ScopedListView { inner, brand: PhantomData })
            .map_err(ScopeError::wrap)
    }

    pub fn modifier_list(
        &self,
        key: ScopedModifiers<'brand>,
    ) -> Result<ScopedModifiersView<'brand, '_>, ScopeError> {
        self.writer
            .modifier_list(key.raw())
            .map(|inner| ScopedModifiersView { inner, brand: PhantomData })
            .map_err(ScopeError::wrap)
    }

    pub fn set_list_range(
        &mut self,
        key: ScopedList<'brand>,
        range: TextRange,
    ) -> Result<(), ScopeError> {
        self.writer.set_list_range(key.raw(), range).map_err(ScopeError::wrap)
    }

    pub fn set_modifier_list_range(
        &mut self,
        key: ScopedModifiers<'brand>,
        range: TextRange,
    ) -> Result<(), ScopeError> {
        self.writer.set_modifier_list_range(key.raw(), range).map_err(ScopeError::wrap)
    }

    pub fn clone_node_list(
        &mut self,
        key: ScopedList<'brand>,
    ) -> Result<ScopedList<'brand>, ScopeError> {
        self.before_write();
        self.writer.clone_node_list(key.raw()).map(ScopedList::wrap).map_err(ScopeError::wrap)
    }

    pub fn clone_modifier_list(
        &mut self,
        key: ScopedModifiers<'brand>,
    ) -> Result<ScopedModifiers<'brand>, ScopeError> {
        self.before_write();
        self.writer
            .clone_modifier_list(key.raw())
            .map(ScopedModifiers::wrap)
            .map_err(ScopeError::wrap)
    }
}

macro_rules! scoped_domain {
    ($scoped:ident, $stable:ty, $import:ident, $rebrand:ident, $retain:ident, $conditional:ident) => {
        impl AstStoreBuilder {
            pub fn $conditional<'borrow>(
                &'borrow mut self,
                buffers: RollbackBuffers,
                callback: impl for<'brand> FnOnce(
                    &mut AstScope<'brand, 'borrow>,
                )
                    -> Result<Option<$scoped<'brand>>, ScopeError>,
            ) -> Result<Option<$stable>, ScopeError> {
                let mut scope =
                    AstScope { writer: self, frame: Frame::new(buffers, None), brand: PhantomData };
                let result = callback(&mut scope)?;
                if let Some(key) = result {
                    scope.frame.keep = true;
                    Ok(Some(key.raw()))
                } else {
                    Ok(None)
                }
            }
        }

        impl<'brand> AstScope<'brand, '_> {
            /// Stable imports may name another retained store. Their ownership
            /// obligations are the same as ordinary foreign edges.
            pub fn $import(&self, key: $stable) -> $scoped<'brand> {
                $scoped::wrap(key)
            }

            /// An ancestor handle can enter a nested scope only for this writer.
            /// A temporary handle from an unrelated writer must never be rebranded.
            pub fn $rebrand(&self, key: $scoped<'_>) -> Result<$scoped<'brand>, ScopeError> {
                if key.raw().store() != self.writer.id {
                    return Err(ScopeError::wrap(AstStoreError::WrongStore {
                        expected: self.writer.id,
                        actual: key.raw().store(),
                    }));
                }
                Ok($scoped::wrap(key.raw()))
            }

            /// Escapes to parser-owned state pin every enclosing frame before
            /// returning a stable key. This retains the whole allocation prefix.
            pub fn $retain(&mut self, key: $scoped<'brand>) -> $stable {
                self.frame.pin_all();
                key.raw()
            }

            pub fn $conditional<'borrow>(
                &'borrow mut self,
                buffers: RollbackBuffers,
                callback: impl for<'inner> FnOnce(
                    &mut AstScope<'inner, 'borrow>,
                )
                    -> Result<Option<$scoped<'inner>>, ScopeError>,
            ) -> Result<Option<$scoped<'brand>>, ScopeError> {
                let mut scope = AstScope {
                    writer: self.writer,
                    frame: Frame::new(buffers, Some(&mut self.frame)),
                    brand: PhantomData,
                };
                let result = callback(&mut scope)?;
                if let Some(key) = result {
                    scope.frame.keep = true;
                    Ok(Some($scoped::wrap(key.raw())))
                } else {
                    Ok(None)
                }
            }
        }
    };
}
scoped_domain!(ScopedNode, NodeKey, import_node, import_scoped_node, retain_node, try_parse_node);
scoped_domain!(ScopedList, ListKey, import_list, import_scoped_list, retain_list, try_parse_list);
scoped_domain!(
    ScopedModifiers,
    ModifierListKey,
    import_modifiers,
    import_scoped_modifiers,
    retain_modifiers,
    try_parse_modifiers
);

pub struct ScopedNodeView<'brand, 'a> {
    pub(super) inner: NodeView<'a>,
    brand: Brand<'brand>,
}

impl<'brand, 'a> ScopedNodeView<'brand, 'a> {
    pub fn key(&self) -> ScopedNode<'brand> {
        ScopedNode::wrap(self.inner.key())
    }
    pub fn kind(&self) -> Kind {
        self.inner.kind()
    }
    pub fn flags(&self) -> NodeFlags {
        self.inner.flags()
    }
    pub fn range(&self) -> TextRange {
        self.inner.range()
    }
    pub fn payload_tag(&self) -> PayloadTag {
        self.inner.payload_tag()
    }
    pub fn source_text(&self) -> Option<&'a str> {
        self.inner.source_text()
    }
    pub fn parent(&self) -> Option<ScopedNode<'brand>> {
        self.inner.parent().map(ScopedNode::wrap)
    }
    pub fn has_trailing_comma_marker(&self) -> bool {
        self.inner.has_trailing_comma_marker()
    }
}

pub struct ScopedListView<'brand, 'a> {
    inner: ListView<'a>,
    brand: Brand<'brand>,
}
pub struct ScopedModifiersView<'brand, 'a> {
    inner: ModifierListView<'a>,
    brand: Brand<'brand>,
}
pub struct ScopedNodeSlice<'brand, 'a> {
    inner: NodeSliceView<'a>,
    brand: Brand<'brand>,
}

impl<'a> ScopedNodeSlice<'_, 'a> {
    pub(super) fn wrap(inner: NodeSliceView<'a>) -> Self {
        Self { inner, brand: PhantomData }
    }
}

macro_rules! scoped_sequence {
    ($name:ident, $($suffix:tt)*) => {
        impl<'brand, 'a> $name<'brand, 'a> {
            pub fn len(&self) -> usize { self.inner $($suffix)* .len() }
            pub fn is_empty(&self) -> bool { self.inner $($suffix)* .is_empty() }
            pub fn get(&self, index: usize) -> Option<ScopedNode<'brand>> { self.inner $($suffix)* .get(index).map(ScopedNode::wrap) }
            pub fn nodes(&self) -> impl ExactSizeIterator<Item = ScopedNode<'brand>> + DoubleEndedIterator + Clone + use<'brand, 'a> { self.inner $($suffix)* .nodes().map(ScopedNode::wrap) }
        }
    };
}
scoped_sequence!(ScopedListView,);
scoped_sequence!(ScopedModifiersView, .list());
scoped_sequence!(ScopedNodeSlice,);

impl ScopedListView<'_, '_> {
    pub fn range(&self) -> TextRange {
        self.inner.range()
    }
    pub fn is_missing(&self) -> bool {
        self.inner.is_missing()
    }
    pub fn has_trailing_comma(
        &self,
        last: Option<&ScopedNodeView<'_, '_>>,
    ) -> Result<bool, ScopeError> {
        self.inner.has_trailing_comma(last.map(|node| &node.inner)).map_err(ScopeError::wrap)
    }
}

impl ScopedModifiersView<'_, '_> {
    pub fn range(&self) -> TextRange {
        self.inner.list().range()
    }
    pub fn modifier_flags(&self) -> ModifierFlags {
        self.inner.modifier_flags()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedChildEdge<'brand> {
    Node(ScopedNode<'brand>),
    List(ScopedList<'brand>),
    Modifiers(ScopedModifiers<'brand>),
}

pub struct ScopedChildCursor<'brand> {
    node: ScopedNode<'brand>,
    member: usize,
    element: usize,
}

impl<'brand> ScopedChildCursor<'brand> {
    pub fn new(node: ScopedNode<'brand>) -> Self {
        Self { node, member: 0, element: 0 }
    }
    pub fn next(
        &mut self,
        scope: &AstScope<'brand, '_>,
    ) -> Result<Option<ScopedChildEdge<'brand>>, ScopeError> {
        scope
            .node(self.node)?
            .inner
            .next_child_edge(&mut self.member, &mut self.element)
            .map(|edge| {
                edge.map(|edge| match edge {
                    ChildEdge::Node(key) => ScopedChildEdge::Node(ScopedNode::wrap(key)),
                    ChildEdge::List(key) => ScopedChildEdge::List(ScopedList::wrap(key)),
                    ChildEdge::Modifiers(key) => {
                        ScopedChildEdge::Modifiers(ScopedModifiers::wrap(key))
                    }
                })
            })
            .map_err(ScopeError::wrap)
    }
}
