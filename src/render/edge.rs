//! Edge rendering on the canvas.

use std::collections::HashMap;

use super::canvas::{Canvas, Connections};
use super::chars::CharSet;
use super::router::{AttachDirection, Point, RoutedEdge, Segment};
use crate::graph::{Arrow, Direction, Stroke};

/// Render a routed edge onto the canvas.
pub fn render_edge(
    canvas: &mut Canvas,
    routed: &RoutedEdge,
    charset: &CharSet,
    diagram_direction: Direction,
) {
    let stroke = routed.edge.stroke;

    // Draw each segment
    for segment in &routed.segments {
        draw_segment(canvas, segment, stroke, charset);
    }

    // Draw arrow at the end point using entry direction
    if routed.edge.arrow != Arrow::None {
        draw_arrow_with_entry(canvas, &routed.end, routed.entry_direction, charset);
    }

    // Draw label if present
    if let Some(label) = &routed.edge.label {
        draw_edge_label_with_tracking(canvas, routed, label, diagram_direction, &[]);
    }
}

/// Draw a label on an edge at an appropriate position along the edge path.
///
/// For forward edges, places the label at the midpoint between start and end.
/// For backward edges (routed around perimeter), places the label along the
/// actual routed path (typically on the longest waypoint segment).
/// If the label would collide with a node or another label, tries alternative positions.
///
/// Returns the placed label's bounding box if successfully placed.
fn draw_edge_label_with_tracking(
    canvas: &mut Canvas,
    routed: &RoutedEdge,
    label: &str,
    direction: Direction,
    placed_labels: &[PlacedLabel],
) -> Option<PlacedLabel> {
    let label_len = label.chars().count();

    // Calculate base position for label.
    // `on_h_seg` tracks whether we placed above a horizontal segment,
    // which means edge cell collisions should be ignored (the label
    // intentionally overwrites the jog line).
    let mut on_h_seg = false;
    let (base_x, base_y) = {
        match direction {
            Direction::TopDown | Direction::BottomTop => {
                // For vertical layouts with Z-shaped paths (3+ segments),
                // place label on the best segment available.
                if routed.segments.len() >= 3 {
                    let is_long_path = routed.segments.len() >= 6;

                    // For short forward paths, prefer placing the label centered
                    // above a horizontal segment when it's wide enough. This keeps
                    // labels on the horizontal "jog" of Z-paths rather than beside
                    // short vertical stubs where they can crowd adjacent edges.
                    let h_seg = if !is_long_path {
                        routed
                            .segments
                            .iter()
                            .filter(|s| match s {
                                Segment::Horizontal { x_start, x_end, .. } => {
                                    // Require padding so the label doesn't touch the
                                    // turn characters at segment endpoints.
                                    x_start.abs_diff(*x_end) >= label_len + 2
                                }
                                _ => false,
                            })
                            .max_by_key(|s| match s {
                                Segment::Horizontal { x_start, x_end, .. } => {
                                    x_start.abs_diff(*x_end)
                                }
                                _ => 0,
                            })
                    } else {
                        None
                    };

                    if let Some(Segment::Horizontal { y, x_start, x_end }) = h_seg {
                        let seg_min_x = (*x_start).min(*x_end);
                        let seg_max_x = (*x_start).max(*x_end);
                        let seg_len = seg_max_x - seg_min_x;
                        let label_x = seg_min_x + (seg_len - label_len) / 2;
                        on_h_seg = true;
                        (label_x, *y)
                    } else {
                        // Fall back to vertical segment placement.
                        // For backward edges, prefer the longest inner vertical segment.
                        // For forward edges, prefer the longest vertical near the source.
                        let chosen_seg = select_label_segment(&routed.segments);

                        if let Some(seg) = chosen_seg {
                            // Determine which side to place the label based on target position
                            let mut place_right = routed.end.x > routed.start.x;

                            // Check if the proposed position would place the label between
                            // two attachment ports. If an edge cell exists on the far side
                            // of the label, flip sides.
                            let (trial_x, trial_y) = find_label_position_on_segment_with_side(
                                seg,
                                label_len,
                                place_right,
                            );
                            if label_adjacent_to_edge_on_far_side(
                                canvas,
                                trial_x,
                                trial_y,
                                label_len,
                                place_right,
                            ) {
                                place_right = !place_right;
                            }

                            find_label_position_on_segment_with_side(seg, label_len, place_right)
                        } else {
                            // Fallback to midpoint
                            let mid_y = (routed.start.y + routed.end.y) / 2;
                            (routed.end.x.saturating_sub(label_len / 2), mid_y)
                        }
                    }
                } else {
                    // Simple straight path - place label beside the edge line
                    let mid_y = (routed.start.y + routed.end.y) / 2;
                    // Place label to the left of the edge, not centered on it
                    // This avoids collision with the edge line
                    let label_x = routed.end.x.saturating_sub(label_len + 1);
                    (label_x, mid_y)
                }
            }
            Direction::LeftRight => {
                if routed.segments.len() >= 3 {
                    label_on_horizontal_segment(routed, label_len)
                } else {
                    // Short/straight path — keep existing inline placement
                    let mid_y = (routed.start.y + routed.end.y) / 2;
                    let max_label_end = routed.end.x.saturating_sub(1);
                    let min_x = routed.start.x.saturating_add(1);
                    let available = max_label_end.saturating_sub(routed.start.x);
                    let label_x = if available >= label_len {
                        let centered = routed.start.x + (available - label_len) / 2;
                        let max_x = max_label_end.saturating_sub(label_len);
                        centered.max(min_x).min(max_x)
                    } else {
                        min_x
                    };
                    (label_x, mid_y)
                }
            }
            Direction::RightLeft => {
                if routed.segments.len() >= 3 {
                    label_on_horizontal_segment(routed, label_len)
                } else {
                    // Short/straight path — keep existing inline placement
                    let mid_y = (routed.start.y + routed.end.y) / 2;
                    let mid_x = (routed.start.x + routed.end.x) / 2;
                    let label_x = mid_x.saturating_sub(label_len / 2);
                    let max_x = routed.start.x.saturating_sub(label_len + 1);
                    let min_x = routed.end.x.saturating_add(2);
                    let label_x = if max_x < min_x {
                        let available = routed.start.x.saturating_sub(routed.end.x);
                        if available >= label_len {
                            routed.end.x + (available - label_len) / 2
                        } else {
                            routed.end.x
                        }
                    } else {
                        label_x.max(min_x).min(max_x)
                    };
                    (label_x, mid_y)
                }
            }
        }
    };

    // Try to find a position that doesn't collide with nodes or other labels.
    // When placed above a horizontal segment, skip edge collision checks since
    // the label intentionally overwrites edge cells on the jog line.
    let (label_x, label_y) = find_safe_label_position(
        canvas,
        base_x,
        base_y,
        label_len,
        direction,
        placed_labels,
        !on_h_seg,
    );
    // Write the label only to non-node cells, avoiding the arrow position
    // Labels can overwrite edge cells since they're drawn after edges and should appear on top
    // For horizontal layouts, don't overwrite the arrow at routed.end
    let arrow_pos = (routed.end.x, routed.end.y);
    for (i, ch) in label.chars().enumerate() {
        let x = label_x + i;
        // Skip if this would overwrite the arrow
        if x == arrow_pos.0 && label_y == arrow_pos.1 {
            continue;
        }
        // Only write if cell is not part of a node (but edge cells can be overwritten)
        let cell = canvas.get(x, label_y);
        let can_write = cell.is_some_and(|cell| !cell.is_node);
        if can_write {
            canvas.set(x, label_y, ch);
        }
    }

    Some(PlacedLabel {
        x: label_x,
        y: label_y,
        len: label_len,
    })
}

