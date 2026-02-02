//! Layout computation for flowchart diagrams.
//!
//! Translates dagre float coordinates into ASCII character-grid positions using
//! uniform scale factors, collision repair, and waypoint transformation.

use std::collections::{HashMap, HashSet};

use super::shape::{NodeBounds, node_dimensions};
#[cfg(test)]
use crate::dagre::Point;
use crate::dagre::normalize::WaypointWithRank;
use crate::dagre::{self, Direction as DagreDirection, LayoutConfig as DagreConfig, Rect};
use crate::graph::{Diagram, Direction, Edge, Shape};

/// Bounding box for a subgraph border in draw coordinates.
#[derive(Debug, Clone)]
pub struct SubgraphBounds {
    /// Left edge x coordinate.
    pub x: usize,
    /// Top edge y coordinate.
    pub y: usize,
    /// Total width including border.
    pub width: usize,
    /// Total height including border.
    pub height: usize,
    /// Display title for the subgraph.
    pub title: String,
    /// Nesting depth (0 = top-level, 1 = nested once, etc.)
    pub depth: usize,
}

/// Draw-coordinate data for a self-edge loop.
#[derive(Debug, Clone)]
pub struct SelfEdgeDrawData {
    /// Node ID the self-edge loops on.
    pub node_id: String,
    /// Original edge index.
    pub edge_index: usize,
    /// Draw-coordinate points for the orthogonal loop.
    pub points: Vec<(usize, usize)>,
}

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

    /// Subgraph bounding boxes in draw coordinates.
    /// Key: subgraph ID, Value: bounds with title.
    /// Empty for diagrams without subgraphs.
    pub subgraph_bounds: HashMap<String, SubgraphBounds>,

    /// Self-edge loop data in draw coordinates.
    pub self_edges: Vec<SelfEdgeDrawData>,
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
    /// Ranking algorithm override.
    pub ranker: Option<crate::dagre::types::Ranker>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            h_spacing: 4,
            v_spacing: 3,
            padding: 1,
            left_label_margin: 0,
            right_label_margin: 0,
            ranker: None,
        }
    }
}

