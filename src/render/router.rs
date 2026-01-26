//! Edge routing between nodes.
//!
//! Computes paths for edges, avoiding node boundaries.

use super::intersect::calculate_attachment_points;
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

/// Calculate the attachment point for a node based on direction.
/// The point is placed just outside the node boundary.
fn attachment_point(bounds: &NodeBounds, direction: AttachDirection) -> Point {
    match direction {
        AttachDirection::Top => {
            let (x, y) = bounds.top();
            // One cell above the top border
            Point::new(x, y.saturating_sub(1))
        }
        AttachDirection::Bottom => {
            let (x, y) = bounds.bottom();
            // One cell below the bottom border
            Point::new(x, y + 1)
        }
        AttachDirection::Left => {
            let (x, y) = bounds.left();
            // One cell to the left of the left border
            Point::new(x.saturating_sub(1), y)
        }
        AttachDirection::Right => {
            let (x, y) = bounds.right();
            // One cell to the right of the right border
            Point::new(x + 1, y)
        }
    }
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
) -> Option<RoutedEdge> {
    let from_bounds = layout.get_bounds(&edge.from)?;
    let to_bounds = layout.get_bounds(&edge.to)?;

    // Check if this is a backward edge
    if is_backward_edge(from_bounds, to_bounds, diagram_direction) {
        return route_backward_edge(edge, from_bounds, to_bounds, layout, diagram_direction);
    }

    // Check if we have waypoints for this edge (from normalization)
    let edge_key = (edge.from.clone(), edge.to.clone());
    let waypoints = layout.edge_waypoints.get(&edge_key);

    // Get node shapes for intersection calculation (default to Rectangle if not found)
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

    if let Some(wps) = waypoints {
        if !wps.is_empty() {
            // Use waypoints with dynamic intersection calculation
            return route_edge_with_waypoints(
                edge,
                from_bounds,
                from_shape,
                to_bounds,
                to_shape,
                wps,
                diagram_direction,
            );
        }
    }

    // No waypoints: use intersection calculation for direct path
    route_edge_direct(
        edge,
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
        diagram_direction,
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
) -> Option<RoutedEdge> {
    // Calculate attachment points based on waypoint positions
    let (src_attach, tgt_attach) =
        calculate_attachment_points(from_bounds, from_shape, to_bounds, to_shape, waypoints);

    // Offset both attachment points by 1 cell outside the node boundaries
    let start = offset_from_boundary(src_attach, from_bounds);
    let end = offset_from_boundary(tgt_attach, to_bounds);

    // Build orthogonal path through waypoints, ending with appropriate segment
    let segments = build_orthogonal_path_with_waypoints(start, waypoints, end, direction);

    // Use canonical entry direction for the layout
    let entry_direction = entry_direction_for_layout(direction);

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
) -> Option<RoutedEdge> {
    // For direct routing, use the other node's center as the "approach point"
    let empty_waypoints: &[(usize, usize)] = &[];
    let (src_attach, tgt_attach) = calculate_attachment_points(
        from_bounds,
        from_shape,
        to_bounds,
        to_shape,
        empty_waypoints,
    );

    // Offset both attachment points by 1 cell outside the node boundaries
    // This ensures edges don't overlap with node drawings and arrows are
    // placed in the gap between nodes
    let start = offset_from_boundary(src_attach, from_bounds);
    let end = offset_from_boundary(tgt_attach, to_bounds);

    // Build orthogonal path with direction-appropriate segment ordering
    let segments = build_orthogonal_path_for_direction(start, end, direction);

    // Determine entry direction based on diagram flow direction
    let entry_direction = entry_direction_for_layout(direction);

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction,
    })
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

/// Determine the entry direction based on how the path approaches the endpoint.
fn determine_entry_direction(segments: &[Segment], _end: Point) -> AttachDirection {
    if let Some(last_segment) = segments.last() {
        match last_segment {
            Segment::Vertical { y_start, y_end, .. } => {
                // Approaching vertically
                if *y_end > *y_start {
                    AttachDirection::Top // Approaching from above
                } else {
                    AttachDirection::Bottom // Approaching from below
                }
            }
            Segment::Horizontal { x_start, x_end, .. } => {
                // Approaching horizontally
                if *x_end > *x_start {
                    AttachDirection::Left // Approaching from the left
                } else {
                    AttachDirection::Right // Approaching from the right
                }
            }
        }
    } else {
        // Fallback: determine from endpoint position relative to where we'd expect
        // This shouldn't normally happen
        AttachDirection::Top
    }
}

