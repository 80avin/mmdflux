//! Layout computation for flowchart diagrams.
//!
//! This module computes the position of nodes on a grid based on topological ordering.
//! It supports both a built-in algorithm and an optional dagre-based algorithm for
//! better crossing reduction and cycle handling.

use std::collections::{HashMap, HashSet};

use super::shape::{NodeBounds, node_dimensions};
use crate::dagre::{self, Direction as DagreDirection, LayoutConfig as DagreConfig};
use crate::graph::{Diagram, Direction};

/// Grid position of a node (layer/column in abstract grid coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPos {
    /// Layer (row for TD/BT, column for LR/RL).
    pub layer: usize,
    /// Position within layer.
    pub pos: usize,
}

/// Layout result containing node positions and canvas dimensions.
#[derive(Debug)]
pub struct Layout {
    /// Node positions in grid coordinates.
    pub grid_positions: HashMap<String, GridPos>,
    /// Node positions in draw coordinates (x, y pixels/chars).
    pub draw_positions: HashMap<String, (usize, usize)>,
    /// Node bounding boxes in draw coordinates.
    pub node_bounds: HashMap<String, NodeBounds>,
    /// Total canvas width needed.
    pub width: usize,
    /// Total canvas height needed.
    pub height: usize,
    /// Spacing between nodes horizontally.
    pub h_spacing: usize,
    /// Spacing between nodes vertically.
    pub v_spacing: usize,
    /// Number of backward edge corridors needed (for routing cycles).
    pub backward_corridors: usize,
    /// Width of each backward edge corridor.
    pub corridor_width: usize,
    /// Lane assignments for backward edges: (from, to) -> lane number.
    pub backward_edge_lanes: HashMap<(String, String), usize>,
}

impl Layout {
    /// Get the bounding box for a node.
    pub fn get_bounds(&self, node_id: &str) -> Option<&NodeBounds> {
        self.node_bounds.get(node_id)
    }
}

/// Configuration for layout computation.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    /// Horizontal spacing between nodes.
    pub h_spacing: usize,
    /// Vertical spacing between nodes.
    pub v_spacing: usize,
    /// Padding around the entire diagram.
    pub padding: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            h_spacing: 4,
            v_spacing: 3,
            padding: 1,
        }
    }
}

/// Compute the layout for a diagram.
pub fn compute_layout(diagram: &Diagram, config: &LayoutConfig) -> Layout {
    // Step 1: Topological sort to assign layers
    let layers = topological_layers(diagram);

    // Step 2: Compute grid positions (layer + position within layer)
    let grid_positions = compute_grid_positions(&layers);

    // Step 3: Compute node dimensions
    let node_dims: HashMap<String, (usize, usize)> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node_dimensions(node)))
        .collect();

    // Step 4: Compute layer dimensions
    let (layer_widths, layer_heights) = compute_layer_dimensions(&layers, &node_dims);

    // Step 5: Identify backward edges and assign corridor lanes
    let (backward_corridors, backward_edge_lanes) =
        assign_backward_edge_lanes(diagram, &grid_positions);
    let corridor_width = 3; // Space for edge line + padding

    // Step 6: Convert grid positions to draw coordinates based on direction
    let (draw_positions, node_bounds, mut width, mut height) = match diagram.direction {
        Direction::TopDown | Direction::BottomTop => grid_to_draw_vertical(
            &grid_positions,
            &node_dims,
            &layers,
            &layer_heights,
            config,
            diagram.direction == Direction::BottomTop,
        ),
        Direction::LeftRight | Direction::RightLeft => grid_to_draw_horizontal(
            &grid_positions,
            &node_dims,
            &layers,
            &layer_widths,
            config,
            diagram.direction == Direction::RightLeft,
        ),
    };

    // Step 7: Expand canvas for backward edge corridors
    if backward_corridors > 0 {
        let corridor_space = backward_corridors * corridor_width;
        match diagram.direction {
            Direction::TopDown | Direction::BottomTop => {
                // Add space on the right side for vertical layouts
                width += corridor_space;
            }
            Direction::LeftRight | Direction::RightLeft => {
                // Add space on the bottom for horizontal layouts
                height += corridor_space;
            }
        }
    }

    Layout {
        grid_positions,
        draw_positions,
        node_bounds,
        width,
        height,
        h_spacing: config.h_spacing,
        v_spacing: config.v_spacing,
        backward_corridors,
        corridor_width,
        backward_edge_lanes,
    }
}

