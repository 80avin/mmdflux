//! Edge rendering on the canvas.

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
        draw_edge_label(canvas, routed, label, diagram_direction);
    }
}

/// Draw a label on an edge at the midpoint of the edge path.
///
/// Places the label at the overall midpoint between start and end points.
/// For backward edges (where end is before start in layout direction),
/// offsets the label to avoid overlapping with forward edge labels.
/// If the label would collide with a node, tries alternative positions.
fn draw_edge_label(canvas: &mut Canvas, routed: &RoutedEdge, label: &str, direction: Direction) {
    let label_len = label.chars().count();

    // Detect if this is a backward edge (going against layout direction)
    let is_backward = match direction {
        Direction::LeftRight => routed.end.x < routed.start.x,
        Direction::RightLeft => routed.end.x > routed.start.x,
        Direction::TopDown => routed.end.y < routed.start.y,
        Direction::BottomTop => routed.end.y > routed.start.y,
    };

    // Calculate overall midpoint between start and end
    let mid_x = (routed.start.x + routed.end.x) / 2;
    let mid_y = (routed.start.y + routed.end.y) / 2;

    // For backward edges, offset the label to avoid collision with forward edges
    let (base_x, base_y) = if is_backward {
        match direction {
            Direction::LeftRight | Direction::RightLeft => {
                // Offset above the main line
                (mid_x.saturating_sub(label_len / 2), mid_y.saturating_sub(1))
            }
            Direction::TopDown | Direction::BottomTop => {
                // Offset to the left of the main line
                (mid_x.saturating_sub(label_len + 1), mid_y)
            }
        }
    } else {
        (mid_x.saturating_sub(label_len / 2), mid_y)
    };

    // Try to find a position that doesn't collide with nodes
    let (label_x, label_y) = find_safe_label_position(canvas, base_x, base_y, label_len, direction);

    // Write the label only to non-node cells
    for (i, ch) in label.chars().enumerate() {
        let x = label_x + i;
        // Only write if cell is not part of a node
        if canvas.get(x, label_y).is_some_and(|cell| !cell.is_node) {
            canvas.set(x, label_y, ch);
        }
    }
}

/// Find a safe position for an edge label that doesn't collide with nodes.
///
/// Tries the base position first, then shifts in the appropriate direction
/// based on the diagram layout until a collision-free position is found.
fn find_safe_label_position(
    canvas: &Canvas,
    base_x: usize,
    base_y: usize,
    label_len: usize,
    direction: Direction,
) -> (usize, usize) {
    // Check if the base position has any collision
    if !label_collides_with_node(canvas, base_x, base_y, label_len) {
        return (base_x, base_y);
    }

    // Try shifting positions based on diagram direction
    let shifts: Vec<(isize, isize)> = match direction {
        Direction::TopDown | Direction::BottomTop => {
            // For vertical layouts, try left/right shifts
            vec![
                (-1, 0),
                (1, 0),
                (-2, 0),
                (2, 0),
                (0, -1),
                (0, 1),
                (-3, 0),
                (3, 0),
            ]
        }
        Direction::LeftRight | Direction::RightLeft => {
            // For horizontal layouts, try up/down shifts
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

        if !label_collides_with_node(canvas, new_x, new_y, label_len) {
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

/// Render all edges onto the canvas.
///
/// Draws all segments and arrows first, then all labels, ensuring labels
/// are not overwritten by later edge segments.
pub fn render_all_edges(
    canvas: &mut Canvas,
    routed_edges: &[RoutedEdge],
    charset: &CharSet,
    diagram_direction: Direction,
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
    for routed in routed_edges {
        if let Some(label) = &routed.edge.label {
            draw_edge_label(canvas, routed, label, diagram_direction);
        }
    }
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
}
