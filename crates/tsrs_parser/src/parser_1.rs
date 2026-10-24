use std::fmt::Display;
use std::sync::LazyLock;

use bitflags::bitflags;
use rustc_hash::{FxHashMap, FxHashSet};
use tsrs_ast::{
    self as ast, CommentRange, Diagnostic, Kind, ModifierList, Node, NodeFactory, NodeFlags,
    NodeList, SourceFile, SourceFileParseOptions, TokenFlags,
};
use tsrs_core::{
    LanguageVariant, P, ScriptKind, TextRange, alloc_slice, alloc_str, alloc_vec, new_text_range,
    tspath,
};
use tsrs_diagnostics::{self as diagnostics, Message};
use tsrs_scanner::{self as scanner, Scanner, ScannerState};

use crate::factory::ParserFactory;
use crate::parser_3::{attach_file_to_diagnostics, get_comment_pragmas};
use crate::references::collect_external_module_references;
use crate::utilities::{get_language_variant, is_keyword_or_punctuation};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum ParsingContext {
    SourceElements,           // Elements in source file
    BlockStatements,          // Statements in block
    SwitchClauses,            // Clauses in switch statement
    SwitchClauseStatements,   // Statements in switch clause
    TypeMembers,              // Members in interface or type literal
    ClassMembers,             // Members in class declaration
    EnumMembers,              // Members in enum declaration
    HeritageClauseElement,    // Elements in a heritage clause
    VariableDeclarations,     // Variable declarations in variable statement
    ObjectBindingElements,    // Binding elements in object binding list
    ArrayBindingElements,     // Binding elements in array binding list
    ArgumentExpressions,      // Expressions in argument list
    ObjectLiteralMembers,     // Members in object literal
    JsxAttributes,            // Attributes in jsx element
    JsxChildren,              // Things between opening and closing JSX tags
    ArrayLiteralMembers,      // Members in array literal
    Parameters,               // Parameters in parameter list
    JSDocParameters,          // JSDoc parameters in parameter list of JSDoc function type
    RestProperties,           // Property names in a rest type list
    TypeParameters,           // Type parameters in type parameter list
    TypeArguments,            // Type arguments in type argument list
    TupleElementTypes,        // Element types in tuple element type list
    HeritageClauses,          // Heritage clauses for a class or interface declaration.
    ImportOrExportSpecifiers, // Named import clause's import specifier list
    ImportAttributes,         // Import attributes
    JSDocComment,             // Parsing via JSDocParser
    Count,                    // Number of parsing contexts
}

impl ParsingContext {
    pub const ALL: [ParsingContext; ParsingContext::Count as usize] = [
        ParsingContext::SourceElements,
        ParsingContext::BlockStatements,
        ParsingContext::SwitchClauses,
        ParsingContext::SwitchClauseStatements,
        ParsingContext::TypeMembers,
        ParsingContext::ClassMembers,
        ParsingContext::EnumMembers,
        ParsingContext::HeritageClauseElement,
        ParsingContext::VariableDeclarations,
        ParsingContext::ObjectBindingElements,
        ParsingContext::ArrayBindingElements,
        ParsingContext::ArgumentExpressions,
        ParsingContext::ObjectLiteralMembers,
        ParsingContext::JsxAttributes,
        ParsingContext::JsxChildren,
        ParsingContext::ArrayLiteralMembers,
        ParsingContext::Parameters,
        ParsingContext::JSDocParameters,
        ParsingContext::RestProperties,
        ParsingContext::TypeParameters,
        ParsingContext::TypeArguments,
        ParsingContext::TupleElementTypes,
        ParsingContext::HeritageClauses,
        ParsingContext::ImportOrExportSpecifiers,
        ParsingContext::ImportAttributes,
        ParsingContext::JSDocComment,
    ];
}

/// Bit set of `1 << ParsingContext`.
pub type ParsingContexts = u32;

pub struct JSDocInfo {
    pub parent: P<Node>,
    pub js_docs: &'static [P<Node>],
}

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
    pub struct JsdocScannerInfo: u8 {
        const HasJSDoc = 1 << 0;
        const HasDeprecated = 1 << 1;
        const HasSeeOrLink = 1 << 2;
    }
}

pub struct Parser {
    pub scanner: Scanner,
    pub factory: ParserFactory,

    opts: SourceFileParseOptions,
    pub source_text: &'static str,

    script_kind: ScriptKind,
    pub language_variant: LanguageVariant,
    pub diagnostics: Vec<P<Diagnostic>>,
    pub js_diagnostics: Vec<P<Diagnostic>>,
    pub jsdoc_diagnostics: Vec<P<Diagnostic>>,

    pub token: Kind,
    pub source_flags: NodeFlags,
    pub context_flags: NodeFlags,
    pub parsing_contexts: ParsingContexts,
    pub statement_has_await_identifier: bool,
    pub has_deprecated_tag: bool,
    pub has_parse_error: bool,

    pub identifier_count: usize,
    // `register_source_text` index of `source_text` (`NO_SOURCE_TEXT`: identifiers store their text)
    pub source_text_index: u32,
    pub not_parenthesized_arrow: FxHashSet<u32>,
    pub jsdoc_infos: Vec<JSDocInfo>,
    possible_await_spans: Vec<usize>,
    pub jsdoc_comments_space: Vec<&'static str>,
    pub jsdoc_tag_comments_space: Vec<&'static str>,
    pub jsdoc_tag_comments_parts_space: Vec<P<Node>>,
    // tsrs-only: the per-node comment ranges, the tags and comment parts of one JSDoc comment and the text of one
    // `{@link}`, kept between comments like the three above (notes/perf-jsdoc-parser.md). Nested lists inside a tag
    // (typedef and callback children, template type parameters) build on `node_stack`.
    pub jsdoc_comment_ranges_space: Vec<CommentRange>,
    pub jsdoc_tags_space: Vec<P<Node>>,
    pub jsdoc_comment_parts_space: Vec<P<Node>>,
    pub jsdoc_link_text_space: Vec<&'static str>,
    pub reparse_list: Vec<P<Node>>,

    pub reparsed_clones: Vec<P<Node>>,
    // tsrs-only: the deep-clone visitor behind `add_deep_clone_reparse`, built on the file's first JSDoc clone and
    // reused for the rest (notes/perf-reparse-cloner.md).
    pub reparse_cloner: Option<ast::deepclone::ReparseCloner>,

    // tsrs-only: member lists of this declaration file are parsed lazily (`enable_lazy_dts`, tsrs_ast `lazylist`),
    // and the records of the lazy lists so far.
    lazy_dts: bool,
    lazy_lists: Vec<P<tsrs_ast::lazylist::LazyNodeList>>,