/// Position a label above the best horizontal segment for LR/RL multi-segment edges.
///
/// Shared by both LeftRight and RightLeft layout branches. Centers the label on
/// the widest horizontal segment when possible, otherwise falls back to the
/// midpoint between source and target anchored to the source y.
fn label_on_horizontal_segment(routed: &RoutedEdge, label_len: usize) -> (usize, usize) {
    if let Some(Segment::Horizontal { y, x_start, x_end }) =
        select_label_segment_horizontal(&routed.segments)
    {
        let seg_min_x = (*x_start).min(*x_end);
        let seg_max_x = (*x_start).max(*x_end);
        let seg_len = seg_max_x - seg_min_x;
        let label_x = if seg_len >= label_len {
            seg_min_x + (seg_len - label_len) / 2
        } else {
            seg_min_x
        };
        (label_x, y.saturating_sub(1))
    } else {
        // Anchor y to source exit point, not averaged midpoint
        let anchor_y = routed.start.y.saturating_sub(1);
        let mid_x = (routed.start.x + routed.end.x) / 2;
        (mid_x.saturating_sub(label_len / 2), anchor_y)
    }
}

/// Find the label position on a segment, with control over which side to place it.
///
/// Only used for TD/BT layouts where edges have Z-shaped paths. LR/RL layouts
/// use inline label positioning with collision avoidance via `find_safe_label_position`.
///
/// For vertical segments (the typical case in TD/BT):
/// - `place_right = false`: label goes to the left of the segment
/// - `place_right = true`: label goes to the right of the segment
///
/// For horizontal segments (middle of Z-paths): label is placed above the segment.
fn find_label_position_on_segment_with_side(
    segment: &Segment,
    label_len: usize,
    place_right: bool,
) -> (usize, usize) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => {
            let mid_y = (*y_start + *y_end) / 2;
            if place_right {
                // Place label to the right of the vertical line (1-space gap)
                (*x + 2, mid_y)
            } else {
                // Place label to the left of the vertical line
                // Prefer 1-space gap if there's room, otherwise place adjacent
                let needed_with_gap = label_len + 1;
                let label_x = if *x >= needed_with_gap {
                    x - needed_with_gap // 1-space gap
                } else {
                    x.saturating_sub(label_len) // no gap, tight fit
                };
                (label_x, mid_y)
            }
        }
        Segment::Horizontal { y, x_start, x_end } => {
            // For horizontal segments, place label above
            let mid_x = (*x_start + *x_end) / 2;
            let label_x = mid_x.saturating_sub(label_len / 2);
            (label_x, y.saturating_sub(1))
        }
    }
}

