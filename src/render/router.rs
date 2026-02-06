//! Edge routing between nodes.
//!
//! Computes paths for edges, avoiding node boundaries.

use std::collections::HashMap;

use super::intersect::{
    NodeFace, calculate_attachment_points, classify_face, spread_points_on_face,
};
use super::layout::{Layout, SelfEdgeDrawData, SubgraphBounds};
use super::shape::NodeBounds;
use crate::graph::{Direction, Edge, Shape, Stroke};

/// Map from (node_id, face) to the edges attached at that face.
/// Each entry is `(edge_index, is_source_side, approach_cross_axis)`.
type FaceGroupMap = HashMap<(String, NodeFace), Vec<(usize, bool, usize, bool)>>;

/// Grouped endpoint parameters for edge routing functions.
struct EdgeEndpoints {
    from_bounds: NodeBounds,
    from_shape: Shape,
    to_bounds: NodeBounds,
    to_shape: Shape,
}

fn subgraph_edge_face(bounds: &NodeBounds, other: &NodeBounds, direction: Direction) -> NodeFace {
    let bounds_right = bounds.x + bounds.width.saturating_sub(1);
    let bounds_bottom = bounds.y + bounds.height.saturating_sub(1);
    let other_right = other.x + other.width.saturating_sub(1);
    let other_bottom = other.y + other.height.saturating_sub(1);

    match direction {
        Direction::TopDown | Direction::BottomTop => {
            if other_bottom < bounds.y {
                return NodeFace::Top;
            }
            if other.y > bounds_bottom {
                return NodeFace::Bottom;
            }
            if other_right < bounds.x {
                return NodeFace::Left;
            }
            if other.x > bounds_right {
                return NodeFace::Right;
            }
        }
        Direction::LeftRight | Direction::RightLeft => {
            if other_right < bounds.x {
                return NodeFace::Left;
            }
            if other.x > bounds_right {
                return NodeFace::Right;
            }
            if other_bottom < bounds.y {
                return NodeFace::Top;
            }
            if other.y > bounds_bottom {
                return NodeFace::Bottom;
            }
        }
    }

    classify_face(
        bounds,
        (other.center_x(), other.center_y()),
        Shape::Rectangle,
    )
}

fn subgraph_bounds_as_node(bounds: &SubgraphBounds) -> NodeBounds {
    NodeBounds {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
        dagre_center_x: None,
        dagre_center_y: None,
    }
}

fn resolve_edge_bounds(layout: &Layout, edge: &Edge) -> Option<(NodeBounds, NodeBounds)> {
    let from_bounds = if let Some(sg_id) = edge.from_subgraph.as_ref() {
        layout
            .subgraph_bounds
            .get(sg_id)
            .map(subgraph_bounds_as_node)?
    } else {
        *layout.get_bounds(&edge.from)?
    };
    let to_bounds = if let Some(sg_id) = edge.to_subgraph.as_ref() {
        layout
            .subgraph_bounds
            .get(sg_id)
            .map(subgraph_bounds_as_node)?
    } else {
        *layout.get_bounds(&edge.to)?
    };
    Some((from_bounds, to_bounds))
}

fn bounds_for_node_id(layout: &Layout, node_id: &str) -> Option<NodeBounds> {
    if let Some(bounds) = layout.get_bounds(node_id) {
        return Some(*bounds);
    }
    layout
        .subgraph_bounds
        .get(node_id)
        .map(subgraph_bounds_as_node)
}

/// A point on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    pub fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// A segment of an edge path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    /// Vertical line from start to end (same x, different y).
    Vertical {
        x: usize,
        y_start: usize,
        y_end: usize,
    },
    /// Horizontal line from start to end (same y, different x).
    Horizontal {
        y: usize,
        x_start: usize,
        x_end: usize,
    },
}

impl Segment {
    /// Manhattan length of this segment.
    pub fn length(&self) -> usize {
        match self {
            Segment::Vertical { y_start, y_end, .. } => y_start.abs_diff(*y_end),
            Segment::Horizontal { x_start, x_end, .. } => x_start.abs_diff(*x_end),
        }
    }

    /// Start point of this segment.
    pub fn start_point(&self) -> Point {
        match self {
            Segment::Vertical { x, y_start, .. } => Point { x: *x, y: *y_start },
            Segment::Horizontal { y, x_start, .. } => Point { x: *x_start, y: *y },
        }
    }

    /// End point of this segment.
    pub fn end_point(&self) -> Point {
        match self {
            Segment::Vertical { x, y_end, .. } => Point { x: *x, y: *y_end },
            Segment::Horizontal { y, x_end, .. } => Point { x: *x_end, y: *y },
        }
    }

    /// Point at a given offset from start along the segment direction.
    /// Clamps to segment bounds if offset exceeds length.
    pub fn point_at_offset(&self, offset: usize) -> Point {
        match self {
            Segment::Vertical { x, y_start, y_end } => {
                let clamped = offset.min(y_start.abs_diff(*y_end));
                let y = if *y_end >= *y_start {
                    y_start + clamped
                } else {
                    y_start - clamped
                };
                Point { x: *x, y }
            }
            Segment::Horizontal { y, x_start, x_end } => {
                let clamped = offset.min(x_start.abs_diff(*x_end));
                let x = if *x_end >= *x_start {
                    x_start + clamped
                } else {
                    x_start - clamped
                };
                Point { x, y: *y }
            }
        }
    }
}

/// A complete routed path for an edge.
#[derive(Debug, Clone)]
pub struct RoutedEdge {
    /// The edge this path represents.
    pub edge: Edge,
    /// Start point (attachment point on source node).
    pub start: Point,
    /// End point (attachment point on target node).
    pub end: Point,
    /// Path segments from start to end.
    pub segments: Vec<Segment>,
    /// Direction from which the edge enters the target node (for arrow drawing).
    pub entry_direction: AttachDirection,
    /// Whether this edge goes backward in the layout direction.
    pub is_backward: bool,
    /// Whether this is a self-edge (source == target).
    pub is_self_edge: bool,
}

/// Direction for attachment points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachDirection {
    Top,
    Bottom,
    Left,
    Right,
}

/// Get the outgoing and incoming attachment directions based on diagram direction.
#[cfg(test)]
fn attachment_directions(diagram_direction: Direction) -> (AttachDirection, AttachDirection) {
    match diagram_direction {
        Direction::TopDown => (AttachDirection::Bottom, AttachDirection::Top),
        Direction::BottomTop => (AttachDirection::Top, AttachDirection::Bottom),
        Direction::LeftRight => (AttachDirection::Right, AttachDirection::Left),
        Direction::RightLeft => (AttachDirection::Left, AttachDirection::Right),
    }
}

