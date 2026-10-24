use tsrs_ast::storage::{AstStore, AstStoreLease, BindingAttempt, FileAstOwner};
use tsrs_ast::{Diagnostic, Node};
use tsrs_core::P;
use tsrs_parser::leaf_types::LeafTypeParser;
use tsrs_parser::template_tokens::TemplateTokenParser;
use tsrs_parser::tokens::TokenParser;

use std::sync::Arc;
use tsrs_ast::storage::{AstStoreBuilder, ListKey, NodeKey, ScopedNode, TextInput};
use tsrs_ast::{Kind, NodeFlags, TokenFlags};
use tsrs_core::{ScriptKind, TextRange};
use tsrs_parser::lexical::LexicalParser;
use tsrs_parser::owned::*;

#[derive(Debug, PartialEq, Eq)]
struct Shape {
    kind: Kind,
    range: TextRange,
    flags: NodeFlags,
    text: String,
    token_flags: TokenFlags,
    template_flags: TokenFlags,
    raw_text: String,
    children: Vec<Shape>,
}

fn legacy_shape(node: P<Node>, parent: Option<P<Node>>) -> Shape {
    assert_eq!(node.parent(), parent);
    let mut children = Vec::new();
    node.for_each_child_static(&mut |child| {
        children.push(legacy_shape(child, Some(node)));
        false
    });
    let token_flags = match node.kind() {
        Kind::StringLiteral => node.as_string_literal().token_flags(),
        Kind::NumericLiteral => node.as_numeric_literal().token_flags(),
        Kind::BigIntLiteral => node.as_big_int_literal().token_flags(),
        Kind::RegularExpressionLiteral => node.as_regular_expression_literal().token_flags(),
        Kind::NoSubstitutionTemplateLiteral
        | Kind::TemplateHead
        | Kind::TemplateMiddle
        | Kind::TemplateTail => node.literal_like_data().unwrap().token_flags.get(),
        _ => TokenFlags::None,
    };
    Shape {
        kind: node.kind(),
        range: node.loc(),
        flags: node.flags(),
        text: if matches!(
            node.kind(),
            Kind::Identifier
                | Kind::PrivateIdentifier
                | Kind::TemplateHead
                | Kind::TemplateMiddle
                | Kind::TemplateTail
        ) || tsrs_ast::is_literal_kind(node.kind())
        {
            node.text().to_owned()
        } else {
            String::new()
        },
        token_flags,
        template_flags: match node.kind() {
            Kind::NoSubstitutionTemplateLiteral => {
                node.as_no_substitution_template_literal().template_flags()
            }
            Kind::TemplateHead => node.as_template_head().template_flags(),
            Kind::TemplateMiddle => node.as_template_middle().template_flags(),
            Kind::TemplateTail => node.as_template_tail().template_flags(),
            _ => TokenFlags::None,
        },
        raw_text: match node.kind() {
            Kind::NoSubstitutionTemplateLiteral
            | Kind::TemplateHead
            | Kind::TemplateMiddle
            | Kind::TemplateTail => node.template_literal_like_data().unwrap().raw_text,
            _ => "",
        }
        .to_owned(),
        children,
    }
}

fn owned_shape(ast: &AstStoreBuilder, key: NodeKey, parent: Option<NodeKey>) -> Shape {
    let node = ast.node(key).unwrap();
    assert_eq!(node.parent(), parent);
    let mut shape = Shape {
        kind: node.kind(),
        range: node.range(),
        flags: node.flags(),
        text: String::new(),
        token_flags: TokenFlags::None,
        template_flags: TokenFlags::None,
        raw_text: String::new(),
        children: Vec::new(),
    };
    macro_rules! literal {
        ($cast:ident) => {{
            let literal = node.$cast().unwrap();
            literal.text().clone_into(&mut shape.text);
            shape.token_flags = literal.token_flags();
        }};
    }
    match node.kind() {
        Kind::Identifier => node.as_identifier().unwrap().text().clone_into(&mut shape.text),
        Kind::PrivateIdentifier => {
            node.as_private_identifier().unwrap().text().clone_into(&mut shape.text);
        }
        Kind::QualifiedName => {
            let name = node.as_qualified_name().unwrap();
            shape.children.push(owned_shape(ast, name.left(), Some(key)));
            shape.children.push(owned_shape(ast, name.right(), Some(key)));
        }
        Kind::LiteralType => {
            let literal = node.as_literal_type_node().unwrap().literal();
            shape.children.push(owned_shape(ast, literal, Some(key)));
        }
        Kind::PrefixUnaryExpression => {
            let operand = node.as_prefix_unary_expression().unwrap().operand();
            shape.children.push(owned_shape(ast, operand, Some(key)));
        }
        Kind::ArrayLiteralExpression => {
            let list = ast.list(node.as_array_literal_expression().unwrap().elements()).unwrap();
            shape.children.extend(list.nodes().map(|child| owned_shape(ast, child, Some(key))));
        }
        Kind::StringLiteral => literal!(as_string_literal),
        Kind::NumericLiteral => literal!(as_numeric_literal),
        Kind::BigIntLiteral => literal!(as_big_int_literal),
        Kind::RegularExpressionLiteral => literal!(as_regular_expression_literal),
        Kind::NoSubstitutionTemplateLiteral => {
            literal!(as_no_substitution_template_literal);
            let literal = node.as_no_substitution_template_literal().unwrap();
            shape.template_flags = literal.template_flags();
            literal.raw_text().clone_into(&mut shape.raw_text);
        }
        Kind::TemplateHead => {
            literal!(as_template_head);
            let literal = node.as_template_head().unwrap();
            shape.template_flags = literal.template_flags();
            literal.raw_text().clone_into(&mut shape.raw_text);
        }
        Kind::TemplateMiddle => {
            literal!(as_template_middle);
            let literal = node.as_template_middle().unwrap();
            shape.template_flags = literal.template_flags();
            literal.raw_text().clone_into(&mut shape.raw_text);
        }
        Kind::TemplateTail => {
            literal!(as_template_tail);
            let literal = node.as_template_tail().unwrap();
            shape.template_flags = literal.template_flags();
            literal.raw_text().clone_into(&mut shape.raw_text);
        }
        _ => {}
    }
    shape
}

#[derive(Debug, PartialEq, Eq)]
struct ErrorShape {
    range: TextRange,
    code: i32,
    args: Vec<String>,
    related: Vec<ErrorShape>,
}

fn legacy_errors(errors: &[P<Diagnostic>]) -> Vec<ErrorShape> {
    errors
        .iter()
        .map(|error| ErrorShape {
            range: error.loc().unwrap(),
            code: error.code(),
            args: error.message_args().to_vec(),
            related: legacy_errors(error.related_information()),
        })
        .collect()
}

fn owned_errors(errors: &[OwnedParseDiagnostic]) -> Vec<ErrorShape> {
    errors
        .iter()
        .map(|error| ErrorShape {
            range: error.range,
            code: error.message.code(),
            args: error.args.clone(),
            related: owned_errors(&error.related_information),
        })
        .collect()
}

fn legacy(source: &str, kind: ScriptKind) -> tsrs_parser::Parser {
    let mut parser = tsrs_parser::parser_1::new_parser();
    parser.initialize_state(tsrs_ast::SourceFileParseOptions::default(), source, kind);
    parser.next_token();
    parser
}

#[test]
fn names_recovery_and_contexts_match_the_production_parser() {
    for source in [
        "",
        "name",
        "α.β",
        r"n\u0061me.\u0061wait",
        "name.default",
        "await.yield",
        "name..tail",
        "name.",
        "name.<T>",
        "name.\nnext keyword",
        "name.next\nkeyword",
        "name.\nnext\nkeyword",
        "#private",
        "name.#private",
        "name.#private.tail",
        r"name.\u{110000}",
        "name.42",
        "/* doc */ name /* gap */ . /* next */ value",
        r"na\u{61}me.\u{1F600}",
    ] {
        for allow_reserved in [false, true] {
            for allow_private in [false, true] {
                for context in [NodeFlags::None, NodeFlags::AwaitContext | NodeFlags::YieldContext]
                {
                    let source: Arc<str> = source.into();
                    let mut old = legacy(&source, ScriptKind::JS);
                    let mut new = OwnedParser::new(&source, ScriptKind::JS).unwrap();
                    old.context_flags |= context;
                    new.state.context_flags |= context;
                    let old_root = old.parse_entity_name(allow_reserved, allow_private, None);
                    let new_root = new.parse_entity_name(allow_reserved, allow_private, None);
                    assert_eq!(
                        owned_shape(&new.ast, new_root, None),
                        legacy_shape(old_root, None),
                        "{source:?}"
                    );
                    assert_eq!(
                        owned_errors(&new.state.diagnostics),
                        legacy_errors(&old.diagnostics),
                        "{source:?}"
                    );
                    assert_eq!(
                        (
                            new.state.token,
                            new.state.identifier_count,
                            new.state.statement_has_await_identifier
                        ),
                        (old.token, old.identifier_count, old.statement_has_await_identifier),
                        "{source:?}"
                    );
                    assert_eq!(new.ast.node_count(), old.factory.node_count());
                    assert_eq!(new.ast.text_count(), old.factory.text_count());
                }
            }
        }
    }
}

