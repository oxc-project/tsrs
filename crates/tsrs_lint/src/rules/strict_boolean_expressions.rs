use rustc_hash::FxHashSet;
use tsrs_ast::{self as ast, Kind, Node};
use tsrs_checker::{
    CheckMode, Checker, LiteralValue, SignatureKind, Type, TypeData, TypeFlags, TypePredicate,
    TypePredicateKind,
};
use tsrs_core::{P, json::Value};

use crate::protocol::{Range, RuleMessage};
use crate::rule::{Listener, Rule, RuleContext};
use crate::utils;

fn message(id: &str) -> RuleMessage {
    let description = match id {
        "conditionErrorNumber" => {
            "Unexpected number value in conditional. A number can be falsy (0, NaN) or truthy."
        }
        "conditionErrorString" => {
            "Unexpected string value in conditional. A string can be falsy (empty string) or truthy."
        }
        "conditionErrorObject" => {
            "Unexpected object value in conditional. An object is always truthy."
        }
        "conditionErrorNullish" => {
            "Unexpected nullish value in conditional. The expression is always falsy."
        }
        "conditionErrorOther" => {
            "Unexpected value in conditional. A union of different types has inconsistent truthiness."
        }
        "conditionErrorNullableBoolean" => "Unexpected nullable boolean value in conditional.",
        "conditionErrorNullableObject" => "Unexpected nullable object value in conditional.",
        "conditionErrorNullableString" => "Unexpected nullable string value in conditional.",
        "conditionErrorNullableNumber" => "Unexpected nullable number value in conditional.",
        "conditionErrorNullableEnum" => "Unexpected nullable enum value in conditional.",
        "conditionErrorAny" => "Unexpected any value in conditional.",
        "noStrictNullCheck" => {
            "This rule requires the `strictNullChecks` compiler option to be turned on to function correctly."
        }
        "predicateCannotBeAsync" => {
            "Predicate function should not be 'async'; expected a boolean return type."
        }
        _ => unreachable!("strict-boolean-expressions message"),
    };
    let help = match id {
        "conditionErrorNullableBoolean" | "conditionErrorNullableObject" => {
            "Handle the nullish case explicitly."
        }
        "conditionErrorNullableString" => "Handle the nullish and empty string cases explicitly.",
        "conditionErrorNullableNumber" => "Handle the nullish and zero cases explicitly.",
        "conditionErrorNullableEnum" => "Handle the nullish and falsy enum cases explicitly.",
        "conditionErrorAny" => "Use a more specific type to ensure type safety.",
        _ => "",
    };
    RuleMessage { id: id.into(), description: description.into(), help: help.into() }
}

#[derive(Clone, Copy)]
struct StrictBooleanExpressionsOptions {
    allow_any: bool,
    allow_nullable_boolean: bool,
    allow_nullable_enum: bool,
    allow_nullable_number: bool,
    allow_nullable_object: bool,
    allow_nullable_string: bool,
    allow_number: bool,
    allow_string: bool,
}

pub struct StrictBooleanExpressions {
    options: StrictBooleanExpressionsOptions,
    traversed_nodes: FxHashSet<P<Node>>,
}

impl StrictBooleanExpressions {
    pub fn new(options: &Value) -> Result<Self, String> {
        let object = match options {
            Value::Null => None,
            Value::Object(object) => Some(object),
            _ => return Err("strict-boolean-expressions: options must be an object".into()),
        };
        let boolean = |name: &str, default| match object.and_then(|object| object.get(name)) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(value)) => Ok(*value),
            _ => Err(format!("strict-boolean-expressions: {name} must be a boolean")),
        };
        Ok(Self {
            options: StrictBooleanExpressionsOptions {
                allow_any: boolean("allowAny", false)?,
                allow_nullable_boolean: boolean("allowNullableBoolean", false)?,
                allow_nullable_enum: boolean("allowNullableEnum", false)?,
                allow_nullable_number: boolean("allowNullableNumber", false)?,
                allow_nullable_object: boolean("allowNullableObject", true)?,
                allow_nullable_string: boolean("allowNullableString", false)?,
                allow_number: boolean("allowNumber", true)?,
                allow_string: boolean("allowString", true)?,
            },
            traversed_nodes: FxHashSet::default(),
        })
    }
}

