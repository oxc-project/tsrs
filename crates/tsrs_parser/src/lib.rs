mod jsdoc;
mod parser_1;
mod parser_2;
mod parser_3;
mod references;
mod reparser;
mod types;
mod utilities;

pub use parser_1::{
    JSDocInfo, Parser, ParserState, ParsingContext, ParsingContexts, enable_lazy_dts,
    parse_isolated_entity_name, parse_source_file, parse_source_file_embedded,
    parse_source_file_keep_text, parse_source_file_owned,
};
pub use types::ParseFlags;
pub use utilities::get_jsdoc_comment_ranges;