/// Find a safe position for an edge label that doesn't collide with nodes or other labels.
///
/// Tries the base position first, then shifts in the appropriate direction
/// based on the diagram layout until a collision-free position is found.
///
/// When `check_edge_collision` is false, labels can be placed over edge cells
/// (useful when intentionally centering above a horizontal segment where the
/// label is expected to overwrite the jog line).
fn find_safe_label_position(
    canvas: &Canvas,
    base_x: usize,
    base_y: usize,
    label_len: usize,
    direction: Direction,
    placed_labels: &[PlacedLabel],
    check_edge_collision: bool,
) -> (usize, usize) {
    let has_collision = |x, y| {
        label_collides_with_node(canvas, x, y, label_len)
            || (check_edge_collision && label_collides_with_edge(canvas, x, y, label_len))
            || placed_labels.iter().any(|p| p.overlaps(x, y, label_len))
    };

    // Check if the base position has any collision
    if !has_collision(base_x, base_y) {
        return (base_x, base_y);
    }

    // Try shifting positions based on diagram direction
    const VERTICAL_SHIFTS: &[(isize, isize)] = &[
        (0, -1),
        (0, 1),
        (0, -2),
        (0, 2),
        (-1, 0),
        (1, 0),
        (-2, 0),
        (2, 0),
        (0, -3),
        (0, 3),
        (-3, 0),
        (3, 0),
    ];
    const HORIZONTAL_SHIFTS: &[(isize, isize)] = &[
        (0, -1),
        (0, 1),
        (0, -2),
        (0, 2),
        (-1, 0),
        (1, 0),
        (0, -3),
        (0, 3),
    ];
    let shifts = match direction {
        Direction::TopDown | Direction::BottomTop => VERTICAL_SHIFTS,
        Direction::LeftRight | Direction::RightLeft => HORIZONTAL_SHIFTS,
    };

    // Try each shift until we find a collision-free position
    for (dx, dy) in shifts {
        let new_x = (base_x as isize + dx).max(0) as usize;
        let new_y = (base_y as isize + dy).max(0) as usize;

        if !has_collision(new_x, new_y) {
            return (new_x, new_y);
        }
    }

    // If all shifts fail, return the base position (will skip node cells when writing)
    (base_x, base_y)
}

/// Check if placing a label at the given position would collide with any node cells.
fn label_collides_with_node(canvas: &Canvas, x: usize, y: usize, label_len: usize) -> bool {
    (0..label_len).any(|i| canvas.get(x + i, y).is_some_and(|cell| cell.is_node))
}

/// Check if placing a label at the given position would collide with any edge cells.
fn label_collides_with_edge(canvas: &Canvas, x: usize, y: usize, label_len: usize) -> bool {
    (0..label_len).any(|i| canvas.get(x + i, y).is_some_and(|cell| cell.is_edge))
}

/// Check if an edge cell exists on the far side of a proposed label position.
///
/// When a label is placed next to a vertical segment, this detects whether
/// there's another edge nearby on the opposite side, which would mean the
/// label is sandwiched between two attachment ports (visually ambiguous).
///
/// `place_right` indicates the side the label was placed on relative to its segment.
/// We check the far side (right edge of label if place_right, left edge if !place_right).
fn label_adjacent_to_edge_on_far_side(
    canvas: &Canvas,
    label_x: usize,
    label_y: usize,
    label_len: usize,
    place_right: bool,
) -> bool {
    if place_right {
        // Label is to the right of its segment; check cells just after the label end
        let check_x = label_x + label_len;
        (0..=1).any(|offset| {
            canvas
                .get(check_x + offset, label_y)
                .is_some_and(|cell| cell.is_edge)
        })
    } else {
        // Label is to the left of its segment; check cells just before the label start
        (1..=2).any(|offset| {
            label_x
                .checked_sub(offset)
                .and_then(|x| canvas.get(x, label_y))
                .is_some_and(|cell| cell.is_edge)
        })
    }
}

