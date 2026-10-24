//! Experimental owned parser context. Lexical, token-recovery and list grammar
//! are shared with production; the complete source-file grammar is not connected yet.

#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;

use tsrs_ast::storage::{
    AstScope, AstStoreBuilder, AstStoreError, ListKey, NodeKey, RollbackBuffers, ScopedList,
    ScopedNode, TextInput,
};
use tsrs_ast::{Kind, NodeFlags, TokenFlags};
use tsrs_core::{ScriptKind, TextRange};
use tsrs_diagnostics::Message;
use tsrs_scanner::{OwnedScanner, OwnedScannerState, OwnedScannerText};

use crate::lexical::{LexicalParser, ScannedText};
use crate::template_tokens::TemplateTokenParser;
use crate::tokens::TokenParser;
use crate::utilities::get_language_variant;

/// Parser errors own their arguments and never borrow an arena diagnostic/file.
pub struct OwnedParseDiagnostic {
    pub range: TextRange,
    pub message: &'static Message,
    pub args: Vec<String>,
    pub related_information: Vec<OwnedParseDiagnostic>,
}

/// Snapshot owns its cooked token value; AST identities and counters are not rewound.
pub struct OwnedParserState<'src> {
    scanner: OwnedScannerState<'src>,
    context_flags: NodeFlags,
    diagnostics_len: usize,
    statement_has_await_identifier: bool,
    has_parse_error: bool,
}

/// The scanner borrows caller-retained source; the builder retains an Arc clone.
/// Moving either the context or the builder cannot invalidate a source slice.
pub struct OwnedParser<'src> {
    pub state: OwnedParserData<'src>,
    pub ast: AstStoreBuilder,
    pub node_stack: Vec<NodeKey>,
}

pub struct OwnedParserData<'src> {
    pub scanner: OwnedScanner<'src>,
    pub token: Kind,
    pub context_flags: NodeFlags,
    pub has_parse_error: bool,
    pub identifier_count: usize,
    pub statement_has_await_identifier: bool,
    pub diagnostics: Vec<OwnedParseDiagnostic>,
    pub parsing_contexts: u32,
    pub reparse_list: Vec<NodeKey>,
}

/// Exclusive construction access for speculative grammar. Parser state, scoped
/// handles and the borrowed storage writer have independent lifetimes.
///
/// Handles become stable only through a successful conditional parse or retention.
pub struct OwnedParserScope<'src, 'parser, 'brand, 'writer> {
    pub state: &'parser mut OwnedParserData<'src>,
    ast: AstScope<'brand, 'writer>,
    pub node_stack: Vec<ScopedNode<'brand>>,
    reparse_list: Vec<ScopedNode<'brand>>,
    saved_state: Option<OwnedParserState<'src>>,
    saved_parsing_contexts: u32,
}

impl<'src> OwnedParser<'src> {
    /// Starts at the first token, with the same language/context flags as parsing
    /// a source file. The caller keeps `source` alive until this context is consumed.
    pub fn new(source: &'src Arc<str>, script_kind: ScriptKind) -> Result<Self, AstStoreError> {
        assert!(script_kind != ScriptKind::Unknown, "ScriptKind must be specified");
        let mut scanner = OwnedScanner::new();
        scanner.set_text(source);
        scanner.set_on_error(true);
        scanner.set_language_variant(get_language_variant(script_kind));
        let mut parser = Self {
            state: OwnedParserData {
                scanner,
                token: Kind::Unknown,
                context_flags: match script_kind {
                    ScriptKind::JS | ScriptKind::JSX => NodeFlags::JavaScriptFile,
                    ScriptKind::JSON => NodeFlags::JavaScriptFile | NodeFlags::JsonFile,
                    _ => NodeFlags::None,
                },
                has_parse_error: false,
                identifier_count: 0,
                statement_has_await_identifier: false,
                diagnostics: Vec::new(),
                parsing_contexts: 0,
                reparse_list: Vec::new(),
            },
            ast: AstStoreBuilder::new(Arc::clone(source))?,
            node_stack: Vec::new(),
        };
        parser.next_token();
        Ok(parser)
    }