    // Go builds node lists in `make([]*ast.Node, 0, 16)` slices that escape analysis keeps on the goroutine stack,
    // and clones the finished list into the node slice arena. The port builds every list on this one stack
    // instead (a nested list pushes above its parent's elements and pops them before returning), so building a
    // list costs no heap allocation; `finish_node_list` copies the elements into the arena.
    pub node_stack: Vec<P<Node>>,
}

pub fn new_parser() -> Parser {
    new_parser_with(NodeFactory::default())
}

fn new_parser_with(factory: NodeFactory) -> Parser {
    Parser {
        scanner: Scanner::new(),
        factory: ParserFactory::with_legacy(factory),
        opts: SourceFileParseOptions::default(),
        source_text: "",
        script_kind: ScriptKind::Unknown,
        language_variant: LanguageVariant::Standard,
        diagnostics: Vec::new(),
        js_diagnostics: Vec::new(),
        jsdoc_diagnostics: Vec::new(),
        token: Kind::Unknown,
        source_flags: NodeFlags::None,
        context_flags: NodeFlags::None,
        parsing_contexts: 0,
        statement_has_await_identifier: false,
        has_deprecated_tag: false,
        has_parse_error: false,
        identifier_count: 0,
        source_text_index: ast::NO_SOURCE_TEXT,
        not_parenthesized_arrow: FxHashSet::default(),
        jsdoc_infos: Vec::new(),
        possible_await_spans: Vec::new(),
        jsdoc_comments_space: Vec::new(),
        jsdoc_tag_comments_space: Vec::new(),
        jsdoc_tag_comments_parts_space: Vec::new(),
        jsdoc_comment_ranges_space: Vec::new(),
        jsdoc_tags_space: Vec::new(),
        jsdoc_comment_parts_space: Vec::new(),
        jsdoc_link_text_space: Vec::new(),
        reparse_list: Vec::new(),
        reparsed_clones: Vec::new(),
        reparse_cloner: None,
        lazy_dts: false,
        lazy_lists: Vec::new(),
        node_stack: Vec::new(),
    }
}

impl Parser {
    /// tsrs-only: puts this parser back into `new_parser`'s state, keeping its factory handle (the two shared
    /// counters) and the `JSDoc` scratch buffers, for the per-thread parser behind `parse_jsdoc_for_node`
    /// (notes/perf-jsdoc-parser.md). Everything else starts fresh, the scanner's number caches included, so nothing
    /// of the previous file is carried over.
    pub fn recycle(&mut self) {
        let mut fresh = new_parser_with(self.factory.legacy_mut().clone());
        fresh.jsdoc_comments_space = std::mem::take(&mut self.jsdoc_comments_space);
        fresh.jsdoc_tag_comments_space = std::mem::take(&mut self.jsdoc_tag_comments_space);
        fresh.jsdoc_tag_comments_parts_space =
            std::mem::take(&mut self.jsdoc_tag_comments_parts_space);
        fresh.jsdoc_comment_ranges_space = std::mem::take(&mut self.jsdoc_comment_ranges_space);
        fresh.jsdoc_tags_space = std::mem::take(&mut self.jsdoc_tags_space);
        fresh.jsdoc_comment_parts_space = std::mem::take(&mut self.jsdoc_comment_parts_space);
        fresh.jsdoc_link_text_space = std::mem::take(&mut self.jsdoc_link_text_space);
        fresh.node_stack = std::mem::take(&mut self.node_stack);
        fresh.node_stack.clear();
        *self = fresh;
    }
}

pub static VIABLE_KEYWORD_SUGGESTIONS: LazyLock<Vec<&'static str>> =
    LazyLock::new(scanner::get_viable_keyword_suggestions);

// MISSING_LIST_NODES is a sentinel backing array used to distinguish "missing" node lists
// (where the expected opening token was not found) from ordinary empty node lists.
// A `usize` static gives a unique address, aligned for `P<Node>`, for a zero-length slice.
static MISSING_LIST_NODES_BACKING: usize = 0;

pub fn missing_list_nodes() -> &'static [P<Node>] {
    // SAFETY: a zero-length slice needs only a non-null pointer aligned for `P<Node>`; a `usize`
    // static is at least as aligned as `P<Node>` (a reference, or a 4-byte handle with compressed
    // pointers), and nothing is ever read through this slice.
    unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref::<usize>(&MISSING_LIST_NODES_BACKING).cast::<P<Node>>(),
            0,
        )
    }
}

pub fn is_missing_node_list(list: Option<P<NodeList>>) -> bool {
    match list {
        Some(list) => std::ptr::eq(list.nodes().as_ptr(), missing_list_nodes().as_ptr()),
        None => false,
    }
}

pub fn parse_source_file(
    opts: SourceFileParseOptions,
    source_text: &str,
    script_kind: ScriptKind,
) -> P<SourceFile> {
    parse_source_file_static(opts, alloc_str(source_text), script_kind)
}

/// `parse_source_file` for text the caller hands over (a file just read): outside a region it becomes the
/// file's text without a copy (Go shares the string) and, like the thread arena, is never freed.
///
/// Inside a freeable region (native API transpile and builds) the text is copied into the region and
/// unregistered when the region is freed, so the file's memory is reclaimed with it.
///
/// Lifetime footgun: the returned file now lives exactly as long as the region that was current at the call.
/// A caller that parses inside a region must not keep the file (or anything derived from it) past that
/// region. Unregistering frees the text but not its source-text slot: slots are never reused
/// (`register_source_text` has a fixed capacity, after which lookups take the slower fallback path).
pub fn parse_source_file_owned(
    opts: SourceFileParseOptions,
    source_text: String,
    script_kind: ScriptKind,
) -> P<SourceFile> {
    match tsrs_core::arena::current_region() {
        None => parse_source_file_static(opts, source_text.leak(), script_kind),
        Some(region) => {
            let file = parse_source_file(opts, &source_text, script_kind);
            let text_index = file.text_index.get();
            region.on_free(Box::new(move || ast::unregister_source_text(text_index)));
            file
        }
    }
}

/// Keeps source text alive after a per-file arena region is released.
///
/// `parse_source_file_owned` that keeps the text (leaked, as outside a region) even inside a region: for the
/// CLI's per-file regions (`tsrs_compiler` fileregions.rs), which may free a checked file's tree but keep its
/// text, its `SourceFile` and its diagnostics for the report.
pub fn parse_source_file_keep_text(
    opts: SourceFileParseOptions,
    source_text: String,
    script_kind: ScriptKind,
) -> P<SourceFile> {
    parse_source_file_static(opts, source_text.leak(), script_kind)
}

