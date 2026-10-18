// Go internal/ls/autoimport.

mod aliasresolver;
mod export;
mod extract;
mod fix;
mod import_adder;
mod index;
mod registry;
mod specifiers;
mod util;
mod view;

pub use export::*;
pub use fix::{Fix, get_import_kind_for_import_statement};
pub use import_adder::*;
pub use index::{Index, Named};
pub use registry::*;
pub use view::*;

#[cfg(test)]
mod aliasresolver_crash_test;
#[cfg(test)]
mod index_test;
#[cfg(test)]
mod util_test;
