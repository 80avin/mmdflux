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
            from_bounds,
            from_shape,
            to_bounds,
            to_shape,
            &waypoints,
            diagram_direction,
            src_attach_override,
            tgt_attach_override,
        );
    }

    // No waypoints: direct routing (works for both forward and short backward edges)
    route_edge_direct(
        edge,
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
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
    from_bounds: &NodeBounds,
    from_shape: Shape,
    to_bounds: &NodeBounds,
    to_shape: Shape,
    waypoints: &[(usize, usize)],
    direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
) -> Option<RoutedEdge> {
    // Calculate attachment points, using overrides where provided
    let (src_attach_raw, tgt_attach_raw) = resolve_attachment_points(
        src_attach_override,
        tgt_attach_override,
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
        waypoints,
        direction,
    );

    // Clamp attachment points to actual node boundaries
    let src_attach_point = clamp_to_boundary(src_attach_raw, from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Offset both attachment points by 1 cell outside the node boundaries
    let start = offset_from_boundary(src_attach, from_bounds);
    let end = offset_from_boundary(tgt_attach, to_bounds);

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
    from_bounds: &NodeBounds,
    from_shape: Shape,
    to_bounds: &NodeBounds,
    to_shape: Shape,
    direction: Direction,
    src_attach_override: Option<(usize, usize)>,
    tgt_attach_override: Option<(usize, usize)>,
) -> Option<RoutedEdge> {
    // For direct routing, use the other node's center as the "approach point"
    let empty_waypoints: &[(usize, usize)] = &[];
    let (src_attach_raw, tgt_attach_raw) = resolve_attachment_points(
        src_attach_override,
        tgt_attach_override,
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
        empty_waypoints,
        direction,
    );

    // Clamp attachment points to actual node boundaries
    // The intersection calculation may return points slightly outside due to
    // floating-point rounding (e.g., height/2 doesn't account for discrete cells)
    let src_attach_point = clamp_to_boundary(src_attach_raw, from_bounds);
    let tgt_attach_point = clamp_to_boundary(tgt_attach_raw, to_bounds);
    let src_attach = (src_attach_point.x, src_attach_point.y);
    let tgt_attach = (tgt_attach_point.x, tgt_attach_point.y);

    // Offset both attachment points by 1 cell outside the node boundaries
    // This ensures edges don't overlap with node drawings and arrows are
    // placed in the gap between nodes
    let start = offset_from_boundary(src_attach, from_bounds);
    let end = offset_from_boundary(tgt_attach, to_bounds);
    let mut segments = Vec::new();

    // Add connector segment from source node boundary to offset start point
    // This ensures the edge visually connects to the node
    if src_attach != (start.x, start.y) {
        add_connector_segment(&mut segments, src_attach, start);
    }

    // Build orthogonal path with direction-appropriate segment ordering
    segments.extend(build_orthogonal_path_for_direction(start, end, direction));

    // Note: We don't add a connector to the target because the arrow is drawn
    // at 'end' which is already at the offset position (1 cell from node).
    // The arrow itself provides the visual connection to the target.

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
    from_bounds: &NodeBounds,
    from_shape: Shape,
    to_bounds: &NodeBounds,
    to_shape: Shape,
    waypoints: &[(usize, usize)],
    direction: Direction,
) -> ((usize, usize), (usize, usize)) {
    let fallback =
        || calculate_attachment_points(from_bounds, from_shape, to_bounds, to_shape, waypoints);

    let src = match src_override {
        Some(s) => s,
        None => match direction {
            Direction::LeftRight => {
                // Source exits on right face
                (
                    from_bounds.x + from_bounds.width - 1,
                    from_bounds.center_y(),
                )
            }
            Direction::RightLeft => {
                // Source exits on left face
                (from_bounds.x, from_bounds.center_y())
            }
            _ => fallback().0,
        },
    };

    let tgt = match tgt_override {
        Some(t) => t,
        None => match direction {
            Direction::LeftRight => {
                // Target entered on left face
                (to_bounds.x, to_bounds.center_y())
            }
            Direction::RightLeft => {
                // Target entered on right face
                (to_bounds.x + to_bounds.width - 1, to_bounds.center_y())
            }
            _ => fallback().1,
        },
    };

    (src, tgt)
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

/// Offset an attachment point by 1 cell outside the node boundary.
///
/// This ensures edges don't overlap with node drawings. The offset direction
/// is determined by which edge of the node the point is closest to.
fn offset_from_boundary(point: (usize, usize), bounds: &NodeBounds) -> Point {
    let (x, y) = point;
    let cx = bounds.center_x();
    let cy = bounds.center_y();

    // Determine which boundary edge the point is on
    let on_top = y == bounds.y;
    let on_bottom = y == bounds.y + bounds.height - 1;
    let on_left = x == bounds.x;
    let on_right = x == bounds.x + bounds.width - 1;

    // Offset in the appropriate direction
    if on_top {
        Point::new(x, y.saturating_sub(1))
    } else if on_bottom {
        Point::new(x, y + 1)
    } else if on_left {
        Point::new(x.saturating_sub(1), y)
    } else if on_right {
        Point::new(x + 1, y)
    } else {
        // Point is not on boundary (shouldn't happen with proper intersection)
        // Fall back to moving away from center
        let dx = if x > cx {
            1
        } else if x < cx {
            -1_isize
        } else {
            0
        };
        let dy = if y > cy {
            1
        } else if y < cy {
            -1_isize
        } else {
            0
        };
        Point::new(
            (x as isize + dx).max(0) as usize,
            (y as isize + dy).max(0) as usize,
        )
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
    // Key: (node_id, face) -> Vec<(edge_index, is_source_side)>
    let mut face_groups: HashMap<(String, NodeFace), Vec<(usize, bool)>> = HashMap::new();

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

        // For LR/RL layouts, force side faces instead of geometric classification
        // to ensure edges exit/enter on the correct sides
        let (src_face, tgt_face) = match direction {
            Direction::LeftRight => (NodeFace::Right, NodeFace::Left),
            Direction::RightLeft => (NodeFace::Left, NodeFace::Right),
            _ => {
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

                (
                    classify_face(src_bounds, src_approach, src_shape),
                    classify_face(tgt_bounds, tgt_approach, tgt_shape),
                )
            }
        };

        face_groups
            .entry((edge.from.clone(), src_face))
            .or_default()
            .push((i, true));
        face_groups
            .entry((edge.to.clone(), tgt_face))
            .or_default()
            .push((i, false));
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

        // Sort edges by cross-axis position of opposite endpoint
        let mut sorted = group.clone();
        sort_face_group(&mut sorted, edges, layout, *face);

        let extent = bounds.face_extent(face);
        let fixed = bounds.face_fixed_coord(face);
        let points = spread_points_on_face(*face, fixed, extent, sorted.len());

        for (idx, &(edge_i, is_source)) in sorted.iter().enumerate() {
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

/// Sort edges within a face group by cross-axis position of the opposite endpoint.
/// This minimizes visual crossings by matching left-to-right edge order to
/// left-to-right source/target order.
fn sort_face_group(
    group: &mut Vec<(usize, bool)>,
    edges: &[Edge],
    layout: &Layout,
    face: NodeFace,
) {
    group.sort_by_key(|&(edge_i, is_source)| {
        let edge = &edges[edge_i];
        let other_id = if is_source { &edge.to } else { &edge.from };
        let other_bounds = layout.get_bounds(other_id);

        match face {
            // For top/bottom faces, sort by x-position of opposite endpoint
            NodeFace::Top | NodeFace::Bottom => other_bounds.map(|b| b.center_x()).unwrap_or(0),
            // For left/right faces, sort by y-position of opposite endpoint
            NodeFace::Left | NodeFace::Right => other_bounds.map(|b| b.center_y()).unwrap_or(0),
        }
    });
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
mod tests {
    use super::super::layout::{LayoutConfig, compute_layout, compute_layout_dagre};
    use super::*;
    use crate::graph::{Diagram, Node};

    fn simple_td_diagram() -> Diagram {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram
    }

    #[test]
    fn test_route_edge_straight_vertical() {
        let diagram = simple_td_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let edge = &diagram.edges[0];
        let routed = route_edge(edge, &layout, Direction::TopDown, None, None).unwrap();

        // Should have at least one segment
        assert!(!routed.segments.is_empty());

        // For vertically aligned nodes, should have:
        // 1. Connector segment from node boundary to offset point
        // 2. Main vertical segment from offset start to offset end
        if routed.start.x == routed.end.x {
            assert_eq!(routed.segments.len(), 2);
            // First segment: connector from source
            match routed.segments[0] {
                Segment::Vertical { .. } => {}
                _ => panic!("Expected vertical connector segment"),
            }
            // Second segment: main vertical
            match routed.segments[1] {
                Segment::Vertical { .. } => {}
                _ => panic!("Expected vertical main segment"),
            }
        }
    }

    #[test]
    fn test_route_edge_with_bend() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("Branch1"));
        diagram.add_node(Node::new("C").with_label("Branch2"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("A", "C"));

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Route edge from A to C (which will be offset horizontally)
        let edge = &diagram.edges[1];
        let routed = route_edge(edge, &layout, Direction::TopDown, None, None).unwrap();

        // If nodes are not aligned, should have multiple segments
        if routed.start.x != routed.end.x {
            assert!(routed.segments.len() > 1);
        }
    }

    #[test]
    fn test_route_all_edges() {
        let diagram = simple_td_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let routed = route_all_edges(&diagram.edges, &layout, Direction::TopDown);

        assert_eq!(routed.len(), 1);
    }

    #[test]
    fn test_attachment_directions_td() {
        let (out_dir, in_dir) = attachment_directions(Direction::TopDown);
        assert!(matches!(out_dir, AttachDirection::Bottom));
        assert!(matches!(in_dir, AttachDirection::Top));
    }

    #[test]
    fn test_attachment_directions_lr() {
        let (out_dir, in_dir) = attachment_directions(Direction::LeftRight);
        assert!(matches!(out_dir, AttachDirection::Right));
        assert!(matches!(in_dir, AttachDirection::Left));
    }

    #[test]
    fn test_point_creation() {
        let p = Point::new(10, 20);
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
    }

    #[test]
    fn test_straight_vertical_path() {
        let start = Point::new(10, 5);
        let end = Point::new(10, 15);
        let segments = compute_vertical_first_path(start, end);

        assert_eq!(segments.len(), 1);
        match segments[0] {
            Segment::Vertical { x, y_start, y_end } => {
                assert_eq!(x, 10);
                assert_eq!(y_start, 5);
                assert_eq!(y_end, 15);
            }
            _ => panic!("Expected vertical segment"),
        }
    }

    #[test]
    fn test_z_shaped_vertical_path() {
        let start = Point::new(5, 5);
        let end = Point::new(15, 15);
        let segments = compute_vertical_first_path(start, end);

        assert_eq!(segments.len(), 3);
        assert!(matches!(segments[0], Segment::Vertical { .. }));
        assert!(matches!(segments[1], Segment::Horizontal { .. }));
        assert!(matches!(segments[2], Segment::Vertical { .. }));
    }

    // Backward edge detection tests

    fn make_bounds(x: usize, y: usize) -> NodeBounds {
        NodeBounds {
            x,
            y,
            width: 10,
            height: 3,
        }
    }

    #[test]
    fn test_is_backward_edge_td_forward() {
        // In TD layout, source above target is forward
        let from = make_bounds(10, 0);
        let to = make_bounds(10, 10);
        assert!(!is_backward_edge(&from, &to, Direction::TopDown));
    }

    #[test]
    fn test_is_backward_edge_td_backward() {
        // In TD layout, source below target is backward
        let from = make_bounds(10, 10);
        let to = make_bounds(10, 0);
        assert!(is_backward_edge(&from, &to, Direction::TopDown));
    }

    #[test]
    fn test_is_backward_edge_bt_forward() {
        // In BT layout, source below target is forward
        let from = make_bounds(10, 10);
        let to = make_bounds(10, 0);
        assert!(!is_backward_edge(&from, &to, Direction::BottomTop));
    }

    #[test]
    fn test_is_backward_edge_bt_backward() {
        // In BT layout, source above target is backward
        let from = make_bounds(10, 0);
        let to = make_bounds(10, 10);
        assert!(is_backward_edge(&from, &to, Direction::BottomTop));
    }

    #[test]
    fn test_is_backward_edge_lr_forward() {
        // In LR layout, source left of target is forward
        let from = make_bounds(0, 10);
        let to = make_bounds(20, 10);
        assert!(!is_backward_edge(&from, &to, Direction::LeftRight));
    }

    #[test]
    fn test_is_backward_edge_lr_backward() {
        // In LR layout, source right of target is backward
        let from = make_bounds(20, 10);
        let to = make_bounds(0, 10);
        assert!(is_backward_edge(&from, &to, Direction::LeftRight));
    }

    #[test]
    fn test_is_backward_edge_rl_forward() {
        // In RL layout, source right of target is forward
        let from = make_bounds(20, 10);
        let to = make_bounds(0, 10);
        assert!(!is_backward_edge(&from, &to, Direction::RightLeft));
    }

    #[test]
    fn test_is_backward_edge_rl_backward() {
        // In RL layout, source left of target is backward
        let from = make_bounds(0, 10);
        let to = make_bounds(20, 10);
        assert!(is_backward_edge(&from, &to, Direction::RightLeft));
    }

    #[test]
    fn test_is_backward_edge_same_position() {
        // Same position is not backward (edge case)
        let from = make_bounds(10, 10);
        let to = make_bounds(10, 10);
        assert!(!is_backward_edge(&from, &to, Direction::TopDown));
        assert!(!is_backward_edge(&from, &to, Direction::BottomTop));
        assert!(!is_backward_edge(&from, &to, Direction::LeftRight));
        assert!(!is_backward_edge(&from, &to, Direction::RightLeft));
    }

    // Orthogonalization tests

    #[test]
    fn test_orthogonalize_segment_vertical() {
        // Vertical segment should stay vertical
        let from = Point::new(10, 5);
        let to = Point::new(10, 15);
        let segments = orthogonalize_segment(from, to, true);

        assert_eq!(segments.len(), 1);
        match segments[0] {
            Segment::Vertical { x, y_start, y_end } => {
                assert_eq!(x, 10);
                assert_eq!(y_start, 5);
                assert_eq!(y_end, 15);
            }
            _ => panic!("Expected vertical segment"),
        }
    }

    #[test]
    fn test_orthogonalize_segment_horizontal() {
        // Horizontal segment should stay horizontal
        let from = Point::new(5, 10);
        let to = Point::new(20, 10);
        let segments = orthogonalize_segment(from, to, true);

        assert_eq!(segments.len(), 1);
        match segments[0] {
            Segment::Horizontal { y, x_start, x_end } => {
                assert_eq!(y, 10);
                assert_eq!(x_start, 5);
                assert_eq!(x_end, 20);
            }
            _ => panic!("Expected horizontal segment"),
        }
    }

    #[test]
    fn test_orthogonalize_segment_diagonal_vertical_first() {
        // Diagonal segment with vertical-first preference
        let from = Point::new(5, 5);
        let to = Point::new(15, 20);
        let segments = orthogonalize_segment(from, to, true);

        assert_eq!(segments.len(), 2);
        // First: vertical from (5,5) to (5,20)
        match segments[0] {
            Segment::Vertical { x, y_start, y_end } => {
                assert_eq!(x, 5);
                assert_eq!(y_start, 5);
                assert_eq!(y_end, 20);
            }
            _ => panic!("Expected vertical segment first"),
        }
        // Second: horizontal from (5,20) to (15,20)
        match segments[1] {
            Segment::Horizontal { y, x_start, x_end } => {
                assert_eq!(y, 20);
                assert_eq!(x_start, 5);
                assert_eq!(x_end, 15);
            }
            _ => panic!("Expected horizontal segment second"),
        }
    }

    #[test]
    fn test_orthogonalize_segment_diagonal_horizontal_first() {
        // Diagonal segment with horizontal-first preference
        let from = Point::new(5, 5);
        let to = Point::new(15, 20);
        let segments = orthogonalize_segment(from, to, false);

        assert_eq!(segments.len(), 2);
        // First: horizontal from (5,5) to (15,5)
        match segments[0] {
            Segment::Horizontal { y, x_start, x_end } => {
                assert_eq!(y, 5);
                assert_eq!(x_start, 5);
                assert_eq!(x_end, 15);
            }
            _ => panic!("Expected horizontal segment first"),
        }
        // Second: vertical from (15,5) to (15,20)
        match segments[1] {
            Segment::Vertical { x, y_start, y_end } => {
                assert_eq!(x, 15);
                assert_eq!(y_start, 5);
                assert_eq!(y_end, 20);
            }
            _ => panic!("Expected vertical segment second"),
        }
    }

    #[test]
    fn test_orthogonalize_empty_waypoints() {
        let waypoints: Vec<(usize, usize)> = vec![];
        let segments = orthogonalize(&waypoints, Direction::TopDown);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_orthogonalize_single_waypoint() {
        // Single waypoint = no segments (need at least 2 points)
        let waypoints = vec![(10, 10)];
        let segments = orthogonalize(&waypoints, Direction::TopDown);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_orthogonalize_two_waypoints_aligned() {
        let waypoints = vec![(10, 5), (10, 15)];
        let segments = orthogonalize(&waypoints, Direction::TopDown);

        assert_eq!(segments.len(), 1);
        assert!(matches!(segments[0], Segment::Vertical { x: 10, .. }));
    }

    #[test]
    fn test_orthogonalize_two_waypoints_diagonal() {
        let waypoints = vec![(5, 5), (15, 20)];
        let segments = orthogonalize(&waypoints, Direction::TopDown);

        // TD is vertical-first, so should be 2 segments
        assert_eq!(segments.len(), 2);
        assert!(matches!(segments[0], Segment::Vertical { .. }));
        assert!(matches!(segments[1], Segment::Horizontal { .. }));
    }

    #[test]
    fn test_orthogonalize_three_waypoints() {
        let waypoints = vec![(5, 5), (15, 10), (25, 20)];
        let segments = orthogonalize(&waypoints, Direction::TopDown);

        // Two diagonal segments → 4 segments total (2 per diagonal)
        assert_eq!(segments.len(), 4);
    }

    #[test]
    fn test_build_orthogonal_path_no_waypoints() {
        let start = Point::new(10, 5);
        let end = Point::new(20, 15);
        let waypoints: Vec<(usize, usize)> = vec![];

        let segments = build_orthogonal_path(start, &waypoints, end, Direction::TopDown);

        // Direct diagonal path → 2 segments (vertical-first for TD)
        assert_eq!(segments.len(), 2);
        assert!(matches!(segments[0], Segment::Vertical { .. }));
        assert!(matches!(segments[1], Segment::Horizontal { .. }));
    }

    #[test]
    fn test_build_orthogonal_path_with_waypoints() {
        let start = Point::new(10, 5);
        let waypoints = vec![(15, 10), (20, 15)];
        let end = Point::new(25, 20);

        let segments = build_orthogonal_path(start, &waypoints, end, Direction::TopDown);

        // start→wp1: diagonal (2 segs), wp1→wp2: diagonal (2 segs), wp2→end: diagonal (2 segs)
        // Total: 6 segments
        assert_eq!(segments.len(), 6);
    }

    #[test]
    fn test_build_orthogonal_path_aligned_waypoints() {
        let start = Point::new(10, 5);
        let waypoints = vec![(10, 10), (10, 15)]; // All on same x
        let end = Point::new(10, 20);

        let segments = build_orthogonal_path(start, &waypoints, end, Direction::TopDown);

        // All aligned vertically → 3 vertical segments
        assert_eq!(segments.len(), 3);
        for seg in segments {
            assert!(matches!(seg, Segment::Vertical { x: 10, .. }));
        }
    }

    #[test]
    fn test_build_orthogonal_path_lr_direction() {
        let start = Point::new(5, 10);
        let end = Point::new(20, 15);
        let waypoints: Vec<(usize, usize)> = vec![];

        let segments = build_orthogonal_path(start, &waypoints, end, Direction::LeftRight);

        // LR uses horizontal-first but note: build_orthogonal_path uses
        // orthogonalize_segment (not build_orthogonal_path_for_direction),
        // so it produces H-V for LR (horizontal-first = !vertical_first)
        assert_eq!(segments.len(), 2);
        assert!(matches!(segments[0], Segment::Horizontal { .. }));
        assert!(matches!(segments[1], Segment::Vertical { .. }));
    }

    // Backward edge routing tests

    #[test]
    fn test_route_backward_edge_td() {
        // Create a diagram with a cycle: A -> B -> A
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B")); // Forward
        diagram.add_edge(Edge::new("B", "A")); // Backward

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Route the backward edge
        let backward_edge = &diagram.edges[1];
        let routed = route_edge(backward_edge, &layout, Direction::TopDown, None, None).unwrap();

        // Backward edge without waypoints uses direct routing.
        // For TD layout with B above A, the edge goes upward, entering from Bottom.
        assert_eq!(routed.entry_direction, AttachDirection::Bottom);

        // Should have segments connecting B to A
        assert!(!routed.segments.is_empty());
    }

    #[test]
    fn test_route_backward_edge_lr() {
        // Create a horizontal layout with a cycle
        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B")); // Forward
        diagram.add_edge(Edge::new("B", "A")); // Backward

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Route the backward edge
        let backward_edge = &diagram.edges[1];
        let routed = route_edge(backward_edge, &layout, Direction::LeftRight, None, None).unwrap();

        // Backward edge without waypoints uses direct routing.
        // For LR layout with B to the right of A, the backward edge goes leftward.
        assert_eq!(routed.entry_direction, AttachDirection::Right);

        // Should have segments connecting B to A
        assert!(!routed.segments.is_empty());
    }

    #[test]
    fn test_forward_edge_entry_direction_td() {
        // Forward edges should have standard entry direction
        let diagram = simple_td_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let edge = &diagram.edges[0];
        let routed = route_edge(edge, &layout, Direction::TopDown, None, None).unwrap();

        // TD forward edges enter from Top
        assert_eq!(routed.entry_direction, AttachDirection::Top);
    }

    #[test]
    fn test_forward_edge_entry_direction_lr() {
        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B"));

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let edge = &diagram.edges[0];
        let routed = route_edge(edge, &layout, Direction::LeftRight, None, None).unwrap();

        // LR forward edges enter from Left
        assert_eq!(routed.entry_direction, AttachDirection::Left);
    }

    #[test]
    fn test_multiple_backward_edges_route_successfully() {
        // Create diagram with two backward edges going to different targets
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Top"));
        diagram.add_node(Node::new("B").with_label("Middle"));
        diagram.add_node(Node::new("C").with_label("Bottom"));
        diagram.add_edge(Edge::new("A", "B")); // Forward
        diagram.add_edge(Edge::new("B", "C")); // Forward
        diagram.add_edge(Edge::new("C", "A")); // Backward to A
        diagram.add_edge(Edge::new("C", "B")); // Backward to B

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        // Route both backward edges — they should both produce valid paths
        let edge_c_to_a = &diagram.edges[2];
        let edge_c_to_b = &diagram.edges[3];
        let routed_c_a = route_edge(edge_c_to_a, &layout, Direction::TopDown, None, None);
        let routed_c_b = route_edge(edge_c_to_b, &layout, Direction::TopDown, None, None);

        assert!(routed_c_a.is_some(), "Backward edge C->A should route");
        assert!(routed_c_b.is_some(), "Backward edge C->B should route");

        // Both should have segments
        assert!(!routed_c_a.unwrap().segments.is_empty());
        assert!(!routed_c_b.unwrap().segments.is_empty());
    }

    // --- Waypoint-based backward edge tests ---

    #[test]
    fn test_backward_edge_with_waypoints_td() {
        // Backward edge spanning 2+ ranks should use waypoints
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Top"));
        diagram.add_node(Node::new("B").with_label("Middle"));
        diagram.add_node(Node::new("C").with_label("Bottom"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "C"));
        diagram.add_edge(Edge::new("C", "A")); // Backward spanning 2 ranks

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let backward_edge = &diagram.edges[2];
        let routed = route_edge(backward_edge, &layout, Direction::TopDown, None, None).unwrap();

        assert!(
            routed.segments.len() >= 2,
            "Backward edge should have routing segments, got {}",
            routed.segments.len()
        );
    }

    #[test]
    fn test_short_backward_edge_no_waypoints() {
        // B→A backward edge spanning 1 rank — no dummies, no waypoints
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Top"));
        diagram.add_node(Node::new("B").with_label("Bottom"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "A")); // Backward, 1 rank

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let backward_edge = &diagram.edges[1];
        let routed = route_edge(backward_edge, &layout, Direction::TopDown, None, None);
        assert!(
            routed.is_some(),
            "Short backward edge should route successfully"
        );
    }

    #[test]
    fn test_backward_edge_lr_with_waypoints() {
        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A").with_label("Left"));
        diagram.add_node(Node::new("B").with_label("Mid"));
        diagram.add_node(Node::new("C").with_label("Right"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram.add_edge(Edge::new("B", "C"));
        diagram.add_edge(Edge::new("C", "A")); // Backward, spans 2 ranks

        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let backward_edge = &diagram.edges[2];
        let routed = route_edge(backward_edge, &layout, Direction::LeftRight, None, None);
        assert!(
            routed.is_some(),
            "LR backward edge should route successfully"
        );
    }

    #[test]
    fn test_backward_edge_no_canvas_expansion() {
        // Backward edge waypoints should NOT expand canvas width
        let mut diagram_with_cycle = Diagram::new(Direction::TopDown);
        diagram_with_cycle.add_node(Node::new("A").with_label("Top"));
        diagram_with_cycle.add_node(Node::new("B").with_label("Bottom"));
        diagram_with_cycle.add_edge(Edge::new("A", "B"));
        diagram_with_cycle.add_edge(Edge::new("B", "A")); // Backward

        let mut diagram_no_cycle = Diagram::new(Direction::TopDown);
        diagram_no_cycle.add_node(Node::new("A").with_label("Top"));
        diagram_no_cycle.add_node(Node::new("B").with_label("Bottom"));
        diagram_no_cycle.add_edge(Edge::new("A", "B"));

        let config = LayoutConfig::default();
        let layout_cycle = compute_layout_dagre(&diagram_with_cycle, &config);
        let layout_no_cycle = compute_layout_dagre(&diagram_no_cycle, &config);

        assert_eq!(
            layout_cycle.width, layout_no_cycle.width,
            "Backward edge should not expand canvas width. With cycle: {}, without: {}",
            layout_cycle.width, layout_no_cycle.width
        );
    }
}
