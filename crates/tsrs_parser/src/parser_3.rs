use std::fmt::Display;

use rustc_hash::FxHashMap;
use tsrs_ast as ast;
use tsrs_ast::{DiagnosticExt, Kind, ModifierFlags, ModifierList, Node, NodeFlags, NodeList};
use tsrs_core::{P, TextRange};
use tsrs_diagnostics as diagnostics;
use tsrs_diagnostics::Message;
use tsrs_scanner as scanner;

use crate::parser_1::{Parser, is_export_modifier};

impl Parser {
    pub fn scan_jsx_text(&mut self) -> Kind {
        self.token = self.scanner.scan_jsx_token();
        self.report_scan_errors();
        self.token
    }

    pub fn scan_jsx_identifier(&mut self) -> Kind {
        self.token = self.scanner.scan_jsx_identifier();
        self.report_scan_errors();
        self.token
    }

    pub fn scan_jsx_attribute_value(&mut self) -> Kind {
        self.token = self.scanner.scan_jsx_attribute_value();
        self.report_scan_errors();
        self.token
    }

    pub fn parse_prefix_unary_expression(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_prefix_unary_expression(self)
    }

    pub fn parse_array_literal_expression(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_array_literal_expression(self)
    }

    pub fn parse_object_literal_expression(&mut self) -> P<Node> {
        crate::type_grammar::TypeParser::parse_object_literal_expression(self)
    }

    pub fn parse_literal_expression(&mut self) -> P<Node> {
        crate::lexical::LexicalParser::parse_literal_expression(self)
    }

    pub fn parse_identifier_name(&mut self) -> P<Node> {
        self.parse_identifier_name_with_diagnostic(None)
    }

    pub fn parse_identifier_name_with_diagnostic(
        &mut self,
        diagnostic_message: Option<&'static Message>,
    ) -> P<Node> {
        crate::lexical::LexicalParser::parse_identifier_name_with_diagnostic(
            self,
            diagnostic_message,
        )
    }

    pub fn parse_identifier(&mut self) -> P<Node> {
        crate::lexical::LexicalParser::parse_identifier(self)
    }

    pub fn create_identifier(&mut self, is_identifier: bool) -> P<Node> {
        self.create_identifier_with_diagnostic(is_identifier, None, None)
    }

    fn create_identifier_with_diagnostic(
        &mut self,
        is_identifier: bool,
        diagnostic_message: Option<&'static Message>,
        private_identifier_diagnostic_message: Option<&'static Message>,
    ) -> P<Node> {
        crate::lexical::LexicalParser::create_identifier_with_diagnostic(
            self,
            is_identifier,
            diagnostic_message,
            private_identifier_diagnostic_message,
        )
    }

    pub fn new_node_list(&mut self, loc: TextRange, nodes: &[P<Node>]) -> P<NodeList> {
        let list = self.factory.new_node_list_from_slice(nodes);
        list.loc.set(loc);
        list
    }

    pub fn new_modifier_list(&mut self, loc: TextRange, nodes: &[P<Node>]) -> P<ModifierList> {
        let list = self.factory.new_modifier_list_from_slice(nodes);
        list.list.loc.set(loc);
        list
    }

    pub fn finish_node(&mut self, node: P<Node>, pos: u32) -> P<Node> {
        let end = self.node_pos();
        self.finish_node_with_end(node, pos, end)
    }

    pub fn finish_node_with_end(&mut self, node: P<Node>, pos: u32, end: u32) -> P<Node> {
        crate::construction::finish_node(
            &mut self.factory,
            node,
            TextRange::new(pos, end),
            self.context_flags,
            &mut self.has_parse_error,
        )
        .unwrap_or_else(|never| match never {})
    }

    pub fn override_parent_in_immediate_children(&mut self, node: P<Node>) {
        crate::construction::override_child_parents(&mut self.factory, node)
            .unwrap_or_else(|never| match never {});
    }

    // Ignore strict mode flag because we will report an error in type checker instead.
    pub fn is_identifier(&self) -> bool {
        crate::lexical::LexicalParser::is_identifier(self)
    }

    pub fn set_context_flags(&mut self, flags: NodeFlags, value: bool) {
        crate::context::ContextParser::set_context_flags(self, flags, value);
    }

