// Go `internal/testutil/contentmappertest`: realistic content mapper implementations used by tests. The spawner
// serves them in-process over an in-memory connection, selecting the implementation by the command the mapper
// package declares (`exec`), so that tests exercise the content mapper host and its JSON-RPC connection without a
// subprocess.

mod component;
mod diagnostic_code_collision;
mod duplicate;
mod duplicate_projection;
mod dynamic_verbatim;
mod editing;
mod failing;
mod hoisting;
mod lisp;
mod manifest;
mod protocol;
mod registry;
mod spawner;
mod supplemental;
mod supplemental_diagnostics;
mod supplemental_globals;
mod supplemental_module;
mod synthesizing;
mod transforming;
mod verbatim;

pub use dynamic_verbatim::ProjectLifecycle;
pub use manifest::*;
pub use registry::TRANSFORMING_MAPPER;
pub use spawner::{new_spawner, serve};
pub use transforming::{DECLARED_OPTIONS, Handler};
