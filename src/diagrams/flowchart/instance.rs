//! Flowchart diagram instance implementation.

use crate::diagram::{OutputFormat, RenderConfig, RenderError};
use crate::graph::{Diagram, build_diagram};
use crate::json::to_json;
use crate::parser::parse_flowchart;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, compute_layout_direct, layout_config_for_diagram, render};

/// Flowchart diagram instance.
///
/// Wraps the existing flowchart parsing and rendering logic behind
/// the `DiagramInstance` trait.
pub struct FlowchartInstance {
    /// Built diagram model.
    diagram: Option<Diagram>,
}

impl FlowchartInstance {
    /// Create a new flowchart instance.
    pub fn new() -> Self {
        Self { diagram: None }
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
        self.diagram = Some(build_diagram(&flowchart));
        Ok(())
    }

    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError> {
        let diagram = self.diagram.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        if matches!(format, OutputFormat::Json) {
            let mut options: RenderOptions = config.into();
            options.output_format = format;
            let layout_config = layout_config_for_diagram(diagram, &options);
            let layout = compute_layout_direct(diagram, &layout_config);
            return Ok(to_json(diagram, Some(&layout)));
        }

        // Convert RenderConfig to RenderOptions
        let mut options: RenderOptions = config.into();
        options.output_format = format;

        Ok(render(diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
        )
    }
}
