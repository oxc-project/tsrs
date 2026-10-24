use tsrs_ast::{self as ast, Kind, ModifierList, Node, NodeList};
use tsrs_core::{P, TextRange};
use tsrs_diagnostics::Message;

use crate::*;

impl Parser {
    pub fn try_parse_import_attributes(&mut self) -> Option<P<Node>> {
        crate::type_grammar::TypeParser::try_parse_import_attributes(self)
    }

    // TYPES

    pub fn parse_jsdoc_type(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_jsdoc_type(self)
    }

    pub fn parse_entity_name(
        &mut self,
        allow_reserved_words: bool,
        allow_private_name: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> P<Node> {
        crate::lexical::LexicalParser::parse_entity_name(
            self,
            allow_reserved_words,
            allow_private_name,
            diagnostic_message,
        )
    }

    pub fn new_identifier(&mut self, text: &'static str) -> P<Node> {
        self.identifier_count += 1;
        let id = self.factory.new_identifier(text);
        if text == "await" {
            self.statement_has_await_identifier = true;
        }
        id
    }

    /// `new_identifier` for the current token's text, which ends at `loc.end()` (the factory keeps only its length
    /// when it is a slice of the source text).
    pub fn new_source_identifier(&mut self, text: &'static str, loc: TextRange) -> P<Node> {
        self.identifier_count += 1;
        let id = self.factory.new_source_identifier(text, self.source_text_index, loc);
        if text == "await" {
            self.statement_has_await_identifier = true;
        }
        id
    }

    pub fn create_missing_identifier(&mut self) -> P<Node> {
        crate::lexical::LexicalParser::create_missing_identifier(self)
    }

    pub fn re_scan_less_than_token(&mut self) -> Kind {
        self.token = self.scanner.re_scan_less_than_token();
        self.report_scan_errors();
        self.token
    }

    pub fn re_scan_greater_than_token(&mut self) -> Kind {
        self.token = self.scanner.re_scan_greater_than_token();
        self.report_scan_errors();
        self.token
    }

    pub fn re_scan_slash_token(&mut self) -> Kind {
        self.token = self.scanner.re_scan_slash_token(false);
        self.report_scan_errors();
        self.token
    }

    pub fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
        self.token = self.scanner.re_scan_template_token(is_tagged_template);
        self.report_scan_errors();
        self.token
    }

    pub fn parse_type_arguments(&mut self) -> Option<P<NodeList>> {
        crate::type_grammar::TypeParser::parse_type_arguments(self)
    }

    pub fn parse_type_member(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_type_member(self)
    }

    pub fn is_start_of_function_type_or_constructor_type(&mut self) -> bool {
        self.token == Kind::LessThanToken
            || self.token == Kind::OpenParenToken
                && self.look_ahead(Parser::next_is_unambiguously_start_of_function_type)
            || self.token == Kind::NewKeyword
            || self.token == Kind::AbstractKeyword
                && self.look_ahead(Parser::next_token_is_new_keyword)
    }

    pub fn next_token_is_new_keyword(&mut self) -> bool {
        crate::recognition::RecognitionParser::next_token_is_new_keyword(self)
    }

    fn next_is_unambiguously_start_of_function_type(&mut self) -> bool {
        self.next_token();
        if self.token == Kind::CloseParenToken || self.token == Kind::DotDotDotToken {
            // ( )
            // ( ...
            return true;
        }
        if self.skip_parameter_start() {
            // We successfully skipped modifiers (if any) and an identifier or binding pattern,
            // now see if we have something that indicates a parameter declaration
            if self.token == Kind::ColonToken
                || self.token == Kind::CommaToken
                || self.token == Kind::QuestionToken
                || self.token == Kind::EqualsToken
            {
                // ( xxx :
                // ( xxx ,
                // ( xxx ?
                // ( xxx =
                return true;
            }
            if self.token == Kind::CloseParenToken
                && self.next_token() == Kind::EqualsGreaterThanToken
            {
                // ( xxx ) =>
                return true;
            }
        }
        false
    }

    fn skip_parameter_start(&mut self) -> bool {
        if ast::is_modifier_kind(self.token) {
            // Skip modifiers
            self.parse_modifiers();
        }
        self.parse_optional(Kind::DotDotDotToken);
        if self.is_identifier() || self.token == Kind::ThisKeyword {
            self.next_token();
            return true;
        }
        if self.token == Kind::OpenBracketToken || self.token == Kind::OpenBraceToken {
            // Return true if we can parse an array or object binding pattern with no errors
            let previous_error_count = self.diagnostics.len();
            self.parse_identifier_or_pattern();
            return previous_error_count == self.diagnostics.len();
        }
        false
    }

    pub fn parse_modifiers(&mut self) -> Option<P<ModifierList>> {
        crate::type_grammar::TypeParser::parse_modifiers(self)
    }

    // Keep speculative modifier parsing and list construction out of the empty-list path.
    #[inline]
    pub fn parse_modifiers_ex(
        &mut self,
        allow_decorators: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<P<ModifierList>> {
        crate::type_grammar::TypeParser::parse_modifiers_ex(
            self,
            allow_decorators,
            permit_const_as_modifier,
            stop_on_start_of_class_static_block,
        )
    }

    pub fn next_token_is_identifier_or_keyword(&mut self) -> bool {
        crate::recognition::RecognitionParser::next_token_is_identifier_or_keyword(self)
    }

    pub fn parse_expression(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_expression(self)
    }
}