/// Check if an edge is a backward edge (goes against the layout direction).
///
/// In a normal layout, edges flow in the diagram direction (e.g., top to bottom for TD).
/// A backward edge goes against this flow, typically creating a cycle.
pub fn is_backward_edge(
    from_bounds: &NodeBounds,
    to_bounds: &NodeBounds,
    direction: Direction,
) -> bool {
    // Backward means target is "before" source in the flow direction
    match direction {
        Direction::TopDown => to_bounds.y < from_bounds.y,
        Direction::BottomTop => to_bounds.y > from_bounds.y,
        Direction::LeftRight => to_bounds.x < from_bounds.x,
        Direction::RightLeft => to_bounds.x > from_bounds.x,
    }
}

/// Gap between node boundary and synthetic backward-edge waypoint path (in cells).
pub const BACKWARD_ROUTE_GAP: usize = 2;

/// Generate synthetic waypoints for a single-rank-span backward edge.
///
/// Routes around the right side (TD/BT) or bottom side (LR/RL) of the nodes
/// when no dagre-assigned waypoints exist.
///
/// Returns empty vec for forward edges or same-position nodes.
pub fn generate_backward_waypoints(
    src_bounds: &NodeBounds,
    tgt_bounds: &NodeBounds,
    direction: Direction,
) -> Vec<(usize, usize)> {
    if !is_backward_edge(src_bounds, tgt_bounds, direction) {
        return vec![];
    }

    match direction {
        Direction::TopDown | Direction::BottomTop => {
            // Route to the right of both nodes
            let right_edge = (src_bounds.x + src_bounds.width).max(tgt_bounds.x + tgt_bounds.width);
            let route_x = right_edge + BACKWARD_ROUTE_GAP;
            vec![
                (route_x, src_bounds.center_y()),
                (route_x, tgt_bounds.center_y()),
            ]
        }
        Direction::LeftRight => {
            // Route below both nodes; backward edge flows right-to-left
            let bottom_edge =
                (src_bounds.y + src_bounds.height).max(tgt_bounds.y + tgt_bounds.height);
            let route_y = bottom_edge + BACKWARD_ROUTE_GAP;
            let right_edge = (src_bounds.x + src_bounds.width).max(tgt_bounds.x + tgt_bounds.width);
            vec![
                (src_bounds.x.saturating_sub(1), route_y),
                (right_edge + BACKWARD_ROUTE_GAP, route_y),
            ]
        }
        Direction::RightLeft => {
            // Route below both nodes; backward edge flows left-to-right
            let bottom_edge =
                (src_bounds.y + src_bounds.height).max(tgt_bounds.y + tgt_bounds.height);
            let route_y = bottom_edge + BACKWARD_ROUTE_GAP;
            let left_edge = src_bounds.x.min(tgt_bounds.x);
            vec![
                (src_bounds.x + src_bounds.width, route_y),
                (left_edge.saturating_sub(BACKWARD_ROUTE_GAP), route_y),
            ]
        }
    }
}

/// Return the faces used by synthetic backward-edge routing.
///
/// Synthetic waypoints route around the right side (TD/BT) or bottom side
/// (LR/RL) of nodes, so both source and target attach on the same face.
fn backward_routing_faces(direction: Direction) -> (NodeFace, NodeFace) {
    match direction {
        Direction::TopDown | Direction::BottomTop => (NodeFace::Right, NodeFace::Right),
        Direction::LeftRight | Direction::RightLeft => (NodeFace::Bottom, NodeFace::Bottom),
    }
}

/// Route an edge between two nodes.
pub fn route_edge(
    edge: &Edge,
    layout: &Layout,
    diagram_direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
    src_first_vertical: bool,
) -> Option<RoutedEdge> {
    let (from_bounds, to_bounds) = resolve_edge_bounds(layout, edge)?;

    // Get node shapes for intersection calculation
    let from_shape = if edge.from_subgraph.is_some() {
        Shape::Rectangle
    } else {
        layout
            .node_shapes
            .get(&edge.from)
            .copied()
            .unwrap_or(Shape::Rectangle)
    };
    let to_shape = if edge.to_subgraph.is_some() {
        Shape::Rectangle
    } else {
        layout
            .node_shapes
            .get(&edge.to)
            .copied()
            .unwrap_or(Shape::Rectangle)
    };

    let endpoints = EdgeEndpoints {
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
    };

    // Check for waypoints from normalization — works for both forward and backward long edges
    let allow_waypoints = edge.from_subgraph.is_none() && edge.to_subgraph.is_none();
    if allow_waypoints
        && let Some(wps) = layout.edge_waypoints.get(&edge.index)
        && !wps.is_empty()
    {
        let is_backward = is_backward_edge(&from_bounds, &to_bounds, diagram_direction);

        // For backward edges, reverse waypoints so they go from source to target.
        // Dagre stores them in effective/forward order (low rank → high rank),
        // but the backward edge goes from high rank → low rank.
        let waypoints: Vec<(usize, usize)> = if is_backward {
            wps.iter().rev().copied().collect()
        } else {
            wps.to_vec()
        };

        return route_edge_with_waypoints(
            edge,
            &endpoints,
            &waypoints,
            diagram_direction,
            src_attach_override,
            tgt_attach_override,
            src_first_vertical,
        );
    }

    // For backward edges with no dagre waypoints, generate synthetic ones
    if is_backward_edge(&from_bounds, &to_bounds, diagram_direction) {
        let synthetic_wps =
            generate_backward_waypoints(&from_bounds, &to_bounds, diagram_direction);
        if !synthetic_wps.is_empty() {
            if matches!(
                diagram_direction,
                Direction::LeftRight | Direction::RightLeft
            ) {
                return route_edge_with_waypoints(
                    edge,
                    &endpoints,
                    &synthetic_wps,
                    diagram_direction,
                    src_attach_override,
                    tgt_attach_override,
                    src_first_vertical,
                );
            }
            return route_backward_with_synthetic_waypoints(
                edge,
                &endpoints,
                &synthetic_wps,
                diagram_direction,
                src_attach_override,
                tgt_attach_override,
                src_first_vertical,
            );
        }
    }

    // No waypoints: direct routing for forward edges
    route_edge_direct(
        edge,
        &endpoints,
        diagram_direction,
        src_attach_override,
        tgt_attach_override,
        src_first_vertical,
    )
}

