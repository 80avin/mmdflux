//! Dagre layout engine adapter.
//!
//! Provides dagre-based layout via `run_dagre_layout` for text and SVG measurement
//! modes, and implements `GraphEngine` for `FluxLayeredEngine` and `MermaidLayeredEngine`.

use super::geometry::GraphGeometry;
use super::render::layout::build_dagre_layout;
use super::render::svg::svg_node_dimensions;
use super::render::svg_metrics::SvgTextMetrics;
use crate::diagram::{
    AlgorithmId, EngineAlgorithmCapabilities, EngineAlgorithmId, EngineConfig, EngineId,
    GeometryLevel, GraphEngine, GraphSolveRequest, GraphSolveResult, OutputFormat, RenderConfig,
    RenderError, RouteOwnership,
};
use crate::diagrams::flowchart::geometry::RoutedGraphGeometry;
use crate::graph::Diagram;
use crate::render::SvgOptions;

/// Measurement mode controls whether layout uses text-grid character
/// dimensions or SVG pixel dimensions for node sizing.
#[derive(Debug, Clone)]
pub enum MeasurementMode {
    /// Text-grid character dimensions (for text/ascii rendering).
    Text,
    /// SVG pixel dimensions (for MMDS and SVG output).
    Svg(SvgTextMetrics),
}

impl MeasurementMode {
    /// Determine the measurement mode from the output format.
    pub fn for_format(format: OutputFormat, config: &RenderConfig) -> Self {
        match format {
            OutputFormat::Mmds | OutputFormat::Svg => {
                let defaults = SvgOptions::default();
                let font_size = defaults.font_size;
                let node_padding_x = config.svg_node_padding_x.unwrap_or(defaults.node_padding_x);
                let node_padding_y = config.svg_node_padding_y.unwrap_or(defaults.node_padding_y);
                let metrics = SvgTextMetrics::new(font_size, node_padding_x, node_padding_y);
                MeasurementMode::Svg(metrics)
            }
            _ => MeasurementMode::Text,
        }
    }
}

fn text_edge_label_dimensions(label: &str) -> (f64, f64) {
    let lines: Vec<&str> = label.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let height = lines.len().max(1);
    (width as f64 + 2.0, height as f64)
}

/// Run dagre layout with a given measurement mode.
///
/// Shared by `FluxLayeredEngine` and `MermaidLayeredEngine` — both use
/// the same dagre kernel; only routing behavior differs.
pub fn run_dagre_layout(
    mode: &MeasurementMode,
    diagram: &Diagram,
    config: &EngineConfig,
) -> Result<GraphGeometry, RenderError> {
    use crate::diagrams::flowchart::geometry;

    let EngineConfig::Layered(dagre_cfg) = config;
    let layout_config = layout_config_from_dagre(dagre_cfg, diagram);
    let direction = diagram.direction;
    let result = match mode {
        MeasurementMode::Text => build_dagre_layout(
            diagram,
            &layout_config,
            |node| {
                let (w, h) = crate::render::node_dimensions(node, direction);
                (w as f64, h as f64)
            },
            |edge| {
                edge.label
                    .as_ref()
                    .map(|label| text_edge_label_dimensions(label))
            },
        ),
        MeasurementMode::Svg(metrics) => build_dagre_layout(
            diagram,
            &layout_config,
            |node| svg_node_dimensions(metrics, node, direction),
            |edge| {
                edge.label
                    .as_ref()
                    .map(|label| metrics.edge_label_dimensions(label))
            },
        ),
    };
    Ok(geometry::from_dagre_layout(&result, diagram))
}

/// Flux-layered engine: dagre layout + unified routing natively.
///
/// Implements `GraphEngine::solve()` with `RouteOwnership::Native` —
/// layout and routing are performed together inside `solve()`.
pub struct FluxLayeredEngine {
    mode: MeasurementMode,
}

impl FluxLayeredEngine {
    /// Create with text-grid measurement mode.
    pub fn text() -> Self {
        Self {
            mode: MeasurementMode::Text,
        }
    }

