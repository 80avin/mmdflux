//! Class diagram instance implementation.

use super::compiler;
use super::parser::parse_class_diagram;
use crate::diagram::{LayoutEngineId, OutputFormat, RenderConfig, RenderError};
use crate::graph::Diagram;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render};

/// Class diagram instance.
///
/// Parses class diagram syntax, compiles to `graph::Diagram`, then
/// renders through the shared graph-family pipeline.
pub struct ClassInstance {
    diagram: Option<Diagram>,
}

impl ClassInstance {
    /// Create a new class diagram instance.
    pub fn new() -> Self {
        Self { diagram: None }
    }
}

impl Default for ClassInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramInstance for ClassInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let model = parse_class_diagram(input)?;
        self.diagram = Some(compiler::compile(&model));
        Ok(())
    }

    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError> {
        let diagram = self.diagram.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        if let Some(engine) = config
            .layout_engine
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            let engine_id = LayoutEngineId::parse(engine)?;
            engine_id.check_available()?;
        }

        let mut options: RenderOptions = config.into();
        options.output_format = format;

        Ok(render(diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg
        )
    }
}