/// Route an edge using waypoints from normalization.
///
/// Uses dynamic intersection calculation to determine attachment points
/// based on the approach angle from the first/last waypoint.
fn route_edge_with_waypoints(
    edge: &Edge,
    ep: &EdgeEndpoints,
    waypoints: &[(usize, usize)],
    direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
    src_first_vertical: bool,
) -> Option<RoutedEdge> {
    // Calculate attachment points, using overrides where provided
    let (src_attach_raw, tgt_attach_raw) = resolve_attachment_points(
        src_attach_override,
        tgt_attach_override,
        ep,
        waypoints,
        direction,
    );

    // Clamp attachment points to actual node boundaries
    let src_attach_point = clamp_to_boundary(src_attach_raw, &ep.from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, &ep.to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    if std::env::var("MMDFLUX_DEBUG_ROUTE_SEGMENTS").is_ok_and(|v| v == "1") {
        eprintln!(
            "[route] {} -> {}: waypoints={:?}",
            edge.from, edge.to, waypoints
        );
    }

    // Use face-aware offset to avoid corner ambiguity (e.g., a point at
    // the top-right corner should offset RIGHT for LR layouts, not UP)
    let is_backward = is_backward_edge(&ep.from_bounds, &ep.to_bounds, direction);
    let (src_face, tgt_face) = if edge.from_subgraph.is_some() || edge.to_subgraph.is_some() {
        (
            classify_face(&ep.from_bounds, src_attach, ep.from_shape),
            classify_face(&ep.to_bounds, tgt_attach, ep.to_shape),
        )
    } else {
        edge_faces(direction, is_backward)
    };
    let mut start = offset_for_face(src_attach, src_face);
    let end = offset_for_face(tgt_attach, tgt_face);

    if let Some(&(wp_x, wp_y)) = waypoints.first() {
        let should_skip_offset = match src_face {
            NodeFace::Top => wp_y >= src_attach.1,
            NodeFace::Bottom => wp_y <= src_attach.1,
            NodeFace::Left => wp_x >= src_attach.0,
            NodeFace::Right => wp_x <= src_attach.0,
        };
        if should_skip_offset {
            start = Point::new(src_attach.0, src_attach.1);
        }
    }

    let mut segments = Vec::new();

    // Add connector segment from source node boundary to offset start point
    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    // Build orthogonal path through waypoints, ending with appropriate segment
    segments.extend(build_orthogonal_path_with_waypoints(
        start,
        waypoints,
        end,
        direction,
        src_first_vertical,
    ));

    // Determine entry direction based on final segment orientation
    let entry_direction = entry_direction_from_segments(&segments);

    if std::env::var("MMDFLUX_DEBUG_ROUTE_SEGMENTS").is_ok_and(|v| v == "1") {
        eprintln!(
            "[route] {} -> {}: start={:?} end={:?} segments={:?}",
            edge.from, edge.to, start, end, segments
        );
    }

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
        is_backward,
        is_self_edge: false,
    })
}

/// Route a backward edge using synthetic waypoints (generated by `generate_backward_waypoints`).
///
/// Unlike `route_edge_with_waypoints`, this determines faces from the waypoint
/// approach angle rather than the layout direction, since synthetic waypoints
/// route around the side of nodes.
fn route_backward_with_synthetic_waypoints(
    edge: &Edge,
    ep: &EdgeEndpoints,
    waypoints: &[(usize, usize)],
    direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
    src_first_vertical: bool,
) -> Option<RoutedEdge> {
    // Use intersection calculation from waypoint approach angles
    let (src_attach_raw, tgt_attach_raw) = calculate_attachment_points(
        &ep.from_bounds,
        ep.from_shape,
        &ep.to_bounds,
        ep.to_shape,
        waypoints,
    );

    let src_attach_raw = src_attach_override.unwrap_or(src_attach_raw);
    let tgt_attach_raw = tgt_attach_override.unwrap_or(tgt_attach_raw);

    // Clamp to boundaries
    let src_attach_point = clamp_to_boundary(src_attach_raw, &ep.from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, &ep.to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Determine faces from waypoint approach angle
    let src_face = classify_face(&ep.from_bounds, waypoints[0], ep.from_shape);
    let tgt_face = classify_face(&ep.to_bounds, *waypoints.last().unwrap(), ep.to_shape);

    let start = offset_for_face(src_attach, src_face);
    let end = offset_for_face(tgt_attach, tgt_face);

    let mut segments = Vec::new();

    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    segments.extend(build_orthogonal_path_with_waypoints(
        start,
        waypoints,
        end,
        direction,
        src_first_vertical,
    ));

    let entry_direction = entry_direction_from_segments(&segments);

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
        is_backward: true,
        is_self_edge: false,
    })
}

/// Route an edge directly between two nodes (no intermediate waypoints).
///
/// Uses intersection calculation to determine attachment points based on
/// the relative positions of the nodes.
fn route_edge_direct(
    edge: &Edge,
    ep: &EdgeEndpoints,
    direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
    _src_first_vertical: bool,
) -> Option<RoutedEdge> {
    // For direct routing, use the other node's center as the "approach point"
    let empty_waypoints: &[(usize, usize)] = &[];
    let (mut src_attach_raw, mut tgt_attach_raw) = resolve_attachment_points(
        src_attach_override,
        tgt_attach_override,
        ep,
        empty_waypoints,
        direction,
    );
    let mut src_face_override = None;
    let mut tgt_face_override = None;
    if edge.from_subgraph.is_some() && src_attach_override.is_none() {
        let face = subgraph_edge_face(&ep.from_bounds, &ep.to_bounds, direction);
        src_face_override = Some(face);
        src_attach_raw = clamp_to_face(
            &ep.from_bounds,
            face,
            (ep.to_bounds.center_x(), ep.to_bounds.center_y()),
        );
    }
    if edge.to_subgraph.is_some() && tgt_attach_override.is_none() {
        let face = subgraph_edge_face(&ep.to_bounds, &ep.from_bounds, direction);
        tgt_face_override = Some(face);
        tgt_attach_raw = clamp_to_face(
            &ep.to_bounds,
            face,
            (ep.from_bounds.center_x(), ep.from_bounds.center_y()),
        );
    }

    // Clamp attachment points to actual node boundaries
    let src_attach_point = clamp_to_boundary(src_attach_raw, &ep.from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, &ep.to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Use face-aware offset to avoid corner ambiguity
    let is_backward = is_backward_edge(&ep.from_bounds, &ep.to_bounds, direction);
    let (src_face, tgt_face) = if edge.from_subgraph.is_some() || edge.to_subgraph.is_some() {
        (
            src_face_override
                .unwrap_or_else(|| classify_face(&ep.from_bounds, src_attach, ep.from_shape)),
            tgt_face_override
                .unwrap_or_else(|| classify_face(&ep.to_bounds, tgt_attach, ep.to_shape)),
        )
    } else {
        edge_faces(direction, is_backward)
    };
    let start = offset_for_face(src_attach, src_face);
    let end = offset_for_face(tgt_attach, tgt_face);
    let mut segments = Vec::new();

    // Add connector segment from source node boundary to offset start point
    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    // Build orthogonal path with direction-appropriate segment ordering.
    // For subgraph edges that attach on left/right faces, route horizontally
    // to avoid running straight through vertical stacks.
    let mut path_direction = direction;
    if (edge.from_subgraph.is_some() || edge.to_subgraph.is_some())
        && (matches!(src_face, NodeFace::Left | NodeFace::Right)
            || matches!(tgt_face, NodeFace::Left | NodeFace::Right))
    {
        path_direction = if start.x <= end.x {
            Direction::LeftRight
        } else {
            Direction::RightLeft
        };
    }
    segments.extend(build_orthogonal_path_for_direction(
        start,
        end,
        path_direction,
    ));

    // Determine entry direction: use canonical direction when start == end
    // (zero-length path produces a degenerate segment that can't indicate direction)
    let entry_direction = if start == end {
        match direction {
            Direction::TopDown => AttachDirection::Top,
            Direction::BottomTop => AttachDirection::Bottom,
            Direction::LeftRight => AttachDirection::Left,
            Direction::RightLeft => AttachDirection::Right,
        }
    } else {
        entry_direction_from_segments(&segments)
    };

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
        is_backward,
        is_self_edge: false,
    })
}

