//! Dagre layout engine adapter.
//!
//! Implements `GraphLayoutEngine` for dagre, providing the default
//! layout engine for flowchart diagrams.

use super::geometry::{self, GraphGeometry};
use super::render::layout::build_dagre_layout;
use crate::diagram::{EngineCapabilities, EngineConfig, GraphLayoutEngine, RenderError};
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
    use crate::diagram::GraphLayoutEngine;

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
}