/// Reuses embedded text when its lifetime permits it.
///
/// Parses text that lives for the whole process (the bundled libs, `include_str!` in the binary): outside a
/// region the file reads the text in place, with no copy and nothing to leak; inside a region it behaves like
/// `parse_source_file_owned` (the region holds the text the file reads, and forgets it when it is freed).
pub fn parse_source_file_embedded(
    opts: SourceFileParseOptions,
    source_text: &'static str,
    script_kind: ScriptKind,
) -> P<SourceFile> {
    match tsrs_core::arena::current_region() {
        None => parse_source_file_static(opts, source_text, script_kind),
        Some(region) => {
            let file = parse_source_file(opts, source_text, script_kind);
            let text_index = file.text_index.get();
            region.on_free(Box::new(move || ast::unregister_source_text(text_index)));
            file
        }
    }
}

fn parse_source_file_static(
    opts: SourceFileParseOptions,
    source_text: &'static str,
    script_kind: ScriptKind,
) -> P<SourceFile> {
    crate::jsdoc::init();
    let mut p = new_parser();
    p.initialize_state_static(opts, source_text, script_kind);
    p.source_text_index = ast::register_source_text(source_text);
    p.next_token();
    if p.script_kind == ScriptKind::JSON {
        return p.parse_json_text();
    }
    p.parse_source_file_worker()
}

impl Parser {
    pub fn is_java_script(&self) -> bool {
        self.script_kind == ScriptKind::JS || self.script_kind == ScriptKind::JSX
    }

    fn parse_json_text(&mut self) -> P<SourceFile> {
        let pos = self.node_pos();
        let statements: P<NodeList>;
        let eof: P<Node>;

        if self.token == Kind::EndOfFile {
            let end = self.node_pos();
            statements = self.new_node_list(new_text_range(pos, end), &[]);
            eof = self.parse_token_node();
        } else {
            // Go keeps a single expression until a second one shows up ([]*ast.Expression | *ast.Expression).
            let mut expressions: Vec<P<Node>> = Vec::new();

            while self.token != Kind::EndOfFile {
                let expression: P<Node> = match self.token {
                    Kind::OpenBracketToken => self.parse_array_literal_expression(),
                    Kind::TrueKeyword | Kind::FalseKeyword | Kind::NullKeyword => {
                        self.parse_token_node()
                    }
                    Kind::MinusToken => {
                        if self.look_ahead(|p| {
                            p.next_token() == Kind::NumericLiteral
                                && p.next_token() != Kind::ColonToken
                        }) {
                            self.parse_prefix_unary_expression()
                        } else {
                            self.parse_object_literal_expression()
                        }
                    }
                    Kind::NumericLiteral | Kind::StringLiteral => {
                        if self.look_ahead(|p| p.next_token() != Kind::ColonToken) {
                            self.parse_literal_expression()
                        } else {
                            self.parse_object_literal_expression()
                        }
                    }
                    _ => self.parse_object_literal_expression(),
                };

                // Error recovery: collect multiple top-level expressions
                if expressions.is_empty() {
                    expressions.push(expression);
                    if self.token != Kind::EndOfFile {
                        self.parse_error_at_current_token(&diagnostics::UNEXPECTED_TOKEN, &[]);
                    }
                } else {
                    expressions.push(expression);
                }
            }

            let expression = if expressions.len() > 1 {
                let end = self.node_pos();
                let list = self.new_node_list(new_text_range(pos, end), &expressions);
                let array = self.factory.new_array_literal_expression(list, false);
                self.finish_node(array, pos)
            } else {
                expressions[0]
            };
            let statement = self.factory.new_expression_statement(expression);
            let statement = self.finish_node(statement, pos);
            let end = self.node_pos();
            statements = self.new_node_list(new_text_range(pos, end), &[statement]);
            eof = self.parse_expected_token(Kind::EndOfFile);
        }
        let node =
            self.factory.new_source_file(self.opts.clone(), self.source_text, statements, eof);
        node.as_source_file().text_index.set(self.source_text_index);
        let node = self.finish_node(node, pos);
        let result = node.as_source_file_p();
        if !result.statements.nodes().is_empty() {
            let value = result.statements.nodes()[0].expression();
            self.validate_json_value(result, value);
        }
        self.finish_source_file(result, false);
        result
    }
}

pub fn get_error_span_for_node(source_text: &str, node: P<Node>) -> TextRange {
    let mut pos = node.pos().expect_source("parsed nodes have source positions");
    if !ast::node_is_missing(Some(node)) {
        pos = scanner::skip_trivia(source_text, pos);
    }
    new_text_range(pos, node.end().expect_source("parsed nodes have source positions"))
}

impl Parser {
    fn validate_json_value(
        &mut self,
        source_file: P<SourceFile>,
        value_expression: Option<P<Node>>,
    ) {
        let Some(value_expression) = value_expression else {
            return;
        };
        match value_expression.kind() {
            Kind::TrueKeyword | Kind::FalseKeyword | Kind::NullKeyword | Kind::NumericLiteral => {
                return;
            }
            Kind::StringLiteral => {
                if !is_double_quoted_string(value_expression) {
                    self.diagnostics.push(ast::new_diagnostic(
                        Some(source_file),
                        get_error_span_for_node(self.source_text, value_expression),
                        &diagnostics::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
                        &[],
                    ));
                }
                return;
            }
            Kind::PrefixUnaryExpression => {
                let prefix = value_expression.as_prefix_unary_expression();
                if !(prefix.operator() != Kind::MinusToken
                    || prefix.operand().kind() != Kind::NumericLiteral)
                {
                    return;
                }
                // not valid JSON syntax
            }
            Kind::ObjectLiteralExpression => {
                self.validate_json_object_literal(source_file, value_expression);
                return;
            }
            Kind::ArrayLiteralExpression => {
                for element in value_expression.elements() {
                    self.validate_json_value(source_file, Some(*element));
                }
                return;
            }
            _ => {}
        }
        self.diagnostics.push(ast::new_diagnostic(
            Some(source_file),
            get_error_span_for_node(self.source_text, value_expression),
            &diagnostics::PROPERTY_VALUE_CAN_ONLY_BE_STRING_LITERAL_NUMERIC_LITERAL_TRUE_FALSE_NULL_OBJECT_LITERAL_OR_ARRAY_LITERAL,
            &[],
        ));
    }
}