impl Rule for StrictBooleanExpressions {
    fn initialize(&mut self, context: &mut RuleContext<'_>) {
        let options = context.program.options();
        if !options.get_strict_option_value(options.strict_null_checks) {
            context.report_range(Range { pos: 0, end: 0 }, message("noStrictNullCheck"));
        }
    }

    fn listens_to(&self, listener: Listener) -> bool {
        matches!(
            listener,
            Listener::Enter(
                Kind::IfStatement
                    | Kind::WhileStatement
                    | Kind::DoStatement
                    | Kind::ForStatement
                    | Kind::ConditionalExpression
                    | Kind::BinaryExpression
                    | Kind::CallExpression
                    | Kind::PrefixUnaryExpression
            )
        )
    }

    fn visit(&mut self, _: Listener, node: P<Node>, context: &mut RuleContext<'_>) {
        match node.kind() {
            Kind::IfStatement | Kind::WhileStatement | Kind::DoStatement => {
                traverse_node(
                    context,
                    node.expression().unwrap(),
                    self.options,
                    &mut self.traversed_nodes,
                    true,
                );
            }
            Kind::ForStatement => {
                if let Some(condition) = node.as_for_statement().condition {
                    traverse_node(
                        context,
                        condition,
                        self.options,
                        &mut self.traversed_nodes,
                        true,
                    );
                }
            }
            Kind::ConditionalExpression => {
                traverse_node(
                    context,
                    node.as_conditional_expression().condition,
                    self.options,
                    &mut self.traversed_nodes,
                    true,
                );
            }
            Kind::BinaryExpression => {
                if ast::is_logical_expression(node)
                    && node.as_binary_expression().operator_token.kind()
                        != Kind::QuestionQuestionToken
                {
                    traverse_logical_expression(
                        context,
                        node,
                        self.options,
                        &mut self.traversed_nodes,
                        false,
                    );
                }
            }
            Kind::CallExpression => {
                if let Some(argument) = find_truthiness_asserted_argument(context.checker, node) {
                    traverse_node(context, argument, self.options, &mut self.traversed_nodes, true);
                }
                if utils::is_array_method_call_with_predicate(context.checker, node) {
                    if let Some(&argument) = node.as_call_expression().arguments.nodes().first() {
                        if ast::get_function_flags(Some(argument))
                            .intersects(ast::FunctionFlags::Async)
                        {
                            context.report_node(argument, message("predicateCannotBeAsync"));
                            return;
                        }
                        let function_type = context.checker.get_type_at_location(argument);
                        let signatures = context
                            .checker
                            .get_signatures_of_type(function_type, SignatureKind::Call);
                        let mut types = Vec::new();
                        for &signature in signatures {
                            let mut return_type =
                                context.checker.get_return_type_of_signature(signature);
                            if return_type.flags().intersects(TypeFlags::TypeParameter) {
                                if let Some(constraint) =
                                    context.checker.get_constraint_of_type_parameter(return_type)
                                {
                                    return_type = constraint;
                                }
                            }
                            types.extend_from_slice(utils::union_type_parts(&return_type));
                        }
                        check_condition(context, node, &types, self.options);
                    }
                }
            }
            Kind::PrefixUnaryExpression => {
                let expression = node.as_prefix_unary_expression();
                if expression.operator == Kind::ExclamationToken {
                    traverse_node(
                        context,
                        expression.operand,
                        self.options,
                        &mut self.traversed_nodes,
                        true,
                    );
                }
            }
            _ => unreachable!("strict-boolean-expressions listener"),
        }
    }
}

fn find_truthiness_asserted_argument(checker: &mut Checker, node: P<Node>) -> Option<P<Node>> {
    let call = node.as_call_expression();
    let arguments = call.arguments.nodes();
    let arguments = &arguments[..arguments
        .iter()
        .position(|argument| argument.kind() == Kind::SpreadElement)
        .unwrap_or(arguments.len())];
    if arguments.is_empty() {
        return None;
    }
    let callee_type = checker.get_type_at_location(call.expression);
    let union_types = utils::union_type_parts(&callee_type);
    let signature = checker.get_resolved_signature(node, None, CheckMode::Normal);
    if let Some(predicate) = checker.get_type_predicate_of_signature_exported(signature) {
        return find_truthiness_asserted_argument_in_predicate(Some(predicate), arguments);
    }
    if union_types.len() > 1 {
        return find_truthiness_asserted_argument_in_union_signatures(
            checker,
            union_types,
            arguments,
        );
    }
    None
}