/// Resolve attachment points, using overrides when provided, falling back to
/// `calculate_attachment_points()` for non-overridden sides.
///
/// For LR/RL layouts, non-overridden attachment points are forced to the
/// appropriate side face (right/left for source, left/right for target)
/// instead of using geometric center-to-center intersection, which can
/// hit top/bottom faces when nodes aren't horizontally aligned.
fn resolve_attachment_points(
    src_override: Option<(usize, usize)>,
    tgt_override: Option<(usize, usize)>,
    ep: &EdgeEndpoints,
    waypoints: &[(usize, usize)],
    direction: Direction,
) -> ((usize, usize), (usize, usize)) {
    let from_bounds = ep.from_bounds;
    let to_bounds = ep.to_bounds;

    let is_backward = is_backward_edge(&from_bounds, &to_bounds, direction);

    // LR/RL layouts: all edges attach to side faces.
    // Forward edges use consensus-y for straight horizontal segments.
    // Backward edges use center-y on the opposite side face.
    match direction {
        Direction::LeftRight | Direction::RightLeft => {
            let flows_right = matches!(direction, Direction::LeftRight) != is_backward;
            let y = if is_backward {
                // Backward: each node uses its own center_y
                // (consensus doesn't apply since the edge wraps around)
                from_bounds.center_y()
            } else {
                consensus_y(&from_bounds, &to_bounds)
            };
            let tgt_y = if is_backward { to_bounds.center_y() } else { y };
            let (src, tgt) = if flows_right {
                // Source exits right face, target enters left face
                (
                    src_override.unwrap_or((from_bounds.x + from_bounds.width - 1, y)),
                    tgt_override.unwrap_or((to_bounds.x, tgt_y)),
                )
            } else {
                // Source exits left face, target enters right face
                (
                    src_override.unwrap_or((from_bounds.x, y)),
                    tgt_override.unwrap_or((to_bounds.x + to_bounds.width - 1, tgt_y)),
                )
            };
            return (src, tgt);
        }
        _ => {}
    }

    if matches!(direction, Direction::TopDown | Direction::BottomTop)
        && let (Some(&first_wp), Some(&last_wp)) = (waypoints.first(), waypoints.last())
    {
        let (src_face, tgt_face) = edge_faces(direction, is_backward);
        let src = src_override.unwrap_or_else(|| clamp_to_face(&from_bounds, src_face, first_wp));
        let tgt = tgt_override.unwrap_or_else(|| clamp_to_face(&to_bounds, tgt_face, last_wp));
        return (src, tgt);
    }

    // TD/BT layouts: use geometric intersection to find attachment points.
    let fallback = || {
        calculate_attachment_points(
            &from_bounds,
            ep.from_shape,
            &to_bounds,
            ep.to_shape,
            waypoints,
        )
    };
    let src = src_override.unwrap_or_else(|| fallback().0);
    let tgt = tgt_override.unwrap_or_else(|| fallback().1);
    (src, tgt)
}

/// Clamp a waypoint to a node face, returning a point on that face.
fn clamp_to_face(bounds: &NodeBounds, face: NodeFace, waypoint: (usize, usize)) -> (usize, usize) {
    let (min, max) = bounds.face_extent(&face);
    let fixed = bounds.face_fixed_coord(&face);
    match face {
        NodeFace::Top | NodeFace::Bottom => (waypoint.0.clamp(min, max), fixed),
        NodeFace::Left | NodeFace::Right => (fixed, waypoint.1.clamp(min, max)),
    }
}

/// Compute a shared y-coordinate for LR/RL attachment, clamped to both nodes' y-ranges.
fn consensus_y(a: &NodeBounds, b: &NodeBounds) -> usize {
    let avg = (a.center_y() + b.center_y()) / 2;
    avg.max(a.y)
        .min(a.y + a.height - 1)
        .max(b.y)
        .min(b.y + b.height - 1)
}

/// Clamp an attachment point to the actual node boundary.
///
/// The intersection calculation may return points slightly outside the
/// actual boundary cells due to rounding. This function ensures the
/// point is on a valid boundary cell.
fn clamp_to_boundary(point: (usize, usize), bounds: &NodeBounds) -> Point {
    let (x, y) = point;

    let left = bounds.x;
    let right = bounds.x + bounds.width - 1;
    let top = bounds.y;
    let bottom = bounds.y + bounds.height - 1;

    let clamped_x = x.clamp(left, right);
    let clamped_y = y.clamp(top, bottom);

    Point::new(clamped_x, clamped_y)
}

/// Determine the source and target faces for an edge based on layout direction
/// and whether the edge is backward.
///
/// Forward edges exit the "downstream" face and enter the "upstream" face.
/// Backward edges reverse these faces.
fn edge_faces(direction: Direction, is_backward: bool) -> (NodeFace, NodeFace) {
    let (forward_src, forward_tgt) = match direction {
        Direction::TopDown => (NodeFace::Bottom, NodeFace::Top),
        Direction::BottomTop => (NodeFace::Top, NodeFace::Bottom),
        Direction::LeftRight => (NodeFace::Right, NodeFace::Left),
        Direction::RightLeft => (NodeFace::Left, NodeFace::Right),
    };
    if is_backward {
        (forward_tgt, forward_src)
    } else {
        (forward_src, forward_tgt)
    }
}

