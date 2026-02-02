//! ASCII rendering for flowchart diagrams.
//!
//! This module converts a [`Diagram`] into ASCII art representation.

mod canvas;
mod chars;
mod edge;
pub mod intersect;
mod layout;
mod router;
mod shape;
mod subgraph;

pub use canvas::Canvas;
pub use chars::CharSet;
pub use edge::{render_all_edges, render_all_edges_with_labels, render_edge};
pub use layout::{Layout, LayoutConfig, SubgraphBounds, compute_layout_direct};
pub use router::{Point, RoutedEdge, Segment, route_all_edges, route_edge};
pub use shape::{NodeBounds, node_dimensions, render_node};

use crate::graph::{Diagram, Direction};

/// Render options for ASCII output.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Use ASCII-only characters instead of Unicode box-drawing.
    pub ascii_only: bool,
    /// Ranking algorithm override. None uses the default (NetworkSimplex).
    pub ranker: Option<crate::dagre::types::Ranker>,
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

    // Step 1: Compute layout with direction-aware spacing
    let mut config = layout_config_for_diagram(diagram);
    config.ranker = options.ranker;
    let layout = compute_layout_direct(diagram, &config);

    // Step 2: Create canvas
    let mut canvas = Canvas::new(layout.width, layout.height);

    // Step 2.5: Render subgraph borders FIRST (z-order: background)
    if !layout.subgraph_bounds.is_empty() {
        subgraph::render_subgraph_borders(&mut canvas, &layout.subgraph_bounds, &charset);
    }

    // Step 3: Render nodes
    let mut node_keys: Vec<&String> = diagram.nodes.keys().collect();
    node_keys.sort();
    for node_id in node_keys {
        let node = &diagram.nodes[node_id];
        if let Some(&(x, y)) = layout.draw_positions.get(node_id) {
            render_node(&mut canvas, node, x, y, &charset);
        }
    }

    // Step 4: Route and render edges
    let routed_edges = route_all_edges(&diagram.edges, &layout, diagram.direction);
    render_all_edges_with_labels(
        &mut canvas,
        &routed_edges,
        &charset,
        diagram.direction,
        &layout.edge_label_positions,
    );

    // Step 5: Convert canvas to string
    canvas.to_string()
}

/// Compute layout configuration appropriate for the diagram.
///
/// For LR/RL layouts, we need more horizontal spacing to accommodate edge labels.
fn layout_config_for_diagram(diagram: &Diagram) -> LayoutConfig {
    let mut config = LayoutConfig::default();

    // Check if any edges have labels
    let max_label_len = diagram
        .edges
        .iter()
        .filter_map(|e| e.label.as_ref())
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);

    match diagram.direction {
        Direction::LeftRight | Direction::RightLeft => {
            // For horizontal layouts, increase h_spacing to fit labels
            // The edge attachment points are 1 cell inside the gap on each side,
            // so available space for label = h_spacing - 2
            // We need: label_len + 2 (1 space before, 1 space after arrow)
            // Therefore: h_spacing = label_len + 4
            config.h_spacing = config.h_spacing.max(max_label_len + 4);
        }
        Direction::TopDown | Direction::BottomTop => {
            // For vertical layouts, increase v_spacing to fit labels
            if max_label_len > 0 {
                // Check if any source node has multiple labeled edges (branching)
                // These need extra space so labels don't overlap
                let (has_branching, left_len, right_len) = branching_label_info(diagram);
                if has_branching {
                    // Branching edges need more vertical space:
                    // - 1 row for edge chars leaving source
                    // - 1 row for horizontal spread
                    // - 1 row for labels
                    // - 1 row for arrows/entry
                    config.v_spacing = config.v_spacing.max(5);
                    // Also need more horizontal space for labels on each branch
                    let max_branching_len = left_len.max(right_len);
                    config.h_spacing = config.h_spacing.max(max_branching_len + 4);
                    // Asymmetric margins: only add margin where the label extends
                    config.left_label_margin = left_len;
                    config.right_label_margin = right_len;
                } else {
                    config.v_spacing = config.v_spacing.max(3);
                }
            }
        }
    }

    // Increase padding for nested subgraphs so outer borders have room
    if diagram.has_subgraphs() {
        let max_depth = diagram
            .subgraphs
            .keys()
            .map(|id| diagram.subgraph_depth(id))
            .max()
            .unwrap_or(0);
        if max_depth > 0 {
            // Each nesting level needs border_padding (2) extra chars
            config.padding += max_depth * 2;
        }
    }

    config
}

/// Check if the diagram has branching edges with labels and return margin info.
///
/// Returns (has_branching, left_label_len, right_label_len) where:
/// - has_branching is true if any source node has multiple outgoing edges with labels
/// - left_label_len is the max label length for left branches (first target in declaration order)
/// - right_label_len is the max label length for right branches (subsequent targets)
fn branching_label_info(diagram: &Diagram) -> (bool, usize, usize) {
    use std::collections::HashMap;

    // Group labeled edges by source node, preserving declaration order
    let mut labeled_edges_per_source: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &diagram.edges {
        if let Some(ref label) = edge.label {
            labeled_edges_per_source
                .entry(&edge.from)
                .or_default()
                .push(label);
        }
    }

    // Find sources with 2+ labeled edges
    // First label goes left, rest go right (based on typical layout ordering)
    let mut has_branching = false;
    let mut max_left_len = 0;
    let mut max_right_len = 0;
    for labels in labeled_edges_per_source.values() {
        if labels.len() >= 2 {
            has_branching = true;
            // First declared target typically ends up on the left
            max_left_len = max_left_len.max(labels[0].chars().count());
            // Remaining targets go to the right
            for label in &labels[1..] {
                max_right_len = max_right_len.max(label.chars().count());
            }
        }
    }

    (has_branching, max_left_len, max_right_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_diagram;
    use crate::parser::parse_flowchart;

    #[test]
    fn test_render_with_subgraph_produces_borders() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &RenderOptions::default());

        // Output should contain border characters
        assert!(
            output.contains('┌') || output.contains('+'),
            "output should contain top-left corner: {output}"
        );
        assert!(
            output.contains('┘') || output.contains('+'),
            "output should contain bottom-right corner: {output}"
        );
        // Output should contain the title (embedded in border)
        assert!(
            output.contains("Group"),
            "output should contain title: {output}"
        );
    }

    #[test]
    fn test_render_simple_diagram_unchanged() {
        let input = "graph TD\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &RenderOptions::default());

        // Should not contain subgraph border artifacts (no ┌ corners
        // that aren't part of node shapes)
        // Simple check: output should contain nodes and edges
        assert!(
            output.contains('A'),
            "output should contain node A: {output}"
        );
        assert!(
            output.contains('B'),
            "output should contain node B: {output}"
        );
    }
}
