//! Native type-aware linting on the tsrs AST and checker.
#![expect(
    clippy::multiple_crate_versions,
    reason = "regress 0.11 uses hashbrown 0.16; the compiler uses hashbrown 0.17"
)]

mod go_quote;
pub mod payload;
pub mod protocol;
pub mod registry;

pub mod engine;

pub mod rule;

pub mod utils;

mod type_matches_specifier;

mod rules {
    pub mod await_thenable;
    pub mod consistent_return;
    pub mod consistent_type_exports;
    pub mod dot_notation;
    pub mod no_array_delete;
    pub mod no_base_to_string;
    pub mod no_confusing_void_expression;
    pub mod no_deprecated;
    pub mod no_duplicate_type_constituents;
    pub mod no_floating_promises;
    pub mod no_for_in_array;
    pub mod no_implied_eval;
    pub mod no_meaningless_void_operator;
    pub mod no_misused_promises;
    pub mod no_misused_spread;
    pub mod no_mixed_enums;
    pub mod no_redundant_type_constituents;
    pub mod no_unnecessary_boolean_literal_compare;
    pub mod no_unnecessary_condition;
    pub mod no_unnecessary_qualifier;
    pub mod no_unnecessary_template_expression;
    pub mod no_unnecessary_type_conversion;
}