/// Offset an attachment point by 1 cell in the direction of the given face.
///
/// Unlike `offset_from_boundary`, this doesn't infer the face from position,
/// avoiding ambiguity when the point is at a corner of the node.
fn offset_for_face(point: (usize, usize), face: NodeFace) -> Point {
    let (x, y) = point;
    match face {
        NodeFace::Top => Point::new(x, y.saturating_sub(1)),
        NodeFace::Bottom => Point::new(x, y + 1),
        NodeFace::Left => Point::new(x.saturating_sub(1), y),
        NodeFace::Right => Point::new(x + 1, y),
    }
}

/// Add a connector segment from a node boundary point to an offset point.
///
/// This creates the short segment that visually connects the edge to the node.
fn add_connector_segment(segments: &mut Vec<Segment>, boundary: (usize, usize), offset: Point) {
    let (bx, by) = boundary;
    if bx == offset.x {
        // Vertical connector
        segments.push(Segment::Vertical {
            x: bx,
            y_start: by,
            y_end: offset.y,
        });
    } else if by == offset.y {
        // Horizontal connector
        segments.push(Segment::Horizontal {
            y: by,
            x_start: bx,
            x_end: offset.x,
        });
    }
    // If neither aligned, skip (shouldn't happen with proper offset)
}

/// Determine the entry direction based on the final segment's orientation.
///
/// With L-shaped paths, the final segment directly represents the approach direction:
/// - Vertical final segment going down → entry from Top (arrow ▼)
/// - Vertical final segment going up → entry from Bottom (arrow ▲)
/// - Horizontal final segment going right → entry from Left (arrow ►)
/// - Horizontal final segment going left → entry from Right (arrow ◄)
fn entry_direction_from_segments(segments: &[Segment]) -> AttachDirection {
    match segments.last() {
        Some(Segment::Vertical { y_start, y_end, .. }) if *y_end > *y_start => AttachDirection::Top,
        Some(Segment::Vertical { .. }) => AttachDirection::Bottom,
        Some(Segment::Horizontal { x_start, x_end, .. }) if *x_end > *x_start => {
            AttachDirection::Left
        }
        Some(Segment::Horizontal { .. }) => AttachDirection::Right,
        None => AttachDirection::Top,
    }
}

/// Minimum horizontal offset (in characters) to trigger side-preference routing.
/// When an edge has horizontal offset greater than this threshold, we use
/// asymmetric routing to avoid the congested middle region of the diagram.
const LARGE_HORIZONTAL_OFFSET_THRESHOLD: usize = 15;

/// Build an orthogonal path that ends with a segment matching the approach direction.
///
/// The final segment orientation determines the arrow direction:
/// - Vertical final segment → ▼ or ▲ arrow
/// - Horizontal final segment → ► or ◄ arrow
///
/// Creates an L-shaped path (2 segments) when start and end are not aligned:
/// - First segment moves in the primary layout direction
/// - Second segment moves in the cross direction to reach the target
///
/// This ensures the arrow glyph visually matches the line direction entering the target.
///
/// For edges with large horizontal offset (source far from target horizontally),
/// the routing is adjusted to place the horizontal segment closer to the target,
/// avoiding the congested middle region of the diagram.
fn build_orthogonal_path_for_direction(
    start: Point,
    end: Point,
    direction: Direction,
) -> Vec<Segment> {
    // If start and end are already aligned, just create a single segment
    if start.x == end.x {
        return vec![Segment::Vertical {
            x: start.x,
            y_start: start.y,
            y_end: end.y,
        }];
    }
    if start.y == end.y {
        return vec![Segment::Horizontal {
            y: start.y,
            x_start: start.x,
            x_end: end.x,
        }];
    }

    // For non-aligned paths, the final segment should match the layout's canonical
    // entry direction so arrows visually connect to the expected side of the target:
    // - TD/BT: final segment is vertical (arrows ▼/▲ enter from top/bottom)
    // - LR/RL: final segment is horizontal (arrows ►/◄ enter from left/right)
    //
    // Both use Z-shaped paths to ensure the correct entry direction:
    // - TD/BT: V-H-V (vertical entry)
    // - LR/RL: H-V-H (horizontal entry)
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            // Vertical layouts: V-H-V (Z-shape) to enter target from top/bottom
            // Final segment is vertical, so arrow will be ▼ or ▲
            let mid_y = compute_mid_y_for_vertical_layout(start, end, direction);
            vec![
                Segment::Vertical {
                    x: start.x,
                    y_start: start.y,
                    y_end: mid_y,
                },
                Segment::Horizontal {
                    y: mid_y,
                    x_start: start.x,
                    x_end: end.x,
                },
                Segment::Vertical {
                    x: end.x,
                    y_start: mid_y,
                    y_end: end.y,
                },
            ]
        }
        Direction::LeftRight | Direction::RightLeft => {
            // Horizontal layouts: H-V-H (Z-shape) to enter target from left/right
            // Final segment is horizontal, so arrow will be ► or ◄
            let mid_x = (start.x + end.x) / 2;
            vec![
                Segment::Horizontal {
                    y: start.y,
                    x_start: start.x,
                    x_end: mid_x,
                },
                Segment::Vertical {
                    x: mid_x,
                    y_start: start.y,
                    y_end: end.y,
                },
                Segment::Horizontal {
                    y: end.y,
                    x_start: mid_x,
                    x_end: end.x,
                },
            ]
        }
    }
}

