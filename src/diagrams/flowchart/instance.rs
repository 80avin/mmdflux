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
        let options = RenderOptions {
            ascii_only: matches!(format, OutputFormat::Ascii),
            ranker: Some(config.layout.ranker),
            node_spacing: Some(config.layout.node_sep),
            rank_spacing: Some(config.layout.rank_sep),
            edge_spacing: Some(config.layout.edge_sep),
            margin: Some(config.layout.margin),
            cluster_ranksep: config.cluster_ranksep,
            padding: config.padding,
        };

        match format {
            OutputFormat::Text | OutputFormat::Ascii => Ok(render(diagram, &options)),
            OutputFormat::Svg => {
                // SVG rendering will be implemented in Sub-Plan C (0045)
                // For now, return an error
                Err(RenderError {
                    message: "SVG output not yet implemented. See plan 0045.".to_string(),
                })
            }
        }
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        // SVG support is planned
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg
        )
    }
}
