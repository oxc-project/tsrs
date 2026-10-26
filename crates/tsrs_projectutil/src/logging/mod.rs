// Go internal/project/logging.

mod logcollector;
pub mod logger;
pub mod logtree;

pub use logcollector::*;
pub use logger::{Logger, Writer, format_time, new_logger, new_nop_logger};
pub use logtree::*;
