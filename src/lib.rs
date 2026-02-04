//! mmdflux - Convert Mermaid diagrams to text/SVG
//!
//! This library provides parsing and rendering for Mermaid diagram syntax.
//! Currently supports flowcharts with text (Unicode/ASCII) output.
//!
//! # Quick Start (Legacy API)
//!
//! ```
//! use mmdflux::{parse_flowchart, build_diagram};
//! use mmdflux::render::{render, RenderOptions};
//!
//! let input = "graph TD\n    A-->B";
//! let flowchart = parse_flowchart(input).unwrap();
//! let diagram = build_diagram(&flowchart);
//! let output = render(&diagram, &RenderOptions::default());
//! println!("{}", output);
//! ```
//!
//! # Using the Registry (New API)
//!
//! The registry provides a unified interface for all diagram types:
//!
//! ```
//! use mmdflux::registry::default_registry;
//! use mmdflux::diagram::{OutputFormat, RenderConfig};
//!
//! let registry = default_registry();
//! let input = "graph TD\n    A-->B";
//!
//! if let Some(diagram_id) = registry.detect(input) {
//!     let mut instance = registry.create(diagram_id).unwrap();
//!     instance.parse(input).unwrap();
//!     let output = instance.render(OutputFormat::Text, &RenderConfig::default()).unwrap();
//!     println!("{}", output);
//! }
//! ```

// Core modules
pub mod dagre;
pub mod diagram;
pub mod diagrams;
pub mod graph;
pub mod parser;
pub mod registry;
pub mod render;

// Re-export commonly used items from graph module
// Re-export diagram abstractions
pub use diagram::{OutputFormat, RenderConfig, RenderError};
pub use graph::{Diagram, Direction, Edge, Node, Shape, build_diagram};
// Re-export commonly used items from parser module
pub use parser::{DiagramType, Flowchart, ParseError, detect_diagram_type, parse_flowchart};
// Re-export registry entry point
pub use registry::default_registry;
