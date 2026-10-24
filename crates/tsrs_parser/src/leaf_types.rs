//! Type leaves construct directly through the parser's exclusive writer.

#![forbid(unsafe_code)]

use crate::lexical::{LexicalParser, LexicalScanner};
use tsrs_ast::{Kind, Node};
use tsrs_core::P;

pub trait LeafTypeParser<'src>: LexicalParser<'src> {
    fn new_keyword_type_node(&mut self, kind: Kind) -> Self::Node;
    fn new_this_type_node(&mut self) -> Self::Node;
    fn new_jsdoc_all_type(&mut self) -> Self::Node;
    fn new_prefix_unary_expression(&mut self, operator: Kind, operand: Self::Node) -> Self::Node;
    fn new_literal_type_node(&mut self, literal: Self::Node) -> Self::Node;

    fn parse_keyword_type_node(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        let result = self.new_keyword_type_node(self.token());
        self.next_token();
        self.finish_node(result, pos)
    }

    fn parse_this_type_node(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        self.next_token();
        let node = self.new_this_type_node();
        self.finish_node(node, pos)
    }

    fn parse_jsdoc_all_type(&mut self) -> Self::Node {
        let pos = self.scanner().token_full_start();
        self.next_token();
        let node = self.new_jsdoc_all_type();
        self.finish_node(node, pos)
    }

    fn parse_literal_type_node(&mut self, negative: bool) -> Self::Node {
        let pos = self.scanner().token_full_start();
        if negative {
            self.next_token();
        }
        let mut expression;
        if self.token() == Kind::TrueKeyword
            || self.token() == Kind::FalseKeyword
            || self.token() == Kind::NullKeyword
        {
            expression = self.parse_keyword_expression();
        } else {
            expression = self.parse_literal_expression();
        }
        if negative {
            let node = self.new_prefix_unary_expression(Kind::MinusToken, expression);
            expression = self.finish_node(node, pos);
        }
        let node = self.new_literal_type_node(expression);
        self.finish_node(node, pos)
    }
}

impl LeafTypeParser<'static> for crate::Parser {
    fn new_keyword_type_node(&mut self, kind: Kind) -> P<Node> {
        self.factory.new_keyword_type_node(kind)
    }
    fn new_this_type_node(&mut self) -> P<Node> {
        self.factory.new_this_type_node()
    }
    fn new_jsdoc_all_type(&mut self) -> P<Node> {
        self.factory.new_jsdoc_all_type()
    }
    fn new_prefix_unary_expression(&mut self, operator: Kind, operand: P<Node>) -> P<Node> {
        self.factory.new_prefix_unary_expression(operator, operand)
    }
    fn new_literal_type_node(&mut self, literal: P<Node>) -> P<Node> {
        self.factory.new_literal_type_node(literal)
    }
}
