//! Port of tsgolint's `internal/rules/no_unsafe_type_assertion/no_unsafe_type_assertion.go`.

use tsrs_ast::{Kind, Node};
use tsrs_checker::{ObjectFlags, Type, TypeFlags};
use tsrs_core::P;

use crate::protocol::RuleMessage;
use crate::rule::{Listener, Rule, RuleContext};
use crate::utils;

pub struct NoUnsafeTypeAssertion;

fn any_type_name(ty: P<Type>) -> &'static str {
    if utils::is_intrinsic_error_type(ty) { "error typed" } else { "`any`" }
}

fn message(id: &str, ty: &str) -> RuleMessage {
    RuleMessage {
        id: id.into(),
        description: match id {
            "unsafeOfAnyTypeAssertion" => format!("Unsafe assertion from {ty} detected."),
            "unsafeToAnyTypeAssertion" => format!("Unsafe assertion to {ty} detected."),
            "unsafeToUnconstrainedTypeAssertion" => format!(
                "Unsafe type assertion: '{ty}' could be instantiated with an arbitrary type which could be unrelated to the original type."
            ),
            "unsafeTypeAssertion" => {
                format!("Unsafe type assertion: type '{ty}' is more narrow than the original type.")
            }
            "unsafeTypeAssertionAssignableToConstraint" => format!(
                "Unsafe type assertion: the original type is assignable to the constraint of type '{ty}', but '{ty}' could be instantiated with a different subtype of its constraint."
            ),
            _ => unreachable!("no-unsafe-type-assertion message"),
        },
        ..Default::default()
    }
}

fn report(
    context: &mut RuleContext<'_>,
    node: P<Node>,
    original: P<Type>,
    asserted: P<Type>,
    mut message: RuleMessage,
) {
    let render = |checker: &mut tsrs_checker::Checker, ty| {
        if utils::is_intrinsic_error_type(ty) {
            Some("error".into())
        } else {
            String::from_utf8(checker.type_to_string_bytes_exported(ty)).ok()
        }
    };
    let Some(original) = render(context.checker, original) else {
        return;
    };
    let Some(asserted) = render(context.checker, asserted) else {
        return;
    };
    let guidance = match message.id.as_str() {
        "unsafeOfAnyTypeAssertion" => {
            "Use a type guard to check the value before asserting its type."
        }
        "unsafeToAnyTypeAssertion" => {
            "Keep the original type or use `unknown` and narrow the value before use."
        }
        _ => "Use a type guard to narrow the value before asserting a more specific type.",
    };
    message.help = format!(
        "The original expression has type `{original}`, and the assertion changes it to `{asserted}`. {guidance}"
    );
    context.report_node(node, message);
}

impl Rule for NoUnsafeTypeAssertion {
    fn listens_to(&self, listener: Listener) -> bool {
        matches!(listener, Listener::Enter(Kind::AsExpression | Kind::TypeAssertionExpression))
    }
    fn visit(&mut self, _: Listener, node: P<Node>, context: &mut RuleContext<'_>) {
        let expression = node.expression().unwrap();
        let annotation = node.type_node().unwrap();
        let expression_type = context.checker.get_type_at_location(expression);
        let asserted_type = context.checker.get_type_at_location(annotation);
        if expression_type == asserted_type {
            return;
        }
        if asserted_type.flags().intersects(TypeFlags::Any)
            && expression_type.flags().intersects(TypeFlags::Unknown)
        {
            report(
                context,
                node,
                expression_type,
                asserted_type,
                message("unsafeToAnyTypeAssertion", "`any`"),
            );
            return;
        }
        if let Some((_, sender)) = utils::is_unsafe_assignment(
            expression_type,
            asserted_type,
            context.checker,
            Some(expression),
        ) {
            report(
                context,
                node,
                expression_type,
                asserted_type,
                message("unsafeOfAnyTypeAssertion", any_type_name(sender)),
            );
            return;
        }
        if let Some((_, sender)) = utils::is_unsafe_assignment(
            asserted_type,
            expression_type,
            context.checker,
            Some(annotation),
        ) {
            report(
                context,
                node,
                expression_type,
                asserted_type,
                message("unsafeToAnyTypeAssertion", any_type_name(sender)),
            );
            return;
        }
        let widened_type = if expression_type.flags().intersects(TypeFlags::Object)
            && expression_type.object_flags().intersects(ObjectFlags::ObjectLiteral)
        {
            context.checker.get_widened_type(expression_type)
        } else {
            expression_type
        };
        if context.checker.is_type_assignable_to(widened_type, asserted_type) {
            return;
        }
        if asserted_type.flags().intersects(TypeFlags::TypeParameter) {
            let constraint = context.checker.get_base_constraint_of_type(asserted_type);
            let Some(constraint) = constraint else {
                let ty = context.checker.type_to_string_exported(asserted_type);
                report(
                    context,
                    node,
                    expression_type,
                    asserted_type,
                    message("unsafeToUnconstrainedTypeAssertion", &ty),
                );
                return;
            };
            if context.checker.is_type_assignable_to(widened_type, constraint) {
                let ty = context.checker.type_to_string_exported(asserted_type);
                report(
                    context,
                    node,
                    expression_type,
                    asserted_type,
                    message("unsafeTypeAssertionAssignableToConstraint", &ty),
                );
                return;
            }
        }
        let ty = context.checker.type_to_string_exported(asserted_type);
        report(context, node, expression_type, asserted_type, message("unsafeTypeAssertion", &ty));
    }
}