pub fn is_double_quoted_string(node: P<Node>) -> bool {
    ast::is_string_literal(node)
        && !node.as_string_literal().token_flags().intersects(TokenFlags::SingleQuote)
}

impl Parser {
    // validateJsonObjectLiteral validates properties of a JSON object literal.
    fn validate_json_object_literal(&mut self, source_file: P<SourceFile>, node: P<Node>) {
        for element in node.as_object_literal_expression().properties().nodes() {
            let element = *element;
            if element.kind() != Kind::PropertyAssignment {
                self.diagnostics.push(ast::new_diagnostic(
                    Some(source_file),
                    get_error_span_for_node(self.source_text, element),
                    &diagnostics::PROPERTY_ASSIGNMENT_EXPECTED,
                    &[],
                ));
                continue;
            }
            if let Some(name) = element.name()
                && !is_double_quoted_string(name)
            {
                self.diagnostics.push(ast::new_diagnostic(
                    Some(source_file),
                    get_error_span_for_node(self.source_text, name),
                    &diagnostics::STRING_LITERAL_WITH_DOUBLE_QUOTES_EXPECTED,
                    &[],
                ));
            }
            self.validate_json_value(
                source_file,
                Some(element.as_property_assignment().initializer()),
            );
        }
    }
}

pub fn parse_isolated_entity_name(text: &str) -> Option<P<Node>> {
    let mut p = new_parser();
    p.initialize_state(SourceFileParseOptions::default(), text, ScriptKind::JS);
    p.next_token();
    let entity_name = p.parse_entity_name(true, false, None);
    if p.token == Kind::EndOfFile && p.diagnostics.is_empty() { Some(entity_name) } else { None }
}

impl Parser {
    pub fn initialize_state(
        &mut self,
        opts: SourceFileParseOptions,
        source_text: &str,
        script_kind: ScriptKind,
    ) {
        self.initialize_state_static(opts, alloc_str(source_text), script_kind);
    }

    /// `initialize_state` for text that already lives in the arena (Go shares the string; no copy).
    fn initialize_state_static(
        &mut self,
        opts: SourceFileParseOptions,
        source_text: &'static str,
        script_kind: ScriptKind,
    ) {
        assert!(
            script_kind != ScriptKind::Unknown,
            "ScriptKind must be specified when parsing source file: {}",
            opts.file_name
        );
        self.opts = opts;
        self.initialize_text_state(source_text, script_kind);
    }

    /// `initialize_state_static` without the parse options, which only `parse_source_file_worker` reads: the lazy
    /// `JSDoc` parser (`parse_jsdoc_for_node`) parses the comments of an already parsed file.
    pub fn initialize_text_state(&mut self, source_text: &'static str, script_kind: ScriptKind) {
        self.scanner.reset();
        self.source_text = source_text;
        self.source_text_index = ast::NO_SOURCE_TEXT;
        self.script_kind = script_kind;
        self.language_variant = get_language_variant(self.script_kind);
        self.context_flags = match self.script_kind {
            ScriptKind::JS | ScriptKind::JSX => NodeFlags::JavaScriptFile,
            ScriptKind::JSON => NodeFlags::JavaScriptFile | NodeFlags::JsonFile,
            _ => NodeFlags::None,
        };
        self.scanner.set_text(self.source_text);
        self.scanner.set_on_error(true);
        self.scanner.set_language_variant(self.language_variant);
    }

    fn scan_error(
        &mut self,
        message: &'static Message,
        pos: u32,
        length: u32,
        args: &[&dyn Display],
    ) {
        self.parse_error_at_range(new_text_range(pos, pos + length), message, args);
    }

    // The scanner buffers what Go reports through its synchronous error callback; this must run
    // right after every scanner call that can report (scan, re_scan_*, scan_jsx_*, scan_jsdoc_*)
    // so scanner errors interleave with parser errors exactly as in Go.
    #[inline]
    pub fn report_scan_errors(&mut self) {
        if self.scanner.has_errors() {
            self.report_buffered_scan_errors();
        }
    }

    #[cold]
    fn report_buffered_scan_errors(&mut self) {
        for error in self.scanner.take_errors() {
            let args: Vec<&dyn Display> =
                error.args.iter().map(|arg| arg as &dyn Display).collect();
            self.scan_error(error.message, error.start, error.length, &args);
        }
    }

    #[cold]
    pub fn parse_error_at(
        &mut self,
        pos: u32,
        end: u32,
        message: &'static Message,
        args: &[&dyn Display],
    ) -> Option<P<Diagnostic>> {
        self.parse_error_at_range(new_text_range(pos, end), message, args)
    }

    #[cold]
    pub fn parse_error_at_current_token(
        &mut self,
        message: &'static Message,
        args: &[&dyn Display],
    ) -> Option<P<Diagnostic>> {
        let loc = self.scanner.token_range();
        self.parse_error_at_range(loc, message, args)
    }

    #[cold]
    pub fn parse_error_at_range(
        &mut self,
        loc: TextRange,
        message: &'static Message,
        args: &[&dyn Display],
    ) -> Option<P<Diagnostic>> {
        // Don't report another error if it would just be at the same location as the last error
        let mut result = None;
        if self.diagnostics.is_empty()
            || self.diagnostics[self.diagnostics.len() - 1].pos() != loc.source_pos()
        {
            let diagnostic = ast::new_diagnostic(None, loc, message, args);
            self.diagnostics.push(diagnostic);
            result = Some(diagnostic);
        }
        self.has_parse_error = true;
        result
    }
}

#[derive(Clone, Copy)]
pub struct ParserState {
    scanner_state: ScannerState,
    context_flags: NodeFlags,
    diagnostics_len: usize,
    js_diagnostics_len: usize,
    jsdoc_infos_len: usize,
    reparsed_clones_len: usize,
    lazy_lists_len: usize,
    statement_has_await_identifier: bool,
    has_parse_error: bool,
    // Rolling back a parse also discards the arena memory it allocated (notes/mem-recycle.md). Parser data that
    // `rewind` does not truncate and that can receive nodes from inside a speculation (`reparse_list`,
    // `jsdoc_diagnostics`, the scanner's number caches) calls `tsrs_core::arena_pin`, which cancels that.
    arena: tsrs_core::arena::Checkpoint,
}

impl Parser {
    pub fn mark(&self) -> ParserState {
        ParserState {
            arena: tsrs_core::arena_checkpoint(),
            scanner_state: self.scanner.mark(),
            context_flags: self.context_flags,
            diagnostics_len: self.diagnostics.len(),
            js_diagnostics_len: self.js_diagnostics.len(),
            jsdoc_infos_len: self.jsdoc_infos.len(),
            reparsed_clones_len: self.reparsed_clones.len(),
            lazy_lists_len: self.lazy_lists.len(),
            statement_has_await_identifier: self.statement_has_await_identifier,
            has_parse_error: self.has_parse_error,
        }
    }

