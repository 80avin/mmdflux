//! Dagre layout engine adapter.
//!
//! Implements `GraphLayoutEngine` for dagre, providing the default
//! layout engine for flowchart diagrams.

use super::geometry::{self, GraphGeometry};
use super::render::layout::build_dagre_layout;
use crate::diagram::{
    EngineCapabilities, EngineConfig, GraphLayoutEngine, RenderConfig, RenderError,
};
use crate::graph::Diagram;

/// Dagre (Sugiyama) layout engine.
///
/// Wraps the existing dagre layout pipeline behind the `GraphLayoutEngine` trait.
/// Node dimensions and edge label dimensions are provided via callbacks at construction.
pub struct DagreLayoutEngine;

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
        let result = build_dagre_layout(
            diagram,
            &layout_config,
            |node| {
                let (w, h) = crate::render::node_dimensions(node, direction);
                (w as f64, h as f64)
            },
            |edge| {
                edge.label
                    .as_ref()
                    .map(|label| (label.len() as f64 + 2.0, 1.0))
            },
        );

        Ok(geometry::from_dagre_layout(&result, diagram))
    }
}

/// Result of engine selection: geometry output + routing mode.
///
/// Fields are read in tests and will be consumed by the rendering pipeline
/// once full engine integration is complete.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct EngineLayoutResult {
    pub geometry: GraphGeometry,
    pub routing_mode: crate::diagram::RoutingMode,
}

/// Resolve the configured flowchart layout engine and execute it.
///
/// Uses the `GraphEngineRegistry` for engine lookup. Dagre is the default
/// when no engine is specified. Returns the layout geometry along with the
/// routing mode determined by engine capabilities.
pub(crate) fn layout_with_selected_engine(
    diagram: &Diagram,
    config: &RenderConfig,
) -> Result<EngineLayoutResult, RenderError> {
    use crate::diagram::{LayoutEngineId, RoutingMode};
    use crate::engines::graph::GraphEngineRegistry;

    let engine_id = match config.layout_engine.as_deref() {
        None | Some("") => LayoutEngineId::Dagre,
        Some(s) => LayoutEngineId::parse(s)?,
    };

    engine_id.check_available()?;

    let registry = GraphEngineRegistry::default();
    let engine = registry.get(engine_id).ok_or_else(|| RenderError {
        message: format!("no adapter registered for engine: {engine_id}"),
    })?;

    let routing_mode = RoutingMode::for_capabilities(&engine.capabilities());

    let engine_config = EngineConfig::Dagre(config.layout.clone());
    let geometry = engine.layout(diagram, &engine_config)?;

    Ok(EngineLayoutResult {
        geometry,
        routing_mode,
    })
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
    use crate::diagram::{GraphLayoutEngine, RenderConfig};

    #[test]
    fn dagre_engine_name() {
        let engine = DagreLayoutEngine;
        assert_eq!(engine.name(), "dagre");
    }

    #[test]
    fn dagre_engine_capabilities() {
        let engine = DagreLayoutEngine;
        let caps = engine.capabilities();
        assert!(!caps.routes_edges);
        assert!(caps.supports_subgraphs);
        assert!(!caps.supports_direction_overrides);
    }

    #[test]
    fn dagre_engine_layout_simple_graph() {
        let engine = DagreLayoutEngine;

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
        let engine = DagreLayoutEngine;

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
    fn dagre_engine_is_object_safe() {
        let engine: Box<dyn GraphLayoutEngine<Input = Diagram, Output = GraphGeometry>> =
            Box::new(DagreLayoutEngine);
        assert_eq!(engine.name(), "dagre");
    }

    #[test]
    fn selected_engine_defaults_to_dagre() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);

        let result = layout_with_selected_engine(&diagram, &RenderConfig::default()).unwrap();
        assert_eq!(result.geometry.nodes.len(), 2);
        assert_eq!(
            result.routing_mode,
            crate::diagram::RoutingMode::FullCompute
        );
    }

    #[test]
    fn selected_engine_accepts_explicit_dagre() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some("dagre".to_string()),
            ..RenderConfig::default()
        };

        let result = layout_with_selected_engine(&diagram, &config).unwrap();
        assert_eq!(result.geometry.edges.len(), 1);
        assert_eq!(
            result.routing_mode,
            crate::diagram::RoutingMode::FullCompute
        );
    }

    #[cfg(not(feature = "engine-elk"))]
    #[test]
    fn selected_engine_rejects_unavailable_engine() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some("elk".to_string()),
            ..RenderConfig::default()
        };

        let err = layout_with_selected_engine(&diagram, &config).unwrap_err();
        assert!(
            err.message.contains("engine-elk") || err.message.contains("not available"),
            "error should be actionable: {}",
            err.message
        );
    }

    #[test]
    fn selected_engine_rejects_unknown_engine() {
        let input = "graph TD\nA-->B";
        let flowchart = crate::parser::parse_flowchart(input).unwrap();
        let diagram = crate::graph::build_diagram(&flowchart);
        let config = RenderConfig {
            layout_engine: Some("nonexistent".to_string()),
            ..RenderConfig::default()
        };

        let err = layout_with_selected_engine(&diagram, &config).unwrap_err();
        assert!(
            err.message.contains("unknown layout engine"),
            "error should mention unknown: {}",
            err.message
        );
    }
}