fn find_truthiness_asserted_argument_in_union_signatures(
    checker: &mut Checker,
    union_types: &[P<Type>],
    arguments: &[P<Node>],
) -> Option<P<Node>> {
    for &t in union_types {
        for &signature in checker.get_signatures_of_type(t, SignatureKind::Call) {
            let predicate = checker.get_type_predicate_of_signature_exported(signature);
            if let Some(argument) =
                find_truthiness_asserted_argument_in_predicate(predicate, arguments)
            {
                return Some(argument);
            }
        }
    }
    None
}

fn find_truthiness_asserted_argument_in_predicate(
    predicate: Option<P<TypePredicate>>,
    arguments: &[P<Node>],
) -> Option<P<Node>> {
    let predicate = predicate?;
    if predicate.kind() != TypePredicateKind::AssertsIdentifier || predicate.type_().is_some() {
        return None;
    }
    arguments.get(predicate.parameter_index()?).copied()
}

fn check_node(
    context: &mut RuleContext<'_>,
    node: P<Node>,
    options: StrictBooleanExpressionsOptions,
) {
    let node_type = utils::get_constrained_type_at_location(context.checker, node);
    check_condition(context, node, utils::union_type_parts(&node_type), options);
}

fn traverse_logical_expression(
    context: &mut RuleContext<'_>,
    node: P<Node>,
    options: StrictBooleanExpressionsOptions,
    traversed: &mut FxHashSet<P<Node>>,
    is_condition: bool,
) {
    let expression = node.as_binary_expression();
    traverse_node(context, expression.left, options, traversed, true);
    traverse_node(context, expression.right(), options, traversed, is_condition);
}