    pub fn skip_range_trivia(&self, text_range: TextRange) -> TextRange {
        TextRange::new(
            scanner::skip_trivia(
                self.source_text,
                text_range.pos().expect_source("parsed ranges have source positions"),
            ),
            text_range.end().expect_source("parsed ranges have source positions"),
        )
    }

    pub fn process_pragmas_into_fields(&mut self, context: P<ast::SourceFile>) {
        let mut check_js_directive: Option<P<ast::CheckJsDirective>> = None;
        let mut referenced_files: Vec<P<ast::FileReference>> = Vec::new();
        let mut type_reference_directives: Vec<P<ast::FileReference>> = Vec::new();
        let mut lib_reference_directives: Vec<P<ast::FileReference>> = Vec::new();
        // context.AmdDependencies = nil
        for pragma in context.pragmas() {
            match &*pragma.name {
                "reference" => {
                    let types = pragma.args.get("types");
                    let lib = pragma.args.get("lib");
                    let path = pragma.args.get("path");
                    let resolution_mode = pragma.args.get("resolution-mode");
                    let preserve = pragma.args.get("preserve");
                    let no_default_lib = pragma.args.get("no-default-lib");
                    if no_default_lib.is_some_and(|no_default_lib| no_default_lib.value == "true") {
                        // Ignored.
                    } else if let Some(types) = types {
                        let mut parsed = tsrs_core::ResolutionMode::default();
                        if let Some(resolution_mode) = resolution_mode {
                            parsed = self.parse_resolution_mode(
                                &resolution_mode.value,
                                resolution_mode
                                    .text_range
                                    .pos()
                                    .expect_source("pragma ranges have source positions"),
                                resolution_mode
                                    .text_range
                                    .end()
                                    .expect_source("pragma ranges have source positions"),
                            );
                        }
                        type_reference_directives.push(P::new(ast::FileReference {
                            text_range: types.text_range,
                            file_name: types.value.clone(),
                            resolution_mode: parsed,
                            preserve: preserve.is_some_and(|preserve| preserve.value == "true"),
                        }));
                    } else if let Some(lib) = lib {
                        lib_reference_directives.push(P::new(ast::FileReference {
                            text_range: lib.text_range,
                            file_name: lib.value.clone(),
                            resolution_mode: tsrs_core::ResolutionMode::default(),
                            preserve: preserve.is_some_and(|preserve| preserve.value == "true"),
                        }));
                    } else if let Some(path) = path {
                        referenced_files.push(P::new(ast::FileReference {
                            text_range: path.text_range,
                            file_name: path.value.clone(),
                            resolution_mode: tsrs_core::ResolutionMode::default(),
                            preserve: preserve.is_some_and(|preserve| preserve.value == "true"),
                        }));
                    } else {
                        self.parse_error_at_range(
                            pragma.comment_range.text_range,
                            &diagnostics::INVALID_REFERENCE_DIRECTIVE_SYNTAX,
                            &[],
                        );
                    }
                }
                "ts-check" | "ts-nocheck" => {
                    // _last_ of either nocheck or check in a file is the "winner"
                    if check_js_directive.is_none_or(|directive| {
                        pragma.comment_range.text_range.pos() > directive.range.text_range.pos()
                    }) {
                        check_js_directive = Some(P::new(ast::CheckJsDirective {
                            enabled: &*pragma.name == "ts-check",
                            range: pragma.comment_range,
                        }));
                    }
                }
                "jsx" | "jsxfrag" | "jsximportsource" | "jsxruntime" => {
                    // Nothing to do here
                }
                _ => panic!("Unhandled pragma kind: {}", pragma.name),
            }
        }
        context.check_js_directive.set(check_js_directive);
        context.referenced_files.set(tsrs_core::alloc_vec(referenced_files));
        context.type_reference_directives.set(tsrs_core::alloc_vec(type_reference_directives));
        context.lib_reference_directives.set(tsrs_core::alloc_vec(lib_reference_directives));
    }

    fn parse_resolution_mode(
        &mut self,
        mode: &str,
        pos: u32,
        end: u32,
    ) -> tsrs_core::ResolutionMode {
        let mut resolution_kind = tsrs_core::ResolutionMode::default();
        if mode == "import" {
            resolution_kind = tsrs_core::ModuleKind::ES_NEXT;
            return resolution_kind;
        }
        if mode == "require" {
            resolution_kind = tsrs_core::ModuleKind::COMMON_JS;
            return resolution_kind;
        }
        self.parse_error_at(
            pos,
            end,
            &diagnostics::X_RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT,
            &[],
        );
        resolution_kind
    }

