//! Graph-family engine registry and adapters.
//!
//! All graph-family diagram types (flowchart, state, class, ER) share
//! the same engine registry. Engines are looked up by `LayoutEngineId`.

pub mod cose;
#[cfg(feature = "engine-elk")]
pub mod elk;
mod registry;

pub use registry::GraphEngineRegistry;