#[test]
fn literal_text_flags_and_error_order_match_the_production_parser() {
    for source in [
        "'plain' next",
        r"'escaped\n\u{1F600}' next",
        r"'\uD800' next",
        "'unterminated\n next",
        "1_000 next",
        "0xA_B next",
        "0b101n next",
        "08 next",
        "0xG next",
        "`plain` next",
        r"`escaped\n` next",
        r"`\xZ` next",
        "`unclosed",
    ] {
        let source: Arc<str> = source.into();
        let mut old = legacy(&source, ScriptKind::TS);
        let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
        let old_root = old.parse_literal_expression();
        let new_root = new.parse_literal_expression();
        assert_eq!(
            owned_shape(&new.ast, new_root, None),
            legacy_shape(old_root, None),
            "{source:?}"
        );
        assert_eq!(
            owned_errors(&new.state.diagnostics),
            legacy_errors(&old.diagnostics),
            "{source:?}"
        );
        assert_eq!(new.state.token, old.token);
    }
}

#[test]
fn identifier_roles_preserve_binding_context_and_recovery() {
    for source in [
        "",
        "name next",
        "await next",
        "yield next",
        "default next",
        "#private next",
        r"na\u006de next",
        r"\u0061wait next",
        r"\u{110000}",
        "42 next",
    ] {
        for role in 0..7 {
            for context in [NodeFlags::None, NodeFlags::AwaitContext | NodeFlags::YieldContext] {
                for had_await in [false, true] {
                    let source: Arc<str> = Arc::from(source);
                    let mut old = legacy(&source, ScriptKind::TS);
                    let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
                    old.context_flags |= context;
                    new.state.context_flags |= context;
                    old.statement_has_await_identifier = had_await;
                    new.state.statement_has_await_identifier = had_await;
                    let (old_node, new_node) = match role {
                        0 => (Some(old.parse_identifier()), Some(new.parse_identifier())),
                        1 => (Some(old.parse_identifier_name()), Some(new.parse_identifier_name())),
                        2 => (
                            Some(old.parse_binding_identifier()),
                            Some(new.parse_binding_identifier()),
                        ),
                        3 => (
                            old.parse_optional_binding_identifier(),
                            new.parse_optional_binding_identifier(),
                        ),
                        4 => (
                            Some(old.parse_identifier_name_error_on_unicode_escape_sequence()),
                            Some(new.parse_identifier_name_error_on_unicode_escape_sequence()),
                        ),
                        5 => (
                            Some(old.parse_identifier_with_diagnostic(
                                Some(&tsrs_diagnostics::TYPE_EXPECTED),
                                None,
                            )),
                            Some(new.parse_identifier_with_diagnostic(
                                Some(&tsrs_diagnostics::TYPE_EXPECTED),
                                None,
                            )),
                        ),
                        _ => (
                            Some(old.parse_binding_identifier_with_diagnostic(Some(
                                &tsrs_diagnostics::IDENTIFIER_EXPECTED,
                            ))),
                            Some(new.parse_binding_identifier_with_diagnostic(Some(
                                &tsrs_diagnostics::IDENTIFIER_EXPECTED,
                            ))),
                        ),
                    };
                    assert_eq!(
                        new_node.map(|key| owned_shape(&new.ast, key, None)),
                        old_node.map(|node| legacy_shape(node, None)),
                        "{source:?}, role {role}, context {context:?}, saved await {had_await}",
                    );
                    assert_eq!(
                        owned_errors(&new.state.diagnostics),
                        legacy_errors(&old.diagnostics)
                    );
                    assert_eq!(
                        (
                            new.state.token,
                            new.state.statement_has_await_identifier,
                            new.state.has_parse_error
                        ),
                        (old.token, old.statement_has_await_identifier, old.has_parse_error),
                    );
                    assert_eq!(new.state.identifier_count, old.identifier_count);
                    assert_eq!(new.ast.node_count(), old.factory.node_count());
                    assert_eq!(new.ast.text_count(), old.factory.text_count());
                }
            }
        }
    }
}

#[test]
fn expected_tokens_preserve_ranges_flags_error_suppression_and_advancement() {
    for source in ["", " \n", "name next", ", next", "( next", "'text' next", r"\u{110000}"] {
        for kind in [
            Kind::EndOfFile,
            Kind::Identifier,
            Kind::CommaToken,
            Kind::OpenParenToken,
            Kind::StringLiteral,
        ] {
            let source: Arc<str> = Arc::from(source);
            let mut old = legacy(&source, ScriptKind::JS);
            let mut new = OwnedParser::new(&source, ScriptKind::JS).unwrap();
            assert_eq!(
                new.parse_expected_without_advancing(kind),
                old.parse_expected_without_advancing(kind)
            );
            assert_eq!(new.state.token, old.token);
            let old_node = old.parse_expected_token(kind);
            let new_node = new.parse_expected_token(kind);
            assert_eq!(
                owned_shape(&new.ast, new_node, None),
                legacy_shape(old_node, None),
                "{source:?}, expected {kind:?}"
            );
            assert_eq!(new.state.token, old.token);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            let old_optional = old.parse_optional_token(kind);
            let new_optional = new.parse_optional_token(kind);
            assert_eq!(
                new_optional.map(|key| owned_shape(&new.ast, key, None)),
                old_optional.map(|node| legacy_shape(node, None)),
            );
            assert_eq!(new.parse_optional(kind), old.parse_optional(kind));
            assert_eq!(new.parse_expected(kind), old.parse_expected(kind));
            for should_advance in [false, true] {
                assert_eq!(
                    new.parse_expected_with_diagnostic(
                        kind,
                        Some(&tsrs_diagnostics::IDENTIFIER_EXPECTED),
                        should_advance
                    ),
                    old.parse_expected_with_diagnostic(
                        kind,
                        Some(&tsrs_diagnostics::IDENTIFIER_EXPECTED),
                        should_advance
                    ),
                );
            }
            assert_eq!(new.state.token, old.token);
            assert_eq!(new.state.has_parse_error, old.has_parse_error);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            assert_eq!(new.ast.node_count(), old.factory.node_count());
        }
    }
}

#[test]
fn matching_brackets_preserve_related_information_and_duplicate_suppression() {
    for source in ["(", "( value", "( ) next", "\n[", "/* gap */ value", ""] {
        for open_parsed in [false, true] {
            for prior_error in [false, true] {
                let source: Arc<str> = Arc::from(source);
                let mut old = legacy(&source, ScriptKind::TS);
                let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
                let old_pos = old.scanner.token_start();
                let new_pos = new.state.scanner.token_start();
                assert_eq!(new_pos, old_pos);
                if old.token == Kind::OpenParenToken {
                    old.next_token();
                    new.next_token();
                }
                if prior_error {
                    old.parse_error_at_current_token(&tsrs_diagnostics::IDENTIFIER_EXPECTED, &[]);
                    new.state.parse_error_at_range(
                        new.state.scanner.token_range(),
                        &tsrs_diagnostics::IDENTIFIER_EXPECTED,
                        &[],
                    );
                }
                for _ in 0..2 {
                    old.parse_expected_matching_brackets(
                        Kind::OpenParenToken,
                        Kind::CloseParenToken,
                        open_parsed,
                        old_pos,
                    );
                    new.parse_expected_matching_brackets(
                        Kind::OpenParenToken,
                        Kind::CloseParenToken,
                        open_parsed,
                        new_pos,
                    );
                    assert_eq!(
                        owned_errors(&new.state.diagnostics),
                        legacy_errors(&old.diagnostics),
                        "{source:?}, opening {open_parsed}, existing error {prior_error}"
                    );
                    assert_eq!(new.state.token, old.token);
                    assert_eq!(new.state.has_parse_error, old.has_parse_error);
                }
            }
        }
    }
}

#[test]
fn owned_recovery_records_and_missing_nodes_survive_parser_and_source_disposal() {
    let source: Arc<str> = Arc::from("( /* gap */ tail");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let open = parser.state.scanner.token_start();
    assert!(parser.parse_expected(Kind::OpenParenToken));
    parser.parse_expected_matching_brackets(
        Kind::OpenParenToken,
        Kind::CloseParenToken,
        true,
        open,
    );
    let missing = parser.parse_expected_token(Kind::CloseParenToken);
    assert_eq!(parser.token(), Kind::Identifier);
    let (builder, errors) = parser.into_parts();
    drop(source);
    let lease = AstStoreLease::from(builder.finish());
    let node = lease.node(missing).unwrap();
    assert_eq!(node.kind(), Kind::CloseParenToken);
    assert_eq!(node.range(), TextRange::new(1, 1));
    assert!(node.flags().contains(NodeFlags::ThisNodeHasError));
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].args, [")"]);
    assert_eq!(errors[0].related_information.len(), 1);
    let related = &errors[0].related_information[0];
    assert_eq!(related.range, TextRange::new(0, 0));
    assert_eq!(related.args, ["(", ")"]);
    assert_eq!(
        related.message.code(),
        tsrs_diagnostics::THE_PARSER_EXPECTED_TO_FIND_A_1_TO_MATCH_THE_0_TOKEN_HERE.code()
    );
}