    pub fn rewind(&mut self, state: ParserState) {
        let arena = state.arena;
        self.rewind_keeping_nodes(state);
        tsrs_core::arena_rewind(arena);
    }

    /// `rewind` for a caller that keeps nodes it parsed after `mark` (the top-level await reparse).
    fn rewind_keeping_nodes(&mut self, state: ParserState) {
        self.scanner.rewind(state.scanner_state);
        self.token = self.scanner.token();
        self.context_flags = state.context_flags;
        self.diagnostics.truncate(state.diagnostics_len);
        self.js_diagnostics.truncate(state.js_diagnostics_len);
        self.jsdoc_infos.truncate(state.jsdoc_infos_len);
        self.reparsed_clones.truncate(state.reparsed_clones_len);
        self.lazy_lists.truncate(state.lazy_lists_len);
        self.statement_has_await_identifier = state.statement_has_await_identifier;
        self.has_parse_error = state.has_parse_error;
    }

    // Keep speculative scanner snapshots and rollback bookkeeping out of callers' common-path frames.
    #[inline(never)]
    pub fn look_ahead(&mut self, callback: impl FnOnce(&mut Parser) -> bool) -> bool {
        let state = self.mark();
        let result = callback(self);
        self.rewind(state);
        result
    }

    pub fn next_token(&mut self) -> Kind {
        crate::lexical::LexicalParser::next_token(self)
    }

    pub fn next_token_without_check(&mut self) -> Kind {
        self.token = self.scanner.scan();
        self.report_scan_errors();
        self.token
    }

    pub fn next_token_jsdoc(&mut self) -> Kind {
        self.token = self.scanner.scan_jsdoc_token();
        self.report_scan_errors();
        self.token
    }

    pub fn next_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> Kind {
        self.token = self.scanner.scan_jsdoc_comment_text_token(in_backticks);
        self.report_scan_errors();
        self.token
    }

    pub fn node_pos(&self) -> u32 {
        self.scanner.token_full_start()
    }

    pub fn jsdoc_scanner_info(&self) -> JsdocScannerInfo {
        if !self.scanner.has_preceding_jsdoc_comment() {
            return JsdocScannerInfo::empty();
        }
        let mut info = JsdocScannerInfo::HasJSDoc;
        if self.scanner.has_preceding_jsdoc_with_deprecated_tag() {
            info |= JsdocScannerInfo::HasDeprecated;
        }
        if self.scanner.has_preceding_jsdoc_with_see_or_link() {
            info |= JsdocScannerInfo::HasSeeOrLink;
        }
        info
    }

    fn parse_source_file_worker(&mut self) -> P<SourceFile> {
        let is_declaration_file = tspath::is_declaration_file_name(&self.opts.file_name);
        if is_declaration_file {
            self.context_flags |= NodeFlags::Ambient;
            // Relaxed: set once by the CLI before any program is created.
            self.lazy_dts = LAZY_DTS.load(std::sync::atomic::Ordering::Relaxed);
        }
        let pos = self.node_pos();
        let mut statements =
            self.parse_list_index(ParsingContext::SourceElements, Parser::parse_toplevel_statement);
        let end = self.node_pos();
        let end_jsdoc = self.jsdoc_scanner_info();
        let eof = self.parse_token_node();
        self.with_jsdoc(eof, end_jsdoc);
        assert!(eof.kind() == Kind::EndOfFile, "Expected end of file token from scanner.");
        if !self.reparse_list.is_empty() {
            statements.append(&mut self.reparse_list);
        }
        let statement_list = self.new_node_list(new_text_range(pos, end), &statements);
        let node = {
            // tsrs-only: the `SourceFile` outlives a scratch region the file is parsed in (fileregions.rs in
            // tsrs_compiler frees a checked file's tree, not the file); a no-op outside one.
            let _outer = tsrs_core::arena::escape_scratch();
            self.factory.new_source_file(self.opts.clone(), self.source_text, statement_list, eof)
        };
        node.as_source_file().text_index.set(self.source_text_index);
        let node = self.finish_node(node, pos);
        let mut result = node.as_source_file_p();
        self.finish_source_file(result, is_declaration_file);
        if !result.is_declaration_file.get()
            && result.external_module_indicator.get().is_some()
            && !self.possible_await_spans.is_empty()
        {
            let reparse = self.reparse_top_level_await(result);
            let reparse = self.finish_node(reparse, pos);
            if node != reparse {
                result = reparse.as_source_file_p();
                self.finish_source_file(result, is_declaration_file);
            }
        }
        if !self.lazy_lists.is_empty() {
            result.lazy_lists.set(alloc_slice(&self.lazy_lists));
        }
        collect_external_module_references(result);
        if ast::is_in_js_file(Some(node)) {
            result.set_js_diagnostics(&attach_file_to_diagnostics(&self.js_diagnostics, result));
        }
        result
    }

    fn finish_source_file(&mut self, result: P<SourceFile>, is_declaration_file: bool) {
        result.comment_directives.set(alloc_slice(self.scanner.comment_directives()));
        result
            .pragmas
            .set(alloc_vec(get_comment_pragmas(self.factory.legacy_mut(), self.source_text)));
        self.process_pragmas_into_fields(result);
        result.set_diagnostics(&attach_file_to_diagnostics(&self.diagnostics, result));
        result.set_jsdoc_diagnostics(&attach_file_to_diagnostics(&self.jsdoc_diagnostics, result));
        result.is_declaration_file.set(is_declaration_file);
        result.language_variant.set(self.language_variant);
        result.script_kind.set(self.script_kind);
        let node = result.as_node();
        node.set_flags(node.flags() | self.source_flags);
        result.node_count.set(self.factory.node_count());
        result.text_count.set(self.factory.text_count());
        result.identifier_count.set(self.identifier_count);
        // For non-JS files, enable lazy JSDoc parsing on demand (before the cache is stored: it picks the map).
        if !self.is_java_script() {
            result.set_has_lazy_jsdoc(true);
        }
        result.set_jsdoc_cache(self.create_jsdoc_cache());
        self.reparsed_clones.sort_by(|a, b| ast::compare_node_positions(*a, *b));
        result.reparsed_clones.set(alloc_slice(&self.reparsed_clones));
        ast::set_external_module_indicator(&result, self.opts.external_module_indicator_options);
    }