/// Return the inner segments of an edge path, excluding the first and last
/// stub segments near the source and target nodes. Falls back to the full
/// slice when there are 2 or fewer segments.
fn inner_segments(segments: &[Segment]) -> &[Segment] {
    if segments.len() > 2 {
        &segments[1..segments.len() - 1]
    } else {
        segments
    }
}

/// Select the best segment for placing a label on a multi-segment edge.
///
/// For forward edges (few segments), returns the last vertical segment
/// approaching the target — labels near the target are clear for short paths.
///
/// For backward edges (many segments routed via dagre waypoints), returns the
/// longest vertical segment. This is typically the long waypoint path spanning
/// multiple ranks, which is isolated from other edges and avoids crowding near
/// the target node.
fn select_label_segment(segments: &[Segment]) -> Option<&Segment> {
    // Backward edges routed through dagre waypoints typically have 6+ segments
    // (exit source, horizontal turns, long vertical spans, horizontal to target,
    // enter target). Forward Z-paths typically have 3-4 segments.
    let is_long_path = segments.len() >= 6;

    if is_long_path {
        // For long paths (backward edges), find the longest vertical segment.
        // Skip the first and last segments (they're short stubs near nodes).
        let inner = inner_segments(segments);
        inner
            .iter()
            .filter(|s| matches!(s, Segment::Vertical { .. }))
            .max_by_key(|s| match s {
                Segment::Vertical { y_start, y_end, .. } => (*y_start).abs_diff(*y_end),
                _ => 0,
            })
            .or_else(|| {
                // Fallback: last vertical segment
                segments
                    .iter()
                    .rev()
                    .find(|s| matches!(s, Segment::Vertical { .. }))
            })
    } else {
        // For short paths (forward edges), prefer the longest vertical segment
        // nearest to the source node. Iterating in reverse makes max_by_key's
        // last-wins tie-breaking favor earlier segments, placing labels near
        // the source where branching originates rather than near the target
        // where sibling-edge labels cluster.
        segments
            .iter()
            .rev()
            .filter(|s| matches!(s, Segment::Vertical { .. }))
            .max_by_key(|s| match s {
                Segment::Vertical { y_start, y_end, .. } => (*y_start).abs_diff(*y_end),
                _ => 0,
            })
    }
}

/// Select the best horizontal segment for label placement on LR/RL edges.
///
/// Analogous to `select_label_segment()` for TD/BT vertical segments.
/// For long paths (backward edges, 6+ segments), returns the longest inner horizontal segment.
/// For shorter paths, returns the last horizontal segment.
fn select_label_segment_horizontal(segments: &[Segment]) -> Option<&Segment> {
    let is_long_path = segments.len() >= 6;

    if is_long_path {
        let inner = inner_segments(segments);
        inner
            .iter()
            .filter(|s| matches!(s, Segment::Horizontal { .. }))
            .max_by_key(|s| match s {
                Segment::Horizontal { x_start, x_end, .. } => x_start.abs_diff(*x_end),
                _ => 0,
            })
            .or_else(|| {
                segments
                    .iter()
                    .rev()
                    .find(|s| matches!(s, Segment::Horizontal { .. }))
            })
    } else {
        // For LR/RL short paths, the last horizontal segment approaches the
        // target at a unique Y position, so labels on sibling edges naturally
        // separate vertically.
        segments
            .iter()
            .rev()
            .find(|s| matches!(s, Segment::Horizontal { .. }))
    }
}

/// Calculate the length of a segment.
#[cfg(test)]
fn segment_length(segment: &Segment) -> usize {
    match segment {
        Segment::Vertical { y_start, y_end, .. } => y_start.abs_diff(*y_end),
        Segment::Horizontal { x_start, x_end, .. } => x_start.abs_diff(*x_end),
    }
}

/// Calculate the midpoint of a segment.
#[cfg(test)]
fn segment_midpoint(segment: &Segment) -> (usize, usize) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => (*x, (*y_start + *y_end) / 2),
        Segment::Horizontal { y, x_start, x_end } => ((*x_start + *x_end) / 2, *y),
    }
}