/// Compute the layout using the dagre algorithm.
///
/// This uses the Sugiyama framework with:
/// - Greedy feedback arc set for cycle removal
/// - Longest-path ranking
/// - Barycenter heuristic for crossing reduction
///
/// The algorithm phases come from dagre, but coordinate assignment uses
/// the original ASCII-friendly logic for proper character grid alignment.
pub fn compute_layout_dagre(diagram: &Diagram, config: &LayoutConfig) -> Layout {
    // Convert diagram to dagre graph
    let mut dgraph = dagre::DiGraph::new();

    // Add nodes with dimensions
    for (id, node) in &diagram.nodes {
        let dims = node_dimensions(node);
        dgraph.add_node(id.as_str(), dims);
    }

    // Add edges
    for edge in &diagram.edges {
        dgraph.add_edge(edge.from.as_str(), edge.to.as_str());
    }

    // Convert direction
    let dagre_direction = match diagram.direction {
        Direction::TopDown => DagreDirection::TopBottom,
        Direction::BottomTop => DagreDirection::BottomTop,
        Direction::LeftRight => DagreDirection::LeftRight,
        Direction::RightLeft => DagreDirection::RightLeft,
    };

    // Run dagre layout with larger spacing to clearly separate layers/positions
    let dagre_config = DagreConfig {
        direction: dagre_direction,
        node_sep: 50.0, // Large value to clearly distinguish positions
        rank_sep: 50.0, // Large value to clearly distinguish layers
        margin: 10.0,
        acyclic: true,
    };

    let result = dagre::layout(&dgraph, &dagre_config, |_, dims| {
        (dims.0 as f64, dims.1 as f64)
    });

    // Group nodes by their y-coordinate (for TD/BT) or x-coordinate (for LR/RL) to determine layers
    let is_vertical = matches!(diagram.direction, Direction::TopDown | Direction::BottomTop);

    // Build list of (node_id, primary_coord, secondary_coord) for grouping
    let mut layer_coords: Vec<(String, f64, f64)> = result
        .nodes
        .iter()
        .map(|(id, rect)| {
            let primary = if is_vertical { rect.y } else { rect.x };
            let secondary = if is_vertical { rect.x } else { rect.y };
            (id.0.clone(), primary, secondary)
        })
        .collect();

    // Sort by primary coordinate to group into layers
    layer_coords.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Group into layers by similar primary coordinate (within rank_sep/2 tolerance)
    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut current_layer: Vec<String> = Vec::new();
    let mut last_primary: Option<f64> = None;

    for (id, primary, _secondary) in &layer_coords {
        if let Some(last) = last_primary {
            // New layer if primary coordinate differs significantly
            if (*primary - last).abs() > 25.0 && !current_layer.is_empty() {
                layers.push(std::mem::take(&mut current_layer));
            }
        }
        current_layer.push(id.clone());
        last_primary = Some(*primary);
    }
    if !current_layer.is_empty() {
        layers.push(current_layer);
    }

    // Sort nodes within each layer by secondary coordinate (dagre's crossing-reduced order)
    for layer in &mut layers {
        layer.sort_by(|a, b| {
            let a_rect = result.nodes.get(&dagre::NodeId(a.clone()));
            let b_rect = result.nodes.get(&dagre::NodeId(b.clone()));
            let a_sec = a_rect
                .map(|r| if is_vertical { r.x } else { r.y })
                .unwrap_or(0.0);
            let b_sec = b_rect
                .map(|r| if is_vertical { r.x } else { r.y })
                .unwrap_or(0.0);
            a_sec
                .partial_cmp(&b_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Now use the original ASCII coordinate assignment with these layers
    let grid_positions = compute_grid_positions(&layers);

    // Compute node dimensions
    let node_dims: HashMap<String, (usize, usize)> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node_dimensions(node)))
        .collect();

    // Compute layer dimensions
    let (layer_widths, layer_heights) = compute_layer_dimensions(&layers, &node_dims);

    // Identify backward edges from dagre's reversed_edges
    let backward_edge_lanes: HashMap<(String, String), usize> = result
        .reversed_edges
        .iter()
        .enumerate()
        .filter_map(|(lane, &edge_idx)| {
            diagram
                .edges
                .get(edge_idx)
                .map(|edge| ((edge.from.clone(), edge.to.clone()), lane))
        })
        .collect();

    let backward_corridors = backward_edge_lanes.len();
    let corridor_width = 3;

    // Convert grid positions to draw coordinates using original ASCII logic
    let (draw_positions, node_bounds, mut width, mut height) = match diagram.direction {
        Direction::TopDown | Direction::BottomTop => grid_to_draw_vertical(
            &grid_positions,
            &node_dims,
            &layers,
            &layer_heights,
            config,
            diagram.direction == Direction::BottomTop,
        ),
        Direction::LeftRight | Direction::RightLeft => grid_to_draw_horizontal(
            &grid_positions,
            &node_dims,
            &layers,
            &layer_widths,
            config,
            diagram.direction == Direction::RightLeft,
        ),
    };

    // Expand canvas for backward edge corridors
    if backward_corridors > 0 {
        let corridor_space = backward_corridors * corridor_width;
        match diagram.direction {
            Direction::TopDown | Direction::BottomTop => width += corridor_space,
            Direction::LeftRight | Direction::RightLeft => height += corridor_space,
        }
    }

    Layout {
        grid_positions,
        draw_positions,
        node_bounds,
        width,
        height,
        h_spacing: config.h_spacing,
        v_spacing: config.v_spacing,
        backward_corridors,
        corridor_width,
        backward_edge_lanes,
    }
}

/// Identify backward edges and assign each to a corridor lane.
///
/// Returns (count, lane_assignments) where lane_assignments maps (from, to) to lane number.
/// Lanes are assigned in a deterministic order based on edge source/target positions.
fn assign_backward_edge_lanes(
    diagram: &Diagram,
    grid_positions: &HashMap<String, GridPos>,
) -> (usize, HashMap<(String, String), usize>) {
    let mut backward_edges: Vec<_> = diagram
        .edges
        .iter()
        .filter(|edge| {
            if let (Some(from_pos), Some(to_pos)) =
                (grid_positions.get(&edge.from), grid_positions.get(&edge.to))
            {
                // Backward edge: target is in an earlier layer than source
                match diagram.direction {
                    Direction::TopDown | Direction::LeftRight => to_pos.layer < from_pos.layer,
                    Direction::BottomTop | Direction::RightLeft => to_pos.layer > from_pos.layer,
                }
            } else {
                false
            }
        })
        .collect();

    // Sort backward edges for deterministic lane assignment
    // Sort by source layer (descending), then by target layer, then by edge names
    backward_edges.sort_by(|a, b| {
        let a_from = grid_positions.get(&a.from).map(|p| p.layer).unwrap_or(0);
        let b_from = grid_positions.get(&b.from).map(|p| p.layer).unwrap_or(0);
        let a_to = grid_positions.get(&a.to).map(|p| p.layer).unwrap_or(0);
        let b_to = grid_positions.get(&b.to).map(|p| p.layer).unwrap_or(0);

        b_from
            .cmp(&a_from) // Edges from later layers get outer lanes
            .then(a_to.cmp(&b_to))
            .then(a.from.cmp(&b.from))
            .then(a.to.cmp(&b.to))
    });

    let mut lane_assignments = HashMap::new();
    for (lane, edge) in backward_edges.iter().enumerate() {
        lane_assignments.insert((edge.from.clone(), edge.to.clone()), lane);
    }

    (backward_edges.len(), lane_assignments)
}

/// Perform topological sort and group nodes into layers.
///
/// Returns a Vec of layers, where each layer is a Vec of node IDs.
/// Nodes with no incoming edges are in layer 0.
fn topological_layers(diagram: &Diagram) -> Vec<Vec<String>> {
    // Build adjacency and in-degree maps
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();

    // Initialize all nodes with 0 in-degree
    for id in diagram.nodes.keys() {
        in_degree.insert(id.as_str(), 0);
        successors.insert(id.as_str(), Vec::new());
    }

    // Build the graph
    for edge in &diagram.edges {
        if diagram.nodes.contains_key(&edge.from) && diagram.nodes.contains_key(&edge.to) {
            *in_degree.get_mut(edge.to.as_str()).unwrap() += 1;
            successors
                .get_mut(edge.from.as_str())
                .unwrap()
                .push(&edge.to);
        }
    }

    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut remaining: HashSet<&str> = diagram.nodes.keys().map(|s| s.as_str()).collect();

    // Process layers until all nodes are assigned
    while !remaining.is_empty() {
        // Find all nodes with in-degree 0 among remaining nodes
        let mut current_layer: Vec<String> = remaining
            .iter()
            .filter(|&&id| in_degree.get(id).copied().unwrap_or(0) == 0)
            .map(|&s| s.to_string())
            .collect();

        // If no nodes have in-degree 0, we have a cycle - break it by picking one
        if current_layer.is_empty() {
            // Pick the node with the smallest in-degree, using node ID as tie-breaker
            // for deterministic output
            let min_node = remaining
                .iter()
                .min_by(|&&a, &&b| {
                    let deg_a = in_degree.get(a).copied().unwrap_or(0);
                    let deg_b = in_degree.get(b).copied().unwrap_or(0);
                    deg_a.cmp(&deg_b).then_with(|| a.cmp(b))
                })
                .unwrap();
            current_layer.push(min_node.to_string());
        }

        // Sort layer for deterministic output
        current_layer.sort();

        // Remove current layer nodes from remaining
        for id in &current_layer {
            remaining.remove(id.as_str());
        }

        // Update in-degrees for successors
        for id in &current_layer {
            if let Some(succs) = successors.get(id.as_str()) {
                for succ in succs {
                    if let Some(deg) = in_degree.get_mut(*succ) {
                        *deg = deg.saturating_sub(1);
                    }
                }
            }
        }

        layers.push(current_layer);
    }

    layers
}

/// Assign grid positions to nodes based on layers.
fn compute_grid_positions(layers: &[Vec<String>]) -> HashMap<String, GridPos> {
    let mut positions = HashMap::new();

    for (layer_idx, layer) in layers.iter().enumerate() {
        for (pos_idx, node_id) in layer.iter().enumerate() {
            positions.insert(
                node_id.clone(),
                GridPos {
                    layer: layer_idx,
                    pos: pos_idx,
                },
            );
        }
    }

    positions
}

/// Compute the width needed for each layer and height of each layer.
fn compute_layer_dimensions(
    layers: &[Vec<String>],
    node_dims: &HashMap<String, (usize, usize)>,
) -> (Vec<usize>, Vec<usize>) {
    let mut layer_widths = Vec::new();
    let mut layer_heights = Vec::new();

    for layer in layers {
        let mut total_width = 0;
        let mut max_height = 0;

        for node_id in layer {
            if let Some(&(w, h)) = node_dims.get(node_id) {
                total_width += w;
                max_height = max_height.max(h);
            }
        }

        layer_widths.push(total_width);
        layer_heights.push(max_height);
    }

    (layer_widths, layer_heights)
}

/// Convert grid positions to draw coordinates for vertical (TD/BT) layouts.
fn grid_to_draw_vertical(
    grid_positions: &HashMap<String, GridPos>,
    node_dims: &HashMap<String, (usize, usize)>,
    layers: &[Vec<String>],
    layer_heights: &[usize],
    config: &LayoutConfig,
    reverse: bool,
) -> (
    HashMap<String, (usize, usize)>,
    HashMap<String, NodeBounds>,
    usize,
    usize,
) {
    let mut draw_positions = HashMap::new();
    let mut node_bounds = HashMap::new();

    // Calculate the maximum total width needed (for centering layers)
    let max_layer_content_width: usize = layers
        .iter()
        .map(|layer| {
            let content_width: usize = layer
                .iter()
                .filter_map(|id| node_dims.get(id).map(|(w, _)| *w))
                .sum();
            let spacing = if layer.len() > 1 {
                (layer.len() - 1) * config.h_spacing
            } else {
                0
            };
            content_width + spacing
        })
        .max()
        .unwrap_or(0);

    let canvas_width = max_layer_content_width + 2 * config.padding;

    // Calculate Y positions for each layer
    let mut layer_y_starts = Vec::new();
    let mut y = config.padding;
    for &height in layer_heights {
        layer_y_starts.push(y);
        y += height + config.v_spacing;
    }
    let canvas_height = y - config.v_spacing + config.padding;

    // For BT, reverse the Y positions
    if reverse {
        layer_y_starts.reverse();
    }

    // Position nodes within each layer
    for (layer_idx, layer) in layers.iter().enumerate() {
        if layer.is_empty() {
            continue;
        }

        // Sort nodes by their grid position
        let mut sorted_nodes: Vec<_> = layer.iter().collect();
        sorted_nodes.sort_by_key(|id| grid_positions.get(*id).map(|p| p.pos).unwrap_or(0));

        // Calculate total width of this layer
        let content_width: usize = sorted_nodes
            .iter()
            .filter_map(|id| node_dims.get(*id).map(|(w, _)| *w))
            .sum();
        let spacing = if sorted_nodes.len() > 1 {
            (sorted_nodes.len() - 1) * config.h_spacing
        } else {
            0
        };
        let total_layer_width = content_width + spacing;

        // Center the layer horizontally
        let layer_start_x = config.padding + (max_layer_content_width - total_layer_width) / 2;

        let mut x = layer_start_x;
        for node_id in sorted_nodes {
            if let Some(&(w, h)) = node_dims.get(node_id) {
                let y = layer_y_starts[layer_idx];
                draw_positions.insert(node_id.clone(), (x, y));
                node_bounds.insert(
                    node_id.clone(),
                    NodeBounds {
                        x,
                        y,
                        width: w,
                        height: h,
                    },
                );
                x += w + config.h_spacing;
            }
        }
    }

    (draw_positions, node_bounds, canvas_width, canvas_height)
}

/// Convert grid positions to draw coordinates for horizontal (LR/RL) layouts.
fn grid_to_draw_horizontal(
    grid_positions: &HashMap<String, GridPos>,
    node_dims: &HashMap<String, (usize, usize)>,
    layers: &[Vec<String>],
    _layer_widths: &[usize],
    config: &LayoutConfig,
    reverse: bool,
) -> (
    HashMap<String, (usize, usize)>,
    HashMap<String, NodeBounds>,
    usize,
    usize,
) {
    let mut draw_positions = HashMap::new();
    let mut node_bounds = HashMap::new();

    // For horizontal layout, layers become columns
    // Calculate max width per layer (column)
    let max_layer_widths: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter_map(|id| node_dims.get(id).map(|(w, _)| *w))
                .max()
                .unwrap_or(0)
        })
        .collect();

    // Calculate the maximum total height needed (for centering columns)
    let max_column_content_height: usize = layers
        .iter()
        .map(|layer| {
            let content_height: usize = layer
                .iter()
                .filter_map(|id| node_dims.get(id).map(|(_, h)| *h))
                .sum();
            let spacing = if layer.len() > 1 {
                (layer.len() - 1) * config.v_spacing
            } else {
                0
            };
            content_height + spacing
        })
        .max()
        .unwrap_or(0);

    let canvas_height = max_column_content_height + 2 * config.padding;

    // Calculate X positions for each layer (column)
    let mut layer_x_starts = Vec::new();
    let mut x = config.padding;
    for &width in &max_layer_widths {
        layer_x_starts.push(x);
        x += width + config.h_spacing;
    }
    let canvas_width = x - config.h_spacing + config.padding;

    // For RL, reverse the X positions
    if reverse {
        layer_x_starts.reverse();
    }

    // Position nodes within each layer (column)
    for (layer_idx, layer) in layers.iter().enumerate() {
        if layer.is_empty() {
            continue;
        }

        // Sort nodes by their grid position
        let mut sorted_nodes: Vec<_> = layer.iter().collect();
        sorted_nodes.sort_by_key(|id| grid_positions.get(*id).map(|p| p.pos).unwrap_or(0));

        // Calculate total height of this column
        let content_height: usize = sorted_nodes
            .iter()
            .filter_map(|id| node_dims.get(*id).map(|(_, h)| *h))
            .sum();
        let spacing = if sorted_nodes.len() > 1 {
            (sorted_nodes.len() - 1) * config.v_spacing
        } else {
            0
        };
        let total_column_height = content_height + spacing;

        // Center the column vertically
        let column_start_y = config.padding + (max_column_content_height - total_column_height) / 2;

        let mut y = column_start_y;
        for node_id in sorted_nodes {
            if let Some(&(w, h)) = node_dims.get(node_id) {
                // Center nodes horizontally within the column width
                let layer_width = max_layer_widths[layer_idx];
                let node_x = layer_x_starts[layer_idx] + (layer_width - w) / 2;

                draw_positions.insert(node_id.clone(), (node_x, y));
                node_bounds.insert(
                    node_id.clone(),
                    NodeBounds {
                        x: node_x,
                        y,
                        width: w,
                        height: h,
                    },
                );
                y += h + config.v_spacing;
            }
        }
    }

    (draw_positions, node_bounds, canvas_width, canvas_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Edge, Node};

    fn simple_diagram() -> Diagram {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("Process"));
        diagram.add_node(Node::new("C").with_label("End"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "C"));
        diagram
    }

    #[test]
    fn test_topological_layers_linear() {
        let diagram = simple_diagram();
        let layers = topological_layers(&diagram);

        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["A"]);
        assert_eq!(layers[1], vec!["B"]);
        assert_eq!(layers[2], vec!["C"]);
    }

    #[test]
    fn test_topological_layers_parallel() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A"));
        diagram.add_node(Node::new("B"));
        diagram.add_node(Node::new("C"));
        diagram.add_edge(Edge::new("A", "C"));
        diagram.add_edge(Edge::new("B", "C"));

        let layers = topological_layers(&diagram);

        assert_eq!(layers.len(), 2);
        // A and B should be in the first layer (sorted)
        assert!(layers[0].contains(&"A".to_string()));
        assert!(layers[0].contains(&"B".to_string()));
        assert_eq!(layers[1], vec!["C"]);
    }

    #[test]
    fn test_topological_layers_diamond() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A"));
        diagram.add_node(Node::new("B"));
        diagram.add_node(Node::new("C"));
        diagram.add_node(Node::new("D"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("A", "C"));
        diagram.add_edge(Edge::new("B", "D"));
        diagram.add_edge(Edge::new("C", "D"));

        let layers = topological_layers(&diagram);

        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["A"]);
        // B and C should be in layer 1 (sorted)
        assert!(layers[1].contains(&"B".to_string()));
        assert!(layers[1].contains(&"C".to_string()));
        assert_eq!(layers[2], vec!["D"]);
    }

    #[test]
    fn test_compute_grid_positions() {
        let diagram = simple_diagram();
        let layers = topological_layers(&diagram);
        let positions = compute_grid_positions(&layers);

        assert_eq!(positions.get("A"), Some(&GridPos { layer: 0, pos: 0 }));
        assert_eq!(positions.get("B"), Some(&GridPos { layer: 1, pos: 0 }));
        assert_eq!(positions.get("C"), Some(&GridPos { layer: 2, pos: 0 }));
    }

    #[test]
    fn test_compute_layout() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Should have positions for all nodes
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
        assert!(layout.draw_positions.contains_key("C"));

        // Should have bounds for all nodes
        assert!(layout.node_bounds.contains_key("A"));
        assert!(layout.node_bounds.contains_key("B"));
        assert!(layout.node_bounds.contains_key("C"));

        // Canvas dimensions should be positive
        assert!(layout.width > 0);
        assert!(layout.height > 0);
    }

    #[test]
    fn test_layout_vertical_ordering() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let a_y = layout.draw_positions.get("A").unwrap().1;
        let b_y = layout.draw_positions.get("B").unwrap().1;
        let c_y = layout.draw_positions.get("C").unwrap().1;

        // A should be above B, B above C
        assert!(a_y < b_y);
        assert!(b_y < c_y);
    }

    #[test]
    fn test_layout_handles_cycle() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A"));
        diagram.add_node(Node::new("B"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "A")); // Cycle!

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Should still produce a layout (cycle is broken)
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
    }

    #[test]
    fn test_layout_horizontal_centering() {
        // Create diagram with nodes of different widths in same layer
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("X")); // narrow
        diagram.add_node(Node::new("B").with_label("Very Long Label")); // wide
        diagram.add_edge(Edge::new("A", "B"));

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Both nodes should be horizontally centered
        let a_bounds = layout.node_bounds.get("A").unwrap();
        let b_bounds = layout.node_bounds.get("B").unwrap();

        // The center of each node should be roughly aligned
        let a_center = a_bounds.center_x();
        let b_center = b_bounds.center_x();

        // They should be within the canvas bounds and reasonably centered
        assert!(a_center < layout.width);
        assert!(b_center < layout.width);
    }

    #[test]
    fn test_compute_layout_dagre_simple() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        // Should have positions for all nodes
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
        assert!(layout.draw_positions.contains_key("C"));

        // Should have bounds for all nodes
        assert!(layout.node_bounds.contains_key("A"));
        assert!(layout.node_bounds.contains_key("B"));
        assert!(layout.node_bounds.contains_key("C"));

        // Canvas dimensions should be positive
        assert!(layout.width > 0);
        assert!(layout.height > 0);
    }

    #[test]
    fn test_compute_layout_dagre_vertical_ordering() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let a_y = layout.draw_positions.get("A").unwrap().1;
        let b_y = layout.draw_positions.get("B").unwrap().1;
        let c_y = layout.draw_positions.get("C").unwrap().1;

        // A should be above B, B above C
        assert!(a_y < b_y, "A ({}) should be above B ({})", a_y, b_y);
        assert!(b_y < c_y, "B ({}) should be above C ({})", b_y, c_y);
    }

    #[test]
    fn test_compute_layout_dagre_handles_cycle() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A"));
        diagram.add_node(Node::new("B"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "A")); // Cycle!

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        // Should still produce a layout (cycle is handled)
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));

        // Should have a backward edge
        assert_eq!(layout.backward_corridors, 1);
    }

    #[test]
    fn test_compute_layout_dagre_lr_direction() {
        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A"));
        diagram.add_node(Node::new("B"));
        diagram.add_edge(Edge::new("A", "B"));

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let a_x = layout.draw_positions.get("A").unwrap().0;
        let b_x = layout.draw_positions.get("B").unwrap().0;

        // A should be left of B
        assert!(a_x < b_x, "A ({}) should be left of B ({})", a_x, b_x);
    }
}