/// Compute the layout using the dagre algorithm with direct coordinate translation.
///
/// This uses uniform scale factors to translate dagre's float coordinates to ASCII
/// character cells, replacing the stagger pipeline. The 3-step process:
/// 1. Compute per-axis scale factors
/// 2. Apply uniform scaling + rounding to all dagre coordinates
/// 3. Enforce minimum spacing via collision repair
pub fn compute_layout_direct(diagram: &Diagram, config: &LayoutConfig) -> Layout {
    // --- Phase A: Build dagre graph ---
    let mut dgraph = dagre::DiGraph::new();

    let mut seen = std::collections::HashSet::new();
    let mut ordered_node_ids = Vec::new();
    for edge in &diagram.edges {
        for node_id in [&edge.from, &edge.to] {
            if seen.insert(node_id.clone()) {
                ordered_node_ids.push(node_id.clone());
            }
        }
    }
    let mut node_keys: Vec<&String> = diagram.nodes.keys().collect();
    node_keys.sort();
    for id in node_keys {
        if seen.insert(id.clone()) {
            ordered_node_ids.push(id.clone());
        }
    }

    for id in &ordered_node_ids {
        if let Some(node) = diagram.nodes.get(id) {
            let dims = node_dimensions(node);
            dgraph.add_node(id.as_str(), dims);
        }
    }

    // Add subgraph compound nodes in reverse parse order (Mermaid parity).
    // subgraph_order is post-order (inner-first); reversing gives outer-first,
    // matching Mermaid's getData() insertion order.
    let subgraph_keys: Vec<&String> = diagram.subgraph_order.iter().rev().collect();
    for sg_id in &subgraph_keys {
        let sg = &diagram.subgraphs[*sg_id];
        dgraph.add_node(sg_id.as_str(), (0, 0));
        if !sg.title.trim().is_empty() {
            dgraph.set_has_title(sg_id.as_str());
        }
    }

    // Set parent relationships for compound nodes
    let mut node_parent_keys: Vec<&String> = diagram.nodes.keys().collect();
    node_parent_keys.sort();
    for node_id in node_parent_keys {
        let node = &diagram.nodes[node_id];
        if let Some(ref parent) = node.parent {
            dgraph.set_parent(node_id.as_str(), parent.as_str());
        }
    }

    // Set parent relationships for nested subgraphs
    for sg_id in &subgraph_keys {
        let sg = &diagram.subgraphs[*sg_id];
        if let Some(ref parent_id) = sg.parent {
            dgraph.set_parent(sg_id.as_str(), parent_id.as_str());
        }
    }

    let mut edge_labels: HashMap<usize, dagre::normalize::EdgeLabelInfo> = HashMap::new();
    for (edge_idx, edge) in diagram.edges.iter().enumerate() {
        dgraph.add_edge(edge.from.as_str(), edge.to.as_str());
        if let Some(ref label) = edge.label {
            let label_width = label.len() + 2;
            edge_labels.insert(
                edge_idx,
                dagre::normalize::EdgeLabelInfo::new(label_width as f64, 1.0),
            );
        }
    }

    let dagre_direction = match diagram.direction {
        Direction::TopDown => DagreDirection::TopBottom,
        Direction::BottomTop => DagreDirection::BottomTop,
        Direction::LeftRight => DagreDirection::LeftRight,
        Direction::RightLeft => DagreDirection::RightLeft,
    };

    let (node_sep, edge_sep) = match dagre_direction {
        DagreDirection::LeftRight | DagreDirection::RightLeft => {
            let total_height: f64 = diagram
                .nodes
                .values()
                .map(|node| node_dimensions(node).1 as f64)
                .sum();
            let count = diagram.nodes.len().max(1) as f64;
            let avg_height = total_height / count;
            let ns = (avg_height * 2.0).max(6.0);
            let es = (avg_height * 0.8).max(2.0);
            (ns, es)
        }
        _ => (50.0, 20.0),
    };

    let dagre_config = DagreConfig {
        direction: dagre_direction,
        node_sep,
        edge_sep,
        rank_sep: 50.0,
        margin: 10.0,
        acyclic: true,
        ranker: config.ranker.unwrap_or_default(),
    };

    let result = dagre::layout_with_labels(
        &dgraph,
        &dagre_config,
        |_, dims| (dims.0 as f64, dims.1 as f64),
        &edge_labels,
    );

    if std::env::var("MMDFLUX_DEBUG_NODE_POS").is_ok_and(|v| v == "1") {
        for (id, rect) in &result.nodes {
            eprintln!(
                "[dagre_nodes] {} x={:.2} y={:.2} w={:.2} h={:.2}",
                id.0, rect.x, rect.y, rect.width, rect.height
            );
        }
    }

    // --- Phase B: Group nodes into layers ---
    let is_vertical = matches!(diagram.direction, Direction::TopDown | Direction::BottomTop);

    // Collect subgraph IDs to exclude from layer grouping (compound nodes are not rendered as nodes)
    let subgraph_ids: std::collections::HashSet<&str> =
        diagram.subgraphs.keys().map(|s| s.as_str()).collect();

    let mut layer_coords: Vec<(String, f64, f64)> = result
        .nodes
        .iter()
        .filter(|(id, _)| !subgraph_ids.contains(id.0.as_str()))
        .map(|(id, rect)| {
            let primary = if is_vertical { rect.y } else { rect.x };
            let secondary = if is_vertical { rect.x } else { rect.y };
            (id.0.clone(), primary, secondary)
        })
        .collect();
    layer_coords.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut current_layer: Vec<String> = Vec::new();
    let mut last_primary: Option<f64> = None;
    for (id, primary, _) in &layer_coords {
        if let Some(last) = last_primary
            && (*primary - last).abs() > 25.0
            && !current_layer.is_empty()
        {
            layers.push(std::mem::take(&mut current_layer));
        }
        current_layer.push(id.clone());
        last_primary = Some(*primary);
    }
    if !current_layer.is_empty() {
        layers.push(current_layer);
    }

    let secondary_coord = |id: &String| -> f64 {
        result
            .nodes
            .get(&dagre::NodeId(id.clone()))
            .map(|r| if is_vertical { r.x } else { r.y })
            .unwrap_or(0.0)
    };
    for layer in &mut layers {
        layer.sort_by(|a, b| secondary_coord(a).total_cmp(&secondary_coord(b)));
    }

    let grid_positions = compute_grid_positions(&layers);

    // --- Phase C: Compute node dimensions ---
    let node_dims: HashMap<String, (usize, usize)> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node_dimensions(node)))
        .collect();

    // --- Phase D: Scale dagre coordinates to ASCII ---
    // Dagre's makeSpaceForEdgeLabels always doubles minlen, which doubles rank gaps.
    // We compensate by adjusting the primary-axis scale factor so overall spacing
    // stays stable for ASCII rendering.
    let ranks_doubled = true;
    let (scale_x, scale_y) = compute_ascii_scale_factors(
        &node_dims,
        dagre_config.rank_sep,
        dagre_config.node_sep,
        config.v_spacing,
        config.h_spacing,
        is_vertical,
        ranks_doubled,
    );

    // Find dagre bounding box min
    let mut dagre_min_x = result
        .nodes
        .values()
        .map(|r| r.x)
        .fold(f64::INFINITY, f64::min);
    let mut dagre_min_y = result
        .nodes
        .values()
        .map(|r| r.y)
        .fold(f64::INFINITY, f64::min);

    if !result.subgraph_bounds.is_empty() {
        let sg_min_x = result
            .subgraph_bounds
            .values()
            .map(|r| r.x)
            .fold(f64::INFINITY, f64::min);
        let sg_min_y = result
            .subgraph_bounds
            .values()
            .map(|r| r.y)
            .fold(f64::INFINITY, f64::min);
        dagre_min_x = dagre_min_x.min(sg_min_x);
        dagre_min_y = dagre_min_y.min(sg_min_y);
    }

    if std::env::var("MMDFLUX_DEBUG_MIN_X").is_ok_and(|v| v == "1") {
        eprintln!(
            "[min_x] dagre_min_x={:.2} dagre_min_y={:.2}",
            dagre_min_x, dagre_min_y
        );
    }

    // Scale each node's center, then compute top-left.
    // First pass: compute raw centers and find the maximum overhang
    // (how much a node's half-width exceeds its raw center coordinate).
    // This prevents clipping to zero, which would destroy the relative
    // separations computed by the layout algorithm (e.g., BK stagger).
    //
    // Lesson: rendering pipeline bugs can silently mask
    // correct layout output. The original saturating_sub here clipped wide
    // left-positioned nodes to x=0, collapsing BK-computed stagger. The fix
    // is a uniform coordinate-space translation that preserves all relative
    // separations. When debugging layout issues, check the rendering pipeline
    // first — the layout algorithm may already be correct.
    let mut raw_centers: Vec<RawCenter> = Vec::new();
    let mut max_overhang_x: usize = 0;
    let mut max_overhang_y: usize = 0;

    for (id, rect) in &result.nodes {
        let node_id = &id.0;
        if let Some(&(w, h)) = node_dims.get(node_id) {
            let cx = ((rect.x + rect.width / 2.0 - dagre_min_x) * scale_x).round() as usize;
            let cy = ((rect.y + rect.height / 2.0 - dagre_min_y) * scale_y).round() as usize;
            if w / 2 > cx {
                max_overhang_x = max_overhang_x.max(w / 2 - cx);
            }
            if h / 2 > cy {
                max_overhang_y = max_overhang_y.max(h / 2 - cy);
            }
            raw_centers.push(RawCenter {
                id: node_id.clone(),
                cx,
                cy,
                w,
                h,
            });
        }
    }

    // Second pass: apply overhang offset and compute draw positions
    let mut draw_positions: HashMap<String, (usize, usize)> = HashMap::new();
    let mut node_bounds: HashMap<String, NodeBounds> = HashMap::new();

    for rc in &raw_centers {
        let center_x = rc.cx + max_overhang_x;
        let center_y = rc.cy + max_overhang_y;

        let x = center_x - rc.w / 2 + config.padding + config.left_label_margin;
        let y = center_y - rc.h / 2 + config.padding;

        draw_positions.insert(rc.id.clone(), (x, y));
        node_bounds.insert(
            rc.id.clone(),
            NodeBounds {
                x,
                y,
                width: rc.w,
                height: rc.h,
                dagre_center_x: Some(center_x + config.padding + config.left_label_margin),
                dagre_center_y: Some(center_y + config.padding),
            },
        );
    }

    // --- Phase E: Collision repair ---
    // Within-layer (cross-axis) repair
    collision_repair(
        &layers,
        &mut draw_positions,
        &node_dims,
        is_vertical,
        if is_vertical {
            config.h_spacing
        } else {
            config.v_spacing
        },
    );
    // Between-layer (primary-axis) repair: ensure minimum gap for edge routing
    rank_gap_repair(
        &layers,
        &mut draw_positions,
        &node_dims,
        is_vertical,
        if is_vertical {
            config.v_spacing
        } else {
            config.h_spacing
        },
    );

    // Update node_bounds after collision repair
    for (id, &(x, y)) in &draw_positions {
        if let Some(&(w, h)) = node_dims.get(id) {
            // Preserve dagre center from the initial pass
            let prev = node_bounds.get(id);
            let dagre_center_x = prev.and_then(|b| b.dagre_center_x);
            let dagre_center_y = prev.and_then(|b| b.dagre_center_y);
            node_bounds.insert(
                id.clone(),
                NodeBounds {
                    x,
                    y,
                    width: w,
                    height: h,
                    dagre_center_x,
                    dagre_center_y,
                },
            );
        }
    }

    // --- Phase F: Compute canvas size ---
    // Add margin for synthetic backward-edge routing around nodes
    let has_backward_edges = !result.reversed_edges.is_empty();
    let backward_margin = if has_backward_edges {
        super::router::BACKWARD_ROUTE_GAP + 2
    } else {
        0
    };

    let base_width = node_bounds
        .values()
        .map(|b| b.x + b.width)
        .max()
        .unwrap_or(0)
        + config.padding
        + config.right_label_margin;
    let base_height = node_bounds
        .values()
        .map(|b| b.y + b.height)
        .max()
        .unwrap_or(0)
        + config.padding;

    // For TD/BT, backward edges route to the right; for LR/RL, below
    let (width, height) = if is_vertical {
        (base_width + backward_margin, base_height)
    } else {
        (base_width, base_height + backward_margin)
    };

    // --- Phase G: Compute layer_starts from draw positions ---
    // layer_starts maps layer index → primary-axis draw coordinate for that layer.
    let layer_starts_raw: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter_map(|id| {
                    draw_positions
                        .get(id)
                        .map(|&(x, y)| if is_vertical { y } else { x })
                })
                .min()
                .unwrap_or(0)
        })
        .collect();

    // Compute max right/bottom edge per layer (primary-axis position + dimension).
    // Used for odd-rank interpolation to place labels in the gap between layers.
    let layer_ends_raw: Vec<usize> = layers
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter_map(|id| {
                    let &(x, y) = draw_positions.get(id)?;
                    let &(w, h) = node_dims.get(id)?;
                    if is_vertical {
                        Some(y + h)
                    } else {
                        Some(x + w)
                    }
                })
                .max()
                .unwrap_or(0)
        })
        .collect();

    // When ranks are doubled (labels present), real nodes sit at even dagre ranks
    // (0, 2, 4, ...) and dummies/labels at odd ranks (1, 3, 5, ...).
    // Build rank_positions: dagre_rank → draw coordinate.
    // Even ranks map to layer_starts_raw[rank/2].
    // Odd ranks interpolate between the right edge of the source layer and
    // the left edge of the target layer, placing labels in the gap between nodes.
    let layer_starts: Vec<usize> = if ranks_doubled && layer_starts_raw.len() >= 2 {
        let max_rank = layer_starts_raw.len() * 2 - 1;
        (0..=max_rank)
            .map(|rank| {
                let layer_idx = rank / 2;
                if rank % 2 == 0 {
                    // Even rank → real node layer
                    layer_starts_raw.get(layer_idx).copied().unwrap_or(0)
                } else {
                    // Odd rank → midpoint between right edge of source and left edge of target
                    let curr_end = layer_ends_raw.get(layer_idx).copied().unwrap_or(0);
                    let next_start = layer_starts_raw
                        .get(layer_idx + 1)
                        .copied()
                        .unwrap_or(curr_end);
                    (curr_end + next_start) / 2
                }
            })
            .collect()
    } else {
        layer_starts_raw
    };

    // --- Phase H: Transform waypoints and labels ---
    let ctx = TransformContext {
        dagre_min_x,
        dagre_min_y,
        scale_x,
        scale_y,
        padding: config.padding,
        left_label_margin: config.left_label_margin,
        overhang_x: max_overhang_x,
        overhang_y: max_overhang_y,
    };

    let edge_waypoints_converted = transform_waypoints_direct(
        &result.edge_waypoints,
        &diagram.edges,
        &ctx,
        &layer_starts,
        is_vertical,
        width,
        height,
    );

    if std::env::var("MMDFLUX_DEBUG_WAYPOINTS").is_ok_and(|v| v == "1") {
        for (key, waypoints) in &edge_waypoints_converted {
            eprintln!("[waypoints] {} -> {}: {:?}", key.0, key.1, waypoints);
        }
    }

    let edge_label_positions_converted = transform_label_positions_direct(
        &result.label_positions,
        &diagram.edges,
        &ctx,
        &layer_starts,
        is_vertical,
        width,
        height,
    );

    // --- Phase I: Strip dagre waypoints from backward edges ---
    // When ranks are doubled (labels present), backward edges get inflated dagre
    // waypoints from normalization dummies that create tall vertical columns.
    // Strip them so the router falls through to synthetic compact routing via
    // generate_backward_waypoints().
    let mut edge_waypoints_final = edge_waypoints_converted;
    const BACKWARD_WAYPOINT_STRIP_THRESHOLD: usize = 6;
    if ranks_doubled && is_vertical {
        for edge in &diagram.edges {
            let key = (edge.from.clone(), edge.to.clone());
            if let (Some(from_b), Some(to_b)) =
                (node_bounds.get(&edge.from), node_bounds.get(&edge.to))
                && crate::render::router::is_backward_edge(from_b, to_b, diagram.direction)
            {
                if edge_waypoints_final
                    .get(&key)
                    .is_some_and(|wps| wps.len() >= BACKWARD_WAYPOINT_STRIP_THRESHOLD)
                {
                    edge_waypoints_final.remove(&key);
                }
            }
        }
    }

    // --- Phase I.5: Nudge waypoints that collide with nodes ---
    nudge_colliding_waypoints(
        &mut edge_waypoints_final,
        &node_bounds,
        is_vertical,
        width,
        height,
    );

    // --- Phase J: Collect node shapes ---
    let node_shapes: HashMap<String, Shape> = diagram
        .nodes
        .iter()
        .map(|(id, node)| (id.clone(), node.shape))
        .collect();

    // --- Phase K: Convert subgraph bounds to draw coordinates ---
    let subgraph_bounds = dagre_subgraph_bounds_to_draw(
        &diagram.subgraphs,
        &result.subgraph_bounds,
        scale_x,
        scale_y,
        dagre_min_x,
        dagre_min_y,
        max_overhang_x,
        max_overhang_y,
        config,
    );
    debug_compare_subgraph_bounds(
        &diagram.subgraphs,
        &subgraph_bounds,
        &result.subgraph_bounds,
        scale_x,
        scale_y,
        dagre_min_x,
        dagre_min_y,
        max_overhang_x,
        max_overhang_y,
        config,
    );

    // --- Phase L: Compute self-edge loop paths in draw coordinates ---
    // We use node bounds directly rather than transforming dagre-space loop points,
    // because the dagre gap (1.0) would collapse to 0 after ASCII scaling.
    let self_edges: Vec<SelfEdgeDrawData> = result
        .self_edges
        .iter()
        .filter_map(|sel| {
            let bounds = node_bounds.get(&sel.node.0)?;
            let loop_extent = 3; // how far the loop extends beyond the node edge

            // Dagre.js places self-edge loops on the right face (TD/BT) or
            // bottom face (LR/RL), matching the "order" dimension where the
            // dummy node is placed after the self-edge node.
            let points = match dagre_direction {
                DagreDirection::TopBottom => {
                    // Loop on right face: exit top-right, loop right, enter bottom-right
                    let right = bounds.x + bounds.width;
                    let loop_x = right + loop_extent;
                    let top_y = bounds.y;
                    let bot_y = bounds.y + bounds.height - 1;
                    vec![
                        (right, top_y),  // exit right face at top
                        (loop_x, top_y), // go right
                        (loop_x, bot_y), // go down
                        (right, bot_y),  // enter right face at bottom
                    ]
                }
                DagreDirection::BottomTop => {
                    // Loop on right face: exit bottom-right, loop right, enter top-right
                    let right = bounds.x + bounds.width;
                    let loop_x = right + loop_extent;
                    let top_y = bounds.y;
                    let bot_y = bounds.y + bounds.height - 1;
                    vec![
                        (right, bot_y),  // exit right face at bottom
                        (loop_x, bot_y), // go right
                        (loop_x, top_y), // go up
                        (right, top_y),  // enter right face at top
                    ]
                }
                DagreDirection::LeftRight => {
                    // Loop on bottom face: exit bottom-right, loop down, enter bottom-left
                    let bot = bounds.y + bounds.height;
                    let loop_y = bot + loop_extent;
                    let left_x = bounds.x;
                    let right_x = bounds.x + bounds.width - 1;
                    vec![
                        (right_x, bot),    // exit bottom face at right
                        (right_x, loop_y), // go down
                        (left_x, loop_y),  // go left
                        (left_x, bot),     // enter bottom face at left
                    ]
                }
                DagreDirection::RightLeft => {
                    // Loop on bottom face: exit bottom-left, loop down, enter bottom-right
                    let bot = bounds.y + bounds.height;
                    let loop_y = bot + loop_extent;
                    let left_x = bounds.x;
                    let right_x = bounds.x + bounds.width - 1;
                    vec![
                        (left_x, bot),     // exit bottom face at left
                        (left_x, loop_y),  // go down
                        (right_x, loop_y), // go right
                        (right_x, bot),    // enter bottom face at right
                    ]
                }
            };

            Some(SelfEdgeDrawData {
                node_id: sel.node.0.clone(),
                edge_index: sel.edge_index,
                points,
            })
        })
        .collect();

    // Expand canvas to fit subgraph borders (which extend beyond member nodes)
    let mut width = width;
    let mut height = height;
    for sb in subgraph_bounds.values() {
        width = width.max(sb.x + sb.width + config.padding);
        height = height.max(sb.y + sb.height + config.padding);
    }

    // Expand canvas to fit self-edge loops
    for se in &self_edges {
        for &(x, y) in &se.points {
            width = width.max(x + config.padding + 1);
            height = height.max(y + config.padding + 1);
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
        edge_waypoints: edge_waypoints_final,
        edge_label_positions: edge_label_positions_converted,
        node_shapes,
        subgraph_bounds,
        self_edges,
    }
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

/// Compute per-axis ASCII scale factors for translating dagre float coordinates
/// to character grid positions.
///
/// Returns `(scale_x, scale_y)` where each factor maps dagre coordinate deltas
/// to ASCII character deltas along that axis.
///
/// For vertical layouts (TD/BT):
///   - scale_y (primary) = (max_h + v_spacing) / (max_h + rank_sep)
///   - scale_x (cross)   = (avg_w + h_spacing) / (avg_w + node_sep)
///
/// For horizontal layouts (LR/RL):
///   - scale_x (primary) = (max_w + h_spacing) / (max_w + rank_sep)
///   - scale_y (cross)   = (avg_h + v_spacing) / (avg_h + node_sep)
fn compute_ascii_scale_factors(
    node_dims: &HashMap<String, (usize, usize)>,
    rank_sep: f64,
    node_sep: f64,
    v_spacing: usize,
    h_spacing: usize,
    is_vertical: bool,
    ranks_doubled: bool,
) -> (f64, f64) {
    let (total_w, total_h, max_w, max_h, count) = node_dims.values().fold(
        (0usize, 0usize, 0usize, 0usize, 0usize),
        |(tw, th, mw, mh, c), &(w, h)| (tw + w, th + h, mw.max(w), mh.max(h), c + 1),
    );
    let count_f = count.max(1) as f64;
    let avg_w = total_w as f64 / count_f;
    let avg_h = total_h as f64 / count_f;

    if is_vertical {
        // When ranks are doubled, dagre positions nodes 2× further apart.
        // To compensate exactly, we need: eff_rs = max_h + 2 * rank_sep
        // This gives scale_primary_new = scale_primary_old / 2, so that
        // (2 * rank_sep) * scale_new = rank_sep * scale_old.
        let effective_rank_sep = if ranks_doubled {
            max_h as f64 + 2.0 * rank_sep
        } else {
            rank_sep
        };
        let scale_primary = (max_h as f64 + v_spacing as f64) / (max_h as f64 + effective_rank_sep);
        let scale_cross = (avg_w + h_spacing as f64) / (avg_w + node_sep);
        (scale_cross, scale_primary)
    } else {
        let effective_rank_sep = if ranks_doubled {
            max_w as f64 + 2.0 * rank_sep
        } else {
            rank_sep
        };
        let scale_primary = (max_w as f64 + h_spacing as f64) / (max_w as f64 + effective_rank_sep);
        let scale_cross = (avg_h + v_spacing as f64) / (avg_h + node_sep);
        (scale_primary, scale_cross)
    }
}

/// Enforce minimum spacing between adjacent nodes within each layer after
/// scaling and rounding.
///
/// Nodes are sorted by their cross-axis position within each layer, then
/// scanned left-to-right (or top-to-bottom for horizontal layouts). If any
/// adjacent pair overlaps or is too close, the later node is pushed forward.
/// This cascades: pushing node B may cause it to overlap C, which also gets pushed.
///
/// For vertical layouts (`is_vertical = true`), the cross-axis is X.
/// For horizontal layouts (`is_vertical = false`), the cross-axis is Y.
fn collision_repair(
    layers: &[Vec<String>],
    draw_positions: &mut HashMap<String, (usize, usize)>,
    node_dims: &HashMap<String, (usize, usize)>,
    is_vertical: bool,
    min_gap: usize,
) {
    for layer in layers {
        if layer.len() <= 1 {
            continue;
        }

        let mut sorted: Vec<String> = layer.clone();
        sorted.sort_by_key(|id| {
            let &(x, y) = &draw_positions[id];
            if is_vertical { x } else { y }
        });

        for i in 1..sorted.len() {
            let prev_id = &sorted[i - 1];
            let curr_id = &sorted[i];
            let &(pw, ph) = &node_dims[prev_id];
            let (prev_x, prev_y) = draw_positions[prev_id];
            let (curr_x, curr_y) = draw_positions[curr_id];

            if is_vertical {
                let min_x = prev_x + pw + min_gap;
                if curr_x < min_x {
                    draw_positions.insert(curr_id.clone(), (min_x, curr_y));
                }
            } else {
                let min_y = prev_y + ph + min_gap;
                if curr_y < min_y {
                    draw_positions.insert(curr_id.clone(), (curr_x, min_y));
                }
            }
        }
    }
}

/// Enforce minimum spacing between adjacent layers along the primary axis.
///
/// For vertical layouts, layers stack along Y; for horizontal, along X.
/// If the closest node in the next layer is too close to the farthest node
/// in the previous layer, shift the entire next layer (and all subsequent layers)
/// forward to maintain the minimum gap.
fn rank_gap_repair(
    layers: &[Vec<String>],
    draw_positions: &mut HashMap<String, (usize, usize)>,
    node_dims: &HashMap<String, (usize, usize)>,
    is_vertical: bool,
    min_gap: usize,
) {
    if layers.len() <= 1 {
        return;
    }

    for i in 1..layers.len() {
        // Find the maximum primary-axis extent of the previous layer
        let prev_max = layers[i - 1]
            .iter()
            .filter_map(|id| {
                let &(x, y) = draw_positions.get(id)?;
                let &(w, h) = node_dims.get(id)?;
                Some(if is_vertical { y + h } else { x + w })
            })
            .max()
            .unwrap_or(0);

        // Find the minimum primary-axis position in the current layer
        let curr_min = layers[i]
            .iter()
            .filter_map(|id| {
                let &(x, y) = draw_positions.get(id)?;
                Some(if is_vertical { y } else { x })
            })
            .min()
            .unwrap_or(0);

        let required = prev_max + min_gap;
        if curr_min < required {
            let shift = required - curr_min;
            // Shift all nodes in this layer and all subsequent layers
            for layer in &layers[i..] {
                for id in layer {
                    if let Some(&(x, y)) = draw_positions.get(id) {
                        let new_pos = if is_vertical {
                            (x, y + shift)
                        } else {
                            (x + shift, y)
                        };
                        draw_positions.insert(id.clone(), new_pos);
                    }
                }
            }
        }
    }
}

/// Intermediate result for a node's scaled center and dimensions, used between
/// the overhang-detection pass and the draw-position pass.
struct RawCenter {
    id: String,
    cx: usize,
    cy: usize,
    w: usize,
    h: usize,
}

/// Build a map from parent subgraph ID to list of direct child subgraph IDs.
#[cfg(test)]
fn build_children_map(
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for sg in subgraphs.values() {
        if let Some(ref parent_id) = sg.parent {
            children
                .entry(parent_id.clone())
                .or_default()
                .push(sg.id.clone());
        }
    }
    children
}

/// Convert subgraph member-node positions to draw-coordinate SubgraphBounds.
///
/// Uses inside-out (bottom-up) computation: leaf subgraphs first, then parents
/// expand to contain their children. This ensures proper nesting of bounds.
fn dagre_subgraph_bounds_to_draw(
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
    dagre_bounds: &HashMap<String, Rect>,
    scale_x: f64,
    scale_y: f64,
    dagre_min_x: f64,
    dagre_min_y: f64,
    max_overhang_x: usize,
    max_overhang_y: usize,
    config: &LayoutConfig,
) -> HashMap<String, SubgraphBounds> {
    let mut bounds: HashMap<String, SubgraphBounds> = HashMap::new();

    for (sg_id, rect) in dagre_bounds {
        let sg = match subgraphs.get(sg_id) {
            Some(sg) => sg,
            None => continue,
        };

        let (x0, y0) = dagre_to_draw_coords(
            rect.x,
            rect.y,
            scale_x,
            scale_y,
            dagre_min_x,
            dagre_min_y,
            max_overhang_x,
            max_overhang_y,
            config,
        );
        let (x1, y1) = dagre_to_draw_coords(
            rect.x + rect.width,
            rect.y + rect.height,
            scale_x,
            scale_y,
            dagre_min_x,
            dagre_min_y,
            max_overhang_x,
            max_overhang_y,
            config,
        );

        let mut final_x = x0;
        let mut final_width = x1.saturating_sub(x0);
        let final_height = y1.saturating_sub(y0);

        // Enforce title-width minimum: ┌─ Title ─┐
        // Overhead: 2 corners + "─ " prefix (2) + " ─" suffix (2) = 6
        let has_visible_title = !sg.title.trim().is_empty();
        let min_title_width = if has_visible_title {
            sg.title.len() + 6
        } else {
            0
        };
        if min_title_width > 0 && final_width < min_title_width {
            let expand = min_title_width - final_width;
            final_x = final_x.saturating_sub(expand / 2);
            final_width = min_title_width;
        }

        // Compute nesting depth by walking parent chain
        let mut depth = 0;
        let mut cur = sg_id.as_str();
        while let Some(s) = subgraphs.get(cur) {
            if let Some(ref p) = s.parent {
                depth += 1;
                cur = p;
            } else {
                break;
            }
        }

        bounds.insert(
            sg_id.clone(),
            SubgraphBounds {
                x: final_x,
                y: y0,
                width: final_width,
                height: final_height,
                title: sg.title.clone(),
                depth,
            },
        );
    }

    // Expand parent bounds to contain child bounds (inside-out).
    let mut ids: Vec<String> = bounds.keys().cloned().collect();
    ids.sort_by_key(|id| bounds.get(id).map(|b| b.depth).unwrap_or(0));
    ids.reverse();
    for id in ids {
        let parent_id = subgraphs
            .get(&id)
            .and_then(|sg| sg.parent.as_ref())
            .cloned();
        let (Some(parent_id), Some(child_bounds)) = (parent_id, bounds.get(&id).cloned()) else {
            continue;
        };
        let Some(parent_bounds) = bounds.get_mut(&parent_id) else {
            continue;
        };

        let pad = 1usize;
        let child_left = child_bounds.x.saturating_sub(pad);
        let child_top = child_bounds.y.saturating_sub(pad);
        let child_right = child_bounds.x + child_bounds.width + pad;
        let child_bottom = child_bounds.y + child_bounds.height + pad;
        let parent_right = parent_bounds.x + parent_bounds.width;
        let parent_bottom = parent_bounds.y + parent_bounds.height;

        let new_left = parent_bounds.x.min(child_left);
        let new_top = parent_bounds.y.min(child_top);
        let new_right = parent_right.max(child_right);
        let new_bottom = parent_bottom.max(child_bottom);

        parent_bounds.x = new_left;
        parent_bounds.y = new_top;
        parent_bounds.width = new_right.saturating_sub(new_left);
        parent_bounds.height = new_bottom.saturating_sub(new_top);
    }

    bounds
}

fn debug_compare_subgraph_bounds(
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
    computed: &HashMap<String, SubgraphBounds>,
    dagre_bounds: &HashMap<String, Rect>,
    scale_x: f64,
    scale_y: f64,
    dagre_min_x: f64,
    dagre_min_y: f64,
    max_overhang_x: usize,
    max_overhang_y: usize,
    config: &LayoutConfig,
) {
    if !std::env::var("MMDFLUX_DEBUG_SUBGRAPH_BOUNDS").is_ok_and(|v| v == "1") {
        return;
    }

    let mut ids: HashSet<String> = HashSet::new();
    ids.extend(subgraphs.keys().cloned());
    ids.extend(computed.keys().cloned());
    ids.extend(dagre_bounds.keys().cloned());

    eprintln!("[subgraph_bounds] comparing computed vs dagre-derived");
    let mut ids: Vec<String> = ids.into_iter().collect();
    ids.sort();
    for id in ids {
        let computed_bounds = computed.get(&id);
        let dagre_rect = dagre_bounds.get(&id);
        if computed_bounds.is_none() && dagre_rect.is_none() {
            continue;
        }

        if let Some(rect) = dagre_rect {
            eprintln!(
                "[subgraph_bounds] raw {} = ({:.2}, {:.2}, {:.2}, {:.2})",
                id, rect.x, rect.y, rect.width, rect.height
            );
        }

        let dagre_draw = dagre_rect.map(|rect| {
            let (x0, y0) = dagre_to_draw_coords(
                rect.x,
                rect.y,
                scale_x,
                scale_y,
                dagre_min_x,
                dagre_min_y,
                max_overhang_x,
                max_overhang_y,
                config,
            );
            let (x1, y1) = dagre_to_draw_coords(
                rect.x + rect.width,
                rect.y + rect.height,
                scale_x,
                scale_y,
                dagre_min_x,
                dagre_min_y,
                max_overhang_x,
                max_overhang_y,
                config,
            );
            (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
        });

        let computed_tuple = computed_bounds.map(|b| (b.x, b.y, b.width, b.height));

        eprintln!(
            "[subgraph_bounds] {} computed={:?} dagre={:?}",
            id, computed_tuple, dagre_draw
        );
    }
}

fn dagre_to_draw_coords(
    x: f64,
    y: f64,
    scale_x: f64,
    scale_y: f64,
    dagre_min_x: f64,
    dagre_min_y: f64,
    max_overhang_x: usize,
    max_overhang_y: usize,
    config: &LayoutConfig,
) -> (usize, usize) {
    let dx = ((x - dagre_min_x) * scale_x).round() as isize;
    let dy = ((y - dagre_min_y) * scale_y).round() as isize;
    let x = dx.max(0) as usize + max_overhang_x + config.padding + config.left_label_margin;
    let y = dy.max(0) as usize + max_overhang_y + config.padding;
    (x, y)
}

/// Check if `ancestor_id` is an ancestor of `descendant_id` in the subgraph hierarchy.
#[cfg(test)]
fn is_ancestor(
    ancestor_id: &str,
    descendant_id: &str,
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
) -> bool {
    let mut current = descendant_id;
    while let Some(sg) = subgraphs.get(current) {
        if let Some(ref parent) = sg.parent {
            if parent == ancestor_id {
                return true;
            }
            current = parent;
        } else {
            break;
        }
    }
    false
}

/// Resolve overlapping subgraph bounds by trimming borders at the midpoint
/// of the overlap region. For vertically stacked subgraphs, trims the upper's
/// bottom and the lower's top. For horizontally adjacent, trims left/right.
/// Skips nested pairs (ancestor/descendant) — only resolves sibling overlaps.
#[cfg(test)]
fn resolve_subgraph_overlap(
    bounds: &mut HashMap<String, SubgraphBounds>,
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
) {
    let ids: Vec<String> = bounds.keys().cloned().collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            // Skip nested pairs — parent/child overlap is intentional
            if is_ancestor(&ids[i], &ids[j], subgraphs) || is_ancestor(&ids[j], &ids[i], subgraphs)
            {
                continue;
            }
            let (a_x, a_y, a_right, a_bottom) = {
                let a = &bounds[&ids[i]];
                (a.x, a.y, a.x + a.width, a.y + a.height)
            };
            let (b_x, b_y, b_right, b_bottom) = {
                let b = &bounds[&ids[j]];
                (b.x, b.y, b.x + b.width, b.y + b.height)
            };

            // Check for overlap (both axes must overlap for a true 2D overlap)
            let x_overlap = a_x < b_right && b_x < a_right;
            let y_overlap = a_y < b_bottom && b_y < a_bottom;

            if !(x_overlap && y_overlap) {
                continue;
            }

            // Determine the primary overlap axis (the one with less overlap)
            let x_overlap_amount = a_right.min(b_right).saturating_sub(a_x.max(b_x));
            let y_overlap_amount = a_bottom.min(b_bottom).saturating_sub(a_y.max(b_y));

            if y_overlap_amount <= x_overlap_amount {
                // Vertical overlap: trim the gap between upper and lower
                let (upper_id, lower_id) = if a_y <= b_y {
                    (&ids[i], &ids[j])
                } else {
                    (&ids[j], &ids[i])
                };
                let upper_bottom = {
                    let u = &bounds[upper_id];
                    u.y + u.height
                };
                let lower_top = bounds[lower_id].y;

                if upper_bottom > lower_top {
                    // Split the overlap: place the boundary at the midpoint
                    let mid = lower_top + (upper_bottom - lower_top) / 2;
                    let gap = 1; // minimum 1-cell gap between borders

                    let upper = bounds.get_mut(upper_id).unwrap();
                    let new_upper_bottom = mid.saturating_sub(gap / 2);
                    if new_upper_bottom > upper.y {
                        upper.height = new_upper_bottom - upper.y;
                    }

                    let lower = bounds.get_mut(lower_id).unwrap();
                    let new_lower_top = mid + gap.div_ceil(2);
                    if new_lower_top < lower.y + lower.height {
                        let old_bottom = lower.y + lower.height;
                        lower.y = new_lower_top;
                        lower.height = old_bottom - new_lower_top;
                    }
                }
            } else {
                // Horizontal overlap: trim left/right
                let (left_id, right_id) = if a_x <= b_x {
                    (&ids[i], &ids[j])
                } else {
                    (&ids[j], &ids[i])
                };
                let left_right = {
                    let l = &bounds[left_id];
                    l.x + l.width
                };
                let right_left = bounds[right_id].x;

                if left_right > right_left {
                    let mid = right_left + (left_right - right_left) / 2;
                    let gap = 1;

                    let left = bounds.get_mut(left_id).unwrap();
                    let new_left_right = mid.saturating_sub(gap / 2);
                    if new_left_right > left.x {
                        left.width = new_left_right - left.x;
                    }

                    let right = bounds.get_mut(right_id).unwrap();
                    let new_right_left = mid + gap.div_ceil(2);
                    if new_right_left < right.x + right.width {
                        let old_right_edge = right.x + right.width;
                        right.x = new_right_left;
                        right.width = old_right_edge - new_right_left;
                    }
                }
            }
        }
    }
}