#[test]
fn safe_source_handoff_and_retirement_keep_parsed_text_alive() {
    let source: Arc<str> = Arc::from(r"name.escaped\u0061wait");
    let weak_source = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let root = parser.parse_entity_name(true, false, None);
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    let owner = FileAstOwner::parsed(builder, Arc::default()).unwrap();
    let BindingAttempt::Started(binding) = owner.try_begin_binding().unwrap() else {
        panic!("unbound");
    };
    let lease = binding.publish().unwrap();
    owner.retire().unwrap();
    drop(owner);
    let name = lease.node(root).unwrap().as_qualified_name().unwrap();
    let right = name.right();
    assert_eq!(lease.node(right).unwrap().as_identifier().unwrap().text(), "escapedawait");
    assert!(weak_source.upgrade().is_some());
    drop(lease);
    assert!(weak_source.upgrade().is_none());
}

#[test]
fn safe_growth_preserves_text_ranges_parents_and_explicit_speculative_retention() {
    let source: Arc<str> =
        (0..300).map(|index| format!("name{index}")).collect::<Vec<_>>().join(".").into();
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let root = parser.parse_entity_name(true, false, None);
    let mut node = root;
    for index in (1..300).rev() {
        let name = parser.ast.node(node).unwrap().as_qualified_name().unwrap();
        let right = name.right();
        assert_eq!(parser.ast.node(right).unwrap().parent(), Some(node));
        assert_eq!(
            parser.ast.node(right).unwrap().as_identifier().unwrap().text(),
            format!("name{index}")
        );
        node = name.left();
    }
    assert_eq!(parser.ast.node(node).unwrap().as_identifier().unwrap().text(), "name0");
    let (builder, _) = parser.into_parts();
    let ast = builder.finish();
    assert_eq!(ast.memory().text.allocations, 0);
    assert_eq!(ast.node_count(), 599);

    let source: Arc<str> = Arc::from(r"'cooked\n' tail");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let mut retained = None;
    assert!(LexicalParser::look_ahead(&mut parser, |parser| {
        let node = parser.parse_literal_expression();
        retained = Some(parser.retain_node(node));
        parser.token() == Kind::Identifier
    }));
    let first = retained.unwrap();
    assert_eq!(parser.token(), Kind::StringLiteral);
    let second = parser.parse_literal_expression();
    assert_ne!(first, second);
    let (builder, _) = parser.into_parts();
    let ast: AstStore = builder.finish();
    let lease = AstStoreLease::from(ast);
    for key in [first, second] {
        assert_eq!(lease.node(key).unwrap().as_string_literal().unwrap().text(), "cooked\n");
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ListShape {
    range: TextRange,
    missing: bool,
    trailing_comma: bool,
    nodes: Vec<Shape>,
}

fn legacy_list_shape(list: P<tsrs_ast::NodeList>) -> ListShape {
    ListShape {
        range: list.loc(),
        missing: tsrs_parser::parser_1::is_missing_node_list(Some(list)),
        trailing_comma: list.has_trailing_comma(),
        nodes: list.nodes().iter().map(|node| legacy_shape(*node, None)).collect(),
    }
}

fn owned_list_shape(parser: &OwnedParser<'_>, list: ListKey) -> ListShape {
    let view = parser.ast.list(list).unwrap();
    let last = view.nodes().next_back().map(|key| parser.ast.node(key).unwrap());
    ListShape {
        range: view.range(),
        missing: view.is_missing(),
        trailing_comma: view.has_trailing_comma(last.as_ref()).unwrap(),
        nodes: view.nodes().map(|key| owned_shape(&parser.ast, key, None)).collect(),
    }
}

fn legacy_list_element(parser: &mut tsrs_parser::Parser) -> P<Node> {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    if parser.token == Kind::OpenBracketToken {
        let pos = parser.node_pos();
        let list = parser
            .parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                legacy_list_element,
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            )
            .unwrap();
        let node = parser.factory.new_array_literal_expression(list, false);
        return parser.finish_node(node, pos);
    }
    if tsrs_ast::is_literal_kind(parser.token) {
        return parser.parse_literal_expression();
    }
    parser.parse_identifier_name()
}

fn owned_list_element(parser: &mut OwnedParser<'_>) -> NodeKey {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    if parser.state.token == Kind::OpenBracketToken {
        let pos = parser.state.scanner.token_full_start();
        let list = parser
            .parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                owned_list_element,
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            )
            .unwrap();
        let node = parser.new_array_literal_expression(list);
        return LexicalParser::finish_node(parser, node, pos);
    }
    if tsrs_ast::is_literal_kind(parser.state.token) {
        return parser.parse_literal_expression();
    }
    parser.parse_identifier_name()
}

#[test]
fn list_recognition_lookahead_preserves_scanner_and_parser_state() {
    use tsrs_parser::parser_1::ParsingContext;
    use tsrs_parser::recognition::RecognitionParser;
    for source in [
        "",
        ";",
        "}",
        ")",
        "]",
        ">",
        "=>",
        ",",
        ".",
        "...",
        "</tag>",
        "from 'module'",
        "from value",
        "import('module')",
        "import.name",
        "import<T>",
        "import value",
        "export type *",
        "export type { name }",
        "export type name",
        "declare type\nname",
        "namespace\nname",
        "namespace name",
        "global {",
        "public name",
        "public\nname",
        "abstract @decorator",
        "get name()",
        "readonly?",
        "await using name",
        "await\nusing name",
        "using { name }",
        "using [name]",
        "using of = value",
        "using of value",
        "using\nname",
        "extends {} {",
        "{} extends",
        "{} name",
        "{} ,",
        "this",
        "#name",
        "-1",
        "-value",
        "()",
        "(1)",
        "(...args)",
        "function",
        "yield",
        "await",
        "in",
        r"\u0061wait using name",
        "'cooked\\n'",
        "'unterminated",
        "/* comment\n */ name",
    ] {
        for context in ParsingContext::ALL {
            for flags in [
                NodeFlags::None,
                NodeFlags::YieldContext,
                NodeFlags::AwaitContext,
                NodeFlags::DisallowInContext,
            ] {
                let source: Arc<str> = Arc::from(source);
                let mut old = legacy(&source, ScriptKind::TS);
                let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
                old.context_flags = flags;
                new.state.context_flags = flags;
                let initial_range = new.state.scanner.token_range();
                let initial_errors = owned_errors(&new.state.diagnostics);
                for recovery in [false, true] {
                    assert_eq!(
                        new.is_list_element(context, recovery),
                        old.is_list_element(context, recovery),
                        "{source:?}, {context:?}, {flags:?}, recovery={recovery}"
                    );
                    assert_eq!(new.is_list_terminator(context), old.is_list_terminator(context));
                    assert_eq!(new.state.token, old.token);
                    assert_eq!(new.state.scanner.token_range(), initial_range);
                    assert_eq!(new.state.scanner.token_range(), old.scanner.token_range());
                    assert_eq!(new.state.context_flags, flags);
                    assert_eq!(owned_errors(&new.state.diagnostics), initial_errors);
                    assert_eq!(
                        owned_errors(&new.state.diagnostics),
                        legacy_errors(&old.diagnostics)
                    );
                    assert_eq!(new.state.has_parse_error, old.has_parse_error);
                }
            }
        }
    }
}

#[test]
fn delimited_lists_preserve_nested_recovery_ranges_and_trailing_commas() {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    for (context, source) in [
        (ParsingContext::ArgumentExpressions, "first,second,)"),
        (ParsingContext::ArgumentExpressions, ")"),
        (ParsingContext::ArgumentExpressions, "first second)"),
        (ParsingContext::ArgumentExpressions, "first; tail"),
        (ParsingContext::ArrayLiteralMembers, "first, [second, [third,],], fourth,]"),
        (ParsingContext::ArrayLiteralMembers, "first #private tail ]"),
        (ParsingContext::ArrayLiteralMembers, "first, [second, } tail"),
        (ParsingContext::EnumMembers, "'one'; 'two'}"),
        (ParsingContext::EnumMembers, "first second}"),
        (ParsingContext::ObjectLiteralMembers, "first; second }"),
        (ParsingContext::ObjectLiteralMembers, "first\n; second }"),
        (ParsingContext::ImportAttributes, "'type'; 'json'}"),
        (ParsingContext::Parameters, "first, 'bad\\xZZ', second)"),
        (ParsingContext::TypeParameters, "T, U >"),
        (ParsingContext::ImportOrExportSpecifiers, "name, from 'module'"),
        (ParsingContext::ArgumentExpressions, "? ? )"),
        (ParsingContext::ArgumentExpressions, ""),
    ] {
        let source: Arc<str> = Arc::from(source);
        let mut old = legacy(&source, ScriptKind::TS);
        let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
        // A caller's live stack prefix must survive nested list completion/recovery.
        let old_prefix = old.factory.new_token(Kind::ExclamationToken);
        let new_prefix = new
            .ast
            .new_token(Kind::ExclamationToken, NodeFlags::None, TextRange::synthetic())
            .unwrap();
        old.node_stack.push(old_prefix);
        new.node_stack.push(new_prefix);
        old.parsing_contexts = 1 << ParsingContext::BlockStatements as u32;
        new.state.parsing_contexts = old.parsing_contexts;
        let old_list = old.parse_delimited_list(context, legacy_list_element).unwrap();
        let new_list = new.parse_delimited_list(context, owned_list_element).unwrap();
        assert_eq!(
            owned_list_shape(&new, new_list),
            legacy_list_shape(old_list),
            "{source:?}, {context:?}"
        );
        assert_eq!(new.node_stack, [new_prefix]);
        assert_eq!(old.node_stack, [old_prefix]);
        assert_eq!(new.state.parsing_contexts, old.parsing_contexts);
        assert_eq!(new.state.token, old.token);
        assert_eq!(new.state.identifier_count, old.identifier_count);
        assert_eq!(new.ast.node_count(), old.factory.node_count());
        assert_eq!(new.state.has_parse_error, old.has_parse_error);
        assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
    }
}

