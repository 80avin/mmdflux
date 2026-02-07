//! Flowchart diagram instance implementation.

use super::routing;
use crate::diagram::{LayoutEngineId, OutputFormat, RenderConfig, RenderError};
use crate::graph::{Diagram, build_diagram};
use crate::json::to_json;
use crate::parser::parse_flowchart;
use crate::registry::DiagramInstance;
use crate::render::{
    RenderOptions, compute_layout_direct, layout_config_for_diagram, render,
    render_svg_from_geometry,
};

/// Flowchart diagram instance.
///
/// Wraps the existing flowchart parsing and rendering logic behind
/// the `DiagramInstance` trait.
pub struct FlowchartInstance {
    /// Built diagram model.
    diagram: Option<Diagram>,
}

impl FlowchartInstance {
    /// Create a new flowchart instance.
    pub fn new() -> Self {
        Self { diagram: None }
    }
}

impl Default for FlowchartInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramInstance for FlowchartInstance {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let flowchart = parse_flowchart(input)?;
        self.diagram = Some(build_diagram(&flowchart));
        Ok(())
    }

    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError> {
        let diagram = self.diagram.as_ref().ok_or_else(|| RenderError {
            message: "No diagram parsed. Call parse() first.".to_string(),
        })?;

        let annotated;
        let diagram = if config.show_ids {
            annotated = annotate_node_ids(diagram);
            &annotated
        } else {
            diagram
        };

        // Route runtime selection through the engine abstraction.
        let engine_result = super::engine::layout_with_selected_engine(diagram, config)?;

        // Produce routed geometry through the routing stage (Layer-2 contract).
        let routed = routing::route_graph_geometry(
            diagram,
            &engine_result.geometry,
            engine_result.routing_mode,
        );

        let mut options: RenderOptions = config.into();
        options.output_format = format;

        if matches!(format, OutputFormat::Json) {
            if engine_result.engine_id != LayoutEngineId::Dagre {
                return Err(RenderError {
                    message: format!(
                        "{} engine is currently supported only for svg output",
                        engine_result.engine_id
                    ),
                });
            }
            let layout_config = layout_config_for_diagram(diagram, &options);
            let layout = compute_layout_direct(diagram, &layout_config);
            return Ok(to_json(diagram, Some(&layout)));
        }

        if matches!(format, OutputFormat::Svg) && engine_result.engine_id != LayoutEngineId::Dagre {
            // Non-dagre SVG: inject routed paths into geometry for rendering.
            let geom = inject_routed_paths(&engine_result.geometry, &routed);
            return Ok(render_svg_from_geometry(
                diagram,
                &options,
                &geom,
                engine_result.routing_mode,
            ));
        }

        if engine_result.engine_id != LayoutEngineId::Dagre {
            return Err(RenderError {
                message: format!(
                    "{} engine is currently supported only for svg output",
                    engine_result.engine_id
                ),
            });
        }

        Ok(render(diagram, &options))
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Json
        )
    }
}

/// Inject routed edge paths from `RoutedGraphGeometry` into `GraphGeometry`.
///
/// Ensures the rendering pipeline uses paths produced by the routing stage.
fn inject_routed_paths(
    geom: &super::geometry::GraphGeometry,
    routed: &super::geometry::RoutedGraphGeometry,
) -> super::geometry::GraphGeometry {
    let mut result = geom.clone();
    for routed_edge in &routed.edges {
        if let Some(layout_edge) = result
            .edges
            .iter_mut()
            .find(|e| e.index == routed_edge.index)
        {
            layout_edge.layout_path_hint = Some(routed_edge.path.clone());
        }
    }
    result
}

/// Create a copy of the diagram with node labels annotated as "ID: Label".
/// Skips nodes where label == id (bare nodes).
fn annotate_node_ids(diagram: &Diagram) -> Diagram {
    let mut annotated = diagram.clone();
    for node in annotated.nodes.values_mut() {
        if node.label != node.id {
            node.label = format!("{}: {}", node.id, node.label);
        }
    }
    annotated
}
