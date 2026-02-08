//! MMDS diagram instance scaffold.

use super::{from_mmds_output, parse_mmds_input};
use crate::diagram::{OutputFormat, RenderConfig, RenderError};
use crate::mmds::MmdsOutput;
use crate::registry::DiagramInstance;

/// MMDS diagram instance.
#[derive(Default)]
pub struct MmdsInstance {
    parsed_payload: Option<MmdsOutput>,
}

impl MmdsInstance {
    /// Returns true when the instance has a parsed MMDS payload.
    pub fn has_parsed_payload(&self) -> bool {
        self.parsed_payload.is_some()
    }

    /// Access the parsed MMDS payload.
    pub fn parsed_payload(&self) -> Option<&MmdsOutput> {
        self.parsed_payload.as_ref()
    }

    fn scaffold_render_error() -> RenderError {
        RenderError {
            message: "MMDS input scaffold: hydration/render pipeline is not implemented yet"
                .to_string(),
        }
    }
}

impl DiagramInstance for MmdsInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let payload = parse_mmds_input(input)?;
        self.parsed_payload = Some(payload);
        Ok(())
    }

    fn render(&self, _format: OutputFormat, _config: &RenderConfig) -> Result<String, RenderError> {
        let payload = self.parsed_payload.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        if let Err(err) = from_mmds_output(payload) {
            return Err(RenderError {
                message: err.to_string(),
            });
        }

        Err(Self::scaffold_render_error())
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
        )
    }
}