/// Draw a single segment on the canvas.
///
/// TODO: Use `stroke` to render dotted (`┄`/`┆`) and thick edges differently.
/// The `CharSet` already has `dotted_horizontal` and `dotted_vertical` characters;
/// this function should select them based on `Stroke::Dotted` vs `Stroke::Solid`.
fn draw_segment(canvas: &mut Canvas, segment: &Segment, _stroke: Stroke, charset: &CharSet) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => {
            let (y_min, y_max) = if y_start < y_end {
                (*y_start, *y_end)
            } else {
                (*y_end, *y_start)
            };

            for y in y_min..=y_max {
                let connections = Connections {
                    up: y > y_min,
                    down: y < y_max,
                    left: false,
                    right: false,
                };
                canvas.set_with_connection(*x, y, connections, charset);
            }
        }
        Segment::Horizontal { y, x_start, x_end } => {
            let (x_min, x_max) = if x_start < x_end {
                (*x_start, *x_end)
            } else {
                (*x_end, *x_start)
            };

            for x in x_min..=x_max {
                let connections = Connections {
                    up: false,
                    down: false,
                    left: x > x_min,
                    right: x < x_max,
                };
                canvas.set_with_connection(x, *y, connections, charset);
            }
        }
    }
}

/// Draw an arrow at the given point based on entry direction.
///
/// The arrow points in the direction the edge is coming from (into the target).
fn draw_arrow_with_entry(
    canvas: &mut Canvas,
    point: &Point,
    entry_direction: AttachDirection,
    charset: &CharSet,
) {
    // Protect node content from being overwritten by arrows
    if let Some(cell) = canvas.get(point.x, point.y) {
        if cell.is_node {
            return;
        }
    }

    // Arrow points in the direction the edge enters FROM
    // Entry from Top means edge is going down, so arrow points down
    // Entry from Right means edge is going left, so arrow points left
    let arrow_char = match entry_direction {
        AttachDirection::Top => charset.arrow_down,
        AttachDirection::Bottom => charset.arrow_up,
        AttachDirection::Left => charset.arrow_right,
        AttachDirection::Right => charset.arrow_left,
    };

    canvas.set(point.x, point.y, arrow_char);
}

/// Draw an arrow at the given point (legacy function for tests).
#[cfg(test)]
fn draw_arrow(canvas: &mut Canvas, point: &Point, direction: Direction, charset: &CharSet) {
    let arrow_char = match direction {
        Direction::TopDown => charset.arrow_down,
        Direction::BottomTop => charset.arrow_up,
        Direction::LeftRight => charset.arrow_right,
        Direction::RightLeft => charset.arrow_left,
    };

    canvas.set(point.x, point.y, arrow_char);
}

/// A placed label's bounding box for collision detection.
#[derive(Debug, Clone)]
struct PlacedLabel {
    x: usize,
    y: usize,
    len: usize,
}

impl PlacedLabel {
    /// Check if this label overlaps with a proposed label position.
    fn overlaps(&self, x: usize, y: usize, len: usize) -> bool {
        // Labels only collide if on the same row
        if self.y != y {
            return false;
        }
        // Check horizontal overlap
        let self_end = self.x + self.len;
        let other_end = x + len;
        // Overlap if ranges intersect
        x < self_end && self.x < other_end
    }
}

/// Render all edges onto the canvas.
///
/// Draws all segments and arrows first, then all labels, ensuring labels
/// are not overwritten by later edge segments.
///
/// # Arguments
/// * `canvas` - The canvas to draw on
/// * `routed_edges` - The edges to render
/// * `charset` - Character set for drawing
/// * `diagram_direction` - Layout direction for label positioning
/// * `label_positions` - Optional pre-computed label positions from normalization
pub fn render_all_edges(
    canvas: &mut Canvas,
    routed_edges: &[RoutedEdge],
    charset: &CharSet,
    diagram_direction: Direction,
) {
    render_all_edges_with_labels(
        canvas,
        routed_edges,
        charset,
        diagram_direction,
        &HashMap::new(),
    )
}

