//! Rule inventory from the pinned tsgolint registry.

pub const REFERENCE_COMMIT: &str = "8897346fff24a9bd0c719099dc1af8e7200572c1";
pub const REFERENCE_COMPILER_COMMIT: &str = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";

pub const BASELINE_RULES: &[&str] = &[
    "await-thenable",
    "consistent-return",
    "consistent-type-exports",
    "dot-notation",
    "no-array-delete",
    "no-base-to-string",
    "no-confusing-void-expression",
    "no-deprecated",
    "no-duplicate-type-constituents",
    "no-floating-promises",
    "no-for-in-array",
    "no-generated-empty-object-type",
    "no-implied-eval",
    "no-meaningless-void-operator",
    "no-misused-promises",
    "no-misused-spread",
    "no-mixed-enums",
    "no-redundant-type-constituents",
    "no-unnecessary-boolean-literal-compare",
    "no-unnecessary-condition",
    "no-unnecessary-qualifier",
    "no-unnecessary-template-expression",
    "no-unnecessary-type-conversion",
    "no-unnecessary-type-arguments",
    "no-unnecessary-type-parameters",
    "no-unnecessary-type-assertion",
    "no-useless-default-assignment",
    "no-unsafe-argument",
    "no-unsafe-assignment",
    "no-unsafe-call",
    "no-unsafe-enum-comparison",
    "no-unsafe-member-access",
    "no-unsafe-return",
    "no-unsafe-type-assertion",
    "no-unsafe-unary-minus",
    "non-nullable-type-assertion-style",
    "only-throw-error",
    "prefer-find",
    "prefer-includes",
    "prefer-optional-chain",
    "prefer-nullish-coalescing",
    "prefer-promise-reject-errors",
    "prefer-readonly-parameter-types",
    "prefer-regexp-exec",
    "prefer-readonly",
    "prefer-reduce-type-parameter",
    "prefer-return-this-type",
    "prefer-string-starts-ends-with",
    "promise-function-async",
    "related-getter-setter-pairs",
    "require-array-sort-compare",
    "require-await",
    "restrict-plus-operands",
    "restrict-template-expressions",
    "return-await",
    "strict-boolean-expressions",
    "strict-void-return",
    "switch-exhaustiveness-check",
    "unbound-method",
    "use-unknown-in-catch-callback-variable",
];

pub fn require_implemented(name: &str) -> Result<(), String> {
    if matches!(
        name,
        "await-thenable"
            | "consistent-return"
            | "consistent-type-exports"
            | "dot-notation"
            | "no-array-delete"
            | "no-base-to-string"
            | "no-confusing-void-expression"
            | "no-deprecated"
            | "no-duplicate-type-constituents"
            | "no-floating-promises"
            | "no-for-in-array"
            | "no-implied-eval"
            | "no-meaningless-void-operator"
            | "no-misused-promises"
            | "no-misused-spread"
            | "no-mixed-enums"
            | "no-redundant-type-constituents"
            | "no-unnecessary-boolean-literal-compare"
            | "no-unnecessary-condition"
            | "no-unnecessary-qualifier"
            | "no-unnecessary-template-expression"
            | "no-unnecessary-type-conversion"
            | "no-unnecessary-type-arguments"
            | "no-unnecessary-type-parameters"
            | "no-unnecessary-type-assertion"
            | "no-useless-default-assignment"
            | "no-unsafe-argument"
            | "no-unsafe-assignment"
            | "no-unsafe-call"
            | "no-unsafe-enum-comparison"
            | "no-unsafe-member-access"
            | "no-unsafe-return"
            | "no-unsafe-type-assertion"
            | "no-unsafe-unary-minus"
            | "non-nullable-type-assertion-style"
            | "only-throw-error"
            | "prefer-find"
            | "prefer-includes"
            | "prefer-optional-chain"
            | "prefer-nullish-coalescing"
            | "prefer-promise-reject-errors"
            | "prefer-readonly-parameter-types"
            | "prefer-regexp-exec"
            | "prefer-readonly"
    ) {
        return Ok(());
    }
    if BASELINE_RULES.contains(&name) {
        Err(format!("rule not yet implemented: {name}"))
    } else {
        Err(format!("unknown rule: {name}"))
    }
}