#[test]
fn failed_list_callbacks_restore_outer_stack_and_context() {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    let source: Arc<str> = Arc::from("first, second, third)");
    let mut old = legacy(&source, ScriptKind::TS);
    let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let old_prefix = old.factory.new_token(Kind::ExclamationToken);
    let new_prefix =
        new.ast.new_token(Kind::ExclamationToken, NodeFlags::None, TextRange::synthetic()).unwrap();
    old.node_stack.push(old_prefix);
    new.node_stack.push(new_prefix);
    old.parsing_contexts = 1 << ParsingContext::SourceElements as u32;
    new.state.parsing_contexts = old.parsing_contexts;
    let mut old_count = 0;
    let mut new_count = 0;
    assert!(
        old.parse_delimited_list(ParsingContext::Parameters, |parser| {
            old_count += 1;
            let node = parser.parse_identifier_name();
            (old_count == 1).then_some(node)
        })
        .is_none()
    );
    assert!(
        new.parse_delimited_list(ParsingContext::Parameters, |parser| {
            new_count += 1;
            let node = parser.parse_identifier_name();
            (new_count == 1).then_some(node)
        })
        .is_none()
    );
    assert_eq!(new_count, 2);
    assert_eq!(new_count, old_count);
    assert_eq!(new.node_stack, [new_prefix]);
    assert_eq!(old.node_stack, [old_prefix]);
    assert_eq!(new.state.parsing_contexts, old.parsing_contexts);
    assert_eq!(new.state.token, Kind::CommaToken);
    assert_eq!(new.state.token, old.token);
    assert_eq!(new.ast.node_count(), old.factory.node_count());
    assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
}

#[test]
fn bracketed_lists_distinguish_missing_opening_empty_and_failed_elements() {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    for source in ["[ ] tail", "tail", "[name,] tail", "[name tail", "[name, other] tail"] {
        for fail in [false, true] {
            let source: Arc<str> = Arc::from(source);
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            let old_list = old.parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                |parser| {
                    let node = parser.parse_identifier_name();
                    (!fail).then_some(node)
                },
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            );
            let new_list = new.parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                |parser| {
                    let node = parser.parse_identifier_name();
                    (!fail).then_some(node)
                },
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            );
            assert_eq!(
                new_list.map(|list| owned_list_shape(&new, list)),
                old_list.map(legacy_list_shape),
                "{source:?}, fail={fail}",
            );
            assert_eq!(new.state.token, old.token);
            assert_eq!(new.state.has_parse_error, old.has_parse_error);
            assert_eq!(new.state.parsing_contexts, old.parsing_contexts);
            assert_eq!(new.node_stack, []);
            assert_eq!(old.node_stack, []);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            let new_empty = new.parse_empty_node_list();
            let old_empty = old.parse_empty_node_list();
            assert_eq!(owned_list_shape(&new, new_empty), legacy_list_shape(old_empty));
            assert!(!new.ast.list(new_empty).unwrap().is_missing());
        }
    }
}

#[test]
fn nonadvancing_list_elements_recover_without_looping() {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    let source: Arc<str> = Arc::from("first second)");
    let mut old = legacy(&source, ScriptKind::TS);
    let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let old_list = old
        .parse_delimited_list(ParsingContext::Parameters, |parser| {
            LexicalParser::create_missing_identifier(parser)
        })
        .unwrap();
    let new_list = new
        .parse_delimited_list(ParsingContext::Parameters, |parser| {
            LexicalParser::create_missing_identifier(parser)
        })
        .unwrap();
    assert_eq!(new.ast.list(new_list).unwrap().len(), 2);
    assert_eq!(owned_list_shape(&new, new_list), legacy_list_shape(old_list));
    assert_eq!(new.state.token, Kind::CloseParenToken);
    assert_eq!(new.state.token, old.token);
    assert_eq!(new.state.has_parse_error, old.has_parse_error);
    assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
}

#[test]
fn owned_nested_list_buffers_survive_growth_freeze_and_source_disposal() {
    use std::fmt::Write as _;
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    let mut text = String::new();
    for index in 0..40 {
        write!(&mut text, "name{index}, [inner{index},],").unwrap();
    }
    let source: Arc<str> = text.into();
    let weak = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let list = parser
        .parse_delimited_list(ParsingContext::ArrayLiteralMembers, owned_list_element)
        .unwrap();
    assert!(parser.state.diagnostics.is_empty());
    assert_eq!(parser.node_stack, []);
    assert_eq!(parser.state.parsing_contexts, 0);
    let (builder, errors) = parser.into_parts();
    drop(source);
    let lease = AstStoreLease::from(builder.finish());
    assert!(errors.is_empty());
    let view = lease.list(list).unwrap();
    assert_eq!(view.len(), 80);
    for index in 0..40 {
        let key = view.get(2 * index).unwrap();
        assert_eq!(
            lease.node(key).unwrap().as_identifier().unwrap().text(),
            format!("name{index}")
        );
        let key = view.get(2 * index + 1).unwrap();
        let array = lease.node(key).unwrap().as_array_literal_expression().unwrap();
        let inner_list = lease.list(array.elements()).unwrap();
        let inner = inner_list.get(0).unwrap();
        let inner_node = lease.node(inner).unwrap();
        assert_eq!(inner_node.as_identifier().unwrap().text(), format!("inner{index}"));
        assert_eq!(inner_node.parent(), Some(key));
        assert!(inner_list.has_trailing_comma(Some(&inner_node)).unwrap());
    }
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn indexed_lists_preserve_reparse_order_and_propagate_js_declarations() {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    for context in [
        ParsingContext::SourceElements,
        ParsingContext::BlockStatements,
        ParsingContext::TypeMembers,
    ] {
        for indexed in [false, true] {
            let source: Arc<str> = Arc::from("first; second; }");
            let mut old = legacy(&source, ScriptKind::JS);
            let mut new = OwnedParser::new(&source, ScriptKind::JS).unwrap();
            let old_prefix = old.factory.new_token(Kind::TrueKeyword);
            let new_prefix = new
                .ast
                .new_token(Kind::TrueKeyword, NodeFlags::None, TextRange::synthetic())
                .unwrap();
            old.node_stack.push(old_prefix);
            new.node_stack.push(new_prefix);
            old.reparse_list.push(old_prefix);
            new.state.reparse_list.push(new_prefix);
            let mut old_indices = Vec::new();
            let mut new_indices = Vec::new();
            let mut old_element = |parser: &mut tsrs_parser::Parser, index| {
                old_indices.push(index);
                let node = parser.parse_identifier_name();
                parser.parse_optional(Kind::SemicolonToken);
                let name = parser.factory.new_identifier("Alias");
                let alias = parser.factory.new_js_type_alias_declaration(None, name, None, None);
                let plain = parser.factory.new_token(Kind::FalseKeyword);
                let module = parser.factory.new_string_literal("module", TokenFlags::None);
                let import = parser.factory.new_js_import_declaration(None, None, module, None);
                parser.reparse_list.extend([alias, plain, import]);
                node
            };
            let mut new_element = |parser: &mut OwnedParser<'_>, index| {
                new_indices.push(index);
                let node = parser.parse_identifier_name();
                parser.parse_optional(Kind::SemicolonToken);
                let range = TextRange::synthetic();
                let name = parser
                    .ast
                    .new_identifier(NodeFlags::None, range, TextInput::Owned("Alias"))
                    .unwrap();
                let alias = parser
                    .ast
                    .new_type_alias_declaration(
                        NodeFlags::None,
                        range,
                        Kind::JSTypeAliasDeclaration,
                        name,
                        None,
                        None,
                        None,
                    )
                    .unwrap();
                let plain =
                    parser.ast.new_token(Kind::FalseKeyword, NodeFlags::None, range).unwrap();
                let module = parser
                    .ast
                    .new_string_literal(
                        NodeFlags::None,
                        range,
                        TextInput::Owned("module"),
                        TokenFlags::None,
                    )
                    .unwrap();
                let import = parser
                    .ast
                    .new_import_declaration(
                        NodeFlags::None,
                        range,
                        Kind::JSImportDeclaration,
                        None,
                        module,
                        None,
                        None,
                    )
                    .unwrap();
                parser.state.reparse_list.extend([alias, plain, import]);
                node
            };
            let (old_nodes, new_nodes) = if indexed {
                (
                    old.parse_list_index(context, &mut old_element),
                    new.parse_list_index(context, &mut new_element),
                )
            } else {
                let old_list = old.parse_list(context, |p| old_element(p, 0));
                let new_list = new.parse_list(context, |p| new_element(p, 0));
                let list = new.ast.list(new_list).unwrap();
                assert_eq!(list.range(), old_list.loc());
                (old_list.nodes().to_vec(), list.nodes().collect())
            };
            assert_eq!(new_indices, old_indices);
            let propagated = context == ParsingContext::TypeMembers;
            assert_eq!(
                new_indices,
                if indexed { vec![0, if propagated { 2 } else { 4 }] } else { vec![0, 0] }
            );
            let actual: Vec<_> =
                new_nodes.iter().map(|key| new.ast.node(*key).unwrap().kind()).collect();
            let expected = if propagated {
                vec![Kind::FalseKeyword, Kind::Identifier, Kind::FalseKeyword, Kind::Identifier]
            } else {
                vec![
                    Kind::JSTypeAliasDeclaration,
                    Kind::FalseKeyword,
                    Kind::JSImportDeclaration,
                    Kind::Identifier,
                    Kind::JSTypeAliasDeclaration,
                    Kind::FalseKeyword,
                    Kind::JSImportDeclaration,
                    Kind::Identifier,
                ]
            };
            assert_eq!(actual, expected);
            assert_eq!(actual, old_nodes.iter().map(|node| node.kind()).collect::<Vec<_>>());
            let deferred: Vec<_> = new
                .state
                .reparse_list
                .iter()
                .map(|key| new.ast.node(*key).unwrap().kind())
                .collect();
            assert_eq!(
                deferred,
                old.reparse_list.iter().map(|node| node.kind()).collect::<Vec<_>>()
            );
            assert_eq!(
                deferred,
                if propagated {
                    vec![
                        Kind::TrueKeyword,
                        Kind::JSTypeAliasDeclaration,
                        Kind::JSImportDeclaration,
                        Kind::JSTypeAliasDeclaration,
                        Kind::JSImportDeclaration,
                    ]
                } else {
                    vec![Kind::TrueKeyword]
                }
            );
            assert_eq!(new.node_stack, [new_prefix]);
            assert_eq!(old.node_stack, [old_prefix]);
            assert_eq!(new.state.parsing_contexts, 0);
            assert_eq!(new.state.parsing_contexts, old.parsing_contexts);
            assert_eq!(
                new.state.token,
                if context == ParsingContext::SourceElements {
                    Kind::EndOfFile
                } else {
                    Kind::CloseBraceToken
                }
            );
            assert_eq!(new.state.token, old.token);
            assert_eq!(new.ast.node_count(), old.factory.node_count());
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
        }
    }
}