/// Render all edges with optional pre-computed label positions.
pub fn render_all_edges_with_labels(
    canvas: &mut Canvas,
    routed_edges: &[RoutedEdge],
    charset: &CharSet,
    diagram_direction: Direction,
    label_positions: &HashMap<(String, String), (usize, usize)>,
) {
    // First pass: draw all segments and arrows
    for routed in routed_edges {
        for segment in &routed.segments {
            draw_segment(canvas, segment, routed.edge.stroke, charset);
        }
        if routed.edge.arrow != Arrow::None {
            draw_arrow_with_entry(canvas, &routed.end, routed.entry_direction, charset);
        }
    }

    // Second pass: draw all labels (so they appear on top of segments)
    // Track placed labels to avoid collisions
    let mut placed_labels: Vec<PlacedLabel> = Vec::new();
    for routed in routed_edges {
        if let Some(label) = &routed.edge.label {
            // Check for pre-computed label position from normalization
            let edge_key = (routed.edge.from.clone(), routed.edge.to.clone());
            let label_len = label.chars().count();

            // Use precomputed position if available and within canvas bounds,
            // otherwise fall back to heuristic placement.
            let precomputed = label_positions.get(&edge_key).filter(|&&(px, py)| {
                px < canvas.width()
                    && py < canvas.height()
                    && px.saturating_add(label_len) <= canvas.width()
            });

            let placed = if let Some(&(pre_x, pre_y)) = precomputed {
                draw_label_at_position(canvas, label, pre_x, pre_y)
            } else {
                draw_edge_label_with_tracking(
                    canvas,
                    routed,
                    label,
                    diagram_direction,
                    &placed_labels,
                )
            };

            if let Some(p) = placed {
                placed_labels.push(p);
            }
        }
    }
}

/// Draw a label at a specific pre-computed position.
fn draw_label_at_position(
    canvas: &mut Canvas,
    label: &str,
    x: usize,
    y: usize,
) -> Option<PlacedLabel> {
    let label_len = label.chars().count();
    // Center the label on the given position
    let label_x = x.saturating_sub(label_len / 2);

    // Write the label only to non-node cells (but edge cells can be overwritten)
    for (i, ch) in label.chars().enumerate() {
        let cell_x = label_x + i;
        if canvas.get(cell_x, y).is_some_and(|cell| !cell.is_node) {
            canvas.set(cell_x, y, ch);
        }
    }

    Some(PlacedLabel {
        x: label_x,
        y,
        len: label_len,
    })
}

#[cfg(test)]
mod tests {
    use super::super::layout::{LayoutConfig, compute_layout_direct};
    use super::super::router::route_edge;
    use super::*;
    use crate::graph::{Diagram, Edge, Node};

    fn simple_diagram() -> Diagram {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B"));
        diagram
    }