    fn js_error_at_range(
        &mut self,
        loc: TextRange,
        message: &'static Message,
        args: &[&dyn Display],
    ) {
        let range = TextRange::new(
            scanner::skip_trivia(
                self.source_text,
                loc.pos().expect_source("parsed ranges have source positions"),
            ),
            loc.end().expect_source("parsed ranges have source positions"),
        );
        self.js_diagnostics.push(ast::new_diagnostic(None, range, message, args));
    }

    fn check_js_decorator_syntax(&mut self, node: P<Node>) {
        let modifiers = node.modifier_nodes();
        if modifiers.is_empty() {
            return;
        }

        if ast::can_have_illegal_decorators(node) {
            if let Some(modifier) = modifiers.iter().find_map(|&modifier| {
                if ast::is_decorator(modifier) { Some(modifier) } else { None }
            }) {
                self.js_error_at_range(
                    modifier.loc(),
                    &diagnostics::DECORATORS_ARE_NOT_VALID_HERE,
                    &[],
                );
            }
        } else if ast::can_have_decorators(node)
            && let Some(decorator_index) = find_index(modifiers, ast::is_decorator)
            && ast::is_class_declaration(node)
            && let Some(export_index) = find_index(modifiers, is_export_modifier)
        {
            let default_index = find_index(modifiers, |m| m.kind() == Kind::DefaultKeyword);
            if decorator_index > export_index
                && default_index.is_some_and(|default_index| decorator_index < default_index)
            {
                // Decorator between `export` and `default`
                self.js_error_at_range(
                    modifiers[decorator_index].loc(),
                    &diagnostics::DECORATORS_ARE_NOT_VALID_HERE,
                    &[],
                );
            } else if decorator_index < export_index {
                // Find a trailing decorator after the export keyword
                let trailing_decorator_index = modifiers[export_index..]
                    .iter()
                    .position(|&modifier| ast::is_decorator(modifier))
                    .map(|index| index + export_index);
                if let Some(trailing_decorator_index) = trailing_decorator_index {
                    let trailing = modifiers[trailing_decorator_index];
                    let diag = ast::new_diagnostic(
                                    None,
                                    TextRange::new(
                                        scanner::skip_trivia(
                                            self.source_text,
                                            trailing.loc().pos().expect_source("parsed nodes have source positions"),
                                        ),
                                        trailing.loc().end().expect_source("parsed nodes have source positions"),
                                    ),
                                    &diagnostics::DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEFORE_EXPORT,
                                    &[],
                                );
                    let decorator = modifiers[decorator_index];
                    diag.add_related_info(ast::new_diagnostic(
                        None,
                        TextRange::new(
                            scanner::skip_trivia(
                                self.source_text,
                                decorator
                                    .loc()
                                    .pos()
                                    .expect_source("parsed nodes have source positions"),
                            ),
                            decorator
                                .loc()
                                .end()
                                .expect_source("parsed nodes have source positions"),
                        ),
                        &diagnostics::DECORATOR_USED_BEFORE_EXPORT_HERE,
                        &[],
                    ));
                    self.js_diagnostics.push(diag);
                }
            }
        }
    }

    #[inline]
    pub fn check_js_syntax(&mut self, node: P<Node>) -> P<Node> {
        if !node.flags().intersects(NodeFlags::JavaScriptFile)
            || node.flags().intersects(NodeFlags::JSDoc | NodeFlags::Reparsed)
        {
            return node;
        }
        self.check_js_syntax_worker(node)
    }

