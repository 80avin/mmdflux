//! Flowchart diagram instance implementation.

use crate::diagram::{OutputFormat, RenderConfig, RenderError};
use crate::graph::{Diagram, build_diagram};
use crate::parser::{Flowchart, parse_flowchart};
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render};

/// Flowchart diagram instance.
///
/// Wraps the existing flowchart parsing and rendering logic behind
/// the `DiagramInstance` trait.
pub struct FlowchartInstance {
    /// Parsed AST (kept for potential re-rendering with different options)
    #[allow(dead_code)]
    flowchart: Option<Flowchart>,
    /// Built diagram model
    diagram: Option<Diagram>,
}

impl FlowchartInstance {
    /// Create a new flowchart instance.
    pub fn new() -> Self {
        Self {
            flowchart: None,
            diagram: None,
        }
    }
}

impl Default for FlowchartInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramInstance for FlowchartInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let flowchart = parse_flowchart(input)?;
        let diagram = build_diagram(&flowchart);
        self.flowchart = Some(flowchart);
        self.diagram = Some(diagram);
        Ok(())
    }

    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError> {
        let diagram = self.diagram.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        // Convert RenderConfig to RenderOptions
        let mut options: RenderOptions = config.into();
        options.output_format = format;

        Ok(render(diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(format, OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg)
    }
}
