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
        name => {
            require_implemented(name)?;
            unreachable!("registered rule factory")
        }
    }
}
