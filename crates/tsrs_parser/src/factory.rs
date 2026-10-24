//! Exclusive parser construction authority during the owned AST migration.
//!
//! The backend is still the legacy factory; this enforces construction borrows
//! in production grammar code before changing stored node identities.

use tsrs_ast::lazylist::LazyNodeList;
use tsrs_ast::{Kind, ModifierList, Node, NodeFactory, NodeList, SourceFileParseOptions};
use tsrs_core::{P, TextRange};

#[derive(Default)]
pub struct ParserFactory {
    legacy: NodeFactory,
}

impl ParserFactory {
    pub(super) fn with_legacy(legacy: NodeFactory) -> Self {
        Self { legacy }
    }

    pub(super) fn legacy_mut(&mut self) -> &mut NodeFactory {
        &mut self.legacy
    }

    pub fn node_count(&self) -> usize {
        self.legacy.node_count()
    }

    pub fn text_count(&self) -> usize {
        self.legacy.text_count()
    }

    pub(super) fn reparse_cloner(&mut self) -> tsrs_ast::deepclone::ReparseCloner {
        self.legacy_mut().reparse_cloner()
    }

    #[inline]
    pub fn new_source_identifier(
        &mut self,
        text: &'static str,
        text_index: u32,
        loc: TextRange,
    ) -> P<Node> {
        self.legacy_mut().new_source_identifier(text, text_index, loc)
    }

    #[inline]
    pub fn new_source_file(
        &mut self,
        opts: SourceFileParseOptions,
        text: &'static str,
        statements: P<NodeList>,
        end_of_file_token: P<Node>,
    ) -> P<Node> {
        self.legacy_mut().new_source_file(opts, text, statements, end_of_file_token)
    }

    #[inline]
    pub fn new_modifier(&mut self, kind: Kind) -> P<Node> {
        self.legacy_mut().new_modifier(kind)
    }

    #[inline]
    pub fn new_node_list(&mut self, nodes: Vec<P<Node>>) -> P<NodeList> {
        self.legacy_mut().new_node_list(nodes)
    }

    #[inline]
    pub fn new_node_list_from_slice(&mut self, nodes: &[P<Node>]) -> P<NodeList> {
        self.legacy_mut().new_node_list_from_slice(nodes)
    }

    #[inline]
    pub fn new_node_list_from_static(&mut self, nodes: &'static [P<Node>]) -> P<NodeList> {
        self.legacy_mut().new_node_list_from_static(nodes)
    }

    #[inline]
    pub fn new_modifier_list_from_slice(&mut self, nodes: &[P<Node>]) -> P<ModifierList> {
        self.legacy_mut().new_modifier_list_from_slice(nodes)
    }

    #[inline]
    pub fn new_lazy_node_list(
        &mut self,
        loc: TextRange,
        record: &'static LazyNodeList,
    ) -> P<NodeList> {
        self.legacy_mut().new_lazy_node_list(loc, record)
    }
}
