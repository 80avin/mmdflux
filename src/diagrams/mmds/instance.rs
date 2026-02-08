//! MMDS diagram instance scaffold.

use super::{from_mmds_output, hydrate_graph_geometry_from_output, parse_mmds_input};
use crate::diagram::{OutputFormat, RenderConfig, RenderError, RoutingMode};
use crate::mmds::MmdsOutput;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render, render_svg_from_geometry};

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

    fn positioned_text_unsupported_error(format: OutputFormat) -> RenderError {
        let format_name = match format {
            OutputFormat::Ascii => "ascii",
            _ => "text",
        };
        RenderError {
            message: format!(
                "positioned MMDS {format_name} output is unsupported; use --format svg for positioned MMDS payloads"
            ),
        }
    }

    fn supports_format_for_payload(payload: &MmdsOutput, format: OutputFormat) -> bool {
        match payload.geometry_level.as_str() {
            "layout" => matches!(
                format,
                OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
            ),
            "routed" => matches!(format, OutputFormat::Svg | OutputFormat::Json),
            _ => true,
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

        if !Self::supports_format_for_payload(payload, _format) {
            return Err(Self::positioned_text_unsupported_error(_format));
        }

        if matches!(_format, OutputFormat::Json) {
            let json = serde_json::to_string_pretty(payload).map_err(|err| RenderError {
                message: format!("MMDS serialization error: {err}"),
            })?;
            return Ok(json);
        }

        let diagram = from_mmds_output(payload).map_err(|err| RenderError {
            message: err.to_string(),
        })?;

        let mut options: RenderOptions = _config.into();
        options.output_format = _format;

        if payload.geometry_level == "routed" {
            let geometry =
                hydrate_graph_geometry_from_output(payload).map_err(|err| RenderError {
                    message: err.to_string(),
                })?;
            return Ok(render_svg_from_geometry(
                &diagram,
                &options,
                &geometry,
                RoutingMode::PassThroughClip,
            ));
        }

        Ok(render(&diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
        )
    }
}