/// Ensure sibling subgraphs (same parent) have at least 1 cell of space between them.
/// After nudging, re-expand any parent whose children now exceed its bounds.
#[cfg(test)]
fn ensure_sibling_gap(
    bounds: &mut HashMap<String, SubgraphBounds>,
    subgraphs: &HashMap<String, crate::graph::Subgraph>,
) {
    let ids: Vec<String> = bounds.keys().cloned().collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            // Only process siblings (same parent)
            let parent_i = subgraphs.get(&ids[i]).and_then(|s| s.parent.as_deref());
            let parent_j = subgraphs.get(&ids[j]).and_then(|s| s.parent.as_deref());
            if parent_i != parent_j {
                continue;
            }

            let a = &bounds[&ids[i]];
            let b = &bounds[&ids[j]];

            // Check horizontal adjacency (touching or within 0 gap)
            let a_right = a.x + a.width;
            let b_right = b.x + b.width;
            let y_overlap = a.y < b.y + b.height && b.y < a.y + a.height;

            if y_overlap {
                if a_right == b.x {
                    let b_mut = bounds.get_mut(&ids[j]).unwrap();
                    b_mut.x += 1;
                } else if b_right == a.x {
                    let a_mut = bounds.get_mut(&ids[i]).unwrap();
                    a_mut.x += 1;
                }
            }

            // Check vertical adjacency
            let a = &bounds[&ids[i]];
            let b = &bounds[&ids[j]];
            let a_bottom = a.y + a.height;
            let b_bottom = b.y + b.height;
            let x_overlap = a.x < b.x + b.width && b.x < a.x + a.width;

            if x_overlap {
                if a_bottom == b.y {
                    let b_mut = bounds.get_mut(&ids[j]).unwrap();
                    b_mut.y += 1;
                } else if b_bottom == a.y {
                    let a_mut = bounds.get_mut(&ids[i]).unwrap();
                    a_mut.y += 1;
                }
            }
        }
    }

    // Re-expand parents to contain children after nudging
    let border_padding: usize = 2;
    for sg_id in subgraphs.keys() {
        if let Some(parent_bounds) = bounds.get(sg_id).cloned() {
            // Find all children of this subgraph
            let mut max_right = parent_bounds.x + parent_bounds.width;
            let mut max_bottom = parent_bounds.y + parent_bounds.height;
            let mut needs_expand = false;

            for (child_id, child_sg) in subgraphs {
                if child_sg.parent.as_deref() == Some(sg_id.as_str())
                    && let Some(child_b) = bounds.get(child_id)
                {
                    let child_right = child_b.x + child_b.width + border_padding;
                    let child_bottom = child_b.y + child_b.height + border_padding;
                    if child_right > max_right {
                        max_right = child_right;
                        needs_expand = true;
                    }
                    if child_bottom > max_bottom {
                        max_bottom = child_bottom;
                        needs_expand = true;
                    }
                }
            }

            if needs_expand {
                let p = bounds.get_mut(sg_id).unwrap();
                p.width = max_right - p.x;
                p.height = max_bottom - p.y;
            }
        }
    }
}

