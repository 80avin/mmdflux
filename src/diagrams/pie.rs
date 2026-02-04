//! Pie diagram shim.
//!
//! Pie diagrams are rendered as simple text representations.
//! Future enhancement: render as horizontal bar charts.

use crate::diagram::{DiagramFamily, OutputFormat, RenderConfig, RenderError};
use crate::registry::{DiagramDefinition, DiagramDetector, DiagramInstance};

/// Detect if input is a pie diagram.
pub fn detect(input: &str) -> bool {
    input.trim_start().starts_with("pie")
}

/// Pie diagram definition for registry.
pub fn definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "pie",
        family: DiagramFamily::Chart,
        detector: detect as DiagramDetector,
        factory: || Box::new(PieInstance::new()),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii],
    }
}

/// Pie diagram instance.
pub struct PieInstance {
    input: Option<String>,
}

impl PieInstance {
    /// Create a new pie diagram instance.
    pub fn new() -> Self {
        Self { input: None }
    }
}

impl Default for PieInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramInstance for PieInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Minimal validation: just check it starts with "pie"
        if !detect(input) {
            return Err("Not a pie diagram".into());
        }
        self.input = Some(input.to_string());
        Ok(())
    }

    fn render(&self, _format: OutputFormat, _config: &RenderConfig) -> Result<String, RenderError> {
        let input = self.input.as_ref().ok_or("Not parsed")?;

        // Trivial rendering: echo the input with a header
        // This matches current behavior in main.rs
        Ok(format!("[Pie Chart]\n{}", input))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(format, OutputFormat::Text | OutputFormat::Ascii)
    }
}
