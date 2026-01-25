//! ASCII rendering for flowchart diagrams.
//!
//! This module converts a [`Diagram`] into ASCII art representation.

mod canvas;
mod chars;
mod edge;
mod layout;
mod router;
mod shape;

pub use canvas::Canvas;
pub use chars::CharSet;
pub use edge::{render_all_edges, render_edge};
pub use layout::{Layout, LayoutConfig, compute_layout};
pub use router::{Point, RoutedEdge, Segment, route_all_edges, route_edge};
pub use shape::{NodeBounds, node_dimensions, render_node};

use crate::graph::Diagram;

/// Render options for ASCII output.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Use ASCII-only characters instead of Unicode box-drawing.
    pub ascii_only: bool,
}

/// Render a diagram to ASCII art.
///
/// # Example
///
/// ```
/// use mmdflux::{parse_flowchart, build_diagram};
/// use mmdflux::render::{render, RenderOptions};
///
/// let input = "graph TD\nA[Start] --> B[End]\n";
/// let flowchart = parse_flowchart(input).unwrap();
/// let diagram = build_diagram(&flowchart);
/// let ascii = render(&diagram, &RenderOptions::default());
/// ```
pub fn render(diagram: &Diagram, options: &RenderOptions) -> String {
    let charset = if options.ascii_only {
        CharSet::ascii()
    } else {
        CharSet::unicode()
    };

    // Step 1: Compute layout
    let config = LayoutConfig::default();
    let layout = compute_layout(diagram, &config);

    // Step 2: Create canvas
    let mut canvas = Canvas::new(layout.width, layout.height);

    // Step 3: Render nodes
    for (node_id, node) in &diagram.nodes {
        if let Some(&(x, y)) = layout.draw_positions.get(node_id) {
            render_node(&mut canvas, node, x, y, &charset);
        }
    }

    // Step 4: Route and render edges
    let routed_edges = route_all_edges(&diagram.edges, &layout, diagram.direction);
    render_all_edges(&mut canvas, &routed_edges, &charset, diagram.direction);

    // Step 5: Convert canvas to string
    canvas.to_string()
}