    fn check_js_syntax_worker(&mut self, node: P<Node>) -> P<Node> {
        match node.kind() {
            Kind::Parameter
            | Kind::PropertyDeclaration
            | Kind::MethodDeclaration
            | Kind::MethodSignature
            | Kind::Constructor
            | Kind::GetAccessor
            | Kind::SetAccessor
            | Kind::FunctionExpression
            | Kind::FunctionDeclaration
            | Kind::ArrowFunction
            | Kind::VariableDeclaration
            | Kind::IndexSignature => {
                if matches!(
                    node.kind(),
                    Kind::Parameter | Kind::PropertyDeclaration | Kind::MethodDeclaration
                ) && let Some(token) = node.question_token()
                    && !token.flags().intersects(NodeFlags::Reparsed)
                    && ast::is_question_token(token)
                {
                    self.js_error_at_range(
                        token.loc(),
                        &diagnostics::THE_0_MODIFIER_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[&"?"],
                    );
                }
                // fallthrough
                if ast::is_function_like(Some(node)) && node.body().is_none() {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::SIGNATURE_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                } else if let Some(t) =
                    node.type_node().filter(|t| !t.flags().intersects(NodeFlags::Reparsed))
                {
                    self.js_error_at_range(
                        t.loc(),
                        &diagnostics::TYPE_ANNOTATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                }
            }
            Kind::ImportDeclaration => {
                if node.import_clause().is_some_and(|clause| clause.is_type_only()) {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[&"import type"],
                    );
                }
            }
            Kind::ExportDeclaration => {
                if node.is_type_only() {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[&"export type"],
                    );
                }
            }
            Kind::ImportSpecifier => {
                if node.is_type_only() {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[&"import...type"],
                    );
                }
            }
            Kind::ExportSpecifier => {
                if node.is_type_only() {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[&"export...type"],
                    );
                }
            }
            Kind::ImportEqualsDeclaration => {
                self.js_error_at_range(
                    node.loc(),
                    &diagnostics::X_IMPORT_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[],
                );
            }
            Kind::ExportAssignment => {
                if node.as_export_assignment().is_export_equals {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_EXPORT_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                }
            }
            Kind::HeritageClause => {
                if node.as_heritage_clause().token == Kind::ImplementsKeyword {
                    self.js_error_at_range(
                        node.loc(),
                        &diagnostics::X_IMPLEMENTS_CLAUSES_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                }
            }
            Kind::InterfaceDeclaration => {
                self.js_error_at_range(
                    node.name().unwrap().loc(),
                    &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[&"interface"],
                );
            }
            Kind::ModuleDeclaration => {
                let keyword = scanner::token_to_string(node.as_module_declaration().keyword);
                self.js_error_at_range(
                    node.name().unwrap().loc(),
                    &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[&keyword],
                );
            }
            Kind::TypeAliasDeclaration => {
                self.js_error_at_range(
                    node.name().unwrap().loc(),
                    &diagnostics::TYPE_ALIASES_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[],
                );
            }
            Kind::EnumDeclaration => {
                self.js_error_at_range(
                    node.name().unwrap().loc(),
                    &diagnostics::X_0_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[&"enum"],
                );
            }
            Kind::NonNullExpression => {
                self.js_error_at_range(
                    node.loc(),
                    &diagnostics::NON_NULL_ASSERTIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[],
                );
            }
            Kind::AsExpression => {
                self.js_error_at_range(
                    node.type_node().unwrap().loc(),
                    &diagnostics::TYPE_ASSERTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                    &[],
                );
            }
            Kind::SatisfiesExpression => {
                self.js_error_at_range(node.type_node().unwrap().loc(), &diagnostics::TYPE_SATISFACTION_EXPRESSIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES, &[]);
            }
            _ => {}
        }
        // Check decorator placement in JS files
        self.check_js_decorator_syntax(node);
        // Check absence of type parameters, type arguments and non-JavaScript modifiers
        match node.kind() {
            Kind::ClassDeclaration
            | Kind::ClassExpression
            | Kind::MethodDeclaration
            | Kind::Constructor
            | Kind::GetAccessor
            | Kind::SetAccessor
            | Kind::FunctionExpression
            | Kind::FunctionDeclaration
            | Kind::ArrowFunction
            | Kind::VariableStatement
            | Kind::PropertyDeclaration => {
                if !matches!(node.kind(), Kind::VariableStatement | Kind::PropertyDeclaration)
                    && let Some(list) = node.type_parameter_list()
                    && list.nodes().iter().any(|n| !n.flags().intersects(NodeFlags::Reparsed))
                {
                    self.js_error_at_range(list.loc.get(), &diagnostics::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES, &[]);
                }
                // fallthrough
                for &modifier in node.modifier_nodes() {
                    if !modifier.flags().intersects(NodeFlags::Reparsed)
                        && modifier.kind() != Kind::Decorator
                        && !ast::modifier_to_flag(modifier.kind())
                            .intersects(ModifierFlags::JavaScript)
                    {
                        let text = scanner::token_to_string(modifier.kind());
                        self.js_error_at_range(
                            modifier.loc(),
                            &diagnostics::THE_0_MODIFIER_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                            &[&text],
                        );
                    }
                }
            }
            Kind::Parameter => {
                if node.modifier_nodes().iter().any(|&m| ast::is_modifier(m)) {
                    self.js_error_at_range(
                        node.modifiers().unwrap().list.loc.get(),
                        &diagnostics::PARAMETER_MODIFIERS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                }
            }
            Kind::CallExpression
            | Kind::NewExpression
            | Kind::ExpressionWithTypeArguments
            | Kind::JsxSelfClosingElement
            | Kind::JsxOpeningElement
            | Kind::TaggedTemplateExpression => {
                if let Some(list) = node.type_argument_list()
                    && list.nodes().iter().any(|n| !n.flags().intersects(NodeFlags::Reparsed))
                {
                    self.js_error_at_range(
                        list.loc.get(),
                        &diagnostics::TYPE_ARGUMENTS_CAN_ONLY_BE_USED_IN_TYPE_SCRIPT_FILES,
                        &[],
                    );
                }
            }
            _ => {}
        }
        node
    }
}

