//! Traversal state owns keys, so recursive parser/binder mutations do not keep
//! a reference into a growable column. List edges preserve the schema's order.

use super::{
    AstStore, AstStoreBuilder, AstStoreError, AstStoreLease, ListKey, ListView, ModifierListKey,
    ModifierListView, NodeKey, NodeView,
};

pub trait AstReader {
    fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstStoreError>;
    fn list(&self, key: ListKey) -> Result<ListView<'_>, AstStoreError>;
    fn modifier_list(&self, key: ModifierListKey) -> Result<ModifierListView<'_>, AstStoreError>;
}

macro_rules! reader {
    ($($owner:ty),+ $(,)?) => {$(
        impl AstReader for $owner {
            fn node(&self, key: NodeKey) -> Result<NodeView<'_>, AstStoreError> {
                Self::node(self, key)
            }
            fn list(&self, key: ListKey) -> Result<ListView<'_>, AstStoreError> {
                Self::list(self, key)
            }
            fn modifier_list(&self, key: ModifierListKey) -> Result<ModifierListView<'_>, AstStoreError> {
                Self::modifier_list(self, key)
            }
        }
    )+};
}
reader!(AstStoreBuilder, AstStore, AstStoreLease);

/// A schema child edge, with raw node spans flattened into individual nodes.
///
/// A list remains one edge; the caller resolves it through its reader and visits
/// its nodes before advancing to the next edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildEdge {
    Node(NodeKey),
    List(ListKey),
    Modifiers(ModifierListKey),
}

pub struct ChildCursor {
    node: NodeKey,
    member: usize,
    element: usize,
}

impl ChildCursor {
    pub const fn new(node: NodeKey) -> Self {
        Self { node, member: 0, element: 0 }
    }

    /// Each step borrows the reader only until the copied edge is returned.
    /// Construction can then continue through the same exclusive builder.
    pub fn next(
        &mut self,
        reader: &(impl AstReader + ?Sized),
    ) -> Result<Option<ChildEdge>, AstStoreError> {
        reader.node(self.node)?.next_child_edge(&mut self.member, &mut self.element)
    }
}
