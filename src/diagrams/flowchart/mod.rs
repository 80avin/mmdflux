//! Flowchart diagram implementation.
//!
//! Flowcharts are node-edge graphs rendered using the Dagre layout engine.
//! This is the original and most feature-complete diagram type in mmdflux.

mod instance;

pub use instance::FlowchartInstance;

use crate::diagram::{DiagramFamily, OutputFormat};
use crate::registry::{DiagramDefinition, DiagramDetector};

/// Detect if input is a flowchart diagram.
///
/// Matches:
/// - `graph TD`, `graph LR`, etc.
/// - `flowchart TD`, `flowchart LR`, etc.
pub fn detect(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with("graph ")
        || trimmed.starts_with("graph\n")
        || trimmed.starts_with("flowchart ")
        || trimmed.starts_with("flowchart\n")
}

/// Flowchart diagram definition for registry.
pub fn definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "flowchart",
        family: DiagramFamily::Graph,
        detector: detect as DiagramDetector,
        factory: || Box::new(FlowchartInstance::new()),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii, OutputFormat::Svg],
    }
}
