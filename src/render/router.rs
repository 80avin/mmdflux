//! Edge routing between nodes.
//!
//! Computes paths for edges, avoiding node boundaries.

use std::collections::HashMap;

use super::intersect::{
    NodeFace, calculate_attachment_points, classify_face, spread_points_on_face,
};
use super::layout::Layout;
use super::shape::NodeBounds;
use crate::graph::{Direction, Edge, Shape};

/// Map from (node_id, face) to the edges attached at that face.
/// Each entry is `(edge_index, is_source_side, approach_cross_axis)`.
type FaceGroupMap = HashMap<(String, NodeFace), Vec<(usize, bool, usize)>>;

/// Grouped endpoint parameters for edge routing functions.
struct EdgeEndpoints<'a> {
    from_bounds: &'a NodeBounds,
    from_shape: Shape,
    to_bounds: &'a NodeBounds,
    to_shape: Shape,
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
    match direction {
        // For TD, backward means target is above source
        Direction::TopDown => to_bounds.y < from_bounds.y,
        // For BT, backward means target is below source
        Direction::BottomTop => to_bounds.y > from_bounds.y,
        // For LR, backward means target is to the left of source
        Direction::LeftRight => to_bounds.x < from_bounds.x,
        // For RL, backward means target is to the right of source
        Direction::RightLeft => to_bounds.x > from_bounds.x,
    }
}