/// Nudge waypoints that overlap with node bounding boxes.
///
/// If a waypoint falls inside a node, push it just past the node's edge along the
/// cross-axis (X for vertical layouts, Y for horizontal). The waypoint is then
/// clamped to stay within canvas bounds.
fn nudge_colliding_waypoints(
    edge_waypoints: &mut HashMap<(String, String), Vec<(usize, usize)>>,
    node_bounds: &HashMap<String, NodeBounds>,
    is_vertical: bool,
    canvas_width: usize,
    canvas_height: usize,
) {
    for waypoints in edge_waypoints.values_mut() {
        for wp in waypoints.iter_mut() {
            for bounds in node_bounds.values() {
                if bounds.contains(wp.0, wp.1) {
                    if is_vertical {
                        wp.0 = bounds.x + bounds.width + 1;
                    } else {
                        wp.1 = bounds.y + bounds.height + 1;
                    }
                    break;
                }
            }
            wp.0 = wp.0.min(canvas_width.saturating_sub(1));
            wp.1 = wp.1.min(canvas_height.saturating_sub(1));
        }
    }
}

/// Shared parameters for transforming dagre coordinates to ASCII draw coordinates.
struct TransformContext {
    dagre_min_x: f64,
    dagre_min_y: f64,
    scale_x: f64,
    scale_y: f64,
    padding: usize,
    left_label_margin: usize,
    overhang_x: usize,
    overhang_y: usize,
}