#[test]
fn remaining_token_recognition_preserves_advancement_diagnostics_and_context() {
    use tsrs_parser::recognition::RecognitionParser;
    for source in [
        "",
        "name",
        "#name",
        "}",
        ")",
        "]",
        ">",
        ">=",
        "=>",
        ":",
        ";",
        "...",
        "?.name",
        "?.[name]",
        "?.\"text\"",
        "yield name",
        "yield\nname",
        "yield(name)",
        "await name",
        "await\nname",
        "await(1)",
        "const enum",
        "const value",
        "export default abstract class",
        "export default async function",
        "export type { name }",
        "static name",
        "async\nfunction",
        "get name",
        "get\nname",
        "public name",
        "readonly ?",
        "default name",
        "[name: string]",
        "[name?: string]",
        "[...name]",
        "[name, other]",
        "...name?:",
        "name?:",
        "name:",
        "in keyof T",
        "+readonly [name in T]",
        "name in T",
        "from 'module'",
        "defer from 'module'",
        "source = value",
        "'cooked\\n'",
        "'unterminated",
        r"\u0061wait name",
        "/* comment\n */ name",
        "return",
        "function",
        "class",
    ] {
        for flags in [
            NodeFlags::None,
            NodeFlags::YieldContext,
            NodeFlags::AwaitContext,
            NodeFlags::DisallowInContext,
        ] {
            for variant in [false, true] {
                let source: Arc<str> = Arc::from(source);
                macro_rules! compare {
                    ($method:ident $(, $argument:expr)*) => {{
                        let mut old = legacy(&source, ScriptKind::TS);
                        let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
                        old.context_flags = flags;
                        new.state.context_flags = flags;
                        old.statement_has_await_identifier = variant;
                        new.state.statement_has_await_identifier = variant;
                        assert_eq!(RecognitionParser::$method(&mut new $(, $argument)*), RecognitionParser::$method(&mut old $(, $argument)*), "{source:?}, {flags:?}, {}", stringify!($method));
                        assert_eq!(new.state.token, old.token, "{}", stringify!($method));
                        assert_eq!(new.state.scanner.token_range(), old.scanner.token_range());
                        assert_eq!(new.state.scanner.token_flags(), old.scanner.token_flags());
                        assert_eq!(new.state.context_flags, old.context_flags);
                        assert_eq!(new.state.statement_has_await_identifier, old.statement_has_await_identifier);
                        assert_eq!(new.state.has_parse_error, old.has_parse_error);
                        assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
                        assert_eq!(new.state.identifier_count, old.identifier_count);
                        assert_eq!(new.ast.node_count(), old.factory.node_count());
                    }};
                }
                compare!(is_let_declaration);
                compare!(next_token_is_binding_identifier_or_start_of_destructuring);
                compare!(next_is_identifier_and_close_paren);
                compare!(next_token_is_identifier);
                compare!(is_implements_clause);
                compare!(next_token_is_open_paren);
                compare!(next_is_not_dot);
                compare!(next_token_is_from_keyword_or_equals_token);
                compare!(should_parse_import_phase_modifier);
                compare!(token_after_import_definitely_produces_import_declaration);
                compare!(token_after_imported_identifier_allows_import_equals_declaration);
                compare!(can_parse_module_export_name);
                compare!(next_is_start_of_type);
                compare!(next_is_start_of_type_of_import_type);
                compare!(next_is_start_of_mapped_type);
                compare!(next_token_is_open_paren_or_less_than);
                compare!(is_parameter_name_start);
                compare!(
                    should_parse_return_type,
                    if variant { Kind::EqualsGreaterThanToken } else { Kind::ColonToken },
                    variant
                );
                compare!(is_index_signature);
                compare!(next_is_unambiguously_index_signature);
                compare!(scan_start_of_named_tuple_element);
                compare!(next_token_is_colon_or_question_colon);
                compare!(next_token_is_new_keyword);
                compare!(next_token_can_follow_modifier);
                compare!(next_token_can_follow_default_keyword);
                compare!(next_token_is_identifier_or_keyword_or_greater_than);
                compare!(next_token_is_identifier_or_keyword_or_literal_on_same_line);
                compare!(next_token_is_class_keyword_on_same_line);
                compare!(next_token_is_function_keyword_on_same_line);
                compare!(next_token_can_follow_export_modifier);
                compare!(can_follow_export_modifier);
                compare!(can_follow_modifier);
                compare!(can_follow_get_or_set_keyword);
                compare!(next_token_is_on_same_line_and_can_follow_modifier);
                compare!(next_token_is_open_brace);
                compare!(is_yield_expression);
                compare!(is_start_of_expression_statement);
                compare!(is_await_expression);
                compare!(next_token_is_dot);
                compare!(is_template_start_of_tagged_template);
                compare!(can_follow_type_arguments_in_expression);
                compare!(is_start_of_member_expression_rest);
                compare!(is_start_of_optional_property_or_element_access_chain);
                compare!(next_token_is_identifier_or_keyword_or_open_bracket_or_template);
                compare!(try_parse_semicolon);
                compare!(parse_semicolon);
                compare!(next_token_is_binding_identifier_or_start_of_destructuring_on_same_line_disallow_of);
                compare!(in_yield_context);
                compare!(in_disallow_conditional_types_context);
                compare!(in_decorator_context);
                compare!(in_await_context);
            }
        }
    }
}

