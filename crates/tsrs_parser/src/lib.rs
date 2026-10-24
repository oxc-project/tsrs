pub mod construction;
pub mod context;
pub mod factory;
mod factory_generated;
mod jsdoc;
pub mod leaf_types;
pub mod lexical;
mod lexical_legacy;
pub mod lists;
pub mod owned;
pub mod parser_1;
mod parser_2;
mod parser_3;
pub mod recognition;
mod references;
mod reparser;
pub mod template_tokens;
pub mod tokens;
mod type_grammar;
pub mod types;
pub mod utilities;

pub use parser_1::{Parser, ParserState, ParsingContext};
pub use parser_1::{
    enable_lazy_dts, parse_isolated_entity_name, parse_source_file, parse_source_file_embedded,
    parse_source_file_keep_text, parse_source_file_owned,
};
pub use utilities::get_jsdoc_comment_ranges;