pub fn create_rule(
    config: &crate::payload::ConfiguredRule,
) -> Result<Box<dyn crate::rule::Rule>, String> {
    match config.name.as_str() {
        "await-thenable" => Ok(Box::new(crate::rules::await_thenable::AwaitThenable)),
        "consistent-return" => {
            crate::rules::consistent_return::ConsistentReturn::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "consistent-type-exports" => {
            crate::rules::consistent_type_exports::ConsistentTypeExports::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "dot-notation" => crate::rules::dot_notation::DotNotation::new(&config.options)
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "no-array-delete" => Ok(Box::new(crate::rules::no_array_delete::NoArrayDelete)),
        "no-base-to-string" => {
            crate::rules::no_base_to_string::NoBaseToString::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-confusing-void-expression" => {
            crate::rules::no_confusing_void_expression::NoConfusingVoidExpression::new(
                &config.options,
            )
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-deprecated" => crate::rules::no_deprecated::NoDeprecated::new(&config.options)
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "no-duplicate-type-constituents" => {
            crate::rules::no_duplicate_type_constituents::NoDuplicateTypeConstituents::new(
                &config.options,
            )
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-floating-promises" => {
            crate::rules::no_floating_promises::NoFloatingPromises::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-for-in-array" => Ok(Box::new(crate::rules::no_for_in_array::NoForInArray)),
        "no-implied-eval" => Ok(Box::new(crate::rules::no_implied_eval::NoImpliedEval)),
        "no-meaningless-void-operator" => {
            crate::rules::no_meaningless_void_operator::NoMeaninglessVoidOperator::new(
                &config.options,
            )
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-misused-promises" => {
            crate::rules::no_misused_promises::NoMisusedPromises::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-misused-spread" => {
            crate::rules::no_misused_spread::NoMisusedSpread::new(&config.options)
                .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>)
        }
        "no-mixed-enums" => Ok(Box::new(crate::rules::no_mixed_enums::NoMixedEnums)),
        "no-redundant-type-constituents" => Ok(Box::new(
            crate::rules::no_redundant_type_constituents::NoRedundantTypeConstituents,
        )),
        "no-unnecessary-boolean-literal-compare" => crate::rules::no_unnecessary_boolean_literal_compare::NoUnnecessaryBooleanLiteralCompare::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "no-unnecessary-condition" => crate::rules::no_unnecessary_condition::NoUnnecessaryCondition::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "no-unnecessary-qualifier" => Ok(Box::new(crate::rules::no_unnecessary_qualifier::NoUnnecessaryQualifier::default())),
        "no-unnecessary-template-expression" => Ok(Box::new(crate::rules::no_unnecessary_template_expression::NoUnnecessaryTemplateExpression)),
        "no-unnecessary-type-conversion" => Ok(Box::new(crate::rules::no_unnecessary_type_conversion::NoUnnecessaryTypeConversion)),
        "no-unnecessary-type-arguments" => Ok(Box::new(crate::rules::no_unnecessary_type_arguments::NoUnnecessaryTypeArguments)),
        "no-unnecessary-type-parameters" => Ok(Box::new(crate::rules::no_unnecessary_type_parameters::NoUnnecessaryTypeParameters)),
        "no-unnecessary-type-assertion" => crate::rules::no_unnecessary_type_assertion::NoUnnecessaryTypeAssertion::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "no-useless-default-assignment" => Ok(Box::new(crate::rules::no_useless_default_assignment::NoUselessDefaultAssignment)),
        "no-unsafe-argument" => Ok(Box::new(crate::rules::no_unsafe_argument::NoUnsafeArgument)),
        "no-unsafe-assignment" => Ok(Box::new(crate::rules::no_unsafe_assignment::NoUnsafeAssignment)),
        "no-unsafe-call" => Ok(Box::new(crate::rules::no_unsafe_call::NoUnsafeCall)),
        "no-unsafe-enum-comparison" => Ok(Box::new(crate::rules::no_unsafe_enum_comparison::NoUnsafeEnumComparison)),
        "no-unsafe-member-access" => Ok(Box::new(crate::rules::no_unsafe_member_access::NoUnsafeMemberAccess::new(&config.options)?)),
        "no-unsafe-return" => Ok(Box::new(crate::rules::no_unsafe_return::NoUnsafeReturn)),
        "no-unsafe-type-assertion" => Ok(Box::new(crate::rules::no_unsafe_type_assertion::NoUnsafeTypeAssertion)),
        "no-unsafe-unary-minus" => Ok(Box::new(crate::rules::no_unsafe_unary_minus::NoUnsafeUnaryMinus)),
        "non-nullable-type-assertion-style" => Ok(Box::new(crate::rules::non_nullable_type_assertion_style::NonNullableTypeAssertionStyle)),
        "only-throw-error" => crate::rules::only_throw_error::OnlyThrowError::new(&config.options)
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "prefer-find" => Ok(Box::new(crate::rules::prefer_find::PreferFind)),
        "prefer-includes" => Ok(Box::new(crate::rules::prefer_includes::PreferIncludes)),
        "prefer-optional-chain" => crate::rules::prefer_optional_chain::PreferOptionalChain::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "prefer-nullish-coalescing" => crate::rules::prefer_nullish_coalescing::PreferNullishCoalescing::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "prefer-promise-reject-errors" => crate::rules::prefer_promise_reject_errors::PreferPromiseRejectErrors::new(&config.options)
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "prefer-readonly-parameter-types" => crate::rules::prefer_readonly_parameter_types::PreferReadonlyParameterTypes::new(&config.options)
            .map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        "prefer-regexp-exec" => Ok(Box::new(crate::rules::prefer_regexp_exec::PreferRegexpExec)),
        "prefer-readonly" => crate::rules::prefer_readonly::PreferReadonly::new(&config.options).map(|rule| Box::new(rule) as Box<dyn crate::rule::Rule>),
        name => {
            require_implemented(name)?;
            unreachable!("registered rule factory")
        }
    }
}
