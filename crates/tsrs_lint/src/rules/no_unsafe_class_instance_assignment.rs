//! Port of tsgolint's
//! `internal/rules/no_unsafe_class_instance_assignment/no_unsafe_class_instance_assignment.go`.

use rustc_hash::FxHashSet;
use tsrs_ast::{self as ast, Kind, Node};
use tsrs_checker::{Checker, ContextFlags, ObjectFlags, Type, TypeFlags};
use tsrs_core::P;

use crate::protocol::RuleMessage;
use crate::rule::{Listener, Rule, RuleContext};
use crate::utils::{self, intersection_type_parts};

fn build_non_instance_message(type_name: &str) -> RuleMessage {
    RuleMessage {
        id: "nonInstance".into(),
        description: format!(
            "This value is used where an instance of class `{type_name}` is expected, but was not created by its constructor."
        ),
        ..Default::default()
    }
}

fn build_object_literal_message(type_name: &str) -> RuleMessage {
    RuleMessage {
        id: "objectLiteral".into(),
        description: format!(
            "This object literal is used where an instance of class `{type_name}` is expected."
        ),
        ..Default::default()
    }
}

fn is_type_reference(t: P<Type>) -> bool {
    t.flags().intersects(TypeFlags::Object) && t.object_flags().intersects(ObjectFlags::Reference)
}

fn get_class_target(t: P<Type>) -> Option<P<Type>> {
    if !t.flags().intersects(TypeFlags::Object) {
        return None;
    }
    let target = if is_type_reference(t) { t.target().unwrap() } else { t };
    target.object_flags().intersects(ObjectFlags::Class).then_some(target)
}

fn get_nominal_class(checker: &mut Checker, t: P<Type>) -> Option<P<Type>> {
    let mut found = None;
    utils::matches_type_or_base_type(checker, t, &mut |_, candidate| {
        if get_class_target(candidate).is_some() {
            found = Some(candidate);
        }
        found.is_some()
    });
    found
}

fn resolve_constraint(checker: &mut Checker, t: P<Type>) -> Option<P<Type>> {
    if t.flags().intersects(TypeFlags::Instantiable) {
        checker.get_base_constraint_of_type(t)
    } else {
        Some(t)
    }
}

fn contains_class(checker: &mut Checker, t: P<Type>, seen: &mut FxHashSet<P<Type>>) -> bool {
    if !seen.insert(t) {
        return false;
    }
    let Some(resolved) = resolve_constraint(checker, t) else {
        return false;
    };
    if resolved.flags().intersects(TypeFlags::UnionOrIntersection) {
        return resolved.types().iter().any(|&part| contains_class(checker, part, seen));
    }
    if get_nominal_class(checker, resolved).is_some() {
        return true;
    }
    if is_type_reference(resolved)
        && checker
            .get_type_arguments(resolved)
            .iter()
            .any(|&type_argument| contains_class(checker, type_argument, seen))
    {
        return true;
    }
    checker
        .get_index_infos_of_type(resolved)
        .iter()
        .any(|index_info| contains_class(checker, index_info.value_type(), seen))
}

fn has_class(checker: &mut Checker, t: P<Type>) -> bool {
    contains_class(checker, t, &mut FxHashSet::default())
}