#[test]
fn async_arrow_recognition_restores_scanner_errors_and_allocates_no_nodes() {
    use tsrs_parser::recognition::RecognitionParser;

    for (source, flags, expected) in [
        ("async value => value", NodeFlags::None, true),
        ("async of => of", NodeFlags::None, true),
        ("async await => await", NodeFlags::None, true),
        ("async await => await", NodeFlags::AwaitContext, false),
        ("async yield => yield", NodeFlags::YieldContext, false),
        ("async\nvalue => value", NodeFlags::None, false),
        ("async value\n=> value", NodeFlags::None, false),
        ("async => value", NodeFlags::None, false),
        ("async (value) => value", NodeFlags::None, false),
        (r"\u0061sync value => value", NodeFlags::None, true),
        (r"async value \u002b", NodeFlags::None, false),
        ("value => value", NodeFlags::None, false),
    ] {
        let source: Arc<str> = Arc::from(source);
        let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
        parser.state.context_flags = flags;
        let token = parser.token();
        let range = parser.state.scanner.token_range();
        let token_flags = parser.state.scanner.token_flags();
        let errors = owned_errors(&parser.state.diagnostics);
        for _ in 0..3 {
            assert_eq!(
                parser.look_ahead(|parser| {
                    parser.next_is_un_parenthesized_async_arrow_function()
                }),
                expected,
                "{source:?}, {flags:?}"
            );
            assert_eq!(parser.token(), token);
            assert_eq!(parser.state.scanner.token_range(), range);
            assert_eq!(parser.state.scanner.token_flags(), token_flags);
            assert_eq!(parser.state.context_flags, flags);
            assert_eq!(owned_errors(&parser.state.diagnostics), errors);
            assert_eq!(parser.ast.node_count(), 0);
        }
        let (builder, _) = parser.into_parts();
        assert_eq!(builder.finish().memory().buffers().live_bytes, 0);
    }
}

#[test]
fn owned_expression_lookahead_retains_cooked_text_after_source_disposal() {
    use tsrs_parser::recognition::RecognitionParser;

    let source: Arc<str> = Arc::from("yield 'cooked\\n'; await name");
    let weak = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    assert!(parser.is_yield_expression());
    assert_eq!(parser.token(), Kind::YieldKeyword);
    let yield_name = parser.parse_identifier();
    let literal = parser.parse_literal_expression();
    assert!(parser.parse_semicolon());
    assert!(parser.is_await_expression());
    assert_eq!(parser.token(), Kind::AwaitKeyword);
    let await_name = parser.parse_identifier();
    let name = parser.parse_identifier();
    assert!(parser.parse_semicolon());
    assert_eq!(parser.token(), Kind::EndOfFile);
    assert_eq!(parser.identifier_count(), 3);
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    let lease = AstStoreLease::from(builder.finish());
    for (key, expected) in [(yield_name, "yield"), (await_name, "await"), (name, "name")] {
        assert_eq!(lease.node(key).unwrap().as_identifier().unwrap().text(), expected);
    }
    assert_eq!(lease.node(literal).unwrap().as_string_literal().unwrap().text(), "cooked\n");
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn contextual_modifiers_preserve_conditional_commit_and_rollback() {
    use tsrs_parser::context::ContextParser;
    for source in [
        "",
        "name",
        "export default class",
        "export default name",
        "export type { name }",
        "readonly name",
        "readonly\nname",
        "static\nname",
        "const enum",
        "const name",
        "async function",
        "async\nfunction",
        "declare name",
        "abstract class",
        "get name",
        "set\nname",
        "public 'cooked\\n'",
        "protected [name]",
        r"\u0073tatic name",
        "export default 'unterminated",
        "static /* comment\n */ name",
        "default @dec",
    ] {
        for expected in
            [None, Some(Kind::StaticKeyword), Some(Kind::ExportKeyword), Some(Kind::GetKeyword)]
        {
            let source: Arc<str> = Arc::from(source);
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            let result = match expected {
                None => (old.parse_any_contextual_modifier(), new.parse_any_contextual_modifier()),
                Some(kind) => {
                    (old.parse_contextual_modifier(kind), new.parse_contextual_modifier(kind))
                }
            };
            assert_eq!(result.0, result.1, "{source:?}, {expected:?}");
            assert_eq!(new.state.token, old.token);
            assert_eq!(new.state.scanner.token_range(), old.scanner.token_range());
            assert_eq!(new.state.scanner.token_flags(), old.scanner.token_flags());
            assert_eq!(new.state.scanner.token_value().as_ref(), old.scanner.token_value());
            assert_eq!(new.state.context_flags, old.context_flags);
            assert_eq!(new.state.has_parse_error, old.has_parse_error);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            assert_eq!(new.ast.node_count(), old.factory.node_count());
        }
    }
    for (source, accepted, next) in [
        ("export default class", true, Kind::DefaultKeyword),
        ("export default name", false, Kind::ExportKeyword),
        ("readonly value", true, Kind::Identifier),
        ("readonly\nvalue", false, Kind::ReadonlyKeyword),
        ("static\nvalue", true, Kind::Identifier),
        ("const value", false, Kind::ConstKeyword),
        ("const enum", true, Kind::EnumKeyword),
    ] {
        let source: Arc<str> = Arc::from(source);
        let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
        assert_eq!(parser.parse_any_contextual_modifier(), accepted);
        assert_eq!(parser.token(), next);
    }
}

#[test]
fn context_scopes_restore_nested_flags_and_preserve_node_results() {
    use tsrs_parser::context::ContextParser;
    for source in ["await", "yield", "name"] {
        for flag in [NodeFlags::AwaitContext, NodeFlags::YieldContext] {
            let source: Arc<str> = Arc::from(source);
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            let mut old_flags = Vec::new();
            let mut new_flags = Vec::new();
            let old_node = old.do_in_context(flag, true, |parser| {
                old_flags.push(parser.context_flags);
                parser.do_in_context(NodeFlags::DisallowInContext, true, |parser| {
                    old_flags.push(parser.context_flags);
                    parser.set_context_flags(NodeFlags::DecoratorContext, true);
                });
                old_flags.push(parser.context_flags);
                parser.parse_identifier()
            });
            let new_node = new.do_in_context(flag, true, |parser| {
                new_flags.push(parser.state.context_flags);
                parser.do_in_context(NodeFlags::DisallowInContext, true, |parser| {
                    new_flags.push(parser.state.context_flags);
                    parser.set_context_flags(NodeFlags::DecoratorContext, true);
                });
                new_flags.push(parser.state.context_flags);
                parser.parse_identifier()
            });
            assert_eq!(new_flags, old_flags);
            assert_eq!(new_flags, [flag, flag | NodeFlags::DisallowInContext, flag]);
            assert_eq!(owned_shape(&new.ast, new_node, None), legacy_shape(old_node, None));
            assert_eq!(new.state.context_flags, NodeFlags::None);
            assert_eq!(new.state.context_flags, old.context_flags);
            assert_eq!(new.state.has_parse_error, old.has_parse_error);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            assert_eq!(new.state.identifier_count, old.identifier_count);
        }
    }
}

#[test]
fn owned_snapshots_restore_cooked_tokens_without_reusing_abandoned_keys() {
    use tsrs_parser::context::ContextParser;
    let source: Arc<str> = Arc::from("'cooked\\n' await");
    let weak = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let state = parser.mark();
    let abandoned = parser.parse_literal_expression();
    let abandoned_name = parser.parse_identifier();
    assert!(parser.state.statement_has_await_identifier);
    assert!(!parser.parse_expected(Kind::OpenParenToken));
    assert!(parser.state.has_parse_error);
    parser.set_context_flags(NodeFlags::DisallowInContext, true);
    parser.rewind(state);
    assert_eq!(parser.token(), Kind::StringLiteral);
    assert_eq!(parser.state.context_flags, NodeFlags::None);
    assert!(!parser.state.has_parse_error);
    assert!(!parser.state.statement_has_await_identifier);
    assert!(parser.state.diagnostics.is_empty());
    assert_eq!(parser.identifier_count(), 1);
    assert_eq!(parser.ast.node_count(), 2);
    let committed = parser.parse_literal_expression();
    assert_ne!(committed, abandoned);
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    let lease = AstStoreLease::from(builder.finish());
    for key in [abandoned, committed] {
        assert_eq!(lease.node(key).unwrap().as_string_literal().unwrap().text(), "cooked\n");
    }
    assert_eq!(lease.node(abandoned_name).unwrap().as_identifier().unwrap().text(), "await");
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn type_leaves_match_tree_state_and_error_order() {
    for (source, negative) in [
        ("true", false),
        ("false next", false),
        ("null", false),
        ("42", false),
        ("0x1ff", false),
        ("1_000n", false),
        ("'cooked\\n'", false),
        ("`cooked\\n`", false),
        ("'bad\\u{}'", false),
        ("'unterminated", false),
        (" /* before */ - /* after */ 42", true),
        ("-0x1ff", true),
        ("-1_000n", true),
        ("-0", true),
        (r"-1 \u0069f", true),
    ] {
        for context in [NodeFlags::None, NodeFlags::JSDoc, NodeFlags::DisallowInContext] {
            let source: Arc<str> = Arc::from(source);
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            old.context_flags = context;
            new.state.context_flags = context;
            let old_node = old.parse_literal_type_node(negative);
            let new_node = new.parse_literal_type_node(negative);
            assert_eq!(
                owned_shape(&new.ast, new_node, None),
                legacy_shape(old_node, None),
                "{source:?}"
            );
            assert_eq!(new.state.token, old.token);
            assert_eq!(new.state.scanner.token_range(), old.scanner.token_range());
            assert_eq!(new.state.scanner.token_flags(), old.scanner.token_flags());
            assert_eq!(new.state.has_parse_error, old.has_parse_error);
            assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
            assert_eq!(new.ast.node_count(), old.factory.node_count());
            assert_eq!(new.state.identifier_count, old.identifier_count);
            let literal = new.ast.node(new_node).unwrap().as_literal_type_node().unwrap().literal();
            if negative {
                let unary = new.ast.node(literal).unwrap().as_prefix_unary_expression().unwrap();
                assert_eq!(unary.operator(), Kind::MinusToken);
            }
        }
    }
    for source in [
        "void",
        "any",
        "unknown",
        "string",
        "number",
        "bigint",
        "symbol",
        "boolean",
        "undefined",
        "never",
        "object",
        "intrinsic",
        r"\u006eumber",
        "this",
        "*",
    ] {
        let source: Arc<str> = Arc::from(source);
        let mut old = legacy(&source, ScriptKind::TS);
        let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
        let (old_node, new_node) = match old.token {
            Kind::ThisKeyword => (old.parse_this_type_node(), new.parse_this_type_node()),
            Kind::AsteriskToken => (old.parse_jsdoc_all_type(), new.parse_jsdoc_all_type()),
            _ => (old.parse_keyword_type_node(), new.parse_keyword_type_node()),
        };
        assert_eq!(
            owned_shape(&new.ast, new_node, None),
            legacy_shape(old_node, None),
            "{source:?}"
        );
        assert_eq!(new.state.token, old.token);
        assert_eq!(new.state.has_parse_error, old.has_parse_error);
        assert_eq!(owned_errors(&new.state.diagnostics), legacy_errors(&old.diagnostics));
        assert_eq!(new.ast.node_count(), old.factory.node_count());
    }
}

#[test]
fn owned_type_leaves_keep_payloads_after_growth_and_source_disposal() {
    let source: Arc<str> = Arc::from("-1_000n 'cooked\\n' number this * ".repeat(512));
    let weak = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let mut nodes = Vec::new();
    while parser.state.token != Kind::EndOfFile {
        nodes.push(parser.parse_literal_type_node(true));
        nodes.push(parser.parse_literal_type_node(false));
        nodes.push(parser.parse_keyword_type_node());
        nodes.push(parser.parse_this_type_node());
        nodes.push(parser.parse_jsdoc_all_type());
    }
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    let lease = AstStoreLease::from(builder.finish());
    for group in nodes.as_chunks::<5>().0 {
        let negative = lease.node(group[0]).unwrap().as_literal_type_node().unwrap().literal();
        let unary = lease.node(negative).unwrap().as_prefix_unary_expression().unwrap();
        assert_eq!(unary.operator(), Kind::MinusToken);
        let operand = lease.node(unary.operand()).unwrap();
        assert_eq!(operand.as_big_int_literal().unwrap().text(), "1000n");
        assert_eq!(operand.parent(), Some(negative));
        let string = lease.node(group[1]).unwrap().as_literal_type_node().unwrap().literal();
        assert_eq!(lease.node(string).unwrap().as_string_literal().unwrap().text(), "cooked\n");
        assert_eq!(lease.node(group[2]).unwrap().kind(), Kind::NumberKeyword);
        assert_eq!(lease.node(group[3]).unwrap().kind(), Kind::ThisType);
        assert_eq!(lease.node(group[4]).unwrap().kind(), Kind::JSDocAllType);
    }
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn speculative_grammar_reclaims_cooked_text_and_restores_parser_state() {
    use tsrs_parser::context::ContextParser;
    let source: Arc<str> = Arc::from("'cooked\\n' await");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    for _ in 0..8 {
        assert!(parser.look_ahead(|parser| {
            let literal = parser.parse_literal_expression();
            assert_eq!(
                parser.ast().node(literal).unwrap().as_string_literal().unwrap().text(),
                "cooked\n"
            );
            parser.parse_identifier();
            assert!(parser.state.statement_has_await_identifier);
            assert!(!parser.parse_expected(Kind::OpenParenToken));
            parser.set_context_flags(NodeFlags::DisallowInContext, true);
            true
        }));
        assert_eq!(parser.token(), Kind::StringLiteral);
        assert_eq!(parser.state.context_flags, NodeFlags::None);
        assert!(!parser.state.statement_has_await_identifier);
        assert!(!parser.state.has_parse_error);
        assert!(parser.diagnostics().is_empty());
        assert_eq!(parser.node_stack, [] as [NodeKey; 0]);
    }
    assert_eq!(parser.identifier_count(), 8);
    let (builder, _) = parser.into_parts();
    assert_eq!(builder.node_count(), 16);
    assert_eq!(builder.finish().memory().buffers().live_bytes, 0);
}

#[test]
fn speculative_type_commit_matches_production_and_survives_source_disposal() {
    let source: Arc<str> = Arc::from("-0x1_0n tail");
    let mut old = legacy(&source, ScriptKind::TS);
    let expected = legacy_shape(old.parse_literal_type_node(true), None);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let node = parser.try_parse_node(|parser| Some(parser.parse_literal_type_node(true))).unwrap();
    assert_eq!(parser.token(), Kind::Identifier);
    assert_eq!(owned_shape(parser.ast(), node, None), expected);
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    assert_eq!(builder.finish().node(node).unwrap().kind(), Kind::LiteralType);
}

#[test]
fn nested_speculative_commit_is_reclaimed_when_the_outer_parse_rejects() {
    let source: Arc<str> = Arc::from("'inner\\n' name");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    assert!(
        parser
            .try_parse_node(|parser| {
                let child = parser
                    .try_parse_node(|parser| Some(parser.parse_literal_type_node(false)))
                    .unwrap();
                assert_eq!(parser.ast().node(child).unwrap().kind(), Kind::LiteralType);
                parser.parse_identifier();
                None
            })
            .is_none()
    );
    assert_eq!(parser.token(), Kind::StringLiteral);
    assert_eq!(parser.identifier_count(), 1);
    assert_eq!(parser.ast.node_count(), 3);
    let node = parser
        .try_parse_node(|parser| {
            assert!(
                parser
                    .try_parse_node(|parser| {
                        parser.parse_literal_expression();
                        None
                    })
                    .is_none()
            );
            assert_eq!(parser.token(), Kind::StringLiteral);
            Some(parser.parse_literal_expression())
        })
        .unwrap();
    assert_eq!(parser.token(), Kind::Identifier);
    let (builder, _) = parser.into_parts();
    let store = builder.finish();
    assert_eq!(store.node(node).unwrap().as_string_literal().unwrap().text(), "inner\n");
    assert_eq!(store.node_count(), 5);
}

// Recursive array construction is a list fixture; lexical/name/list grammar is
// shared with production. This does not substitute for porting expression grammar.
fn scoped_list_element<'brand>(
    parser: &mut OwnedParserScope<'_, '_, 'brand, '_>,
) -> ScopedNode<'brand> {
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    if parser.token() == Kind::OpenBracketToken {
        let pos = parser.state.scanner.token_full_start();
        let list = parser
            .parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                scoped_list_element,
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            )
            .unwrap();
        let node = parser.new_array_literal_expression(list);
        LexicalParser::finish_node(parser, node, pos)
    } else if tsrs_ast::is_literal_kind(parser.token()) {
        parser.parse_literal_expression()
    } else {
        parser.parse_entity_name(true, false, None)
    }
}

