//! Layout computation for flowchart diagrams.
//!
//! This module computes the position of nodes on a grid based on topological ordering.
//! It supports both a built-in algorithm and an optional dagre-based algorithm for
//! better crossing reduction and cycle handling.

use std::collections::{HashMap, HashSet};

use super::shape::{NodeBounds, node_dimensions};
use crate::dagre::{self, Direction as DagreDirection, LayoutConfig as DagreConfig};
use crate::graph::{Diagram, Direction, Shape};

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

    // --- Edge routing data from normalization ---
    /// Waypoints for each edge, derived from dummy node positions.
    /// Key: (from_id, to_id), Value: list of waypoint coordinates.
    /// Empty for short edges (span 1 rank), populated for long edges.
    pub edge_waypoints: HashMap<(String, String), Vec<(usize, usize)>>,

    /// Pre-computed label positions for edges with labels.
    /// Key: (from_id, to_id), Value: (x, y) position for the label center.
    /// Only populated for edges that have labels.
    pub edge_label_positions: HashMap<(String, String), (usize, usize)>,

    /// Node shapes for intersection calculation.
    /// Maps node ID to its shape for computing dynamic attachment points.
    pub node_shapes: HashMap<String, Shape>,
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
    /// Extra left margin for edge labels on left branches.
    pub left_label_margin: usize,
    /// Extra right margin for edge labels on right branches.
    pub right_label_margin: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            h_spacing: 4,
            v_spacing: 3,
            padding: 1,
            left_label_margin: 0,
            right_label_margin: 0,
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

    // Step 5: Convert grid positions to draw coordinates based on direction
    let (draw_positions, node_bounds, width, height) = match diagram.direction {
        Direction::TopDown | Direction::BottomTop => {
            let result = grid_to_draw_vertical(
                &grid_positions,
                &node_dims,
                &layers,
                &layer_heights,
                config,
                diagram.direction == Direction::BottomTop,
            );
            (
                result.draw_positions,
                result.node_bounds,
                result.width,
                result.height,
            )
        }
        Direction::LeftRight | Direction::RightLeft => {
            let result = grid_to_draw_horizontal(
                &grid_positions,
                &node_dims,
                &layers,
                &layer_widths,
                config,
                diagram.direction == Direction::RightLeft,
            );
            (
                result.draw_positions,
                result.node_bounds,
                result.width,
                result.height,
            )
        }
    };

    // Step 6: Collect node shapes
    let node_shapes: HashMap<String, Shape> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node.shape))
        .collect();

    Layout {
        grid_positions,
        draw_positions,
        node_bounds,
        width,
        height,
        h_spacing: config.h_spacing,
        v_spacing: config.v_spacing,
        edge_waypoints: HashMap::new(),
        edge_label_positions: HashMap::new(),
        node_shapes,
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

    // Collect node IDs in declaration order (order they first appear in edges)
    // This preserves the user's intended flow direction for cycle detection
    let mut seen_nodes = std::collections::HashSet::new();
    let mut ordered_node_ids = Vec::new();

    for edge in &diagram.edges {
        for node_id in [&edge.from, &edge.to] {
            if !seen_nodes.contains(node_id) {
                seen_nodes.insert(node_id.clone());
                ordered_node_ids.push(node_id.clone());
            }
        }
    }

    // Add any remaining nodes that aren't in any edges
    for id in diagram.nodes.keys() {
        if !seen_nodes.contains(id) {
            ordered_node_ids.push(id.clone());
        }
    }

    // Add nodes with dimensions in declaration order
    for id in &ordered_node_ids {
        if let Some(node) = diagram.nodes.get(id) {
            let dims = node_dimensions(node);
            dgraph.add_node(id.as_str(), dims);
        }
    }

    // Add edges and collect label dimensions
    let mut edge_labels: std::collections::HashMap<usize, dagre::normalize::EdgeLabelInfo> =
        std::collections::HashMap::new();

    for (edge_idx, edge) in diagram.edges.iter().enumerate() {
        dgraph.add_edge(edge.from.as_str(), edge.to.as_str());

        // If edge has a label, calculate its dimensions
        if let Some(ref label) = edge.label {
            // Calculate label dimensions in character coordinates
            // Label width: label length + 2 for padding
            // Label height: 1 for single line
            let label_width = label.len() + 2;
            let label_height = 1;
            edge_labels.insert(
                edge_idx,
                dagre::normalize::EdgeLabelInfo::new(label_width as f64, label_height as f64),
            );
        }
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

    let result = dagre::layout_with_labels(
        &dgraph,
        &dagre_config,
        |_, dims| (dims.0 as f64, dims.1 as f64),
        &edge_labels,
    );

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

    // Convert grid positions to draw coordinates using original ASCII logic
    // Also capture layer positions for waypoint transformation
    let (draw_positions, node_bounds, width, height, layer_starts) = match diagram.direction {
        Direction::TopDown | Direction::BottomTop => {
            let result = grid_to_draw_vertical(
                &grid_positions,
                &node_dims,
                &layers,
                &layer_heights,
                config,
                diagram.direction == Direction::BottomTop,
            );
            (
                result.draw_positions,
                result.node_bounds,
                result.width,
                result.height,
                result.layer_y_starts,
            )
        }
        Direction::LeftRight | Direction::RightLeft => {
            let result = grid_to_draw_horizontal(
                &grid_positions,
                &node_dims,
                &layers,
                &layer_widths,
                config,
                diagram.direction == Direction::RightLeft,
            );
            (
                result.draw_positions,
                result.node_bounds,
                result.width,
                result.height,
                result.layer_x_starts,
            )
        }
    };

    // Build per-rank anchor mapping from dagre coordinate space to draw coordinate space.
    // For each rank, collect (dagre_cross_pos, draw_cross_center) pairs from real nodes.
    // These anchors allow us to map waypoint cross-axis positions accurately instead of
    // linearly interpolating between source and target.
    let rank_cross_anchors: Vec<Vec<(f64, f64)>> = layers
        .iter()
        .map(|layer| {
            let mut anchors: Vec<(f64, f64)> = layer
                .iter()
                .filter_map(|node_id| {
                    let dagre_node = result.nodes.get(&dagre::NodeId(node_id.clone()))?;
                    let &(draw_x, draw_y) = draw_positions.get(node_id)?;
                    let &(w, h) = node_dims.get(node_id)?;

                    if is_vertical {
                        // TD/BT: cross-axis is X
                        let dagre_center_x = dagre_node.x + dagre_node.width / 2.0;
                        let draw_center_x = (draw_x + w / 2) as f64;
                        Some((dagre_center_x, draw_center_x))
                    } else {
                        // LR/RL: cross-axis is Y
                        let dagre_center_y = dagre_node.y + dagre_node.height / 2.0;
                        let draw_center_y = (draw_y + h / 2) as f64;
                        Some((dagre_center_y, draw_center_y))
                    }
                })
                .collect();

            anchors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            anchors
        })
        .collect();

    // Convert dagre waypoints to Layout format with proper coordinate transformation.
    // Waypoints are in dagre's internal coordinate space (node_sep=50, rank_sep=50) and
    // need to be transformed to ASCII draw coordinates using rank information.
    let mut edge_waypoints_converted: HashMap<(String, String), Vec<(usize, usize)>> =
        HashMap::new();
    let mut edge_label_positions_converted: HashMap<(String, String), (usize, usize)> =
        HashMap::new();

    // Transform edge_waypoints from dagre coordinates to draw coordinates
    let is_vertical = matches!(diagram.direction, Direction::TopDown | Direction::BottomTop);

    for (edge_idx, waypoints) in &result.edge_waypoints {
        if let Some(edge) = diagram.edges.get(*edge_idx) {
            let key = (edge.from.clone(), edge.to.clone());

            let canvas_cross_extent = if is_vertical { width } else { height };

            let converted: Vec<(usize, usize)> = waypoints
                .iter()
                .map(|wp| {
                    let rank_idx = wp.rank as usize;

                    if is_vertical {
                        // TD/BT: primary axis = Y (from layer_starts), cross axis = X (from dagre)
                        let y = layer_starts.get(rank_idx).copied().unwrap_or(0);
                        let anchors = rank_cross_anchors
                            .get(rank_idx)
                            .map(|a| a.as_slice())
                            .unwrap_or(&[]);
                        let x = map_cross_axis(wp.point.x, anchors, canvas_cross_extent);
                        (x, y)
                    } else {
                        // LR/RL: primary axis = X (from layer_starts), cross axis = Y (from dagre)
                        let x = layer_starts.get(rank_idx).copied().unwrap_or(0);
                        let anchors = rank_cross_anchors
                            .get(rank_idx)
                            .map(|a| a.as_slice())
                            .unwrap_or(&[]);
                        let y = map_cross_axis(wp.point.y, anchors, canvas_cross_extent);
                        (x, y)
                    }
                })
                .collect();

            edge_waypoints_converted.insert(key, converted);
        }
    }

    // Post-process: nudge waypoints that collide with node bounding boxes.
    // Dagre's ordering should prevent this in most cases, but wide nodes
    // can occasionally overlap with waypoint positions.
    for waypoints in edge_waypoints_converted.values_mut() {
        for wp in waypoints.iter_mut() {
            for bounds in node_bounds.values() {
                let (wp_x, wp_y) = *wp;

                // Check if waypoint falls within a node's bounding box
                let collides = wp_x >= bounds.x
                    && wp_x < bounds.x + bounds.width
                    && wp_y >= bounds.y
                    && wp_y < bounds.y + bounds.height;

                if collides {
                    // Nudge waypoint to just past the right/bottom edge of the node
                    if is_vertical {
                        wp.0 = bounds.x + bounds.width + 1;
                    } else {
                        wp.1 = bounds.y + bounds.height + 1;
                    }
                    // Only handle first collision per waypoint
                    break;
                }
            }
        }
    }

    // Convert label_positions from edge index to (from, to) key
    for (edge_idx, pos) in &result.label_positions {
        if let Some(edge) = diagram.edges.get(*edge_idx) {
            let key = (edge.from.clone(), edge.to.clone());
            edge_label_positions_converted
                .insert(key, (pos.x.round() as usize, pos.y.round() as usize));
        }
    }

    // Collect node shapes
    let node_shapes: HashMap<String, Shape> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node.shape))
        .collect();

    Layout {
        grid_positions,
        draw_positions,
        node_bounds,
        width,
        height,
        h_spacing: config.h_spacing,
        v_spacing: config.v_spacing,
        edge_waypoints: edge_waypoints_converted,
        edge_label_positions: edge_label_positions_converted,
        node_shapes,
    }
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
/// Result of grid_to_draw_vertical, including layer position data for waypoint transformation.
struct VerticalLayoutResult {
    draw_positions: HashMap<String, (usize, usize)>,
    node_bounds: HashMap<String, NodeBounds>,
    width: usize,
    height: usize,
    /// Y position where each layer starts (index = layer/rank).
    layer_y_starts: Vec<usize>,
}