    /// Create with the specified measurement mode.
    pub fn with_mode(mode: MeasurementMode) -> Self {
        Self { mode }
    }
}

impl GraphEngine for FluxLayeredEngine {
    fn id(&self) -> EngineAlgorithmId {
        EngineAlgorithmId::new(EngineId::Flux, AlgorithmId::Layered)
    }

    fn capabilities(&self) -> EngineAlgorithmCapabilities {
        EngineAlgorithmCapabilities {
            route_ownership: RouteOwnership::Native,
            supports_subgraphs: true,
        }
    }

    fn solve(
        &self,
        diagram: &Diagram,
        config: &EngineConfig,
        request: &GraphSolveRequest,
    ) -> Result<GraphSolveResult, RenderError> {
        use crate::diagram::EdgeRouting;
        use crate::render::SvgOptions;

        // For SVG/MMDS output, pixel-accurate SVG measurement mode is required.
        // Use self.mode if already SVG (explicit override), else derive from format.
        let mode = match request.output_format {
            OutputFormat::Svg | OutputFormat::Mmds => match &self.mode {
                MeasurementMode::Svg(_) => self.mode.clone(),
                MeasurementMode::Text => {
                    let defaults = SvgOptions::default();
                    let metrics = super::render::svg_metrics::SvgTextMetrics::new(
                        defaults.font_size,
                        defaults.node_padding_x,
                        defaults.node_padding_y,
                    );
                    MeasurementMode::Svg(metrics)
                }
            },
            _ => self.mode.clone(),
        };

        let geometry = run_dagre_layout(&mode, diagram, config)?;

        // Route when routed geometry is requested (Native ownership).
        let routed: Option<RoutedGraphGeometry> =
            if matches!(request.geometry_level, GeometryLevel::Routed) {
                Some(super::routing::route_graph_geometry(
                    diagram,
                    &geometry,
                    EdgeRouting::UnifiedPreview,
                ))
            } else {
                None
            };

        Ok(GraphSolveResult {
            engine_id: self.id(),
            geometry,
            routed,
        })
    }
}

/// Mermaid-layered engine: dagre layout with legacy (FullCompute) routing.
///
/// Implements `GraphEngine::solve()` with `RouteOwnership::HintDriven` —
/// layout uses the same dagre kernel as `FluxLayeredEngine`, but routing
/// uses the legacy `FullCompute` path for Mermaid.js compatibility.
pub struct MermaidLayeredEngine {
    mode: MeasurementMode,
}

impl MermaidLayeredEngine {
    /// Create with text-grid measurement mode.
    pub fn text() -> Self {
        Self {
            mode: MeasurementMode::Text,
        }
    }

    /// Create with the specified measurement mode.
    pub fn with_mode(mode: MeasurementMode) -> Self {
        Self { mode }
    }
}

impl GraphEngine for MermaidLayeredEngine {
    fn id(&self) -> EngineAlgorithmId {
        EngineAlgorithmId::new(EngineId::Mermaid, AlgorithmId::Layered)
    }

    fn capabilities(&self) -> EngineAlgorithmCapabilities {
        EngineAlgorithmCapabilities {
            route_ownership: RouteOwnership::HintDriven,
            supports_subgraphs: true,
        }
    }

    fn solve(
        &self,
        diagram: &Diagram,
        config: &EngineConfig,
        request: &GraphSolveRequest,
    ) -> Result<GraphSolveResult, RenderError> {
        use crate::diagram::EdgeRouting;
        use crate::render::SvgOptions;

        // For SVG/MMDS output, pixel-accurate SVG measurement mode is required.
        // Use self.mode if already SVG (explicit override), else derive from format.
        let mode = match request.output_format {
            OutputFormat::Svg | OutputFormat::Mmds => match &self.mode {
                MeasurementMode::Svg(_) => self.mode.clone(),
                MeasurementMode::Text => {
                    let defaults = SvgOptions::default();
                    let metrics = super::render::svg_metrics::SvgTextMetrics::new(
                        defaults.font_size,
                        defaults.node_padding_x,
                        defaults.node_padding_y,
                    );
                    MeasurementMode::Svg(metrics)
                }
            },
            _ => self.mode.clone(),
        };

        let geometry = run_dagre_layout(&mode, diagram, config)?;

        // HintDriven: route via legacy FullCompute path if routed level requested.
        let routed: Option<RoutedGraphGeometry> =
            if matches!(request.geometry_level, GeometryLevel::Routed) {
                Some(super::routing::route_graph_geometry(
                    diagram,
                    &geometry,
                    EdgeRouting::FullCompute,
                ))
            } else {
                None
            };

        Ok(GraphSolveResult {
            engine_id: self.id(),
            geometry,
            routed,
        })
    }
}