/// Compute the Y coordinate for the horizontal segment in a Z-shaped path.
///
/// For edges with large horizontal offset (source far right, target more centered),
/// we place the horizontal segment closer to the target to avoid routing through
/// the congested middle region of the diagram. This creates an asymmetric Z-path
/// that "hugs" the bottom (for TD) or top (for BT).
///
/// For normal edges, uses the standard midpoint calculation.
fn compute_mid_y_for_vertical_layout(start: Point, end: Point, direction: Direction) -> usize {
    let horizontal_offset = start.x.abs_diff(end.x);

    // Check if this edge has a large horizontal offset
    let mut mid_y = if horizontal_offset > LARGE_HORIZONTAL_OFFSET_THRESHOLD {
        // Determine if source is to the right of target (right-to-left routing)
        let is_right_to_left = start.x > end.x;

        if is_right_to_left {
            // For right-to-left edges with large offset:
            // Place the horizontal segment closer to the target to avoid
            // crossing through the congested middle of the diagram.
            //
            // For TD: place horizontal near the bottom (closer to end.y)
            // For BT: place horizontal near the top (closer to end.y, which is lower)
            match direction {
                Direction::TopDown => {
                    // TD: end.y > start.y, so we want mid_y close to end.y
                    // Leave room for the final vertical segment (at least 2 rows)
                    let target_mid = end.y.saturating_sub(2);
                    // But don't go above the standard midpoint (avoid going too high)
                    let standard_mid = (start.y + end.y) / 2;
                    target_mid.max(standard_mid)
                }
                Direction::BottomTop => {
                    // BT: end.y < start.y, so we want mid_y close to end.y
                    // Leave room for the final vertical segment (at least 2 rows)
                    let target_mid = end.y + 2;
                    // But don't go below the standard midpoint
                    let standard_mid = (start.y + end.y) / 2;
                    target_mid.min(standard_mid)
                }
                _ => (start.y + end.y) / 2,
            }
        } else {
            // Left-to-right edges: use standard midpoint
            // (these typically don't have the same congestion issue)
            (start.y + end.y) / 2
        }
    } else {
        // Normal edges: use standard midpoint
        (start.y + end.y) / 2
    };

    // Avoid placing the horizontal segment on the target row, which creates a
    // zero-length final vertical segment and makes arrowheads look like they
    // attach to horizontal lines.
    if mid_y == end.y {
        if start.y > end.y {
            mid_y = end.y + 1;
        } else {
            mid_y = end.y.saturating_sub(1);
        }
    }

    mid_y
}

/// Build an orthogonal path through waypoints, ending with appropriate segment for layout.
///
/// Similar to build_orthogonal_path but ensures the final segment type matches the
/// layout direction for proper arrow positioning.
fn build_orthogonal_path_with_waypoints(
    start: Point,
    waypoints: &[(usize, usize)],
    end: Point,
    direction: Direction,
    start_vertical: bool,
) -> Vec<Segment> {
    let vertical_first = matches!(direction, Direction::TopDown | Direction::BottomTop);

    if waypoints.is_empty() {
        // No waypoints: use direction-appropriate path
        return build_orthogonal_path_for_direction(start, end, direction);
    }

    let mut start_vertical_override = start_vertical;
    let mut waypoint_slice = waypoints;
    if vertical_first
        && let Some(&(wp_x, wp_y)) = waypoint_slice.first()
        && wp_y == start.y
    {
        waypoint_slice = &waypoint_slice[1..];
        if wp_x != start.x {
            start_vertical_override = false;
        }
    }
    if waypoint_slice.is_empty() {
        return build_orthogonal_path_for_direction(start, end, direction);
    }
    if vertical_first && start.x != end.x && waypoint_slice.iter().all(|(x, _)| *x == end.x) {
        let mut mid_y = start.y;
        if mid_y == end.y {
            mid_y = match direction {
                Direction::TopDown => end.y.saturating_sub(1),
                Direction::BottomTop => end.y.saturating_add(1),
                _ => mid_y,
            };
        }

        let mut segments = Vec::new();
        if start.y != mid_y {
            segments.push(Segment::Vertical {
                x: start.x,
                y_start: start.y,
                y_end: mid_y,
            });
        }
        segments.push(Segment::Horizontal {
            y: mid_y,
            x_start: start.x,
            x_end: end.x,
        });
        if mid_y != end.y {
            segments.push(Segment::Vertical {
                x: end.x,
                y_start: mid_y,
                y_end: end.y,
            });
        }
        return segments;
    }

    let mut segments = Vec::new();

    // Start → first waypoint
    let first_wp = Point::new(waypoint_slice[0].0, waypoint_slice[0].1);
    let first_vertical = start_vertical_override || !vertical_first;

    segments.extend(orthogonalize_segment(start, first_wp, first_vertical));

    // Through all intermediate waypoints
    for window in waypoint_slice.windows(2) {
        let from = Point::new(window[0].0, window[0].1);
        let to = Point::new(window[1].0, window[1].1);
        segments.extend(orthogonalize_segment(from, to, !vertical_first));
    }

    // Last waypoint → end: use direction-appropriate final segment
    let &(last_x, last_y) = waypoint_slice.last().unwrap();
    let last_wp = Point::new(last_x, last_y);
    segments.extend(build_orthogonal_path_for_direction(last_wp, end, direction));

    segments
}

/// Compute path preferring vertical movement first (used in tests).
///
/// Delegates to `build_orthogonal_path_for_direction` with TD direction,
/// which produces the same V-H-V Z-shaped path.
#[cfg(test)]
fn compute_vertical_first_path(start: Point, end: Point) -> Vec<Segment> {
    build_orthogonal_path_for_direction(start, end, Direction::TopDown)
}

/// Convert a single diagonal segment into orthogonal (axis-aligned) segments.
///
/// For diagonal paths (where neither x nor y coordinates match), this creates
/// a Z-shaped path with a horizontal and vertical component. The direction
/// preference determines whether we go vertical-first or horizontal-first.
///
/// # Arguments
/// * `from` - Starting point
/// * `to` - Ending point
/// * `vertical_first` - If true, prefer vertical-then-horizontal routing (for TD/BT).
///   If false, prefer horizontal-then-vertical routing (for LR/RL).
fn orthogonalize_segment(from: Point, to: Point, vertical_first: bool) -> Vec<Segment> {
    if from.x == to.x {
        // Already vertical
        vec![Segment::Vertical {
            x: from.x,
            y_start: from.y,
            y_end: to.y,
        }]
    } else if from.y == to.y {
        // Already horizontal
        vec![Segment::Horizontal {
            y: from.y,
            x_start: from.x,
            x_end: to.x,
        }]
    } else if vertical_first {
        // Z-path: vertical → horizontal
        // Go vertically from `from` to `to.y`, then horizontally to `to.x`
        vec![
            Segment::Vertical {
                x: from.x,
                y_start: from.y,
                y_end: to.y,
            },
            Segment::Horizontal {
                y: to.y,
                x_start: from.x,
                x_end: to.x,
            },
        ]
    } else {
        // Z-path: horizontal → vertical
        // Go horizontally from `from` to `to.x`, then vertically to `to.y`
        vec![
            Segment::Horizontal {
                y: from.y,
                x_start: from.x,
                x_end: to.x,
            },
            Segment::Vertical {
                x: to.x,
                y_start: from.y,
                y_end: to.y,
            },
        ]
    }
}

