//! MMDS diagram instance.

use super::{
    MmdsProfileNegotiation, evaluate_mmds_profiles_for_output, from_mmds_output,
    hydrate_graph_geometry_from_output_with_diagram, parse_mmds_input,
};
use crate::diagram::{OutputFormat, RenderConfig, RenderError, RoutingMode};
use crate::mmds::MmdsOutput;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render, render_svg_from_geometry};

/// MMDS diagram instance.
#[derive(Default)]
pub struct MmdsInstance {
    parsed_payload: Option<MmdsOutput>,
    profile_negotiation: Option<MmdsProfileNegotiation>,
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

    /// Access profile negotiation for the last parsed payload.
    pub fn profile_negotiation(&self) -> Option<&MmdsProfileNegotiation> {
        self.profile_negotiation.as_ref()
    }

    fn supports_format_for_payload(payload: &MmdsOutput, format: OutputFormat) -> bool {
        match payload.geometry_level.as_str() {
            "layout" => matches!(
                format,
                OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
            ),
            "routed" => matches!(format, OutputFormat::Svg | OutputFormat::Json),
            _ => false,
        }
    }
}

impl DiagramInstance for MmdsInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let payload = parse_mmds_input(input)?;
        self.profile_negotiation = Some(evaluate_mmds_profiles_for_output(&payload));
        self.parsed_payload = Some(payload);
        Ok(())
    }

    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError> {
        let payload = self.parsed_payload.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        if !matches!(payload.geometry_level.as_str(), "layout" | "routed") {
            return Err(RenderError {
                message: format!(
                    "MMDS validation error: invalid geometry_level '{}'",
                    payload.geometry_level
                ),
            });
        }

        if !Self::supports_format_for_payload(payload, format) {
            return Err(Self::positioned_text_unsupported_error(format));
        }

        if matches!(format, OutputFormat::Json) {
            let json = serde_json::to_string_pretty(payload).map_err(|err| RenderError {
                message: format!("MMDS serialization error: {err}"),
            })?;
            return Ok(json);
        }

        let diagram = from_mmds_output(payload).map_err(|err| RenderError {
            message: err.to_string(),
        })?;

        let mut options: RenderOptions = config.into();
        options.output_format = format;

        if payload.geometry_level == "routed" {
            let geometry = hydrate_graph_geometry_from_output_with_diagram(payload, &diagram)
                .map_err(|err| RenderError {
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
