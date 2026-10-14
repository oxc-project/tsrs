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
}
