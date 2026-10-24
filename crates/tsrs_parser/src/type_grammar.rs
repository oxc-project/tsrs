//! Recursive type, expression, statement and declaration grammar uses copied handles
//! and explicit writers. Constructing lookahead and `JSDoc` still need owned
//! implementations before the owned parser can use this complete recursive grammar.

#![forbid(unsafe_code)]

use crate::context::ContextParser;
use crate::leaf_types::LeafTypeParser;
use crate::lexical::{LexicalParser, LexicalScanner};
use crate::lists::ListParser;
use crate::parser_1::{JsdocScannerInfo, ParsingContext};
use crate::recognition::RecognitionParser;
use crate::template_tokens::TemplateTokenParser;
use crate::tokens::TokenParser;
use crate::types::ParseFlags;
use crate::utilities::{is_keyword_or_punctuation, token_is_identifier_or_keyword};
use tsrs_ast::{
    self as ast, Kind, ModifierFlags, ModifierList, Node, NodeFlags, NodeList, OperatorPrecedence,
    TokenFlags,
};
use tsrs_core::{LanguageVariant, P, TextRange, Tristate};
use tsrs_diagnostics::{self as diagnostics, Message};
use tsrs_scanner as scanner;

pub trait TypeParser<'src>:
    ContextParser<'src> + ListParser<'src> + LeafTypeParser<'src> + TemplateTokenParser<'src>
{
    type Modifiers: Copy;
    fn modifier_flags(&self, modifiers: Self::Modifiers) -> ModifierFlags;
    fn modifier_list_len(&self, modifiers: Self::Modifiers) -> usize;
    fn modifier_list_element(&self, modifiers: Self::Modifiers, index: usize) -> Self::Node;
    fn node_text(&self, node: Self::Node) -> &str;
    fn node_name(&self, node: Self::Node) -> Option<Self::Node>;
    fn tagged_template(&self, node: Self::Node) -> Self::Node;
    fn heritage_clause_parts(&self, node: Self::Node) -> (Kind, Self::List);
    fn current_token_value_is(&self, text: &str) -> bool;
    // Diagnostics can read cooked text without consuming the current scanner value.
    fn error_at_current_token_value(&mut self, message: &'static Message);
    fn new_block(&mut self, statements: Self::List, multi_line: bool) -> Self::Node;
    fn new_empty_statement(&mut self) -> Self::Node;
    fn new_if_statement(
        &mut self,
        expression: Self::Node,
        then_statement: Self::Node,
        else_statement: Option<Self::Node>,
    ) -> Self::Node;
    fn new_do_statement(&mut self, statement: Self::Node, expression: Self::Node) -> Self::Node;
    fn new_while_statement(&mut self, expression: Self::Node, statement: Self::Node) -> Self::Node;
    fn new_for_in_or_of_statement(
        &mut self,
        kind: Kind,
        await_modifier: Option<Self::Node>,
        initializer: Self::Node,
        expression: Self::Node,
        statement: Self::Node,
    ) -> Self::Node;
    fn new_for_statement(
        &mut self,
        initializer: Option<Self::Node>,
        condition: Option<Self::Node>,
        incrementor: Option<Self::Node>,
        statement: Self::Node,
    ) -> Self::Node;
    fn new_break_statement(&mut self, label: Option<Self::Node>) -> Self::Node;
    fn new_continue_statement(&mut self, label: Option<Self::Node>) -> Self::Node;
    fn new_return_statement(&mut self, expression: Option<Self::Node>) -> Self::Node;
    fn new_with_statement(&mut self, expression: Self::Node, statement: Self::Node) -> Self::Node;
    fn new_case_or_default_clause(
        &mut self,
        kind: Kind,
        expression: Option<Self::Node>,
        statements: Self::List,
    ) -> Self::Node;
    fn new_case_block(&mut self, clauses: Self::List) -> Self::Node;
    fn new_switch_statement(
        &mut self,
        expression: Self::Node,
        case_block: Self::Node,
    ) -> Self::Node;
    fn new_throw_statement(&mut self, expression: Self::Node) -> Self::Node;
    fn new_try_statement(
        &mut self,
        try_block: Self::Node,
        catch_clause: Option<Self::Node>,
        finally_block: Option<Self::Node>,
    ) -> Self::Node;
    fn new_catch_clause(
        &mut self,
        variable_declaration: Option<Self::Node>,
        block: Self::Node,
    ) -> Self::Node;
    fn new_debugger_statement(&mut self) -> Self::Node;
    fn new_labeled_statement(&mut self, label: Self::Node, statement: Self::Node) -> Self::Node;
    fn new_expression_statement(&mut self, expression: Self::Node) -> Self::Node;
    fn new_variable_statement(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        declaration_list: Self::Node,
    ) -> Self::Node;
    fn new_variable_declaration_list(
        &mut self,
        declarations: Self::List,
        flags: NodeFlags,
    ) -> Self::Node;
    fn new_variable_declaration(
        &mut self,
        name: Self::Node,
        exclamation_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_binding_pattern(&mut self, kind: Kind, elements: Self::List) -> Self::Node;
    fn new_binding_element(
        &mut self,
        dot_dot_dot_token: Option<Self::Node>,
        property_name: Option<Self::Node>,
        name: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_function_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        asterisk_token: Option<Self::Node>,
        name: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_class_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        heritage_clauses: Option<Self::List>,
        members: Self::List,
    ) -> Self::Node;
    fn new_class_expression(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        heritage_clauses: Option<Self::List>,
        members: Self::List,
    ) -> Self::Node;
    fn new_heritage_clause(&mut self, token: Kind, types: Self::List) -> Self::Node;
    fn new_semicolon_class_element(&mut self) -> Self::Node;
    fn new_class_static_block_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        body: Self::Node,
    ) -> Self::Node;
    fn new_constructor_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_property_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        postfix_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_interface_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        type_parameters: Option<Self::List>,
        heritage_clauses: Option<Self::List>,
        members: Self::List,
    ) -> Self::Node;
    fn new_type_alias_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        type_parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_enum_member(&mut self, name: Self::Node, initializer: Option<Self::Node>) -> Self::Node;
    fn new_enum_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        members: Self::List,
    ) -> Self::Node;
    fn new_module_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        keyword: Kind,
        name: Self::Node,
        attributes: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_module_block(&mut self, statements: Self::List) -> Self::Node;
    fn new_import_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        import_clause: Option<Self::Node>,
        module_specifier: Self::Node,
        attributes: Option<Self::Node>,
    ) -> Self::Node;
    fn new_import_equals_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        is_type_only: bool,
        name: Self::Node,
        module_reference: Self::Node,
    ) -> Self::Node;
    fn new_external_module_reference(&mut self, expression: Self::Node) -> Self::Node;
    fn new_import_clause(
        &mut self,
        phase_modifier: Kind,
        name: Option<Self::Node>,
        named_bindings: Option<Self::Node>,
    ) -> Self::Node;
    fn new_namespace_import(&mut self, name: Self::Node) -> Self::Node;
    fn new_named_imports(&mut self, elements: Self::List) -> Self::Node;
    fn new_import_specifier(
        &mut self,
        is_type_only: bool,
        property_name: Option<Self::Node>,
        name: Self::Node,
    ) -> Self::Node;
    fn new_export_assignment(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        is_export_equals: bool,
        type_node: Option<Self::Node>,
        expression: Self::Node,
    ) -> Self::Node;
    fn new_namespace_export_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
    ) -> Self::Node;
    fn new_export_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        is_type_only: bool,
        export_clause: Option<Self::Node>,
        module_specifier: Option<Self::Node>,
        attributes: Option<Self::Node>,
    ) -> Self::Node;
    fn new_namespace_export(&mut self, name: Self::Node) -> Self::Node;
    fn new_named_exports(&mut self, elements: Self::List) -> Self::Node;
    fn new_export_specifier(
        &mut self,
        is_type_only: bool,
        property_name: Option<Self::Node>,
        name: Self::Node,
    ) -> Self::Node;
    fn finish_node_with_end(&mut self, node: Self::Node, pos: u32, end: u32) -> Self::Node;
    fn current_import_phase_modifier(&self) -> Kind;
    fn modifier_list_has_async(&self, modifiers: Option<Self::Modifiers>) -> bool;
    fn not_parenthesized_arrow_contains(&self, pos: u32) -> bool;
    fn remember_not_parenthesized_arrow(&mut self, pos: u32);
    fn node_expression(&self, node: Self::Node) -> Option<Self::Node>;
    fn node_type_arguments(&self, node: Self::Node) -> Option<Self::List>;
    fn expression_with_type_arguments_parts(
        &self,
        node: Self::Node,
    ) -> (Self::Node, Option<Self::List>);
    fn template_span_literal(&self, node: Self::Node) -> Self::Node;
    fn list_len(&self, list: Self::List) -> usize;
    fn list_element(&self, list: Self::List, index: usize) -> Self::Node;
    fn list_range(&self, list: Self::List) -> TextRange;
    fn node_is_import_phase_meta_property(&self, node: Self::Node) -> bool;
    fn expression_source_text(&self, node: Self::Node) -> String;
    fn skip_expression_range_trivia(&self, range: TextRange) -> TextRange;
    fn re_scan_slash_token(&mut self) -> Kind;
    fn scan_jsx_text(&mut self) -> Kind;
    fn scan_jsx_identifier(&mut self) -> Kind;
    fn scan_jsx_attribute_value(&mut self) -> Kind;
    fn re_scan_jsx_token(&mut self, allow_multiline: bool) -> Kind;
    fn jsx_element_parts(&self, node: Self::Node) -> (Self::Node, Self::List, Self::Node);
    fn jsx_tag_name(&self, node: Self::Node) -> Self::Node;
    fn jsx_tag_names_are_equivalent(&self, left: Self::Node, right: Self::Node) -> bool;
    fn new_scanned_jsx_text(
        &mut self,
        text: crate::lexical::ScannedText<
            <Self::TextPolicy as tsrs_scanner::ScannerText<'src>>::Text,
        >,
        whitespace: bool,
    ) -> Self::Node;
    fn type_reference_name(&self, node: Self::Node) -> Self::Node;
    fn function_parameters(&self, node: Self::Node) -> Option<Self::List>;
    fn list_is_missing(&self, list: Option<Self::List>) -> bool;
    fn node_is_missing(&self, node: Self::Node) -> bool;
    fn new_arrow_function(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        equals_greater_than_token: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_jsx_closing_element(&mut self, tag_name: Self::Node) -> Self::Node;
    fn new_jsx_element(
        &mut self,
        opening_element: Self::Node,
        children: Self::List,
        closing_element: Self::Node,
    ) -> Self::Node;
    fn new_jsx_fragment(
        &mut self,
        opening_fragment: Self::Node,
        children: Self::List,
        closing_fragment: Self::Node,
    ) -> Self::Node;
    fn new_jsx_expression(
        &mut self,
        dot_dot_dot_token: Option<Self::Node>,
        expression: Option<Self::Node>,
    ) -> Self::Node;
    fn new_jsx_opening_fragment(&mut self) -> Self::Node;
    fn new_jsx_opening_element(
        &mut self,
        tag_name: Self::Node,
        type_arguments: Option<Self::List>,
        attributes: Self::Node,
    ) -> Self::Node;
    fn new_jsx_self_closing_element(
        &mut self,
        tag_name: Self::Node,
        type_arguments: Option<Self::List>,
        attributes: Self::Node,
    ) -> Self::Node;
    fn new_property_access_expression(
        &mut self,
        expression: Self::Node,
        question_dot_token: Option<Self::Node>,
        name: Self::Node,
        flags: NodeFlags,
    ) -> Self::Node;
    fn new_jsx_namespaced_name(&mut self, namespace: Self::Node, name: Self::Node) -> Self::Node;
    fn new_jsx_attributes(&mut self, properties: Self::List) -> Self::Node;
    fn new_jsx_attribute(
        &mut self,
        name: Self::Node,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_jsx_spread_attribute(&mut self, expression: Self::Node) -> Self::Node;
    fn new_jsx_closing_fragment(&mut self) -> Self::Node;
    fn new_meta_property(&mut self, keyword_token: Kind, name: Self::Node) -> Self::Node;
    fn new_expression_with_type_arguments(
        &mut self,
        expression: Self::Node,
        type_arguments: Option<Self::List>,
    ) -> Self::Node;
    fn new_non_null_expression(&mut self, expression: Self::Node, flags: NodeFlags) -> Self::Node;
    fn new_element_access_expression(
        &mut self,
        expression: Self::Node,
        question_dot_token: Option<Self::Node>,
        argument_expression: Self::Node,
        flags: NodeFlags,
    ) -> Self::Node;
    fn new_call_expression(
        &mut self,
        expression: Self::Node,
        question_dot_token: Option<Self::Node>,
        type_arguments: Option<Self::List>,
        arguments: Self::List,
        flags: NodeFlags,
    ) -> Self::Node;
    fn new_omitted_expression(&mut self) -> Self::Node;
    fn new_spread_element(&mut self, expression: Self::Node) -> Self::Node;
    fn new_tagged_template_expression(
        &mut self,
        tag: Self::Node,
        question_dot_token: Option<Self::Node>,
        type_arguments: Option<Self::List>,
        template: Self::Node,
        flags: NodeFlags,
    ) -> Self::Node;
    fn new_template_expression(
        &mut self,
        head: Self::Node,
        template_spans: Self::List,
    ) -> Self::Node;
    fn new_template_span(&mut self, expression: Self::Node, literal: Self::Node) -> Self::Node;
    fn new_parenthesized_expression(&mut self, expression: Self::Node) -> Self::Node;
    fn new_array_literal_expression(
        &mut self,
        elements: Self::List,
        multi_line: bool,
    ) -> Self::Node;
    fn new_object_literal_expression(
        &mut self,
        properties: Self::List,
        multi_line: bool,
    ) -> Self::Node;
    fn new_spread_assignment(&mut self, expression: Self::Node) -> Self::Node;
    fn new_shorthand_property_assignment(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        postfix_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        equals_token: Option<Self::Node>,
        object_assignment_initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_property_assignment(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        postfix_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        initializer: Self::Node,
    ) -> Self::Node;
    fn new_function_expression(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        asterisk_token: Option<Self::Node>,
        name: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_missing_declaration(&mut self, modifiers: Option<Self::Modifiers>) -> Self::Node;
    fn new_new_expression(
        &mut self,
        expression: Self::Node,
        type_arguments: Option<Self::List>,
        arguments: Option<Self::List>,
    ) -> Self::Node;
    fn new_method_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        asterisk_token: Option<Self::Node>,
        name: Self::Node,
        postfix_token: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn add_source_flags(&mut self, flags: NodeFlags);
    fn last_expected_diagnostic(&self) -> Option<Self::Diagnostic>;
    fn language_variant(&self) -> LanguageVariant;
    fn node_is_left_hand_side_expression(&self, node: Self::Node) -> bool;
    fn node_is_present(&self, node: Self::Node) -> bool;
    fn binary_operator_kind(&self, node: Self::Node) -> Kind;
    fn re_scan_greater_than_token(&mut self) -> Kind;
    fn skip_expression_trivia(&self, pos: u32) -> u32;
    fn new_import_type_node(
        &mut self,
        is_type_of: bool,
        type_node: Self::Node,
        attributes: Option<Self::Node>,
        qualifier: Option<Self::Node>,
        type_arguments: Option<Self::List>,
    ) -> Self::Node;
    fn new_import_attribute(&mut self, name: Option<Self::Node>, value: Self::Node) -> Self::Node;
    fn new_import_attributes(
        &mut self,
        token: Kind,
        elements: Self::List,
        multi_line: bool,
    ) -> Self::Node;
    fn new_yield_expression(
        &mut self,
        asterisk_token: Option<Self::Node>,
        expression: Option<Self::Node>,
    ) -> Self::Node;
    fn new_conditional_expression(
        &mut self,
        condition: Self::Node,
        question_token: Self::Node,
        when_true: Self::Node,
        colon_token: Self::Node,
        when_false: Self::Node,
    ) -> Self::Node;
    fn new_satisfies_expression(
        &mut self,
        expression: Self::Node,
        type_node: Self::Node,
    ) -> Self::Node;
    fn new_as_expression(&mut self, expression: Self::Node, type_node: Self::Node) -> Self::Node;
    fn new_binary_expression(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        left: Self::Node,
        type_node: Option<Self::Node>,
        operator_token: Self::Node,
        right: Self::Node,
    ) -> Self::Node;
    fn new_postfix_unary_expression(&mut self, operand: Self::Node, operator: Kind) -> Self::Node;
    fn new_delete_expression(&mut self, expression: Self::Node) -> Self::Node;
    fn new_type_of_expression(&mut self, expression: Self::Node) -> Self::Node;
    fn new_void_expression(&mut self, expression: Self::Node) -> Self::Node;
    fn new_await_expression(&mut self, expression: Self::Node) -> Self::Node;
    fn new_type_assertion(&mut self, type_node: Self::Node, expression: Self::Node) -> Self::Node;
    fn modifier_first_range(&self, modifiers: Self::Modifiers) -> TextRange;
    fn check_js_syntax(&mut self, node: Self::Node) -> Self::Node;
    fn create_identifier(&mut self, is_identifier: bool) -> Self::Node;
    fn finish_modifier_list(&mut self, start: usize, range: TextRange) -> Self::Modifiers;
    fn new_call_signature_declaration(
        &mut self,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_construct_signature_declaration(
        &mut self,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_parameter_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        dot_dot_dot_token: Option<Self::Node>,
        name: Self::Node,
        question_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_get_accessor_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_set_accessor_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
        full_signature: Option<Self::Node>,
        body: Option<Self::Node>,
    ) -> Self::Node;
    fn new_computed_property_name(&mut self, expression: Self::Node) -> Self::Node;
    fn new_index_signature_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_method_signature_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        question_token: Option<Self::Node>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_property_signature_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        question_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) -> Self::Node;
    fn new_decorator(&mut self, expression: Self::Node) -> Self::Node;
    fn re_scan_asterisk_equals_token(&mut self);
    fn re_scan_question_token(&mut self);
    fn set_skip_jsdoc_leading_asterisks(&mut self, value: bool);
    fn parse_member_list(
        &mut self,
        kind: ParsingContext,
        parse_element: fn(&mut Self) -> Self::Node,
    ) -> Self::List;
    fn set_lazy_owner(&mut self, members: Self::List, owner: Self::Node);
    fn jsdoc_scanner_info(&self) -> JsdocScannerInfo;
    fn with_jsdoc(&mut self, node: Self::Node, info: JsdocScannerInfo);
    fn node_type(&self, node: Self::Node) -> Option<Self::Node>;
    fn node_flags(&self, node: Self::Node) -> NodeFlags;
    fn set_node_flags(&mut self, node: Self::Node, flags: NodeFlags);
    fn set_node_range(&mut self, node: Self::Node, range: TextRange);
    fn set_node_parent(&mut self, node: Self::Node, parent: Option<Self::Node>);
    fn template_type_span_literal(&self, span: Self::Node) -> Self::Node;
    fn new_modifier_list(&mut self, range: TextRange, nodes: &[Self::Node]) -> Self::Modifiers;
    fn new_mapped_type_node(
        &mut self,
        readonly_token: Option<Self::Node>,
        type_parameter: Self::Node,
        name_type: Option<Self::Node>,
        question_token: Option<Self::Node>,
        type_node: Option<Self::Node>,
        members: Option<Self::List>,
    ) -> Self::Node;
    fn new_type_literal_node(&mut self, members: Self::List) -> Self::Node;
    fn new_tuple_type_node(&mut self, elements: Self::List) -> Self::Node;
    fn new_named_tuple_member(
        &mut self,
        dot_dot_dot_token: Option<Self::Node>,
        name: Self::Node,
        question_token: Option<Self::Node>,
        type_node: Self::Node,
    ) -> Self::Node;
    fn new_rest_type_node(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_optional_type_node(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_template_literal_type_node(&mut self, head: Self::Node, spans: Self::List)
    -> Self::Node;
    fn new_template_literal_type_span(
        &mut self,
        type_node: Self::Node,
        literal: Self::Node,
    ) -> Self::Node;
    fn new_jsdoc_variadic_type(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_jsdoc_optional_type(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_function_type_node(
        &mut self,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_constructor_type_node(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        type_parameters: Option<Self::List>,
        parameters: Option<Self::List>,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_modifier(&mut self, kind: Kind) -> Self::Node;
    fn node_kind(&self, node: Self::Node) -> Kind;
    fn node_range(&self, node: Self::Node) -> TextRange;
    fn is_start_of_function_type_or_constructor_type(&mut self) -> bool;
    fn re_scan_less_than_token(&mut self) -> Kind;
    fn new_conditional_type_node(
        &mut self,
        check_type: Self::Node,
        extends_type: Self::Node,
        true_type: Self::Node,
        false_type: Self::Node,
    ) -> Self::Node;
    fn new_union_type_node(&mut self, types: Self::List) -> Self::Node;
    fn new_intersection_type_node(&mut self, types: Self::List) -> Self::Node;
    fn new_type_operator_node(&mut self, operator: Kind, type_node: Self::Node) -> Self::Node;
    fn new_infer_type_node(&mut self, type_parameter: Self::Node) -> Self::Node;
    fn new_type_parameter_declaration(
        &mut self,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        constraint: Option<Self::Node>,
        expression: Option<Self::Node>,
        default_type: Option<Self::Node>,
    ) -> Self::Node;
    fn new_jsdoc_non_nullable_type(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_jsdoc_nullable_type(&mut self, type_node: Self::Node) -> Self::Node;
    fn new_indexed_access_type_node(
        &mut self,
        object_type: Self::Node,
        index_type: Self::Node,
    ) -> Self::Node;
    fn new_array_type_node(&mut self, element_type: Self::Node) -> Self::Node;
    fn new_type_reference_node(
        &mut self,
        type_name: Self::Node,
        type_arguments: Option<Self::List>,
    ) -> Self::Node;
    fn new_type_predicate_node(
        &mut self,
        asserts_modifier: Option<Self::Node>,
        parameter_name: Self::Node,
        type_node: Option<Self::Node>,
    ) -> Self::Node;
    fn new_type_query_node(
        &mut self,
        entity_name: Self::Node,
        type_arguments: Option<Self::List>,
    ) -> Self::Node;
    fn new_parenthesized_type_node(&mut self, type_node: Self::Node) -> Self::Node;

    fn parse_type(&mut self) -> Self::Node {
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::TypeExcludesFlags, false);
        let mut type_node;
        if self.is_start_of_function_type_or_constructor_type() {
            type_node = self.parse_function_or_constructor_type();
        } else {
            let pos = self.node_pos();
            type_node = self.parse_union_type_or_higher();
            if !self.in_disallow_conditional_types_context()
                && !self.has_preceding_line_break()
                && self.parse_optional(Kind::ExtendsKeyword)
            {
                // The type following 'extends' is not permitted to be another conditional type
                let extends_type = self.do_in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    true,
                    Self::parse_type,
                );
                self.parse_expected(Kind::QuestionToken);
                let true_type = self.do_in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    false,
                    Self::parse_type,
                );
                self.parse_expected(Kind::ColonToken);
                let false_type = self.do_in_context(
                    NodeFlags::DisallowConditionalTypesContext,
                    false,
                    Self::parse_type,
                );
                let conditional_type =
                    self.new_conditional_type_node(type_node, extends_type, true_type, false_type);
                self.finish_node(conditional_type, pos);
                type_node = conditional_type;
            }
        }
        self.replace_context_flags(save_context_flags);
        type_node
    }

    fn parse_union_type_or_higher(&mut self) -> Self::Node {
        self.parse_union_or_intersection_type(
            Kind::BarToken,
            Self::parse_intersection_type_or_higher,
        )
    }

    fn parse_intersection_type_or_higher(&mut self) -> Self::Node {
        self.parse_union_or_intersection_type(
            Kind::AmpersandToken,
            Self::parse_type_operator_or_higher,
        )
    }

    // Specialize the two constituent parsers so type parsing does not dispatch through a function pointer.
    fn parse_union_or_intersection_type(
        &mut self,
        operator: Kind,
        parse_constituent_type: impl Fn(&mut Self) -> Self::Node + Copy,
    ) -> Self::Node {
        let pos = self.node_pos();
        let is_union_type = operator == Kind::BarToken;
        let has_leading_operator = self.parse_optional(operator);
        let mut type_node;
        if has_leading_operator {
            type_node = self
                .parse_function_or_constructor_type_to_error(is_union_type, parse_constituent_type);
        } else {
            type_node = parse_constituent_type(self);
        }
        if self.token() == operator || has_leading_operator {
            let start = self.node_stack().len();
            self.node_stack_mut().push(type_node);
            while self.parse_optional(operator) {
                let t = self.parse_function_or_constructor_type_to_error(
                    is_union_type,
                    parse_constituent_type,
                );
                self.node_stack_mut().push(t);
            }
            let end = self.node_pos();
            let list = self.finish_node_list(start, TextRange::new(pos, end));
            type_node = self.create_union_or_intersection_type_node(operator, list);
            self.finish_node(type_node, pos);
        }
        type_node
    }

    fn create_union_or_intersection_type_node(
        &mut self,
        operator: Kind,
        types: Self::List,
    ) -> Self::Node {
        match operator {
            Kind::BarToken => self.new_union_type_node(types),
            Kind::AmpersandToken => self.new_intersection_type_node(types),
            _ => panic!("Unhandled case in createUnionOrIntersectionType"),
        }
    }

    fn parse_type_operator_or_higher(&mut self) -> Self::Node {
        let operator = self.token();
        match operator {
            Kind::KeyOfKeyword | Kind::UniqueKeyword | Kind::ReadonlyKeyword => {
                return self.parse_type_operator(operator);
            }
            Kind::InferKeyword => {
                return self.parse_infer_type();
            }
            _ => {}
        }
        self.do_in_context(
            NodeFlags::DisallowConditionalTypesContext,
            false,
            Self::parse_postfix_type_or_higher,
        )
    }

    fn parse_type_operator(&mut self, operator: Kind) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(operator);
        let type_ = self.parse_type_operator_or_higher();
        let node = self.new_type_operator_node(operator, type_);
        self.finish_node(node, pos)
    }

    fn parse_infer_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::InferKeyword);
        let type_parameter = self.parse_type_parameter_of_infer_type();
        let node = self.new_infer_type_node(type_parameter);
        self.finish_node(node, pos)
    }

    fn parse_type_parameter_of_infer_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let name = self.parse_identifier();
        let constraint = self.try_parse_constraint_of_infer_type();
        let node = self.new_type_parameter_declaration(
            None, /*modifiers*/
            name, constraint, None, /*expression*/
            None, /*defaultType*/
        );
        self.finish_node(node, pos)
    }

    fn try_parse_constraint_of_infer_type(&mut self) -> Option<Self::Node> {
        let state = self.mark();
        if self.parse_optional(Kind::ExtendsKeyword) {
            let constraint = self.do_in_context(
                NodeFlags::DisallowConditionalTypesContext,
                true,
                Self::parse_type,
            );
            if self.in_disallow_conditional_types_context() || self.token() != Kind::QuestionToken {
                return Some(constraint);
            }
        }
        self.rewind(state);
        None
    }

    fn parse_postfix_type_or_higher(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let mut type_node = self.parse_non_array_type();
        while !self.has_preceding_line_break() {
            match self.token() {
                Kind::ExclamationToken => {
                    self.next_token();
                    let node = self.new_jsdoc_non_nullable_type(type_node);
                    type_node = self.finish_node(node, pos);
                }
                Kind::QuestionToken => {
                    // If next token is start of a type we have a conditional type
                    if self.look_ahead(|parser| parser.next_is_start_of_type()) {
                        return type_node;
                    }
                    self.next_token();
                    let node = self.new_jsdoc_nullable_type(type_node);
                    type_node = self.finish_node(node, pos);
                }
                Kind::OpenBracketToken => {
                    self.parse_expected(Kind::OpenBracketToken);
                    if self.is_start_of_type(false /*isStartOfParameter*/) {
                        let index_type = self.parse_type();
                        self.parse_expected(Kind::CloseBracketToken);
                        let node = self.new_indexed_access_type_node(type_node, index_type);
                        type_node = self.finish_node(node, pos);
                    } else {
                        self.parse_expected(Kind::CloseBracketToken);
                        let node = self.new_array_type_node(type_node);
                        type_node = self.finish_node(node, pos);
                    }
                }
                _ => return type_node,
            }
        }
        type_node
    }

    fn parse_this_type_predicate(&mut self, lhs: Self::Node) -> Self::Node {
        self.next_token();
        let type_ = self.parse_type();
        let node = self.new_type_predicate_node(None /*assertsModifier*/, lhs, Some(type_));
        self.finish_node(
            node,
            self.node_range(lhs).pos().expect_source("parsed nodes have source positions"),
        )
    }

    fn parse_jsdoc_non_nullable_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.next_token();
        let type_ = self.parse_type_operator_or_higher();
        let node = self.new_jsdoc_non_nullable_type(type_);
        self.finish_node(node, pos)
    }

    fn parse_jsdoc_nullable_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        // skip the ?
        self.next_token();
        let type_ = self.parse_type_operator_or_higher();
        let node = self.new_jsdoc_nullable_type(type_);
        self.finish_node(node, pos)
    }

    fn parse_type_reference(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let type_name = self.parse_entity_name_of_type_reference();
        let type_arguments = self.parse_type_arguments_of_type_reference();
        let node = self.new_type_reference_node(type_name, type_arguments);
        self.finish_node(node, pos)
    }

    fn parse_entity_name_of_type_reference(&mut self) -> Self::Node {
        self.parse_entity_name(
            true,  /*allowReservedWords*/
            false, /*allowPrivateName*/
            Some(&diagnostics::TYPE_EXPECTED),
        )
    }

    fn parse_type_arguments_of_type_reference(&mut self) -> Option<Self::List> {
        if !self.has_preceding_line_break() && self.re_scan_less_than_token() == Kind::LessThanToken
        {
            return self.parse_type_arguments();
        }
        None
    }

    fn parse_type_arguments(&mut self) -> Option<Self::List> {
        if self.token() == Kind::LessThanToken {
            return self.parse_bracketed_list(
                ParsingContext::TypeArguments,
                Self::parse_type,
                Kind::LessThanToken,
                Kind::GreaterThanToken,
            );
        }
        None
    }

    fn parse_type_query(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::TypeOfKeyword);
        let entity_name = self.parse_entity_name(
            true, /*allowReservedWords*/
            true, /*allowPrivateName*/
            None,
        );
        // Make sure we perform ASI to prevent parsing the next line's type arguments as part of an instantiation expression
        let mut type_arguments: Option<Self::List> = None;
        if !self.has_preceding_line_break() {
            type_arguments = self.parse_type_arguments();
        }
        let node = self.new_type_query_node(entity_name, type_arguments);
        self.finish_node(node, pos)
    }

    fn parse_type_or_type_predicate(&mut self) -> Self::Node {
        if self.is_identifier() {
            let state = self.mark();
            let pos = self.node_pos();
            let id = self.parse_identifier();
            if self.token() == Kind::IsKeyword && !self.has_preceding_line_break() {
                self.next_token();
                let type_ = self.parse_type();
                let node =
                    self.new_type_predicate_node(None /*assertsModifier*/, id, Some(type_));
                return self.finish_node(node, pos);
            }
            self.rewind(state);
        }
        self.parse_type()
    }

    fn parse_parenthesized_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenParenToken);
        let type_node = self.parse_type();
        self.parse_expected(Kind::CloseParenToken);
        let node = self.new_parenthesized_type_node(type_node);
        self.finish_node(node, pos)
    }

    fn parse_asserts_type_predicate(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let asserts_modifier = self.parse_expected_token(Kind::AssertsKeyword);

        let parameter_name = if self.token() == Kind::ThisKeyword {
            self.parse_this_type_node()
        } else {
            self.parse_identifier()
        };
        let mut type_node: Option<Self::Node> = None;
        if self.parse_optional(Kind::IsKeyword) {
            type_node = Some(self.parse_type());
        }
        let node = self.new_type_predicate_node(Some(asserts_modifier), parameter_name, type_node);
        self.finish_node(node, pos)
    }

    fn parse_function_or_constructor_type_to_error(
        &mut self,
        is_in_union_type: bool,
        parse_constituent_type: impl Fn(&mut Self) -> Self::Node + Copy,
    ) -> Self::Node {
        // the function type and constructor type shorthand notation
        // are not allowed directly in unions and intersections, but we'll
        // try to parse them gracefully and issue a helpful message.
        if self.is_start_of_function_type_or_constructor_type() {
            let type_node = self.parse_function_or_constructor_type();

            let diagnostic: &'static Message = if self.node_kind(type_node) == Kind::FunctionType {
                if is_in_union_type {
                    &diagnostics::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE
                } else {
                    &diagnostics::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYPE
                }
            } else {
                if is_in_union_type {
                    &diagnostics::CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE
                } else {
                    &diagnostics::CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYPE
                }
            };
            self.error_at(self.node_range(type_node), diagnostic, &[]);
            return type_node;
        }
        parse_constituent_type(self)
    }
    fn parse_non_array_type(&mut self) -> Self::Node {
        match self.token() {
            Kind::AnyKeyword
            | Kind::UnknownKeyword
            | Kind::StringKeyword
            | Kind::NumberKeyword
            | Kind::BigIntKeyword
            | Kind::SymbolKeyword
            | Kind::BooleanKeyword
            | Kind::UndefinedKeyword
            | Kind::NeverKeyword
            | Kind::ObjectKeyword => {
                let state = self.mark();
                let keyword_type_node = self.parse_keyword_type_node();
                // If these are followed by a dot then parse these out as a dotted type reference instead
                if self.token() != Kind::DotToken {
                    return keyword_type_node;
                }
                self.rewind(state);
                self.parse_type_reference()
            }
            Kind::AsteriskEqualsToken => {
                // If there is '*=', treat it as * followed by postfix =
                self.re_scan_asterisk_equals_token();
                self.parse_jsdoc_all_type()
            }
            Kind::AsteriskToken => self.parse_jsdoc_all_type(),
            Kind::QuestionQuestionToken => {
                // If there is '??', treat it as prefix-'?' in JSDoc type.
                self.re_scan_question_token();
                self.parse_jsdoc_nullable_type()
            }
            Kind::QuestionToken => self.parse_jsdoc_nullable_type(),
            Kind::ExclamationToken => self.parse_jsdoc_non_nullable_type(),
            Kind::NoSubstitutionTemplateLiteral
            | Kind::StringLiteral
            | Kind::NumericLiteral
            | Kind::BigIntLiteral
            | Kind::TrueKeyword
            | Kind::FalseKeyword
            | Kind::NullKeyword => self.parse_literal_type_node(false /*negative*/),
            Kind::MinusToken => {
                if self.look_ahead(|parser| parser.next_token_is_numeric_or_big_int_literal()) {
                    return self.parse_literal_type_node(true /*negative*/);
                }
                self.parse_type_reference()
            }
            Kind::VoidKeyword => self.parse_keyword_type_node(),
            Kind::ThisKeyword => {
                let this_keyword = self.parse_this_type_node();
                if self.token() == Kind::IsKeyword && !self.has_preceding_line_break() {
                    return self.parse_this_type_predicate(this_keyword);
                }
                this_keyword
            }
            Kind::TypeOfKeyword => {
                if self.look_ahead(|parser| parser.next_is_start_of_type_of_import_type()) {
                    return self.parse_import_type();
                }
                self.parse_type_query()
            }
            Kind::OpenBraceToken => {
                if self.look_ahead(|parser| parser.next_is_start_of_mapped_type()) {
                    return self.parse_mapped_type();
                }
                self.parse_type_literal()
            }
            Kind::OpenBracketToken => self.parse_tuple_type(),
            Kind::OpenParenToken => self.parse_parenthesized_type(),
            Kind::ImportKeyword => self.parse_import_type(),
            Kind::AssertsKeyword => {
                if self
                    .look_ahead(|parser| parser.next_token_is_identifier_or_keyword_on_same_line())
                {
                    return self.parse_asserts_type_predicate();
                }
                self.parse_type_reference()
            }
            Kind::TemplateHead => self.parse_template_type(),
            _ => self.parse_type_reference(),
        }
    }

    fn parse_jsdoc_type(&mut self) -> Self::Node {
        self.set_skip_jsdoc_leading_asterisks(true);
        let pos = self.node_pos();

        let has_dot_dot_dot = self.parse_optional(Kind::DotDotDotToken);
        let mut t = self.parse_type_or_type_predicate();
        self.set_skip_jsdoc_leading_asterisks(false);
        if has_dot_dot_dot {
            let node = self.new_jsdoc_variadic_type(t);
            t = self.finish_node(node, pos);
        }
        if self.token() == Kind::EqualsToken {
            self.next_token();
            let node = self.new_jsdoc_optional_type(t);
            return self.finish_node(node, pos);
        }
        t
    }

    fn parse_mapped_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenBraceToken);
        let mut readonly_token: Option<Self::Node> = None; // ReadonlyKeyword | PlusToken | MinusToken
        if self.token() == Kind::ReadonlyKeyword
            || self.token() == Kind::PlusToken
            || self.token() == Kind::MinusToken
        {
            let token = self.parse_token_node();
            readonly_token = Some(token);
            if self.node_kind(token) != Kind::ReadonlyKeyword {
                self.parse_expected(Kind::ReadonlyKeyword);
            }
        }
        self.parse_expected(Kind::OpenBracketToken);
        let type_parameter = self.parse_mapped_type_parameter();
        let mut name_type: Option<Self::Node> = None;
        if self.parse_optional(Kind::AsKeyword) {
            name_type = Some(self.parse_type());
        }
        self.parse_expected(Kind::CloseBracketToken);
        let mut question_token: Option<Self::Node> = None; // QuestionToken | PlusToken | MinusToken
        if self.token() == Kind::QuestionToken
            || self.token() == Kind::PlusToken
            || self.token() == Kind::MinusToken
        {
            let token = self.parse_token_node();
            question_token = Some(token);
            if self.node_kind(token) != Kind::QuestionToken {
                self.parse_expected(Kind::QuestionToken);
            }
        }
        let type_node = self.parse_type_annotation();
        self.parse_semicolon();
        let members = self.parse_list(ParsingContext::TypeMembers, Self::parse_type_member);
        self.parse_expected(Kind::CloseBraceToken);
        let node = self.new_mapped_type_node(
            readonly_token,
            type_parameter,
            name_type,
            question_token,
            type_node,
            Some(members),
        );
        self.finish_node(node, pos)
    }

    fn parse_mapped_type_parameter(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let name = self.parse_identifier_name();
        self.parse_expected(Kind::InKeyword);
        let type_node = self.parse_type();
        let node = self.new_type_parameter_declaration(
            None, /*modifiers*/
            name,
            Some(type_node),
            None, /*expression*/
            None, /*defaultType*/
        );
        self.finish_node(node, pos)
    }

    fn parse_type_literal(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let members = self.parse_object_type_members();
        let node = self.new_type_literal_node(members);
        self.set_lazy_owner(members, node);
        self.finish_node(node, pos)
    }

    fn parse_object_type_members(&mut self) -> Self::List {
        if self.parse_expected(Kind::OpenBraceToken) {
            let members =
                self.parse_member_list(ParsingContext::TypeMembers, Self::parse_type_member);
            self.parse_expected(Kind::CloseBraceToken);
            return members;
        }
        self.create_missing_list()
    }

    fn parse_tuple_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let elements = self
            .parse_bracketed_list(
                ParsingContext::TupleElementTypes,
                Self::parse_tuple_element_name_or_tuple_element_type,
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            )
            .unwrap();
        let node = self.new_tuple_type_node(elements);
        self.finish_node(node, pos)
    }

    fn parse_tuple_element_name_or_tuple_element_type(&mut self) -> Self::Node {
        if self.look_ahead(|parser| parser.scan_start_of_named_tuple_element()) {
            let pos = self.node_pos();
            let jsdoc = self.jsdoc_scanner_info();
            let dot_dot_dot_token = self.parse_optional_token(Kind::DotDotDotToken);
            let name = self.parse_identifier_name();
            let question_token = self.parse_optional_token(Kind::QuestionToken);
            self.parse_expected(Kind::ColonToken);
            let type_node = self.parse_tuple_element_type();
            let node =
                self.new_named_tuple_member(dot_dot_dot_token, name, question_token, type_node);
            let result = self.finish_node(node, pos);
            self.with_jsdoc(result, jsdoc);
            return result;
        }
        self.parse_tuple_element_type()
    }

    fn parse_tuple_element_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        if self.parse_optional(Kind::DotDotDotToken) {
            let type_ = self.parse_type();
            let node = self.new_rest_type_node(type_);
            return self.finish_node(node, pos);
        }
        let type_node = self.parse_type();
        if self.node_kind(type_node) == Kind::JSDocNullableType
            && self.node_range(type_node).pos()
                == self.node_range(self.node_type(type_node).unwrap()).pos()
        {
            let inner = self.node_type(type_node).unwrap();
            let node = self.new_optional_type_node(inner);
            self.set_node_flags(node, self.node_flags(type_node));
            self.set_node_range(node, self.node_range(type_node));
            self.set_node_parent(inner, Some(node));
            return node;
        }
        type_node
    }

    fn parse_template_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let head = self.parse_template_head(false /*isTaggedTemplate*/);
        let spans = self.parse_template_type_spans();
        let node = self.new_template_literal_type_node(head, spans);
        self.finish_node(node, pos)
    }

    fn parse_template_type_spans(&mut self) -> Self::List {
        let pos = self.node_pos();
        let start = self.node_stack().len();
        loop {
            let span = self.parse_template_type_span();
            self.node_stack_mut().push(span);
            if self.node_kind(self.template_type_span_literal(span)) != Kind::TemplateMiddle {
                break;
            }
        }
        let end = self.node_pos();
        self.finish_node_list(start, TextRange::new(pos, end))
    }

    fn parse_template_type_span(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let type_ = self.parse_type();
        let literal = self.parse_literal_of_template_span(false /*isTaggedTemplate*/);
        let node = self.new_template_literal_type_span(type_, literal);
        self.finish_node(node, pos)
    }

    fn parse_type_parameters(&mut self) -> Option<Self::List> {
        if self.token() == Kind::LessThanToken {
            return self.parse_bracketed_list(
                ParsingContext::TypeParameters,
                Self::parse_type_parameter,
                Kind::LessThanToken,
                Kind::GreaterThanToken,
            );
        }
        None
    }

    fn parse_type_parameter(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let modifiers = self.parse_modifiers_ex(
            false, /*allowDecorators*/
            true,  /*permitConstAsModifier*/
            false, /*stopOnStartOfClassStaticBlock*/
        );
        let name = self.parse_identifier();
        let mut constraint: Option<Self::Node> = None;
        let mut expression: Option<Self::Node> = None;
        if self.parse_optional(Kind::ExtendsKeyword) {
            // It's not uncommon for people to write improper constraints to a generic.  If the
            // user writes a constraint that is an expression and not an actual type, then parse
            // it out as an expression (so we can recover well), but report that a type is needed
            // instead.
            if self.is_start_of_type(false /*inStartOfParameter*/) || !self.is_start_of_expression()
            {
                constraint = Some(self.parse_type());
            } else {
                // It was not a type, and it looked like an expression.  Parse out an expression
                // here so we recover well.  Note: it is important that we call parseUnaryExpression
                // and not parseExpression here.  If the user has:
                //
                //      <T extends "">
                //
                // We do *not* want to consume the `>` as we're consuming the expression for "".
                expression = Some(self.parse_unary_expression_or_higher());
            }
        }
        let mut default_type: Option<Self::Node> = None;
        if self.parse_optional(Kind::EqualsToken) {
            default_type = Some(self.parse_type());
        }
        let result = self.new_type_parameter_declaration(
            modifiers,
            name,
            constraint,
            expression,
            default_type,
        );
        self.finish_node(result, pos)
    }

    fn parse_return_type(&mut self, return_token: Kind, is_type: bool) -> Option<Self::Node> {
        if self.should_parse_return_type(return_token, is_type) {
            return Some(self.do_in_context(
                NodeFlags::DisallowConditionalTypesContext,
                false,
                Self::parse_type_or_type_predicate,
            ));
        }
        None
    }

    fn parse_function_or_constructor_type(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_for_constructor_type();
        let is_constructor_type = self.parse_optional(Kind::NewKeyword);
        assert!(
            modifiers.is_none() || is_constructor_type,
            "Per isStartOfFunctionOrConstructorType, a function type cannot have modifiers."
        );
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::Type);
        let return_type =
            self.parse_return_type(Kind::EqualsGreaterThanToken, false /*isType*/);

        let result = if is_constructor_type {
            self.new_constructor_type_node(
                modifiers,
                type_parameters,
                Some(parameters),
                return_type,
            )
        } else {
            self.new_function_type_node(type_parameters, Some(parameters), return_type)
        };
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_modifiers_for_constructor_type(&mut self) -> Option<Self::Modifiers> {
        if self.token() == Kind::AbstractKeyword {
            let pos = self.node_pos();
            let modifier = self.new_modifier(self.token());
            self.next_token();
            self.finish_node(modifier, pos);
            return Some(self.new_modifier_list(self.node_range(modifier), &[modifier]));
        }
        None
    }

    fn parse_type_annotation(&mut self) -> Option<Self::Node> {
        if self.parse_optional(Kind::ColonToken) {
            return Some(self.parse_type());
        }
        None
    }
    fn parse_type_member(&mut self) -> Self::Node {
        if self.token() == Kind::OpenParenToken || self.token() == Kind::LessThanToken {
            return self.parse_signature_member(Kind::CallSignature);
        }
        if self.token() == Kind::NewKeyword
            && self.look_ahead(|parser| parser.next_token_is_open_paren_or_less_than())
        {
            return self.parse_signature_member(Kind::ConstructSignature);
        }
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers();
        if self.parse_contextual_modifier(Kind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::GetAccessor,
                ParseFlags::Type,
            );
        }
        if self.parse_contextual_modifier(Kind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::SetAccessor,
                ParseFlags::Type,
            );
        }
        if self.is_index_signature() {
            return self.parse_index_signature_declaration(pos, jsdoc, modifiers);
        }
        self.parse_property_or_method_signature(pos, jsdoc, modifiers)
    }

    fn parse_signature_member(&mut self, kind: Kind) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if kind == Kind::ConstructSignature {
            self.parse_expected(Kind::NewKeyword);
        }
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::Type);
        let type_node = self.parse_return_type(Kind::ColonToken, true /*isType*/);
        self.parse_type_member_semicolon();

        let result = if kind == Kind::CallSignature {
            self.new_call_signature_declaration(type_parameters, Some(parameters), type_node)
        } else {
            self.new_construct_signature_declaration(type_parameters, Some(parameters), type_node)
        };
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_parameters(&mut self, flags: ParseFlags) -> Self::List {
        // FormalParameters [Yield,Await]: (modified)
        //      [empty]
        //      FormalParameterList[?Yield,Await]
        //
        // FormalParameter[Yield,Await]: (modified)
        //      BindingElement[?Yield,Await]
        //
        // BindingElement [Yield,Await]: (modified)
        //      SingleNameBinding[?Yield,?Await]
        //      BindingPattern[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        //
        // SingleNameBinding [Yield,Await]:
        //      BindingIdentifier[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        if self.parse_expected(Kind::OpenParenToken) {
            // With allowAmbiguity, parseParameterEx never returns nil, so neither does the list.
            let parameters = self.parse_parameters_worker(flags, true /*allowAmbiguity*/).unwrap();
            self.parse_expected(Kind::CloseParenToken);
            return parameters;
        }
        self.create_missing_list()
    }

    fn parse_parameters_worker(
        &mut self,
        flags: ParseFlags,
        allow_ambiguity: bool,
    ) -> Option<Self::List> {
        // FormalParameters [Yield,Await]: (modified)
        //      [empty]
        //      FormalParameterList[?Yield,Await]
        //
        // FormalParameter[Yield,Await]: (modified)
        //      BindingElement[?Yield,Await]
        //
        // BindingElement [Yield,Await]: (modified)
        //      SingleNameBinding[?Yield,?Await]
        //      BindingPattern[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        //
        // SingleNameBinding [Yield,Await]:
        //      BindingIdentifier[?Yield,?Await]Initializer [In, ?Yield,?Await] opt
        let in_await_context = self.context_flags().intersects(NodeFlags::AwaitContext);
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::YieldContext, flags.intersects(ParseFlags::Yield));
        self.set_context_flags(NodeFlags::AwaitContext, flags.intersects(ParseFlags::Await));
        let parameters =
            self.parse_delimited_list(ParsingContext::Parameters, move |p: &mut Self| {
                let parameter = p.parse_parameter_ex(in_await_context, allow_ambiguity);
                if let Some(parameter) = parameter
                    && !flags.intersects(ParseFlags::Type)
                {
                    p.check_js_syntax(parameter);
                }
                parameter
            });
        self.replace_context_flags(save_context_flags);
        parameters
    }

    fn parse_parameter(&mut self) -> Self::Node {
        self.parse_parameter_ex(false /*inOuterAwaitContext*/, true /*allowAmbiguity*/).unwrap()
    }

    fn parse_parameter_ex(
        &mut self,
        in_outer_await_context: bool,
        allow_ambiguity: bool,
    ) -> Option<Self::Node> {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        // FormalParameter [Yield,Await]:
        //      BindingElement[?Yield,?Await]
        // Decorators are parsed in the outer [Await] context, the rest of the parameter is parsed in the function's [Await] context.
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::AwaitContext, in_outer_await_context);
        let modifiers = self.parse_modifiers_ex(
            true,  /*allowDecorators*/
            false, /*permitConstAsModifier*/
            false, /*stopOnStartOfClassStaticBlock*/
        );
        self.replace_context_flags(save_context_flags);
        if self.token() == Kind::ThisKeyword {
            let name = self.create_identifier(true /*isIdentifier*/);
            let type_annotation = self.parse_type_annotation();
            let result = self.new_parameter_declaration(
                modifiers,
                None, /*dotDotDotToken*/
                name,
                None, /*questionToken*/
                type_annotation,
                None, /*initializer*/
            );
            if let Some(modifiers) = modifiers {
                self.error_at(
                    self.modifier_first_range(modifiers),
                    &diagnostics::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS,
                    &[],
                );
            }
            let result = self.finish_node(result, pos);
            self.with_jsdoc(result, jsdoc);
            return Some(result);
        }
        let dot_dot_dot_token = self.parse_optional_token(Kind::DotDotDotToken);
        if !allow_ambiguity && !self.is_parameter_name_start() {
            return None;
        }
        let name = self.parse_name_of_parameter(modifiers);
        let question_token = self.parse_optional_token(Kind::QuestionToken);
        let type_annotation = self.parse_type_annotation();
        let initializer = self.parse_initializer();
        let result = self.new_parameter_declaration(
            modifiers,
            dot_dot_dot_token,
            name,
            question_token,
            type_annotation,
            initializer,
        );
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        Some(result)
    }

    fn parse_name_of_parameter(&mut self, modifiers: Option<Self::Modifiers>) -> Self::Node {
        // FormalParameter [Yield,Await]:
        //      BindingElement[?Yield,?Await]
        let name = self.parse_identifier_or_pattern_with_diagnostic(Some(
            &diagnostics::PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS,
        ));
        if self.node_range(name).len() == Some(0)
            && modifiers.is_none()
            && ast::is_modifier_kind(self.token())
        {
            // in cases like
            // 'use strict'
            // function foo(static)
            // isParameter('static') == true, because of isModifier('static')
            // however 'static' is not a legal identifier in a strict mode.
            // so result of this function will be Parameter (flags = 0, name = missing, type = undefined, initializer = undefined)
            // and current token will not change => parsing of the enclosing parameter list will last till the end of time (or OOM)
            // to avoid this we'll advance cursor to the next token.
            self.next_token();
        }
        name
    }

    fn parse_type_member_semicolon(&mut self) {
        // We allow type members to be separated by commas or (possibly ASI) semicolons.
        // First check if it was a comma.  If so, we're done with the member.
        if self.parse_optional(Kind::CommaToken) {
            return;
        }
        // Didn't have a comma.  We must have a (possible ASI) semicolon.
        self.parse_semicolon();
    }

    fn parse_accessor_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        kind: Kind,
        flags: ParseFlags,
    ) -> Self::Node {
        let name = self.parse_property_name();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(ParseFlags::None);
        let return_type = self.parse_return_type(Kind::ColonToken, false /*isType*/);
        let body = self.parse_function_block_or_semicolon(flags, None /*diagnosticMessage*/);

        // Keep track of `typeParameters` (for both) and `type` (for setters) if they were parsed those indicate grammar errors
        let result = if kind == Kind::GetAccessor {
            self.new_get_accessor_declaration(
                modifiers,
                name,
                type_parameters,
                Some(parameters),
                return_type,
                None, /*fullSignature*/
                body,
            )
        } else {
            self.new_set_accessor_declaration(
                modifiers,
                name,
                type_parameters,
                Some(parameters),
                return_type,
                None, /*fullSignature*/
                body,
            )
        };
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        if !flags.intersects(ParseFlags::Type) {
            self.check_js_syntax(result);
        }
        result
    }

    fn parse_property_name(&mut self) -> Self::Node {
        let save_has_await_identifier = self.statement_has_await_identifier();
        let prop = self.parse_property_name_worker(true /*allowComputedPropertyNames*/);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        prop
    }

    fn parse_property_name_worker(&mut self, allow_computed_property_names: bool) -> Self::Node {
        if self.token() == Kind::StringLiteral
            || self.token() == Kind::NumericLiteral
            || self.token() == Kind::BigIntLiteral
        {
            return self.parse_literal_expression();
        }
        if allow_computed_property_names && self.token() == Kind::OpenBracketToken {
            return self.parse_computed_property_name();
        }
        if self.token() == Kind::PrivateIdentifier {
            return self.parse_private_identifier();
        }
        self.parse_identifier_name()
    }

    fn parse_computed_property_name(&mut self) -> Self::Node {
        // PropertyName [Yield]:
        //      LiteralPropertyName
        //      ComputedPropertyName[?Yield]
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenBracketToken);
        // We parse any expression (including a comma expression). But the grammar
        // says that only an assignment expression is allowed, so the grammar checker
        // will error if it sees a comma expression.
        let expression = self.parse_expression_allow_in();
        self.parse_expected(Kind::CloseBracketToken);
        let node = self.new_computed_property_name(expression);
        self.finish_node(node, pos)
    }

    fn parse_function_block_or_semicolon(
        &mut self,
        flags: ParseFlags,
        diagnostic_message: Option<&'static Message>,
    ) -> Option<Self::Node> {
        if self.token() != Kind::OpenBraceToken {
            if flags.intersects(ParseFlags::Type) {
                self.parse_type_member_semicolon();
                return None;
            }
            if self.can_parse_semicolon() {
                self.parse_semicolon();
                return None;
            }
        }
        Some(self.parse_function_block(flags, diagnostic_message))
    }

    fn parse_function_block(
        &mut self,
        flags: ParseFlags,
        diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let save_context_flags = self.context_flags();
        let save_has_await_identifier = self.statement_has_await_identifier();
        self.set_context_flags(NodeFlags::YieldContext, flags.intersects(ParseFlags::Yield));
        self.set_context_flags(NodeFlags::AwaitContext, flags.intersects(ParseFlags::Await));
        // We may be in a [Decorator] context when parsing a function expression or
        // arrow function. The body of the function is not in [Decorator] context.
        self.set_context_flags(NodeFlags::DecoratorContext, false);
        let block = self
            .parse_block(flags.intersects(ParseFlags::IgnoreMissingOpenBrace), diagnostic_message);
        self.replace_context_flags(save_context_flags);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        block
    }

    fn parse_index_signature_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let parameters = self
            .parse_bracketed_list(
                ParsingContext::Parameters,
                Self::parse_parameter,
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            )
            .unwrap();
        let type_node = self.parse_type_annotation();
        self.parse_type_member_semicolon();
        let node = self.new_index_signature_declaration(modifiers, Some(parameters), type_node);
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_property_or_method_signature(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let name = self.parse_property_name();
        let question_token = self.parse_optional_token(Kind::QuestionToken);

        let result = if self.token() == Kind::OpenParenToken || self.token() == Kind::LessThanToken
        {
            // Method signatures don't exist in expression contexts.  So they have neither
            // [Yield] nor [Await]
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameters(ParseFlags::Type);
            let return_type = self.parse_return_type(Kind::ColonToken, true /*isType*/);
            self.new_method_signature_declaration(
                modifiers,
                name,
                question_token,
                type_parameters,
                Some(parameters),
                return_type,
            )
        } else {
            let type_node = self.parse_type_annotation();
            // Although type literal properties cannot not have initializers, we attempt
            // to parse an initializer so we can report in the checker that an interface
            // property or type literal property cannot have an initializer.
            let mut initializer: Option<Self::Node> = None;
            if self.token() == Kind::EqualsToken {
                initializer = self.parse_initializer();
            }
            self.new_property_signature_declaration(
                modifiers,
                name,
                question_token,
                type_node,
                initializer,
            )
        };
        self.parse_type_member_semicolon();
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_modifiers(&mut self) -> Option<Self::Modifiers> {
        self.parse_modifiers_ex(false, false, false)
    }

    #[inline]
    fn parse_modifiers_ex(
        &mut self,
        allow_decorators: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<Self::Modifiers> {
        if !(ast::is_modifier_kind(self.token())
            || allow_decorators && self.token() == Kind::AtToken)
        {
            return None;
        }
        self.parse_modifiers_worker(
            allow_decorators,
            permit_const_as_modifier,
            stop_on_start_of_class_static_block,
        )
    }

    #[inline(never)]
    fn parse_modifiers_worker(
        &mut self,
        allow_decorators: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<Self::Modifiers> {
        let mut has_leading_modifier = false;
        let mut has_trailing_decorator = false;
        let mut has_trailing_modifier = false;
        let mut has_static_modifier = false;
        // Decorators should be contiguous in a list of modifiers but can potentially appear in two places (i.e., `[...leadingDecorators, ...leadingModifiers, ...trailingDecorators, ...trailingModifiers]`).
        // The leading modifiers *should* only contain `export` and `default` when trailingDecorators are present, but we'll handle errors for any other leading modifiers in the checker.
        // It is illegal to have both leadingDecorators and trailingDecorators, but we will report that as a grammar check in the checker.
        // parse leading decorators
        let pos = self.node_pos();
        let start = self.node_stack().len();
        loop {
            if allow_decorators && self.token() == Kind::AtToken && !has_trailing_modifier {
                let decorator = self.parse_decorator();
                self.node_stack_mut().push(decorator);
                if has_leading_modifier {
                    has_trailing_decorator = true;
                }
            } else {
                let Some(modifier) = self.try_parse_modifier(
                    has_static_modifier,
                    permit_const_as_modifier,
                    stop_on_start_of_class_static_block,
                ) else {
                    break;
                };
                if self.node_kind(modifier) == Kind::StaticKeyword {
                    has_static_modifier = true;
                }
                self.node_stack_mut().push(modifier);
                if has_trailing_decorator {
                    has_trailing_modifier = true;
                } else {
                    has_leading_modifier = true;
                }
            }
        }
        if self.node_stack().len() > start {
            let end = self.node_pos();
            let list = self.finish_modifier_list(start, TextRange::new(pos, end));
            return Some(list);
        }
        None
    }

    fn parse_decorator(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::AtToken);
        let expression =
            self.do_in_context(NodeFlags::DecoratorContext, true, Self::parse_decorator_expression);
        let node = self.new_decorator(expression);
        self.finish_node(node, pos)
    }

    fn try_parse_modifier(
        &mut self,
        has_seen_static_modifier: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Option<Self::Node> {
        let pos = self.node_pos();
        let kind = self.token();
        if self.token() == Kind::ConstKeyword && permit_const_as_modifier {
            // We need to ensure that any subsequent modifiers appear on the same line
            // so that when 'const' is a standalone declaration, we don't issue an error.
            if !self
                .look_ahead(|parser| parser.next_token_is_on_same_line_and_can_follow_modifier())
            {
                return None;
            }
            self.next_token();
        } else if (stop_on_start_of_class_static_block
            && self.token() == Kind::StaticKeyword
            && self.look_ahead(|parser| parser.next_token_is_open_brace()))
            || (has_seen_static_modifier && self.token() == Kind::StaticKeyword)
            || (!self.parse_any_contextual_modifier())
        {
            return None;
        }
        let node = self.new_modifier(kind);
        Some(self.finish_node(node, pos))
    }
    fn parse_import_type(&mut self) -> Self::Node {
        self.add_source_flags(NodeFlags::PossiblyContainsDynamicImport);
        let pos = self.node_pos();
        let is_type_of = self.parse_optional(Kind::TypeOfKeyword);
        self.parse_expected(Kind::ImportKeyword);
        self.parse_expected(Kind::OpenParenToken);
        let type_node = self.parse_type();
        let mut attributes: Option<Self::Node> = None;
        if self.parse_optional(Kind::CommaToken) {
            let open_brace_position = self.scanner().token_start();
            self.parse_expected(Kind::OpenBraceToken);
            let current_token = self.token();
            if current_token == Kind::WithKeyword || current_token == Kind::AssertKeyword {
                if current_token == Kind::AssertKeyword {
                    self.parse_error_at_current_token(
                        &diagnostics::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_ASSERT,
                        &[],
                    );
                }
                self.next_token();
            } else {
                self.parse_error_at_current_token(
                    &diagnostics::X_0_EXPECTED,
                    &[&scanner::token_to_string(Kind::WithKeyword)],
                );
            }
            self.parse_expected(Kind::ColonToken);
            attributes = Some(self.parse_import_attributes(current_token, true /*skipKeyword*/));
            self.parse_optional(Kind::CommaToken);
            if !self.parse_expected(Kind::CloseBraceToken)
                && let Some(last_diagnostic) = self.last_expected_diagnostic()
            {
                self.add_bracket_related_info(
                    last_diagnostic,
                    TextRange::new(open_brace_position, open_brace_position),
                    &diagnostics::THE_PARSER_EXPECTED_TO_FIND_A_1_TO_MATCH_THE_0_TOKEN_HERE,
                    "{",
                    "}",
                );
            }
        }
        self.parse_expected(Kind::CloseParenToken);
        let mut qualifier: Option<Self::Node> = None;
        if self.parse_optional(Kind::DotToken) {
            qualifier = Some(self.parse_entity_name_of_type_reference());
        }
        let type_arguments = self.parse_type_arguments_of_type_reference();
        let node =
            self.new_import_type_node(is_type_of, type_node, attributes, qualifier, type_arguments);
        self.finish_node(node, pos)
    }

    fn parse_import_attribute(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let mut name: Option<Self::Node> = None;
        if token_is_identifier_or_keyword(self.token()) {
            name = Some(self.parse_identifier_name());
        } else if self.token() == Kind::StringLiteral {
            name = Some(self.parse_literal_expression());
        }
        if name.is_some() {
            self.parse_expected(Kind::ColonToken);
        } else {
            self.parse_error_at_current_token(
                &diagnostics::IDENTIFIER_OR_STRING_LITERAL_EXPECTED,
                &[],
            );
        }
        let value = self.parse_assignment_expression_or_higher();
        let node = self.new_import_attribute(name, value);
        self.finish_node(node, pos)
    }

    fn parse_import_attributes(&mut self, token: Kind, skip_keyword: bool) -> Self::Node {
        let pos = self.node_pos();
        if !skip_keyword {
            self.parse_expected(token);
        }
        let elements;
        let mut multi_line = false;
        let open_brace_position = self.scanner().token_start();
        if self.parse_expected(Kind::OpenBraceToken) {
            multi_line = self.has_preceding_line_break();
            elements = self.parse_delimited_list(
                ParsingContext::ImportAttributes,
                Self::parse_import_attribute,
            );
            if !self.parse_expected(Kind::CloseBraceToken)
                && let Some(last_diagnostic) = self.last_expected_diagnostic()
            {
                self.add_bracket_related_info(
                    last_diagnostic,
                    TextRange::new(open_brace_position, open_brace_position),
                    &diagnostics::THE_PARSER_EXPECTED_TO_FIND_A_1_TO_MATCH_THE_0_TOKEN_HERE,
                    "{",
                    "}",
                );
            }
        } else {
            elements = Some(self.parse_empty_node_list());
        }
        let node = self.new_import_attributes(token, elements.unwrap(), multi_line);
        self.finish_node(node, pos)
    }

    fn parse_expression(&mut self) -> Self::Node {
        // Expression[in]:
        //      AssignmentExpression[in]
        //      Expression[in] , AssignmentExpression[in]

        // clear the decorator context when parsing Expression, as it should be unambiguous when parsing a decorator
        let save_context_flags = self.context_flags();
        self.replace_context_flags(self.context_flags() & !NodeFlags::DecoratorContext);
        let pos = self.node_pos();
        let mut expr = self.parse_assignment_expression_or_higher();
        while let Some(operator_token) = self.parse_optional_token(Kind::CommaToken) {
            let right = self.parse_assignment_expression_or_higher();
            expr = self.make_binary_expression(expr, operator_token, right, pos);
        }
        self.replace_context_flags(save_context_flags);
        expr
    }

    fn parse_expression_allow_in(&mut self) -> Self::Node {
        self.do_in_context(NodeFlags::DisallowInContext, false, Self::parse_expression)
    }

    fn parse_assignment_expression_or_higher(&mut self) -> Self::Node {
        self.parse_assignment_expression_or_higher_worker(
            true, /*allowReturnTypeInArrowFunction*/
        )
    }

    fn parse_assignment_expression_or_higher_worker(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Self::Node {
        //  AssignmentExpression[in,yield]:
        //      1) ConditionalExpression[?in,?yield]
        //      2) LeftHandSideExpression = AssignmentExpression[?in,?yield]
        //      3) LeftHandSideExpression AssignmentOperator AssignmentExpression[?in,?yield]
        //      4) ArrowFunctionExpression[?in,?yield]
        //      5) AsyncArrowFunctionExpression[in,yield,await]
        //      6) [+Yield] YieldExpression[?In]
        //
        // Note: for ease of implementation we treat productions '2' and '3' as the same thing.
        // (i.e. they're both BinaryExpressions with an assignment operator in it).
        // First, do the simple check if we have a YieldExpression (production '6').
        if self.is_yield_expression() {
            return self.parse_yield_expression();
        }
        // Then, check if we have an arrow function (production '4' and '5') that starts with a parenthesized
        // parameter list or is an async arrow function.
        // AsyncArrowFunctionExpression:
        //      1) async[no LineTerminator here]AsyncArrowBindingIdentifier[?Yield][no LineTerminator here]=>AsyncConciseBody[?In]
        //      2) CoverCallExpressionAndAsyncArrowHead[?Yield, ?Await][no LineTerminator here]=>AsyncConciseBody[?In]
        // Production (1) of AsyncArrowFunctionExpression is parsed in "tryParseAsyncSimpleArrowFunctionExpression".
        // And production (2) is parsed in "tryParseParenthesizedArrowFunctionExpression".
        //
        // If we do successfully parse arrow-function, we must *not* recurse for productions 1, 2 or 3. An ArrowFunction is
        // not a LeftHandSideExpression, nor does it start a ConditionalExpression.  So we are done
        // with AssignmentExpression if we see one.
        if let Some(arrow_expression) = self
            .try_parse_parenthesized_arrow_function_expression(allow_return_type_in_arrow_function)
        {
            return arrow_expression;
        }
        if let Some(arrow_expression) = self
            .try_parse_async_simple_arrow_function_expression(allow_return_type_in_arrow_function)
        {
            return arrow_expression;
        }
        // Now try to see if we're in production '1', '2' or '3'.  A conditional expression can
        // start with a LogicalOrExpression, while the assignment productions can only start with
        // LeftHandSideExpressions.
        //
        // So, first, we try to just parse out a BinaryExpression.  If we get something that is a
        // LeftHandSide or higher, then we can try to parse out the assignment expression part.
        // Otherwise, we try to parse out the conditional expression bit.  We want to allow any
        // binary expression here, so we pass in the 'lowest' precedence here so that it matches
        // and consumes anything.
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let expr = self.parse_binary_expression_or_higher(OperatorPrecedence::LOWEST);
        // To avoid a look-ahead, we did not handle the case of an arrow function with a single un-parenthesized
        // parameter ('x => ...') above. We handle it here by checking if the parsed expression was a single
        // identifier and the current token is an arrow.
        if self.node_kind(expr) == Kind::Identifier && self.token() == Kind::EqualsGreaterThanToken
        {
            return self.parse_simple_arrow_function_expression(
                pos,
                expr,
                allow_return_type_in_arrow_function,
                jsdoc,
                None, /*asyncModifier*/
            );
        }
        // Now see if we might be in cases '2' or '3'.
        // If the expression was a LHS expression, and we have an assignment operator, then
        // we're in '2' or '3'. Consume the assignment and return.
        //
        // Note: we call reScanGreaterToken so that we get an appropriately merged token
        // for cases like `> > =` becoming `>>=`
        if self.node_is_left_hand_side_expression(expr)
            && ast::is_assignment_operator(self.re_scan_greater_than_token())
        {
            let operator_token = self.parse_token_node();
            let right = self
                .parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function);
            return self.make_binary_expression(expr, operator_token, right, pos);
        }
        // It wasn't an assignment or a lambda.  This is a conditional expression:
        self.parse_conditional_expression_rest(expr, pos, allow_return_type_in_arrow_function)
    }

    fn parse_yield_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        // YieldExpression[In] :
        //      yield
        //      yield [no LineTerminator here] [Lexical goal InputElementRegExp]AssignmentExpression[?In, Yield]
        //      yield [no LineTerminator here] * [Lexical goal InputElementRegExp]AssignmentExpression[?In, Yield]
        self.next_token();

        let result = if !self.has_preceding_line_break()
            && (self.token() == Kind::AsteriskToken || self.is_start_of_expression())
        {
            let asterisk_token = self.parse_optional_token(Kind::AsteriskToken);
            let expression = self.parse_assignment_expression_or_higher();
            self.new_yield_expression(asterisk_token, Some(expression))
        } else {
            // if the next token is not on the same line as yield.  or we don't have an '*' or
            // the start of an expression, then this is just a simple "yield" expression.
            self.new_yield_expression(None /*asteriskToken*/, None /*expression*/)
        };
        self.finish_node(result, pos)
    }

    fn parse_conditional_expression_rest(
        &mut self,
        left_operand: Self::Node,
        pos: u32,
        allow_return_type_in_arrow_function: bool,
    ) -> Self::Node {
        // Note: we are passed in an expression which was produced from parseBinaryExpressionOrHigher.
        let Some(question_token) = self.parse_optional_token(Kind::QuestionToken) else {
            return left_operand;
        };
        // Note: we explicitly 'allowIn' in the whenTrue part of the condition expression, and
        // we do not that for the 'whenFalse' part.
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let true_expression = self.parse_assignment_expression_or_higher_worker(
            false, /*allowReturnTypeInArrowFunction*/
        );
        self.replace_context_flags(save_context_flags);
        let colon_token = self.parse_expected_token(Kind::ColonToken);
        let false_expression = if self.node_is_present(colon_token) {
            self.parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function)
        } else {
            self.create_missing_identifier()
        };
        let node = self.new_conditional_expression(
            left_operand,
            question_token,
            true_expression,
            colon_token,
            false_expression,
        );
        self.finish_node(node, pos)
    }

    fn parse_binary_expression_or_higher(&mut self, precedence: OperatorPrecedence) -> Self::Node {
        let pos = self.node_pos();
        let left_operand = self.parse_unary_expression_or_higher();
        self.parse_binary_expression_rest(precedence, left_operand, pos)
    }

    fn parse_binary_expression_rest(
        &mut self,
        precedence: OperatorPrecedence,
        left_operand: Self::Node,
        pos: u32,
    ) -> Self::Node {
        let mut left_operand = left_operand;
        let mut last_operand = left_operand;
        loop {
            // We either have a binary operator here, or we're finished.  We call
            // reScanGreaterToken so that we merge token sequences like > and = into >=
            let operator = self.re_scan_greater_than_token();
            let new_precedence = ast::get_binary_operator_precedence(operator);
            // Check the precedence to see if we should "take" this operator
            // - For left associative operator (all operator but **), consume the operator,
            //   recursively call the function below, and parse binaryExpression as a rightOperand
            //   of the caller if the new precedence of the operator is greater then or equal to the current precedence.
            //   For example:
            //      a - b - c;
            //            ^token; leftOperand = b. Return b to the caller as a rightOperand
            //      a * b - c
            //            ^token; leftOperand = b. Return b to the caller as a rightOperand
            //      a - b * c;
            //            ^token; leftOperand = b. Return b * c to the caller as a rightOperand
            // - For right associative operator (**), consume the operator, recursively call the function
            //   and parse binaryExpression as a rightOperand of the caller if the new precedence of
            //   the operator is strictly grater than the current precedence
            //   For example:
            //      a ** b ** c;
            //             ^^token; leftOperand = b. Return b ** c to the caller as a rightOperand
            //      a - b ** c;
            //            ^^token; leftOperand = b. Return b ** c to the caller as a rightOperand
            //      a ** b - c
            //             ^token; leftOperand = b. Return b to the caller as a rightOperand
            if !should_consume_binary_operator(operator, new_precedence, precedence) {
                break;
            }
            if operator == Kind::InKeyword && self.in_disallow_in_context() {
                break;
            }
            if operator == Kind::AsKeyword || operator == Kind::SatisfiesKeyword {
                // Make sure we *do* perform ASI for constructs like this:
                //    var x = foo
                //    as (Bar)
                // This should be parsed as an initialized variable, followed
                // by a function call to 'as' with the argument 'Bar'
                if self.has_preceding_line_break() {
                    break;
                }
                self.next_token();
                // When we have 'a ## b as SomeType $$ c' or 'a ## b satisfies SomeType $$ c', where ## and $$
                // are binary operators, we want to stop parsing when $$ would bind before ## after erasing the
                // assertion. See https://github.com/microsoft/TypeScript/issues/63527.
                let mut last_precedence = OperatorPrecedence::HIGHEST;
                if self.node_kind(last_operand) == Kind::BinaryExpression {
                    last_precedence = ast::get_binary_operator_precedence(
                        self.binary_operator_kind(last_operand),
                    );
                }
                if operator == Kind::SatisfiesKeyword {
                    let type_node = self.parse_type();
                    left_operand = self.make_satisfies_expression(left_operand, type_node);
                } else {
                    let type_node = self.parse_type();
                    left_operand = self.make_as_expression(left_operand, type_node);
                }
                // Stop if the next operator would bind before the last operator when the assertion is erased.
                let next_operator = self.re_scan_greater_than_token();
                let next_precedence = ast::get_binary_operator_precedence(next_operator);
                if should_consume_binary_operator(next_operator, next_precedence, last_precedence) {
                    break;
                }
            } else {
                let operator_token = self.parse_token_node();
                let right = self.parse_binary_expression_or_higher(new_precedence);
                left_operand =
                    self.make_binary_expression(left_operand, operator_token, right, pos);
                last_operand = left_operand;
            }
        }
        left_operand
    }

    fn make_satisfies_expression(
        &mut self,
        expression: Self::Node,
        type_node: Self::Node,
    ) -> Self::Node {
        let node = self.new_satisfies_expression(expression, type_node);
        let node = self.finish_node(
            node,
            self.node_range(expression).pos().expect_source("parsed nodes have source positions"),
        );
        self.check_js_syntax(node)
    }

    fn make_as_expression(&mut self, left: Self::Node, right: Self::Node) -> Self::Node {
        let node = self.new_as_expression(left, right);
        let node = self.finish_node(
            node,
            self.node_range(left).pos().expect_source("parsed nodes have source positions"),
        );
        self.check_js_syntax(node)
    }

    fn make_binary_expression(
        &mut self,
        left: Self::Node,
        operator_token: Self::Node,
        right: Self::Node,
        pos: u32,
    ) -> Self::Node {
        let node = self.new_binary_expression(
            None, /*modifiers*/
            left,
            None, /*typeNode*/
            operator_token,
            right,
        );
        self.finish_node(node, pos)
    }

    #[inline]
    fn parse_unary_expression_or_higher(&mut self) -> Self::Node {
        // ES7 UpdateExpression:
        //      1) LeftHandSideExpression[?Yield]
        //      2) LeftHandSideExpression[?Yield][no LineTerminator here]++
        //      3) LeftHandSideExpression[?Yield][no LineTerminator here]--
        //      4) ++UnaryExpression[?Yield]
        //      5) --UnaryExpression[?Yield]
        if self.is_update_expression() {
            let pos = self.node_pos();
            let update_expression = self.parse_update_expression();
            if self.token() == Kind::AsteriskAsteriskToken {
                return self.parse_binary_expression_rest(
                    ast::get_binary_operator_precedence(self.token()),
                    update_expression,
                    pos,
                );
            }
            return update_expression;
        }
        // ES7 UnaryExpression:
        //      1) UpdateExpression[?yield]
        //      2) delete UpdateExpression[?yield]
        //      3) void UpdateExpression[?yield]
        //      4) typeof UpdateExpression[?yield]
        //      5) + UpdateExpression[?yield]
        //      6) - UpdateExpression[?yield]
        //      7) ~ UpdateExpression[?yield]
        //      8) ! UpdateExpression[?yield]
        let unary_operator = self.token();
        let simple_unary_expression = self.parse_simple_unary_expression();
        if self.token() == Kind::AsteriskAsteriskToken {
            self.report_invalid_exponentiation_operand(simple_unary_expression, unary_operator);
        }
        simple_unary_expression
    }

    #[cold]
    fn report_invalid_exponentiation_operand(
        &mut self,
        simple_unary_expression: Self::Node,
        unary_operator: Kind,
    ) {
        let pos = self.skip_expression_trivia(
            self.node_range(simple_unary_expression)
                .pos()
                .expect_source("parsed nodes have source positions"),
        );
        let end = self
            .node_range(simple_unary_expression)
            .end()
            .expect_source("parsed nodes have source positions");
        if self.node_kind(simple_unary_expression) == Kind::TypeAssertionExpression {
            self.error_at(TextRange::new(pos, end), &diagnostics::A_TYPE_ASSERTION_EXPRESSION_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTIATION_EXPRESSION_CONSIDER_ENCLOSING_THE_EXPRESSION_IN_PARENTHESES, &[]);
        } else {
            assert!(is_keyword_or_punctuation(unary_operator));
            self.error_at(TextRange::new(pos, end), &diagnostics::AN_UNARY_EXPRESSION_WITH_THE_0_OPERATOR_IS_NOT_ALLOWED_IN_THE_LEFT_HAND_SIDE_OF_AN_EXPONENTIATION_EXPRESSION_CONSIDER_ENCLOSING_THE_EXPRESSION_IN_PARENTHESES, &[&scanner::token_to_string(unary_operator)]);
        }
    }

    fn is_update_expression(&self) -> bool {
        match self.token() {
            Kind::PlusToken
            | Kind::MinusToken
            | Kind::TildeToken
            | Kind::ExclamationToken
            | Kind::DeleteKeyword
            | Kind::TypeOfKeyword
            | Kind::VoidKeyword
            | Kind::AwaitKeyword => false,
            Kind::LessThanToken => self.language_variant() == LanguageVariant::JSX,
            _ => true,
        }
    }

    // Outlining ++/-- leaves a small common path; forcing it inline saves instructions in the parser corpora.
    #[expect(
        clippy::inline_always,
        reason = "Measured on the Compiler, VS Code, and t3code parser corpora after outlining update operators"
    )]
    #[inline(always)]
    fn parse_update_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        if self.token() == Kind::PlusPlusToken || self.token() == Kind::MinusMinusToken {
            return self.parse_prefix_update_expression(pos);
        } else if self.language_variant() == LanguageVariant::JSX
            && self.token() == Kind::LessThanToken
            && self
                .look_ahead(|parser| parser.next_token_is_identifier_or_keyword_or_greater_than())
        {
            // JSXElement is part of primaryExpression
            return self.parse_jsx_element_or_self_closing_element_or_fragment(
                true,  /* inExpressionContext */
                None,  /* topInvalidNodePosition */
                None,  /* openingTag */
                false, /* mustBeUnary */
            );
        }
        let expression = self.parse_left_hand_side_expression_or_higher();
        if (self.token() == Kind::PlusPlusToken || self.token() == Kind::MinusMinusToken)
            && !self.has_preceding_line_break()
        {
            return self.parse_postfix_update_expression(pos, expression);
        }
        expression
    }

    #[inline(never)]
    fn parse_prefix_update_expression(&mut self, pos: u32) -> Self::Node {
        let operator = self.token();
        self.next_token();
        let operand = self.parse_left_hand_side_expression_or_higher();
        let node = self.new_prefix_unary_expression(operator, operand);
        self.finish_node(node, pos)
    }

    #[inline(never)]
    fn parse_postfix_update_expression(&mut self, pos: u32, expression: Self::Node) -> Self::Node {
        let operator = self.token();
        self.next_token();
        let node = self.new_postfix_unary_expression(expression, operator);
        self.finish_node(node, pos)
    }

    fn parse_simple_unary_expression(&mut self) -> Self::Node {
        let token = self.token();
        match token {
            Kind::PlusToken | Kind::MinusToken | Kind::TildeToken | Kind::ExclamationToken => {
                self.parse_prefix_unary_expression()
            }
            Kind::DeleteKeyword => self.parse_delete_expression(),
            Kind::TypeOfKeyword => self.parse_type_of_expression(),
            Kind::VoidKeyword => self.parse_void_expression(),
            Kind::LessThanToken => {
                // Just like in parseUpdateExpression, we need to avoid parsing type assertions when
                // in JSX and we see an expression like "+ <foo> bar".
                if self.language_variant() == LanguageVariant::JSX {
                    return self.parse_jsx_element_or_self_closing_element_or_fragment(
                        true, None, None, true,
                    );
                }
                // // This is modified UnaryExpression grammar in TypeScript
                // //  UnaryExpression (modified):
                // //      < type > UnaryExpression
                self.parse_type_assertion()
            }
            Kind::AwaitKeyword if self.is_await_expression() => self.parse_await_expression(),
            _ => self.parse_update_expression(),
        }
    }

    fn parse_prefix_unary_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let operator = self.token();
        self.next_token();
        let operand = self.parse_simple_unary_expression();
        let node = self.new_prefix_unary_expression(operator, operand);
        self.finish_node(node, pos)
    }

    fn parse_delete_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.new_delete_expression(expression);
        self.finish_node(node, pos)
    }

    fn parse_type_of_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.new_type_of_expression(expression);
        self.finish_node(node, pos)
    }

    fn parse_void_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.new_void_expression(expression);
        self.finish_node(node, pos)
    }

    fn parse_await_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.next_token();
        let expression = self.parse_simple_unary_expression();
        let node = self.new_await_expression(expression);
        self.finish_node(node, pos)
    }

    fn parse_type_assertion(&mut self) -> Self::Node {
        assert!(
            self.language_variant() != LanguageVariant::JSX,
            "Type assertions should never be parsed in JSX; they should be parsed as comparisons or JSX elements/fragments."
        );
        let pos = self.node_pos();
        self.parse_expected(Kind::LessThanToken);
        let type_node = self.parse_type();
        self.parse_expected(Kind::GreaterThanToken);
        let expression = self.parse_simple_unary_expression();
        let node = self.new_type_assertion(type_node, expression);
        self.finish_node(node, pos)
    }
    fn is_parenthesized_arrow_function_expression(&mut self) -> Tristate {
        if self.token() == Kind::OpenParenToken
            || self.token() == Kind::LessThanToken
            || self.token() == Kind::AsyncKeyword
        {
            let state = self.mark();
            let result = self.next_is_parenthesized_arrow_function_expression();
            self.rewind(state);
            return result;
        }
        if self.token() == Kind::EqualsGreaterThanToken {
            // ERROR RECOVERY TWEAK:
            // If we see a standalone => try to parse it as an arrow function expression as that's
            // likely what the user intended to write.
            return Tristate::True;
        }
        // Definitely not a parenthesized arrow function.
        Tristate::False
    }

    fn next_is_parenthesized_arrow_function_expression(&mut self) -> Tristate {
        if self.token() == Kind::AsyncKeyword {
            self.next_token();
            if self.has_preceding_line_break() {
                return Tristate::False;
            }
            if self.token() != Kind::OpenParenToken && self.token() != Kind::LessThanToken {
                return Tristate::False;
            }
        }
        let first = self.token();
        let second = self.next_token();
        if first == Kind::OpenParenToken {
            if second == Kind::CloseParenToken {
                // Simple cases: "() =>", "(): ", and "() {".
                // This is an arrow function with no parameters.
                // The last one is not actually an arrow function,
                // but this is probably what the user intended.
                let third = self.next_token();
                match third {
                    Kind::EqualsGreaterThanToken | Kind::ColonToken | Kind::OpenBraceToken => {
                        return Tristate::True;
                    }
                    _ => {}
                }
                return Tristate::False;
            }
            // If encounter "([" or "({", this could be the start of a binding pattern.
            // Examples:
            //      ([ x ]) => { }
            //      ({ x }) => { }
            //      ([ x ])
            //      ({ x })
            if second == Kind::OpenBracketToken || second == Kind::OpenBraceToken {
                return Tristate::Unknown;
            }
            // Simple case: "(..."
            // This is an arrow function with a rest parameter.
            if second == Kind::DotDotDotToken {
                return Tristate::True;
            }
            // Check for "(xxx yyy", where xxx is a modifier and yyy is an identifier. This
            // isn't actually allowed, but we want to treat it as a lambda so we can provide
            // a good error message.
            if ast::is_modifier_kind(second)
                && second != Kind::AsyncKeyword
                && self.look_ahead(|parser| parser.next_token_is_identifier())
            {
                if self.next_token() == Kind::AsKeyword {
                    // https://github.com/microsoft/TypeScript/issues/44466
                    return Tristate::False;
                }
                return Tristate::True;
            }
            // If we had "(" followed by something that's not an identifier,
            // then this definitely doesn't look like a lambda.  "this" is not
            // valid, but we want to parse it and then give a semantic error.
            if !self.is_identifier() && second != Kind::ThisKeyword {
                return Tristate::False;
            }
            match self.next_token() {
                Kind::ColonToken => {
                    // If we have something like "(a:", then we must have a
                    // type-annotated parameter in an arrow function expression.
                    return Tristate::True;
                }
                Kind::QuestionToken => {
                    self.next_token();
                    // If we have "(a?:" or "(a?," or "(a?=" or "(a?)" then it is definitely a lambda.
                    if self.token() == Kind::ColonToken
                        || self.token() == Kind::CommaToken
                        || self.token() == Kind::EqualsToken
                        || self.token() == Kind::CloseParenToken
                    {
                        return Tristate::True;
                    }
                    // Otherwise it is definitely not a lambda.
                    return Tristate::False;
                }
                Kind::CommaToken | Kind::EqualsToken | Kind::CloseParenToken => {
                    // If we have "(a," or "(a=" or "(a)" this *could* be an arrow function
                    return Tristate::Unknown;
                }
                _ => {}
            }
            // It is definitely not an arrow function
            Tristate::False
        } else {
            assert_eq!(first, Kind::LessThanToken);
            // If we have "<" not followed by an identifier,
            // then this definitely is not an arrow function.
            if !self.is_identifier() && self.token() != Kind::ConstKeyword {
                return Tristate::False;
            }
            // JSX overrides
            if self.language_variant() == LanguageVariant::JSX {
                let is_arrow_function_in_jsx = self.look_ahead(|p| {
                    p.parse_optional(Kind::ConstKeyword);
                    let third = p.next_token();
                    if third == Kind::ExtendsKeyword {
                        let fourth = p.next_token();
                        match fourth {
                            Kind::EqualsToken | Kind::GreaterThanToken | Kind::SlashToken => {
                                return false;
                            }
                            _ => {}
                        }
                        return true;
                    } else if third == Kind::CommaToken || third == Kind::EqualsToken {
                        return true;
                    }
                    false
                });
                if is_arrow_function_in_jsx {
                    return Tristate::True;
                }
                return Tristate::False;
            }
            // This *could* be a parenthesized arrow function.
            Tristate::Unknown
        }
    }

    #[inline]
    fn try_parse_parenthesized_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Self::Node> {
        // Keep the speculative snapshot in the worker, including standalone `=>` for error recovery.
        if !matches!(
            self.token(),
            Kind::OpenParenToken
                | Kind::LessThanToken
                | Kind::AsyncKeyword
                | Kind::EqualsGreaterThanToken
        ) {
            return None;
        }
        self.try_parse_parenthesized_arrow_function_expression_worker(
            allow_return_type_in_arrow_function,
        )
    }

    #[inline(never)]
    fn try_parse_parenthesized_arrow_function_expression_worker(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Self::Node> {
        let tristate = self.is_parenthesized_arrow_function_expression();
        if tristate == Tristate::False {
            // It's definitely not a parenthesized arrow function expression.
            return None;
        }
        // If we definitely have an arrow function, then we can just parse one, not requiring a
        // following => or { token. Otherwise, we *might* have an arrow function.  Try to parse
        // it out, but don't allow any ambiguity, and return 'undefined' if this could be an
        // expression instead.
        if tristate == Tristate::True {
            return self.parse_parenthesized_arrow_function_expression(
                true, /*allowAmbiguity*/
                true, /*allowReturnTypeInArrowFunction*/
            );
        }
        let state = self.mark();
        let result = self.parse_possible_parenthesized_arrow_function_expression(
            allow_return_type_in_arrow_function,
        );
        if result.is_none() {
            self.rewind(state);
        }
        result
    }

    fn parse_parenthesized_arrow_function_expression(
        &mut self,
        allow_ambiguity: bool,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Self::Node> {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_for_arrow_function();
        let is_async = self.modifier_list_has_async(modifiers);
        let signature_flags = if is_async { ParseFlags::Await } else { ParseFlags::None };
        // Arrow functions are never generators.
        //
        // If we're speculatively parsing a signature for a parenthesized arrow function, then
        // we have to have a complete parameter list.  Otherwise we might see something like
        // a => (b => c)
        // And think that "(b =>" was actually a parenthesized arrow function with a missing
        // close paren.
        let type_parameters = self.parse_type_parameters();
        let parameters: Self::List;
        if self.parse_expected(Kind::OpenParenToken) {
            if allow_ambiguity {
                parameters =
                    self.parse_parameters_worker(signature_flags, allow_ambiguity).unwrap();
            } else {
                let maybe_parameters =
                    self.parse_parameters_worker(signature_flags, allow_ambiguity);
                {
                    let maybe_parameters = maybe_parameters?;
                    parameters = maybe_parameters;
                }
            }
            if !self.parse_expected(Kind::CloseParenToken) && !allow_ambiguity {
                return None;
            }
        } else {
            if !allow_ambiguity {
                return None;
            }
            parameters = self.create_missing_list();
        }
        let has_return_colon = self.token() == Kind::ColonToken;
        let return_type = self.parse_return_type(Kind::ColonToken, false /*isType*/);
        if let Some(return_type) = return_type
            && !allow_ambiguity
            && self.type_has_arrow_function_blocking_parse_error(return_type)
        {
            return None;
        }
        // Parsing a signature isn't enough.
        // Parenthesized arrow signatures often look like other valid expressions.
        // For instance:
        //  - "(x = 10)" is an assignment expression parsed as a signature with a default parameter value.
        //  - "(x,y)" is a comma expression parsed as a signature with two parameters.
        //  - "a ? (b): c" will have "(b):" parsed as a signature with a return type annotation.
        //  - "a ? (b): function() {}" will too, since function() is a valid JSDoc function type.
        //  - "a ? (b): (function() {})" as well, but inside of a parenthesized type with an arbitrary amount of nesting.
        //
        // So we need just a bit of lookahead to ensure that it can only be a signature.
        let mut unwrapped_type = return_type;
        while let Some(t) = unwrapped_type {
            if self.node_kind(t) != Kind::ParenthesizedType {
                break;
            }
            unwrapped_type = self.node_type(t); // Skip parens if need be
        }
        if !allow_ambiguity
            && self.token() != Kind::EqualsGreaterThanToken
            && self.token() != Kind::OpenBraceToken
        {
            // Returning undefined here will cause our caller to rewind to where we started from.
            return None;
        }
        // If we have an arrow, then try to parse the body. Even if not, try to parse if we
        // have an opening brace, just in case we're in an error state.
        let last_token = self.token();
        let equals_greater_than_token = self.parse_expected_token(Kind::EqualsGreaterThanToken);
        let body = if last_token == Kind::EqualsGreaterThanToken
            || last_token == Kind::OpenBraceToken
        {
            self.parse_arrow_function_expression_body(is_async, allow_return_type_in_arrow_function)
        } else {
            self.parse_identifier()
        };
        // Given:
        //     x ? y => ({ y }) : z => ({ z })
        // We try to parse the body of the first arrow function by looking at:
        //     ({ y }) : z => ({ z })
        // This is a valid arrow function with "z" as the return type.
        //
        // But, if we're in the true side of a conditional expression, this colon
        // terminates the expression, so we cannot allow a return type if we aren't
        // certain whether or not the preceding text was parsed as a parameter list.
        //
        // For example,
        //     a() ? (b: number, c?: string): void => d() : e
        // is determined by isParenthesizedArrowFunctionExpression to unambiguously
        // be an arrow expression, so we allow a return type.
        if !allow_return_type_in_arrow_function && has_return_colon {
            // However, if the arrow function we were able to parse is followed by another colon
            // as in:
            //     a ? (x): string => x : null
            // Then allow the arrow function, and treat the second colon as terminating
            // the conditional expression. It's okay to do this because this code would
            // be a syntax error in JavaScript (as the second colon shouldn't be there).
            if self.token() != Kind::ColonToken {
                return None;
            }
        }
        let node = self.new_arrow_function(
            modifiers,
            type_parameters,
            Some(parameters),
            return_type,
            None, /*fullSignature*/
            Some(equals_greater_than_token),
            Some(body),
        );
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        Some(result)
    }

    fn parse_modifiers_for_arrow_function(&mut self) -> Option<Self::Modifiers> {
        if self.token() == Kind::AsyncKeyword {
            let pos = self.node_pos();
            self.next_token();
            let node = self.new_modifier(Kind::AsyncKeyword);
            let modifier = self.finish_node(node, pos);
            return Some(self.new_modifier_list(self.node_range(modifier), &[modifier]));
        }
        None
    }

    fn parse_arrow_function_expression_body(
        &mut self,
        is_async: bool,
        allow_return_type_in_arrow_function: bool,
    ) -> Self::Node {
        if self.token() == Kind::OpenBraceToken {
            return self.parse_function_block(
                if is_async { ParseFlags::Await } else { ParseFlags::None },
                None, /*diagnosticMessage*/
            );
        }
        if self.token() != Kind::SemicolonToken
            && self.token() != Kind::FunctionKeyword
            && self.token() != Kind::ClassKeyword
            && self.is_start_of_statement()
            && !self.is_start_of_expression_statement()
        {
            // Check if we got a plain statement (i.e. no expression-statements, no function/class expressions/declarations)
            //
            // Here we try to recover from a potential error situation in the case where the
            // user meant to supply a block. For example, if the user wrote:
            //
            //  a =>
            //      let v = 0;
            //  }
            //
            // they may be missing an open brace.  Check to see if that's the case so we can
            // try to recover better.  If we don't do this, then the next close curly we see may end
            // up preemptively closing the containing construct.
            //
            // Note: even when 'IgnoreMissingOpenBrace' is passed, parseBody will still error.
            return self.parse_function_block(
                ParseFlags::IgnoreMissingOpenBrace
                    | if is_async { ParseFlags::Await } else { ParseFlags::None },
                None, /*diagnosticMessage*/
            );
        }
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::AwaitContext, is_async);
        self.set_context_flags(NodeFlags::YieldContext, false);
        let node =
            self.parse_assignment_expression_or_higher_worker(allow_return_type_in_arrow_function);
        self.replace_context_flags(save_context_flags);
        node
    }

    fn parse_possible_parenthesized_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Self::Node> {
        let token_pos = self.scanner().token_start();
        if self.not_parenthesized_arrow_contains(token_pos) {
            return None;
        }
        let result = self.parse_parenthesized_arrow_function_expression(
            false, /*allowAmbiguity*/
            allow_return_type_in_arrow_function,
        );
        if result.is_none() {
            self.remember_not_parenthesized_arrow(token_pos);
        }
        result
    }

    fn try_parse_async_simple_arrow_function_expression(
        &mut self,
        allow_return_type_in_arrow_function: bool,
    ) -> Option<Self::Node> {
        // We do a check here so that we won't be doing unnecessarily call to "lookAhead"
        if self.token() == Kind::AsyncKeyword
            && self.look_ahead(|parser| parser.next_is_un_parenthesized_async_arrow_function())
        {
            let pos = self.node_pos();
            let jsdoc = self.jsdoc_scanner_info();
            let async_modifier = self.parse_modifiers_for_arrow_function();
            let expr = self.parse_binary_expression_or_higher(OperatorPrecedence::LOWEST);
            return Some(self.parse_simple_arrow_function_expression(
                pos,
                expr,
                allow_return_type_in_arrow_function,
                jsdoc,
                async_modifier,
            ));
        }
        None
    }

    fn parse_simple_arrow_function_expression(
        &mut self,
        pos: u32,
        identifier: Self::Node,
        allow_return_type_in_arrow_function: bool,
        jsdoc: JsdocScannerInfo,
        async_modifier: Option<Self::Modifiers>,
    ) -> Self::Node {
        assert!(
            self.token() == Kind::EqualsGreaterThanToken,
            "parseSimpleArrowFunctionExpression should only have been called if we had a =>"
        );
        let node = self.new_parameter_declaration(
            None, /*modifiers*/
            None, /*dotDotDotToken*/
            identifier, None, /*questionToken*/
            None, /*typeNode*/
            None, /*initializer*/
        );
        let parameter = self.finish_node(
            node,
            self.node_range(identifier).pos().expect_source("parsed nodes have source positions"),
        );
        let parameters = self.new_node_list(self.node_range(parameter), &[parameter]);
        let equals_greater_than_token = self.parse_expected_token(Kind::EqualsGreaterThanToken);
        let body = self.parse_arrow_function_expression_body(
            async_modifier.is_some(), /*isAsync*/
            allow_return_type_in_arrow_function,
        );
        let node = self.new_arrow_function(
            async_modifier,
            None, /*typeParameters*/
            Some(parameters),
            None, /*returnType*/
            None, /*fullSignature*/
            Some(equals_greater_than_token),
            Some(body),
        );
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_jsx_element_or_self_closing_element_or_fragment(
        &mut self,
        in_expression_context: bool,
        top_invalid_node_position: Option<u32>,
        opening_tag: Option<Self::Node>,
        must_be_unary: bool,
    ) -> Self::Node {
        let pos = self.node_pos();
        let opening = self
            .parse_jsx_opening_or_self_closing_element_or_opening_fragment(in_expression_context);
        let mut result: Self::Node;
        match self.node_kind(opening) {
            Kind::JsxOpeningElement => {
                let mut children = self.parse_jsx_children(opening);
                let closing_element: Self::Node;
                let last_child = self
                    .list_len(children)
                    .checked_sub(1)
                    .map(|index| self.list_element(children, index));
                if let Some(last_child) = last_child.filter(|&last_child| {
                    self.node_kind(last_child) == Kind::JsxElement
                        && !self.jsx_tag_names_are_equivalent(
                            self.jsx_tag_name(self.jsx_element_parts(last_child).0),
                            self.jsx_tag_name(self.jsx_element_parts(last_child).2),
                        )
                        && self.jsx_tag_names_are_equivalent(
                            self.jsx_tag_name(opening),
                            self.jsx_tag_name(self.jsx_element_parts(last_child).2),
                        )
                }) {
                    // when an unclosed JsxOpeningElement incorrectly parses its parent's JsxClosingElement,
                    // restructure (<div>(...<span>...</div>)) --> (<div>(...<span>...</>)</div>)
                    // (no need to error; the parent will error)
                    let (last_opening, last_children, last_closing) =
                        self.jsx_element_parts(last_child);
                    let end = self
                        .list_range(last_children)
                        .end()
                        .expect_source("parsed lists have source positions");
                    let missing = self.new_identifier("");
                    let missing_identifier = self.finish_node_with_end(missing, end, end);
                    let closing = self.new_jsx_closing_element(missing_identifier);
                    let new_closing_element = self.finish_node_with_end(closing, end, end);
                    let element =
                        self.new_jsx_element(last_opening, last_children, new_closing_element);
                    let new_last = self.finish_node_with_end(
                        element,
                        self.node_range(last_opening)
                            .pos()
                            .expect_source("parsed nodes have source positions"),
                        end,
                    );
                    // force reset parent pointers from discarded parse result
                    self.set_node_parent(last_opening, Some(new_last));
                    for index in 0..self.list_len(last_children) {
                        let child = self.list_element(last_children, index);
                        self.set_node_parent(child, Some(new_last));
                    }
                    self.set_node_parent(new_closing_element, Some(new_last));
                    let mut nodes: Vec<_> = (0..self.list_len(children) - 1)
                        .map(|index| self.list_element(children, index))
                        .collect();
                    nodes.push(new_last);
                    children = self.new_node_list(
                        TextRange::new(
                            self.list_range(children)
                                .pos()
                                .expect_source("parsed lists have source positions"),
                            self.node_range(new_last)
                                .end()
                                .expect_source("parsed nodes have source positions"),
                        ),
                        &nodes,
                    );
                    closing_element = last_closing;
                } else {
                    closing_element =
                        self.parse_jsx_closing_element(opening, in_expression_context);
                    if !self.jsx_tag_names_are_equivalent(
                        self.jsx_tag_name(opening),
                        self.jsx_tag_name(closing_element),
                    ) {
                        if opening_tag.is_some_and(|opening_tag| {
                            self.node_kind(opening_tag) == Kind::JsxOpeningElement
                                && self.jsx_tag_names_are_equivalent(
                                    self.jsx_tag_name(closing_element),
                                    self.jsx_tag_name(opening_tag),
                                )
                        }) {
                            // opening incorrectly matched with its parent's closing -- put error on opening
                            let text = self.expression_source_text(self.jsx_tag_name(opening));
                            self.error_at(
                                self.node_range(self.jsx_tag_name(opening)),
                                &diagnostics::JSX_ELEMENT_0_HAS_NO_CORRESPONDING_CLOSING_TAG,
                                &[&text],
                            );
                        } else {
                            // other opening/closing mismatches -- put error on closing
                            let text = self.expression_source_text(self.jsx_tag_name(opening));
                            self.error_at(
                                self.node_range(self.jsx_tag_name(closing_element)),
                                &diagnostics::EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR_0,
                                &[&text],
                            );
                        }
                    }
                }
                let element = self.new_jsx_element(opening, children, closing_element);
                result = self.finish_node(element, pos);
                self.set_node_parent(closing_element, Some(result)); // force reset parent pointers from possibly discarded parse result
            }
            Kind::JsxOpeningFragment => {
                let children = self.parse_jsx_children(opening);
                let closing_fragment = self.parse_jsx_closing_fragment(in_expression_context);
                let fragment = self.new_jsx_fragment(opening, children, closing_fragment);
                result = self.finish_node(fragment, pos);
            }
            Kind::JsxSelfClosingElement => {
                // Nothing else to do for self-closing elements
                result = opening;
            }
            _ => panic!("Unhandled case in parseJsxElementOrSelfClosingElementOrFragment"),
        }
        // If the user writes the invalid code '<div></div><div></div>' in an expression context (i.e. not wrapped in
        // an enclosing tag), we'll naively try to parse   ^ this as a 'less than' operator and the remainder of the tag
        // as garbage, which will cause the formatter to badly mangle the JSX. Perform a speculative parse of a JSX
        // element if we see a < token so that we can wrap it in a synthetic binary expression so the formatter
        // does less damage and we can report a better error.
        // Since JSX elements are invalid < operands anyway, this lookahead parse will only occur in error scenarios
        // of one sort or another.
        // If we are in a unary context, we can't do this recovery; the binary expression we return here is not
        // a valid UnaryExpression and will cause problems later.
        if !must_be_unary && in_expression_context && self.token() == Kind::LessThanToken {
            let top_bad_pos = top_invalid_node_position.unwrap_or_else(|| {
                self.node_range(result).pos().expect_source("parsed nodes have source positions")
            });
            let invalid_element = self.parse_jsx_element_or_self_closing_element_or_fragment(
                true,
                Some(top_bad_pos),
                None,
                false,
            );
            let operator_token = self.new_token(Kind::CommaToken);
            let invalid_pos = self
                .node_range(invalid_element)
                .pos()
                .expect_source("parsed nodes have source positions");
            self.set_node_range(operator_token, TextRange::new(invalid_pos, invalid_pos));
            self.error_at(
                TextRange::new(
                    self.skip_expression_trivia(top_bad_pos),
                    self.node_range(invalid_element)
                        .end()
                        .expect_source("parsed nodes have source positions"),
                ),
                &diagnostics::JSX_EXPRESSIONS_MUST_HAVE_ONE_PARENT_ELEMENT,
                &[],
            );
            let binary = self.new_binary_expression(
                None, /*modifiers*/
                result,
                None, /*typeNode*/
                operator_token,
                invalid_element,
            );
            result = self.finish_node(binary, pos);
        }
        result
    }

    fn parse_jsx_children(&mut self, opening_tag: Self::Node) -> Self::List {
        let pos = self.node_pos();
        let save_parsing_contexts = self.parsing_contexts();
        self.set_parsing_contexts(
            self.parsing_contexts() | (1 << ParsingContext::JsxChildren as u32),
        );
        let start = self.node_stack().len();
        loop {
            let current_token = self.re_scan_jsx_token(true /*allowMultilineJsxText*/);
            let Some(child) = self.parse_jsx_child(opening_tag, current_token) else {
                break;
            };
            self.node_stack_mut().push(child);
            if self.node_kind(opening_tag) == Kind::JsxOpeningElement
                && self.node_kind(child) == Kind::JsxElement
                && !self.jsx_tag_names_are_equivalent(
                    self.jsx_tag_name(self.jsx_element_parts(child).0),
                    self.jsx_tag_name(self.jsx_element_parts(child).2),
                )
                && self.jsx_tag_names_are_equivalent(
                    self.jsx_tag_name(opening_tag),
                    self.jsx_tag_name(self.jsx_element_parts(child).2),
                )
            {
                // stop after parsing a mismatched child like <div>...(<span></div>) in order to reattach the </div> higher
                break;
            }
        }
        self.set_parsing_contexts(save_parsing_contexts);
        let end = self.node_pos();
        self.finish_node_list(start, TextRange::new(pos, end))
    }

    fn parse_jsx_child(&mut self, opening_tag: Self::Node, token: Kind) -> Option<Self::Node> {
        match token {
            Kind::EndOfFile => {
                // If we hit EOF, issue the error at the tag that lacks the closing element
                // rather than at the end of the file (which is useless)
                if self.node_kind(opening_tag) == Kind::JsxOpeningFragment {
                    self.error_at(
                        self.node_range(opening_tag),
                        &diagnostics::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                        &[],
                    );
                } else {
                    // We want the error span to cover only 'Foo.Bar' in < Foo.Bar >
                    // or to cover only 'Foo' in < Foo >
                    let tag = self.jsx_tag_name(opening_tag);
                    let tag_end = self
                        .node_range(tag)
                        .end()
                        .expect_source("parsed nodes have source positions");
                    let start = std::cmp::min(
                        self.skip_expression_trivia(
                            self.node_range(tag)
                                .pos()
                                .expect_source("parsed nodes have source positions"),
                        ),
                        tag_end,
                    );
                    let text = self.expression_source_text(self.jsx_tag_name(opening_tag));
                    self.error_at(
                        TextRange::new(start, tag_end),
                        &diagnostics::JSX_ELEMENT_0_HAS_NO_CORRESPONDING_CLOSING_TAG,
                        &[&text],
                    );
                }
                None
            }
            Kind::LessThanSlashToken | Kind::ConflictMarkerTrivia => None,
            Kind::JsxText | Kind::JsxTextAllWhiteSpaces => Some(self.parse_jsx_text()),
            Kind::OpenBraceToken => self.parse_jsx_expression(false /*inExpressionContext*/),
            Kind::LessThanToken => {
                Some(self.parse_jsx_element_or_self_closing_element_or_fragment(
                    false,
                    None,
                    Some(opening_tag),
                    false,
                ))
            }
            _ => panic!("Unhandled case in parseJsxChild"),
        }
    }

    fn parse_jsx_text(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let text = self.take_current_text();
        let result = self.new_scanned_jsx_text(text, self.token() == Kind::JsxTextAllWhiteSpaces);
        self.scan_jsx_text();
        self.finish_node(result, pos)
    }

    fn parse_jsx_expression(&mut self, in_expression_context: bool) -> Option<Self::Node> {
        let pos = self.node_pos();
        if !self.parse_expected(Kind::OpenBraceToken) {
            return None;
        }
        let mut dot_dot_dot_token: Option<Self::Node> = None;
        let mut expression: Option<Self::Node> = None;
        if self.token() != Kind::CloseBraceToken {
            if !in_expression_context {
                dot_dot_dot_token = self.parse_optional_token(Kind::DotDotDotToken);
            }
            // Only an AssignmentExpression is valid here per the JSX spec,
            // but we can unambiguously parse a comma sequence and provide
            // a better error message in grammar checking.
            expression = Some(self.parse_expression());
        }
        if in_expression_context {
            self.parse_expected(Kind::CloseBraceToken);
        } else if self.parse_expected_without_advancing(Kind::CloseBraceToken) {
            self.scan_jsx_text();
        }
        let node = self.new_jsx_expression(dot_dot_dot_token, expression);
        Some(self.finish_node(node, pos))
    }

    fn parse_jsx_closing_element(
        &mut self,
        open: Self::Node,
        in_expression_context: bool,
    ) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        if self.parse_expected_with_diagnostic(
            Kind::GreaterThanToken,
            None,  /*diagnosticMessage*/
            false, /*shouldAdvance*/
        ) {
            // manually advance the scanner in order to look for jsx text inside jsx
            if in_expression_context
                || !self.jsx_tag_names_are_equivalent(self.jsx_tag_name(open), tag_name)
            {
                self.next_token();
            } else {
                self.scan_jsx_text();
            }
        }
        let node = self.new_jsx_closing_element(tag_name);
        self.finish_node(node, pos)
    }

    fn parse_jsx_opening_or_self_closing_element_or_opening_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::LessThanToken);
        if self.token() == Kind::GreaterThanToken {
            // See below for explanation of scanJsxText
            self.scan_jsx_text();
            let node = self.new_jsx_opening_fragment();
            return self.finish_node(node, pos);
        }
        let tag_name = self.parse_jsx_element_name();
        let mut type_arguments: Option<Self::List> = None;
        if !self.context_flags().intersects(NodeFlags::JavaScriptFile) {
            type_arguments = self.parse_type_arguments();
        }
        let attributes = self.parse_jsx_attributes();

        let result = if self.token() == Kind::GreaterThanToken {
            // Closing tag, so scan the immediately-following text with the JSX scanning instead
            // of regular scanning to avoid treating illegal characters (e.g. '#') as immediate
            // scanning errors
            self.scan_jsx_text();
            self.new_jsx_opening_element(tag_name, type_arguments, attributes)
        } else {
            self.parse_expected(Kind::SlashToken);
            if self.parse_expected_without_advancing(Kind::GreaterThanToken) {
                if in_expression_context {
                    self.next_token();
                } else {
                    self.scan_jsx_text();
                }
            }
            self.new_jsx_self_closing_element(tag_name, type_arguments, attributes)
        };
        self.finish_node(result, pos)
    }

    fn parse_jsx_element_name(&mut self) -> Self::Node {
        let pos = self.node_pos();
        // JsxElement can have name in the form of
        //      propertyAccessExpression
        //      primaryExpression in the form of an identifier and "this" keyword
        // We can't just simply use parseLeftHandSideExpressionOrHigher because then we will start consider class,function etc as a keyword
        // We only want to consider "this" as a primaryExpression
        let initial_expression = self.parse_jsx_tag_name();
        if self.node_kind(initial_expression) == Kind::JsxNamespacedName {
            return initial_expression; // `a:b.c` is invalid syntax, don't even look for the `.` if we parse `a:b`, and let `parseAttribute` report "unexpected :" instead.
        }
        let mut expression = initial_expression;
        while self.parse_optional(Kind::DotToken) {
            let name = self.parse_right_side_of_dot(
                true,  /*allowIdentifierNames*/
                false, /*allowPrivateIdentifiers*/
                false, /*allowUnicodeEscapeSequenceInIdentifierName*/
            );
            let node = self.new_property_access_expression(expression, None, name, NodeFlags::None);
            expression = self.finish_node(node, pos);
        }
        expression
    }

    fn parse_jsx_tag_name(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.scan_jsx_identifier();
        let is_this = self.token() == Kind::ThisKeyword;
        let tag_name = self.parse_identifier_name_error_on_unicode_escape_sequence();
        if self.parse_optional(Kind::ColonToken) {
            self.scan_jsx_identifier();
            let name = self.parse_identifier_name_error_on_unicode_escape_sequence();
            let node = self.new_jsx_namespaced_name(tag_name, name);
            return self.finish_node(node, pos);
        }
        if is_this {
            let result = self.new_keyword_expression(Kind::ThisKeyword);
            return self.finish_node(result, pos);
        }
        tag_name
    }

    fn parse_jsx_attributes(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let properties = self.parse_list(ParsingContext::JsxAttributes, Self::parse_jsx_attribute);
        let node = self.new_jsx_attributes(properties);
        self.finish_node(node, pos)
    }

    fn parse_jsx_attribute(&mut self) -> Self::Node {
        if self.token() == Kind::OpenBraceToken {
            return self.parse_jsx_spread_attribute();
        }
        let pos = self.node_pos();
        let name = self.parse_jsx_attribute_name();
        let initializer = self.parse_jsx_attribute_value();
        let node = self.new_jsx_attribute(name, initializer);
        self.finish_node(node, pos)
    }

    fn parse_jsx_spread_attribute(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenBraceToken);
        self.parse_expected(Kind::DotDotDotToken);
        let expression = self.parse_expression();
        self.parse_expected(Kind::CloseBraceToken);
        let node = self.new_jsx_spread_attribute(expression);
        self.finish_node(node, pos)
    }

    fn parse_jsx_attribute_name(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.scan_jsx_identifier();
        let attr_name = self.parse_identifier_name_error_on_unicode_escape_sequence();
        if self.parse_optional(Kind::ColonToken) {
            self.scan_jsx_identifier();
            let name = self.parse_identifier_name_error_on_unicode_escape_sequence();
            let node = self.new_jsx_namespaced_name(attr_name, name);
            return self.finish_node(node, pos);
        }
        attr_name
    }

    fn parse_jsx_attribute_value(&mut self) -> Option<Self::Node> {
        if self.token() == Kind::EqualsToken {
            if self.scan_jsx_attribute_value() == Kind::StringLiteral {
                return Some(self.parse_literal_expression());
            }
            if self.token() == Kind::OpenBraceToken {
                return self.parse_jsx_expression(true /*inExpressionContext*/);
            }
            if self.token() == Kind::LessThanToken {
                // An attribute value must be a single JsxAttributeValue, so don't allow the sibling-element
                // recovery to wrap it in a synthetic binary expression.
                return Some(self.parse_jsx_element_or_self_closing_element_or_fragment(
                    true, None, None, true,
                ));
            }
            self.parse_error_at_current_token(&diagnostics::X_OR_JSX_ELEMENT_EXPECTED, &[]);
        }
        None
    }

    fn parse_jsx_closing_fragment(&mut self, in_expression_context: bool) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::LessThanSlashToken);
        if self.parse_expected_with_diagnostic(
            Kind::GreaterThanToken,
            Some(&diagnostics::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT),
            false, /*shouldAdvance*/
        ) {
            // manually advance the scanner in order to look for jsx text inside jsx
            if in_expression_context {
                self.next_token();
            } else {
                self.scan_jsx_text();
            }
        }
        let node = self.new_jsx_closing_fragment();
        self.finish_node(node, pos)
    }

    #[inline]
    fn parse_left_hand_side_expression_or_higher(&mut self) -> Self::Node {
        // Original Ecma:
        // LeftHandSideExpression: See 11.2
        //      NewExpression
        //      CallExpression
        //
        // Our simplification:
        //
        // LeftHandSideExpression: See 11.2
        //      MemberExpression
        //      CallExpression
        //
        // See comment in parseMemberExpressionOrHigher on how we replaced NewExpression with
        // MemberExpression to make our lives easier.
        //
        // to best understand the below code, it's important to see how CallExpression expands
        // out into its own productions:
        //
        // CallExpression:
        //      MemberExpression Arguments
        //      CallExpression Arguments
        //      CallExpression[Expression]
        //      CallExpression.IdentifierName
        //      import (AssignmentExpression)
        //      super Arguments
        //      super.IdentifierName
        //
        // Because of the recursion in these calls, we need to bottom out first. There are three
        // bottom out states we can run into: 1) We see 'super' which must start either of
        // the last two CallExpression productions. 2) We see 'import' which must start import call.
        // 3)we have a MemberExpression which either completes the LeftHandSideExpression,
        // or starts the beginning of the first four CallExpression productions.
        let pos = self.node_pos();
        let expression = if self.token() == Kind::ImportKeyword {
            self.parse_import_expression(pos)
        } else if self.token() == Kind::SuperKeyword {
            self.parse_super_expression()
        } else {
            self.parse_member_expression_or_higher()
        };
        // Now, we *may* be complete.  However, we might have consumed the start of a
        // CallExpression or OptionalExpression.  As such, we need to consume the rest
        // of it here to be complete.
        self.parse_call_expression_rest(pos, expression)
    }

    #[inline(never)]
    fn parse_import_expression(&mut self, pos: u32) -> Self::Node {
        let expression: Self::Node;
        if self.look_ahead(|parser| parser.next_token_is_open_paren_or_less_than()) {
            // We don't want to eagerly consume all import keyword as import call expression so we look ahead to find "("
            // For example:
            //      var foo3 = require("subfolder
            //      import * as foo1 from "module-from-node
            // We want this import to be a statement rather than import call expression
            self.add_source_flags(NodeFlags::PossiblyContainsDynamicImport);
            expression = self.parse_keyword_expression();
        } else if self.look_ahead(|parser| parser.next_token_is_dot()) {
            // This is an 'import.*' metaproperty (i.e. 'import.meta')
            self.next_token(); // advance past the 'import'
            self.next_token(); // advance past the dot
            let name = self.parse_import_meta_property_name();
            let node = self.new_meta_property(Kind::ImportKeyword, name);
            expression = self.finish_node(node, pos);
            if self.node_is_import_phase_meta_property(expression) {
                if self.token() == Kind::OpenParenToken || self.token() == Kind::LessThanToken {
                    self.add_source_flags(NodeFlags::PossiblyContainsDynamicImport);
                }
            } else {
                self.add_source_flags(NodeFlags::PossiblyContainsImportMeta);
            }
        } else {
            expression = self.parse_member_expression_or_higher();
        }
        expression
    }

    fn parse_import_meta_property_name(&mut self) -> Self::Node {
        if matches!(self.token(), Kind::DeferKeyword | Kind::SourceKeyword)
            && self.current_import_phase_modifier() == Kind::Unknown
        {
            self.parse_error_at_current_token(
                &diagnostics::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS,
                &[],
            );
        }
        self.parse_identifier_name()
    }

    fn parse_super_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let mut expression = self.parse_keyword_expression();
        if self.token() == Kind::LessThanToken {
            let start_pos = self.node_pos();
            let type_arguments = self.try_parse_type_arguments_in_expression();
            if type_arguments.is_some() {
                let end = self.node_pos();
                self.error_at(
                    TextRange::new(start_pos, end),
                    &diagnostics::X_SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                    &[],
                );
                if !self.is_template_start_of_tagged_template() {
                    let node = self.new_expression_with_type_arguments(expression, type_arguments);
                    expression = self.finish_node(node, pos);
                }
            }
        }
        if self.token() == Kind::OpenParenToken
            || self.token() == Kind::DotToken
            || self.token() == Kind::OpenBracketToken
        {
            return expression;
        }
        // If we have seen "super" it must be followed by '(' or '.'.
        // If it wasn't then just try to parse out a '.' and report an error.
        self.parse_error_at_current_token(
            &diagnostics::X_SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
            &[],
        );
        // private names will never work with `super` (`super.#foo`), but that's a semantic error, not syntactic
        let name = self.parse_right_side_of_dot(
            true, /*allowIdentifierNames*/
            true, /*allowPrivateIdentifiers*/
            true, /*allowUnicodeEscapeSequenceInIdentifierName*/
        );
        let node = self.new_property_access_expression(
            expression,
            None, /*questionDotToken*/
            name,
            NodeFlags::None,
        );
        self.finish_node(node, pos)
    }

    #[inline]
    fn try_parse_type_arguments_in_expression(&mut self) -> Option<Self::List> {
        // TypeArguments must not be parsed in JavaScript files to avoid ambiguity with binary operators.
        // Check the cheap preconditions before saving the parser state: unless the current token is `<`
        // (or `<<`, which reScanLessThanToken would split), there is nothing to speculatively parse and
        // the mark/rewind would be a no-op.
        if self.context_flags().intersects(NodeFlags::JavaScriptFile)
            || (self.token() != Kind::LessThanToken && self.token() != Kind::LessThanLessThanToken)
        {
            return None;
        }
        self.try_parse_type_arguments_in_expression_worker()
    }

    fn try_parse_type_arguments_in_expression_worker(&mut self) -> Option<Self::List> {
        let state = self.mark();
        if self.re_scan_less_than_token() == Kind::LessThanToken {
            self.next_token();
            let type_arguments =
                self.parse_delimited_list(ParsingContext::TypeArguments, Self::parse_type);
            // If it doesn't have the closing `>` then it's definitely not an type argument list.
            if self.re_scan_greater_than_token() == Kind::GreaterThanToken {
                self.next_token();
                // We successfully parsed a type argument list. The next token determines whether we want to
                // treat it as such. If the type argument list is followed by `(` or a template literal, as in
                // `f<number>(42)`, we favor the type argument interpretation even though JavaScript would view
                // it as a relational expression.
                if self.can_follow_type_arguments_in_expression() {
                    return type_arguments;
                }
            }
        }
        self.rewind(state);
        None
    }

    fn parse_member_expression_or_higher(&mut self) -> Self::Node {
        // Note: to make our lives simpler, we decompose the NewExpression productions and
        // place ObjectCreationExpression and FunctionExpression into PrimaryExpression.
        // like so:
        //
        //   PrimaryExpression : See 11.1
        //      this
        //      Identifier
        //      Literal
        //      ArrayLiteral
        //      ObjectLiteral
        //      (Expression)
        //      FunctionExpression
        //      new MemberExpression Arguments?
        //
        //   MemberExpression : See 11.2
        //      PrimaryExpression
        //      MemberExpression[Expression]
        //      MemberExpression.IdentifierName
        //
        //   CallExpression : See 11.2
        //      MemberExpression
        //      CallExpression Arguments
        //      CallExpression[Expression]
        //      CallExpression.IdentifierName
        //
        // Technically this is ambiguous.  i.e. CallExpression defines:
        //
        //   CallExpression:
        //      CallExpression Arguments
        //
        // If you see: "new Foo()"
        //
        // Then that could be treated as a single ObjectCreationExpression, or it could be
        // treated as the invocation of "new Foo".  We disambiguate that in code (to match
        // the original grammar) by making sure that if we see an ObjectCreationExpression
        // we always consume arguments if they are there. So we treat "new Foo()" as an
        // object creation only, and not at all as an invocation.  Another way to think
        // about this is that for every "new" that we see, we will consume an argument list if
        // it is there as part of the *associated* object creation node.  Any additional
        // argument lists we see, will become invocation expressions.
        //
        // Because there are no other places in the grammar now that refer to FunctionExpression
        // or ObjectCreationExpression, it is safe to push down into the PrimaryExpression
        // production.
        //
        // Because CallExpression and MemberExpression are left recursive, we need to bottom out
        // of the recursion immediately.  So we parse out a primary expression to start with.
        let pos = self.node_pos();
        let expression = self.parse_primary_expression();
        self.parse_member_expression_rest(pos, expression, true /*allowOptionalChain*/)
    }

    #[inline]
    fn parse_member_expression_rest(
        &mut self,
        pos: u32,
        expression: Self::Node,
        allow_optional_chain: bool,
    ) -> Self::Node {
        // Every other token leaves the expression unchanged without scanning or allocating. Keep that
        // return outside the worker's frame, which also contains node construction and error recovery.
        if !self.is_start_of_member_expression_rest() {
            return expression;
        }
        self.parse_member_expression_rest_worker(pos, expression, allow_optional_chain)
    }

    #[inline(never)]
    fn parse_member_expression_rest_worker(
        &mut self,
        pos: u32,
        expression: Self::Node,
        allow_optional_chain: bool,
    ) -> Self::Node {
        let mut expression = expression;
        loop {
            let mut question_dot_token: Option<Self::Node> = None;
            let is_property_access;
            if allow_optional_chain && self.is_start_of_optional_property_or_element_access_chain()
            {
                question_dot_token = Some(self.parse_expected_token(Kind::QuestionDotToken));
                is_property_access = token_is_identifier_or_keyword(self.token());
            } else {
                is_property_access = self.parse_optional(Kind::DotToken);
            }
            if is_property_access {
                expression =
                    self.parse_property_access_expression_rest(pos, expression, question_dot_token);
                continue;
            }
            // when in the [Decorator] context, we do not parse ElementAccess as it could be part of a ComputedPropertyName
            if (question_dot_token.is_some() || !self.in_decorator_context())
                && self.parse_optional(Kind::OpenBracketToken)
            {
                expression =
                    self.parse_element_access_expression_rest(pos, expression, question_dot_token);
                continue;
            }
            if self.is_template_start_of_tagged_template() {
                // Absorb type arguments into TemplateExpression when preceding expression is ExpressionWithTypeArguments
                if question_dot_token.is_none()
                    && self.node_kind(expression) == Kind::ExpressionWithTypeArguments
                {
                    let (original_expression, original_type_arguments) =
                        self.expression_with_type_arguments_parts(expression);
                    expression = self.parse_tagged_template_rest(
                        pos,
                        original_expression,
                        question_dot_token,
                        original_type_arguments,
                    );
                    self.unparse_expression_with_type_arguments(
                        Some(original_expression),
                        original_type_arguments,
                        expression,
                    );
                } else {
                    expression = self.parse_tagged_template_rest(
                        pos,
                        expression,
                        question_dot_token,
                        None, /*typeArguments*/
                    );
                }
                continue;
            }
            if question_dot_token.is_none() {
                if self.token() == Kind::ExclamationToken && !self.has_preceding_line_break() {
                    self.next_token();
                    let node = self.new_non_null_expression(expression, NodeFlags::None);
                    let node = self.finish_node(node, pos);
                    expression = self.check_js_syntax(node);
                    continue;
                }
                let type_arguments = self.try_parse_type_arguments_in_expression();
                if type_arguments.is_some() {
                    let node = self.new_expression_with_type_arguments(expression, type_arguments);
                    expression = self.finish_node(node, pos);
                    continue;
                }
            }
            return expression;
        }
    }

    fn parse_property_access_expression_rest(
        &mut self,
        pos: u32,
        expression: Self::Node,
        question_dot_token: Option<Self::Node>,
    ) -> Self::Node {
        let name = self.parse_right_side_of_dot(
            true, /*allowIdentifierNames*/
            true, /*allowPrivateIdentifiers*/
            true, /*allowUnicodeEscapeSequenceInIdentifierName*/
        );
        let is_optional_chain =
            question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
        let property_access = self.new_property_access_expression(
            expression,
            question_dot_token,
            name,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::None },
        );
        if is_optional_chain && self.node_kind(name) == Kind::PrivateIdentifier {
            let loc = self.skip_expression_range_trivia(self.node_range(name));
            self.error_at(
                loc,
                &diagnostics::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                &[],
            );
        }
        if self.node_kind(expression) == Kind::ExpressionWithTypeArguments
            && let Some(type_arguments) = self.node_type_arguments(expression)
        {
            let loc = TextRange::new(
                self.list_range(type_arguments)
                    .pos()
                    .expect_source("parsed lists have source positions")
                    - 1,
                self.skip_expression_trivia(
                    self.list_range(type_arguments)
                        .end()
                        .expect_source("parsed lists have source positions"),
                ) + 1,
            );
            self.error_at(
                loc,
                &diagnostics::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                &[],
            );
        }
        self.finish_node(property_access, pos)
    }

    #[inline]
    fn try_reparse_optional_chain(&mut self, node: Self::Node) -> bool {
        if self.node_flags(node).intersects(NodeFlags::OptionalChain) {
            return true;
        }
        // check for an optional chain in a non-null expression
        self.node_kind(node) == Kind::NonNullExpression
            && self.try_reparse_optional_chain_in_non_null_expression(node)
    }

    fn try_reparse_optional_chain_in_non_null_expression(&mut self, node: Self::Node) -> bool {
        {
            let mut expr = self.node_expression(node).unwrap();
            while self.node_kind(expr) == Kind::NonNullExpression
                && !self.node_flags(expr).intersects(NodeFlags::OptionalChain)
            {
                expr = self.node_expression(expr).unwrap();
            }
            if self.node_flags(expr).intersects(NodeFlags::OptionalChain) {
                // this is part of an optional chain. Walk down from `node` to `expression` and set the flag.
                let mut node = node;
                while self.node_kind(node) == Kind::NonNullExpression {
                    self.set_node_flags(node, self.node_flags(node) | NodeFlags::OptionalChain);
                    node = self.node_expression(node).unwrap();
                }
                return true;
            }
        }
        false
    }

    fn parse_element_access_expression_rest(
        &mut self,
        pos: u32,
        expression: Self::Node,
        question_dot_token: Option<Self::Node>,
    ) -> Self::Node {
        let mut argument_expression = self.create_missing_identifier();
        if self.token() == Kind::CloseBracketToken {
            let node_pos = self.node_pos();
            self.error_at(
                TextRange::new(node_pos, node_pos),
                &diagnostics::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                &[],
            );
        } else {
            argument_expression = self.parse_expression_allow_in();
        }
        self.parse_expected(Kind::CloseBracketToken);
        let is_optional_chain =
            question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
        let node = self.new_element_access_expression(
            expression,
            question_dot_token,
            argument_expression,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::None },
        );
        self.finish_node(node, pos)
    }

    #[inline]
    fn parse_call_expression_rest(&mut self, pos: u32, expression: Self::Node) -> Self::Node {
        if self.token() != Kind::OpenParenToken && !self.is_start_of_member_expression_rest() {
            return expression;
        }
        self.parse_call_expression_rest_worker(pos, expression)
    }

    #[inline(never)]
    fn parse_call_expression_rest_worker(
        &mut self,
        pos: u32,
        expression: Self::Node,
    ) -> Self::Node {
        let mut expression = expression;
        loop {
            expression = self
                .parse_member_expression_rest(pos, expression, true /*allowOptionalChain*/);
            let mut type_arguments: Option<Self::List> = None;
            let question_dot_token = self.parse_optional_token(Kind::QuestionDotToken);
            if question_dot_token.is_some() {
                type_arguments = self.try_parse_type_arguments_in_expression();
                if self.is_template_start_of_tagged_template() {
                    expression = self.parse_tagged_template_rest(
                        pos,
                        expression,
                        question_dot_token,
                        type_arguments,
                    );
                    continue;
                }
            }
            if type_arguments.is_some() || self.token() == Kind::OpenParenToken {
                // Absorb type arguments into CallExpression when preceding expression is ExpressionWithTypeArguments
                if question_dot_token.is_none()
                    && self.node_kind(expression) == Kind::ExpressionWithTypeArguments
                {
                    type_arguments = self.node_type_arguments(expression);
                    expression = self.node_expression(expression).unwrap();
                }
                let inner = expression;
                let argument_list = self.parse_argument_list();
                let is_optional_chain =
                    question_dot_token.is_some() || self.try_reparse_optional_chain(expression);
                let node = self.new_call_expression(
                    expression,
                    question_dot_token,
                    type_arguments,
                    argument_list,
                    if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::None },
                );
                let node = self.finish_node(node, pos);
                expression = self.check_js_syntax(node);
                self.unparse_expression_with_type_arguments(
                    Some(inner),
                    type_arguments,
                    expression,
                );
                continue;
            }
            if question_dot_token.is_some() {
                // We parsed `?.` but then failed to parse anything, so report a missing identifier here.
                self.parse_error_at_current_token(&diagnostics::IDENTIFIER_EXPECTED, &[]);
                let name = self.create_missing_identifier();
                let node = self.new_property_access_expression(
                    expression,
                    question_dot_token,
                    name,
                    NodeFlags::OptionalChain,
                );
                expression = self.finish_node(node, pos);
            }
            break;
        }
        expression
    }

    fn parse_argument_list(&mut self) -> Self::List {
        self.parse_expected(Kind::OpenParenToken);
        let result = self
            .parse_delimited_list(
                ParsingContext::ArgumentExpressions,
                Self::parse_argument_expression,
            )
            .unwrap();
        self.parse_expected(Kind::CloseParenToken);
        result
    }

    fn parse_argument_expression(&mut self) -> Self::Node {
        self.do_in_context(
            NodeFlags::DisallowInContext | NodeFlags::DecoratorContext,
            false,
            Self::parse_argument_or_array_literal_element,
        )
    }

    fn parse_argument_or_array_literal_element(&mut self) -> Self::Node {
        match self.token() {
            Kind::DotDotDotToken => return self.parse_spread_element(),
            Kind::CommaToken => {
                let node = self.new_omitted_expression();
                let pos = self.node_pos();
                return self.finish_node(node, pos);
            }
            _ => {}
        }
        self.parse_assignment_expression_or_higher()
    }

    fn parse_spread_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::DotDotDotToken);
        let expression = self.parse_assignment_expression_or_higher();
        let node = self.new_spread_element(expression);
        self.finish_node(node, pos)
    }

    fn parse_tagged_template_rest(
        &mut self,
        pos: u32,
        tag: Self::Node,
        question_dot_token: Option<Self::Node>,
        type_arguments: Option<Self::List>,
    ) -> Self::Node {
        let template: Self::Node = if self.token() == Kind::NoSubstitutionTemplateLiteral {
            self.re_scan_template_token(true /*isTaggedTemplate*/);
            self.parse_literal_expression()
        } else {
            self.parse_template_expression(true /*isTaggedTemplate*/)
        };
        let is_optional_chain = question_dot_token.is_some()
            || self.node_flags(tag).intersects(NodeFlags::OptionalChain);
        let node = self.new_tagged_template_expression(
            tag,
            question_dot_token,
            type_arguments,
            template,
            if is_optional_chain { NodeFlags::OptionalChain } else { NodeFlags::None },
        );
        let node = self.finish_node(node, pos);
        self.check_js_syntax(node)
    }

    fn parse_template_expression(&mut self, is_tagged_template: bool) -> Self::Node {
        let pos = self.node_pos();
        let head = self.parse_template_head(is_tagged_template);
        let spans = self.parse_template_spans(is_tagged_template);
        let node = self.new_template_expression(head, spans);
        self.finish_node(node, pos)
    }

    fn parse_template_spans(&mut self, is_tagged_template: bool) -> Self::List {
        let pos = self.node_pos();
        let start = self.node_stack().len();
        loop {
            let span = self.parse_template_span(is_tagged_template);
            self.node_stack_mut().push(span);
            if self.node_kind(self.template_span_literal(span)) != Kind::TemplateMiddle {
                break;
            }
        }
        let end = self.node_pos();
        self.finish_node_list(start, TextRange::new(pos, end))
    }

    fn parse_template_span(&mut self, is_tagged_template: bool) -> Self::Node {
        let pos = self.node_pos();
        let expression = self.parse_expression_allow_in();
        let literal = self.parse_literal_of_template_span(is_tagged_template);
        let node = self.new_template_span(expression, literal);
        self.finish_node(node, pos)
    }

    #[inline]
    fn parse_primary_expression(&mut self) -> Self::Node {
        if self.token() == Kind::Identifier {
            return self
                .parse_identifier_with_diagnostic(Some(&diagnostics::EXPRESSION_EXPECTED), None);
        }
        self.parse_primary_expression_worker()
    }

    #[inline(never)]
    fn parse_primary_expression_worker(&mut self) -> Self::Node {
        let token = self.token();
        match token {
            Kind::NoSubstitutionTemplateLiteral
            | Kind::NumericLiteral
            | Kind::BigIntLiteral
            | Kind::StringLiteral => {
                if token == Kind::NoSubstitutionTemplateLiteral
                    && self.scanner().token_flags().intersects(TokenFlags::IsInvalid)
                {
                    self.re_scan_template_token(false /*isTaggedTemplate*/);
                }
                return self.parse_literal_expression();
            }
            Kind::ThisKeyword
            | Kind::SuperKeyword
            | Kind::NullKeyword
            | Kind::TrueKeyword
            | Kind::FalseKeyword => {
                return self.parse_keyword_expression();
            }
            Kind::OpenParenToken => return self.parse_parenthesized_expression(),
            Kind::OpenBracketToken => return self.parse_array_literal_expression(),
            Kind::OpenBraceToken => return self.parse_object_literal_expression(),
            Kind::AsyncKeyword => {
                // Async arrow functions are parsed earlier in parseAssignmentExpressionOrHigher.
                // If we encounter `async [no LineTerminator here] function` then this is an async
                // function; otherwise, its an identifier.
                if self.look_ahead(|parser| parser.next_token_is_function_keyword_on_same_line()) {
                    return self.parse_function_expression();
                }
            }
            Kind::AtToken => return self.parse_decorated_expression(),
            Kind::ClassKeyword => return self.parse_class_expression(),
            Kind::FunctionKeyword => return self.parse_function_expression(),
            Kind::NewKeyword => return self.parse_new_expression_or_new_dot_target(),
            Kind::SlashToken | Kind::SlashEqualsToken => {
                if self.re_scan_slash_token() == Kind::RegularExpressionLiteral {
                    return self.parse_literal_expression();
                }
            }
            Kind::TemplateHead => {
                return self.parse_template_expression(false /*isTaggedTemplate*/);
            }
            Kind::PrivateIdentifier => return self.parse_private_identifier(),
            _ => {}
        }
        self.parse_identifier_with_diagnostic(Some(&diagnostics::EXPRESSION_EXPECTED), None)
    }

    fn parse_parenthesized_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(Kind::CloseParenToken);
        let node = self.new_parenthesized_expression(expression);
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_array_literal_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let open_bracket_position = self.scanner().token_start();
        let open_bracket_parsed = self.parse_expected(Kind::OpenBracketToken);
        let multi_line = self.has_preceding_line_break();
        let elements = self
            .parse_delimited_list(
                ParsingContext::ArrayLiteralMembers,
                Self::parse_argument_or_array_literal_element,
            )
            .unwrap();
        self.parse_expected_matching_brackets(
            Kind::OpenBracketToken,
            Kind::CloseBracketToken,
            open_bracket_parsed,
            open_bracket_position,
        );
        let node = self.new_array_literal_expression(elements, multi_line);
        self.finish_node(node, pos)
    }

    fn parse_object_literal_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let open_brace_position = self.scanner().token_start();
        let open_brace_parsed = self.parse_expected(Kind::OpenBraceToken);
        let multi_line = self.has_preceding_line_break();
        let properties = self
            .parse_delimited_list(
                ParsingContext::ObjectLiteralMembers,
                Self::parse_object_literal_element,
            )
            .unwrap();
        self.parse_expected_matching_brackets(
            Kind::OpenBraceToken,
            Kind::CloseBraceToken,
            open_brace_parsed,
            open_brace_position,
        );
        let node = self.new_object_literal_expression(properties, multi_line);
        self.finish_node(node, pos)
    }

    fn parse_object_literal_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if self.parse_optional(Kind::DotDotDotToken) {
            let expression = self.parse_assignment_expression_or_higher();
            let node = self.new_spread_assignment(expression);
            let result = self.finish_node(node, pos);
            self.with_jsdoc(result, jsdoc);
            return result;
        }
        let modifiers = self.parse_modifiers_ex(
            true,  /*allowDecorators*/
            false, /*permitConstAsModifier*/
            false, /*stopOnStartOfClassStaticBlock*/
        );
        if self.parse_contextual_modifier(Kind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::GetAccessor,
                ParseFlags::None,
            );
        }
        if self.parse_contextual_modifier(Kind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::SetAccessor,
                ParseFlags::None,
            );
        }
        let asterisk_token = self.parse_optional_token(Kind::AsteriskToken);
        let token_is_identifier = self.is_identifier();
        let name = self.parse_property_name();
        // Disallowing of optional property assignments and definite assignment assertion happens in the grammar checker.
        let mut postfix_token = self.parse_optional_token(Kind::QuestionToken);
        // Decorators, Modifiers, questionToken, and exclamationToken are not supported by property assignments and are reported in the grammar checker
        if postfix_token.is_none() {
            postfix_token = self.parse_optional_token(Kind::ExclamationToken);
        }
        if asterisk_token.is_some()
            || self.token() == Kind::OpenParenToken
            || self.token() == Kind::LessThanToken
        {
            return self.parse_method_declaration(
                pos,
                jsdoc,
                modifiers,
                asterisk_token,
                name,
                postfix_token,
                None, /*diagnosticMessage*/
            );
        }
        // check if it is short-hand property assignment or normal property assignment
        // NOTE: if token is EqualsToken it is interpreted as CoverInitializedName production
        // CoverInitializedName[Yield] :
        //     IdentifierReference[?Yield] Initializer[In, ?Yield]
        // this is necessary because ObjectLiteral productions are also used to cover grammar for ObjectAssignmentPattern

        let is_shorthand_property_assignment =
            token_is_identifier && self.token() != Kind::ColonToken;
        let node: Self::Node = if is_shorthand_property_assignment {
            let equals_token = self.parse_optional_token(Kind::EqualsToken);
            let mut initializer: Option<Self::Node> = None;
            if equals_token.is_some() {
                initializer = Some(self.do_in_context(
                    NodeFlags::DisallowInContext,
                    false,
                    Self::parse_assignment_expression_or_higher,
                ));
            }
            self.new_shorthand_property_assignment(
                modifiers,
                name,
                postfix_token,
                None, /*typeNode*/
                equals_token,
                initializer,
            )
        } else {
            self.parse_expected(Kind::ColonToken);
            let initializer = self.do_in_context(
                NodeFlags::DisallowInContext,
                false,
                Self::parse_assignment_expression_or_higher,
            );
            self.new_property_assignment(
                modifiers,
                name,
                postfix_token,
                None, /*typeNode*/
                initializer,
            )
        };
        self.finish_node(node, pos);
        self.with_jsdoc(node, jsdoc);
        node
    }

    fn parse_function_expression(&mut self) -> Self::Node {
        // GeneratorExpression:
        //      function* BindingIdentifier [Yield][opt](FormalParameters[Yield]){ GeneratorBody }
        //
        // FunctionExpression:
        //      function BindingIdentifier[opt](FormalParameters){ FunctionBody }
        let save_contex_flags = self.context_flags();
        self.set_context_flags(NodeFlags::DecoratorContext, false);
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers();
        self.parse_expected(Kind::FunctionKeyword);
        let asterisk_token = self.parse_optional_token(Kind::AsteriskToken);
        let is_generator = asterisk_token.is_some();
        let is_async = self.modifier_list_has_async(modifiers);
        let signature_flags = (if is_generator { ParseFlags::Yield } else { ParseFlags::None })
            | (if is_async { ParseFlags::Await } else { ParseFlags::None });
        let name = if is_generator && is_async {
            self.do_in_context(
                NodeFlags::YieldContext | NodeFlags::AwaitContext,
                true,
                Self::parse_optional_binding_identifier,
            )
        } else if is_generator {
            self.do_in_context(
                NodeFlags::YieldContext,
                true,
                Self::parse_optional_binding_identifier,
            )
        } else if is_async {
            self.do_in_context(
                NodeFlags::AwaitContext,
                true,
                Self::parse_optional_binding_identifier,
            )
        } else {
            self.parse_optional_binding_identifier()
        };
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(signature_flags);
        let return_type = self.parse_return_type(Kind::ColonToken, false /*isType*/);
        let body = self.parse_function_block(signature_flags, None /*diagnosticMessage*/);
        self.replace_context_flags(save_contex_flags);
        let result = self.new_function_expression(
            modifiers,
            asterisk_token,
            name,
            type_parameters,
            Some(parameters),
            return_type,
            None, /*fullSignature*/
            Some(body),
        );
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_decorated_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_ex(
            true,  /*allowDecorators*/
            false, /*permitConstAsModifier*/
            false, /*stopOnStartOfClassStaticBlock*/
        );
        if self.token() == Kind::ClassKeyword {
            return self.parse_class_declaration_or_expression(
                pos,
                jsdoc,
                modifiers,
                Kind::ClassExpression,
            );
        }
        let node_pos = self.node_pos();
        self.error_at(TextRange::new(node_pos, node_pos), &diagnostics::EXPRESSION_EXPECTED, &[]);
        let node = self.new_missing_declaration(modifiers);
        self.finish_node(node, pos)
    }

    fn unparse_expression_with_type_arguments(
        &mut self,
        expression: Option<Self::Node>,
        type_arguments: Option<Self::List>,
        result: Self::Node,
    ) {
        // force overwrite the `.Parent` of the expression and type arguments to erase the fact that they may have originally been parsed as an ExpressionWithTypeArguments and be parented to such
        if let Some(expression) = expression {
            self.set_node_parent(expression, Some(result));
        }
        if let Some(type_arguments) = type_arguments {
            for index in 0..self.list_len(type_arguments) {
                let argument = self.list_element(type_arguments, index);
                self.set_node_parent(argument, Some(result));
            }
        }
    }

    fn parse_new_expression_or_new_dot_target(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::NewKeyword);
        if self.parse_optional(Kind::DotToken) {
            let name = self.parse_identifier_name();
            let node = self.new_meta_property(Kind::NewKeyword, name);
            return self.finish_node(node, pos);
        }
        let expression_pos = self.node_pos();
        let primary = self.parse_primary_expression();
        let mut expression = self.parse_member_expression_rest(
            expression_pos,
            primary,
            false, /*allowOptionalChain*/
        );
        let mut type_arguments: Option<Self::List> = None;
        // Absorb type arguments into NewExpression when preceding expression is ExpressionWithTypeArguments
        if self.node_kind(expression) == Kind::ExpressionWithTypeArguments {
            type_arguments = self.node_type_arguments(expression);
            expression = self.node_expression(expression).unwrap();
        }
        if self.token() == Kind::QuestionDotToken {
            let text = self.expression_source_text(expression);
            self.parse_error_at_current_token(
                &diagnostics::INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL_0,
                &[&text],
            );
        }
        let mut argument_list: Option<Self::List> = None;
        if self.token() == Kind::OpenParenToken {
            argument_list = Some(self.parse_argument_list());
        }
        let node = self.new_new_expression(expression, type_arguments, argument_list);
        let node = self.finish_node(node, pos);
        let result = self.check_js_syntax(node);
        self.unparse_expression_with_type_arguments(Some(expression), type_arguments, result);
        result
    }

    fn parse_class_expression(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_class_declaration_or_expression(
            pos,
            jsdoc,
            None, /*modifiers*/
            Kind::ClassExpression,
        )
    }

    fn parse_method_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        asterisk_token: Option<Self::Node>,
        name: Self::Node,
        question_token: Option<Self::Node>,
        diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let signature_flags =
            (if asterisk_token.is_some() { ParseFlags::Yield } else { ParseFlags::None })
                | (if self.modifier_list_has_async(modifiers) {
                    ParseFlags::Await
                } else {
                    ParseFlags::None
                });
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters(signature_flags);
        let type_node = self.parse_return_type(Kind::ColonToken, false /*isType*/);
        let body = self.parse_function_block_or_semicolon(signature_flags, diagnostic_message);
        let result = self.new_method_declaration(
            modifiers,
            asterisk_token,
            name,
            question_token,
            type_parameters,
            Some(parameters),
            type_node,
            None, /*fullSignature*/
            body,
        );
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }
    // Preserve the parser's rejection of incomplete arrow return types, including nested signatures.
    fn type_has_arrow_function_blocking_parse_error(&self, node: Self::Node) -> bool {
        match self.node_kind(node) {
            Kind::TypeReference => self.node_is_missing(self.type_reference_name(node)),
            Kind::FunctionType | Kind::ConstructorType => {
                self.list_is_missing(self.function_parameters(node))
                    || self
                        .type_has_arrow_function_blocking_parse_error(self.node_type(node).unwrap())
            }
            Kind::ParenthesizedType => {
                self.type_has_arrow_function_blocking_parse_error(self.node_type(node).unwrap())
            }
            _ => false,
        }
    }
    fn parse_statement(&mut self) -> Self::Node {
        let token = self.token();
        match token {
            Kind::SemicolonToken => return self.parse_empty_statement(),
            Kind::OpenBraceToken => {
                return self.parse_block(false /*ignoreMissingOpenBrace*/, None);
            }
            Kind::VarKeyword => {
                let pos = self.node_pos();
                let jsdoc = self.jsdoc_scanner_info();
                return self.parse_variable_statement(pos, jsdoc, None /*modifiers*/);
            }
            Kind::LetKeyword => {
                if self.is_let_declaration() {
                    let pos = self.node_pos();
                    let jsdoc = self.jsdoc_scanner_info();
                    return self.parse_variable_statement(pos, jsdoc, None /*modifiers*/);
                }
            }
            Kind::AwaitKeyword => {
                if self.is_await_using_declaration() {
                    let pos = self.node_pos();
                    let jsdoc = self.jsdoc_scanner_info();
                    return self.parse_variable_statement(pos, jsdoc, None /*modifiers*/);
                }
            }
            Kind::UsingKeyword => {
                if self.is_using_declaration() {
                    let pos = self.node_pos();
                    let jsdoc = self.jsdoc_scanner_info();
                    return self.parse_variable_statement(pos, jsdoc, None /*modifiers*/);
                }
            }
            Kind::FunctionKeyword => {
                let pos = self.node_pos();
                let jsdoc = self.jsdoc_scanner_info();
                return self.parse_function_declaration(pos, jsdoc, None /*modifiers*/);
            }
            Kind::ClassKeyword => {
                let pos = self.node_pos();
                let jsdoc = self.jsdoc_scanner_info();
                return self.parse_class_declaration(pos, jsdoc, None /*modifiers*/);
            }
            Kind::IfKeyword => return self.parse_if_statement(),
            Kind::DoKeyword => return self.parse_do_statement(),
            Kind::WhileKeyword => return self.parse_while_statement(),
            Kind::ForKeyword => return self.parse_for_or_for_in_or_for_of_statement(),
            Kind::ContinueKeyword => return self.parse_continue_statement(),
            Kind::BreakKeyword => return self.parse_break_statement(),
            Kind::ReturnKeyword => return self.parse_return_statement(),
            Kind::WithKeyword => return self.parse_with_statement(),
            Kind::SwitchKeyword => return self.parse_switch_statement(),
            Kind::ThrowKeyword => return self.parse_throw_statement(),
            Kind::TryKeyword | Kind::CatchKeyword | Kind::FinallyKeyword => {
                return self.parse_try_statement();
            }
            Kind::DebuggerKeyword => return self.parse_debugger_statement(),
            Kind::AtToken => return self.parse_declaration(),
            Kind::AsyncKeyword
            | Kind::InterfaceKeyword
            | Kind::TypeKeyword
            | Kind::ModuleKeyword
            | Kind::NamespaceKeyword
            | Kind::DeclareKeyword
            | Kind::ConstKeyword
            | Kind::EnumKeyword
            | Kind::ExportKeyword
            | Kind::ImportKeyword
            | Kind::PrivateKeyword
            | Kind::ProtectedKeyword
            | Kind::PublicKeyword
            | Kind::AbstractKeyword
            | Kind::AccessorKeyword
            | Kind::StaticKeyword
            | Kind::ReadonlyKeyword
            | Kind::GlobalKeyword
                if self.is_start_of_declaration() =>
            {
                return self.parse_declaration();
            }
            _ => {}
        }
        self.parse_expression_or_labeled_statement()
    }

    fn parse_declaration(&mut self) -> Self::Node {
        // `parseListElement` attempted to get the reused node at this position,
        // but the ambient context flag was not yet set, so the node appeared
        // not reusable in that context.
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let modifiers = self.parse_modifiers_ex(
            true,  /*allowDecorators*/
            false, /*permitConstAsModifier*/
            false, /*stopOnStartOfClassStaticBlock*/
        );
        let is_ambient = modifiers
            .is_some_and(|modifiers| self.modifier_list_has_kind(modifiers, Kind::DeclareKeyword));
        if is_ambient {
            // !!! incremental parsing
            // node := p.tryReuseAmbientDeclaration(pos)
            // if node {
            // 	return node
            // }
            self.set_modifiers_ambient(modifiers.unwrap());
            let save_context_flags = self.context_flags();
            self.set_context_flags(NodeFlags::Ambient, true);
            let result = self.parse_declaration_worker(pos, jsdoc, modifiers);
            self.replace_context_flags(save_context_flags);
            result
        } else {
            self.parse_declaration_worker(pos, jsdoc, modifiers)
        }
    }

    fn parse_declaration_worker(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        match self.token() {
            Kind::VarKeyword | Kind::LetKeyword | Kind::ConstKeyword | Kind::UsingKeyword => {
                return self.parse_variable_statement(pos, jsdoc, modifiers);
            }
            Kind::AwaitKeyword => {
                if self.is_await_using_declaration() {
                    return self.parse_variable_statement(pos, jsdoc, modifiers);
                }
            }
            Kind::FunctionKeyword => return self.parse_function_declaration(pos, jsdoc, modifiers),
            Kind::ClassKeyword => return self.parse_class_declaration(pos, jsdoc, modifiers),
            Kind::InterfaceKeyword => {
                return self.parse_interface_declaration(pos, jsdoc, modifiers);
            }
            Kind::TypeKeyword => return self.parse_type_alias_declaration(pos, jsdoc, modifiers),
            Kind::EnumKeyword => return self.parse_enum_declaration(pos, jsdoc, modifiers),
            Kind::GlobalKeyword | Kind::ModuleKeyword | Kind::NamespaceKeyword => {
                return self.parse_module_declaration(pos, jsdoc, modifiers);
            }
            Kind::ImportKeyword => {
                return self
                    .parse_import_declaration_or_import_equals_declaration(pos, jsdoc, modifiers);
            }
            Kind::ExportKeyword => {
                self.next_token();
                return match self.token() {
                    Kind::DefaultKeyword | Kind::EqualsToken => {
                        self.parse_export_assignment(pos, jsdoc, modifiers)
                    }
                    Kind::AsKeyword => {
                        self.parse_namespace_export_declaration(pos, jsdoc, modifiers)
                    }
                    _ => self.parse_export_declaration(pos, jsdoc, modifiers),
                };
            }
            _ => {}
        }
        if modifiers.is_some() {
            // We reached this point because we encountered decorators and/or modifiers and assumed a declaration
            // would follow. For recovery and error reporting purposes, return an incomplete declaration.
            let node_pos = self.node_pos();
            self.error_at(
                TextRange::new(node_pos, node_pos),
                &diagnostics::DECLARATION_EXPECTED,
                &[],
            );
            let result = self.new_missing_declaration(modifiers);
            return self.finish_node(result, pos);
        }
        panic!("Unhandled case in parseDeclarationWorker");
    }

    fn parse_block(
        &mut self,
        ignore_missing_open_brace: bool,
        diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let open_brace_position = self.scanner().token_start();
        let open_brace_parsed = self.parse_expected_with_diagnostic(
            Kind::OpenBraceToken,
            diagnostic_message,
            true, /*shouldAdvance*/
        );
        let mut multiline = false;
        if open_brace_parsed || ignore_missing_open_brace {
            multiline = self.has_preceding_line_break();
            let statements =
                self.parse_list(ParsingContext::BlockStatements, Self::parse_statement);
            self.parse_expected_matching_brackets(
                Kind::OpenBraceToken,
                Kind::CloseBraceToken,
                open_brace_parsed,
                open_brace_position,
            );
            let result = self.new_block(statements, multiline);
            let result = self.finish_node(result, pos);
            self.with_jsdoc(result, jsdoc);
            if self.token() == Kind::EqualsToken {
                self.parse_error_at_current_token(&diagnostics::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_INTENDED_TO_WRITE_A_DESTRUCTURING_ASSIGNMENT_YOU_MIGHT_NEED_TO_WRAP_THE_WHOLE_ASSIGNMENT_IN_PARENTHESES, &[]);
                self.next_token();
            }
            return result;
        }
        let statements = self.create_missing_list();
        let result = self.new_block(statements, multiline);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_empty_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::SemicolonToken);
        let result = self.new_empty_statement();
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_if_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::IfKeyword);
        let open_paren_position = self.scanner().token_start();
        let open_paren_parsed = self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            Kind::OpenParenToken,
            Kind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        let then_statement = self.parse_statement();
        let mut else_statement = None;
        if self.parse_optional(Kind::ElseKeyword) {
            else_statement = Some(self.parse_statement());
        }
        let result = self.new_if_statement(expression, then_statement, else_statement);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_do_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::DoKeyword);
        let statement = self.parse_statement();
        self.parse_expected(Kind::WhileKeyword);
        let open_paren_position = self.scanner().token_start();
        let open_paren_parsed = self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            Kind::OpenParenToken,
            Kind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        // From: https://mail.mozilla.org/pipermail/es-discuss/2011-August/016188.html
        // 157 min --- All allen at wirfs-brock.com CONF --- "do{;}while(false)false" prohibited in
        // spec but allowed in consensus reality. Approved -- this is the de-facto standard whereby
        //  do;while(0)x will have a semicolon inserted before x.
        self.parse_optional(Kind::SemicolonToken);
        let result = self.new_do_statement(statement, expression);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_while_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::WhileKeyword);
        let open_paren_position = self.scanner().token_start();
        let open_paren_parsed = self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            Kind::OpenParenToken,
            Kind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        let statement = self.parse_statement();
        let result = self.new_while_statement(expression, statement);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_for_or_for_in_or_for_of_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::ForKeyword);
        let await_token = self.parse_optional_token(Kind::AwaitKeyword);
        self.parse_expected(Kind::OpenParenToken);
        let mut initializer: Option<Self::Node> = None;
        if self.token() != Kind::SemicolonToken {
            if self.token() == Kind::VarKeyword
                || self.token() == Kind::LetKeyword
                || self.token() == Kind::ConstKeyword
                || self.token() == Kind::UsingKeyword
                    && self.look_ahead(|parser| parser.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line_disallow_of())
                // this one is meant to allow of
                || self.token() == Kind::AwaitKeyword
                    && self.look_ahead(|parser| parser.next_is_using_keyword_then_binding_identifier_or_start_of_object_destructuring_on_same_line())
            {
                initializer = Some(self.parse_variable_declaration_list(true /*inForStatementInitializer*/));
            } else {
                initializer = Some(self.do_in_context(NodeFlags::DisallowInContext, true, Self::parse_expression));
            }
        }

        let result: Self::Node = if await_token.is_some() && self.parse_expected(Kind::OfKeyword)
            || await_token.is_none() && self.parse_optional(Kind::OfKeyword)
        {
            let expression = self.do_in_context(
                NodeFlags::DisallowInContext,
                false,
                Self::parse_assignment_expression_or_higher,
            );
            self.parse_expected(Kind::CloseParenToken);
            let statement = self.parse_statement();
            self.new_for_in_or_of_statement(
                Kind::ForOfStatement,
                await_token,
                initializer.unwrap(),
                expression,
                statement,
            )
        } else if self.parse_optional(Kind::InKeyword) {
            let expression = self.parse_expression_allow_in();
            self.parse_expected(Kind::CloseParenToken);
            let statement = self.parse_statement();
            self.new_for_in_or_of_statement(
                Kind::ForInStatement,
                None, /*awaitToken*/
                initializer.unwrap(),
                expression,
                statement,
            )
        } else {
            self.parse_expected(Kind::SemicolonToken);
            let mut condition = None;
            if self.token() != Kind::SemicolonToken && self.token() != Kind::CloseParenToken {
                condition = Some(self.parse_expression_allow_in());
            }
            self.parse_expected(Kind::SemicolonToken);
            let mut incrementor = None;
            if self.token() != Kind::CloseParenToken {
                incrementor = Some(self.parse_expression_allow_in());
            }
            self.parse_expected(Kind::CloseParenToken);
            let statement = self.parse_statement();
            self.new_for_statement(initializer, condition, incrementor, statement)
        };
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_break_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::BreakKeyword);
        let label = self.parse_identifier_unless_at_semicolon();
        self.parse_semicolon();
        let result = self.new_break_statement(label);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_continue_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::ContinueKeyword);
        let label = self.parse_identifier_unless_at_semicolon();
        self.parse_semicolon();
        let result = self.new_continue_statement(label);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_identifier_unless_at_semicolon(&mut self) -> Option<Self::Node> {
        if !self.can_parse_semicolon() {
            return Some(self.parse_identifier());
        }
        None
    }

    fn parse_return_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::ReturnKeyword);
        let mut expression = None;
        if !self.can_parse_semicolon() {
            expression = Some(self.parse_expression_allow_in());
        }
        self.parse_semicolon();
        let result = self.new_return_statement(expression);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_with_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::WithKeyword);
        let open_paren_position = self.scanner().token_start();
        let open_paren_parsed = self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected_matching_brackets(
            Kind::OpenParenToken,
            Kind::CloseParenToken,
            open_paren_parsed,
            open_paren_position,
        );
        let statement = self.do_in_context(NodeFlags::InWithStatement, true, Self::parse_statement);
        let result = self.new_with_statement(expression, statement);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_case_clause(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::CaseKeyword);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(Kind::ColonToken);
        let statements =
            self.parse_list(ParsingContext::SwitchClauseStatements, Self::parse_statement);
        let result =
            self.new_case_or_default_clause(Kind::CaseClause, Some(expression), statements);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_default_clause(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::DefaultKeyword);
        self.parse_expected(Kind::ColonToken);
        let statements =
            self.parse_list(ParsingContext::SwitchClauseStatements, Self::parse_statement);
        let result = self.new_case_or_default_clause(
            Kind::DefaultClause,
            None, /*expression*/
            statements,
        );
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_case_or_default_clause(&mut self) -> Self::Node {
        if self.token() == Kind::CaseKeyword {
            return self.parse_case_clause();
        }
        self.parse_default_clause()
    }

    fn parse_case_block(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::OpenBraceToken);
        let clauses =
            self.parse_list(ParsingContext::SwitchClauses, Self::parse_case_or_default_clause);
        self.parse_expected(Kind::CloseBraceToken);
        let result = self.new_case_block(clauses);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_switch_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::SwitchKeyword);
        self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_expression_allow_in();
        self.parse_expected(Kind::CloseParenToken);
        let case_block = self.parse_case_block();
        let result = self.new_switch_statement(expression, case_block);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_throw_statement(&mut self) -> Self::Node {
        // ThrowStatement[Yield] :
        //      throw [no LineTerminator here]Expression[In, ?Yield];
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::ThrowKeyword);
        // Because of automatic semicolon insertion, we need to report error if this
        // throw could be terminated with a semicolon.  Note: we can't call 'parseExpression'
        // directly as that might consume an expression on the following line.
        // Instead, we create a "missing" identifier, but don't report an error. The actual error
        // will be reported in the grammar walker.
        let expression = if self.has_preceding_line_break() {
            self.create_missing_identifier()
        } else {
            self.parse_expression_allow_in()
        };
        if !self.try_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
        }
        let result = self.new_throw_statement(expression);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    // TODO: Review for error recovery
    fn parse_try_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::TryKeyword);
        let try_block = self.parse_block(false /*ignoreMissingOpenBrace*/, None);
        let mut catch_clause = None;
        if self.token() == Kind::CatchKeyword {
            catch_clause = Some(self.parse_catch_clause());
        }
        // If we don't have a catch clause, then we must have a finally clause.  Try to parse
        // one out no matter what.
        let mut finally_block = None;
        if catch_clause.is_none() || self.token() == Kind::FinallyKeyword {
            self.parse_expected_with_diagnostic(
                Kind::FinallyKeyword,
                Some(&diagnostics::X_CATCH_OR_FINALLY_EXPECTED),
                true, /*shouldAdvance*/
            );
            finally_block = Some(self.parse_block(false /*ignoreMissingOpenBrace*/, None));
        }
        let result = self.new_try_statement(try_block, catch_clause, finally_block);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_catch_clause(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::CatchKeyword);
        let mut variable_declaration = None;
        if self.parse_optional(Kind::OpenParenToken) {
            variable_declaration = Some(self.parse_variable_declaration());
            self.parse_expected(Kind::CloseParenToken);
        }
        let block = self.parse_block(false /*ignoreMissingOpenBrace*/, None);
        let result = self.new_catch_clause(variable_declaration, block);
        self.finish_node(result, pos)
    }

    fn parse_debugger_statement(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        self.parse_expected(Kind::DebuggerKeyword);
        self.parse_semicolon();
        let result = self.new_debugger_statement();
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_expression_or_labeled_statement(&mut self) -> Self::Node {
        // Avoiding having to do the lookahead for a labeled statement by just trying to parse
        // out an expression, seeing if it is identifier and then seeing if it is followed by
        // a colon.
        let pos = self.node_pos();
        let mut jsdoc = self.jsdoc_scanner_info();
        let has_paren = self.token() == Kind::OpenParenToken;
        let expression = self.parse_expression();

        if self.node_kind(expression) == Kind::Identifier && self.parse_optional(Kind::ColonToken) {
            let statement = self.parse_statement();
            let result = self.new_labeled_statement(expression, statement);
            let result = self.finish_node(result, pos);
            self.with_jsdoc(result, jsdoc);
            return result;
        }

        if !self.try_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
        }
        let result = self.new_expression_statement(expression);
        let result = self.finish_node(result, pos);
        if has_paren {
            jsdoc.remove(JsdocScannerInfo::HasJSDoc);
        }
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_variable_statement(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let declaration_list =
            self.parse_variable_declaration_list(false /*inForStatementInitializer*/);
        self.parse_semicolon();
        let result = self.new_variable_statement(modifiers, declaration_list);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_variable_declaration_list(
        &mut self,
        in_for_statement_initializer: bool,
    ) -> Self::Node {
        let pos = self.node_pos();
        let mut flags = NodeFlags::None;
        match self.token() {
            Kind::VarKeyword => flags = NodeFlags::None,
            Kind::LetKeyword => flags = NodeFlags::Let,
            Kind::ConstKeyword => flags = NodeFlags::Const,
            Kind::UsingKeyword => flags = NodeFlags::Using,
            Kind::AwaitKeyword => {
                if self.is_await_using_declaration() {
                    flags = NodeFlags::AwaitUsing;
                    self.next_token();
                }
            }
            _ => panic!("Unhandled case in parseVariableDeclarationList"),
        }
        self.next_token();
        // The user may have written the following:
        //
        //    for (let of X) { }
        //
        // In this case, we want to parse an empty declaration list, and then parse 'of'
        // as a keyword. The reason this is not automatic is that 'of' is a valid identifier.
        // So we need to look ahead to determine if 'of' should be treated as a keyword in
        // this context.
        // The checker will then give an error that there is an empty declaration list.
        let declarations: Self::List;
        if self.token() == Kind::OfKeyword
            && self.look_ahead(|parser| parser.next_is_identifier_and_close_paren())
        {
            declarations = self.create_missing_list();
        } else {
            let save_context_flags = self.context_flags();
            self.set_context_flags(NodeFlags::DisallowInContext, in_for_statement_initializer);
            let parse_element: fn(&mut Self) -> Self::Node = if in_for_statement_initializer {
                Self::parse_variable_declaration
            } else {
                Self::parse_variable_declaration_allow_exclamation
            };
            declarations = self
                .parse_delimited_list(ParsingContext::VariableDeclarations, parse_element)
                .unwrap();
            self.replace_context_flags(save_context_flags);
        }
        let result = self.new_variable_declaration_list(declarations, flags);
        self.finish_node(result, pos)
    }

    fn parse_variable_declaration(&mut self) -> Self::Node {
        self.parse_variable_declaration_worker(false /*allowExclamation*/)
    }

    fn parse_variable_declaration_allow_exclamation(&mut self) -> Self::Node {
        self.parse_variable_declaration_worker(true /*allowExclamation*/)
    }

    fn parse_variable_declaration_worker(&mut self, allow_exclamation: bool) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let name = self.parse_identifier_or_pattern_with_diagnostic(Some(
            &diagnostics::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_IN_VARIABLE_DECLARATIONS,
        ));
        let mut exclamation_token = None;
        if allow_exclamation
            && self.node_kind(name) == Kind::Identifier
            && self.token() == Kind::ExclamationToken
            && !self.has_preceding_line_break()
        {
            exclamation_token = Some(self.parse_token_node());
        }
        let type_node = self.parse_type_annotation();
        let mut initializer = None;
        if self.token() != Kind::InKeyword && self.token() != Kind::OfKeyword {
            initializer = self.parse_initializer();
        }
        let result = self.new_variable_declaration(name, exclamation_token, type_node, initializer);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_identifier_or_pattern(&mut self) -> Self::Node {
        self.parse_identifier_or_pattern_with_diagnostic(None)
    }

    fn parse_identifier_or_pattern_with_diagnostic(
        &mut self,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> Self::Node {
        if self.token() == Kind::OpenBracketToken {
            return self.parse_array_binding_pattern();
        }
        if self.token() == Kind::OpenBraceToken {
            return self.parse_object_binding_pattern();
        }
        self.parse_binding_identifier_with_diagnostic(private_identifier_diagnostic_message)
    }

    fn parse_array_binding_pattern(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenBracketToken);
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let elements = self
            .parse_delimited_list(
                ParsingContext::ArrayBindingElements,
                Self::parse_array_binding_element,
            )
            .unwrap();
        self.replace_context_flags(save_context_flags);
        self.parse_expected(Kind::CloseBracketToken);
        let result = self.new_binding_pattern(Kind::ArrayBindingPattern, elements);
        self.finish_node(result, pos)
    }

    fn parse_array_binding_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let mut dot_dot_dot_token = None;
        let mut name = None;
        let mut initializer = None;
        if self.token() != Kind::CommaToken {
            // These are all nil for a missing element
            dot_dot_dot_token = self.parse_optional_token(Kind::DotDotDotToken);
            name = Some(self.parse_identifier_or_pattern());
            initializer = self.parse_initializer();
        }
        let result = self.new_binding_element(
            dot_dot_dot_token,
            None, /*propertyName*/
            name,
            initializer,
        );
        self.finish_node(result, pos)
    }

    fn parse_object_binding_pattern(&mut self) -> Self::Node {
        let pos = self.node_pos();
        self.parse_expected(Kind::OpenBraceToken);
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::DisallowInContext, false);
        let elements = self
            .parse_delimited_list(
                ParsingContext::ObjectBindingElements,
                Self::parse_object_binding_element,
            )
            .unwrap();
        self.replace_context_flags(save_context_flags);
        self.parse_expected(Kind::CloseBraceToken);
        let result = self.new_binding_pattern(Kind::ObjectBindingPattern, elements);
        self.finish_node(result, pos)
    }

    fn parse_object_binding_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let dot_dot_dot_token = self.parse_optional_token(Kind::DotDotDotToken);
        let token_is_identifier = self.is_binding_identifier();
        let mut property_name = Some(self.parse_property_name());
        let name;
        if token_is_identifier && self.token() != Kind::ColonToken {
            name = property_name;
            property_name = None;
        } else {
            self.parse_expected(Kind::ColonToken);
            name = Some(self.parse_identifier_or_pattern());
        }
        let initializer = self.parse_initializer();
        let result = self.new_binding_element(dot_dot_dot_token, property_name, name, initializer);
        self.finish_node(result, pos)
    }

    fn parse_initializer(&mut self) -> Option<Self::Node> {
        if self.parse_optional(Kind::EqualsToken) {
            return Some(self.parse_assignment_expression_or_higher());
        }
        None
    }

    fn parse_function_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected(Kind::FunctionKeyword);
        let asterisk_token = self.parse_optional_token(Kind::AsteriskToken);
        // We don't parse the name here in await context, instead we will report a grammar error in the checker.
        let mut name = None;
        if modifiers.is_none_or(|modifiers| {
            !self.modifier_flags(modifiers).intersects(ModifierFlags::Default)
        }) || self.is_binding_identifier()
        {
            name = Some(self.parse_binding_identifier());
        }
        let signature_flags =
            (if asterisk_token.is_some() { ParseFlags::Yield } else { ParseFlags::None })
                | (if modifiers.is_some_and(|modifiers| {
                    self.modifier_flags(modifiers).intersects(ModifierFlags::Async)
                }) {
                    ParseFlags::Await
                } else {
                    ParseFlags::None
                });
        let type_parameters = self.parse_type_parameters();
        let save_context_flags = self.context_flags();
        if modifiers.is_some_and(|modifiers| {
            self.modifier_flags(modifiers).intersects(ModifierFlags::Export)
        }) {
            self.set_context_flags(NodeFlags::AwaitContext, true);
        }
        let parameters = self.parse_parameters(signature_flags);
        let return_type = self.parse_return_type(Kind::ColonToken, false /*isType*/);
        let body = self
            .parse_function_block_or_semicolon(signature_flags, Some(&diagnostics::X_OR_EXPECTED));
        self.replace_context_flags(save_context_flags);
        let result = self.new_function_declaration(
            modifiers,
            asterisk_token,
            name,
            type_parameters,
            Some(parameters),
            return_type,
            None, /*fullSignature*/
            body,
        );
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_class_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_class_declaration_or_expression(pos, jsdoc, modifiers, Kind::ClassDeclaration)
    }

    fn parse_class_declaration_or_expression(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        kind: Kind,
    ) -> Self::Node {
        let save_context_flags = self.context_flags();
        let save_has_await_identifier = self.statement_has_await_identifier();
        self.parse_expected(Kind::ClassKeyword);
        // We don't parse the name here in await context, instead we will report a grammar error in the checker.
        let name = self.parse_name_of_class_declaration_or_expression();
        let type_parameters = self.parse_type_parameters();
        if modifiers.is_some()
            && self.parsing_contexts() & (1 << ParsingContext::SourceElements as u32) != 0
            && self.parsing_contexts()
                & ((1 << ParsingContext::BlockStatements as u32)
                    | (1 << ParsingContext::SwitchClauseStatements as u32))
                == 0
            && self.modifier_list_has_kind(modifiers.unwrap(), Kind::ExportKeyword)
        {
            self.set_context_flags(NodeFlags::AwaitContext, true /*value*/);
        }
        let heritage_clauses = self.parse_heritage_clauses(false /*isInterface*/);
        let members: Self::List;
        if self.parse_expected(Kind::OpenBraceToken) {
            // ClassTail[Yield,Await] : (Modified) See 14.5
            //      ClassHeritage[?Yield,?Await]opt { ClassBody[?Yield,?Await]opt }
            members =
                self.parse_member_list(ParsingContext::ClassMembers, Self::parse_class_element);
            self.parse_expected(Kind::CloseBraceToken);
        } else {
            members = self.create_missing_list();
        }
        self.replace_context_flags(save_context_flags);
        if modifiers.is_some_and(|modifiers| {
            self.modifier_flags_from_nodes(modifiers).intersects(ModifierFlags::Ambient)
        }) {
            self.set_statement_has_await_identifier(save_has_await_identifier);
        }
        let result = if kind == Kind::ClassDeclaration {
            self.new_class_declaration(modifiers, name, type_parameters, heritage_clauses, members)
        } else {
            self.new_class_expression(modifiers, name, type_parameters, heritage_clauses, members)
        };
        self.set_lazy_owner(members, result);
        self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        if self.node_flags(result).intersects(NodeFlags::JavaScriptFile) {
            self.check_js_syntax(result);
            if let Some(heritage_clauses) = heritage_clauses {
                for index in 0..self.list_len(heritage_clauses) {
                    let clause = self.list_element(heritage_clauses, index);
                    let (token, types) = self.heritage_clause_parts(clause);
                    if token == Kind::ExtendsKeyword {
                        for type_index in 0..self.list_len(types) {
                            let expression = self.list_element(types, type_index);
                            self.check_js_syntax(expression);
                        }
                    }
                }
            }
        }
        result
    }

    fn parse_name_of_class_declaration_or_expression(&mut self) -> Option<Self::Node> {
        // implements is a future reserved word so
        // 'class implements' might mean either
        // - class expression with omitted name, 'implements' starts heritage clause
        // - class with name 'implements'
        // 'isImplementsClause' helps to disambiguate between these two cases
        if self.is_binding_identifier() && !self.is_implements_clause() {
            let save_has_await_identifier = self.statement_has_await_identifier();
            let is_binding_identifier = self.is_binding_identifier();
            let id = self.create_identifier(is_binding_identifier);
            self.set_statement_has_await_identifier(save_has_await_identifier);
            return Some(id);
        }
        None
    }

    fn parse_heritage_clauses(&mut self, is_interface: bool) -> Option<Self::List> {
        // ClassTail[Yield,Await] : (Modified) See 14.5
        //      ClassHeritage[?Yield,?Await]opt { ClassBody[?Yield,?Await]opt }
        if self.is_heritage_clause() {
            return Some(self.parse_list(ParsingContext::HeritageClauses, |p| {
                p.parse_heritage_clause(is_interface)
            }));
        }
        None
    }

    fn parse_heritage_clause(&mut self, is_interface: bool) -> Self::Node {
        let pos = self.node_pos();
        let kind = self.token();
        self.next_token();
        let mut parse_element: fn(&mut Self) -> Self::Node =
            Self::parse_expression_with_type_arguments;
        if crate::parser_1::is_type_heritage_clause(is_interface, kind) {
            parse_element = Self::parse_type_heritage_clause_element;
        }
        let types = self
            .parse_delimited_list(ParsingContext::HeritageClauseElement, parse_element)
            .unwrap();
        let result = self.new_heritage_clause(kind, types);
        let result = self.finish_node(result, pos);
        self.check_js_syntax(result)
    }

    fn parse_type_heritage_clause_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let node = self.parse_expression_with_type_arguments();
        let (expression, type_arguments) = self.expression_with_type_arguments_parts(node);
        if !self.is_valid_heritage_type_reference_expression(expression) {
            return node;
        }
        let type_name = self.convert_entity_name_expression_to_entity_name(expression);
        let result = self.new_type_reference_node(type_name, type_arguments);
        self.finish_node(result, pos)
    }

    fn convert_entity_name_expression_to_entity_name(&mut self, node: Self::Node) -> Self::Node {
        if self.node_kind(node) == Kind::Identifier {
            return node;
        }
        let expression = self.node_expression(node).unwrap();
        let name = self.node_name(node).unwrap();
        let left = self.convert_entity_name_expression_to_entity_name(expression);
        let result = self.new_qualified_name(left, name);
        self.finish_node_with_end(
            result,
            self.node_range(node).pos().expect_source("parsed nodes have source positions"),
            self.node_range(node).end().expect_source("parsed nodes have source positions"),
        )
    }

    fn parse_expression_with_type_arguments(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let expression = self.parse_left_hand_side_expression_or_higher();
        if self.node_kind(expression) == Kind::ExpressionWithTypeArguments {
            return expression;
        }
        let type_arguments = self.parse_type_arguments();
        let result = self.new_expression_with_type_arguments(expression, type_arguments);
        self.finish_node(result, pos)
    }

    fn parse_class_element(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        if self.token() == Kind::SemicolonToken {
            self.next_token();
            let result = self.new_semicolon_class_element();
            let result = self.finish_node(result, pos);
            self.with_jsdoc(result, jsdoc);
            return result;
        }
        let modifiers = self.parse_modifiers_ex(
            true, /*allowDecorators*/
            true, /*permitConstAsModifier*/
            true, /*stopOnStartOfClassStaticBlock*/
        );
        if self.token() == Kind::StaticKeyword
            && self.look_ahead(|parser| parser.next_token_is_open_brace())
        {
            return self.parse_class_static_block_declaration(pos, jsdoc, modifiers);
        }
        if self.parse_contextual_modifier(Kind::GetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::GetAccessor,
                ParseFlags::None,
            );
        }
        if self.parse_contextual_modifier(Kind::SetKeyword) {
            return self.parse_accessor_declaration(
                pos,
                jsdoc,
                modifiers,
                Kind::SetAccessor,
                ParseFlags::None,
            );
        }
        if (self.token() == Kind::ConstructorKeyword || self.token() == Kind::StringLiteral)
            && let Some(constructor_declaration) =
                self.try_parse_constructor_declaration(pos, jsdoc, modifiers)
        {
            return constructor_declaration;
        }
        if self.is_index_signature() {
            let result = self.parse_index_signature_declaration(pos, jsdoc, modifiers);
            return self.check_js_syntax(result);
        }
        // It is very important that we check this *after* checking indexers because
        // the [ token can start an index signature or a computed property name
        if token_is_identifier_or_keyword(self.token())
            || self.token() == Kind::StringLiteral
            || self.token() == Kind::NumericLiteral
            || self.token() == Kind::BigIntLiteral
            || self.token() == Kind::AsteriskToken
            || self.token() == Kind::OpenBracketToken
        {
            let is_ambient = modifiers.is_some_and(|modifiers| {
                self.modifier_list_has_kind(modifiers, Kind::DeclareKeyword)
            });
            if is_ambient {
                self.set_modifiers_ambient(modifiers.unwrap());
                let save_context_flags = self.context_flags();
                self.set_context_flags(NodeFlags::Ambient, true);
                let result = self.parse_property_or_method_declaration(pos, jsdoc, modifiers);
                self.replace_context_flags(save_context_flags);
                return result;
            }
            return self.parse_property_or_method_declaration(pos, jsdoc, modifiers);
        }
        if modifiers.is_some() {
            // treat this as a property declaration with a missing name.
            let node_pos = self.node_pos();
            self.error_at(
                TextRange::new(node_pos, node_pos),
                &diagnostics::DECLARATION_EXPECTED,
                &[],
            );
            let name = self.create_missing_identifier();
            return self.parse_property_declaration(
                pos, jsdoc, modifiers, name, None, /*questionToken*/
            );
        }
        // 'isClassMemberStart' should have hinted not to attempt parsing.
        panic!("Should not have attempted to parse class member declaration.");
    }

    fn parse_class_static_block_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected_token(Kind::StaticKeyword);
        let body = self.parse_class_static_block_body();
        let result = self.new_class_static_block_declaration(modifiers, body);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_class_static_block_body(&mut self) -> Self::Node {
        let save_context_flags = self.context_flags();
        self.set_context_flags(NodeFlags::YieldContext, false);
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let body = self
            .parse_block(false /*ignoreMissingOpenBrace*/, None /*diagnosticMessage*/);
        self.replace_context_flags(save_context_flags);
        body
    }

    fn try_parse_constructor_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Option<Self::Node> {
        let state = self.mark();
        if self.token() == Kind::ConstructorKeyword
            || self.token() == Kind::StringLiteral
                && self.current_token_value_is("constructor")
                && self.look_ahead(|parser| parser.next_token_is_open_paren())
        {
            self.next_token();
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameters(ParseFlags::None);
            let return_type = self.parse_return_type(Kind::ColonToken, false /*isType*/);
            let body = self.parse_function_block_or_semicolon(
                ParseFlags::None,
                Some(&diagnostics::X_OR_EXPECTED),
            );
            let result = self.new_constructor_declaration(
                modifiers,
                type_parameters,
                Some(parameters),
                return_type,
                None, /*fullSignature*/
                body,
            );
            let result = self.finish_node(result, pos);
            self.with_jsdoc(result, jsdoc);
            self.check_js_syntax(result);
            return Some(result);
        }
        self.rewind(state);
        None
    }

    fn parse_property_or_method_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let asterisk_token = self.parse_optional_token(Kind::AsteriskToken);
        let name = self.parse_property_name();
        // Note: this is not legal as per the grammar.  But we allow it in the parser and
        // report an error in the grammar checker.
        let question_token = self.parse_optional_token(Kind::QuestionToken);
        if asterisk_token.is_some()
            || self.token() == Kind::OpenParenToken
            || self.token() == Kind::LessThanToken
        {
            return self.parse_method_declaration(
                pos,
                jsdoc,
                modifiers,
                asterisk_token,
                name,
                question_token,
                Some(&diagnostics::X_OR_EXPECTED),
            );
        }
        self.parse_property_declaration(pos, jsdoc, modifiers, name, question_token)
    }

    fn parse_property_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        name: Self::Node,
        question_token: Option<Self::Node>,
    ) -> Self::Node {
        let mut postfix_token = question_token;
        if postfix_token.is_none() && !self.has_preceding_line_break() {
            postfix_token = self.parse_optional_token(Kind::ExclamationToken);
        }
        let type_node = self.parse_type_annotation();
        let initializer = self.do_in_context(
            NodeFlags::YieldContext | NodeFlags::AwaitContext | NodeFlags::DisallowInContext,
            false,
            Self::parse_initializer,
        );
        self.parse_semicolon_after_property_name(name, type_node, initializer);
        let result =
            self.new_property_declaration(modifiers, name, postfix_token, type_node, initializer);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_semicolon_after_property_name(
        &mut self,
        name: Self::Node,
        type_node: Option<Self::Node>,
        initializer: Option<Self::Node>,
    ) {
        if self.token() == Kind::AtToken && !self.has_preceding_line_break() {
            self.parse_error_at_current_token(&diagnostics::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS, &[]);
            return;
        }
        if self.token() == Kind::OpenParenToken {
            self.parse_error_at_current_token(
                &diagnostics::CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION,
                &[],
            );
            self.next_token();
            return;
        }
        if type_node.is_some() && !self.can_parse_semicolon() {
            if initializer.is_some() {
                self.parse_error_at_current_token(
                    &diagnostics::X_0_EXPECTED,
                    &[&scanner::token_to_string(Kind::SemicolonToken)],
                );
            } else {
                self.parse_error_at_current_token(
                    &diagnostics::EXPECTED_FOR_PROPERTY_INITIALIZER,
                    &[],
                );
            }
            return;
        }
        if self.try_parse_semicolon() {
            return;
        }
        if initializer.is_some() {
            self.parse_error_at_current_token(
                &diagnostics::X_0_EXPECTED,
                &[&scanner::token_to_string(Kind::SemicolonToken)],
            );
            return;
        }
        self.parse_error_for_missing_semicolon_after(name);
    }

    fn parse_error_for_missing_semicolon_after(&mut self, node: Self::Node) {
        // Tagged template literals are sometimes used in places where only simple strings are allowed, i.e.:
        //   module `M1` {
        //   ^^^^^^^^^^^ This block is parsed as a template literal like module`M1`.
        if self.node_kind(node) == Kind::TaggedTemplateExpression {
            let loc =
                self.skip_expression_range_trivia(self.node_range(self.tagged_template(node)));
            self.error_at(
                loc,
                &diagnostics::MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS,
                &[],
            );
            return;
        }
        // Otherwise, if this isn't a well-known keyword-like identifier, give the generic fallback message.
        let mut expression_text = "";
        if self.node_kind(node) == Kind::Identifier {
            expression_text = self.node_text(node);
        }
        if expression_text.is_empty() {
            self.parse_error_at_current_token(
                &diagnostics::X_0_EXPECTED,
                &[&scanner::token_to_string(Kind::SemicolonToken)],
            );
            return;
        }
        let pos = self.skip_expression_trivia(
            self.node_range(node).pos().expect_source("parsed nodes have source positions"),
        );
        let end = self.node_range(node).end().expect_source("parsed nodes have source positions");
        // Some known keywords are likely signs of syntax being used improperly.
        match expression_text {
            "const" | "let" | "var" => {
                self.error_at(
                    TextRange::new(pos, end),
                    &diagnostics::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION,
                    &[],
                );
                return;
            }
            "declare" => {
                // If a declared node failed to parse, it would have emitted a diagnostic already.
                return;
            }
            "interface" => {
                self.parse_error_for_invalid_name(
                    &diagnostics::INTERFACE_NAME_CANNOT_BE_0,
                    &diagnostics::INTERFACE_MUST_BE_GIVEN_A_NAME,
                    Kind::OpenBraceToken,
                );
                return;
            }
            "is" => {
                let end = self.scanner().token_start();
                self.error_at(TextRange::new(pos, end), &diagnostics::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHODS, &[]);
                return;
            }
            "module" | "namespace" => {
                self.parse_error_for_invalid_name(
                    &diagnostics::NAMESPACE_NAME_CANNOT_BE_0,
                    &diagnostics::NAMESPACE_MUST_BE_GIVEN_A_NAME,
                    Kind::OpenBraceToken,
                );
                return;
            }
            "type" => {
                self.parse_error_for_invalid_name(
                    &diagnostics::TYPE_ALIAS_NAME_CANNOT_BE_0,
                    &diagnostics::TYPE_ALIAS_MUST_BE_GIVEN_A_NAME,
                    Kind::EqualsToken,
                );
                return;
            }
            _ => {}
        }
        // The user alternatively might have misspelled or forgotten to add a space after a common keyword.
        let mut suggestion = tsrs_core::get_spelling_suggestion_for_strings(
            expression_text,
            crate::parser_1::VIABLE_KEYWORD_SUGGESTIONS.iter().copied(),
        )
        .map(std::string::ToString::to_string)
        .unwrap_or_default();
        if suggestion.is_empty() {
            suggestion = crate::parser_1::get_space_suggestion(expression_text);
        }
        if !suggestion.is_empty() {
            self.error_at(
                TextRange::new(pos, end),
                &diagnostics::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN_0,
                &[&suggestion],
            );
            return;
        }
        // Unknown tokens are handled with their own errors in the scanner
        if self.token() == Kind::Unknown {
            return;
        }
        // Otherwise, we know this some kind of unknown word, not just a missing expected semicolon.
        self.error_at(
            TextRange::new(pos, end),
            &diagnostics::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            &[],
        );
    }

    fn parse_error_for_invalid_name(
        &mut self,
        name_diagnostic: &'static Message,
        blank_diagnostic: &'static Message,
        token_if_blank_name: Kind,
    ) {
        if self.token() == token_if_blank_name {
            self.parse_error_at_current_token(blank_diagnostic, &[]);
        } else {
            self.error_at_current_token_value(name_diagnostic);
        }
    }

    fn parse_interface_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected(Kind::InterfaceKeyword);
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        let heritage_clauses = self.parse_heritage_clauses(true /*isInterface*/);
        let members = self.parse_object_type_members();
        let result = self.new_interface_declaration(
            modifiers,
            name,
            type_parameters,
            heritage_clauses,
            members,
        );
        self.set_lazy_owner(members, result);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_type_alias_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected(Kind::TypeKeyword);
        if self.has_preceding_line_break() {
            self.parse_error_at_current_token(&diagnostics::LINE_BREAK_NOT_PERMITTED_HERE, &[]);
        }
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        self.parse_expected(Kind::EqualsToken);
        let type_node = if self.token() == Kind::IntrinsicKeyword
            && self.look_ahead(|parser| parser.next_is_not_dot())
        {
            self.parse_keyword_type_node()
        } else {
            self.parse_type()
        };
        self.parse_semicolon();
        let result =
            self.new_type_alias_declaration(modifiers, name, type_parameters, Some(type_node));
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    // In an ambient declaration, the grammar only allows integer literals as initializers.
    // In a non-ambient declaration, the grammar allows uninitialized members only in a
    // ConstantEnumMemberSection, which starts at the beginning of an enum declaration
    // or any time an integer literal initializer is encountered.
    fn parse_enum_member(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let name = self.parse_property_name();
        let initializer =
            self.do_in_context(NodeFlags::DisallowInContext, false, Self::parse_initializer);
        let result = self.new_enum_member(name, initializer);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_enum_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let save_has_await_identifier = self.statement_has_await_identifier();
        self.parse_expected(Kind::EnumKeyword);
        let name = self.parse_identifier();
        let members: Self::List;
        if self.parse_expected(Kind::OpenBraceToken) {
            let save_context_flags = self.context_flags();
            self.set_context_flags(NodeFlags::YieldContext | NodeFlags::AwaitContext, false);
            members = self
                .parse_delimited_list(ParsingContext::EnumMembers, Self::parse_enum_member)
                .unwrap();
            self.replace_context_flags(save_context_flags);
            self.parse_expected(Kind::CloseBraceToken);
        } else {
            members = self.create_missing_list();
        }
        let result = self.new_enum_declaration(modifiers, name, members);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        result
    }

    fn parse_module_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let mut keyword = Kind::ModuleKeyword;
        if self.token() == Kind::GlobalKeyword {
            // global augmentation
            return self.parse_ambient_external_module_declaration(pos, jsdoc, modifiers);
        } else if self.parse_optional(Kind::NamespaceKeyword) {
            keyword = Kind::NamespaceKeyword;
        } else {
            self.parse_expected(Kind::ModuleKeyword);
            if self.token() == Kind::StringLiteral {
                return self.parse_ambient_external_module_declaration(pos, jsdoc, modifiers);
            }
        }
        self.parse_module_or_namespace_declaration(
            pos, jsdoc, modifiers, false, /*nested*/
            keyword,
        )
    }

    fn parse_ambient_external_module_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let name: Self::Node;
        let mut keyword = Kind::ModuleKeyword;
        let save_has_await_identifier = self.statement_has_await_identifier();
        if self.token() == Kind::GlobalKeyword {
            // parse 'global' as name of global scope augmentation
            name = self.parse_identifier();
            keyword = Kind::GlobalKeyword;
        } else {
            // parse string literal
            name = self.parse_literal_expression();
        }
        let mut attributes = None;
        if keyword == Kind::ModuleKeyword && self.parse_optional(Kind::WithKeyword) {
            attributes = Some(self.parse_type_literal());
        }
        let mut body = None;
        if self.token() == Kind::OpenBraceToken {
            body = Some(self.parse_module_block());
        } else {
            self.parse_semicolon();
        }
        let result = self.new_module_declaration(modifiers, keyword, name, attributes, body);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        result
    }

    fn parse_module_block(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let statements: Self::List;
        if self.parse_expected(Kind::OpenBraceToken) {
            statements = self.parse_list(ParsingContext::BlockStatements, Self::parse_statement);
            self.parse_expected(Kind::CloseBraceToken);
        } else {
            statements = self.create_missing_list();
        }
        let result = self.new_module_block(statements);
        self.finish_node(result, pos)
    }

    fn parse_module_or_namespace_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        nested: bool,
        keyword: Kind,
    ) -> Self::Node {
        let save_has_await_identifier = self.statement_has_await_identifier();
        let name = if nested { self.parse_identifier_name() } else { self.parse_identifier() };

        let body: Self::Node = if self.parse_optional(Kind::DotToken) {
            let implicit_export = self.new_modifier(Kind::ExportKeyword);
            let node_pos = self.node_pos();
            self.set_node_range(implicit_export, TextRange::new(node_pos, node_pos));
            self.set_node_flags(implicit_export, NodeFlags::Reparsed);
            let implicit_modifiers =
                self.new_modifier_list(self.node_range(implicit_export), &[implicit_export]);
            let node_pos = self.node_pos();
            self.parse_module_or_namespace_declaration(
                node_pos,
                JsdocScannerInfo::empty(), /*jsdoc*/
                Some(implicit_modifiers),
                true, /*nested*/
                keyword,
            )
        } else {
            self.parse_module_block()
        };
        let result = self.new_module_declaration(modifiers, keyword, name, None, Some(body));
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        result
    }

    fn parse_import_declaration_or_import_equals_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected(Kind::ImportKeyword);
        let after_import_pos = self.node_pos();
        // We don't parse the identifier here in await context, instead we will report a grammar error in the checker.
        let save_has_await_identifier = self.statement_has_await_identifier();
        let phase_modifier_candidate = self.current_import_phase_modifier();
        let mut identifier = None;
        if self.is_identifier() {
            identifier = Some(self.parse_identifier());
        }
        let mut phase_modifier = Kind::Unknown;
        if identifier.is_some_and(|identifier| self.node_text(identifier) == "type")
            && (self.token() != Kind::FromKeyword
                || self.is_identifier()
                    && self
                        .look_ahead(|parser| parser.next_token_is_from_keyword_or_equals_token()))
            && (self.is_identifier()
                || self.token_after_import_definitely_produces_import_declaration())
        {
            phase_modifier = Kind::TypeKeyword;
            identifier = None;
            if self.is_identifier() {
                identifier = Some(self.parse_identifier());
            }
        } else if identifier.is_some()
            && phase_modifier_candidate != Kind::Unknown
            && self.should_parse_import_phase_modifier()
        {
            phase_modifier = phase_modifier_candidate;
            identifier = None;
            if self.is_identifier() {
                identifier = Some(self.parse_identifier());
            }
        }
        if let Some(identifier) = identifier
            && self.token_after_imported_identifier_allows_import_equals_declaration()
            && phase_modifier != Kind::DeferKeyword
            && phase_modifier != Kind::SourceKeyword
        {
            let import_equals = self.parse_import_equals_declaration(
                pos,
                jsdoc,
                modifiers,
                identifier,
                phase_modifier == Kind::TypeKeyword,
            );
            let import_equals = self.check_js_syntax(import_equals);
            self.set_statement_has_await_identifier(save_has_await_identifier); // Import= declaration is always parsed in an Await context, no need to reparse
            return import_equals;
        }
        let import_clause = self.try_parse_import_clause(
            identifier,
            after_import_pos,
            phase_modifier,
            false, /*skipJSDocLeadingAsterisks*/
        );
        self.set_statement_has_await_identifier(save_has_await_identifier); // import clause is always parsed in an Await context
        let module_specifier = self.parse_module_specifier();
        let attributes = self.try_parse_import_attributes();
        self.parse_semicolon();
        let result =
            self.new_import_declaration(modifiers, import_clause, module_specifier, attributes);
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_import_equals_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
        identifier: Self::Node,
        is_type_only: bool,
    ) -> Self::Node {
        self.parse_expected(Kind::EqualsToken);
        let module_reference = self.parse_module_reference();
        self.parse_semicolon();
        let result = self.new_import_equals_declaration(
            modifiers,
            is_type_only,
            identifier,
            module_reference,
        );
        let result = self.finish_node(result, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_module_reference(&mut self) -> Self::Node {
        if self.token() == Kind::RequireKeyword
            && self.look_ahead(|parser| parser.next_token_is_open_paren())
        {
            return self.parse_external_module_reference();
        }
        self.parse_entity_name(
            false, /*allowReservedWords*/
            false, /*allowPrivateName*/
            None,  /*diagnosticMessage*/
        )
    }

    fn parse_external_module_reference(&mut self) -> Self::Node {
        let save_has_await_identifier = self.statement_has_await_identifier();
        let pos = self.node_pos();
        self.parse_expected(Kind::RequireKeyword);
        self.parse_expected(Kind::OpenParenToken);
        let expression = self.parse_module_specifier();
        self.parse_expected(Kind::CloseParenToken);
        let result = self.new_external_module_reference(expression);
        let result = self.finish_node(result, pos);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        result
    }

    fn parse_module_specifier(&mut self) -> Self::Node {
        if self.token() == Kind::StringLiteral {
            return self.parse_literal_expression();
        }
        // We allow arbitrary expressions here, even though the grammar only allows string
        // literals.  We check to ensure that it is only a string literal later in the grammar
        // check pass.
        self.parse_expression()
    }

    fn try_parse_import_clause(
        &mut self,
        identifier: Option<Self::Node>,
        pos: u32,
        phase_modifier: Kind,
        skip_jsdoc_leading_asterisks: bool,
    ) -> Option<Self::Node> {
        // ImportDeclaration:
        //  import ImportClause from ModuleSpecifier ;
        //  import ModuleSpecifier;
        if identifier.is_some()
            || self.token() == Kind::AsteriskToken
            || self.token() == Kind::OpenBraceToken
        {
            let import_clause = self.parse_import_clause(
                identifier,
                pos,
                phase_modifier,
                skip_jsdoc_leading_asterisks,
            );
            self.parse_expected(Kind::FromKeyword);
            return Some(import_clause);
        }
        if phase_modifier == Kind::DeferKeyword || phase_modifier == Kind::SourceKeyword {
            let clause = self.new_import_clause(phase_modifier, None, None);
            return Some(self.finish_node(clause, pos));
        }
        None
    }

    fn parse_import_clause(
        &mut self,
        identifier: Option<Self::Node>,
        pos: u32,
        phase_modifier: Kind,
        skip_jsdoc_leading_asterisks: bool,
    ) -> Self::Node {
        // ImportClause:
        //  ImportedDefaultBinding
        //  NameSpaceImport
        //  NamedImports
        //  ImportedDefaultBinding, NameSpaceImport
        //  ImportedDefaultBinding, NamedImports
        // If there was no default import or if there is comma token after default import
        // parse namespace or named imports
        let mut named_bindings = None;
        let save_has_await_identifier = self.statement_has_await_identifier();
        if identifier.is_none() || self.parse_optional(Kind::CommaToken) {
            if skip_jsdoc_leading_asterisks {
                self.set_skip_jsdoc_leading_asterisks(true);
            }
            if self.token() == Kind::AsteriskToken {
                named_bindings = Some(self.parse_namespace_import());
            } else {
                named_bindings = Some(self.parse_named_imports());
            }
            if skip_jsdoc_leading_asterisks {
                self.set_skip_jsdoc_leading_asterisks(false);
            }
        }
        let result = self.new_import_clause(phase_modifier, identifier, named_bindings);
        let result = self.finish_node(result, pos);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        result
    }

    fn parse_namespace_import(&mut self) -> Self::Node {
        // NameSpaceImport:
        //  * as ImportedBinding
        let pos = self.node_pos();
        self.parse_expected(Kind::AsteriskToken);
        self.parse_expected(Kind::AsKeyword);
        let name = self.parse_identifier();
        let node = self.new_namespace_import(name);
        self.finish_node(node, pos)
    }

    fn parse_named_imports(&mut self) -> Self::Node {
        let pos = self.node_pos();
        // NamedImports:
        //  { }
        //  { ImportsList }
        //  { ImportsList, }
        let imports = self
            .parse_bracketed_list(
                ParsingContext::ImportOrExportSpecifiers,
                Self::parse_import_specifier,
                Kind::OpenBraceToken,
                Kind::CloseBraceToken,
            )
            .unwrap();
        let node = self.new_named_imports(imports);
        self.finish_node(node, pos)
    }

    fn parse_import_specifier(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let (is_type_only, property_name, name) =
            self.parse_import_or_export_specifier(Kind::ImportSpecifier);
        let identifier_name;
        if self.node_kind(name) == Kind::Identifier {
            identifier_name = name;
        } else {
            let loc = self.skip_expression_range_trivia(self.node_range(name));
            self.error_at(loc, &diagnostics::IDENTIFIER_EXPECTED, &[]);
            identifier_name = self.new_identifier("");
            self.finish_node(
                identifier_name,
                self.node_range(name).pos().expect_source("parsed nodes have source positions"),
            );
        }
        let node = self.new_import_specifier(is_type_only, property_name, identifier_name);
        let node = self.finish_node(node, pos);
        self.check_js_syntax(node)
    }

    fn parse_import_or_export_specifier(
        &mut self,
        kind: Kind,
    ) -> (bool, Option<Self::Node>, Self::Node) {
        // ImportSpecifier:
        //   BindingIdentifier
        //   ModuleExportName as BindingIdentifier
        // ExportSpecifier:
        //   ModuleExportName
        //   ModuleExportName as ModuleExportName
        // let checkIdentifierIsKeyword = isKeyword(token()) && !isIdentifier();
        // let checkIdentifierStart = scanner.getTokenStart();
        // let checkIdentifierEnd = scanner.getTokenEnd();
        let mut is_type_only = false;
        let mut property_name: Option<Self::Node> = None;
        let mut can_parse_as_keyword = true;
        let disallow_keywords = kind == Kind::ImportSpecifier;
        let (mut name, mut name_ok) = self.parse_module_export_name(disallow_keywords);
        if self.node_kind(name) == Kind::Identifier && self.node_text(name) == "type" {
            // If the first token of an import specifier is 'type', there are a lot of possibilities,
            // especially if we see 'as' afterwards:
            //
            // import { type } from "mod";          - isTypeOnly: false,   name: type
            // import { type as } from "mod";       - isTypeOnly: true,    name: as
            // import { type as as } from "mod";    - isTypeOnly: false,   name: as,    propertyName: type
            // import { type as as as } from "mod"; - isTypeOnly: true,    name: as,    propertyName: as
            if self.token() == Kind::AsKeyword {
                // { type as ...? }
                let first_as = self.parse_identifier_name();
                if self.token() == Kind::AsKeyword {
                    // { type as as ...? }
                    let second_as = self.parse_identifier_name();
                    if self.can_parse_module_export_name() {
                        // { type as as something }
                        // { type as as "something" }
                        is_type_only = true;
                        property_name = Some(first_as);
                        (name, name_ok) = self.parse_module_export_name(disallow_keywords);
                        can_parse_as_keyword = false;
                    } else {
                        // { type as as }
                        property_name = Some(name);
                        name = second_as;
                        can_parse_as_keyword = false;
                    }
                } else if self.can_parse_module_export_name() {
                    // { type as something }
                    // { type as "something" }
                    property_name = Some(name);
                    can_parse_as_keyword = false;
                    (name, name_ok) = self.parse_module_export_name(disallow_keywords);
                } else {
                    // { type as }
                    is_type_only = true;
                    name = first_as;
                }
            } else if self.can_parse_module_export_name() {
                // { type something ...? }
                // { type "something" ...? }
                is_type_only = true;
                (name, name_ok) = self.parse_module_export_name(disallow_keywords);
            }
        }
        if can_parse_as_keyword && self.token() == Kind::AsKeyword {
            property_name = Some(name);
            self.parse_expected(Kind::AsKeyword);
            (name, name_ok) = self.parse_module_export_name(disallow_keywords);
        }

        if !name_ok {
            let loc = self.skip_expression_range_trivia(self.node_range(name));
            self.error_at(loc, &diagnostics::IDENTIFIER_EXPECTED, &[]);
        }

        (is_type_only, property_name, name)
    }

    fn parse_module_export_name(&mut self, disallow_keywords: bool) -> (Self::Node, bool) {
        let mut name_ok = true;

        if self.token() == Kind::StringLiteral {
            return (self.parse_literal_expression(), name_ok);
        }
        if disallow_keywords && ast::is_keyword(self.token()) && !self.is_identifier() {
            name_ok = false;
        }
        (self.parse_identifier_name(), name_ok)
    }

    fn try_parse_import_attributes(&mut self) -> Option<Self::Node> {
        if self.token() == Kind::WithKeyword
            || (self.token() == Kind::AssertKeyword && !self.has_preceding_line_break())
        {
            if self.token() == Kind::AssertKeyword {
                self.parse_error_at_current_token(
                    &diagnostics::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_ASSERT,
                    &[],
                );
            }
            return Some(self.parse_import_attributes(self.token(), false /*skipKeyword*/));
        }
        None
    }

    fn parse_export_assignment(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let save_context_flags = self.context_flags();
        let save_has_await_identifier = self.statement_has_await_identifier();
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let mut is_export_equals = false;
        if self.parse_optional(Kind::EqualsToken) {
            is_export_equals = true;
        } else {
            self.parse_expected(Kind::DefaultKeyword);
        }
        let expression = self.parse_assignment_expression_or_higher();
        self.parse_semicolon();
        self.replace_context_flags(save_context_flags);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        let node = self.new_export_assignment(
            modifiers,
            is_export_equals,
            None, /*typeNode*/
            expression,
        );
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_namespace_export_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        self.parse_expected(Kind::AsKeyword);
        self.parse_expected(Kind::NamespaceKeyword);
        let save_has_await_identifier = self.statement_has_await_identifier();
        let name = self.parse_identifier();
        self.set_statement_has_await_identifier(save_has_await_identifier);
        self.parse_semicolon();
        // NamespaceExportDeclaration nodes cannot have decorators or modifiers, we attach them here so we can report them in the grammar checker
        let node = self.new_namespace_export_declaration(modifiers, name);
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        result
    }

    fn parse_export_declaration(
        &mut self,
        pos: u32,
        jsdoc: JsdocScannerInfo,
        modifiers: Option<Self::Modifiers>,
    ) -> Self::Node {
        let save_context_flags = self.context_flags();
        let save_has_await_identifier = self.statement_has_await_identifier();
        self.set_context_flags(NodeFlags::AwaitContext, true);
        let mut export_clause: Option<Self::Node> = None;
        let mut module_specifier: Option<Self::Node> = None;
        let mut attributes: Option<Self::Node> = None;
        let is_type_only = self.parse_optional(Kind::TypeKeyword);
        let namespace_export_pos = self.node_pos();
        if self.parse_optional(Kind::AsteriskToken) {
            if self.parse_optional(Kind::AsKeyword) {
                export_clause = Some(self.parse_namespace_export(namespace_export_pos));
            }
            self.parse_expected(Kind::FromKeyword);
            module_specifier = Some(self.parse_module_specifier());
        } else {
            export_clause = Some(self.parse_named_exports());
            // It is not uncommon to accidentally omit the 'from' keyword. Additionally, in editing scenarios,
            // the 'from' keyword can be parsed as a named export when the export clause is unterminated (i.e. `export { from "moduleName";`)
            // If we don't have a 'from' keyword, see if we have a string literal such that ASI won't take effect.
            if self.token() == Kind::FromKeyword
                || (self.token() == Kind::StringLiteral && !self.has_preceding_line_break())
            {
                self.parse_expected(Kind::FromKeyword);
                module_specifier = Some(self.parse_module_specifier());
            }
        }
        if module_specifier.is_some()
            && (self.token() == Kind::WithKeyword || self.token() == Kind::AssertKeyword)
            && !self.has_preceding_line_break()
        {
            if self.token() == Kind::AssertKeyword {
                self.parse_error_at_current_token(
                    &diagnostics::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_ASSERT,
                    &[],
                );
            }
            attributes = Some(self.parse_import_attributes(self.token(), false /*skipKeyword*/));
        }
        self.parse_semicolon();
        self.replace_context_flags(save_context_flags);
        self.set_statement_has_await_identifier(save_has_await_identifier);
        let node = self.new_export_declaration(
            modifiers,
            is_type_only,
            export_clause,
            module_specifier,
            attributes,
        );
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_namespace_export(&mut self, pos: u32) -> Self::Node {
        let (export_name, _) = self.parse_module_export_name(false /*disallowKeywords*/);
        let node = self.new_namespace_export(export_name);
        self.finish_node(node, pos)
    }

    fn parse_named_exports(&mut self) -> Self::Node {
        let pos = self.node_pos();
        // NamedImports:
        //  { }
        //  { ImportsList }
        //  { ImportsList, }
        let exports = self
            .parse_bracketed_list(
                ParsingContext::ImportOrExportSpecifiers,
                Self::parse_export_specifier,
                Kind::OpenBraceToken,
                Kind::CloseBraceToken,
            )
            .unwrap();
        let node = self.new_named_exports(exports);
        self.finish_node(node, pos)
    }

    fn parse_export_specifier(&mut self) -> Self::Node {
        let pos = self.node_pos();
        let jsdoc = self.jsdoc_scanner_info();
        let (is_type_only, property_name, name) =
            self.parse_import_or_export_specifier(Kind::ExportSpecifier);
        let node = self.new_export_specifier(is_type_only, property_name, name);
        let result = self.finish_node(node, pos);
        self.with_jsdoc(result, jsdoc);
        self.check_js_syntax(result);
        result
    }

    fn parse_decorator_expression(&mut self) -> Self::Node {
        if self.in_await_context() && self.token() == Kind::AwaitKeyword {
            // `@await` is disallowed in an [Await] context, but can cause parsing to go off the rails
            // This simply parses the missing identifier and moves on.
            let pos = self.node_pos();
            let await_expression = self
                .parse_identifier_with_diagnostic(Some(&diagnostics::EXPRESSION_EXPECTED), None);
            self.next_token();
            let member_expression = self.parse_member_expression_rest(
                pos,
                await_expression,
                true, /*allowOptionalChain*/
            );
            return self.parse_call_expression_rest(pos, member_expression);
        }
        self.parse_left_hand_side_expression_or_higher()
    }
    fn modifier_list_has_kind(&self, modifiers: Self::Modifiers, kind: Kind) -> bool {
        (0..self.modifier_list_len(modifiers))
            .any(|index| self.node_kind(self.modifier_list_element(modifiers, index)) == kind)
    }
    fn modifier_flags_from_nodes(&self, modifiers: Self::Modifiers) -> ModifierFlags {
        (0..self.modifier_list_len(modifiers)).fold(ModifierFlags::None, |flags, index| {
            flags
                | ast::modifier_to_flag(
                    self.node_kind(self.modifier_list_element(modifiers, index)),
                )
        })
    }
    fn set_modifiers_ambient(&mut self, modifiers: Self::Modifiers) {
        for index in 0..self.modifier_list_len(modifiers) {
            let modifier = self.modifier_list_element(modifiers, index);
            self.set_node_flags(modifier, self.node_flags(modifier) | NodeFlags::Ambient);
        }
    }
    fn is_valid_heritage_type_reference_expression(&self, node: Self::Node) -> bool {
        if self.node_kind(node) == Kind::Identifier {
            return self.node_is_present(node);
        }
        self.node_kind(node) == Kind::PropertyAccessExpression
            && !self.node_flags(node).intersects(NodeFlags::OptionalChain)
            && self.node_name(node).is_some_and(|name| self.node_is_present(name))
            && self.is_valid_heritage_type_reference_expression(self.node_expression(node).unwrap())
    }
}

impl TypeParser<'static> for crate::Parser {
    type Modifiers = P<ModifierList>;
    fn node_kind(&self, node: P<Node>) -> Kind {
        node.kind()
    }
    fn node_range(&self, node: P<Node>) -> TextRange {
        node.loc()
    }
    fn is_start_of_function_type_or_constructor_type(&mut self) -> bool {
        Self::is_start_of_function_type_or_constructor_type(self)
    }
    fn re_scan_less_than_token(&mut self) -> Kind {
        Self::re_scan_less_than_token(self)
    }
    fn new_conditional_type_node(
        &mut self,
        check_type: P<Node>,
        extends_type: P<Node>,
        true_type: P<Node>,
        false_type: P<Node>,
    ) -> P<Node> {
        self.factory.new_conditional_type_node(check_type, extends_type, true_type, false_type)
    }
    fn new_union_type_node(&mut self, types: P<NodeList>) -> P<Node> {
        self.factory.new_union_type_node(types)
    }
    fn new_intersection_type_node(&mut self, types: P<NodeList>) -> P<Node> {
        self.factory.new_intersection_type_node(types)
    }
    fn new_type_operator_node(&mut self, operator: Kind, type_node: P<Node>) -> P<Node> {
        self.factory.new_type_operator_node(operator, type_node)
    }
    fn new_infer_type_node(&mut self, type_parameter: P<Node>) -> P<Node> {
        self.factory.new_infer_type_node(type_parameter)
    }
    fn new_type_parameter_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        constraint: Option<P<Node>>,
        expression: Option<P<Node>>,
        default_type: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_type_parameter_declaration(
            modifiers,
            name,
            constraint,
            expression,
            default_type,
        )
    }
    fn new_jsdoc_non_nullable_type(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_jsdoc_non_nullable_type(type_node)
    }
    fn new_jsdoc_nullable_type(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_jsdoc_nullable_type(type_node)
    }
    fn new_indexed_access_type_node(
        &mut self,
        object_type: P<Node>,
        index_type: P<Node>,
    ) -> P<Node> {
        self.factory.new_indexed_access_type_node(object_type, index_type)
    }
    fn new_array_type_node(&mut self, element_type: P<Node>) -> P<Node> {
        self.factory.new_array_type_node(element_type)
    }
    fn new_type_reference_node(
        &mut self,
        type_name: P<Node>,
        type_arguments: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_type_reference_node(type_name, type_arguments)
    }
    fn new_type_predicate_node(
        &mut self,
        asserts_modifier: Option<P<Node>>,
        parameter_name: P<Node>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_type_predicate_node(asserts_modifier, parameter_name, type_node)
    }
    fn new_type_query_node(
        &mut self,
        entity_name: P<Node>,
        type_arguments: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_type_query_node(entity_name, type_arguments)
    }
    fn new_parenthesized_type_node(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_parenthesized_type_node(type_node)
    }
    fn re_scan_asterisk_equals_token(&mut self) {
        self.scanner.re_scan_asterisk_equals_token();
        self.report_scan_errors();
    }
    fn re_scan_question_token(&mut self) {
        self.scanner.re_scan_question_token();
        self.report_scan_errors();
    }
    fn set_skip_jsdoc_leading_asterisks(&mut self, value: bool) {
        self.scanner.set_skip_jsdoc_leading_asterisks(value);
    }
    fn parse_member_list(
        &mut self,
        kind: ParsingContext,
        parse_element: fn(&mut Self) -> P<Node>,
    ) -> P<NodeList> {
        Self::parse_member_list(self, kind, parse_element)
    }
    fn set_lazy_owner(&mut self, members: P<NodeList>, owner: P<Node>) {
        Self::set_lazy_owner(members, owner);
    }
    fn jsdoc_scanner_info(&self) -> JsdocScannerInfo {
        Self::jsdoc_scanner_info(self)
    }
    fn with_jsdoc(&mut self, node: P<Node>, info: JsdocScannerInfo) {
        Self::with_jsdoc(self, node, info);
    }
    fn node_type(&self, node: P<Node>) -> Option<P<Node>> {
        node.type_node()
    }
    fn node_flags(&self, node: P<Node>) -> NodeFlags {
        node.flags()
    }
    fn set_node_flags(&mut self, node: P<Node>, flags: NodeFlags) {
        node.set_flags(flags);
    }
    fn set_node_range(&mut self, node: P<Node>, range: TextRange) {
        node.set_loc(range);
    }
    fn set_node_parent(&mut self, node: P<Node>, parent: Option<P<Node>>) {
        node.set_parent(parent);
    }
    fn template_type_span_literal(&self, span: P<Node>) -> P<Node> {
        span.as_template_literal_type_span().literal
    }
    fn new_modifier_list(&mut self, range: TextRange, nodes: &[P<Node>]) -> P<ModifierList> {
        Self::new_modifier_list(self, range, nodes)
    }
    fn new_mapped_type_node(
        &mut self,
        readonly_token: Option<P<Node>>,
        type_parameter: P<Node>,
        name_type: Option<P<Node>>,
        question_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        members: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_mapped_type_node(
            readonly_token,
            type_parameter,
            name_type,
            question_token,
            type_node,
            members,
        )
    }
    fn new_type_literal_node(&mut self, members: P<NodeList>) -> P<Node> {
        self.factory.new_type_literal_node(members)
    }
    fn new_tuple_type_node(&mut self, elements: P<NodeList>) -> P<Node> {
        self.factory.new_tuple_type_node(elements)
    }
    fn new_named_tuple_member(
        &mut self,
        dot_dot_dot_token: Option<P<Node>>,
        name: P<Node>,
        question_token: Option<P<Node>>,
        type_node: P<Node>,
    ) -> P<Node> {
        self.factory.new_named_tuple_member(dot_dot_dot_token, name, question_token, type_node)
    }
    fn new_rest_type_node(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_rest_type_node(type_node)
    }
    fn new_optional_type_node(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_optional_type_node(type_node)
    }
    fn new_template_literal_type_node(&mut self, head: P<Node>, spans: P<NodeList>) -> P<Node> {
        self.factory.new_template_literal_type_node(head, spans)
    }
    fn new_template_literal_type_span(&mut self, type_node: P<Node>, literal: P<Node>) -> P<Node> {
        self.factory.new_template_literal_type_span(type_node, literal)
    }
    fn new_jsdoc_variadic_type(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_jsdoc_variadic_type(type_node)
    }
    fn new_jsdoc_optional_type(&mut self, type_node: P<Node>) -> P<Node> {
        self.factory.new_jsdoc_optional_type(type_node)
    }
    fn new_function_type_node(
        &mut self,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_function_type_node(type_parameters, parameters, type_node)
    }
    fn new_constructor_type_node(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_constructor_type_node(modifiers, type_parameters, parameters, type_node)
    }
    fn new_modifier(&mut self, kind: Kind) -> P<Node> {
        self.factory.new_modifier(kind)
    }
    fn modifier_first_range(&self, modifiers: P<ModifierList>) -> TextRange {
        modifiers.nodes()[0].loc()
    }
    fn check_js_syntax(&mut self, node: P<Node>) -> P<Node> {
        Self::check_js_syntax(self, node)
    }
    fn create_identifier(&mut self, is_identifier: bool) -> P<Node> {
        Self::create_identifier(self, is_identifier)
    }
    fn finish_modifier_list(&mut self, start: usize, range: TextRange) -> P<ModifierList> {
        let list = self.factory.new_modifier_list_from_slice(&self.node_stack[start..]);
        list.list.loc.set(range);
        self.node_stack.truncate(start);
        list
    }
    fn new_call_signature_declaration(
        &mut self,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_call_signature_declaration(type_parameters, parameters, type_node)
    }
    fn new_construct_signature_declaration(
        &mut self,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_construct_signature_declaration(type_parameters, parameters, type_node)
    }
    fn new_parameter_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        dot_dot_dot_token: Option<P<Node>>,
        name: P<Node>,
        question_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_parameter_declaration(
            modifiers,
            dot_dot_dot_token,
            name,
            question_token,
            type_node,
            initializer,
        )
    }
    fn new_get_accessor_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_get_accessor_declaration(
            modifiers,
            name,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn new_set_accessor_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_set_accessor_declaration(
            modifiers,
            name,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn new_computed_property_name(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_computed_property_name(expression)
    }
    fn new_index_signature_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_index_signature_declaration(modifiers, parameters, type_node)
    }
    fn new_method_signature_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        question_token: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_method_signature_declaration(
            modifiers,
            name,
            question_token,
            type_parameters,
            parameters,
            type_node,
        )
    }
    fn new_property_signature_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        question_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_property_signature_declaration(
            modifiers,
            name,
            question_token,
            type_node,
            initializer,
        )
    }
    fn new_decorator(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_decorator(expression)
    }
    fn add_source_flags(&mut self, flags: NodeFlags) {
        self.source_flags |= flags;
    }
    fn last_expected_diagnostic(&self) -> Option<Self::Diagnostic> {
        self.diagnostics.last().copied().filter(|d| d.code() == diagnostics::X_0_EXPECTED.code())
    }
    fn language_variant(&self) -> LanguageVariant {
        self.language_variant
    }
    fn node_is_left_hand_side_expression(&self, node: P<Node>) -> bool {
        ast::is_left_hand_side_expression(node)
    }
    fn node_is_present(&self, node: P<Node>) -> bool {
        ast::node_is_present(Some(node))
    }
    fn binary_operator_kind(&self, node: P<Node>) -> Kind {
        node.as_binary_expression().operator_token.kind()
    }
    fn re_scan_greater_than_token(&mut self) -> Kind {
        Self::re_scan_greater_than_token(self)
    }
    fn skip_expression_trivia(&self, pos: u32) -> u32 {
        scanner::skip_trivia(self.source_text, pos)
    }
    fn new_import_type_node(
        &mut self,
        is_type_of: bool,
        type_node: P<Node>,
        attributes: Option<P<Node>>,
        qualifier: Option<P<Node>>,
        type_arguments: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_import_type_node(
            is_type_of,
            type_node,
            attributes,
            qualifier,
            type_arguments,
        )
    }
    fn new_import_attribute(&mut self, name: Option<P<Node>>, value: P<Node>) -> P<Node> {
        self.factory.new_import_attribute(name, value)
    }
    fn new_import_attributes(
        &mut self,
        token: Kind,
        elements: P<NodeList>,
        multi_line: bool,
    ) -> P<Node> {
        self.factory.new_import_attributes(token, elements, multi_line)
    }
    fn new_yield_expression(
        &mut self,
        asterisk_token: Option<P<Node>>,
        expression: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_yield_expression(asterisk_token, expression)
    }
    fn new_conditional_expression(
        &mut self,
        condition: P<Node>,
        question_token: P<Node>,
        when_true: P<Node>,
        colon_token: P<Node>,
        when_false: P<Node>,
    ) -> P<Node> {
        self.factory.new_conditional_expression(
            condition,
            question_token,
            when_true,
            colon_token,
            when_false,
        )
    }
    fn new_satisfies_expression(&mut self, expression: P<Node>, type_node: P<Node>) -> P<Node> {
        self.factory.new_satisfies_expression(expression, type_node)
    }
    fn new_as_expression(&mut self, expression: P<Node>, type_node: P<Node>) -> P<Node> {
        self.factory.new_as_expression(expression, type_node)
    }
    fn new_binary_expression(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        left: P<Node>,
        type_node: Option<P<Node>>,
        operator_token: P<Node>,
        right: P<Node>,
    ) -> P<Node> {
        self.factory.new_binary_expression(modifiers, left, type_node, operator_token, right)
    }
    fn new_postfix_unary_expression(&mut self, operand: P<Node>, operator: Kind) -> P<Node> {
        self.factory.new_postfix_unary_expression(operand, operator)
    }
    fn new_delete_expression(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_delete_expression(expression)
    }
    fn new_type_of_expression(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_type_of_expression(expression)
    }
    fn new_void_expression(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_void_expression(expression)
    }
    fn new_await_expression(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_await_expression(expression)
    }
    fn new_type_assertion(&mut self, type_node: P<Node>, expression: P<Node>) -> P<Node> {
        self.factory.new_type_assertion(type_node, expression)
    }
    fn modifier_list_has_async(&self, modifiers: Option<P<ModifierList>>) -> bool {
        crate::parser_1::modifier_list_has_async(modifiers)
    }
    fn not_parenthesized_arrow_contains(&self, pos: u32) -> bool {
        self.not_parenthesized_arrow.contains(&pos)
    }
    fn remember_not_parenthesized_arrow(&mut self, pos: u32) {
        self.not_parenthesized_arrow.insert(pos);
    }
    fn node_expression(&self, node: P<Node>) -> Option<P<Node>> {
        node.expression()
    }
    fn node_type_arguments(&self, node: P<Node>) -> Option<P<NodeList>> {
        node.type_argument_list()
    }
    fn expression_with_type_arguments_parts(
        &self,
        node: P<Node>,
    ) -> (P<Node>, Option<P<NodeList>>) {
        let data = node.as_expression_with_type_arguments();
        (data.expression, data.type_arguments.get())
    }
    fn template_span_literal(&self, node: P<Node>) -> P<Node> {
        node.as_template_span().literal
    }
    fn list_len(&self, list: P<NodeList>) -> usize {
        list.nodes().len()
    }
    fn list_element(&self, list: P<NodeList>, index: usize) -> P<Node> {
        list.nodes()[index]
    }
    fn list_range(&self, list: P<NodeList>) -> TextRange {
        list.loc.get()
    }
    fn node_is_import_phase_meta_property(&self, node: P<Node>) -> bool {
        ast::is_import_phase_meta_property(node)
    }
    fn expression_source_text(&self, node: P<Node>) -> String {
        scanner::get_text_of_node_from_source_text(self.source_text, node, false)
    }
    fn skip_expression_range_trivia(&self, range: TextRange) -> TextRange {
        Self::skip_range_trivia(self, range)
    }
    fn re_scan_slash_token(&mut self) -> Kind {
        Self::re_scan_slash_token(self)
    }
    fn scan_jsx_text(&mut self) -> Kind {
        Self::scan_jsx_text(self)
    }
    fn scan_jsx_identifier(&mut self) -> Kind {
        Self::scan_jsx_identifier(self)
    }
    fn scan_jsx_attribute_value(&mut self) -> Kind {
        Self::scan_jsx_attribute_value(self)
    }
    fn re_scan_jsx_token(&mut self, allow_multiline: bool) -> Kind {
        let token = self.scanner.re_scan_jsx_token(allow_multiline);
        self.report_scan_errors();
        token
    }
    fn jsx_element_parts(&self, node: P<Node>) -> (P<Node>, P<NodeList>, P<Node>) {
        let data = node.as_jsx_element();
        (data.opening_element, data.children, data.closing_element)
    }
    fn jsx_tag_name(&self, node: P<Node>) -> P<Node> {
        node.tag_name()
    }
    fn jsx_tag_names_are_equivalent(&self, left: P<Node>, right: P<Node>) -> bool {
        ast::tag_names_are_equivalent(left, right)
    }
    fn new_scanned_jsx_text(
        &mut self,
        text: crate::lexical::ScannedText<
            <Self::TextPolicy as tsrs_scanner::ScannerText<'static>>::Text,
        >,
        whitespace: bool,
    ) -> P<Node> {
        self.factory.new_jsx_text(text.value, whitespace)
    }
    fn type_reference_name(&self, node: P<Node>) -> P<Node> {
        node.as_type_reference_node().type_name
    }
    fn function_parameters(&self, node: P<Node>) -> Option<P<NodeList>> {
        node.function_like_data().unwrap().parameters.get()
    }
    fn list_is_missing(&self, list: Option<P<NodeList>>) -> bool {
        crate::parser_1::is_missing_node_list(list)
    }
    fn node_is_missing(&self, node: P<Node>) -> bool {
        ast::node_is_missing(Some(node))
    }
    fn new_arrow_function(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        equals_greater_than_token: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_arrow_function(
            modifiers,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            equals_greater_than_token,
            body,
        )
    }
    fn new_jsx_closing_element(&mut self, tag_name: P<Node>) -> P<Node> {
        self.factory.new_jsx_closing_element(tag_name)
    }
    fn new_jsx_element(
        &mut self,
        opening_element: P<Node>,
        children: P<NodeList>,
        closing_element: P<Node>,
    ) -> P<Node> {
        self.factory.new_jsx_element(opening_element, children, closing_element)
    }
    fn new_jsx_fragment(
        &mut self,
        opening_fragment: P<Node>,
        children: P<NodeList>,
        closing_fragment: P<Node>,
    ) -> P<Node> {
        self.factory.new_jsx_fragment(opening_fragment, children, closing_fragment)
    }
    fn new_jsx_expression(
        &mut self,
        dot_dot_dot_token: Option<P<Node>>,
        expression: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_jsx_expression(dot_dot_dot_token, expression)
    }
    fn new_jsx_opening_fragment(&mut self) -> P<Node> {
        self.factory.new_jsx_opening_fragment()
    }
    fn new_jsx_opening_element(
        &mut self,
        tag_name: P<Node>,
        type_arguments: Option<P<NodeList>>,
        attributes: P<Node>,
    ) -> P<Node> {
        self.factory.new_jsx_opening_element(tag_name, type_arguments, attributes)
    }
    fn new_jsx_self_closing_element(
        &mut self,
        tag_name: P<Node>,
        type_arguments: Option<P<NodeList>>,
        attributes: P<Node>,
    ) -> P<Node> {
        self.factory.new_jsx_self_closing_element(tag_name, type_arguments, attributes)
    }
    fn new_property_access_expression(
        &mut self,
        expression: P<Node>,
        question_dot_token: Option<P<Node>>,
        name: P<Node>,
        flags: NodeFlags,
    ) -> P<Node> {
        self.factory.new_property_access_expression(expression, question_dot_token, name, flags)
    }
    fn new_jsx_namespaced_name(&mut self, namespace: P<Node>, name: P<Node>) -> P<Node> {
        self.factory.new_jsx_namespaced_name(namespace, name)
    }
    fn new_jsx_attributes(&mut self, properties: P<NodeList>) -> P<Node> {
        self.factory.new_jsx_attributes(properties)
    }
    fn new_jsx_attribute(&mut self, name: P<Node>, initializer: Option<P<Node>>) -> P<Node> {
        self.factory.new_jsx_attribute(name, initializer)
    }
    fn new_jsx_spread_attribute(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_jsx_spread_attribute(expression)
    }
    fn new_jsx_closing_fragment(&mut self) -> P<Node> {
        self.factory.new_jsx_closing_fragment()
    }
    fn new_meta_property(&mut self, keyword_token: Kind, name: P<Node>) -> P<Node> {
        self.factory.new_meta_property(keyword_token, name)
    }
    fn new_expression_with_type_arguments(
        &mut self,
        expression: P<Node>,
        type_arguments: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_expression_with_type_arguments(expression, type_arguments)
    }
    fn new_non_null_expression(&mut self, expression: P<Node>, flags: NodeFlags) -> P<Node> {
        self.factory.new_non_null_expression(expression, flags)
    }
    fn new_element_access_expression(
        &mut self,
        expression: P<Node>,
        question_dot_token: Option<P<Node>>,
        argument_expression: P<Node>,
        flags: NodeFlags,
    ) -> P<Node> {
        self.factory.new_element_access_expression(
            expression,
            question_dot_token,
            argument_expression,
            flags,
        )
    }
    fn new_call_expression(
        &mut self,
        expression: P<Node>,
        question_dot_token: Option<P<Node>>,
        type_arguments: Option<P<NodeList>>,
        arguments: P<NodeList>,
        flags: NodeFlags,
    ) -> P<Node> {
        self.factory.new_call_expression(
            expression,
            question_dot_token,
            type_arguments,
            arguments,
            flags,
        )
    }
    fn new_omitted_expression(&mut self) -> P<Node> {
        self.factory.new_omitted_expression()
    }
    fn new_spread_element(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_spread_element(expression)
    }
    fn new_tagged_template_expression(
        &mut self,
        tag: P<Node>,
        question_dot_token: Option<P<Node>>,
        type_arguments: Option<P<NodeList>>,
        template: P<Node>,
        flags: NodeFlags,
    ) -> P<Node> {
        self.factory.new_tagged_template_expression(
            tag,
            question_dot_token,
            type_arguments,
            template,
            flags,
        )
    }
    fn new_template_expression(&mut self, head: P<Node>, template_spans: P<NodeList>) -> P<Node> {
        self.factory.new_template_expression(head, template_spans)
    }
    fn new_template_span(&mut self, expression: P<Node>, literal: P<Node>) -> P<Node> {
        self.factory.new_template_span(expression, literal)
    }
    fn new_parenthesized_expression(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_parenthesized_expression(expression)
    }
    fn new_array_literal_expression(&mut self, elements: P<NodeList>, multi_line: bool) -> P<Node> {
        self.factory.new_array_literal_expression(elements, multi_line)
    }
    fn new_object_literal_expression(
        &mut self,
        properties: P<NodeList>,
        multi_line: bool,
    ) -> P<Node> {
        self.factory.new_object_literal_expression(properties, multi_line)
    }
    fn new_spread_assignment(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_spread_assignment(expression)
    }
    fn new_shorthand_property_assignment(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        postfix_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        equals_token: Option<P<Node>>,
        object_assignment_initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_shorthand_property_assignment(
            modifiers,
            name,
            postfix_token,
            type_node,
            equals_token,
            object_assignment_initializer,
        )
    }
    fn new_property_assignment(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        postfix_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        initializer: P<Node>,
    ) -> P<Node> {
        self.factory.new_property_assignment(modifiers, name, postfix_token, type_node, initializer)
    }
    fn new_function_expression(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        asterisk_token: Option<P<Node>>,
        name: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_function_expression(
            modifiers,
            asterisk_token,
            name,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn new_missing_declaration(&mut self, modifiers: Option<P<ModifierList>>) -> P<Node> {
        self.factory.new_missing_declaration(modifiers)
    }
    fn new_new_expression(
        &mut self,
        expression: P<Node>,
        type_arguments: Option<P<NodeList>>,
        arguments: Option<P<NodeList>>,
    ) -> P<Node> {
        self.factory.new_new_expression(expression, type_arguments, arguments)
    }
    fn new_method_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        asterisk_token: Option<P<Node>>,
        name: P<Node>,
        postfix_token: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_method_declaration(
            modifiers,
            asterisk_token,
            name,
            postfix_token,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn finish_node_with_end(&mut self, node: P<Node>, pos: u32, end: u32) -> P<Node> {
        Self::finish_node_with_end(self, node, pos, end)
    }
    fn current_import_phase_modifier(&self) -> Kind {
        Self::current_import_phase_modifier(self)
    }
    fn modifier_flags(&self, modifiers: P<ModifierList>) -> ModifierFlags {
        modifiers.modifier_flags
    }
    fn modifier_list_len(&self, modifiers: P<ModifierList>) -> usize {
        modifiers.nodes().len()
    }
    fn modifier_list_element(&self, modifiers: P<ModifierList>, index: usize) -> P<Node> {
        modifiers.nodes()[index]
    }
    fn node_text(&self, node: P<Node>) -> &str {
        node.text()
    }
    fn node_name(&self, node: P<Node>) -> Option<P<Node>> {
        node.name()
    }
    fn tagged_template(&self, node: P<Node>) -> P<Node> {
        node.as_tagged_template_expression().template()
    }
    fn heritage_clause_parts(&self, node: P<Node>) -> (Kind, P<NodeList>) {
        let clause = node.as_heritage_clause();
        (clause.token(), clause.types())
    }
    fn current_token_value_is(&self, text: &str) -> bool {
        self.scanner.token_value() == text
    }
    fn error_at_current_token_value(&mut self, message: &'static Message) {
        let value = self.scanner.token_value();
        self.parse_error_at_current_token(message, &[&value]);
    }
    fn new_block(&mut self, statements: P<NodeList>, multi_line: bool) -> P<Node> {
        self.factory.new_block(statements, multi_line)
    }
    fn new_empty_statement(&mut self) -> P<Node> {
        self.factory.new_empty_statement()
    }
    fn new_if_statement(
        &mut self,
        expression: P<Node>,
        then_statement: P<Node>,
        else_statement: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_if_statement(expression, then_statement, else_statement)
    }
    fn new_do_statement(&mut self, statement: P<Node>, expression: P<Node>) -> P<Node> {
        self.factory.new_do_statement(statement, expression)
    }
    fn new_while_statement(&mut self, expression: P<Node>, statement: P<Node>) -> P<Node> {
        self.factory.new_while_statement(expression, statement)
    }
    fn new_for_in_or_of_statement(
        &mut self,
        kind: Kind,
        await_modifier: Option<P<Node>>,
        initializer: P<Node>,
        expression: P<Node>,
        statement: P<Node>,
    ) -> P<Node> {
        self.factory.new_for_in_or_of_statement(
            kind,
            await_modifier,
            initializer,
            expression,
            statement,
        )
    }
    fn new_for_statement(
        &mut self,
        initializer: Option<P<Node>>,
        condition: Option<P<Node>>,
        incrementor: Option<P<Node>>,
        statement: P<Node>,
    ) -> P<Node> {
        self.factory.new_for_statement(initializer, condition, incrementor, statement)
    }
    fn new_break_statement(&mut self, label: Option<P<Node>>) -> P<Node> {
        self.factory.new_break_statement(label)
    }
    fn new_continue_statement(&mut self, label: Option<P<Node>>) -> P<Node> {
        self.factory.new_continue_statement(label)
    }
    fn new_return_statement(&mut self, expression: Option<P<Node>>) -> P<Node> {
        self.factory.new_return_statement(expression)
    }
    fn new_with_statement(&mut self, expression: P<Node>, statement: P<Node>) -> P<Node> {
        self.factory.new_with_statement(expression, statement)
    }
    fn new_case_or_default_clause(
        &mut self,
        kind: Kind,
        expression: Option<P<Node>>,
        statements: P<NodeList>,
    ) -> P<Node> {
        self.factory.new_case_or_default_clause(kind, expression, statements)
    }
    fn new_case_block(&mut self, clauses: P<NodeList>) -> P<Node> {
        self.factory.new_case_block(clauses)
    }
    fn new_switch_statement(&mut self, expression: P<Node>, case_block: P<Node>) -> P<Node> {
        self.factory.new_switch_statement(expression, case_block)
    }
    fn new_throw_statement(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_throw_statement(expression)
    }
    fn new_try_statement(
        &mut self,
        try_block: P<Node>,
        catch_clause: Option<P<Node>>,
        finally_block: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_try_statement(try_block, catch_clause, finally_block)
    }
    fn new_catch_clause(
        &mut self,
        variable_declaration: Option<P<Node>>,
        block: P<Node>,
    ) -> P<Node> {
        self.factory.new_catch_clause(variable_declaration, block)
    }
    fn new_debugger_statement(&mut self) -> P<Node> {
        self.factory.new_debugger_statement()
    }
    fn new_labeled_statement(&mut self, label: P<Node>, statement: P<Node>) -> P<Node> {
        self.factory.new_labeled_statement(label, statement)
    }
    fn new_expression_statement(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_expression_statement(expression)
    }
    fn new_variable_statement(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        declaration_list: P<Node>,
    ) -> P<Node> {
        self.factory.new_variable_statement(modifiers, declaration_list)
    }
    fn new_variable_declaration_list(
        &mut self,
        declarations: P<NodeList>,
        flags: NodeFlags,
    ) -> P<Node> {
        self.factory.new_variable_declaration_list(declarations, flags)
    }
    fn new_variable_declaration(
        &mut self,
        name: P<Node>,
        exclamation_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_variable_declaration(name, exclamation_token, type_node, initializer)
    }
    fn new_binding_pattern(&mut self, kind: Kind, elements: P<NodeList>) -> P<Node> {
        self.factory.new_binding_pattern(kind, elements)
    }
    fn new_binding_element(
        &mut self,
        dot_dot_dot_token: Option<P<Node>>,
        property_name: Option<P<Node>>,
        name: Option<P<Node>>,
        initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_binding_element(dot_dot_dot_token, property_name, name, initializer)
    }
    fn new_function_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        asterisk_token: Option<P<Node>>,
        name: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_function_declaration(
            modifiers,
            asterisk_token,
            name,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn new_class_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        heritage_clauses: Option<P<NodeList>>,
        members: P<NodeList>,
    ) -> P<Node> {
        self.factory.new_class_declaration(
            modifiers,
            name,
            type_parameters,
            heritage_clauses,
            members,
        )
    }
    fn new_class_expression(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: Option<P<Node>>,
        type_parameters: Option<P<NodeList>>,
        heritage_clauses: Option<P<NodeList>>,
        members: P<NodeList>,
    ) -> P<Node> {
        self.factory.new_class_expression(
            modifiers,
            name,
            type_parameters,
            heritage_clauses,
            members,
        )
    }
    fn new_heritage_clause(&mut self, token: Kind, types: P<NodeList>) -> P<Node> {
        self.factory.new_heritage_clause(token, types)
    }
    fn new_semicolon_class_element(&mut self) -> P<Node> {
        self.factory.new_semicolon_class_element()
    }
    fn new_class_static_block_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        body: P<Node>,
    ) -> P<Node> {
        self.factory.new_class_static_block_declaration(modifiers, body)
    }
    fn new_constructor_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        type_parameters: Option<P<NodeList>>,
        parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
        full_signature: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_constructor_declaration(
            modifiers,
            type_parameters,
            parameters,
            type_node,
            full_signature,
            body,
        )
    }
    fn new_property_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        postfix_token: Option<P<Node>>,
        type_node: Option<P<Node>>,
        initializer: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_property_declaration(
            modifiers,
            name,
            postfix_token,
            type_node,
            initializer,
        )
    }
    fn new_interface_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        type_parameters: Option<P<NodeList>>,
        heritage_clauses: Option<P<NodeList>>,
        members: P<NodeList>,
    ) -> P<Node> {
        self.factory.new_interface_declaration(
            modifiers,
            name,
            type_parameters,
            heritage_clauses,
            members,
        )
    }
    fn new_type_alias_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        type_parameters: Option<P<NodeList>>,
        type_node: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_type_alias_declaration(modifiers, name, type_parameters, type_node)
    }
    fn new_enum_member(&mut self, name: P<Node>, initializer: Option<P<Node>>) -> P<Node> {
        self.factory.new_enum_member(name, initializer)
    }
    fn new_enum_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
        members: P<NodeList>,
    ) -> P<Node> {
        self.factory.new_enum_declaration(modifiers, name, members)
    }
    fn new_module_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        keyword: Kind,
        name: P<Node>,
        attributes: Option<P<Node>>,
        body: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_module_declaration(modifiers, keyword, name, attributes, body)
    }
    fn new_module_block(&mut self, statements: P<NodeList>) -> P<Node> {
        self.factory.new_module_block(statements)
    }
    fn new_import_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        import_clause: Option<P<Node>>,
        module_specifier: P<Node>,
        attributes: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_import_declaration(modifiers, import_clause, module_specifier, attributes)
    }
    fn new_import_equals_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        is_type_only: bool,
        name: P<Node>,
        module_reference: P<Node>,
    ) -> P<Node> {
        self.factory.new_import_equals_declaration(modifiers, is_type_only, name, module_reference)
    }
    fn new_external_module_reference(&mut self, expression: P<Node>) -> P<Node> {
        self.factory.new_external_module_reference(expression)
    }
    fn new_import_clause(
        &mut self,
        phase_modifier: Kind,
        name: Option<P<Node>>,
        named_bindings: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_import_clause(phase_modifier, name, named_bindings)
    }
    fn new_namespace_import(&mut self, name: P<Node>) -> P<Node> {
        self.factory.new_namespace_import(name)
    }
    fn new_named_imports(&mut self, elements: P<NodeList>) -> P<Node> {
        self.factory.new_named_imports(elements)
    }
    fn new_import_specifier(
        &mut self,
        is_type_only: bool,
        property_name: Option<P<Node>>,
        name: P<Node>,
    ) -> P<Node> {
        self.factory.new_import_specifier(is_type_only, property_name, name)
    }
    fn new_export_assignment(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        is_export_equals: bool,
        type_node: Option<P<Node>>,
        expression: P<Node>,
    ) -> P<Node> {
        self.factory.new_export_assignment(modifiers, is_export_equals, type_node, expression)
    }
    fn new_namespace_export_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        name: P<Node>,
    ) -> P<Node> {
        self.factory.new_namespace_export_declaration(modifiers, name)
    }
    fn new_export_declaration(
        &mut self,
        modifiers: Option<P<ModifierList>>,
        is_type_only: bool,
        export_clause: Option<P<Node>>,
        module_specifier: Option<P<Node>>,
        attributes: Option<P<Node>>,
    ) -> P<Node> {
        self.factory.new_export_declaration(
            modifiers,
            is_type_only,
            export_clause,
            module_specifier,
            attributes,
        )
    }
    fn new_namespace_export(&mut self, name: P<Node>) -> P<Node> {
        self.factory.new_namespace_export(name)
    }
    fn new_named_exports(&mut self, elements: P<NodeList>) -> P<Node> {
        self.factory.new_named_exports(elements)
    }
    fn new_export_specifier(
        &mut self,
        is_type_only: bool,
        property_name: Option<P<Node>>,
        name: P<Node>,
    ) -> P<Node> {
        self.factory.new_export_specifier(is_type_only, property_name, name)
    }
}

// shouldConsumeBinaryOperator reports whether an operator binds before the operator represented by currentPrecedence.
// At equal precedence, only the right-associative exponentiation operator binds first.
fn should_consume_binary_operator(
    operator: Kind,
    operator_precedence: OperatorPrecedence,
    current_precedence: OperatorPrecedence,
) -> bool {
    if operator_precedence > current_precedence {
        return true;
    }
    operator_precedence == current_precedence && operator == Kind::AsteriskAsteriskToken
}