/// Convert a series of waypoints into orthogonal path segments.
///
/// Waypoints from dagre's normalization may be at arbitrary positions. This
/// function converts each consecutive pair of points into axis-aligned segments,
/// creating Z-paths for any diagonal sections.
///
/// # Arguments
/// * `waypoints` - List of (x, y) coordinates representing the path
/// * `direction` - Layout direction (determines vertical-first vs horizontal-first preference)
///
/// # Returns
/// A list of orthogonal segments connecting all waypoints.
#[cfg(test)]
pub fn orthogonalize(waypoints: &[(usize, usize)], direction: Direction) -> Vec<Segment> {
    if waypoints.len() < 2 {
        return Vec::new();
    }

    let vertical_first = matches!(direction, Direction::TopDown | Direction::BottomTop);
    let mut segments = Vec::new();

    for window in waypoints.windows(2) {
        let from = Point::new(window[0].0, window[0].1);
        let to = Point::new(window[1].0, window[1].1);
        segments.extend(orthogonalize_segment(from, to, vertical_first));
    }

    segments
}

/// Build a complete orthogonal path from start attachment through waypoints to end attachment.
///
/// This is the main entry point for creating routed edge paths that use waypoint
/// information from normalization and dynamic attachment points from intersection
/// calculation.
///
/// # Arguments
/// * `start` - Attachment point on the source node boundary
/// * `waypoints` - Intermediate waypoints from dummy node positions (may be empty)
/// * `end` - Attachment point on the target node boundary
/// * `direction` - Layout direction
///
/// # Returns
/// A list of orthogonal segments forming the complete path from start to end.
#[cfg(test)]
pub fn build_orthogonal_path(
    start: Point,
    waypoints: &[(usize, usize)],
    end: Point,
    direction: Direction,
) -> Vec<Segment> {
    let vertical_first = matches!(direction, Direction::TopDown | Direction::BottomTop);

    if waypoints.is_empty() {
        // No intermediate waypoints: direct path from start to end
        return orthogonalize_segment(start, end, vertical_first);
    }

    let mut segments = Vec::new();

    // Start → first waypoint
    let first_wp = Point::new(waypoints[0].0, waypoints[0].1);
    segments.extend(orthogonalize_segment(start, first_wp, vertical_first));

    // Through all waypoints
    for window in waypoints.windows(2) {
        let from = Point::new(window[0].0, window[0].1);
        let to = Point::new(window[1].0, window[1].1);
        segments.extend(orthogonalize_segment(from, to, vertical_first));
    }

    // Last waypoint → end
    let &(last_x, last_y) = waypoints.last().unwrap();
    let last_wp = Point::new(last_x, last_y);
    segments.extend(orthogonalize_segment(last_wp, end, vertical_first));

    segments
}

/// Pre-computed attachment override for one edge.
#[derive(Debug, Clone)]
pub struct AttachmentOverride {
    pub source: Option<(usize, usize)>,
    pub target: Option<(usize, usize)>,
    pub source_first_vertical: bool,
}

/// Compute pre-assigned attachment points for edges that share a node face.
///
/// Only produces overrides for faces with >1 edge. Single-edge faces
/// use the default intersect_rect() calculation (no override).
pub fn compute_attachment_plan(
    edges: &[Edge],
    layout: &Layout,
    direction: Direction,
) -> HashMap<usize, AttachmentOverride> {
    // Step 1: Classify faces and build groups
    // The approach_cross_axis is the cross-axis coordinate of the approach point,
    // used for sorting to minimize visual crossings.
    let mut face_groups: FaceGroupMap = HashMap::new();

    for (i, edge) in edges.iter().enumerate() {
        // Skip self-edges — they are routed separately
        if edge.from == edge.to {
            continue;
        }
        // Skip invisible edges — they affect layout but are not rendered
        if edge.stroke == Stroke::Invisible {
            continue;
        }
        let (src_bounds, tgt_bounds) = match resolve_edge_bounds(layout, edge) {
            Some(bounds) => bounds,
            None => continue,
        };

        let src_shape = if edge.from_subgraph.is_some() {
            Shape::Rectangle
        } else {
            layout
                .node_shapes
                .get(&edge.from)
                .copied()
                .unwrap_or(Shape::Rectangle)
        };
        let tgt_shape = if edge.to_subgraph.is_some() {
            Shape::Rectangle
        } else {
            layout
                .node_shapes
                .get(&edge.to)
                .copied()
                .unwrap_or(Shape::Rectangle)
        };

        // Determine approach points using waypoints if available
        let allow_waypoints = edge.from_subgraph.is_none() && edge.to_subgraph.is_none();
        let waypoints = if allow_waypoints {
            layout.edge_waypoints.get(&edge.index)
        } else {
            None
        };

        // For source: approach point is first waypoint or target center
        let src_approach = waypoints
            .and_then(|wps| wps.first().copied())
            .unwrap_or((tgt_bounds.center_x(), tgt_bounds.center_y()));

        // For target: approach point is last waypoint or source center
        let tgt_approach = waypoints
            .and_then(|wps| wps.last().copied())
            .unwrap_or((src_bounds.center_x(), src_bounds.center_y()));

        // Determine the effective direction for this edge.
        // Internal edges within a direction-override subgraph use that
        // subgraph's direction; cross-boundary edges use the diagram direction.
        let edge_dir = layout.effective_edge_direction(&edge.from, &edge.to, direction);

        let is_subgraph_edge = edge.from_subgraph.is_some() || edge.to_subgraph.is_some();

        // Determine faces for this edge.
        // Backward edges without dagre waypoints use synthetic routing (around
        // the right/bottom of nodes), so they must be classified on the face
        // that matches the synthetic path — not the geometric approach angle.
        let is_backward = is_backward_edge(&src_bounds, &tgt_bounds, edge_dir);
        let has_dagre_waypoints = waypoints.is_some_and(|wps| !wps.is_empty());
        let (mut src_face, mut tgt_face) = if is_backward && !has_dagre_waypoints {
            backward_routing_faces(edge_dir)
        } else if matches!(edge_dir, Direction::TopDown | Direction::BottomTop)
            && !is_backward
            && !is_subgraph_edge
        {
            // For forward edges in vertical layouts, stick to canonical faces to
            // keep fan-in/out attachment spreading stable.
            edge_faces(edge_dir, false)
        } else {
            match edge_dir {
                Direction::LeftRight | Direction::RightLeft => edge_faces(edge_dir, is_backward),
                _ => (
                    classify_face(&src_bounds, src_approach, src_shape),
                    classify_face(&tgt_bounds, tgt_approach, tgt_shape),
                ),
            }
        };

        if edge.from_subgraph.is_some() {
            src_face = subgraph_edge_face(&src_bounds, &tgt_bounds, edge_dir);
        }
        if edge.to_subgraph.is_some() {
            tgt_face = subgraph_edge_face(&tgt_bounds, &src_bounds, edge_dir);
        }

        // Extract cross-axis coordinate from approach point for sorting
        let src_cross = match src_face {
            NodeFace::Top | NodeFace::Bottom => src_approach.0,
            NodeFace::Left | NodeFace::Right => src_approach.1,
        };
        let tgt_cross = match tgt_face {
            NodeFace::Top | NodeFace::Bottom => tgt_approach.0,
            NodeFace::Left | NodeFace::Right => tgt_approach.1,
        };

        let src_id = edge.from_subgraph.as_deref().unwrap_or(edge.from.as_str());
        let tgt_id = edge.to_subgraph.as_deref().unwrap_or(edge.to.as_str());

        face_groups
            .entry((src_id.to_string(), src_face))
            .or_default()
            .push((i, true, src_cross, has_dagre_waypoints));
        face_groups
            .entry((tgt_id.to_string(), tgt_face))
            .or_default()
            .push((i, false, tgt_cross, has_dagre_waypoints));
    }

    // Step 2: For faces with >1 edge, compute spread positions
    let mut overrides: HashMap<usize, AttachmentOverride> = HashMap::new();

    for ((node_id, face), group) in &face_groups {
        if group.len() <= 1 {
            continue;
        }

        let bounds = match bounds_for_node_id(layout, node_id) {
            Some(b) => b,
            None => continue,
        };

        // Sort edges by approach point's cross-axis coordinate
        let mut sorted = group.clone();
        sorted.sort_by_key(|&(_, _, approach_cross, _)| approach_cross);

        let extent = bounds.face_extent(face);
        let fixed = bounds.face_fixed_coord(face);
        let points = spread_points_on_face(*face, fixed, extent, sorted.len());

        let flow_face = match direction {
            Direction::TopDown => Some(NodeFace::Bottom),
            Direction::BottomTop => Some(NodeFace::Top),
            _ => None,
        };
        let center_cross = match face {
            NodeFace::Top | NodeFace::Bottom => bounds.center_x(),
            NodeFace::Left | NodeFace::Right => bounds.center_y(),
        } as isize;
        let mut left_count = 0usize;
        let mut right_count = 0usize;
        for (_, is_source, cross, has_wps) in &sorted {
            if *is_source && *has_wps {
                if *cross as isize >= center_cross {
                    right_count += 1;
                } else {
                    left_count += 1;
                }
            }
        }
        let should_consider = flow_face.is_some_and(|face_match| *face == face_match);
        let mut left_lane = 0usize;
        let mut right_lane = 0usize;

        for (idx, &(edge_i, is_source, cross, has_wps)) in sorted.iter().enumerate() {
            let point = points[idx];
            let entry = overrides.entry(edge_i).or_insert(AttachmentOverride {
                source: None,
                target: None,
                source_first_vertical: false,
            });
            if is_source {
                entry.source = Some(point);
                if should_consider && has_wps {
                    let side = if cross as isize >= center_cross {
                        1
                    } else {
                        -1
                    };
                    let side_count = if side > 0 { right_count } else { left_count };
                    if side_count > 1 {
                        if side > 0 {
                            entry.source_first_vertical = right_lane % 2 == 1;
                            right_lane += 1;
                        } else {
                            entry.source_first_vertical = left_lane % 2 == 1;
                            left_lane += 1;
                        }
                    }
                }
            } else {
                entry.target = Some(point);
            }
        }
    }

    overrides
}

