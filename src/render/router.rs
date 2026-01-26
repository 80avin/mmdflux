//! Edge routing between nodes.
//!
//! Computes paths for edges, avoiding node boundaries.

use super::layout::Layout;
use super::shape::NodeBounds;
use crate::graph::{Direction, Edge};

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

    let (out_dir, in_dir) = attachment_directions(diagram_direction);

    let start = attachment_point(from_bounds, out_dir);
    let end = attachment_point(to_bounds, in_dir);

    // Route based on the relative positions
    let segments = compute_path(start, end, diagram_direction);

    Some(RoutedEdge {
        edge: edge.clone(),
        start,
        end,
        segments,
        entry_direction: in_dir,
    })
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