    pub fn ast(&self) -> &AstStoreBuilder {
        &self.ast
    }

    /// Returns exclusive construction storage for subsequent grammar/binding.
    /// No scanner view can escape this consuming handoff.
    pub fn into_parts(self) -> (AstStoreBuilder, Vec<OwnedParseDiagnostic>) {
        (self.ast, self.state.diagnostics)
    }

    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        reason = "The shared backend hook flushes scoped reparse handles; root handles are already stable."
    )]
    fn prepare_speculation(&mut self) {}
    pub fn reparse_nodes(&mut self) -> &mut Vec<NodeKey> {
        &mut self.state.reparse_list
    }
}

impl AsMut<Self> for OwnedParserData<'_> {
    fn as_mut(&mut self) -> &mut Self {
        self
    }
}

impl<'src> OwnedParserData<'src> {
    fn template_raw_range(&self, raw_text: &str) -> TextRange {
        let start = self.scanner.token_start() + 1;
        TextRange::new(start, start + u32::try_from(raw_text.len()).expect("source text capacity"))
    }
    fn report_scan_errors(&mut self) {
        for error in self.scanner.take_errors() {
            let args: Vec<&dyn Display> =
                error.args.iter().map(|arg| arg as &dyn Display).collect();
            self.parse_error_at_range(
                TextRange::new(error.start, error.start + error.length),
                error.message,
                &args,
            );
        }
    }

    fn count_identifier(&mut self, text: &str) {
        self.identifier_count += 1;
        if text == "await" {
            self.statement_has_await_identifier = true;
        }
    }

    #[cold]
    pub fn parse_error_at_range(
        &mut self,
        range: TextRange,
        message: &'static Message,
        args: &[&dyn Display],
    ) -> Option<usize> {
        let mut result = None;
        if self.diagnostics.last().is_none_or(|last| last.range.source_pos() != range.source_pos())
        {
            result = Some(self.diagnostics.len());
            self.diagnostics.push(OwnedParseDiagnostic {
                range,
                message,
                args: args.iter().map(ToString::to_string).collect(),
                related_information: Vec::new(),
            });
        }
        self.has_parse_error = true;
        result
    }

    fn mark(&self) -> OwnedParserState<'src> {
        OwnedParserState {
            scanner: self.scanner.mark(),
            context_flags: self.context_flags,
            diagnostics_len: self.diagnostics.len(),
            statement_has_await_identifier: self.statement_has_await_identifier,
            has_parse_error: self.has_parse_error,
        }
    }
    fn rewind(&mut self, state: OwnedParserState<'src>) {
        self.scanner.rewind(state.scanner);
        self.token = self.scanner.token();
        self.context_flags = state.context_flags;
        self.diagnostics.truncate(state.diagnostics_len);
        self.statement_has_await_identifier = state.statement_has_await_identifier;
        self.has_parse_error = state.has_parse_error;
    }
}
impl<T: AsRef<str>> ScannedText<T> {
    fn input(&self) -> TextInput<'_> {
        match self.source_range {
            Some(range) => TextInput::Source(range),
            None => TextInput::Owned(self.value.as_ref()),
        }
    }
}

impl<'brand, 'writer> OwnedParserScope<'_, '_, 'brand, 'writer> {
    pub fn ast(&self) -> &AstScope<'brand, 'writer> {
        &self.ast
    }
    /// Explicit outward escape, matching the production reparse retention barrier.
    pub fn retain_node(&mut self, node: ScopedNode<'brand>) -> NodeKey {
        self.ast.retain_node(node)
    }
    fn prepare_speculation(&mut self) {
        self.flush_reparse();
    }
    fn flush_reparse(&mut self) {
        for node in self.reparse_list.drain(..) {
            self.state.reparse_list.push(self.ast.retain_node(node));
        }
    }
    fn reparse_nodes(&mut self) -> &mut Vec<ScopedNode<'brand>> {
        self.reparse_list
            .extend(self.state.reparse_list.drain(..).map(|node| self.ast.import_node(node)));
        &mut self.reparse_list
    }
}
impl Drop for OwnedParserScope<'_, '_, '_, '_> {
    fn drop(&mut self) {
        self.flush_reparse();
        if let Some(saved) = self.saved_state.take() {
            self.state.rewind(saved);
        }
        self.state.parsing_contexts = self.saved_parsing_contexts;
    }
}