/// Get the canonical entry direction for edges in a given layout direction.
///
/// For forward edges, this is the direction from which edges enter target nodes:
/// - TD: edges enter from Top (arrow points down)
/// - BT: edges enter from Bottom (arrow points up)
/// - LR: edges enter from Left (arrow points right)
/// - RL: edges enter from Right (arrow points left)
fn entry_direction_for_layout(direction: Direction) -> AttachDirection {
    match direction {
        Direction::TopDown => AttachDirection::Top,
        Direction::BottomTop => AttachDirection::Bottom,
        Direction::LeftRight => AttachDirection::Left,
        Direction::RightLeft => AttachDirection::Right,
    }
}

/// Build an orthogonal path that ends with a segment aligned with the layout direction.
///
/// For TD/BT layouts, paths should end with vertical segments so arrows point down/up.
/// For LR/RL layouts, paths should end with horizontal segments so arrows point right/left.
///
/// Creates a Z-shaped path (3 segments) when start and end are not aligned, with the
/// middle segment placed at a position that avoids creating visual artifacts near nodes.
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

    // For non-aligned paths, create a Z-shaped path with:
    // - First segment in the direction of flow
    // - Middle segment perpendicular at midpoint
    // - Last segment completing the path to target
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            // Vertical layouts: vertical-horizontal-vertical (Z-shape)
            // Place horizontal segment at midpoint y
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
            // Horizontal layouts: horizontal-vertical-horizontal (Z-shape)
            // Place vertical segment at midpoint x
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

/// Route a backward edge around the diagram perimeter.
///
/// Backward edges (cycles) are routed around the side of the diagram to avoid
/// passing through intermediate nodes.
fn route_backward_edge(
    edge: &Edge,
    from_bounds: &NodeBounds,
    to_bounds: &NodeBounds,
    layout: &Layout,
    diagram_direction: Direction,
) -> Option<RoutedEdge> {
    match diagram_direction {
        Direction::TopDown | Direction::BottomTop => {
            route_backward_edge_vertical(edge, from_bounds, to_bounds, layout, diagram_direction)
        }
        Direction::LeftRight | Direction::RightLeft => {
            route_backward_edge_horizontal(edge, from_bounds, to_bounds, layout, diagram_direction)
        }
    }
}

/// Route a backward edge for vertical (TD/BT) layouts.
///
/// For TD: exits from TOP of source, travels horizontally to corridor, then up
/// in corridor, then horizontally to enter target from the right. Exiting from
/// the top makes the edge origin unambiguous when sibling nodes exist on the
/// same row.
///
/// For BT: exits from BOTTOM of source (mirrored logic).
fn route_backward_edge_vertical(
    edge: &Edge,
    from_bounds: &NodeBounds,
    to_bounds: &NodeBounds,
    layout: &Layout,
    diagram_direction: Direction,
) -> Option<RoutedEdge> {
    // Exit direction depends on layout: TD exits from top, BT exits from bottom
    let exit_dir = if diagram_direction == Direction::TopDown {
        AttachDirection::Top
    } else {
        AttachDirection::Bottom
    };
    let start = attachment_point(from_bounds, exit_dir);
    // Enter from right side of target
    let end = attachment_point(to_bounds, AttachDirection::Right);

    // Get the node border point (attachment_point adds 1 cell offset)
    let (border_x, border_y) = if diagram_direction == Direction::TopDown {
        from_bounds.top()
    } else {
        from_bounds.bottom()
    };

    // Get lane assignment for this edge (default to 0 if not found)
    let lane = layout
        .backward_edge_lanes
        .get(&(edge.from.clone(), edge.to.clone()))
        .copied()
        .unwrap_or(0);

    // Corridor X position: each lane gets its own corridor space
    // content_width + (lane * corridor_width) + corridor_width/2
    let content_width = layout.width - (layout.backward_corridors * layout.corridor_width);
    let corridor_x = content_width + (lane * layout.corridor_width) + layout.corridor_width / 2;

    let mut segments = Vec::new();

    // Vertical segment: connect node border to attachment point (1 cell)
    segments.push(Segment::Vertical {
        x: border_x,
        y_start: border_y,
        y_end: start.y,
    });

    // Horizontal segment: attachment point → corridor
    segments.push(Segment::Horizontal {
        y: start.y,
        x_start: start.x,
        x_end: corridor_x,
    });

    // Vertical segment in corridor: from start.y to end.y
    segments.push(Segment::Vertical {
        x: corridor_x,
        y_start: start.y,
        y_end: end.y,
    });

    // Horizontal segment: corridor → target right
    segments.push(Segment::Horizontal {
        y: end.y,
        x_start: corridor_x,
        x_end: end.x,
    });

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction: AttachDirection::Right,
    })
}