    fn create_jsdoc_cache(&self) -> FxHashMap<P<Node>, &'static [P<Node>]> {
        if self.jsdoc_infos.is_empty() {
            return FxHashMap::default();
        }
        let mut result =
            FxHashMap::with_capacity_and_hasher(self.jsdoc_infos.len(), rustc_hash::FxBuildHasher);
        result.extend(self.jsdoc_infos.iter().map(|info| (info.parent, info.js_docs)));
        result
    }

    fn parse_toplevel_statement(&mut self, i: usize) -> P<Node> {
        self.statement_has_await_identifier = false;
        let statement = self.parse_statement();
        // Reparsed nodes (e.g. JSDoc @typedef) produced while parsing this statement are inserted
        // into the statement list before this statement, so account for them when recording the
        // statement's index for possibleAwaitSpans.
        let i = i + self.reparse_list.len();
        if self.statement_has_await_identifier
            && !statement.flags().intersects(NodeFlags::AwaitContext)
        {
            if self.possible_await_spans.is_empty()
                || self.possible_await_spans[self.possible_await_spans.len() - 1] != i
            {
                self.possible_await_spans.push(i);
                self.possible_await_spans.push(i + 1);
            } else {
                let last = self.possible_await_spans.len() - 1;
                self.possible_await_spans[last] = i + 1;
            }
        }
        statement
    }

    fn reparse_top_level_await(&mut self, source_file: P<SourceFile>) -> P<Node> {
        assert!(
            self.possible_await_spans.len() % 2 != 1,
            "possibleAwaitSpans malformed: odd number of indices, not paired into spans."
        );
        let source_statements: &'static [P<Node>] = source_file.statements.nodes();
        let mut statements: Vec<P<Node>> = Vec::new();
        let saved_parse_diagnostics = std::mem::take(&mut self.diagnostics);

        let mut after_await_statement = 0;
        let mut i = 0;
        while i < self.possible_await_spans.len() {
            let next_await_statement = self.possible_await_spans[i];
            // append all non-await statements between afterAwaitStatement and nextAwaitStatement
            let prev_statement = source_statements[after_await_statement];
            let next_statement = source_statements[next_await_statement];
            statements
                .extend_from_slice(&source_statements[after_await_statement..next_await_statement]);

            // append all diagnostics associated with the copied range
            let prev_statement_pos =
                Some(prev_statement.pos().expect_source("parsed nodes have source positions"));
            let next_statement_pos =
                Some(next_statement.pos().expect_source("parsed nodes have source positions"));
            let diagnostic_start = saved_parse_diagnostics
                .iter()
                .position(|diagnostic| diagnostic.pos() >= prev_statement_pos);
            let diagnostic_end = match diagnostic_start {
                Some(start) => saved_parse_diagnostics[start..]
                    .iter()
                    .position(|diagnostic| diagnostic.pos() >= next_statement_pos),
                None => None,
            };
            if let Some(start) = diagnostic_start {
                let slice = match diagnostic_end {
                    Some(end) => &saved_parse_diagnostics[start..start + end],
                    None => &saved_parse_diagnostics[start..],
                };
                self.diagnostics.extend_from_slice(slice);
            }

            let mut state = self.mark();
            // reparse all statements between start and pos. We skip existing diagnostics for the same range and allow the parser to generate new ones.
            self.context_flags |= NodeFlags::AwaitContext;
            self.scanner.reset_pos(
                next_statement.pos().expect_source("parsed nodes have source positions"),
            );
            self.next_token();

            after_await_statement = self.possible_await_spans[i + 1];
            while self.token != Kind::EndOfFile {
                let start_pos = self.scanner.token_full_start();
                let statement = self.parse_statement();
                statements.push(statement);
                if start_pos == self.scanner.token_full_start() {
                    self.next_token();
                }
                if after_await_statement < source_statements.len() {
                    let last_await_statement = source_statements[after_await_statement - 1];
                    if statement.end() == last_await_statement.end() {
                        // done reparsing this section
                        break;
                    }
                    if statement.end() > last_await_statement.end() {
                        // we ate into the next statement, so we must continue reparsing the next span
                        i += 2;
                        if i < self.possible_await_spans.len() {
                            after_await_statement = self.possible_await_spans[i + 1];
                        } else {
                            after_await_statement = source_statements.len();
                        }
                    }
                }
            }

            // Keep diagnostics from the reparse
            state.diagnostics_len = self.diagnostics.len();
            self.rewind_keeping_nodes(state);
            i += 2;
        }

        // append all statements between pos and the end of the list
        if after_await_statement < source_statements.len() {
            let prev_statement = source_statements[after_await_statement];
            statements.extend_from_slice(&source_statements[after_await_statement..]);

            // append all diagnostics associated with the copied range
            let prev_statement_pos =
                Some(prev_statement.pos().expect_source("parsed nodes have source positions"));
            let diagnostic_start = saved_parse_diagnostics
                .iter()
                .position(|diagnostic| diagnostic.pos() >= prev_statement_pos);
            if let Some(start) = diagnostic_start {
                self.diagnostics.extend_from_slice(&saved_parse_diagnostics[start..]);
            }
        }

        let loc = source_file.statements.loc.get();
        let list = self.new_node_list(loc, &statements);
        let result = {
            // See parse_source_file_worker.
            let _outer = tsrs_core::arena::escape_scratch();
            self.factory.new_source_file(
                source_file.parse_options().clone(),
                self.source_text,
                list,
                source_file.end_of_file_token,
            )
        };
        result.as_source_file().text_index.set(self.source_text_index);
        for s in &statements {
            s.set_parent(Some(result)); // force (re)set parent to reparsed source file
        }
        result
    }

    fn parse_list_index(
        &mut self,
        kind: ParsingContext,
        parse_element: impl FnMut(&mut Parser, usize) -> P<Node>,
    ) -> Vec<P<Node>> {
        crate::lists::ListParser::parse_list_index(self, kind, parse_element)
    }

    /// `parse_list` for the member list of an interface, class or type literal: lazy in a declaration file the program
    /// will not check (`parse_member_list_lazily`).
    pub fn parse_member_list(
        &mut self,
        kind: ParsingContext,
        parse_element: fn(&mut Parser) -> P<Node>,
    ) -> P<NodeList> {
        if self.lazy_dts && !self.context_flags.intersects(NodeFlags::JSDoc) {
            return self.parse_member_list_lazily(kind, parse_element);
        }
        self.parse_list(kind, parse_element)
    }

    /// tsrs-only (notes/mem-lazy-dts-members.md): parses the member list of an interface, class or type
    /// literal of a declaration file, and if it is plain (`lazy_list_blocked`: no diagnostic, no eager
    /// `JSDoc`, no comment directive, nothing whose binding reaches outside the list, and the arena can
    /// discard it), discards its nodes and returns a lazy list (`tsrs_ast` `lazylist`) that
    /// `reparse_lazy_list` parses again from the same position on first use.
    fn parse_member_list_lazily(
        &mut self,
        kind: ParsingContext,
        parse_element: fn(&mut Parser) -> P<Node>,
    ) -> P<NodeList> {
        let pos = self.node_pos();
        let context_flags = self.context_flags;
        let parsing_contexts = self.parsing_contexts;
        let clean_start = !self.has_parse_error;
        let cp = tsrs_core::arena_checkpoint();
        let before = self.lazy_watch();
        let list = self.parse_list(kind, parse_element);
        let after = self.lazy_watch();
        if !clean_start
            || list.nodes().is_empty()
            || list.loc().len().is_some_and(|len| len > LAZY_LIST_MAX_TEXT)
            || (after.0, after.1, after.2, after.3, after.5)
                != (before.0, before.1, before.2, before.3, before.5)
            || self.has_parse_error
            || !tsrs_core::arena_rewindable(&cp)
            || lazy_list_blocked(list.nodes())
        {
            return list;
        }
        let loc = list.loc();
        debug_assert_eq!(loc.source_pos(), Some(pos));
        self.lazy_lists.truncate(before.4);
        tsrs_core::arena_rewind(cp);
        let record = P::new(tsrs_ast::lazylist::LazyNodeList::new(
            loc.pos().expect_source("parsed lists have source positions"),
            loc.end().expect_source("parsed lists have source positions"),
            kind as u8,
            context_flags,
            parsing_contexts,
        ));
        self.lazy_lists.push(record);
        self.factory.new_lazy_node_list(loc, record.get())
    }

    /// What a lazy list must not change (`parse_member_list_lazily`): diagnostics, eager `JSDoc`, reparsed
    /// clones, comment directives (each reallocates an arena slice the parser or scanner keeps), and the lazy
    /// records so far (index 4: nested records are dropped with the list).
    fn lazy_watch(&self) -> (usize, usize, usize, usize, usize, usize) {
        (
            self.diagnostics.len() + self.js_diagnostics.len() + self.jsdoc_diagnostics.len(),
            self.jsdoc_infos.len(),
            self.reparsed_clones.len(),
            self.scanner.comment_directives().len(),
            self.lazy_lists.len(),
            self.source_flags.bits() as usize,
        )
    }

    /// The owner node of a member list that may be lazy, right after it is created.
    pub fn set_lazy_owner(members: P<NodeList>, owner: P<Node>) {
        if let Some(record) = members.lazy_record() {
            record.set_owner(owner);
        }
    }

    pub fn parse_list(
        &mut self,
        kind: ParsingContext,
        parse_element: impl FnMut(&mut Parser) -> P<Node>,
    ) -> P<NodeList> {
        crate::lists::ListParser::parse_list(self, kind, parse_element)
    }

    /// Moves the elements above `start` off `node_stack` into a new arena node list.
    pub fn finish_node_list(&mut self, start: usize, loc: TextRange) -> P<NodeList> {
        let list = self.factory.new_node_list_from_slice(&self.node_stack[start..]);
        list.loc.set(loc);
        self.node_stack.truncate(start);
        list
    }

    // Return a non-nil (but possibly empty) slice if parsing was successful, or nil if parseElement returned nil

    // Return a non-nil (but possibly empty) NodeList if parsing was successful, a missing NodeList if the opening
    // token wasn't found, or nil if parseElement returned nil.

    pub fn create_missing_list(&mut self) -> P<NodeList> {
        let pos = self.node_pos();
        let result = self.factory.new_node_list_from_static(missing_list_nodes());
        result.loc.set(new_text_range(pos, pos));
        result
    }

    pub fn parse_expected_jsdoc(&mut self, kind: Kind) -> bool {
        if self.token == kind {
            self.next_token_jsdoc();
            return true;
        }
        assert!(
            is_keyword_or_punctuation(kind),
            "Invalid JSDoc kind: expected keyword or punctuation"
        );
        self.parse_error_at_current_token(
            &diagnostics::X_0_EXPECTED,
            &[&scanner::token_to_string(kind)],
        );
        false
    }

    pub fn parse_optional(&mut self, token: Kind) -> bool {
        crate::tokens::TokenParser::parse_optional(self, token)
    }

    pub fn parse_expected(&mut self, kind: Kind) -> bool {
        crate::tokens::TokenParser::parse_expected(self, kind)
    }

    pub fn parse_token_node(&mut self) -> P<Node> {
        crate::lexical::LexicalParser::parse_token_node(self)
    }

    pub fn parse_expected_token(&mut self, kind: Kind) -> P<Node> {
        crate::tokens::TokenParser::parse_expected_token(self, kind)
    }

    pub fn parse_optional_token(&mut self, kind: Kind) -> Option<P<Node>> {
        crate::tokens::TokenParser::parse_optional_token(self, kind)
    }

    pub fn parse_expected_token_jsdoc(&mut self, kind: Kind) -> P<Node> {
        if let Some(optional) = self.parse_optional_token_jsdoc(kind) {
            optional
        } else {
            assert!(is_keyword_or_punctuation(kind), "expected keyword or punctuation");
            self.parse_error_at_current_token(
                &diagnostics::X_0_EXPECTED,
                &[&scanner::token_to_string(kind)],
            );
            let token = self.factory.new_token(kind);
            let pos = self.node_pos();
            self.finish_node(token, pos)
        }
    }

    fn parse_optional_token_jsdoc(&mut self, kind: Kind) -> Option<P<Node>> {
        if self.token == kind {
            return Some(self.parse_token_node());
        }
        None
    }

    pub fn parse_statement(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_statement(self)
    }
}