impl TransformContext {
    /// Transform a dagre top-left-based Rect to draw coordinates (x, y, width, height).
    #[allow(dead_code)]
    ///
    /// Transforms the top-left and bottom-right corners independently using
    /// `to_ascii()`, then computes the draw rect between them. This ensures
    /// the transformed rect faithfully represents the dagre bounding box in
    /// draw space.
    fn to_ascii_rect(&self, rect: &Rect) -> (usize, usize, usize, usize) {
        let (x1, y1) = self.to_ascii(rect.x, rect.y);
        let (x2, y2) = self.to_ascii(rect.x + rect.width, rect.y + rect.height);
        let draw_x = x1.min(x2);
        let draw_y = y1.min(y2);
        let draw_w = x1.max(x2) - draw_x;
        let draw_h = y1.max(y2) - draw_y;
        (draw_x, draw_y, draw_w.max(1), draw_h.max(1))
    }

    /// Transform a dagre (x, y) coordinate to ASCII draw coordinates.
    fn to_ascii(&self, dagre_x: f64, dagre_y: f64) -> (usize, usize) {
        let x = ((dagre_x - self.dagre_min_x) * self.scale_x).round() as usize
            + self.overhang_x
            + self.padding
            + self.left_label_margin;
        let y = ((dagre_y - self.dagre_min_y) * self.scale_y).round() as usize
            + self.overhang_y
            + self.padding;
        (x, y)
    }
}

/// Transform dagre waypoints to ASCII draw coordinates using uniform scale factors.
///
/// The primary axis (Y for TD/BT, X for LR/RL) uses `layer_starts` to snap to
/// the correct rank position. The cross axis uses uniform scaling from dagre
/// coordinates, ensuring consistency with node positions.
fn transform_waypoints_direct(
    edge_waypoints: &HashMap<usize, Vec<WaypointWithRank>>,
    edges: &[Edge],
    ctx: &TransformContext,
    layer_starts: &[usize],
    is_vertical: bool,
    canvas_width: usize,
    canvas_height: usize,
) -> HashMap<(String, String), Vec<(usize, usize)>> {
    let mut converted = HashMap::new();

    for (edge_idx, waypoints) in edge_waypoints {
        if let Some(edge) = edges.get(*edge_idx) {
            let key = (edge.from.clone(), edge.to.clone());

            let wps: Vec<(usize, usize)> = waypoints
                .iter()
                .map(|wp| {
                    let rank_idx = wp.rank as usize;
                    let layer_pos = layer_starts.get(rank_idx).copied().unwrap_or(0);
                    let (scaled_x, scaled_y) = ctx.to_ascii(wp.point.x, wp.point.y);

                    if is_vertical {
                        (scaled_x.min(canvas_width.saturating_sub(1)), layer_pos)
                    } else {
                        (layer_pos, scaled_y.min(canvas_height.saturating_sub(1)))
                    }
                })
                .collect();

            converted.insert(key, wps);
        }
    }

    converted
}