pub fn is_reserved_word(token: Kind) -> bool {
    Kind::FIRST_RESERVED_WORD <= token && token <= Kind::LAST_RESERVED_WORD
}

pub fn attach_file_to_diagnostics(
    diagnostics: &[P<ast::Diagnostic>],
    file: P<ast::SourceFile>,
) -> Vec<P<ast::Diagnostic>> {
    for d in diagnostics {
        d.set_file(Some(file));
        for r in d.related_information() {
            r.set_file(Some(file));
        }
    }
    diagnostics.to_vec()
}

pub fn get_comment_pragmas(_f: &mut ast::NodeFactory, source_text: &str) -> Vec<ast::Pragma> {
    let mut pragmas = Vec::new();
    pragmas.extend(scanner::get_leading_comment_ranges(source_text, 0).flat_map(|comment_range| {
        let comment = &source_text[comment_range.pos() as usize..comment_range.end() as usize];
        extract_pragmas(comment_range, comment)
    }));
    pragmas
}

fn extract_pragmas(comment_range: ast::CommentRange, text: &str) -> Vec<ast::Pragma> {
    let mut text = text;
    if comment_range.kind == Kind::SingleLineCommentTrivia {
        let mut pos: usize = 2;
        let triple_slash = match_(text, pos, "/");
        if triple_slash {
            pos += 1;
        }
        pos = skip_blanks(text, pos);
        if triple_slash && match_(text, pos, "<") {
            let tag_name = extract_name(text, pos + 1);
            if tag_name != "reference" {
                return Vec::new();
            }
            pos += 10;
            let mut args: FxHashMap<String, ast::PragmaArgument> = FxHashMap::default();
            loop {
                pos = skip_blanks(text, pos);
                if match_(text, pos, "/>") {
                    break;
                }
                let arg_name = extract_name(text, pos);
                if arg_name.is_empty() {
                    break;
                }
                pos = skip_blanks(text, pos + arg_name.len());
                if !match_(text, pos, "=") {
                    break;
                }
                pos = skip_blanks(text, pos + 1);
                let Some(value) = extract_quoted_string(text, pos) else {
                    break;
                };
                args.insert(
                    arg_name.clone(),
                    ast::PragmaArgument {
                        text_range: TextRange::new(
                            comment_range.pos() + pos as u32 + 1,
                            comment_range.pos() + pos as u32 + 1 + value.len() as u32,
                        ),
                        name: arg_name.clone(),
                        value: value.to_string(),
                    },
                );
                pos += value.len() + 2;
            }
            return vec![ast::Pragma { comment_range, name: "reference".to_string(), args }];
        }
        if match_(text, pos, "@") {
            pos += 1;
            let pragma_name = extract_name(text, pos);
            if !(pragma_name == "ts-check" || pragma_name == "ts-nocheck") {
                return Vec::new();
            }
            return vec![ast::Pragma {
                comment_range,
                name: pragma_name,
                args: FxHashMap::default(),
            }];
        }
    }
    if comment_range.kind == Kind::MultiLineCommentTrivia {
        text = text.strip_suffix("*/").unwrap_or(text);
        let mut pos: usize = 2;
        let mut pragmas = Vec::new();
        while let Some(found) = skip_to(text, pos, "@") {
            pos = found;
            // Mirrors the /@(\S+)(\s+(?:\S.*)?)?$/gm pragma regex used by TypeScript: the '@'
            // must be immediately followed by a non-whitespace pragma name, and the remainder
            // of the line is consumed as that pragma's arguments. As a consequence, only the
            // first '@'-token on a line is considered, so an unrelated '@token' earlier on the
            // line (e.g. an email address) prevents a later '@jsx' on the same line from being
            // treated as a pragma.
            let name_pos = pos + 1;
            let name_end = skip_non_blanks(text, name_pos);
            if name_end == name_pos {
                pos += 1;
                continue;
            }
            let line_end = line_end_pos(text, pos);
            let pragma_name = text[name_pos..name_end].to_lowercase();
            if pragma_name == "jsx"
                || pragma_name == "jsxfrag"
                || pragma_name == "jsximportsource"
                || pragma_name == "jsxruntime"
            {
                let start = skip_blanks(text, name_end);
                let arg_end = skip_non_blanks(text, start);
                if arg_end != start {
                    let mut args: FxHashMap<String, ast::PragmaArgument> =
                        FxHashMap::with_capacity_and_hasher(1, rustc_hash::FxBuildHasher);
                    args.insert(
                        "factory".to_string(),
                        ast::PragmaArgument {
                            text_range: TextRange::new(
                                comment_range.pos() + start as u32,
                                comment_range.pos() + arg_end as u32,
                            ),
                            name: "factory".to_string(),
                            value: text[start..arg_end].to_string(),
                        },
                    );
                    pragmas.push(ast::Pragma { comment_range, name: pragma_name.clone(), args });
                }
            }
            pos = line_end;
        }
        return pragmas;
    }
    Vec::new()
}

