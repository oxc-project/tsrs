//! Port of tsgolint's `internal/rules/no_meaningless_void_operator/no_meaningless_void_operator.go`.

use tsrs_ast::{self as ast, Kind, Node};
use tsrs_checker::TypeFlags;
use tsrs_core::{P, json::Value};

use crate::protocol::{Diagnostic, Fix, Range, RuleMessage, Suggestion};
use crate::rule::{Listener, Rule, RuleContext, trim_node_range};
use crate::utils::{
    is_start_of_expression_statement_needing_parentheses, is_thenable_type, union_type_parts,
};

pub struct NoMeaninglessVoidOperator {
    check_never: bool,
}

impl NoMeaninglessVoidOperator {
    pub fn new(options: &Value) -> Result<Self, String> {
        let object = match options {
            Value::Null => None,
            Value::Object(object) => Some(object),
            _ => return Err("no-meaningless-void-operator: options must be an object".into()),
        };
        let check_never = match object.and_then(|object| object.get("checkNever")) {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err("no-meaningless-void-operator: checkNever must be a boolean".into()),
        };
        Ok(Self { check_never })
    }
}

fn remove_void_keyword(context: &RuleContext<'_>, node: P<Node>, argument: P<Node>) -> Vec<Fix> {
    let argument_pos = argument.pos().expect_source("parsed lint node");
    let mut keyword = trim_node_range(context.source_file, node);
    keyword.end = argument_pos;
    let statement = node.parent().expect("void expressions have parents");
    if statement.kind() != Kind::ExpressionStatement {
        return vec![Fix { range: keyword, text: String::new() }];
    }

    let mut scanner = tsrs_scanner::get_scanner_for_source_file(context.source_file, argument_pos);
    let mut first_token = scanner.token();
    if first_token == Kind::AsyncKeyword
        && scanner.scan() == Kind::FunctionKeyword
        && !scanner.has_preceding_line_break()
    {
        first_token = Kind::FunctionKeyword;
    }
    let wrap = ast::is_string_literal(argument)
        || first_token == Kind::AtToken
        || first_token == Kind::LetKeyword && scanner.scan() == Kind::OpenBracketToken
        || is_start_of_expression_statement_needing_parentheses(
            context.source_file,
            node,
            first_token,
        );
    if wrap {
        first_token = Kind::OpenParenToken;
    }

    let prefix = if matches!(
        first_token,
        Kind::OpenParenToken
            | Kind::OpenBracketToken
            | Kind::PlusToken
            | Kind::MinusToken
            | Kind::SlashToken
            | Kind::SlashEqualsToken
            | Kind::LessThanToken
            | Kind::NoSubstitutionTemplateLiteral
            | Kind::TemplateHead
    ) && previous_statement_lacks_semicolon(context, statement)
    {
        ";"
    } else {
        ""
    };
    if wrap {
        let end = node.end().expect_source("parsed lint node");
        vec![
            Fix { range: keyword, text: format!("{prefix}(") },
            Fix { range: Range { pos: end, end }, text: ")".into() },
        ]
    } else {
        vec![Fix { range: keyword, text: prefix.into() }]
    }
}

fn previous_statement_lacks_semicolon(context: &RuleContext<'_>, statement: P<Node>) -> bool {
    let Some(parent) = statement.parent().filter(|parent| parent.can_have_statements()) else {
        return false;
    };
    let statements = parent.statements();
    let pos = statement.pos().expect_source("parsed statement");
    let index = statements
        .partition_point(|statement| statement.pos().expect_source("parsed statement") < pos);
    index > 0 && {
        let end = statements[index - 1].end().expect_source("parsed statement") as usize;
        !context.source_file.text().as_bytes()[..end].ends_with(b";")
    }
}

impl Rule for NoMeaninglessVoidOperator {
    fn listens_to(&self, listener: Listener) -> bool {
        listener == Listener::Enter(Kind::VoidExpression)
    }

    fn visit(&mut self, _: Listener, node: P<Node>, context: &mut RuleContext<'_>) {
        let argument = node.as_void_expression().expression();
        let inner = ast::skip_outer_expressions(
            argument,
            ast::OEK::Parentheses | ast::OEK::Assertions | ast::OEK::Comma,
        );
        if ast::is_assignment_expression(inner, false)
            || ast::is_numeric_literal(inner) && inner.as_numeric_literal().text() == "0"
        {
            return;
        }
        let t = context.checker.get_type_at_location(argument);

        if !ast::is_call_expression(inner) || ast::is_import_call(inner) {
            if is_thenable_type(context.checker, argument, t) {
                return;
            }
            let mut parent = node.parent().expect("void expressions have parents");
            while parent.kind() == Kind::ParenthesizedExpression {
                parent = parent.parent().expect("parenthesized expressions have parents");
            }
            let fixes = if parent.kind() == Kind::ExpressionStatement && context.options.fix {
                remove_void_keyword(context, node, argument)
            } else {
                Vec::new()
            };
            context.report_diagnostic(Diagnostic {
                range: Some(trim_node_range(context.source_file, node)),
                message: RuleMessage {
                    id: "meaninglessVoidOnNonCall".into(),
                    description:
                        "void operator is useless here; it should only discard a call's return value"
                            .into(),
                    ..Default::default()
                },
                fixes,
                ..Default::default()
            });
            return;
        }

        let parts = union_type_parts(&t);
        let always_void_like = parts.iter().all(|t| t.flags().intersects(TypeFlags::VoidLike));
        let always_void_like_or_never =
            parts.iter().all(|t| t.flags().intersects(TypeFlags::VoidLike | TypeFlags::Never));
        if !(always_void_like || self.check_never && always_void_like_or_never) {
            return;
        }
        let text = context.checker.type_to_string_exported(t);
        let message = RuleMessage {
            id: "meaninglessVoidOperator".into(),
            description: format!(
                "void operator shouldn't be used on {text}; it should convey that a return value is being ignored"
            ),
            ..Default::default()
        };
        let fixes = if always_void_like && context.options.fix {
            remove_void_keyword(context, node, argument)
        } else {
            Vec::new()
        };
        let suggestions = if !always_void_like && context.options.fix_suggestions {
            vec![Suggestion {
                message: RuleMessage {
                    id: "removeVoid".into(),
                    description: "Remove 'void'".into(),
                    ..Default::default()
                },
                fixes: remove_void_keyword(context, node, argument),
            }]
        } else {
            Vec::new()
        };
        context.report_diagnostic(Diagnostic {
            range: Some(trim_node_range(context.source_file, node)),
            message,
            fixes,
            suggestions,
            ..Default::default()
        });
    }
}
