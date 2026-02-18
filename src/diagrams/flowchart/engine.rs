//! Dagre layout engine adapter.
//!
//! Implements `GraphLayoutEngine` for dagre, providing the default
//! layout engine for flowchart diagrams.

use super::geometry::{self, GraphGeometry};
use super::render::layout::build_dagre_layout;
use super::render::svg::svg_node_dimensions;
use super::render::svg_metrics::SvgTextMetrics;
use crate::diagram::{
    AlgorithmId, EngineAlgorithmCapabilities, EngineAlgorithmId, EngineCapabilities, EngineConfig,
    EngineId, GeometryLevel, GraphEngine, GraphLayoutEngine, GraphSolveRequest, GraphSolveResult,
    LayoutEngineId, OutputFormat, RenderConfig, RenderError, RouteOwnership,
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

/// Dagre (Sugiyama) layout engine.
///
/// Wraps the existing dagre layout pipeline behind the `GraphLayoutEngine` trait.
/// Measurement mode determines whether node/edge dimensions are computed in
/// text-grid characters or SVG pixels.
pub struct DagreLayoutEngine {
    mode: MeasurementMode,
}

impl DagreLayoutEngine {
    /// Create a dagre engine with text-grid measurement (default for text output).
    pub fn text() -> Self {
        Self {
            mode: MeasurementMode::Text,
        }
    }

    /// Create a dagre engine with the specified measurement mode.
    pub fn with_mode(mode: MeasurementMode) -> Self {
        Self { mode }
    }
}

impl GraphLayoutEngine for DagreLayoutEngine {
    type Input = Diagram;
    type Output = GraphGeometry;

    fn name(&self) -> &str {
        "dagre"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            routes_edges: false,
            supports_subgraphs: true,
            supports_direction_overrides: false,
        }
    }

    fn layout(
        &self,
        diagram: &Self::Input,
        config: &EngineConfig,
    ) -> Result<Self::Output, RenderError> {
        let EngineConfig::Dagre(dagre_cfg) = config;

        // Build a flowchart LayoutConfig from the dagre config.
        let layout_config = layout_config_from_dagre(dagre_cfg, diagram);

        let direction = diagram.direction;
        let result = match &self.mode {
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
}

/// Run dagre layout with a given measurement mode.
///
/// Shared by `FluxLayeredEngine` and `MermaidLayeredEngine` — both use
/// the same dagre kernel; only routing behavior differs.
fn run_dagre_layout(
    mode: &MeasurementMode,
    diagram: &Diagram,
    config: &EngineConfig,
) -> Result<GraphGeometry, RenderError> {
    let dagre = DagreLayoutEngine::with_mode(mode.clone());
    dagre.layout(diagram, config)
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

        let geometry = run_dagre_layout(&self.mode, diagram, config)?;

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

        let geometry = run_dagre_layout(&self.mode, diagram, config)?;

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

/// Result of engine selection: geometry output + edge routing.
///
/// Fields are read in tests and will be consumed by the rendering pipeline
/// once full engine integration is complete.
#[derive(Debug)]
pub struct EngineLayoutResult {
    pub engine_id: LayoutEngineId,
    pub geometry: GraphGeometry,
    pub edge_routing: crate::diagram::EdgeRouting,
}

/// Resolve the configured flowchart layout engine and execute it.
///
/// Uses the `GraphEngineRegistry` for engine lookup. Dagre is the default
/// when no engine is specified. The output format determines the measurement
/// mode: MMDS and SVG use pixel dimensions, text uses character dimensions.
pub fn layout_with_selected_engine(
    diagram: &Diagram,
    config: &RenderConfig,
    format: OutputFormat,
) -> Result<EngineLayoutResult, RenderError> {
    use crate::diagram::{EdgeRouting, RouteOwnership};

    // Temporary bridge: map EngineAlgorithmId → LayoutEngineId until Phase 3.
    let engine_id = config
        .layout_engine
        .map(|id| match id.engine() {
            EngineId::Flux | EngineId::Mermaid => LayoutEngineId::Dagre,
            EngineId::Elk => LayoutEngineId::Elk,
        })
        .unwrap_or(LayoutEngineId::Dagre);

    if let Some(algo_id) = config.layout_engine {
        algo_id.check_available()?;
    } else {
        engine_id.check_available()?;
    }

    // Derive routing from EngineAlgorithmId capabilities, not the underlying adapter.
    // Default (None) behaves as flux-layered (Native → UnifiedPreview).
    let edge_routing = config
        .layout_engine
        .map(|id| match id.capabilities().route_ownership {
            RouteOwnership::Native => EdgeRouting::UnifiedPreview,
            RouteOwnership::HintDriven => EdgeRouting::FullCompute,
            RouteOwnership::EngineProvided => EdgeRouting::PassThroughClip,
        })
        .unwrap_or(EdgeRouting::UnifiedPreview);

    match engine_id {
        LayoutEngineId::Dagre => {
            let mode = MeasurementMode::for_format(format, config);
            let engine = DagreLayoutEngine::with_mode(mode);
            let engine_config = EngineConfig::Dagre(config.layout.clone());
            let geometry = engine.layout(diagram, &engine_config)?;
            Ok(EngineLayoutResult {
                engine_id,
                geometry,
                edge_routing,
            })
        }
        _ => {
            use crate::engines::graph::GraphEngineRegistry;
            let registry = GraphEngineRegistry::default();
            let engine = registry.get(engine_id).ok_or_else(|| RenderError {
                message: format!("no adapter registered for engine: {engine_id}"),
            })?;
            let engine_config = EngineConfig::Dagre(config.layout.clone());
            let geometry = engine.layout(diagram, &engine_config)?;
            Ok(EngineLayoutResult {
                engine_id,
                geometry,
                edge_routing,
            })
        }
    }
}

/// Build a flowchart LayoutConfig from dagre config parameters.
///
/// This bridges the engine's dagre config back to the flowchart render
/// config that `build_dagre_layout` expects.
fn layout_config_from_dagre(
    dagre_cfg: &crate::dagre::types::LayoutConfig,
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
    use crate::diagram::{EngineAlgorithmId, GraphLayoutEngine, RenderConfig};

    #[test]
    fn dagre_engine_name() {
        let engine = DagreLayoutEngine::text();
        assert_eq!(engine.name(), "dagre");
    }

    #[test]
    fn dagre_engine_capabilities() {
        let engine = DagreLayoutEngine::text();
        let caps = engine.capabilities();
        assert!(!caps.routes_edges);
        assert!(caps.supports_subgraphs);
        assert!(!caps.supports_direction_overrides);
    }

    #[test]
    fn dagre_engine_layout_simple_graph() {
        let engine = DagreLayoutEngine::text();

        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Dagre(crate::dagre::types::LayoutConfig::default());
        let geom = engine.layout(&diagram, &config).unwrap();

        assert_eq!(geom.nodes.len(), 2);
        assert!(geom.nodes.contains_key("A"));
        assert!(geom.nodes.contains_key("B"));
        assert_eq!(geom.edges.len(), 1);
    }

    #[test]
    fn dagre_engine_layout_with_subgraphs() {
        let engine = DagreLayoutEngine::text();

        let input = "graph TD\nsubgraph sg1[Group]\nA-->B\nend\nC-->A";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Dagre(crate::dagre::types::LayoutConfig::default());
        let geom = engine.layout(&diagram, &config).unwrap();

        assert!(geom.nodes.contains_key("A"));
        assert!(geom.nodes.contains_key("B"));
        assert!(geom.nodes.contains_key("C"));
        assert!(!geom.subgraphs.is_empty());
    }

    #[test]
    fn dagre_engine_svg_mode_produces_larger_dimensions() {
        let text_engine = DagreLayoutEngine::text();
        let svg_engine = DagreLayoutEngine::with_mode(MeasurementMode::Svg(SvgTextMetrics::new(
            16.0, 15.0, 15.0,
        )));

        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let config = EngineConfig::Dagre(crate::dagre::types::LayoutConfig::default());
        let text_geom = text_engine.layout(&diagram, &config).unwrap();
        let svg_geom = svg_engine.layout(&diagram, &config).unwrap();

        // SVG dimensions should be significantly larger than text dimensions
        let text_w = text_geom.nodes["A"].rect.width;
        let svg_w = svg_geom.nodes["A"].rect.width;
        assert!(
            svg_w > text_w * 3.0,
            "SVG width ({svg_w}) should be much larger than text width ({text_w})"
        );
    }

    #[test]
    fn dagre_engine_is_object_safe() {
        let engine: Box<dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>> =
            Box::new(DagreLayoutEngine::text());
        assert_eq!(engine.name(), "dagre");
    }

    #[test]
    fn selected_engine_defaults_to_flux_layered_routing() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let result =
            layout_with_selected_engine(&diagram, &RenderConfig::default(), OutputFormat::Text)
                .unwrap();
        assert_eq!(result.geometry.nodes.len(), 2);
        // Default (None) behaves as flux-layered → UnifiedPreview
        assert_eq!(
            result.edge_routing,
            crate::diagram::EdgeRouting::UnifiedPreview
        );
    }

    #[test]
    fn selected_engine_flux_layered_uses_unified_preview() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some(EngineAlgorithmId::parse("flux-layered").unwrap()),
            ..RenderConfig::default()
        };

        let result = layout_with_selected_engine(&diagram, &config, OutputFormat::Text).unwrap();
        assert_eq!(result.geometry.edges.len(), 1);
        assert_eq!(
            result.edge_routing,
            crate::diagram::EdgeRouting::UnifiedPreview
        );
    }

    #[test]
    fn selected_engine_mermaid_layered_uses_full_compute() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some(EngineAlgorithmId::parse("mermaid-layered").unwrap()),
            ..RenderConfig::default()
        };

        let result = layout_with_selected_engine(&diagram, &config, OutputFormat::Text).unwrap();
        assert_eq!(result.geometry.edges.len(), 1);
        assert_eq!(
            result.edge_routing,
            crate::diagram::EdgeRouting::FullCompute
        );
    }

    #[cfg(not(feature = "engine-elk"))]
    #[test]
    fn selected_engine_rejects_unavailable_engine() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some(EngineAlgorithmId::parse("elk-layered").unwrap()),
            ..RenderConfig::default()
        };

        let err = layout_with_selected_engine(&diagram, &config, OutputFormat::Text).unwrap_err();
        assert!(
            err.message.contains("engine-elk") || err.message.contains("not available"),
            "error should be actionable: {}",
            err.message
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