    #[test]
    fn test_render_vertical_edge() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed =
            route_edge(&diagram.edges[0], &layout, Direction::TopDown, None, None).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        let output = canvas.to_string();
        // Should contain vertical line character or arrow
        assert!(output.contains('│') || output.contains('▼'));
    }

    #[test]
    fn test_render_edge_with_arrow() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed =
            route_edge(&diagram.edges[0], &layout, Direction::TopDown, None, None).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        let output = canvas.to_string();
        // Should contain down arrow for TD direction
        assert!(output.contains('▼'));
    }

    #[test]
    fn test_render_dotted_edge() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("A"));
        diagram.add_node(Node::new("B").with_label("B"));
        diagram.add_edge(Edge::new("A", "B").with_stroke(Stroke::Dotted));

        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed =
            route_edge(&diagram.edges[0], &layout, Direction::TopDown, None, None).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        // Dotted edge should be drawn (may or may not be visible depending on layout)
        // Just verify it doesn't crash
        let _output = canvas.to_string();
    }

    #[test]
    fn test_render_edge_without_arrow() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("A"));
        diagram.add_node(Node::new("B").with_label("B"));
        diagram.add_edge(Edge::new("A", "B").with_arrow(Arrow::None));

        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed =
            route_edge(&diagram.edges[0], &layout, Direction::TopDown, None, None).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        let output = canvas.to_string();
        // Should NOT contain arrow for Arrow::None
        assert!(!output.contains('▼'));
    }

    #[test]
    fn test_draw_arrow_directions() {
        let charset = CharSet::unicode();

        // Test each direction
        let mut canvas = Canvas::new(10, 10);
        draw_arrow(&mut canvas, &Point::new(1, 1), Direction::TopDown, &charset);
        assert_eq!(canvas.get(1, 1).unwrap().ch, '▼');

        let mut canvas = Canvas::new(10, 10);
        draw_arrow(
            &mut canvas,
            &Point::new(1, 1),
            Direction::BottomTop,
            &charset,
        );
        assert_eq!(canvas.get(1, 1).unwrap().ch, '▲');

        let mut canvas = Canvas::new(10, 10);
        draw_arrow(
            &mut canvas,
            &Point::new(1, 1),
            Direction::LeftRight,
            &charset,
        );
        assert_eq!(canvas.get(1, 1).unwrap().ch, '►');

        let mut canvas = Canvas::new(10, 10);
        draw_arrow(
            &mut canvas,
            &Point::new(1, 1),
            Direction::RightLeft,
            &charset,
        );
        assert_eq!(canvas.get(1, 1).unwrap().ch, '◄');
    }

    #[test]
    fn test_render_all_edges() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed_edges: Vec<_> = diagram
            .edges
            .iter()
            .filter_map(|e| route_edge(e, &layout, Direction::TopDown, None, None))
            .collect();

        render_all_edges(&mut canvas, &routed_edges, &charset, Direction::TopDown);

        // Should have rendered something
        let output = canvas.to_string();
        assert!(!output.trim().is_empty());
    }

    #[test]
    fn test_render_edge_with_label() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B").with_label("Yes"));

        let output = crate::render::render(
            &diagram,
            &crate::render::RenderOptions { ascii_only: false },
        );
        // Should contain the label
        assert!(output.contains("Yes"));
    }

    #[test]
    fn test_label_rendered_at_precomputed_position() {
        let output = crate::render::render(
            &{
                let mut d = Diagram::new(Direction::TopDown);
                d.add_node(Node::new("A").with_label("A"));
                d.add_node(Node::new("B").with_label("B"));
                d.add_edge(Edge::new("A", "B").with_label("yes"));
                d
            },
            &crate::render::RenderOptions { ascii_only: false },
        );

        assert!(output.contains("yes"), "Label 'yes' should be rendered");

        // Label should appear between A and B rows
        let lines: Vec<&str> = output.lines().collect();
        let a_line = lines.iter().position(|l| l.contains('A')).unwrap();
        let b_line = lines.iter().rposition(|l| l.contains('B')).unwrap();
        let yes_line = lines.iter().position(|l| l.contains("yes")).unwrap();
        assert!(
            yes_line > a_line && yes_line < b_line,
            "Label at line {} should be between A (line {}) and B (line {})\n{}",
            yes_line,
            a_line,
            b_line,
            output
        );
    }

    #[test]
    fn test_labeled_edge_has_waypoints() {
        // Verify a labeled short edge (A->B) now produces waypoints
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        diagram.add_edge(Edge::new("A", "B").with_label("yes"));

        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        // The A->B edge should have waypoints from the label dummy
        let edge_key = ("A".to_string(), "B".to_string());
        assert!(
            layout.edge_waypoints.contains_key(&edge_key),
            "Labeled short edge should have waypoints from label dummy"
        );
    }

    #[test]
    fn test_segment_length() {
        let vertical = Segment::Vertical {
            x: 5,
            y_start: 10,
            y_end: 20,
        };
        assert_eq!(segment_length(&vertical), 10);

        let horizontal = Segment::Horizontal {
            y: 5,
            x_start: 20,
            x_end: 10,
        };
        assert_eq!(segment_length(&horizontal), 10);
    }

    #[test]
    fn test_segment_midpoint() {
        let vertical = Segment::Vertical {
            x: 5,
            y_start: 10,
            y_end: 20,
        };
        assert_eq!(segment_midpoint(&vertical), (5, 15));

        let horizontal = Segment::Horizontal {
            y: 5,
            x_start: 10,
            x_end: 20,
        };
        assert_eq!(segment_midpoint(&horizontal), (15, 5));
    }

    #[test]
    fn test_label_collides_with_edge() {
        let mut canvas = Canvas::new(20, 10);
        let charset = CharSet::unicode();

        // Draw a horizontal edge segment
        let connections = Connections {
            up: false,
            down: false,
            left: true,
            right: true,
        };
        for x in 5..15 {
            canvas.set_with_connection(x, 5, connections, &charset);
        }

        // Label at y=5 should collide with edge
        assert!(label_collides_with_edge(&canvas, 7, 5, 5));

        // Label at y=4 should not collide
        assert!(!label_collides_with_edge(&canvas, 7, 4, 5));

        // Label at y=6 should not collide
        assert!(!label_collides_with_edge(&canvas, 7, 6, 5));

        // Partial overlap still collides
        assert!(label_collides_with_edge(&canvas, 3, 5, 5)); // ends at x=7, overlapping edge at x=5-7
    }

    #[test]
    fn test_select_label_segment_horizontal_short_path() {
        // 3-segment H-V-H forward path
        let segments = vec![
            Segment::Horizontal {
                y: 5,
                x_start: 10,
                x_end: 20,
            },
            Segment::Vertical {
                x: 20,
                y_start: 5,
                y_end: 10,
            },
            Segment::Horizontal {
                y: 10,
                x_start: 20,
                x_end: 30,
            },
        ];
        let chosen = select_label_segment_horizontal(&segments);
        // For short paths, should return the last horizontal segment
        match chosen {
            Some(Segment::Horizontal { y, .. }) => assert_eq!(*y, 10),
            _ => panic!("Expected last horizontal segment at y=10"),
        }
    }

    #[test]
    fn test_select_label_segment_horizontal_long_path() {
        // 7-segment backward edge path
        let segments = vec![
            Segment::Horizontal {
                y: 3,
                x_start: 50,
                x_end: 55,
            }, // short exit stub
            Segment::Vertical {
                x: 55,
                y_start: 3,
                y_end: 12,
            },
            Segment::Horizontal {
                y: 12,
                x_start: 55,
                x_end: 5,
            }, // long bottom run (50 chars)
            Segment::Vertical {
                x: 5,
                y_start: 12,
                y_end: 3,
            },
            Segment::Horizontal {
                y: 3,
                x_start: 5,
                x_end: 10,
            }, // short entry stub
            Segment::Vertical {
                x: 10,
                y_start: 3,
                y_end: 5,
            },
            Segment::Horizontal {
                y: 5,
                x_start: 10,
                x_end: 15,
            },
        ];
        let chosen = select_label_segment_horizontal(&segments);
        // For long paths, should return the longest inner horizontal segment (50 chars at y=12)
        match chosen {
            Some(Segment::Horizontal { y, .. }) => assert_eq!(*y, 12),
            _ => panic!("Expected longest inner horizontal segment at y=12"),
        }
    }

    #[test]
    fn test_lr_label_placement_near_edge_segment() {
        use crate::graph::Direction;

        let mut diagram = Diagram::new(Direction::LeftRight);
        diagram.add_node(Node::new("A").with_label("Start"));
        diagram.add_node(Node::new("B").with_label("End"));
        let mut edge = Edge::new("A", "B");
        edge.label = Some("test".to_string());
        diagram.add_edge(edge);

        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);
        let charset = CharSet::unicode();

        let routed =
            route_edge(&diagram.edges[0], &layout, Direction::LeftRight, None, None).unwrap();

        // Check that the routed edge has segments
        assert!(
            !routed.segments.is_empty(),
            "Routed edge should have segments"
        );

        // Render the edge with label
        let mut canvas = Canvas::new(layout.width, layout.height);
        render_edge(&mut canvas, &routed, &charset, Direction::LeftRight);

        let output = canvas.to_string();
        // The label "test" should appear in the output
        assert!(
            output.contains("test"),
            "Label 'test' should appear in output:\n{}",
            output
        );

        // Find where "test" appears and where edge chars appear in the output
        let lines: Vec<&str> = output.lines().collect();
        let label_line = lines
            .iter()
            .position(|l| l.contains("test"))
            .expect("Label should be on some line");

        // Find lines with edge characters (horizontal segments appear as ─ or -)
        let edge_lines: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains('─') || l.contains('►') || l.contains('-'))
            .map(|(i, _)| i)
            .collect();

        // The label should be within 1 row of an actual edge line
        let near_edge = edge_lines.iter().any(|&ey| ey.abs_diff(label_line) <= 1);
        assert!(
            near_edge,
            "Label at line {} should be within 1 row of an edge line (edge lines at {:?})",
            label_line, edge_lines
        );
    }

    #[test]
    fn test_select_label_segment_horizontal_no_horizontal() {
        // Edge case: only vertical segments
        let segments = vec![Segment::Vertical {
            x: 5,
            y_start: 0,
            y_end: 10,
        }];
        let chosen = select_label_segment_horizontal(&segments);
        assert!(
            chosen.is_none(),
            "Should return None when no horizontal segments exist"
        );
    }

    #[test]
    fn draw_arrow_does_not_overwrite_node_content() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(10, 10);

        // Mark a cell as node content
        canvas.set(5, 5, 'X');
        canvas.mark_as_node(5, 5);

        // Try to draw an arrow at the same position
        let point = Point { x: 5, y: 5 };
        draw_arrow_with_entry(&mut canvas, &point, AttachDirection::Top, &charset);

        // The cell should still contain 'X', not an arrow
        let cell = canvas.get(5, 5).unwrap();
        assert_eq!(cell.ch, 'X', "Arrow should not overwrite node content");
        assert!(cell.is_node, "Cell should still be marked as node");
    }

    #[test]
    fn draw_arrow_writes_on_non_node_cell() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(10, 10);

        // Draw an arrow on an empty cell (no node)
        let point = Point { x: 5, y: 5 };
        draw_arrow_with_entry(&mut canvas, &point, AttachDirection::Top, &charset);

        // Should succeed — arrow should be drawn
        let cell = canvas.get(5, 5).unwrap();
        assert_eq!(
            cell.ch, charset.arrow_down,
            "Arrow should be drawn on empty cell"
        );
    }
}