#[test]
fn speculative_recursive_lists_keep_the_outer_stack_and_reclaim_rejected_nodes() {
    let source: Arc<str> = Arc::from("seed [a, [c, 'cooked\\n'],] tail");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let seed = parser.parse_identifier();
    parser.node_stack.push(seed);
    assert!(parser.look_ahead(|parser| {
        let array = scoped_list_element(parser);
        assert_eq!(parser.ast().node(array).unwrap().kind(), Kind::ArrayLiteralExpression);
        assert_eq!(parser.node_stack, [] as [ScopedNode<'_>; 0]);
        assert_eq!(parser.state.parsing_contexts, 0);
        true
    }));
    assert_eq!(parser.node_stack, [seed]);
    assert_eq!(parser.token(), Kind::OpenBracketToken);
    let committed = parser.try_parse_node(|parser| Some(scoped_list_element(parser))).unwrap();
    assert_eq!(parser.node_stack, [seed]);
    assert_eq!(parser.token(), Kind::Identifier);
    let mut old = legacy(&source, ScriptKind::TS);
    old.parse_identifier();
    let expected = legacy_shape(legacy_list_element(&mut old), None);
    assert_eq!(owned_shape(parser.ast(), committed, None), expected);
    let (builder, _) = parser.into_parts();
    let store = builder.finish();
    assert_eq!(store.node(seed).unwrap().as_identifier().unwrap().text(), "seed");
}

#[test]
fn speculative_panics_restore_scanner_errors_contexts_and_outer_stack() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use tsrs_parser::lists::ListParser;
    use tsrs_parser::parser_1::ParsingContext;
    let source: Arc<str> = Arc::from("seed [a, b]");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let seed = parser.parse_identifier();
    parser.node_stack.push(seed);
    let result = catch_unwind(AssertUnwindSafe(|| {
        parser.look_ahead(|parser| {
            parser.parse_bracketed_list(
                ParsingContext::ArrayLiteralMembers,
                |parser| -> Option<ScopedNode<'_>> {
                    parser.parse_identifier();
                    parser.parse_expected(Kind::OpenBraceToken);
                    panic!("stop in recursive grammar");
                },
                Kind::OpenBracketToken,
                Kind::CloseBracketToken,
            );
            true
        })
    }));
    assert!(result.is_err());
    assert_eq!(parser.token(), Kind::OpenBracketToken);
    assert!(parser.diagnostics().is_empty());
    assert_eq!(parser.state.parsing_contexts, 0);
    assert!(!parser.state.has_parse_error);
    assert_eq!(parser.node_stack, [seed]);
    assert_eq!(parser.ast().node(seed).unwrap().as_identifier().unwrap().text(), "seed");
}