/// Route an edge between two nodes.
pub fn route_edge(
    edge: &Edge,
    layout: &Layout,
    diagram_direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
) -> Option<RoutedEdge> {
    let from_bounds = layout.get_bounds(&edge.from)?;
    let to_bounds = layout.get_bounds(&edge.to)?;

    // Get node shapes for intersection calculation
    let from_shape = layout
        .node_shapes
        .get(&edge.from)
        .copied()
        .unwrap_or(Shape::Rectangle);
    let to_shape = layout
        .node_shapes
        .get(&edge.to)
        .copied()
        .unwrap_or(Shape::Rectangle);

    let endpoints = EdgeEndpoints {
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
    };

    // Check for waypoints from normalization — works for both forward and backward long edges
    let edge_key = (edge.from.clone(), edge.to.clone());
    if let Some(wps) = layout.edge_waypoints.get(&edge_key)
        && !wps.is_empty()
    {
        let is_backward = is_backward_edge(from_bounds, to_bounds, diagram_direction);

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
        );
    }

    // No waypoints: direct routing (works for both forward and short backward edges)
    route_edge_direct(
        edge,
        &endpoints,
        diagram_direction,
        src_attach_override,
        tgt_attach_override,
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
    let src_attach_point = clamp_to_boundary(src_attach_raw, ep.from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, ep.to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Use face-aware offset to avoid corner ambiguity (e.g., a point at
    // the top-right corner should offset RIGHT for LR layouts, not UP)
    let is_backward = is_backward_edge(ep.from_bounds, ep.to_bounds, direction);
    let (src_face, tgt_face) = edge_faces(direction, is_backward);
    let start = offset_for_face(src_attach, src_face);
    let end = offset_for_face(tgt_attach, tgt_face);

    let mut segments = Vec::new();

    // Add connector segment from source node boundary to offset start point
    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    // Build orthogonal path through waypoints, ending with appropriate segment
    segments.extend(build_orthogonal_path_with_waypoints(
        start, waypoints, end, direction,
    ));

    // Determine entry direction based on final segment orientation
    let entry_direction = entry_direction_from_segments(&segments);

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
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
) -> Option<RoutedEdge> {
    // For direct routing, use the other node's center as the "approach point"
    let empty_waypoints: &[(usize, usize)] = &[];
    let (src_attach_raw, tgt_attach_raw) = resolve_attachment_points(
        src_attach_override,
        tgt_attach_override,
        ep,
        empty_waypoints,
        direction,
    );

    // Clamp attachment points to actual node boundaries
    let src_attach_point = clamp_to_boundary(src_attach_raw, ep.from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, ep.to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Use face-aware offset to avoid corner ambiguity
    let is_backward = is_backward_edge(ep.from_bounds, ep.to_bounds, direction);
    let (src_face, tgt_face) = edge_faces(direction, is_backward);
    let start = offset_for_face(src_attach, src_face);
    let end = offset_for_face(tgt_attach, tgt_face);
    let mut segments = Vec::new();

    // Add connector segment from source node boundary to offset start point
    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    // Build orthogonal path with direction-appropriate segment ordering
    segments.extend(build_orthogonal_path_for_direction(start, end, direction));

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

    // For LR/RL layouts, all edges (forward and backward) attach to the
    // side faces. Forward edges use consensus-y for straight horizontal
    // segments. Backward edges also use side faces (matching Mermaid behavior)
    // but may use the opposite side face — the source exits from its LEFT face
    // and the target is entered from its LEFT face (for LR), with the edge
    // routing around below/above the nodes.
    let is_backward = is_backward_edge(from_bounds, to_bounds, direction);

    match direction {
        Direction::LeftRight if is_backward => {
            // Backward LR: source exits LEFT face, target enters RIGHT face.
            // The edge wraps around below (or above) the nodes and approaches
            // the target from its right side, matching Mermaid behavior.
            let src = src_override.unwrap_or((from_bounds.x, from_bounds.center_y()));
            let tgt =
                tgt_override.unwrap_or((to_bounds.x + to_bounds.width - 1, to_bounds.center_y()));
            return (src, tgt);
        }
        Direction::RightLeft if is_backward => {
            // Backward RL: source exits RIGHT face, target enters LEFT face.
            let src = src_override.unwrap_or((
                from_bounds.x + from_bounds.width - 1,
                from_bounds.center_y(),
            ));
            let tgt = tgt_override.unwrap_or((to_bounds.x, to_bounds.center_y()));
            return (src, tgt);
        }
        Direction::LeftRight => {
            let consensus_y = consensus_y(from_bounds, to_bounds);
            let src = src_override.unwrap_or((from_bounds.x + from_bounds.width - 1, consensus_y));
            let tgt = tgt_override.unwrap_or((to_bounds.x, consensus_y));
            return (src, tgt);
        }
        Direction::RightLeft => {
            let consensus_y = consensus_y(from_bounds, to_bounds);
            let src = src_override.unwrap_or((from_bounds.x, consensus_y));
            let tgt = tgt_override.unwrap_or((to_bounds.x + to_bounds.width - 1, consensus_y));
            return (src, tgt);
        }
        _ => {}
    }

    let fallback = || {
        calculate_attachment_points(
            from_bounds,
            ep.from_shape,
            to_bounds,
            ep.to_shape,
            waypoints,
        )
    };
    let src = src_override.unwrap_or_else(|| fallback().0);
    let tgt = tgt_override.unwrap_or_else(|| fallback().1);
    (src, tgt)
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

    // Calculate actual boundary cell coordinates
    let left = bounds.x;
    let right = bounds.x + bounds.width - 1;
    let top = bounds.y;
    let bottom = bounds.y + bounds.height - 1;

    // Clamp x to boundary
    let clamped_x = x.clamp(left, right);
    // Clamp y to boundary
    let clamped_y = y.clamp(top, bottom);

    Point::new(clamped_x, clamped_y)
}

/// Determine the source and target faces for an edge based on layout direction
/// and whether the edge is backward.
fn edge_faces(direction: Direction, is_backward: bool) -> (NodeFace, NodeFace) {
    match direction {
        Direction::TopDown => {
            if is_backward {
                (NodeFace::Top, NodeFace::Bottom)
            } else {
                (NodeFace::Bottom, NodeFace::Top)
            }
        }
        Direction::BottomTop => {
            if is_backward {
                (NodeFace::Bottom, NodeFace::Top)
            } else {
                (NodeFace::Top, NodeFace::Bottom)
            }
        }
        Direction::LeftRight => {
            if is_backward {
                (NodeFace::Left, NodeFace::Right)
            } else {
                (NodeFace::Right, NodeFace::Left)
            }
        }
        Direction::RightLeft => {
            if is_backward {
                (NodeFace::Right, NodeFace::Left)
            } else {
                (NodeFace::Left, NodeFace::Right)
            }
        }
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
    if let Some(last_segment) = segments.last() {
        match last_segment {
            Segment::Vertical { y_start, y_end, .. } => {
                if *y_end > *y_start {
                    // Moving downward: entering from Top (arrow points down ▼)
                    AttachDirection::Top
                } else {
                    // Moving upward: entering from Bottom (arrow points up ▲)
                    AttachDirection::Bottom
                }
            }
            Segment::Horizontal { x_start, x_end, .. } => {
                if *x_end > *x_start {
                    // Moving rightward: entering from Left (arrow points right ►)
                    AttachDirection::Left
                } else {
                    // Moving leftward: entering from Right (arrow points left ◄)
                    AttachDirection::Right
                }
            }
        }
    } else {
        // No segments: shouldn't happen, default to Top
        AttachDirection::Top
    }
}

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
            let mid_y = (start.y + end.y) / 2;
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

/// Build an orthogonal path through waypoints, ending with appropriate segment for layout.
///
/// Similar to build_orthogonal_path but ensures the final segment type matches the
/// layout direction for proper arrow positioning.
fn build_orthogonal_path_with_waypoints(
    start: Point,
    waypoints: &[(usize, usize)],
    end: Point,
    direction: Direction,
) -> Vec<Segment> {
    let vertical_first = matches!(direction, Direction::TopDown | Direction::BottomTop);

    if waypoints.is_empty() {
        // No waypoints: use direction-appropriate path
        return build_orthogonal_path_for_direction(start, end, direction);
    }

    let mut segments = Vec::new();

    // Start → first waypoint
    let first_wp = Point::new(waypoints[0].0, waypoints[0].1);
    segments.extend(orthogonalize_segment(start, first_wp, !vertical_first));

    // Through all intermediate waypoints
    for window in waypoints.windows(2) {
        let from = Point::new(window[0].0, window[0].1);
        let to = Point::new(window[1].0, window[1].1);
        segments.extend(orthogonalize_segment(from, to, !vertical_first));
    }

    // Last waypoint → end: use direction-appropriate final segment
    let last_wp = Point::new(
        waypoints[waypoints.len() - 1].0,
        waypoints[waypoints.len() - 1].1,
    );
    segments.extend(build_orthogonal_path_for_direction(last_wp, end, direction));

    segments
}

/// Compute path preferring vertical movement first (used in tests).
#[cfg(test)]
fn compute_vertical_first_path(start: Point, end: Point) -> Vec<Segment> {
    let mut segments = Vec::new();

    if start.x == end.x {
        // Straight vertical line
        segments.push(Segment::Vertical {
            x: start.x,
            y_start: start.y,
            y_end: end.y,
        });
    } else if start.y == end.y {
        // Straight horizontal line (shouldn't happen often in TD)
        segments.push(Segment::Horizontal {
            y: start.y,
            x_start: start.x,
            x_end: end.x,
        });
    } else {
        // L-shaped or Z-shaped path
        // Calculate midpoint for the bend
        let mid_y = if start.y < end.y {
            start.y + (end.y - start.y) / 2
        } else {
            end.y + (start.y - end.y) / 2
        };

        // Vertical segment from start to midpoint
        segments.push(Segment::Vertical {
            x: start.x,
            y_start: start.y,
            y_end: mid_y,
        });

        // Horizontal segment at midpoint
        segments.push(Segment::Horizontal {
            y: mid_y,
            x_start: start.x,
            x_end: end.x,
        });

        // Vertical segment from midpoint to end
        segments.push(Segment::Vertical {
            x: end.x,
            y_start: mid_y,
            y_end: end.y,
        });
    }

    segments
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
    let last_wp = Point::new(
        waypoints[waypoints.len() - 1].0,
        waypoints[waypoints.len() - 1].1,
    );
    segments.extend(orthogonalize_segment(last_wp, end, vertical_first));

    segments
}

/// Pre-computed attachment override for one edge.
#[derive(Debug, Clone)]
pub struct AttachmentOverride {
    pub source: Option<(usize, usize)>,
    pub target: Option<(usize, usize)>,
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
        let src_bounds = match layout.get_bounds(&edge.from) {
            Some(b) => b,
            None => continue,
        };
        let tgt_bounds = match layout.get_bounds(&edge.to) {
            Some(b) => b,
            None => continue,
        };

        let src_shape = layout
            .node_shapes
            .get(&edge.from)
            .copied()
            .unwrap_or(Shape::Rectangle);
        let tgt_shape = layout
            .node_shapes
            .get(&edge.to)
            .copied()
            .unwrap_or(Shape::Rectangle);

        // Determine approach points using waypoints if available
        let edge_key = (edge.from.clone(), edge.to.clone());
        let waypoints = layout.edge_waypoints.get(&edge_key);

        // For source: approach point is first waypoint or target center
        let src_approach = waypoints
            .and_then(|wps| wps.first().copied())
            .unwrap_or((tgt_bounds.center_x(), tgt_bounds.center_y()));

        // For target: approach point is last waypoint or source center
        let tgt_approach = waypoints
            .and_then(|wps| wps.last().copied())
            .unwrap_or((src_bounds.center_x(), src_bounds.center_y()));

        // For LR/RL layouts, force side faces based on edge direction.
        // Forward edges: source exits forward face, target enters forward face.
        // Backward edges: source exits backward face, target enters backward face.
        let is_backward = is_backward_edge(src_bounds, tgt_bounds, direction);
        let (src_face, tgt_face) = match direction {
            Direction::LeftRight | Direction::RightLeft => edge_faces(direction, is_backward),
            _ => (
                classify_face(src_bounds, src_approach, src_shape),
                classify_face(tgt_bounds, tgt_approach, tgt_shape),
            ),
        };

        // Extract cross-axis coordinate from approach point for sorting
        let src_cross = match src_face {
            NodeFace::Top | NodeFace::Bottom => src_approach.0,
            NodeFace::Left | NodeFace::Right => src_approach.1,
        };
        let tgt_cross = match tgt_face {
            NodeFace::Top | NodeFace::Bottom => tgt_approach.0,
            NodeFace::Left | NodeFace::Right => tgt_approach.1,
        };

        face_groups
            .entry((edge.from.clone(), src_face))
            .or_default()
            .push((i, true, src_cross));
        face_groups
            .entry((edge.to.clone(), tgt_face))
            .or_default()
            .push((i, false, tgt_cross));
    }

    // Step 2: For faces with >1 edge, compute spread positions
    let mut overrides: HashMap<usize, AttachmentOverride> = HashMap::new();

    for ((node_id, face), group) in &face_groups {
        if group.len() <= 1 {
            continue;
        }

        let bounds = match layout.get_bounds(node_id) {
            Some(b) => b,
            None => continue,
        };

        // Sort edges by approach point's cross-axis coordinate
        let mut sorted = group.clone();
        sorted.sort_by_key(|&(_, _, approach_cross)| approach_cross);

        let extent = bounds.face_extent(face);
        let fixed = bounds.face_fixed_coord(face);
        let points = spread_points_on_face(*face, fixed, extent, sorted.len());

        for (idx, &(edge_i, is_source, _)) in sorted.iter().enumerate() {
            let point = points[idx];
            let entry = overrides.entry(edge_i).or_insert(AttachmentOverride {
                source: None,
                target: None,
            });
            if is_source {
                entry.source = Some(point);
            } else {
                entry.target = Some(point);
            }
        }
    }

    overrides
}

/// Route all edges in the layout.
pub fn route_all_edges(
    edges: &[Edge],
    layout: &Layout,
    diagram_direction: Direction,
) -> Vec<RoutedEdge> {
    // Pre-pass: compute attachment plan for edges sharing a face
    let plan = compute_attachment_plan(edges, layout, diagram_direction);

    edges
        .iter()
        .enumerate()
        .filter_map(|(i, edge)| {
            let (src_override, tgt_override) = plan
                .get(&i)
                .map(|ov| (ov.source, ov.target))
                .unwrap_or((None, None));
            route_edge(edge, layout, diagram_direction, src_override, tgt_override)
        })
        .collect()
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