/// Route a backward edge for horizontal (LR/RL) layouts.
///
/// The edge exits from the bottom side of the source, travels left/right in a
/// corridor below the diagram, then enters the target from the bottom.
fn route_backward_edge_horizontal(
    edge: &Edge,
    from_bounds: &NodeBounds,
    to_bounds: &NodeBounds,
    layout: &Layout,
    _diagram_direction: Direction,
) -> Option<RoutedEdge> {
    // Exit from bottom side of source
    let start = attachment_point(from_bounds, AttachDirection::Bottom);
    // Enter from bottom side of target
    let end = attachment_point(to_bounds, AttachDirection::Bottom);

    // Get lane assignment for this edge (default to 0 if not found)
    let lane = layout
        .backward_edge_lanes
        .get(&(edge.from.clone(), edge.to.clone()))
        .copied()
        .unwrap_or(0);

    // Corridor Y position: each lane gets its own corridor space
    let content_height = layout.height - (layout.backward_corridors * layout.corridor_width);
    let corridor_y = content_height + (lane * layout.corridor_width) + layout.corridor_width / 2;

    let mut segments = Vec::new();

    // Vertical segment: source bottom → corridor
    segments.push(Segment::Vertical {
        x: start.x,
        y_start: start.y,
        y_end: corridor_y,
    });

    // Horizontal segment in corridor
    segments.push(Segment::Horizontal {
        y: corridor_y,
        x_start: start.x,
        x_end: end.x,
    });

    // Vertical segment: corridor → target bottom
    segments.push(Segment::Vertical {
        x: end.x,
        y_start: corridor_y,
        y_end: end.y,
    });

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction: AttachDirection::Bottom,
    })
}

/// Compute the path segments between two points.
fn compute_path(start: Point, end: Point, direction: Direction) -> Vec<Segment> {
    // For TD/BT layouts, prefer vertical-first routing
    // For LR/RL layouts, prefer horizontal-first routing
    match direction {
        Direction::TopDown | Direction::BottomTop => compute_vertical_first_path(start, end),
        Direction::LeftRight | Direction::RightLeft => compute_horizontal_first_path(start, end),
    }
}

/// Compute path preferring vertical movement first.
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