fn traverse_node(
    context: &mut RuleContext<'_>,
    node: P<Node>,
    options: StrictBooleanExpressionsOptions,
    traversed: &mut FxHashSet<P<Node>>,
    is_condition: bool,
) {
    if !traversed.insert(node) {
        return;
    }
    if node.kind() == Kind::ParenthesizedExpression {
        traverse_node(context, node.expression().unwrap(), options, traversed, is_condition);
        return;
    }
    if node.kind() == Kind::BinaryExpression
        && ast::is_logical_expression(node)
        && node.as_binary_expression().operator_token.kind() != Kind::QuestionQuestionToken
    {
        traverse_logical_expression(context, node, options, traversed, is_condition);
        return;
    }
    if is_condition {
        check_node(context, node, options);
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum TypeVariant {
    #[default]
    Nullish,
    Boolean,
    String,
    Number,
    BigInt,
    Object,
    Any,
    Unknown,
    Never,
    Mixed,
    Generic,
}

#[derive(Default)]
struct TypeInfo {
    variant: TypeVariant,
    is_nullable: bool,
    is_truthy: bool,
    is_enum: bool,
}

fn analyze_type_parts(types: &[P<Type>]) -> TypeInfo {
    let mut info = TypeInfo::default();
    let mut variants = Vec::new();
    let mut met_not_truthy = false;
    for &part in types {
        let part_info = analyze_type_part(part);
        if !variants.contains(&part_info.variant) {
            variants.push(part_info.variant);
        }
        if part_info.variant == TypeVariant::Nullish {
            info.is_nullable = true;
        }
        if part_info.is_enum {
            info.is_enum = true;
        }
        if matches!(
            part_info.variant,
            TypeVariant::Boolean | TypeVariant::Number | TypeVariant::String | TypeVariant::BigInt
        ) && !met_not_truthy
        {
            info.is_truthy = part_info.is_truthy;
            met_not_truthy = !part_info.is_truthy;
        }
    }
    info.variant = if variants.len() == 1 {
        variants[0]
    } else if variants.len() == 2 && info.is_nullable {
        *variants.iter().find(|&&variant| variant != TypeVariant::Nullish).unwrap()
    } else {
        TypeVariant::Mixed
    };
    info
}

fn analyze_type_part(t: P<Type>) -> TypeInfo {
    let mut info = TypeInfo::default();
    let flags = t.flags();
    if flags.intersects(TypeFlags::Intersection) {
        info.variant = if t
            .types()
            .iter()
            .any(|&part| analyze_type_part(part).variant == TypeVariant::Boolean)
        {
            TypeVariant::Boolean
        } else {
            TypeVariant::Object
        };
        return info;
    }
    info.variant = if flags.intersects(TypeFlags::TypeParameter) {
        TypeVariant::Generic
    } else if flags.intersects(TypeFlags::Any) {
        TypeVariant::Any
    } else if flags.intersects(TypeFlags::Unknown) {
        TypeVariant::Unknown
    } else if flags.intersects(TypeFlags::Never) {
        TypeVariant::Never
    } else if flags.intersects(TypeFlags::Null | TypeFlags::Undefined | TypeFlags::Void) {
        TypeVariant::Nullish
    } else if flags.intersects(TypeFlags::BooleanLike) {
        info.is_truthy = match t.data() {
            TypeData::Intrinsic(intrinsic) => intrinsic.intrinsic_name() == "true",
            TypeData::Literal(literal) => {
                matches!(literal.value(), Some(LiteralValue::Boolean(true)))
            }
            _ => false,
        };
        TypeVariant::Boolean
    } else if flags.intersects(TypeFlags::EnumLike) {
        info.is_enum = true;
        if flags.intersects(TypeFlags::StringLiteral) {
            TypeVariant::String
        } else {
            TypeVariant::Number
        }
    } else if flags.intersects(TypeFlags::StringLike) {
        if flags.intersects(TypeFlags::StringLiteral) {
            info.is_truthy = matches!(t.as_literal_type().value(), Some(LiteralValue::String(value)) if !value.is_empty());
        }
        TypeVariant::String
    } else if flags.intersects(TypeFlags::NumberLike) {
        if flags.intersects(TypeFlags::NumberLiteral) {
            info.is_truthy = matches!(t.as_literal_type().value(), Some(LiteralValue::Number(value)) if value.to_string() != "0");
        }
        TypeVariant::Number
    } else if flags.intersects(TypeFlags::BigInt | TypeFlags::BigIntLiteral) {
        info.is_truthy = flags.intersects(TypeFlags::BigIntLiteral);
        TypeVariant::BigInt
    } else if flags.intersects(
        TypeFlags::ESSymbol
            | TypeFlags::UniqueESSymbol
            | TypeFlags::Object
            | TypeFlags::NonPrimitive,
    ) {
        TypeVariant::Object
    } else {
        TypeVariant::Mixed
    };
    info
}

fn check_condition(
    context: &mut RuleContext<'_>,
    node: P<Node>,
    types: &[P<Type>],
    options: StrictBooleanExpressionsOptions,
) {
    let info = analyze_type_parts(types);
    let error = match info.variant {
        TypeVariant::Any | TypeVariant::Unknown | TypeVariant::Generic => {
            (!options.allow_any).then_some("conditionErrorAny")
        }
        TypeVariant::Never => None,
        TypeVariant::Nullish => Some("conditionErrorNullish"),
        TypeVariant::String => {
            if options.allow_string && info.is_truthy {
                None
            } else if info.is_nullable {
                if info.is_enum {
                    (!options.allow_nullable_enum).then_some("conditionErrorNullableEnum")
                } else {
                    (!options.allow_nullable_string).then_some("conditionErrorNullableString")
                }
            } else {
                (!options.allow_string).then_some("conditionErrorString")
            }
        }
        TypeVariant::Number => {
            if options.allow_number && info.is_truthy {
                None
            } else if info.is_nullable {
                if info.is_enum {
                    (!options.allow_nullable_enum).then_some("conditionErrorNullableEnum")
                } else {
                    (!options.allow_nullable_number).then_some("conditionErrorNullableNumber")
                }
            } else {
                (!options.allow_number).then_some("conditionErrorNumber")
            }
        }
        TypeVariant::Boolean => {
            (!info.is_truthy && info.is_nullable && !options.allow_nullable_boolean)
                .then_some("conditionErrorNullableBoolean")
        }
        TypeVariant::Object => {
            if info.is_nullable {
                (!options.allow_nullable_object).then_some("conditionErrorNullableObject")
            } else {
                Some("conditionErrorObject")
            }
        }
        TypeVariant::Mixed => {
            if info.is_enum {
                (info.is_nullable && !options.allow_nullable_enum)
                    .then_some("conditionErrorNullableEnum")
            } else {
                Some("conditionErrorOther")
            }
        }
        TypeVariant::BigInt => {
            if info.is_nullable {
                (!options.allow_nullable_number).then_some("conditionErrorNullableNumber")
            } else {
                (!options.allow_number).then_some("conditionErrorNumber")
            }
        }
    };
    if let Some(error) = error {
        context.report_node(node, message(error));
    }
}
