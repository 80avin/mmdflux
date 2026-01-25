//! Edge routing between nodes.
//!
//! Computes paths for edges, avoiding node boundaries.

use crate::graph::{Direction, Edge};

use super::layout::Layout;
use super::shape::NodeBounds;

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
    Vertical { x: usize, y_start: usize, y_end: usize },
    /// Horizontal line from start to end (same y, different x).
    Horizontal { y: usize, x_start: usize, x_end: usize },
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
#[derive(Debug, Clone, Copy)]
enum AttachDirection {
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

/// Route an edge between two nodes.
pub fn route_edge(
    edge: &Edge,
    layout: &Layout,
    diagram_direction: Direction,
) -> Option<RoutedEdge> {
    let from_bounds = layout.get_bounds(&edge.from)?;
    let to_bounds = layout.get_bounds(&edge.to)?;

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
    })
}

/// Compute the path segments between two points.
fn compute_path(start: Point, end: Point, direction: Direction) -> Vec<Segment> {
    // For TD/BT layouts, prefer vertical-first routing
    // For LR/RL layouts, prefer horizontal-first routing
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            compute_vertical_first_path(start, end)
        }
        Direction::LeftRight | Direction::RightLeft => {
            compute_horizontal_first_path(start, end)
        }
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
    use crate::graph::{Diagram, Node};

    use super::super::layout::{LayoutConfig, compute_layout};
    use super::*;

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
}