/// Compute path preferring horizontal movement first.
fn compute_horizontal_first_path(start: Point, end: Point) -> Vec<Segment> {
    let mut segments = Vec::new();

    if start.y == end.y {
        // Straight horizontal line
        segments.push(Segment::Horizontal {
            y: start.y,
            x_start: start.x,
            x_end: end.x,
        });
    } else if start.x == end.x {
        // Straight vertical line (shouldn't happen often in LR)
        segments.push(Segment::Vertical {
            x: start.x,
            y_start: start.y,
            y_end: end.y,
        });
    } else {
        // L-shaped or Z-shaped path
        let mid_x = if start.x < end.x {
            start.x + (end.x - start.x) / 2
        } else {
            end.x + (start.x - end.x) / 2
        };

        // Horizontal segment from start to midpoint
        segments.push(Segment::Horizontal {
            y: start.y,
            x_start: start.x,
            x_end: mid_x,
        });

        // Vertical segment at midpoint
        segments.push(Segment::Vertical {
            x: mid_x,
            y_start: start.y,
            y_end: end.y,
        });

        // Horizontal segment from midpoint to end
        segments.push(Segment::Horizontal {
            y: end.y,
            x_start: mid_x,
            x_end: end.x,
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
///                      If false, prefer horizontal-then-vertical routing (for LR/RL).
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

/// Route all edges in the layout.
pub fn route_all_edges(
    edges: &[Edge],
    layout: &Layout,
    diagram_direction: Direction,
) -> Vec<RoutedEdge> {
    edges
        .iter()
        .filter_map(|edge| route_edge(edge, layout, diagram_direction))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::layout::{LayoutConfig, compute_layout};
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
        let routed = route_edge(edge, &layout, Direction::TopDown).unwrap();

        // Should have at least one segment
        assert!(!routed.segments.is_empty());

        // For vertically aligned nodes, should be a single vertical segment
        if routed.start.x == routed.end.x {
            assert_eq!(routed.segments.len(), 1);
            match routed.segments[0] {
                Segment::Vertical { .. } => {}
                _ => panic!("Expected vertical segment"),
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
        let routed = route_edge(edge, &layout, Direction::TopDown).unwrap();

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

        // LR uses horizontal-first
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
        let routed = route_edge(backward_edge, &layout, Direction::TopDown).unwrap();

        // Backward edge should route around the right side
        assert_eq!(routed.entry_direction, AttachDirection::Right);

        // Should have 4 segments:
        // 1. vertical (connect node to attachment point)
        // 2. horizontal (to corridor)
        // 3. vertical (in corridor)
        // 4. horizontal (to target)
        assert_eq!(routed.segments.len(), 4);
        assert!(matches!(routed.segments[0], Segment::Vertical { .. }));
        assert!(matches!(routed.segments[1], Segment::Horizontal { .. }));
        assert!(matches!(routed.segments[2], Segment::Vertical { .. }));
        assert!(matches!(routed.segments[3], Segment::Horizontal { .. }));

        // The corridor x should be within canvas but in the corridor area
        let content_width = layout.width - (layout.backward_corridors * layout.corridor_width);
        if let Segment::Horizontal { x_end, .. } = routed.segments[1] {
            assert!(
                x_end > content_width,
                "Corridor should be beyond content area"
            );
            assert!(x_end < layout.width, "Corridor should be within canvas");
        }
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
        let routed = route_edge(backward_edge, &layout, Direction::LeftRight).unwrap();

        // Backward edge should route around the bottom
        assert_eq!(routed.entry_direction, AttachDirection::Bottom);

        // Should have 3 segments: vertical (to corridor), horizontal, vertical (back)
        assert_eq!(routed.segments.len(), 3);
        assert!(matches!(routed.segments[0], Segment::Vertical { .. }));
        assert!(matches!(routed.segments[1], Segment::Horizontal { .. }));
        assert!(matches!(routed.segments[2], Segment::Vertical { .. }));

        // The corridor y should be within canvas but in the corridor area
        let content_height = layout.height - (layout.backward_corridors * layout.corridor_width);
        if let Segment::Vertical { y_end, .. } = routed.segments[0] {
            assert!(
                y_end > content_height,
                "Corridor should be beyond content area"
            );
            assert!(y_end < layout.height, "Corridor should be within canvas");
        }
    }

    #[test]
    fn test_forward_edge_entry_direction_td() {
        // Forward edges should have standard entry direction
        let diagram = simple_td_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let edge = &diagram.edges[0];
        let routed = route_edge(edge, &layout, Direction::TopDown).unwrap();

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
        let routed = route_edge(edge, &layout, Direction::LeftRight).unwrap();

        // LR forward edges enter from Left
        assert_eq!(routed.entry_direction, AttachDirection::Left);
    }

    #[test]
    fn test_multiple_backward_edges_use_separate_lanes() {
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

        // Should have 2 backward corridors
        assert_eq!(layout.backward_corridors, 2);

        // Route both backward edges
        let edge_c_to_a = &diagram.edges[2];
        let edge_c_to_b = &diagram.edges[3];
        let routed_c_a = route_edge(edge_c_to_a, &layout, Direction::TopDown).unwrap();
        let routed_c_b = route_edge(edge_c_to_b, &layout, Direction::TopDown).unwrap();

        // Extract corridor X positions from the horizontal segment going to corridor (index 1)
        let corridor_x_ca = match routed_c_a.segments[1] {
            Segment::Horizontal { x_end, .. } => x_end,
            _ => panic!("Expected horizontal segment"),
        };
        let corridor_x_cb = match routed_c_b.segments[1] {
            Segment::Horizontal { x_end, .. } => x_end,
            _ => panic!("Expected horizontal segment"),
        };

        // The two backward edges should use different corridor lanes
        assert_ne!(
            corridor_x_ca, corridor_x_cb,
            "Backward edges should use different lanes"
        );
    }
}
