//! Legacy backend operations for the shared lexical grammar.

use std::fmt::Display;

use tsrs_ast::{Kind, Node, NodeFlags, TokenFlags};
use tsrs_core::{P, TextRange};
use tsrs_diagnostics::Message;
use tsrs_scanner::{ArenaScannerText, Scanner};

use crate::Parser;
use crate::lexical::{LexicalParser, ScannedText};

impl LexicalParser<'static> for Parser {
    type TextPolicy = ArenaScannerText;
    type Scanner = Scanner;
    type Node = P<Node>;
    type Speculative<'brand, 'writer, 'parser> = Self;

    fn scanner(&self) -> &Scanner {
        &self.scanner
    }
    fn token(&self) -> Kind {
        self.token
    }
    fn context_flags(&self) -> NodeFlags {
        self.context_flags
    }
    fn statement_has_await_identifier(&self) -> bool {
        self.statement_has_await_identifier
    }
    fn set_statement_has_await_identifier(&mut self, value: bool) {
        self.statement_has_await_identifier = value;
    }
    fn take_current_text(&mut self) -> ScannedText<&'static str> {
        // The legacy factory already recognizes source-backed identifiers.
        ScannedText { value: self.scanner.token_value(), source_range: None }
    }
    fn scan_token(&mut self) -> Kind {
        self.next_token_without_check()
    }
    fn look_ahead<'parser>(
        &'parser mut self,
        callback: impl for<'brand, 'writer> FnOnce(
            &mut Self::Speculative<'brand, 'writer, 'parser>,
        ) -> bool,
    ) -> bool {
        Parser::look_ahead(self, callback)
    }
    fn error_at(&mut self, range: TextRange, message: &'static Message, args: &[&dyn Display]) {
        self.parse_error_at_range(range, message, args);
    }
    fn finish_node(&mut self, node: P<Node>, pos: u32) -> P<Node> {
        Parser::finish_node(self, node, pos)
    }
    fn new_token(&mut self, kind: Kind) -> P<Node> {
        self.factory.new_token(kind)
    }
    fn new_keyword_expression(&mut self, kind: Kind) -> P<Node> {
        self.factory.new_keyword_expression(kind)
    }
    fn new_identifier(&mut self, text: &'static str) -> P<Node> {
        Parser::new_identifier(self, text)
    }
    fn new_source_identifier(
        &mut self,
        text: ScannedText<&'static str>,
        range: TextRange,
    ) -> P<Node> {
        Parser::new_source_identifier(self, text.value, range)
    }
    fn new_private_identifier(&mut self, text: ScannedText<&'static str>) -> P<Node> {
        self.factory.new_private_identifier(text.value)
    }
    fn new_literal(
        &mut self,
        kind: Kind,
        text: ScannedText<&'static str>,
        flags: TokenFlags,
    ) -> P<Node> {
        match kind {
            Kind::StringLiteral => self.factory.new_string_literal(text.value, flags),
            Kind::NumericLiteral => self.factory.new_numeric_literal(text.value, flags),
            Kind::BigIntLiteral => self.factory.new_big_int_literal(text.value, flags),
            Kind::RegularExpressionLiteral => {
                self.factory.new_regular_expression_literal(text.value, flags)
            }
            Kind::NoSubstitutionTemplateLiteral => {
                self.factory.new_no_substitution_template_literal(text.value, flags)
            }
            _ => panic!("Unhandled case in parseLiteralExpression"),
        }
    }
    fn new_qualified_name(&mut self, left: P<Node>, right: P<Node>) -> P<Node> {
        self.factory.new_qualified_name(left, right)
    }
}