fn grid_to_draw_vertical(
    grid_positions: &HashMap<String, GridPos>,
    node_dims: &HashMap<String, (usize, usize)>,
    layers: &[Vec<String>],
    layer_heights: &[usize],
    config: &LayoutConfig,
    reverse: bool,
) -> VerticalLayoutResult {
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

    let canvas_width = max_layer_content_width
        + 2 * config.padding
        + config.left_label_margin
        + config.right_label_margin;

    // Calculate Y positions for each layer
    let mut layer_y_starts = Vec::new();
    let mut y = config.padding;
    for &height in layer_heights {
        layer_y_starts.push(y);
        y += height + config.v_spacing;
    }
    let canvas_height = y - config.v_spacing + config.padding;

    // Note: We no longer reverse for BT because dagre's position.rs already
    // flips y-coordinates for BottomTop direction. Double-reversing would
    // produce incorrect results.
    let _ = reverse; // Parameter kept for API compatibility but not used

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

        // Center the layer horizontally, accounting for left label margin
        let layer_start_x = config.padding
            + config.left_label_margin
            + (max_layer_content_width - total_layer_width) / 2;

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

    VerticalLayoutResult {
        draw_positions,
        node_bounds,
        width: canvas_width,
        height: canvas_height,
        layer_y_starts,
    }
}