fn match_(text: &str, pos: usize, s: &str) -> bool {
    text.as_bytes()[pos..].starts_with(s.as_bytes())
}

fn skip_blanks(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let mut pos = pos;
    pos += (pos..bytes.len())
        .take_while(|&index| bytes[index] == b' ' || bytes[index] == b'\t')
        .count();
    pos
}

fn skip_non_blanks(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let mut pos = pos;
    pos += (pos..bytes.len())
        .take_while(|&index| {
            bytes[index] != b' '
                && bytes[index] != b'\t'
                && bytes[index] != b'\r'
                && bytes[index] != b'\n'
        })
        .count();
    pos
}

fn skip_to(text: &str, pos: usize, s: &str) -> Option<usize> {
    if pos >= text.len() {
        return None;
    }
    let haystack = &text.as_bytes()[pos..];
    let needle = s.as_bytes();
    haystack.windows(needle.len()).position(|w| w == needle).map(|i| pos + i)
}

fn line_end_pos(text: &str, pos: usize) -> usize {
    if let Some((offset, _)) =
        text[pos..].char_indices().find(|&(_, ch)| tsrs_core::stringutil::is_line_break(ch as u32))
    {
        return pos + offset;
    }
    text.len()
}

fn extract_name(text: &str, pos: usize) -> String {
    let bytes = text.as_bytes();
    let start = pos;
    let mut pos = pos;
    pos += (pos..bytes.len())
        .take_while(|&index| {
            bytes[index] >= b'A' && bytes[index] <= b'Z'
                || bytes[index] >= b'a' && bytes[index] <= b'z'
                || bytes[index] == b'-'
        })
        .count();
    text[start..pos].to_lowercase()
}

fn extract_quoted_string(text: &str, pos: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    if pos == bytes.len() {
        return None;
    }
    let quote = bytes[pos];
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let mut pos = pos + 1;
    let start = pos;
    pos += (pos..bytes.len()).take_while(|&index| bytes[index] != quote).count();
    if pos == bytes.len() {
        return None;
    }
    Some(&text[start..pos])
}

// core.FindIndex
fn find_index(nodes: &[P<Node>], mut f: impl FnMut(P<Node>) -> bool) -> Option<usize> {
    nodes.iter().position(|&n| f(n))
}
