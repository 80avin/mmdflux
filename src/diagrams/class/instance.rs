//! Class diagram instance implementation.

use super::compiler;
use super::parser::parse_class_diagram;
use crate::diagram::{
    EdgeRoutingPolicyToggles, EngineId, GeometryLevel, LayoutEngineId, OutputFormat, RenderConfig,
    RenderError,
};
use crate::diagrams::flowchart::engine::layout_with_selected_engine;
use crate::diagrams::flowchart::geometry::{GraphGeometry, RoutedGraphGeometry};
use crate::diagrams::flowchart::routing;
use crate::graph::Diagram;
use crate::mmds::to_mmds_json_typed;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render, render_svg_from_geometry};

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

        // Temporary bridge: map EngineAlgorithmId → LayoutEngineId until Phase 3.
        if let Some(algo_id) = config.layout_engine {
            algo_id.check_available()?;
        }
        let selected_engine = config
            .layout_engine
            .map(|id| match id.engine() {
                EngineId::Flux | EngineId::Mermaid => LayoutEngineId::Dagre,
                EngineId::Elk => LayoutEngineId::Elk,
            })
            .unwrap_or(LayoutEngineId::Dagre);

        let mut options: RenderOptions = config.into();
        options.output_format = format;

        if matches!(format, OutputFormat::Mmds) {
            let engine_result = layout_with_selected_engine(diagram, config, format)?;
            let edge_routing = engine_result.edge_routing;
            let routed = if matches!(config.geometry_level, GeometryLevel::Routed) {
                Some(routing::route_graph_geometry_with_policies(
                    diagram,
                    &engine_result.geometry,
                    edge_routing,
                    EdgeRoutingPolicyToggles,
                ))
            } else {
                None
            };
            return to_mmds_json_typed(
                "class",
                diagram,
                &engine_result.geometry,
                routed.as_ref(),
                config.geometry_level,
                config.path_detail,
            );
        }

        if matches!(format, OutputFormat::Svg) && selected_engine != LayoutEngineId::Dagre {
            let engine_result = layout_with_selected_engine(diagram, config, format)?;
            let edge_routing = engine_result.edge_routing;
            let routed = routing::route_graph_geometry_with_policies(
                diagram,
                &engine_result.geometry,
                edge_routing,
                EdgeRoutingPolicyToggles,
            );
            let geom = inject_routed_paths(&engine_result.geometry, &routed);
            return Ok(render_svg_from_geometry(
                diagram,
                &options,
                &geom,
                edge_routing,
            ));
        }

        if selected_engine != LayoutEngineId::Dagre {
            return Err(RenderError {
                message: format!(
                    "{} engine is currently supported only for svg and json output",
                    selected_engine
                ),
            });
        }

        Ok(render(diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Mmds
        )
    }
}

fn inject_routed_paths(geom: &GraphGeometry, routed: &RoutedGraphGeometry) -> GraphGeometry {
    let mut result = geom.clone();
    for routed_edge in &routed.edges {
        if let Some(layout_edge) = result
            .edges
            .iter_mut()
            .find(|e| e.index == routed_edge.index)
        {
            layout_edge.layout_path_hint = Some(routed_edge.path.clone());
            layout_edge.label_position = routed_edge.label_position;
        }
    }
    result
}