/// Result of grid_to_draw_horizontal, including layer position data for waypoint transformation.
struct HorizontalLayoutResult {
    draw_positions: HashMap<String, (usize, usize)>,
    node_bounds: HashMap<String, NodeBounds>,
    width: usize,
    height: usize,
    /// X position where each layer starts (index = layer/rank).
    layer_x_starts: Vec<usize>,
}

/// Convert grid positions to draw coordinates for horizontal (LR/RL) layouts.
fn grid_to_draw_horizontal(
    grid_positions: &HashMap<String, GridPos>,
    node_dims: &HashMap<String, (usize, usize)>,
    layers: &[Vec<String>],
    _layer_widths: &[usize],
    config: &LayoutConfig,
    reverse: bool,
) -> HorizontalLayoutResult {
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

    // Note: We no longer reverse for RL because dagre's position.rs already
    // flips x-coordinates for RightLeft direction. Double-reversing would
    // produce incorrect results.
    let _ = reverse; // Parameter kept for API compatibility but not used

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

    HorizontalLayoutResult {
        draw_positions,
        node_bounds,
        width: canvas_width,
        height: canvas_height,
        layer_x_starts,
    }
}

/// Map a dagre cross-axis coordinate to draw coordinate using anchor points at a given rank.
///
/// Uses piecewise linear interpolation between known node positions.
/// If the target coordinate is outside the anchor range, extrapolates from the nearest pair.
/// Falls back to returning the coordinate clamped to canvas bounds if no anchors exist.
fn map_cross_axis(dagre_pos: f64, anchors: &[(f64, f64)], canvas_extent: usize) -> usize {
    match anchors.len() {
        0 => {
            // No anchors at this rank — clamp to canvas center
            canvas_extent / 2
        }
        1 => {
            // Single anchor: offset from it
            let (dagre_anchor, draw_anchor) = anchors[0];
            let offset = dagre_pos - dagre_anchor;
            // Scale: dagre uses node_sep=50, draw uses h_spacing≈4-6
            // Use ratio of ~0.1 as rough scaling factor
            let scaled_offset = offset * 0.1;
            let result = draw_anchor + scaled_offset;
            result
                .round()
                .max(0.0)
                .min(canvas_extent.saturating_sub(1) as f64) as usize
        }
        _ => {
            // Multiple anchors: piecewise linear interpolation
            // Find the two anchors bracketing dagre_pos
            if dagre_pos <= anchors[0].0 {
                // Before first anchor — extrapolate from first two
                let (d0, w0) = anchors[0];
                let (d1, w1) = anchors[1];
                let ratio = if (d1 - d0).abs() > f64::EPSILON {
                    (dagre_pos - d0) / (d1 - d0)
                } else {
                    0.0
                };
                let result = w0 + ratio * (w1 - w0);
                result
                    .round()
                    .max(0.0)
                    .min(canvas_extent.saturating_sub(1) as f64) as usize
            } else if dagre_pos >= anchors[anchors.len() - 1].0 {
                // After last anchor — extrapolate from last two
                let n = anchors.len();
                let (d0, w0) = anchors[n - 2];
                let (d1, w1) = anchors[n - 1];
                let ratio = if (d1 - d0).abs() > f64::EPSILON {
                    (dagre_pos - d0) / (d1 - d0)
                } else {
                    1.0
                };
                let result = w0 + ratio * (w1 - w0);
                result
                    .round()
                    .max(0.0)
                    .min(canvas_extent.saturating_sub(1) as f64) as usize
            } else {
                // Between two anchors — interpolate
                for i in 0..anchors.len() - 1 {
                    let (d0, w0) = anchors[i];
                    let (d1, w1) = anchors[i + 1];
                    if dagre_pos >= d0 && dagre_pos <= d1 {
                        let ratio = if (d1 - d0).abs() > f64::EPSILON {
                            (dagre_pos - d0) / (d1 - d0)
                        } else {
                            0.5
                        };
                        let result = w0 + ratio * (w1 - w0);
                        return result
                            .round()
                            .max(0.0)
                            .min(canvas_extent.saturating_sub(1) as f64)
                            as usize;
                    }
                }
                // Shouldn't reach here but fallback
                canvas_extent / 2
            }
        }
    }
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

    #[test]
    fn test_waypoint_transformation_vertical() {
        // Create a diagram with a long edge: A -> B -> C -> D, and A -> D
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("Step1"));
        diagram.add_node(Node::new("C").with_label("Step2"));
        diagram.add_node(Node::new("D").with_label("End"));
        diagram.add_edge(Edge::new("A", "B")); // Edge 0
        diagram.add_edge(Edge::new("B", "C")); // Edge 1
        diagram.add_edge(Edge::new("C", "D")); // Edge 2
        diagram.add_edge(Edge::new("A", "D")); // Edge 3 - long edge spanning 3 ranks

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        // The A->D edge should have waypoints
        let key = ("A".to_string(), "D".to_string());
        assert!(
            layout.edge_waypoints.contains_key(&key),
            "Long edge A->D should have waypoints"
        );

        let waypoints = layout.edge_waypoints.get(&key).unwrap();
        // A->D spans 3 ranks, needs 2 dummies (at ranks 1 and 2)
        assert_eq!(
            waypoints.len(),
            2,
            "Should have 2 waypoints for edge spanning 3 ranks"
        );

        // Get node positions to verify waypoint positions are reasonable
        let a_pos = layout.draw_positions.get("A").unwrap();
        let d_pos = layout.draw_positions.get("D").unwrap();

        // Waypoints should be between A and D vertically
        for (i, &(wx, wy)) in waypoints.iter().enumerate() {
            assert!(
                wy > a_pos.1 && wy < d_pos.1,
                "Waypoint {} y={} should be between A.y={} and D.y={}",
                i,
                wy,
                a_pos.1,
                d_pos.1
            );
            // Waypoints should be within canvas bounds
            assert!(
                wx < layout.width,
                "Waypoint {} x={} should be within canvas width={}",
                i,
                wx,
                layout.width
            );
        }

        // Waypoints should be in increasing y order
        assert!(
            waypoints[0].1 < waypoints[1].1,
            "Waypoints should be in increasing y order"
        );
    }

    #[test]
    fn test_waypoint_transformation_horizontal() {
        // Create a diagram with a long edge in LR direction
        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("Step1"));
        diagram.add_node(Node::new("C").with_label("Step2"));
        diagram.add_node(Node::new("D").with_label("End"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "C"));
        diagram.add_edge(Edge::new("C", "D"));
        diagram.add_edge(Edge::new("A", "D")); // Long edge

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let key = ("A".to_string(), "D".to_string());
        assert!(
            layout.edge_waypoints.contains_key(&key),
            "Long edge A->D should have waypoints"
        );

        let waypoints = layout.edge_waypoints.get(&key).unwrap();
        assert_eq!(
            waypoints.len(),
            2,
            "Should have 2 waypoints for edge spanning 3 ranks"
        );

        // Get node positions
        let a_pos = layout.draw_positions.get("A").unwrap();
        let d_pos = layout.draw_positions.get("D").unwrap();

        // Waypoints should be between A and D horizontally
        for (i, &(wx, wy)) in waypoints.iter().enumerate() {
            assert!(
                wx > a_pos.0 && wx < d_pos.0,
                "Waypoint {} x={} should be between A.x={} and D.x={}",
                i,
                wx,
                a_pos.0,
                d_pos.0
            );
            assert!(
                wy < layout.height,
                "Waypoint {} y={} should be within canvas height={}",
                i,
                wy,
                layout.height
            );
        }

        // Waypoints should be in increasing x order
        assert!(
            waypoints[0].0 < waypoints[1].0,
            "Waypoints should be in increasing x order"
        );
    }
}
