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
/// actual routed path (typically on the corridor segment).
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

    // Detect if this is a backward edge (going against layout direction)
    let is_backward = match direction {
        Direction::LeftRight => routed.end.x < routed.start.x,
        Direction::RightLeft => routed.end.x > routed.start.x,
        Direction::TopDown => routed.end.y < routed.start.y,
        Direction::BottomTop => routed.end.y > routed.start.y,
    };

    // Calculate base position for label
    let (base_x, base_y) = if is_backward && routed.segments.len() >= 4 {
        // For backward edges with 4 segments (connector + 3 routing segments),
        // place label on segment[2] which is the long corridor segment
        find_label_position_on_segment(&routed.segments[2], label_len, direction)
    } else if is_backward && routed.segments.len() == 3 {
        // For backward edges with 3 segments, use segment[1] (the corridor)
        find_label_position_on_segment(&routed.segments[1], label_len, direction)
    } else {
        // For forward edges, use midpoint between start and end
        let mid_x = (routed.start.x + routed.end.x) / 2;
        let mid_y = (routed.start.y + routed.end.y) / 2;
        let label_x = mid_x.saturating_sub(label_len / 2);

        // For horizontal layouts, ensure label doesn't touch source or target
        // by leaving at least 1 cell padding on each side when there's room
        let label_x = match direction {
            Direction::LeftRight => {
                // Source connector is at start.x, arrow at end.x
                // The label should not overlap the arrow, so it must end before end.x
                let max_label_end = routed.end.x.saturating_sub(1);
                let min_x = routed.start.x.saturating_add(1);

                // Available space for the label (between source and arrow)
                let available = max_label_end.saturating_sub(routed.start.x);

                if available >= label_len {
                    // Enough room - center the label with padding
                    let centered = routed.start.x + (available - label_len) / 2;
                    let max_x = max_label_end.saturating_sub(label_len);
                    centered.max(min_x).min(max_x)
                } else {
                    // Not enough room - place at start, accepting overlap
                    // The label will be clipped when it reaches node cells
                    min_x
                }
            }
            Direction::RightLeft => {
                // Source at start.x (high x), arrow at end.x (low x)
                let max_x = routed.start.x.saturating_sub(label_len + 1);
                let min_x = routed.end.x.saturating_add(2);

                if max_x < min_x {
                    // Not enough room, center as best we can
                    let available = routed.start.x.saturating_sub(routed.end.x);
                    if available >= label_len {
                        routed.end.x + (available - label_len) / 2
                    } else {
                        routed.end.x
                    }
                } else {
                    label_x.max(min_x).min(max_x)
                }
            }
            _ => label_x,
        };

        (label_x, mid_y)
    };

    // Try to find a position that doesn't collide with nodes or other labels
    let (label_x, label_y) =
        find_safe_label_position(canvas, base_x, base_y, label_len, direction, placed_labels);

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
        if canvas.get(x, label_y).is_some_and(|cell| !cell.is_node) {
            canvas.set(x, label_y, ch);
        }
    }

    Some(PlacedLabel {
        x: label_x,
        y: label_y,
        len: label_len,
    })
}

/// Find the label position on a specific segment (for backward edge labels).
///
/// Places the label at the midpoint of the segment, offset appropriately
/// based on segment orientation and diagram direction.
fn find_label_position_on_segment(
    segment: &Segment,
    label_len: usize,
    direction: Direction,
) -> (usize, usize) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => {
            // For vertical segments (corridor in LR/RL layouts), place label to the left
            let mid_y = (*y_start + *y_end) / 2;
            // Offset label to the left of the vertical line
            (x.saturating_sub(label_len + 1), mid_y)
        }
        Segment::Horizontal { y, x_start, x_end } => {
            // For horizontal segments (corridor in TD/BT layouts), place label above/below
            let mid_x = (*x_start + *x_end) / 2;
            let label_x = mid_x.saturating_sub(label_len / 2);
            // For TD, corridor is on right side going up, place label to left of line
            // For BT, similar logic
            match direction {
                Direction::TopDown | Direction::BottomTop => {
                    // Vertical layout: horizontal corridor segment, place above the line
                    (label_x, y.saturating_sub(1))
                }
                Direction::LeftRight | Direction::RightLeft => {
                    // Horizontal layout: this is the main corridor segment below diagram
                    // Place label above the line
                    (label_x, y.saturating_sub(1))
                }
            }
        }
    }
}

/// Find a safe position for an edge label that doesn't collide with nodes or other labels.
///
/// Tries the base position first, then shifts in the appropriate direction
/// based on the diagram layout until a collision-free position is found.
fn find_safe_label_position(
    canvas: &Canvas,
    base_x: usize,
    base_y: usize,
    label_len: usize,
    direction: Direction,
    placed_labels: &[PlacedLabel],
) -> (usize, usize) {
    // Check if the base position has any collision
    if !label_has_collision(canvas, base_x, base_y, label_len, placed_labels) {
        return (base_x, base_y);
    }

    // Try shifting positions based on diagram direction
    let shifts: Vec<(isize, isize)> = match direction {
        Direction::TopDown | Direction::BottomTop => {
            // For vertical layouts, try up/down shifts first, then left/right
            vec![
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
            ]
        }
        Direction::LeftRight | Direction::RightLeft => {
            // For horizontal layouts, try up/down shifts first
            vec![
                (0, -1),
                (0, 1),
                (0, -2),
                (0, 2),
                (-1, 0),
                (1, 0),
                (0, -3),
                (0, 3),
            ]
        }
    };

    // Try each shift until we find a collision-free position
    for (dx, dy) in shifts {
        let new_x = (base_x as isize + dx).max(0) as usize;
        let new_y = (base_y as isize + dy).max(0) as usize;

        if !label_has_collision(canvas, new_x, new_y, label_len, placed_labels) {
            return (new_x, new_y);
        }
    }

    // If all shifts fail, return the base position (will skip node cells when writing)
    (base_x, base_y)
}