/// Transform dagre label positions to ASCII draw coordinates.
///
/// The primary axis (Y for TD/BT, X for LR/RL) uses rank-based snapping via
/// `layer_starts[rank]`, matching how `transform_waypoints_direct()` works.
/// The cross axis uses uniform scaling from dagre coordinates.
fn transform_label_positions_direct(
    label_positions: &HashMap<usize, WaypointWithRank>,
    edges: &[Edge],
    ctx: &TransformContext,
    layer_starts: &[usize],
    is_vertical: bool,
    canvas_width: usize,
    canvas_height: usize,
) -> HashMap<(String, String), (usize, usize)> {
    let mut converted = HashMap::new();

    for (edge_idx, wp) in label_positions {
        if let Some(edge) = edges.get(*edge_idx) {
            let key = (edge.from.clone(), edge.to.clone());
            let rank_idx = wp.rank as usize;
            let layer_pos = layer_starts.get(rank_idx).copied().unwrap_or(0);
            let (scaled_x, scaled_y) = ctx.to_ascii(wp.point.x, wp.point.y);

            let pos = if is_vertical {
                (scaled_x.min(canvas_width.saturating_sub(1)), layer_pos)
            } else {
                (layer_pos, scaled_y.min(canvas_height.saturating_sub(1)))
            };
            converted.insert(key, pos);
        }
    }

    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Scale Factor Tests (Phase 2)
    // =========================================================================

    #[test]
    fn scale_factors_td_typical() {
        // Typical TD: 3 nodes with widths 9,7,11 and heights all 3
        // avg_w = 9.0, max_h = 3
        // rank_sep = 50.0, node_sep = 50.0, v_spacing = 3, h_spacing = 4
        // scale_y (primary) = (3 + 3) / (3 + 50) = 6/53
        // scale_x (cross)   = (9 + 4) / (9 + 50) = 13/59
        let mut dims = HashMap::new();
        dims.insert("A".into(), (9, 3));
        dims.insert("B".into(), (7, 3));
        dims.insert("C".into(), (11, 3));

        let (sx, sy) = compute_ascii_scale_factors(&dims, 50.0, 50.0, 3, 4, true, false);

        let expected_sy = 6.0 / 53.0;
        let expected_sx = 13.0 / 59.0;
        assert!(
            (sx - expected_sx).abs() < 1e-6,
            "sx: got {sx}, expected {expected_sx}"
        );
        assert!(
            (sy - expected_sy).abs() < 1e-6,
            "sy: got {sy}, expected {expected_sy}"
        );
    }

    #[test]
    fn scale_factors_lr_direction_aware() {
        // LR: nodes widths 9,9, heights 3,3 → avg_h = 3, max_w = 9
        // scale_x (primary) = (9 + 4) / (9 + 50) = 13/59
        // scale_y (cross)   = (3 + 3) / (3 + 6) = 6/9
        let mut dims = HashMap::new();
        dims.insert("A".into(), (9, 3));
        dims.insert("B".into(), (9, 3));

        let (sx, sy) = compute_ascii_scale_factors(&dims, 50.0, 6.0, 3, 4, false, false);

        let expected_sx = 13.0 / 59.0;
        let expected_sy = 6.0 / 9.0;
        assert!(
            (sx - expected_sx).abs() < 1e-6,
            "sx: got {sx}, expected {expected_sx}"
        );
        assert!(
            (sy - expected_sy).abs() < 1e-6,
            "sy: got {sy}, expected {expected_sy}"
        );
    }

    #[test]
    fn scale_factors_single_node() {
        let mut dims = HashMap::new();
        dims.insert("X".into(), (5, 3));

        let (sx, sy) = compute_ascii_scale_factors(&dims, 50.0, 50.0, 3, 4, true, false);
        assert!(sx > 0.0, "sx should be positive, got {sx}");
        assert!(sy > 0.0, "sy should be positive, got {sy}");
        assert!(sx.is_finite());
        assert!(sy.is_finite());
    }

    #[test]
    fn scale_factors_halved_for_doubled_ranks() {
        // With ranks_doubled=true, effective_rank_sep = max_h + 2*rank_sep = 3 + 100 = 103
        // scale_y = (max_h + v_spacing) / (max_h + eff_rs) = 6/106
        // This is exactly half of the non-doubled scale: 6/53 / 2 = 6/106
        let mut dims = HashMap::new();
        dims.insert("A".into(), (9, 3));
        dims.insert("B".into(), (7, 3));

        let (_, sy_normal) = compute_ascii_scale_factors(&dims, 50.0, 50.0, 3, 4, true, false);
        let (_, sy_doubled) = compute_ascii_scale_factors(&dims, 50.0, 50.0, 3, 4, true, true);

        // Doubled-rank scale should be exactly half of normal scale
        let expected_sy = sy_normal / 2.0;
        assert!(
            (sy_doubled - expected_sy).abs() < 1e-6,
            "sy_doubled: got {sy_doubled}, expected {expected_sy} (half of {sy_normal})"
        );

        // Verify: gap_new = 2*rank_sep*scale_doubled = gap_old = rank_sep*scale_normal
        let gap_normal = 50.0 * sy_normal;
        let gap_doubled = 100.0 * sy_doubled;
        assert!(
            (gap_normal - gap_doubled).abs() < 1e-6,
            "Gaps should match: normal={gap_normal}, doubled={gap_doubled}"
        );
    }

    #[test]
    fn scale_factors_empty_nodes() {
        let dims: HashMap<String, (usize, usize)> = HashMap::new();
        let (sx, sy) = compute_ascii_scale_factors(&dims, 50.0, 50.0, 3, 4, true, false);
        assert!(sx.is_finite());
        assert!(sy.is_finite());
    }

    // =========================================================================
    // Collision Repair Tests (Phase 3)
    // =========================================================================

    #[test]
    fn collision_repair_pushes_overlapping_nodes_apart() {
        let layers = vec![vec!["A".into(), "B".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (0, 0));
        positions.insert("B".into(), (5, 0));
        let dims: HashMap<String, (usize, usize)> =
            [("A".into(), (8, 3)), ("B".into(), (8, 3))].into();

        collision_repair(&layers, &mut positions, &dims, true, 4);

        assert_eq!(positions["A"], (0, 0), "A should not move");
        assert_eq!(positions["B"], (12, 0), "B pushed to right edge of A + gap");
    }

    #[test]
    fn collision_repair_cascading() {
        let layers = vec![vec!["A".into(), "B".into(), "C".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (0, 0));
        positions.insert("B".into(), (3, 0));
        positions.insert("C".into(), (8, 0));
        let dims: HashMap<String, (usize, usize)> = [
            ("A".into(), (6, 3)),
            ("B".into(), (6, 3)),
            ("C".into(), (6, 3)),
        ]
        .into();

        collision_repair(&layers, &mut positions, &dims, true, 2);

        assert_eq!(positions["A"], (0, 0));
        assert_eq!(positions["B"], (8, 0));
        assert_eq!(positions["C"], (16, 0));
    }

    #[test]
    fn collision_repair_no_change_when_spaced() {
        let layers = vec![vec!["A".into(), "B".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (0, 0));
        positions.insert("B".into(), (20, 0));
        let dims: HashMap<String, (usize, usize)> =
            [("A".into(), (8, 3)), ("B".into(), (8, 3))].into();

        collision_repair(&layers, &mut positions, &dims, true, 4);

        assert_eq!(positions["A"], (0, 0));
        assert_eq!(positions["B"], (20, 0));
    }

    #[test]
    fn collision_repair_horizontal_layout() {
        let layers = vec![vec!["A".into(), "B".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (0, 0));
        positions.insert("B".into(), (0, 2));
        let dims: HashMap<String, (usize, usize)> =
            [("A".into(), (8, 3)), ("B".into(), (8, 3))].into();

        collision_repair(&layers, &mut positions, &dims, false, 3);

        assert_eq!(positions["A"], (0, 0));
        assert_eq!(positions["B"], (0, 6));
    }

    #[test]
    fn collision_repair_single_node_layer_noop() {
        let layers = vec![vec!["A".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (5, 5));
        let dims: HashMap<String, (usize, usize)> = [("A".into(), (8, 3))].into();

        collision_repair(&layers, &mut positions, &dims, true, 4);

        assert_eq!(positions["A"], (5, 5));
    }

    #[test]
    fn collision_repair_sorts_by_cross_axis() {
        let layers = vec![vec!["A".into(), "B".into()]];
        let mut positions: HashMap<String, (usize, usize)> = HashMap::new();
        positions.insert("A".into(), (20, 0));
        positions.insert("B".into(), (0, 0));
        let dims: HashMap<String, (usize, usize)> =
            [("A".into(), (8, 3)), ("B".into(), (8, 3))].into();

        collision_repair(&layers, &mut positions, &dims, true, 4);

        assert_eq!(positions["B"], (0, 0));
        assert_eq!(positions["A"], (20, 0));
    }

    // =========================================================================
    // Waypoint Transform Tests (Phase 4)
    // =========================================================================

    #[test]
    fn waypoint_transform_vertical_basic() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "C".into(),
            label: None,
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut waypoints = HashMap::new();
        waypoints.insert(
            0usize,
            vec![WaypointWithRank {
                point: Point { x: 100.0, y: 75.0 },
                rank: 1,
            }],
        );

        let layer_starts = vec![1, 5, 9];
        let ctx = TransformContext {
            dagre_min_x: 50.0,
            dagre_min_y: 25.0,
            scale_x: 0.22,
            scale_y: 0.11,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let result =
            transform_waypoints_direct(&waypoints, &edges, &ctx, &layer_starts, true, 80, 20);

        let key = ("A".to_string(), "C".to_string());
        assert!(result.contains_key(&key), "should have waypoints for A→C");
        let wps = &result[&key];
        assert_eq!(wps.len(), 1);
        assert_eq!(wps[0].1, 5, "y should be layer_starts[1]");
        assert_eq!(wps[0].0, 12, "x should be scaled dagre x + padding");
    }

    #[test]
    fn waypoint_transform_horizontal_basic() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "C".into(),
            label: None,
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut waypoints = HashMap::new();
        waypoints.insert(
            0usize,
            vec![WaypointWithRank {
                point: Point { x: 75.0, y: 100.0 },
                rank: 1,
            }],
        );

        let layer_starts = vec![1, 8, 15];
        let ctx = TransformContext {
            dagre_min_x: 25.0,
            dagre_min_y: 50.0,
            scale_x: 0.22,
            scale_y: 0.67,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let result =
            transform_waypoints_direct(&waypoints, &edges, &ctx, &layer_starts, false, 40, 80);

        let key = ("A".to_string(), "C".to_string());
        let wps = &result[&key];
        assert_eq!(wps[0].0, 8, "x should be layer_starts[1]");
        assert_eq!(wps[0].1, 35, "y should be scaled dagre y + padding");
    }

    #[test]
    fn waypoint_transform_clamps_to_canvas() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "B".into(),
            label: None,
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut waypoints = HashMap::new();
        waypoints.insert(
            0usize,
            vec![WaypointWithRank {
                point: Point { x: 5000.0, y: 50.0 },
                rank: 0,
            }],
        );

        let layer_starts = vec![1];
        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.5,
            scale_y: 0.5,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let result =
            transform_waypoints_direct(&waypoints, &edges, &ctx, &layer_starts, true, 30, 20);

        let key = ("A".to_string(), "B".to_string());
        let wps = &result[&key];
        assert!(wps[0].0 <= 29, "x clamped to canvas_width - 1");
    }

    #[test]
    fn waypoint_transform_empty_input() {
        let edges: Vec<Edge> = vec![];
        let waypoints: HashMap<usize, Vec<WaypointWithRank>> = HashMap::new();
        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.2,
            scale_y: 0.1,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let result = transform_waypoints_direct(&waypoints, &edges, &ctx, &[], true, 80, 20);
        assert!(result.is_empty());
    }

    // =========================================================================
    // Label Transform Tests (Phase 5)
    // =========================================================================

    #[test]
    fn label_transform_basic_scaling() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "B".into(),
            label: Some("yes".into()),
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut labels = HashMap::new();
        labels.insert(
            0usize,
            WaypointWithRank {
                point: Point { x: 150.0, y: 100.0 },
                rank: 1,
            },
        );

        let ctx = TransformContext {
            dagre_min_x: 50.0,
            dagre_min_y: 50.0,
            scale_x: 0.22,
            scale_y: 0.11,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        // layer_starts: rank 0 → y=0, rank 1 → y=8, rank 2 → y=16
        let layer_starts = vec![0, 8, 16];
        let result =
            transform_label_positions_direct(&labels, &edges, &ctx, &layer_starts, true, 50, 20);

        let key = ("A".to_string(), "B".to_string());
        assert!(result.contains_key(&key));
        // x uses uniform scale: (150-50)*0.22 + 1 = 23
        // y = layer_starts[rank=1] = 8
        assert_eq!(result[&key], (23, 8));
    }

    #[test]
    fn label_transform_with_left_margin() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "B".into(),
            label: Some("yes".into()),
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut labels = HashMap::new();
        labels.insert(
            0usize,
            WaypointWithRank {
                point: Point { x: 150.0, y: 100.0 },
                rank: 1,
            },
        );

        let ctx = TransformContext {
            dagre_min_x: 50.0,
            dagre_min_y: 50.0,
            scale_x: 0.22,
            scale_y: 0.11,
            padding: 1,
            left_label_margin: 3,
            overhang_x: 0,
            overhang_y: 0,
        };
        let layer_starts = vec![0, 8, 16];
        let result =
            transform_label_positions_direct(&labels, &edges, &ctx, &layer_starts, true, 50, 20);

        let key = ("A".to_string(), "B".to_string());
        // x = 23 + 3 (left_label_margin) = 26
        assert_eq!(result[&key].0, 26);
    }

    #[test]
    fn label_transform_empty_input() {
        let edges: Vec<Edge> = vec![];
        let labels: HashMap<usize, WaypointWithRank> = HashMap::new();
        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.2,
            scale_y: 0.1,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let layer_starts: Vec<usize> = vec![];
        let result =
            transform_label_positions_direct(&labels, &edges, &ctx, &layer_starts, true, 50, 20);
        assert!(result.is_empty());
    }

    // =========================================================================
    // Compound Graph Wiring Tests
    // =========================================================================

    #[test]
    fn test_layout_subgraph_bounds_present() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        assert!(
            layout.subgraph_bounds.contains_key("sg1"),
            "should have bounds for sg1"
        );
        let bounds = &layout.subgraph_bounds["sg1"];
        assert!(bounds.width > 0, "width should be positive");
        assert!(bounds.height > 0, "height should be positive");
        assert_eq!(bounds.title, "Group");
    }

    #[test]
    fn test_nested_subgraph_layout_produces_both_bounds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph outer[Outer]\nA[Node A]\nsubgraph inner[Inner]\nB[Node B]\nend\nend\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        assert!(
            layout.subgraph_bounds.contains_key("outer"),
            "should have outer bounds"
        );
        assert!(
            layout.subgraph_bounds.contains_key("inner"),
            "should have inner bounds"
        );
    }

    #[test]
    fn test_layout_no_subgraph_bounds_simple() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        assert!(layout.subgraph_bounds.is_empty());
    }

    #[test]
    fn test_layout_canvas_dimensions_include_borders() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        let bounds = &layout.subgraph_bounds["sg1"];
        assert!(
            layout.width >= bounds.x + bounds.width,
            "canvas width {} should contain border x+w={}",
            layout.width,
            bounds.x + bounds.width
        );
        assert!(
            layout.height >= bounds.y + bounds.height,
            "canvas height {} should contain border y+h={}",
            layout.height,
            bounds.y + bounds.height
        );
    }

    #[test]
    fn test_compute_layout_subgraph_diagram_succeeds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\nC --> A\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        // Should not panic
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
        assert!(layout.draw_positions.contains_key("C"));
    }

    #[test]
    fn test_compute_layout_simple_diagram_no_compound() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        assert!(!diagram.has_subgraphs());

        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        assert!(layout.draw_positions.contains_key("A"));
    }

    #[test]
    fn label_position_within_canvas_bounds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\n    A -->|yes| B";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        // Label position should exist
        let key = ("A".to_string(), "B".to_string());
        assert!(
            layout.edge_label_positions.contains_key(&key),
            "Should have precomputed label position for A->B, got keys: {:?}",
            layout.edge_label_positions.keys().collect::<Vec<_>>()
        );

        let (lx, ly) = layout.edge_label_positions[&key];
        // Should be within canvas bounds
        assert!(
            lx < layout.width && ly < layout.height,
            "Label position ({}, {}) should be within canvas ({}, {})",
            lx,
            ly,
            layout.width,
            layout.height
        );
    }

    #[test]
    fn label_transform_skips_missing_edge() {
        use crate::graph::{Arrow, Stroke};
        let edges = vec![Edge {
            from: "A".into(),
            to: "B".into(),
            label: Some("x".into()),
            stroke: Stroke::Solid,
            arrow: Arrow::Normal,
        }];

        let mut labels = HashMap::new();
        labels.insert(
            5usize,
            WaypointWithRank {
                point: Point { x: 100.0, y: 100.0 },
                rank: 0,
            },
        );

        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.2,
            scale_y: 0.1,
            padding: 1,
            left_label_margin: 0,
            overhang_x: 0,
            overhang_y: 0,
        };
        let layer_starts = vec![0];
        let result =
            transform_label_positions_direct(&labels, &edges, &ctx, &layer_starts, true, 50, 20);

        assert!(
            result.is_empty(),
            "out-of-bounds edge index should be skipped"
        );
    }

    // =========================================================================
    // Nested Subgraph Tests (Plan 0032)
    // =========================================================================

    #[test]
    fn test_nested_borders_inner_visible() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;
        use crate::render::{RenderOptions, render};

        let input = "graph TD\nsubgraph outer[Outer]\nA\nsubgraph inner[Inner]\nB --> C\nend\nend\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &RenderOptions::default());
        assert!(
            output.contains("Outer"),
            "Output should contain 'Outer' title"
        );
        assert!(
            output.contains("Inner"),
            "Output should contain 'Inner' title"
        );
    }

    #[test]
    fn test_nested_subgraph_depth_values() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph outer[Outer]\nA\nsubgraph inner[Inner]\nB\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        assert_eq!(layout.subgraph_bounds["outer"].depth, 0);
        assert_eq!(layout.subgraph_bounds["inner"].depth, 1);
    }

    #[test]
    fn test_nested_subgraph_parent_contains_child_bounds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph outer[Outer]\nA\nsubgraph inner[Inner]\nB --> C\nend\nend\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        let outer = &layout.subgraph_bounds["outer"];
        let inner = &layout.subgraph_bounds["inner"];
        // Parent must fully contain child
        assert!(
            outer.x <= inner.x,
            "outer.x ({}) should be <= inner.x ({})",
            outer.x,
            inner.x
        );
        assert!(
            outer.y <= inner.y,
            "outer.y ({}) should be <= inner.y ({})",
            outer.y,
            inner.y
        );
        assert!(
            outer.x + outer.width >= inner.x + inner.width,
            "outer right ({}) should be >= inner right ({})",
            outer.x + outer.width,
            inner.x + inner.width
        );
        assert!(
            outer.y + outer.height >= inner.y + inner.height,
            "outer bottom ({}) should be >= inner bottom ({})",
            outer.y + outer.height,
            inner.y + inner.height
        );
    }

    #[test]
    fn test_nested_outer_only_subgraph_gets_bounds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA --> B\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
        assert!(
            layout.subgraph_bounds.contains_key("outer"),
            "outer should have bounds"
        );
        let outer = &layout.subgraph_bounds["outer"];
        assert!(outer.width > 0, "width should be positive");
        assert!(outer.height > 0, "height should be positive");
    }

    #[test]
    fn test_build_children_map() {
        use crate::graph::Subgraph;
        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "inner".to_string(),
            Subgraph {
                id: "inner".to_string(),
                title: "Inner".to_string(),
                nodes: vec!["A".to_string()],
                parent: Some("outer".to_string()),
            },
        );
        subgraphs.insert(
            "outer".to_string(),
            Subgraph {
                id: "outer".to_string(),
                title: "Outer".to_string(),
                nodes: vec!["A".to_string()],
                parent: None,
            },
        );
        let children_map = build_children_map(&subgraphs);
        assert_eq!(children_map["outer"], vec!["inner".to_string()]);
        assert!(!children_map.contains_key("inner"));
    }

    // =========================================================================
    // Subgraph Bounds Tests (Dagre-derived bounds)
    // =========================================================================

    #[test]
    fn test_subgraph_bounds_no_overlap_from_separated_dagre_rects() {
        use crate::graph::Subgraph;

        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "sg1".to_string(),
            Subgraph {
                id: "sg1".to_string(),
                title: "Left".to_string(),

                nodes: vec!["A".to_string()],
                parent: None,
            },
        );
        subgraphs.insert(
            "sg2".to_string(),
            Subgraph {
                id: "sg2".to_string(),
                title: "Right".to_string(),

                nodes: vec!["B".to_string()],
                parent: None,
            },
        );

        let mut dagre_bounds = HashMap::new();
        dagre_bounds.insert(
            "sg1".to_string(),
            Rect {
                x: 10.0,
                y: 10.0,
                width: 10.0,
                height: 5.0,
            },
        );
        dagre_bounds.insert(
            "sg2".to_string(),
            Rect {
                x: 40.0,
                y: 10.0,
                width: 10.0,
                height: 5.0,
            },
        );

        let config = LayoutConfig {
            padding: 0,
            left_label_margin: 0,
            ..LayoutConfig::default()
        };

        let result = dagre_subgraph_bounds_to_draw(
            &subgraphs,
            &dagre_bounds,
            1.0,
            1.0,
            0.0,
            0.0,
            0,
            0,
            &config,
        );

        let a = &result["sg1"];
        let b = &result["sg2"];

        // Separated member nodes should produce non-overlapping draw bounds
        let no_x_overlap = a.x + a.width <= b.x || b.x + b.width <= a.x;
        let no_y_overlap = a.y + a.height <= b.y || b.y + b.height <= a.y;
        assert!(
            no_x_overlap || no_y_overlap,
            "Bounds should not overlap: sg1=({},{} {}x{}) sg2=({},{} {}x{})",
            a.x,
            a.y,
            a.width,
            a.height,
            b.x,
            b.y,
            b.width,
            b.height
        );
    }

    #[test]
    fn test_dagre_subgraph_bounds_maps_rects() {
        use crate::graph::Subgraph;

        let mut subgraphs = HashMap::new();
        subgraphs.insert(
            "sg1".to_string(),
            Subgraph {
                id: "sg1".to_string(),
                title: "G".to_string(),

                nodes: vec!["A".to_string()],
                parent: None,
            },
        );

        let mut dagre_bounds = HashMap::new();
        dagre_bounds.insert(
            "sg1".to_string(),
            Rect {
                x: 10.0,
                y: 10.0,
                width: 5.0,
                height: 3.0,
            },
        );

        let config = LayoutConfig {
            padding: 0,
            left_label_margin: 0,
            ..LayoutConfig::default()
        };

        let result = dagre_subgraph_bounds_to_draw(
            &subgraphs,
            &dagre_bounds,
            1.0,
            1.0,
            0.0,
            0.0,
            0,
            0,
            &config,
        );

        let b = &result["sg1"];
        // Title "G" requires min width = len("G") + 6 = 7, which exceeds rect width 5.
        // Title-width enforcement expands by (7-5)=2 and shifts x left by 2/2=1.
        assert_eq!(b.x, 9, "x shifted left by 1 due to title-width expansion");
        assert_eq!(b.y, 10, "y should match dagre rect y");
        assert_eq!(b.width, 7, "width expanded to fit title");
        assert_eq!(b.height, 3, "height should match dagre rect height");
    }

    // =========================================================================
    // Title Width Enforcement Tests (Plan 0026, Task 2.3)
    // =========================================================================

    #[test]
    fn test_subgraph_bounds_expanded_for_title() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph sg1[This Is A Very Long Title]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        let bounds = layout
            .subgraph_bounds
            .values()
            .next()
            .expect("Expected subgraph bounds");

        // Border must be wide enough for: corners (2) + "─ " (2) + title + " ─" (2)
        let min_width = "This Is A Very Long Title".len() + 6;
        assert!(
            bounds.width >= min_width,
            "Border width {} too narrow for title (need >= {})",
            bounds.width,
            min_width
        );
    }

    #[test]
    fn test_titled_subgraph_creates_title_rank() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = r#"graph TD
    subgraph sg1[Processing]
        A[Step 1] --> B[Step 2]
    end"#;

        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        assert!(layout.subgraph_bounds.contains_key("sg1"));
        let bounds = &layout.subgraph_bounds["sg1"];
        assert!(bounds.height > 0);
    }

    // =========================================================================
    // to_ascii_rect() Tests (Plan 0028, Task 1.1)
    // =========================================================================

    #[test]
    fn to_ascii_rect_at_dagre_minimum() {
        // A rect centered at the dagre minimum should produce draw coords near origin + padding
        let ctx = TransformContext {
            dagre_min_x: 50.0,
            dagre_min_y: 30.0,
            scale_x: 0.2,
            scale_y: 0.1,
            overhang_x: 2,
            overhang_y: 1,
            padding: 1,
            left_label_margin: 0,
        };
        let rect = Rect {
            x: 50.0,
            y: 30.0,
            width: 40.0,
            height: 20.0,
        };
        let (_x, _y, w, h) = ctx.to_ascii_rect(&rect);
        assert!(w > 0, "width should be positive, got {w}");
        assert!(h > 0, "height should be positive, got {h}");
    }

    #[test]
    fn to_ascii_rect_offset_from_minimum() {
        // A rect offset from dagre minimum should have proportionally offset draw coords
        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.2,
            scale_y: 0.1,
            overhang_x: 0,
            overhang_y: 0,
            padding: 0,
            left_label_margin: 0,
        };
        let rect1 = Rect {
            x: 50.0,
            y: 50.0,
            width: 40.0,
            height: 20.0,
        };
        let rect2 = Rect {
            x: 100.0,
            y: 100.0,
            width: 40.0,
            height: 20.0,
        };
        let (x1, y1, _, _) = ctx.to_ascii_rect(&rect1);
        let (x2, y2, _, _) = ctx.to_ascii_rect(&rect2);
        assert!(x2 > x1, "rect2 should be further right: x2={x2} vs x1={x1}");
        assert!(y2 > y1, "rect2 should be further down: y2={y2} vs y1={y1}");
    }

    #[test]
    fn to_ascii_rect_dimensions_scale_with_dagre_size() {
        let ctx = TransformContext {
            dagre_min_x: 0.0,
            dagre_min_y: 0.0,
            scale_x: 0.5,
            scale_y: 0.5,
            overhang_x: 0,
            overhang_y: 0,
            padding: 0,
            left_label_margin: 0,
        };
        let small = Rect {
            x: 50.0,
            y: 50.0,
            width: 20.0,
            height: 10.0,
        };
        let large = Rect {
            x: 50.0,
            y: 50.0,
            width: 60.0,
            height: 30.0,
        };
        let (_, _, w1, h1) = ctx.to_ascii_rect(&small);
        let (_, _, w2, h2) = ctx.to_ascii_rect(&large);
        assert!(
            w2 > w1,
            "larger rect should have larger width: w2={w2} vs w1={w1}"
        );
        assert!(
            h2 > h1,
            "larger rect should have larger height: h2={h2} vs h1={h1}"
        );
    }

    // =========================================================================
    // Non-overlap Tests (Plan 0028, Task 2.1)
    // =========================================================================

    #[test]
    fn stacked_subgraphs_do_not_overlap() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\n\
            subgraph sg1[Input]\nA[Data]\nB[Config]\nend\n\
            subgraph sg2[Output]\nC[Result]\nD[Log]\nend\n\
            A --> C\nB --> D";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        let sg1 = &layout.subgraph_bounds["sg1"];
        let sg2 = &layout.subgraph_bounds["sg2"];

        let sg1_bottom = sg1.y + sg1.height;
        let sg2_bottom = sg2.y + sg2.height;

        // Determine which is "upper" and which is "lower"
        let (_upper, lower, upper_bottom) = if sg1.y < sg2.y {
            (sg1, sg2, sg1_bottom)
        } else {
            (sg2, sg1, sg2_bottom)
        };

        // Upper subgraph's bottom must be strictly above lower's top
        assert!(
            upper_bottom <= lower.y,
            "Subgraphs should not overlap vertically: upper bottom={upper_bottom}, lower top={}",
            lower.y
        );
    }

    // =========================================================================
    // Containment Tests (Plan 0028, Task 1.2)
    // =========================================================================

    #[test]
    fn subgraph_bounds_contain_member_node_bounds() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\nsubgraph sg1[Group]\nA[Node1]\nB[Node2]\nend\nA --> B";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        assert_subgraph_contains_members(&layout, "sg1", &["A", "B"]);
    }

    #[test]
    fn stacked_subgraph_bounds_contain_member_nodes_after_overlap_resolution() {
        use crate::graph::build_diagram;
        use crate::parser::parse_flowchart;

        let input = "graph TD\n\
            subgraph sg1[Input]\nA[Data]\nB[Config]\nend\n\
            subgraph sg2[Output]\nC[Result]\nD[Log]\nend\n\
            A --> C\nB --> D";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let layout = compute_layout_direct(&diagram, &LayoutConfig::default());

        assert_subgraph_contains_members(&layout, "sg1", &["A", "B"]);
        assert_subgraph_contains_members(&layout, "sg2", &["C", "D"]);
    }

    fn assert_subgraph_contains_members(layout: &Layout, sg_id: &str, members: &[&str]) {
        let sg = &layout.subgraph_bounds[sg_id];
        let sg_right = sg.x + sg.width;
        let sg_bottom = sg.y + sg.height;

        for member_id in members {
            let nb = &layout.node_bounds[*member_id];
            let nb_right = nb.x + nb.width;
            let nb_bottom = nb.y + nb.height;

            assert!(
                sg.x <= nb.x,
                "{sg_id} left ({}) should be <= {member_id} left ({})",
                sg.x,
                nb.x
            );
            assert!(
                sg.y <= nb.y,
                "{sg_id} top ({}) should be <= {member_id} top ({})",
                sg.y,
                nb.y
            );
            assert!(
                sg_right >= nb_right,
                "{sg_id} right ({sg_right}) should be >= {member_id} right ({nb_right})"
            );
            assert!(
                sg_bottom >= nb_bottom,
                "{sg_id} bottom ({sg_bottom}) should be >= {member_id} bottom ({nb_bottom})"
            );
        }
    }
}