/// Build a flowchart LayoutConfig from dagre config parameters.
///
/// This bridges the engine's dagre config back to the flowchart render
/// config that `build_dagre_layout` expects.
fn layout_config_from_dagre(
    dagre_cfg: &crate::layered::types::LayoutConfig,
    diagram: &Diagram,
) -> crate::diagrams::flowchart::render::layout::LayoutConfig {
    use crate::diagrams::flowchart::render::layout::LayoutConfig as FlowchartLayoutConfig;

    let defaults = FlowchartLayoutConfig::default();
    let extra_padding = if diagram.has_subgraphs() {
        diagram
            .subgraphs
            .keys()
            .map(|id| diagram.subgraph_depth(id))
            .max()
            .unwrap_or(0)
            * 2
    } else {
        0
    };

    FlowchartLayoutConfig {
        dagre_node_sep: dagre_cfg.node_sep,
        dagre_edge_sep: dagre_cfg.edge_sep,
        dagre_rank_sep: dagre_cfg.rank_sep,
        dagre_margin: dagre_cfg.margin,
        ranker: Some(dagre_cfg.ranker),
        padding: defaults.padding + extra_padding,
        ..defaults
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram::EngineAlgorithmId;

    #[test]
    fn run_dagre_layout_simple_graph() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Layered(crate::layered::types::LayoutConfig::default());
        let geom = run_dagre_layout(&MeasurementMode::Text, &diagram, &config).unwrap();

        assert_eq!(geom.nodes.len(), 2);
        assert!(geom.nodes.contains_key("A"));
        assert!(geom.nodes.contains_key("B"));
        assert_eq!(geom.edges.len(), 1);
    }

    #[test]
    fn run_dagre_layout_with_subgraphs() {
        let input = "graph TD\nsubgraph sg1[Group]\nA-->B\nend\nC-->A";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Layered(crate::layered::types::LayoutConfig::default());
        let geom = run_dagre_layout(&MeasurementMode::Text, &diagram, &config).unwrap();

        assert!(geom.nodes.contains_key("A"));
        assert!(geom.nodes.contains_key("B"));
        assert!(geom.nodes.contains_key("C"));
        assert!(!geom.subgraphs.is_empty());
    }

    #[test]
    fn run_dagre_layout_svg_mode_produces_larger_dimensions() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Layered(crate::layered::types::LayoutConfig::default());
        let text_geom = run_dagre_layout(&MeasurementMode::Text, &diagram, &config).unwrap();
        let svg_geom = run_dagre_layout(
            &MeasurementMode::Svg(SvgTextMetrics::new(16.0, 15.0, 15.0)),
            &diagram,
            &config,
        )
        .unwrap();

        // SVG dimensions should be significantly larger than text dimensions
        let text_w = text_geom.nodes["A"].rect.width;
        let svg_w = svg_geom.nodes["A"].rect.width;
        assert!(
            svg_w > text_w * 3.0,
            "SVG width ({svg_w}) should be much larger than text width ({text_w})"
        );
    }

    #[test]
    fn selected_engine_rejects_unknown_engine_at_parse_boundary() {
        let err = EngineAlgorithmId::parse("nonexistent").unwrap_err();
        assert!(
            err.message.contains("unknown engine"),
            "error should mention unknown: {}",
            err.message
        );
    }
}