// Backend hooks differ only in handle/storage types. The grammar stays in the
// shared lexical, token, recognition, context, list and type traits.
macro_rules! owned_parser_impl {
    ($parser:ty, [$($scope:lifetime),*], $node:ty, $list:ty) => {
        impl<'src $(, $scope)*> $parser {
            pub fn new_array_literal_expression(&mut self, elements: $list) -> $node {
                self.ast.new_array_literal_expression(
                    NodeFlags::None, TextRange::synthetic(), elements, false,
                ).expect("array literal capacity")
            }
            pub fn token(&self) -> Kind {
                self.state.token
            }
            pub fn diagnostics(&self) -> &[OwnedParseDiagnostic] {
                &self.state.diagnostics
            }
            pub fn identifier_count(&self) -> usize {
                self.state.identifier_count
            }
            pub fn next_token(&mut self) -> Kind {
                LexicalParser::next_token(self)
            }
            pub fn parse_token_node(&mut self) -> $node {
                LexicalParser::parse_token_node(self)
            }
            pub fn parse_keyword_expression(&mut self) -> $node {
                LexicalParser::parse_keyword_expression(self)
            }
            pub fn parse_literal_expression(&mut self) -> $node {
                LexicalParser::parse_literal_expression(self)
            }
            pub fn parse_optional(&mut self, kind: Kind) -> bool {
                TokenParser::parse_optional(self, kind)
            }
            pub fn parse_expected(&mut self, kind: Kind) -> bool {
                TokenParser::parse_expected(self, kind)
            }
            pub fn parse_expected_without_advancing(&mut self, kind: Kind) -> bool {
                TokenParser::parse_expected_without_advancing(self, kind)
            }
            pub fn parse_expected_with_diagnostic(
                &mut self,
                kind: Kind,
                message: Option<&'static Message>,
                should_advance: bool,
            ) -> bool {
                TokenParser::parse_expected_with_diagnostic(self, kind, message, should_advance)
            }
            pub fn parse_expected_token(&mut self, kind: Kind) -> $node {
                TokenParser::parse_expected_token(self, kind)
            }
            pub fn parse_optional_token(&mut self, kind: Kind) -> Option<$node> {
                TokenParser::parse_optional_token(self, kind)
            }
            pub fn parse_expected_matching_brackets(
                &mut self,
                open_kind: Kind,
                close_kind: Kind,
                open_parsed: bool,
                open_position: u32,
            ) {
                TokenParser::parse_expected_matching_brackets(
                    self,
                    open_kind,
                    close_kind,
                    open_parsed,
                    open_position,
                );
            }
            pub fn parse_identifier_name_error_on_unicode_escape_sequence(&mut self) -> $node {
                LexicalParser::parse_identifier_name_error_on_unicode_escape_sequence(self)
            }
            pub fn parse_binding_identifier(&mut self) -> $node {
                LexicalParser::parse_binding_identifier(self)
            }
            pub fn parse_binding_identifier_with_diagnostic(
                &mut self,
                private_identifier_diagnostic_message: Option<&'static Message>,
            ) -> $node {
                LexicalParser::parse_binding_identifier_with_diagnostic(
                    self,
                    private_identifier_diagnostic_message,
                )
            }
            pub fn parse_optional_binding_identifier(&mut self) -> Option<$node> {
                LexicalParser::parse_optional_binding_identifier(self)
            }
            pub fn parse_identifier_name(&mut self) -> $node {
                LexicalParser::parse_identifier_name(self)
            }
            pub fn parse_identifier_name_with_diagnostic(
                &mut self,
                diagnostic_message: Option<&'static Message>,
            ) -> $node {
                LexicalParser::parse_identifier_name_with_diagnostic(self, diagnostic_message)
            }
            pub fn parse_identifier(&mut self) -> $node {
                LexicalParser::parse_identifier(self)
            }
            pub fn parse_identifier_with_diagnostic(
                &mut self,
                diagnostic_message: Option<&'static Message>,
                private_identifier_diagnostic_message: Option<&'static Message>,
            ) -> $node {
                LexicalParser::parse_identifier_with_diagnostic(
                    self,
                    diagnostic_message,
                    private_identifier_diagnostic_message,
                )
            }
            pub fn create_identifier_with_diagnostic(
                &mut self,
                is_identifier: bool,
                diagnostic_message: Option<&'static Message>,
                private_identifier_diagnostic_message: Option<&'static Message>,
            ) -> $node {
                LexicalParser::create_identifier_with_diagnostic(
                    self,
                    is_identifier,
                    diagnostic_message,
                    private_identifier_diagnostic_message,
                )
            }
            pub fn parse_entity_name(
                &mut self,
                allow_reserved_words: bool,
                allow_private_name: bool,
                diagnostic_message: Option<&'static Message>,
            ) -> $node {
                LexicalParser::parse_entity_name(
                    self,
                    allow_reserved_words,
                    allow_private_name,
                    diagnostic_message,
                )
            }

            pub fn parse_keyword_type_node(&mut self) -> $node {
                crate::leaf_types::LeafTypeParser::parse_keyword_type_node(self)
            }
            pub fn parse_this_type_node(&mut self) -> $node {
                crate::leaf_types::LeafTypeParser::parse_this_type_node(self)
            }
            pub fn parse_jsdoc_all_type(&mut self) -> $node {
                crate::leaf_types::LeafTypeParser::parse_jsdoc_all_type(self)
            }
            pub fn parse_literal_type_node(&mut self, negative: bool) -> $node {
                crate::leaf_types::LeafTypeParser::parse_literal_type_node(self, negative)
            }
            pub fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
                TemplateTokenParser::re_scan_template_token(self, is_tagged_template)
            }
            pub fn parse_template_head(&mut self, is_tagged_template: bool) -> $node {
                TemplateTokenParser::parse_template_head(self, is_tagged_template)
            }
            pub fn parse_literal_of_template_span(&mut self, is_tagged_template: bool) -> $node {
                TemplateTokenParser::parse_literal_of_template_span(self, is_tagged_template)
            }
            pub fn parse_template_middle_or_tail(&mut self) -> $node {
                TemplateTokenParser::parse_template_middle_or_tail(self)
            }

            /// Runs shared grammar with fresh handles, restoring parser state afterward.
            /// AST slots are reclaimed unless an explicit outward escape pins the scope.
            pub fn look_ahead<'parser>(
                &'parser mut self,
                callback: impl for<'brand, 'writer> FnOnce(
                    &mut OwnedParserScope<'src, 'parser, 'brand, 'writer>,
                ) -> bool,
            ) -> bool {
                self.prepare_speculation();
                let saved_state = Some(self.state.mark());
                let saved_parsing_contexts = self.state.parsing_contexts;
                self.ast.look_ahead(RollbackBuffers::Reuse, |scope| {
                    let mut parser = OwnedParserScope {
                        state: self.state.as_mut(),
                        ast: scope.borrow_writer(),
                        node_stack: Vec::new(),
                        reparse_list: Vec::new(),
                        saved_state,
                        saved_parsing_contexts,
                    };
                    callback(&mut parser)
                })
            }

            /// Accepting promotes only the result handle to the enclosing scope.
            /// Rejection restores scanner/errors and discards unretained AST allocations.
            pub fn try_parse_node<'parser>(
                &'parser mut self,
                callback: impl for<'brand, 'writer> FnOnce(
                    &mut OwnedParserScope<'src, 'parser, 'brand, 'writer>,
                ) -> Option<ScopedNode<'brand>>,
            ) -> Option<$node> {
                self.prepare_speculation();
                let saved_state = Some(self.state.mark());
                let saved_parsing_contexts = self.state.parsing_contexts;
                let result = self
                    .ast
                    .try_parse_node(RollbackBuffers::Reuse, |scope| {
                        let mut parser = OwnedParserScope {
                            state: self.state.as_mut(),
                            ast: scope.borrow_writer(),
                            node_stack: Vec::new(),
                            reparse_list: Vec::new(),
                            saved_state,
                            saved_parsing_contexts,
                        };
                        let node = callback(&mut parser);
                        if node.is_some() {
                            parser.saved_state = None;
                        }
                        Ok(node)
                    })
                    .expect("conditional parser allocation");
                result
            }
        }
        impl<'src $(, $scope)*> LexicalParser<'src> for $parser {
            type TextPolicy = OwnedScannerText;
            type Scanner = OwnedScanner<'src>;
            type Node = $node;
            type Speculative<'brand, 'writer, 'parser>
                = OwnedParserScope<'src, 'parser, 'brand, 'writer>
            where
                Self: 'parser;

            fn scanner(&self) -> &OwnedScanner<'src> {
                &self.state.scanner
            }
            fn token(&self) -> Kind {
                self.state.token
            }
            fn context_flags(&self) -> NodeFlags {
                self.state.context_flags
            }
            fn statement_has_await_identifier(&self) -> bool {
                self.state.statement_has_await_identifier
            }
            fn set_statement_has_await_identifier(&mut self, value: bool) {
                self.state.statement_has_await_identifier = value;
            }
            fn take_current_text(&mut self) -> ScannedText<Cow<'src, str>> {
                let source_range = self.state.scanner.token_value_source_range();
                ScannedText { value: self.state.scanner.take_token_value(), source_range }
            }
            fn scan_token(&mut self) -> Kind {
                self.state.token = self.state.scanner.scan();
                self.state.report_scan_errors();
                self.state.token
            }
            fn look_ahead<'parser>(
                &'parser mut self,
                callback: impl for<'brand, 'writer> FnOnce(
                    &mut Self::Speculative<'brand, 'writer, 'parser>,
                ) -> bool,
            ) -> bool {
                Self::look_ahead(self, callback)
            }
            #[cold]
            fn error_at(&mut self, range: TextRange, message: &'static Message, args: &[&dyn Display]) {
                self.state.parse_error_at_range(range, message, args);
            }
            fn finish_node(&mut self, node: $node, pos: u32) -> $node {
                crate::construction::finish_node(
                    &mut self.ast,
                    node,
                    TextRange::new(pos, self.state.scanner.token_full_start()),
                    self.state.context_flags,
                    &mut self.state.has_parse_error,
                )
                .expect("finish node in its exclusive parser store")
            }
            fn new_token(&mut self, kind: Kind) -> $node {
                self.ast.new_token(kind, NodeFlags::None, TextRange::synthetic()).expect("token capacity")
            }
            fn new_keyword_expression(&mut self, kind: Kind) -> $node {
                self.ast
                    .new_keyword_expression(NodeFlags::None, TextRange::synthetic(), kind)
                    .expect("keyword capacity")
            }
            fn new_identifier(&mut self, text: &'static str) -> $node {
                self.state.count_identifier(text);
                let input = if text.is_empty() {
                    TextInput::Source(TextRange::new(0, 0))
                } else {
                    TextInput::Owned(text)
                };
                self.ast
                    .new_identifier(NodeFlags::None, TextRange::synthetic(), input)
                    .expect("identifier capacity")
            }
            fn new_source_identifier(
                &mut self,
                text: ScannedText<Cow<'src, str>>,
                range: TextRange,
            ) -> $node {
                self.state.count_identifier(&text.value);
                self.ast.new_identifier(NodeFlags::None, range, text.input()).expect("identifier capacity")
            }
            fn new_private_identifier(&mut self, text: ScannedText<Cow<'src, str>>) -> $node {
                self.ast
                    .new_private_identifier(NodeFlags::None, TextRange::synthetic(), text.input())
                    .expect("private identifier capacity")
            }
            fn new_literal(
                &mut self,
                kind: Kind,
                text: ScannedText<Cow<'src, str>>,
                flags: TokenFlags,
            ) -> $node {
                let range = TextRange::synthetic();
                let text = text.input();
                match kind {
                    Kind::StringLiteral => self.ast.new_string_literal(NodeFlags::None, range, text, flags),
                    Kind::NumericLiteral => {
                        self.ast.new_numeric_literal(NodeFlags::None, range, text, flags)
                    }
                    Kind::BigIntLiteral => {
                        self.ast.new_big_int_literal(NodeFlags::None, range, text, flags)
                    }
                    Kind::RegularExpressionLiteral => {
                        self.ast.new_regular_expression_literal(NodeFlags::None, range, text, flags)
                    }
                    Kind::NoSubstitutionTemplateLiteral => self.ast.new_no_substitution_template_literal(
                        NodeFlags::None,
                        range,
                        TextInput::Source(TextRange::new(0, 0)),
                        flags,
                        text,
                        TokenFlags::None,
                    ),
                    _ => panic!("Unhandled case in parseLiteralExpression"),
                }
                .expect("literal capacity")
            }
            fn new_qualified_name(&mut self, left: $node, right: $node) -> $node {
                self.ast
                    .new_qualified_name(NodeFlags::None, TextRange::synthetic(), left, right)
                    .expect("qualified name capacity")
            }
        }

        impl<'src $(, $scope)*> TokenParser<'src> for $parser {
            type Diagnostic = usize;

            #[cold]
            fn expected_error(&mut self, kind: Kind, message: Option<&'static Message>) {
                if let Some(message) = message {
                    self.state.parse_error_at_range(self.state.scanner.token_range(), message, &[]);
                } else {
                    self.state.parse_error_at_range(
                        self.state.scanner.token_range(),
                        &tsrs_diagnostics::X_0_EXPECTED,
                        &[&tsrs_scanner::token_to_string(kind)],
                    );
                }
            }

            fn unmatched_bracket_error(&mut self, close_kind: Kind) -> Option<Self::Diagnostic> {
                self.state.parse_error_at_range(
                    self.state.scanner.token_range(),
                    &tsrs_diagnostics::X_0_EXPECTED,
                    &[&tsrs_scanner::token_to_string(close_kind)],
                )
            }

            fn add_bracket_related_info(
                &mut self,
                diagnostic: usize,
                range: TextRange,
                message: &'static Message,
                open_text: &str,
                close_text: &str,
            ) {
                self.state.diagnostics[diagnostic].related_information.push(OwnedParseDiagnostic {
                    range,
                    message,
                    args: vec![open_text.to_owned(), close_text.to_owned()],
                    related_information: Vec::new(),
                });
            }
        }

        impl<'src $(, $scope)*> crate::lists::ListParser<'src> for $parser {
            type List = $list;
            fn parsing_contexts(&self) -> u32 {
                self.state.parsing_contexts
            }
            fn set_parsing_contexts(&mut self, contexts: u32) {
                self.state.parsing_contexts = contexts;
            }
            fn node_stack(&self) -> &[$node] {
                &self.node_stack
            }
            fn node_stack_mut(&mut self) -> &mut Vec<$node> {
                &mut self.node_stack
            }
            fn reparse_list_mut(&mut self) -> &mut Vec<$node> {
                self.reparse_nodes()
            }
            fn node_is_js_alias_or_import(&self, node: $node) -> bool {
                matches!(
                    self.ast.node(node).expect("parser node").kind(),
                    Kind::JSTypeAliasDeclaration | Kind::JSImportDeclaration
                )
            }
            fn finish_node_list(&mut self, start: usize, range: TextRange) -> $list {
                let list =
                    self.ast.new_node_list(range, &self.node_stack[start..]).expect("node list capacity");
                self.node_stack.truncate(start);
                list
            }
            fn new_node_list(&mut self, range: TextRange, nodes: &[$node]) -> $list {
                self.ast.new_node_list(range, nodes).expect("node list capacity")
            }
            fn create_missing_list(&mut self) -> $list {
                let pos = self.state.scanner.token_full_start();
                self.ast
                    .new_missing_node_list(TextRange::new(pos, pos))
                    .expect("missing node list capacity")
            }
        }

        impl<'src $(, $scope)*> crate::context::ContextParser<'src> for $parser {
            type State = OwnedParserState<'src>;
            fn mark(&self) -> Self::State {
                self.state.mark()
            }
            fn rewind(&mut self, state: Self::State) {
                self.state.rewind(state);
            }
            fn replace_context_flags(&mut self, flags: NodeFlags) {
                self.state.context_flags = flags;
            }
        }

        impl<'src $(, $scope)*> crate::leaf_types::LeafTypeParser<'src> for $parser {
            fn new_keyword_type_node(&mut self, kind: Kind) -> $node {
                self.ast
                    .new_keyword_type_node(NodeFlags::None, TextRange::synthetic(), kind)
                    .expect("keyword type capacity")
            }
            fn new_this_type_node(&mut self) -> $node {
                self.ast
                    .new_this_type_node(NodeFlags::None, TextRange::synthetic())
                    .expect("this type capacity")
            }
            fn new_jsdoc_all_type(&mut self) -> $node {
                self.ast
                    .new_jsdoc_all_type(NodeFlags::None, TextRange::synthetic())
                    .expect("JSDoc all type capacity")
            }
            fn new_prefix_unary_expression(&mut self, operator: Kind, operand: $node) -> $node {
                self.ast
                    .new_prefix_unary_expression(NodeFlags::None, TextRange::synthetic(), operator, operand)
                    .expect("prefix unary capacity")
            }
            fn new_literal_type_node(&mut self, literal: $node) -> $node {
                self.ast
                    .new_literal_type_node(NodeFlags::None, TextRange::synthetic(), literal)
                    .expect("literal type capacity")
            }
        }

        impl<'src $(, $scope)*> TemplateTokenParser<'src> for $parser {
            fn re_scan_template_token(&mut self, is_tagged_template: bool) -> Kind {
                self.state.token = self.state.scanner.re_scan_template_token(is_tagged_template);
                self.state.report_scan_errors();
                self.state.token
            }
            fn new_template_head(
                &mut self,
                text: ScannedText<Cow<'src, str>>,
                raw_text: &'src str,
                template_flags: TokenFlags,
            ) -> $node {
                self.ast.new_template_head(
                    NodeFlags::None,
                    TextRange::synthetic(),
                    TextInput::Source(self.state.template_raw_range(raw_text)),
                    template_flags,
                    text.input(),
                    TokenFlags::None,
                ).expect("template head capacity")
            }
            fn new_template_middle(
                &mut self,
                text: ScannedText<Cow<'src, str>>,
                raw_text: &'src str,
                template_flags: TokenFlags,
            ) -> $node {
                self.ast.new_template_middle(
                    NodeFlags::None,
                    TextRange::synthetic(),
                    TextInput::Source(self.state.template_raw_range(raw_text)),
                    template_flags,
                    text.input(),
                    TokenFlags::None,
                ).expect("template middle capacity")
            }
            fn new_template_tail(
                &mut self,
                text: ScannedText<Cow<'src, str>>,
                raw_text: &'src str,
                template_flags: TokenFlags,
            ) -> $node {
                self.ast.new_template_tail(
                    NodeFlags::None,
                    TextRange::synthetic(),
                    TextInput::Source(self.state.template_raw_range(raw_text)),
                    template_flags,
                    text.input(),
                    TokenFlags::None,
                ).expect("template tail capacity")
            }
            fn new_missing_template_tail(&mut self) -> $node {
                self.ast.new_template_tail(
                    NodeFlags::None,
                    TextRange::synthetic(),
                    TextInput::Source(TextRange::new(0, 0)),
                    TokenFlags::None,
                    TextInput::Source(TextRange::new(0, 0)),
                    TokenFlags::None,
                ).expect("missing template tail capacity")
            }
        }
    };
}
owned_parser_impl!(OwnedParser<'src>, [], NodeKey, ListKey);
owned_parser_impl!(OwnedParserScope<'src, 'state, 'outer, 'storage>, ['state, 'outer, 'storage], ScopedNode<'outer>, ScopedList<'outer>);