fn is_instance_of(checker: &mut Checker, source: P<Type>, class_type: P<Type>) -> bool {
    let class_target = get_class_target(class_type);
    intersection_type_parts(&source).iter().any(|&part| {
        part.flags().intersects(TypeFlags::Object)
            && part.object_flags().intersects(ObjectFlags::Mapped)
            || utils::matches_type_or_base_type(checker, part, &mut |_, t| {
                get_class_target(t) == class_target
            })
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ClassInstanceComparison {
    source: P<Type>,
    target: P<Type>,
    shallow: bool,
}

fn find_violation(
    checker: &mut Checker,
    source: P<Type>,
    target: P<Type>,
    shallow: bool,
) -> Option<P<Type>> {
    find_violation_worker(checker, source, target, shallow, &mut FxHashSet::default())
}

fn find_violation_worker(
    checker: &mut Checker,
    source: P<Type>,
    target: P<Type>,
    shallow: bool,
    active: &mut FxHashSet<ClassInstanceComparison>,
) -> Option<P<Type>> {
    let resolved_source = resolve_constraint(checker, source);
    let resolved_target = resolve_constraint(checker, target);
    let (Some(resolved_source), Some(resolved_target)) = (resolved_source, resolved_target) else {
        return None;
    };
    if resolved_source == resolved_target
        || resolved_source
            .flags()
            .intersects(TypeFlags::Any | TypeFlags::Unknown | TypeFlags::Never)
        || !has_class(checker, resolved_target)
    {
        return None;
    }

    let comparison =
        ClassInstanceComparison { source: resolved_source, target: resolved_target, shallow };
    if !active.insert(comparison) {
        return None;
    }
    let violation = find_active_violation(checker, comparison, active);
    active.remove(&comparison);
    violation
}

fn find_active_violation(
    checker: &mut Checker,
    comparison: ClassInstanceComparison,
    active: &mut FxHashSet<ClassInstanceComparison>,
) -> Option<P<Type>> {
    let ClassInstanceComparison { source: resolved_source, target: resolved_target, shallow } =
        comparison;

    if resolved_source.is_union() {
        for &part in resolved_source.types() {
            if !checker.is_type_assignable_to(part, resolved_target) {
                continue;
            }
            if let Some(violation) =
                find_violation_worker(checker, part, resolved_target, shallow, active)
            {
                return Some(violation);
            }
        }
        return None;
    }

    let never = checker.get_never_type();
    if !(resolved_source.flags().intersects(TypeFlags::Object)
        || resolved_source.is_intersection()
            && !checker.is_type_assignable_to(resolved_source, never))
    {
        return None;
    }

    if resolved_target.is_union() {
        let candidates: Vec<_> = resolved_target
            .types()
            .iter()
            .copied()
            .filter(|&part| checker.is_type_assignable_to(resolved_source, part))
            .collect();
        if candidates.is_empty()
            || candidates.iter().any(|&candidate| !has_class(checker, candidate))
        {
            return None;
        }
        let mut first_violation = None;
        for candidate in candidates {
            let violation =
                find_violation_worker(checker, resolved_source, candidate, shallow, active)?;
            first_violation.get_or_insert(violation);
        }
        return first_violation;
    }

    if resolved_target.is_intersection() {
        return resolved_target.types().iter().find_map(|&part| {
            find_violation_worker(checker, resolved_source, part, shallow, active)
        });
    }

    if let Some(nominal_class) = get_nominal_class(checker, resolved_target)
        && !is_instance_of(checker, resolved_source, nominal_class)
    {
        return Some(nominal_class);
    }

    if shallow {
        return None;
    }

    if is_type_reference(resolved_source)
        && is_type_reference(resolved_target)
        && resolved_source.target() == resolved_target.target()
    {
        let source_arguments = checker.get_type_arguments(resolved_source);
        let target_arguments = checker.get_type_arguments(resolved_target);
        return source_arguments.iter().zip(target_arguments).find_map(
            |(&source_argument, &target_argument)| {
                find_violation_worker(checker, source_argument, target_argument, false, active)
            },
        );
    }

    for key_type in [checker.get_number_type(), checker.get_string_type()] {
        let source_index_info = checker.get_index_info_of_type_exported(resolved_source, key_type);
        let target_index_info = checker.get_index_info_of_type_exported(resolved_target, key_type);
        let (Some(source_index_info), Some(target_index_info)) =
            (source_index_info, target_index_info)
        else {
            continue;
        };
        if let Some(violation) = find_violation_worker(
            checker,
            source_index_info.value_type(),
            target_index_info.value_type(),
            false,
            active,
        ) {
            return Some(violation);
        }
    }
    None
}

fn is_skipped_site(node: P<Node>) -> bool {
    match node.kind() {
        Kind::ArrowFunction | Kind::ConditionalExpression | Kind::FunctionExpression => true,
        Kind::BinaryExpression => matches!(
            node.as_binary_expression().operator_token.kind(),
            Kind::AmpersandAmpersandToken
                | Kind::BarBarToken
                | Kind::QuestionQuestionToken
                | Kind::CommaToken
        ),
        _ => false,
    }
}

fn check_site(node: Option<P<Node>>, context: &mut RuleContext<'_>) {
    let Some(node) = node else {
        return;
    };
    let node = ast::skip_parentheses(node);
    if is_skipped_site(node) {
        return;
    }
    let Some(contextual_type) =
        context.checker.get_contextual_type_exported(node, ContextFlags::None)
    else {
        return;
    };
    let is_literal =
        ast::is_object_literal_expression(node) || ast::is_array_literal_expression(node);
    let source = context.checker.get_type_at_location(node);
    let Some(violation) = find_violation(context.checker, source, contextual_type, is_literal)
    else {
        return;
    };
    let Ok(type_name) = String::from_utf8(context.checker.type_to_string_bytes_exported(violation))
    else {
        return;
    };
    if ast::is_object_literal_expression(node) {
        context.report_node(node, build_object_literal_message(&type_name));
    } else {
        context.report_node(node, build_non_instance_message(&type_name));
    }
}

fn check_arguments(arguments: &[P<Node>], context: &mut RuleContext<'_>) {
    for &argument in arguments {
        if !ast::is_spread_element(argument) {
            check_site(Some(argument), context);
        }
    }
}

pub struct NoUnsafeClassInstanceAssignment;

impl Rule for NoUnsafeClassInstanceAssignment {
    fn listens_to(&self, listener: Listener) -> bool {
        matches!(
            listener,
            Listener::Enter(
                Kind::ArrayLiteralExpression
                    | Kind::ArrowFunction
                    | Kind::BinaryExpression
                    | Kind::BindingElement
                    | Kind::CallExpression
                    | Kind::ConditionalExpression
                    | Kind::JsxExpression
                    | Kind::NewExpression
                    | Kind::Parameter
                    | Kind::PropertyAssignment
                    | Kind::PropertyDeclaration
                    | Kind::ReturnStatement
                    | Kind::ShorthandPropertyAssignment
                    | Kind::AsExpression
                    | Kind::SatisfiesExpression
                    | Kind::TypeAssertionExpression
                    | Kind::VariableDeclaration
                    | Kind::YieldExpression
            )
        )
    }

    fn visit(&mut self, _: Listener, node: P<Node>, context: &mut RuleContext<'_>) {
        match node.kind() {
            Kind::ArrayLiteralExpression => {
                if ast::is_assignment_target(node) {
                    return;
                }
                for &element in node.elements() {
                    if !ast::is_omitted_expression(element) && !ast::is_spread_element(element) {
                        check_site(Some(element), context);
                    }
                }
            }
            Kind::ArrowFunction => {
                let body = node.body();
                if !body.is_some_and(ast::is_block) {
                    check_site(body, context);
                }
            }
            Kind::BinaryExpression => {
                let expression = node.as_binary_expression();
                match expression.operator_token.kind() {
                    Kind::EqualsToken
                    | Kind::AmpersandAmpersandEqualsToken
                    | Kind::QuestionQuestionEqualsToken
                    | Kind::BarBarEqualsToken
                    | Kind::CommaToken => check_site(Some(expression.right()), context),
                    Kind::AmpersandAmpersandToken
                    | Kind::BarBarToken
                    | Kind::QuestionQuestionToken => {
                        check_site(Some(expression.left), context);
                        check_site(Some(expression.right()), context);
                    }
                    _ => {}
                }
            }
            Kind::CallExpression => {
                if !ast::is_import_call(node) {
                    check_arguments(node.arguments(), context);
                }
            }
            Kind::ConditionalExpression => {
                let expression = node.as_conditional_expression();
                check_site(Some(expression.when_true), context);
                check_site(Some(expression.when_false), context);
            }
            Kind::JsxExpression => {
                let expression = node.as_jsx_expression();
                if expression.dot_dot_dot_token.is_none() {
                    check_site(expression.expression, context);
                }
            }
            Kind::NewExpression => check_arguments(node.arguments(), context),
            Kind::PropertyAssignment => {
                if !ast::is_assignment_target(node.parent().unwrap()) {
                    check_site(node.initializer(), context);
                }
            }
            Kind::ShorthandPropertyAssignment => {
                if let Some(initializer) =
                    node.as_shorthand_property_assignment().object_assignment_initializer()
                {
                    check_site(Some(initializer), context);
                } else if !ast::is_assignment_target(node.parent().unwrap()) {
                    check_site(node.name(), context);
                }
            }
            Kind::BindingElement
            | Kind::Parameter
            | Kind::PropertyDeclaration
            | Kind::VariableDeclaration => check_site(node.initializer(), context),
            Kind::ReturnStatement
            | Kind::AsExpression
            | Kind::SatisfiesExpression
            | Kind::TypeAssertionExpression
            | Kind::YieldExpression => check_site(node.expression(), context),
            _ => unreachable!("unexpected listener kind"),
        }
    }
}