impl Parser {
    pub fn parse_identifier_or_pattern(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_identifier_or_pattern(self)
    }
}

pub fn is_export_modifier(modifier: P<Node>) -> bool {
    modifier.kind() == Kind::ExportKeyword
}

pub fn is_async_modifier(modifier: P<Node>) -> bool {
    modifier.kind() == Kind::AsyncKeyword
}

pub fn is_type_heritage_clause(is_interface: bool, token: Kind) -> bool {
    is_interface && token == Kind::ExtendsKeyword
        || !is_interface && token == Kind::ImplementsKeyword
}

impl Parser {
    pub fn parse_class_element(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_class_element(self)
    }
}

pub fn modifier_list_has_async(modifiers: Option<P<ModifierList>>) -> bool {
    modifiers.is_some_and(|modifiers| modifiers.nodes().iter().any(|m| is_async_modifier(*m)))
}

pub fn get_space_suggestion(expression_text: &str) -> String {
    if let Some(result) = VIABLE_KEYWORD_SUGGESTIONS.iter().find_map(|keyword| {
        if expression_text.len() > keyword.len() + 2 && expression_text.starts_with(keyword) {
            Some(format!("{} {}", keyword, &expression_text[keyword.len()..]))
        } else {
            None
        }
    }) {
        return result;
    }
    String::new()
}

