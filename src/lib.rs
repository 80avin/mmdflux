//! mmdflux - Convert Mermaid diagrams to ASCII art
//!
//! This library provides parsing and rendering of Mermaid flowchart diagrams.
//!
//! # Example
//!
//! ```
//! use mmdflux::{parse_flowchart, build_diagram};
//!
//! let input = "graph TD\nA[Start] --> B{Decision}\n";
//! let flowchart = parse_flowchart(input).unwrap();
//! let diagram = build_diagram(&flowchart);
//!
//! assert_eq!(diagram.nodes.len(), 2);
//! assert_eq!(diagram.edges.len(), 1);
//! ```

pub mod graph;
pub mod parser;
pub mod render;

// Re-export commonly used items at the crate root
pub use graph::{Diagram, Direction, Edge, Node, Shape, build_diagram};
pub use parser::{Flowchart, ParseError, parse_flowchart};