#[test]
fn speculative_reparse_escapes_pin_ancestors_and_keep_outward_order() {
    use tsrs_parser::lists::ListParser;
    let source: Arc<str> = Arc::from("outer inner tail");
    let mut parser = OwnedParser::new(&source, ScriptKind::JS).unwrap();
    assert!(parser.look_ahead(|parser| {
        let outer = parser.parse_identifier();
        tsrs_parser::lists::ListParser::reparse_list_mut(parser).push(outer);
        assert!(parser.look_ahead(|parser| {
            let inner = parser.parse_identifier();
            tsrs_parser::lists::ListParser::reparse_list_mut(parser).push(inner);
            true
        }));
        let names: Vec<_> = parser.reparse_list_mut().clone();
        assert_eq!(names.len(), 2);
        for (node, expected) in names.into_iter().zip(["outer", "inner"]) {
            assert_eq!(parser.ast().node(node).unwrap().as_identifier().unwrap().text(), expected);
        }
        true
    }));
    assert_eq!(parser.token(), Kind::Identifier);
    assert_eq!(parser.state.reparse_list.len(), 2);
    let names = parser.state.reparse_list.clone();
    let (builder, _) = parser.into_parts();
    drop(source);
    let store = builder.finish();
    for (node, expected) in names.into_iter().zip(["outer", "inner"]) {
        assert_eq!(store.node(node).unwrap().as_identifier().unwrap().text(), expected);
    }
}

fn assert_template_state(old: &tsrs_parser::Parser, new: &OwnedParser<'_>, source: &str) {
    assert_eq!(owned_errors(new.diagnostics()), legacy_errors(&old.diagnostics), "{source:?}");
    assert_eq!(new.token(), old.token, "{source:?}");
    assert_eq!(new.state.scanner.token_range(), old.scanner.token_range(), "{source:?}");
    assert_eq!(new.state.scanner.token_flags(), old.scanner.token_flags(), "{source:?}");
    if new.token() != Kind::EndOfFile {
        assert_eq!(
            new.state.scanner.token_value_ref(),
            old.scanner.token_value_ref(),
            "{source:?}"
        );
    }
    assert_eq!(new.state.has_parse_error, old.has_parse_error, "{source:?}");
    assert_eq!(new.ast.node_count(), old.factory.node_count(), "{source:?}");
    assert_eq!(new.ast.text_count(), old.factory.text_count(), "{source:?}");
}

#[test]
fn template_heads_preserve_raw_text_cooked_text_and_tagged_error_policy() {
    for source in [
        "`plain${value}tail` next",
        " /* 🦀 */ `中${value}tail` next",
        "`line\r\nnext${value}tail` next",
        "`line\rnext${value}tail` next",
        "`line\u{2028}next${value}tail` next",
        r"`escaped\n\u{1F600}\${raw}${value}tail` next",
        r"`\uD800${value}tail` next",
        r"`\xZ${value}tail` next",
        r"`\u{110000}${value}tail` next",
        r"`\01${value}tail` next",
        "`${",
    ] {
        for tagged in [false, true] {
            let source: Arc<str> = source.into();
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            assert_eq!(new.token(), Kind::TemplateHead);
            let old_root = old.parse_template_head(tagged);
            let new_root = new.parse_template_head(tagged);
            assert_eq!(
                owned_shape(new.ast(), new_root, None),
                legacy_shape(old_root, None),
                "{source:?}"
            );
            assert_template_state(&old, &new, &source);
        }
    }
}

#[test]
fn template_span_literals_preserve_rescanning_recovery_and_next_tokens() {
    for source in [
        "}plain` next",
        " /* 🦀 */ }中${value}tail` next",
        "}line\r\nnext` next",
        r"}escaped\n\u{1F600}\${raw}${value}tail` next",
        r"}\uD800` next",
        r"}\xZ${value}tail` next",
        r"}\u{110000}` next",
        r"}\01` next",
        "}unterminated",
        "}unterminated\\",
        "}",
        "",
        "missing next",
        "] next",
    ] {
        for tagged in [false, true] {
            let source: Arc<str> = source.into();
            let mut old = legacy(&source, ScriptKind::TS);
            let mut new = OwnedParser::new(&source, ScriptKind::TS).unwrap();
            let old_root = old.parse_literal_of_template_span(tagged);
            let new_root = new.parse_literal_of_template_span(tagged);
            assert_eq!(
                owned_shape(new.ast(), new_root, None),
                legacy_shape(old_root, None),
                "{source:?}"
            );
            assert_template_state(&old, &new, &source);
            if !source.trim_start().starts_with('}') && !source.starts_with(" /*") {
                let old_root = old.parse_literal_of_template_span(tagged);
                let new_root = new.parse_literal_of_template_span(tagged);
                assert_eq!(owned_shape(new.ast(), new_root, None), legacy_shape(old_root, None));
                assert_template_state(&old, &new, &source);
            }
        }
    }
}

#[test]
fn owned_template_tokens_keep_both_text_forms_after_growth_and_source_disposal() {
    let source: Arc<str> = "`中\\n${value}middle\\u0061${value}tail\\u{1F600}` ".repeat(128).into();
    let weak = Arc::downgrade(&source);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let mut groups = Vec::new();
    while parser.token() != Kind::EndOfFile {
        let head = parser.parse_template_head(false);
        parser.parse_identifier();
        let middle = parser.parse_literal_of_template_span(false);
        parser.parse_identifier();
        let tail = parser.parse_literal_of_template_span(false);
        groups.push([head, middle, tail]);
    }
    let (builder, errors) = parser.into_parts();
    assert!(errors.is_empty());
    drop(source);
    let store = builder.finish();
    // Only three cooked strings and their eight-byte descriptors enter the text
    // buffer. Raw text and identifier names use the retained source ranges.
    assert_eq!(store.memory().text.live_bytes, 128 * (4 + 7 + 8 + 3 * 8));
    let lease = AstStoreLease::from(store);
    for [head, middle, tail] in groups {
        let head = lease.node(head).unwrap().as_template_head().unwrap();
        assert_eq!((head.text(), head.raw_text()), ("中\n", "中\\n"));
        let middle = lease.node(middle).unwrap().as_template_middle().unwrap();
        assert_eq!((middle.text(), middle.raw_text()), ("middlea", "middle\\u0061"));
        let tail = lease.node(tail).unwrap().as_template_tail().unwrap();
        assert_eq!((tail.text(), tail.raw_text()), ("tail😀", "tail\\u{1F600}"));
    }
    assert!(weak.upgrade().is_some());
    drop(lease);
    assert!(weak.upgrade().is_none());
}

#[test]
fn speculative_templates_restore_scanner_errors_and_reclaim_text_after_nested_acceptance() {
    let source: Arc<str> = Arc::from(r"`head\xZ${value}tail\n");
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    for _ in 0..4 {
        assert!(parser.look_ahead(|parser| {
            let head = parser.parse_template_head(false);
            assert!(
                parser
                    .ast()
                    .node(head)
                    .unwrap()
                    .as_template_head()
                    .unwrap()
                    .template_flags()
                    .intersects(TokenFlags::ContainsInvalidEscape)
            );
            assert!(!parser.diagnostics().is_empty());
            parser.parse_identifier();
            let tail = parser
                .try_parse_node(|parser| Some(parser.parse_literal_of_template_span(false)))
                .unwrap();
            let tail = parser.ast().node(tail).unwrap().as_template_tail().unwrap();
            assert_eq!((tail.text(), tail.raw_text()), ("tail\n", r"tail\n"));
            assert_eq!(parser.token(), Kind::EndOfFile);
            true
        }));
        assert_eq!(parser.token(), Kind::TemplateHead);
        assert!(parser.diagnostics().is_empty());
        assert_eq!(parser.state.scanner.token_value_ref(), r"head\xZ");
        assert!(!parser.state.has_parse_error);
    }
    let (builder, _) = parser.into_parts();
    assert_eq!(builder.finish().memory().buffers().live_bytes, 0);
    let mut parser = OwnedParser::new(&source, ScriptKind::TS).unwrap();
    let root = parser.try_parse_node(|parser| Some(parser.parse_template_head(true))).unwrap();
    assert!(parser.diagnostics().is_empty());
    let (builder, _) = parser.into_parts();
    drop(source);
    let store = builder.finish();
    assert_eq!(store.node(root).unwrap().as_template_head().unwrap().raw_text(), r"head\xZ");
}
