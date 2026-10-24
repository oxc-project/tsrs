//! Parser finalization shared by the legacy and owned AST backends.
//! Keys survive mutation; child views never cross the mutable visitor call.

#![forbid(unsafe_code)]

use std::convert::Infallible;

use tsrs_ast::storage::{
    AstScope, AstStoreBuilder, AstStoreError, ChildCursor, ChildEdge, NodeKey, ScopeError,
    ScopedChildCursor, ScopedChildEdge, ScopedNode,
};
use tsrs_ast::{Node, NodeFlags};
use tsrs_core::{P, TextRange};

use crate::factory::ParserFactory;

pub trait ConstructionWriter {
    type Node: Copy;
    type Error;

    fn flags(&self, node: Self::Node) -> Result<NodeFlags, Self::Error>;
    fn set_flags(&mut self, node: Self::Node, flags: NodeFlags) -> Result<(), Self::Error>;
    fn set_range(&mut self, node: Self::Node, range: TextRange) -> Result<(), Self::Error>;
    fn set_parent(
        &mut self,
        node: Self::Node,
        parent: Option<Self::Node>,
    ) -> Result<(), Self::Error>;
    fn for_each_child(
        &mut self,
        node: Self::Node,
        visit: impl FnMut(&mut Self, Self::Node) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}

/// Go `finishNodeWithEnd`, with construction authority passed explicitly.
#[inline]
pub fn finish_node<W: ConstructionWriter>(
    writer: &mut W,
    node: W::Node,
    range: TextRange,
    context_flags: NodeFlags,
    has_parse_error: &mut bool,
) -> Result<W::Node, W::Error> {
    writer.set_range(node, range)?;
    writer.set_flags(node, writer.flags(node)? | context_flags)?;
    if *has_parse_error {
        writer.set_flags(node, writer.flags(node)? | NodeFlags::ThisNodeHasError)?;
        *has_parse_error = false;
    }
    override_child_parents(writer, node)?;
    Ok(node)
}

/// The visitor receives the same writer, so callbacks cannot mutate through a
/// detached shared node reference or retain a column borrow across a write.
#[inline]
pub fn override_child_parents<W: ConstructionWriter>(
    writer: &mut W,
    node: W::Node,
) -> Result<(), W::Error> {
    writer.for_each_child(node, |writer, child| writer.set_parent(child, Some(node)))
}

impl ConstructionWriter for ParserFactory {
    type Node = P<Node>;
    type Error = Infallible;

    #[inline]
    fn flags(&self, node: P<Node>) -> Result<NodeFlags, Infallible> {
        Ok(node.flags())
    }

    #[inline]
    fn set_flags(&mut self, node: P<Node>, flags: NodeFlags) -> Result<(), Infallible> {
        node.set_flags(flags);
        Ok(())
    }

    #[inline]
    fn set_range(&mut self, node: P<Node>, range: TextRange) -> Result<(), Infallible> {
        node.set_loc(range);
        Ok(())
    }

    #[inline]
    fn set_parent(&mut self, node: P<Node>, parent: Option<P<Node>>) -> Result<(), Infallible> {
        node.set_parent(parent);
        Ok(())
    }

    #[inline]
    fn for_each_child(
        &mut self,
        node: P<Node>,
        mut visit: impl FnMut(&mut Self, P<Node>) -> Result<(), Infallible>,
    ) -> Result<(), Infallible> {
        node.for_each_child_static(&mut |child| {
            visit(self, child).unwrap_or_else(|never| match never {});
            false
        });
        Ok(())
    }
}

impl ConstructionWriter for AstStoreBuilder {
    type Node = NodeKey;
    type Error = AstStoreError;

    fn flags(&self, node: NodeKey) -> Result<NodeFlags, AstStoreError> {
        Ok(self.node(node)?.flags())
    }

    fn set_flags(&mut self, node: NodeKey, flags: NodeFlags) -> Result<(), AstStoreError> {
        Self::set_flags(self, node, flags)
    }

    fn set_range(&mut self, node: NodeKey, range: TextRange) -> Result<(), AstStoreError> {
        Self::set_range(self, node, range)
    }

    fn set_parent(&mut self, node: NodeKey, parent: Option<NodeKey>) -> Result<(), AstStoreError> {
        Self::set_parent(self, node, parent)
    }

    fn for_each_child(
        &mut self,
        node: NodeKey,
        mut visit: impl FnMut(&mut Self, NodeKey) -> Result<(), AstStoreError>,
    ) -> Result<(), AstStoreError> {
        let mut cursor = ChildCursor::new(node);
        while let Some(edge) = cursor.next(self)? {
            match edge {
                ChildEdge::Node(child) => visit(self, child)?,
                ChildEdge::List(list) => {
                    let count = self.list(list)?.len();
                    for index in 0..count {
                        let child = self.list(list)?.get(index).expect("index within child list");
                        visit(self, child)?;
                    }
                }
                ChildEdge::Modifiers(list) => {
                    let count = self.modifier_list(list)?.list().len();
                    for index in 0..count {
                        let child = self
                            .modifier_list(list)?
                            .list()
                            .get(index)
                            .expect("index within child modifier list");
                        visit(self, child)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'brand> ConstructionWriter for AstScope<'brand, '_> {
    type Node = ScopedNode<'brand>;
    type Error = ScopeError;

    fn flags(&self, node: Self::Node) -> Result<NodeFlags, ScopeError> {
        Ok(self.node(node)?.flags())
    }
    fn set_flags(&mut self, node: Self::Node, flags: NodeFlags) -> Result<(), ScopeError> {
        Self::set_flags(self, node, flags)
    }
    fn set_range(&mut self, node: Self::Node, range: TextRange) -> Result<(), ScopeError> {
        Self::set_range(self, node, range)
    }
    fn set_parent(
        &mut self,
        node: Self::Node,
        parent: Option<Self::Node>,
    ) -> Result<(), ScopeError> {
        Self::set_parent(self, node, parent)
    }
    fn for_each_child(
        &mut self,
        node: Self::Node,
        mut visit: impl FnMut(&mut Self, Self::Node) -> Result<(), ScopeError>,
    ) -> Result<(), ScopeError> {
        let mut cursor = ScopedChildCursor::new(node);
        while let Some(edge) = cursor.next(self)? {
            match edge {
                ScopedChildEdge::Node(child) => visit(self, child)?,
                ScopedChildEdge::List(list) => {
                    let count = self.list(list)?.len();
                    for index in 0..count {
                        let child = self.list(list)?.get(index).expect("index within scoped list");
                        visit(self, child)?;
                    }
                }
                ScopedChildEdge::Modifiers(list) => {
                    let count = self.modifier_list(list)?.len();
                    for index in 0..count {
                        let child = self
                            .modifier_list(list)?
                            .get(index)
                            .expect("index within scoped modifiers");
                        visit(self, child)?;
                    }
                }
            }
        }
        Ok(())
    }
}
