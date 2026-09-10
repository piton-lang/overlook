//! Reading a Piton specbase off disk and turning it into a graph.

pub mod loader;
pub mod model;
pub mod parser;

pub use loader::CONFIG_FILE;
pub use model::*;