impl Parser {
    pub fn current_import_phase_modifier(&self) -> Kind {
        match self.scanner.token_text() {
            "defer" => Kind::DeferKeyword,
            "source" => Kind::SourceKeyword,
            _ => Kind::Unknown,
        }
    }

    pub fn parse_module_specifier(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_module_specifier(self)
    }

    pub fn try_parse_import_clause(
        &mut self,
        identifier: Option<P<Node>>,
        pos: u32,
        phase_modifier: Kind,
        skip_jsdoc_leading_asterisks: bool,
    ) -> Option<P<Node>> {
        crate::type_grammar::TypeParser::try_parse_import_clause(
            self,
            identifier,
            pos,
            phase_modifier,
            skip_jsdoc_leading_asterisks,
        )
    }
}

const _: () = assert!(ParsingContext::ClassMembers as u8 == tsrs_ast::lazylist::CLASS_MEMBERS);

/// A member list longer than this (in text) stays eager: parsing and binding it again takes a millisecond or
/// more, while every checker that needs it waits (the generated `OpenAPI` `operations` / `components`
/// interfaces of supabase-studio, 65-437 KB each, are needed by most checkers). Keeping them eager costs at
/// most 0.5% of peak on the bench projects (notes/mem-lazy-dts-members.md).
const LAZY_LIST_MAX_TEXT: u32 = 64 << 10;

static LAZY_DTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// tsrs-only: parse the member lists of declaration files lazily from now on (the CLI with `skipLibCheck`;
/// `tsrs_ast` `lazylist`, notes/mem-lazy-dts-members.md).
///
/// Process-wide; the binder must be enabled too (`tsrs_binder`).
pub fn enable_lazy_dts() {
    tsrs_ast::lazylist::set_parse_hook(reparse_lazy_list);
    // Relaxed: the CLI enables it before creating the program; parser threads start after that.
    LAZY_DTS.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Parses a lazy member list again from its recorded position and context (`parse_member_list_lazily`): the same
/// parser code from the same scanner position, so the same nodes; their parent is set to the owner.
fn reparse_lazy_list(
    record: &tsrs_ast::lazylist::LazyNodeList,
    file: P<SourceFile>,
) -> &'static [P<Node>] {
    crate::jsdoc::init();
    let mut p = new_parser();
    p.initialize_state_static(file.parse_options().clone(), file.text(), file.script_kind());
    p.source_text_index = file.text_index.get();
    p.scanner.reset_token_state(record.pos);
    p.next_token();
    p.context_flags = record.context_flags;
    p.parsing_contexts = record.parsing_contexts as ParsingContexts;
    let list = if record.parsing_context == ParsingContext::ClassMembers as u8 {
        p.parse_list(ParsingContext::ClassMembers, Parser::parse_class_element)
    } else {
        debug_assert_eq!(record.parsing_context, ParsingContext::TypeMembers as u8);
        p.parse_list(ParsingContext::TypeMembers, Parser::parse_type_member)
    };
    assert!(
        p.diagnostics.is_empty()
            && list.pos().source_offset() == Some(record.pos)
            && list.end().source_offset() == Some(record.end)
            && p.token == Kind::CloseBraceToken,
        "lazy member list of {} at {} parsed differently",
        file.file_name(),
        record.pos
    );
    let owner = record.owner();
    let nodes = list.nodes();
    for &m in nodes {
        m.set_parent(Some(owner));
    }
    nodes
}

/// Whether binding a member list could reach outside it, which a lazy list must not (the binder binds it
/// later with the state it had at the list, `tsrs_binder` `bind_lazy_list`): `this` (sets `ContainsThis` on
/// the enclosing containers), `infer` (declares its type parameter in the enclosing conditional type),
/// bodies, initializers, decorators, `async`, and expressions that make flow nodes or flow effects; import
/// types and calls (the file's imports are collected from the tree when the file is parsed).
fn lazy_list_blocked(nodes: &[P<Node>]) -> bool {
    fn blocked(n: P<Node>) -> bool {
        let hit = match n.kind() {
            Kind::ThisKeyword
            | Kind::ThisType
            | Kind::SuperKeyword
            | Kind::ImportType
            | Kind::ImportKeyword
            | Kind::MetaProperty
            | Kind::InferType
            | Kind::Block
            | Kind::Decorator
            | Kind::AsyncKeyword
            | Kind::CallExpression
            | Kind::NewExpression
            | Kind::BinaryExpression
            | Kind::ConditionalExpression
            | Kind::ElementAccessExpression
            | Kind::PostfixUnaryExpression
            | Kind::ArrowFunction
            | Kind::FunctionExpression
            | Kind::ClassExpression
            | Kind::ObjectLiteralExpression
            | Kind::ArrayLiteralExpression
            | Kind::TaggedTemplateExpression
            | Kind::TemplateExpression
            | Kind::ParenthesizedExpression
            | Kind::AwaitExpression
            | Kind::YieldExpression
            | Kind::DeleteExpression
            | Kind::TypeOfExpression
            | Kind::VoidExpression
            | Kind::NonNullExpression
            | Kind::AsExpression
            | Kind::SatisfiesExpression
            | Kind::TypeAssertionExpression
            | Kind::SpreadElement => true,
            Kind::PrefixUnaryExpression => {
                let u = n.as_prefix_unary_expression();
                !matches!(u.operator, Kind::MinusToken | Kind::PlusToken)
                    || !matches!(u.operand.kind(), Kind::NumericLiteral | Kind::BigIntLiteral)
            }
            Kind::PropertyAccessExpression => n.flags().intersects(NodeFlags::OptionalChain),
            Kind::PropertyDeclaration
            | Kind::PropertySignature
            | Kind::Parameter
            | Kind::VariableDeclaration
            | Kind::BindingElement => n.initializer().is_some(),
            _ => false,
        };
        hit || n.for_each_child(&mut |c| blocked(c))
    }
    nodes.iter().any(|&n| blocked(n))
}
