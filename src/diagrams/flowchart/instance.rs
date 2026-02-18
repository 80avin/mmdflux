//! Flowchart diagram instance implementation.

use crate::diagram::{
    AlgorithmId, EngineAlgorithmId, EngineConfig, EngineId, GraphSolveRequest, OutputFormat,
    RenderConfig, RenderError,
};
use crate::engines::graph::GraphEngineRegistry;
use crate::graph::{Diagram, build_diagram};
use crate::mmds::to_mmds_json;
use crate::parser::parse_flowchart;
use crate::registry::DiagramInstance;
use crate::render::{RenderOptions, render};

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

        // Resolve engine (default: flux-layered).
        let engine_id = config
            .layout_engine
            .unwrap_or_else(|| EngineAlgorithmId::new(EngineId::Flux, AlgorithmId::Layered));
        engine_id.check_available()?;

        let mut options: RenderOptions = config.into();
        options.output_format = format;

        match format {
            OutputFormat::Mmds => {
                // MMDS: use solve() to obtain geometry and optionally routed paths.
                let request = GraphSolveRequest::from_config(config, format);
                let registry = GraphEngineRegistry::default();
                let engine = registry.get_solver(engine_id).ok_or_else(|| RenderError {
                    message: format!("no engine registered for: {engine_id}"),
                })?;
                let result = engine.solve(
                    diagram,
                    &EngineConfig::Dagre(config.layout.clone()),
                    &request,
                )?;
                to_mmds_json(
                    diagram,
                    &result.geometry,
                    result.routed.as_ref(),
                    config.geometry_level,
                    config.path_detail,
                )
            }
            // SVG and Text/Ascii: render() handles layout and routing internally.
            //
            // Text uses character-grid coordinates (integer positions) while the
            // solve result uses float pixel coordinates — fundamentally different
            // coordinate systems. Bridging them requires a float-to-grid mapping
            // layer that is out of scope for the taxonomy refactor (task 4.2 decision).
            //
            // SVG uses render_svg() which includes subgraph post-processing steps
            // (sublayout direction overrides, padding, edge spacing) that cannot be
            // replaced by DagreLayoutEngine::layout() alone (task 4.3 will decouple).
            _ => Ok(render(diagram, &options)),
        }
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(
            format,
            OutputFormat::Text | OutputFormat::Ascii | OutputFormat::Svg | OutputFormat::Mmds
        )
    }
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