/// Route a self-edge as orthogonal segments from pre-computed draw-coordinate points.
fn route_self_edge(data: &SelfEdgeDrawData, edge: &Edge, direction: Direction) -> RoutedEdge {
    let segments: Vec<Segment> = data
        .points
        .windows(2)
        .flat_map(|window| {
            let (x1, y1) = window[0];
            let (x2, y2) = window[1];

            match (x1 == x2, y1 == y2) {
                (_, true) => vec![Segment::Horizontal {
                    y: y1,
                    x_start: x1.min(x2),
                    x_end: x1.max(x2),
                }],
                (true, _) => vec![Segment::Vertical {
                    x: x1,
                    y_start: y1.min(y2),
                    y_end: y1.max(y2),
                }],
                // Diagonal — split into L-shape (shouldn't happen with orthogonal points)
                _ => vec![
                    Segment::Vertical {
                        x: x1,
                        y_start: y1.min(y2),
                        y_end: y1.max(y2),
                    },
                    Segment::Horizontal {
                        y: y2,
                        x_start: x1.min(x2),
                        x_end: x1.max(x2),
                    },
                ],
            }
        })
        .collect();

    let to_point = |&(x, y)| Point::new(x, y);
    let start = data
        .points
        .first()
        .map(to_point)
        .unwrap_or(Point::new(0, 0));
    let end = data.points.last().map(to_point).unwrap_or(Point::new(0, 0));

    // Entry direction: the arrow enters from the side where the loop is.
    // TD/BT: loop is on the right face. LR/RL: loop is on the bottom face.
    let entry_direction = match direction {
        Direction::TopDown | Direction::BottomTop => AttachDirection::Right,
        Direction::LeftRight | Direction::RightLeft => AttachDirection::Bottom,
    };

    RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
        is_backward: false,
        is_self_edge: true,
    }
}

/// Route all edges in the layout.
pub fn route_all_edges(
    edges: &[Edge],
    layout: &Layout,
    diagram_direction: Direction,
) -> Vec<RoutedEdge> {
    // Pre-pass: compute attachment plan for edges sharing a face
    let plan = compute_attachment_plan(edges, layout, diagram_direction);

    let mut routed: Vec<RoutedEdge> = edges
        .iter()
        .enumerate()
        .filter_map(|(i, edge)| {
            // Skip self-edges in normal routing
            if edge.from == edge.to {
                return None;
            }
            // Skip invisible edges — they affect layout but are not rendered
            if edge.stroke == Stroke::Invisible {
                return None;
            }
            let (src_override, tgt_override, src_first_vertical) = plan
                .get(&i)
                .map(|ov| (ov.source, ov.target, ov.source_first_vertical))
                .unwrap_or((None, None, false));
            let edge_dir = layout.effective_edge_direction(&edge.from, &edge.to, diagram_direction);
            route_edge(
                edge,
                layout,
                edge_dir,
                src_override,
                tgt_override,
                src_first_vertical,
            )
        })
        .collect();

    // Route self-edges separately using pre-computed loop points
    for se_data in &layout.self_edges {
        if let Some(edge) = edges
            .iter()
            .find(|e| e.from == e.to && e.from == se_data.node_id)
            && !se_data.points.is_empty()
        {
            routed.push(route_self_edge(se_data, edge, diagram_direction));
        }
    }

    routed
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