/// Check if placing a label at the given position would collide with any node cells, edge cells, or other labels.
fn label_has_collision(
    canvas: &Canvas,
    x: usize,
    y: usize,
    label_len: usize,
    placed_labels: &[PlacedLabel],
) -> bool {
    // Check collision with nodes
    if label_collides_with_node(canvas, x, y, label_len) {
        return true;
    }
    // Check collision with edge path characters
    if label_collides_with_edge(canvas, x, y, label_len) {
        return true;
    }
    // Check collision with already placed labels
    for placed in placed_labels {
        if placed.overlaps(x, y, label_len) {
            return true;
        }
    }
    false
}

/// Check if placing a label at the given position would collide with any node cells.
fn label_collides_with_node(canvas: &Canvas, x: usize, y: usize, label_len: usize) -> bool {
    (0..label_len).any(|i| canvas.get(x + i, y).is_some_and(|cell| cell.is_node))
}

/// Check if placing a label at the given position would collide with any edge cells.
fn label_collides_with_edge(canvas: &Canvas, x: usize, y: usize, label_len: usize) -> bool {
    (0..label_len).any(|i| canvas.get(x + i, y).is_some_and(|cell| cell.is_edge))
}

/// Calculate the length of a segment.
#[cfg(test)]
fn segment_length(segment: &Segment) -> usize {
    match segment {
        Segment::Vertical { y_start, y_end, .. } => {
            if y_start > y_end {
                y_start - y_end
            } else {
                y_end - y_start
            }
        }
        Segment::Horizontal { x_start, x_end, .. } => {
            if x_start > x_end {
                x_start - x_end
            } else {
                x_end - x_start
            }
        }
    }
}

/// Calculate the midpoint of a segment.
#[cfg(test)]
fn segment_midpoint(segment: &Segment) -> (usize, usize) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => {
            let mid_y = if y_start < y_end {
                y_start + (y_end - y_start) / 2
            } else {
                y_end + (y_start - y_end) / 2
            };
            (*x, mid_y)
        }
        Segment::Horizontal { y, x_start, x_end } => {
            let mid_x = if x_start < x_end {
                x_start + (x_end - x_start) / 2
            } else {
                x_end + (x_start - x_end) / 2
            };
            (mid_x, *y)
        }
    }
}

/// Draw a single segment on the canvas.
fn draw_segment(canvas: &mut Canvas, segment: &Segment, _stroke: Stroke, charset: &CharSet) {
    match segment {
        Segment::Vertical { x, y_start, y_end } => {
            let (y_min, y_max) = if y_start < y_end {
                (*y_start, *y_end)
            } else {
                (*y_end, *y_start)
            };

            for y in y_min..=y_max {
                // Note: For dotted strokes, we could use different characters,
                // but set_with_connection uses the charset's junction() method
                // which handles the character selection based on connections.

                let connections = Connections {
                    up: y > y_min,
                    down: y < y_max,
                    left: false,
                    right: false,
                };

                // Try to set with connection merging; if cell is protected, skip
                if !canvas.set_with_connection(*x, y, connections, charset) {
                    // Cell is protected (part of a node), skip
                }
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

                if !canvas.set_with_connection(x, *y, connections, charset) {
                    // Cell is protected
                }
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

            // Only use precomputed position if it's within canvas bounds
            let placed = if let Some(&(pre_x, pre_y)) = label_positions.get(&edge_key) {
                // Check if position is within canvas bounds
                if pre_x < canvas.width()
                    && pre_y < canvas.height()
                    && pre_x.saturating_add(label_len) <= canvas.width()
                {
                    // Use pre-computed position
                    draw_label_at_position(canvas, label, pre_x, pre_y)
                } else {
                    // Pre-computed position is out of bounds, fall back to heuristic
                    draw_edge_label_with_tracking(
                        canvas,
                        routed,
                        label,
                        diagram_direction,
                        &placed_labels,
                    )
                }
            } else {
                // Fall back to heuristic placement
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
    use super::super::layout::{LayoutConfig, compute_layout};
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
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed = route_edge(&diagram.edges[0], &layout, Direction::TopDown).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        let output = canvas.to_string();
        // Should contain vertical line character or arrow
        assert!(output.contains('│') || output.contains('▼'));
    }

    #[test]
    fn test_render_edge_with_arrow() {
        let diagram = simple_diagram();
        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed = route_edge(&diagram.edges[0], &layout, Direction::TopDown).unwrap();
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
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed = route_edge(&diagram.edges[0], &layout, Direction::TopDown).unwrap();
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
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed = route_edge(&diagram.edges[0], &layout, Direction::TopDown).unwrap();
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
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed_edges: Vec<_> = diagram
            .edges
            .iter()
            .filter_map(|e| route_edge(e, &layout, Direction::TopDown))
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

        let config = LayoutConfig::default();
        let layout = compute_layout(&diagram, &config);

        let mut canvas = Canvas::new(layout.width, layout.height);
        let charset = CharSet::unicode();

        let routed = route_edge(&diagram.edges[0], &layout, Direction::TopDown).unwrap();
        render_edge(&mut canvas, &routed, &charset, Direction::TopDown);

        let output = canvas.to_string();
        // Should contain the label
        assert!(output.contains("Yes"));
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
}
