use mmdflux::dagre::Ranker;
use mmdflux::diagram::{
    EdgeRouting, EdgeStyle, LayoutEngineId, OutputFormat, RenderConfig, RenderError,
};
use mmdflux::registry::default_registry;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WasmRenderConfig {
    #[serde(alias = "layoutEngine")]
    layout_engine: Option<String>,
    #[serde(alias = "clusterRanksep")]
    cluster_ranksep: Option<f64>,
    padding: Option<usize>,
    #[serde(alias = "svgScale")]
    svg_scale: Option<f64>,
    #[serde(alias = "edgeStyle")]
    edge_style: Option<String>,
    #[serde(alias = "svgEdgeRadius")]
    edge_radius: Option<f64>,
    #[serde(alias = "svgDiagramPadding")]
    svg_diagram_padding: Option<f64>,
    #[serde(alias = "svgNodePaddingX")]
    svg_node_padding_x: Option<f64>,
    #[serde(alias = "svgNodePaddingY")]
    svg_node_padding_y: Option<f64>,
    #[serde(alias = "showIds")]
    show_ids: Option<bool>,
    #[serde(alias = "edgeRouting")]
    edge_routing: Option<String>,
    #[serde(alias = "geometryLevel")]
    geometry_level: Option<String>,
    #[serde(alias = "pathDetail")]
    path_detail: Option<String>,
    layout: Option<WasmLayoutConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WasmLayoutConfig {
    #[serde(alias = "nodeSep", alias = "nodeSpacing")]
    node_sep: Option<f64>,
    #[serde(alias = "edgeSep", alias = "edgeSpacing")]
    edge_sep: Option<f64>,
    #[serde(alias = "rankSep", alias = "rankSpacing")]
    rank_sep: Option<f64>,
    margin: Option<f64>,
    ranker: Option<String>,
}

#[wasm_bindgen]
pub fn render(input: &str, format: &str, config_json: &str) -> Result<String, JsError> {
    let format = parse_output_format(format)?;
    let config = parse_render_config(format, config_json)?;
    let registry = default_registry();

    let diagram_id = registry
        .detect(input)
        .ok_or_else(|| js_error("unknown diagram type"))?;
    let mut instance = registry
        .create(diagram_id)
        .ok_or_else(|| js_error(format!("no implementation for diagram type: {diagram_id}")))?;

    instance
        .parse(input)
        .map_err(|error| js_error(format!("parse error: {error}")))?;

    if !instance.supports_format(format) {
        return Err(js_error(format!(
            "{diagram_id} diagrams do not support {format} output"
        )));
    }

    instance
        .render(format, &config)
        .map_err(|error| js_error(format!("render error: {error}")))
}

#[wasm_bindgen]
pub fn detect(input: &str) -> Option<String> {
    default_registry().detect(input).map(str::to_string)
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn parse_render_config(format: OutputFormat, config_json: &str) -> Result<RenderConfig, JsError> {
    if config_json.trim().is_empty() {
        let mut config = RenderConfig::default();
        apply_wasm_format_defaults(format, &mut config);
        return Ok(config);
    }

    let wasm_config: WasmRenderConfig = serde_json::from_str(config_json)
        .map_err(|error| js_error(format!("invalid config_json: {error}")))?;
    let mut config = wasm_config.into_render_config()?;
    apply_wasm_format_defaults(format, &mut config);
    Ok(config)
}

impl WasmRenderConfig {
    fn into_render_config(self) -> Result<RenderConfig, JsError> {
        let mut config = RenderConfig {
            cluster_ranksep: self.cluster_ranksep,
            padding: self.padding,
            svg_scale: self.svg_scale,
            edge_radius: self.edge_radius,
            svg_diagram_padding: self.svg_diagram_padding,
            svg_node_padding_x: self.svg_node_padding_x,
            svg_node_padding_y: self.svg_node_padding_y,
            ..RenderConfig::default()
        };

        if let Some(layout_engine) = self.layout_engine {
            config.layout_engine =
                Some(LayoutEngineId::parse(&layout_engine).map_err(|err| js_error(err.message))?);
        }
        if let Some(show_ids) = self.show_ids {
            config.show_ids = show_ids;
        }
        if let Some(edge_style) = self.edge_style {
            config.edge_style = Some(parse_edge_style(&edge_style)?);
        }
        if let Some(edge_routing) = self.edge_routing {
            config.edge_routing = Some(parse_edge_routing(&edge_routing)?);
        }
        if let Some(layout) = self.layout {
            if let Some(node_sep) = layout.node_sep {
                config.layout.node_sep = node_sep;
            }
            if let Some(edge_sep) = layout.edge_sep {
                config.layout.edge_sep = edge_sep;
            }
            if let Some(rank_sep) = layout.rank_sep {
                config.layout.rank_sep = rank_sep;
            }
            if let Some(margin) = layout.margin {
                config.layout.margin = margin;
            }
            if let Some(ranker) = layout.ranker {
                config.layout.ranker = parse_ranker(&ranker)?;
            }
        }

        Ok(config)
    }
}

fn parse_output_format(value: &str) -> Result<OutputFormat, JsError> {
    parse_via_render_error(value)
}

fn parse_edge_routing(value: &str) -> Result<EdgeRouting, JsError> {
    match normalized(value).as_str() {
        "full-compute" | "fullcompute" => Ok(EdgeRouting::FullCompute),
        "pass-through-clip" | "passthroughclip" => Ok(EdgeRouting::PassThroughClip),
        "unified-preview" | "unifiedpreview" => Ok(EdgeRouting::UnifiedPreview),
        _ => Err(js_error(format!(
            "unknown edge routing: {value:?} (expected one of: full-compute, pass-through-clip, unified-preview)"
        ))),
    }
}

fn parse_edge_style(value: &str) -> Result<EdgeStyle, JsError> {
    EdgeStyle::parse(value).map_err(|err| js_error(err.message))
}

fn parse_ranker(value: &str) -> Result<Ranker, JsError> {
    match normalized(value).as_str() {
        "network-simplex" | "networksimplex" => Ok(Ranker::NetworkSimplex),
        "longest-path" | "longestpath" => Ok(Ranker::LongestPath),
        _ => Err(js_error(format!("unknown ranker: {value}"))),
    }
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn parse_via_render_error<T>(value: &str) -> Result<T, JsError>
where
    T: std::str::FromStr<Err = RenderError>,
{
    value
        .parse::<T>()
        .map_err(|err| js_error(err.message.to_string()))
}

fn js_error(message: impl Into<String>) -> JsError {
    JsError::new(&message.into())
}

fn apply_wasm_format_defaults(format: OutputFormat, config: &mut RenderConfig) {
    if matches!(format, OutputFormat::Svg)
        && config.edge_routing.is_none()
        && config.layout_engine.is_none()
    {
        config.edge_routing = Some(EdgeRouting::UnifiedPreview);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_export_signatures_are_stable() {
        let _render: fn(&str, &str, &str) -> Result<String, JsError> = render;
        let _detect: fn(&str) -> Option<String> = detect;
        let _version: fn() -> String = version;
    }

    #[test]
    fn render_text_output_contains_nodes() {
        let output = render("graph TD\nA-->B", "text", "{}").expect("render should succeed");
        assert!(output.contains("A"));
        assert!(output.contains("B"));
    }

    #[test]
    fn detect_returns_flowchart_for_graph_input() {
        assert_eq!(detect("graph TD\nA-->B"), Some("flowchart".to_string()));
    }

    #[test]
    fn parse_render_config_defaults_svg_to_unified_preview() {
        let config = parse_render_config(OutputFormat::Svg, "{}")
            .expect("svg config parsing should succeed");
        assert_eq!(config.edge_routing, Some(EdgeRouting::UnifiedPreview));
    }

    #[test]
    fn parse_render_config_keeps_non_svg_without_edge_routing_default() {
        let config = parse_render_config(OutputFormat::Text, "{}")
            .expect("text config parsing should succeed");
        assert_eq!(config.edge_routing, None);
    }

    #[test]
    fn parse_render_config_respects_explicit_edge_routing() {
        let config = parse_render_config(OutputFormat::Svg, r#"{"edgeRouting":"full-compute"}"#)
            .expect("explicit edge routing should parse");
        assert_eq!(config.edge_routing, Some(EdgeRouting::FullCompute));
    }

    #[test]
    fn parse_render_config_does_not_force_default_with_layout_engine_override() {
        let config = parse_render_config(OutputFormat::Svg, r#"{"layoutEngine":"dagre"}"#)
            .expect("layout engine config should parse");
        assert_eq!(config.edge_routing, None);
    }
}
