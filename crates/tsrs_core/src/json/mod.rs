#[expect(
    clippy::module_inception,
    reason = "The module path preserves the upstream Go package and file layout."
)]
mod json;
mod map;
mod plain;

pub use json::*;
pub use map::{Entry, JsonMap, OccupiedEntry, VacantEntry};
pub use plain::plain_ascii_len;
