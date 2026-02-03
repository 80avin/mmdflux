use super::super::layout::{LayoutConfig, compute_layout_direct};
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
    let layout = compute_layout_direct(&diagram, &config);

    let edge = &diagram.edges[0];
    let routed = route_edge(edge, &layout, Direction::TopDown, None, None, false).unwrap();

    // Should have at least one segment
    assert!(!routed.segments.is_empty());

    // For vertically aligned nodes, routing produces connector + main + connector segments.
    // All segments should be vertical and share the same x coordinate.
    if routed.start.x == routed.end.x {
        assert!(
            routed.segments.len() >= 2,
            "Expected at least 2 segments, got {}",
            routed.segments.len()
        );
        for seg in &routed.segments {
            match seg {
                Segment::Vertical { x, .. } => {
                    assert_eq!(
                        *x, routed.start.x as usize,
                        "Vertical segment should be colinear with start/end"
                    );
                }
                _ => panic!(
                    "Expected all vertical segments for colinear nodes, got {:?}",
                    seg
                ),
            }
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
    let layout = compute_layout_direct(&diagram, &config);

    // Route edge from A to C (which will be offset horizontally)
    let edge = &diagram.edges[1];
    let routed = route_edge(edge, &layout, Direction::TopDown, None, None, false).unwrap();

    // If nodes are not aligned, should have multiple segments
    if routed.start.x != routed.end.x {
        assert!(routed.segments.len() > 1);
    }
}

#[test]
fn test_route_all_edges() {
    let diagram = simple_td_diagram();
    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);

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
    make_bounds_sized(x, y, 10, 3)
}

fn make_bounds_sized(x: usize, y: usize, width: usize, height: usize) -> NodeBounds {
    NodeBounds {
        x,
        y,
        width,
        height,
        dagre_center_x: None,
        dagre_center_y: None,
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
#[ignore = "backward edge entry direction — will be fixed by BK parity work (plan 0040)"]
fn test_route_backward_edge_td() {
    // Create a diagram with a cycle: A -> B -> A
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(Node::new("A").with_label("Start"));
    diagram.add_node(Node::new("B").with_label("End"));
    diagram.add_edge(Edge::new("A", "B")); // Forward
    diagram.add_edge(Edge::new("B", "A")); // Backward

    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);

    // Route the backward edge
    let backward_edge = &diagram.edges[1];
    let routed = route_edge(
        backward_edge,
        &layout,
        Direction::TopDown,
        None,
        None,
        false,
    )
    .unwrap();

    // Backward edge uses synthetic waypoints routing around the right side.
    // The edge approaches the target from the right.
    assert_eq!(routed.entry_direction, AttachDirection::Right);

    // Should have segments connecting B to A
    assert!(!routed.segments.is_empty());
}

#[test]
#[ignore = "backward edge entry direction — will be fixed by BK parity work (plan 0040)"]
fn test_route_backward_edge_lr() {
    // Create a horizontal layout with a cycle
    let mut diagram = Diagram::new(Direction::LeftRight);
    diagram.add_node(Node::new("A").with_label("Start"));
    diagram.add_node(Node::new("B").with_label("End"));
    diagram.add_edge(Edge::new("A", "B")); // Forward
    diagram.add_edge(Edge::new("B", "A")); // Backward

    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);

    // Route the backward edge
    let backward_edge = &diagram.edges[1];
    let routed = route_edge(
        backward_edge,
        &layout,
        Direction::LeftRight,
        None,
        None,
        false,
    )
    .unwrap();

    // Backward edge uses synthetic waypoints routing below nodes.
    // The edge approaches the target from below.
    assert_eq!(routed.entry_direction, AttachDirection::Bottom);

    // Should have segments connecting B to A
    assert!(!routed.segments.is_empty());
}

#[test]
fn test_forward_edge_entry_direction_td() {
    // Forward edges should have standard entry direction
    let diagram = simple_td_diagram();
    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);

    let edge = &diagram.edges[0];
    let routed = route_edge(edge, &layout, Direction::TopDown, None, None, false).unwrap();

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
    let layout = compute_layout_direct(&diagram, &config);

    let edge = &diagram.edges[0];
    let routed = route_edge(edge, &layout, Direction::LeftRight, None, None, false).unwrap();

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
    let layout = compute_layout_direct(&diagram, &config);

    // Route both backward edges — they should both produce valid paths
    let edge_c_to_a = &diagram.edges[2];
    let edge_c_to_b = &diagram.edges[3];
    let routed_c_a = route_edge(edge_c_to_a, &layout, Direction::TopDown, None, None, false);
    let routed_c_b = route_edge(edge_c_to_b, &layout, Direction::TopDown, None, None, false);

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
    let layout = compute_layout_direct(&diagram, &config);

    let backward_edge = &diagram.edges[2];
    let routed = route_edge(
        backward_edge,
        &layout,
        Direction::TopDown,
        None,
        None,
        false,
    )
    .unwrap();

    assert!(
        routed.segments.len() >= 2,
        "Backward edge should have routing segments, got {}",
        routed.segments.len()
    );
}

#[test]
#[ignore = "synthetic waypoint routing — will be fixed by BK parity work (plan 0040)"]
fn test_short_backward_edge_uses_synthetic_waypoints() {
    // B→A backward edge spanning 1 rank — no dummies, no dagre waypoints
    // With synthetic waypoints, should route around the right side of nodes
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(Node::new("A").with_label("Top"));
    diagram.add_node(Node::new("B").with_label("Bottom"));
    diagram.add_edge(Edge::new("A", "B"));
    diagram.add_edge(Edge::new("B", "A")); // Backward, 1 rank

    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);

    let backward_edge = &diagram.edges[1];
    let routed = route_edge(
        backward_edge,
        &layout,
        Direction::TopDown,
        None,
        None,
        false,
    );
    assert!(routed.is_some(), "Backward edge should route successfully");

    let routed = routed.unwrap();
    // With synthetic waypoints routing around the right side, there should be
    // more than 2 segments (direct routing gives ~2, waypoint routing gives >= 4)
    assert!(
        routed.segments.len() >= 4,
        "Backward edge with synthetic waypoints should have >= 4 segments, got {}",
        routed.segments.len()
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
    let layout = compute_layout_direct(&diagram, &config);

    let backward_edge = &diagram.edges[2];
    let routed = route_edge(
        backward_edge,
        &layout,
        Direction::LeftRight,
        None,
        None,
        false,
    );
    assert!(
        routed.is_some(),
        "LR backward edge should route successfully"
    );
}

#[test]
fn test_backward_edge_expands_canvas_for_routing() {
    // Backward edges add canvas margin for synthetic waypoint routing
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
    let layout_cycle = compute_layout_direct(&diagram_with_cycle, &config);
    let layout_no_cycle = compute_layout_direct(&diagram_no_cycle, &config);

    assert!(
        layout_cycle.width > layout_no_cycle.width,
        "Backward edge should expand canvas width for routing margin. With cycle: {}, without: {}",
        layout_cycle.width,
        layout_no_cycle.width
    );
}

// --- LR zero-gap entry direction test ---

#[test]
fn test_lr_zero_gap_entry_direction() {
    // When compute_layout_direct places nodes with minimal gap, the
    // offset start and end points can coincide. The entry direction
    // should still match the layout direction (Left for LR).
    let mut diagram = Diagram::new(Direction::LeftRight);
    diagram.add_node(Node::new("Input").with_label("User Input"));
    diagram.add_node(Node::new("Process").with_label("Process Data"));
    diagram.add_node(Node::new("Output").with_label("Display Result"));
    diagram.add_edge(Edge::new("Input", "Process"));
    diagram.add_edge(Edge::new("Process", "Output"));

    let config = LayoutConfig::default();
    let layout = compute_layout_direct(&diagram, &config);
    let routed_edges = route_all_edges(&diagram.edges, &layout, Direction::LeftRight);

    for routed in &routed_edges {
        assert_eq!(
            routed.entry_direction,
            AttachDirection::Left,
            "LR edge {}->{} should enter from Left, not {:?}",
            routed.edge.from,
            routed.edge.to,
            routed.entry_direction,
        );
    }
}

// --- Consensus-Y tests for LR/RL attachment points (Task 1.1) ---

#[test]
fn test_lr_attachment_consensus_y_same_height() {
    let from = make_bounds_sized(0, 2, 10, 3);
    let to = make_bounds_sized(20, 4, 10, 3);
    let ep = EdgeEndpoints {
        from_bounds: &from,
        from_shape: Shape::Rectangle,
        to_bounds: &to,
        to_shape: Shape::Rectangle,
    };
    let (src, tgt) = resolve_attachment_points(None, None, &ep, &[], Direction::LeftRight);
    assert_eq!(
        src.1, tgt.1,
        "LR attachment points should have consensus y, got src.y={} tgt.y={}",
        src.1, tgt.1
    );
}

#[test]
fn test_lr_attachment_consensus_y_different_height() {
    let from = make_bounds_sized(0, 2, 10, 3);
    let to = make_bounds_sized(20, 3, 10, 5);
    let ep = EdgeEndpoints {
        from_bounds: &from,
        from_shape: Shape::Rectangle,
        to_bounds: &to,
        to_shape: Shape::Rectangle,
    };
    let (src, tgt) = resolve_attachment_points(None, None, &ep, &[], Direction::LeftRight);
    assert_eq!(
        src.1, tgt.1,
        "LR attachment points should have consensus y even with different heights, got src.y={} tgt.y={}",
        src.1, tgt.1
    );
}

#[test]
fn test_rl_attachment_consensus_y() {
    let from = make_bounds_sized(20, 2, 10, 3);
    let to = make_bounds_sized(0, 4, 10, 3);
    let ep = EdgeEndpoints {
        from_bounds: &from,
        from_shape: Shape::Rectangle,
        to_bounds: &to,
        to_shape: Shape::Rectangle,
    };
    let (src, tgt) = resolve_attachment_points(None, None, &ep, &[], Direction::RightLeft);
    assert_eq!(
        src.1, tgt.1,
        "RL attachment points should have consensus y, got src.y={} tgt.y={}",
        src.1, tgt.1
    );
}

// --- generate_backward_waypoints tests ---

#[test]
fn test_generate_backward_waypoints_td() {
    // TD layout: source (B) at y=6, target (A) at y=0 — backward
    let src = make_bounds_sized(4, 6, 8, 3);
    let tgt = make_bounds_sized(4, 0, 8, 3);

    let waypoints = generate_backward_waypoints(&src, &tgt, Direction::TopDown);

    assert!(!waypoints.is_empty(), "should produce waypoints");
    // Waypoints should be to the right of both nodes
    let max_right = (src.x + src.width).max(tgt.x + tgt.width);
    for wp in &waypoints {
        assert!(
            wp.0 > max_right,
            "waypoint x={} should be right of nodes (max_right={})",
            wp.0,
            max_right
        );
    }
}

#[test]
fn test_generate_backward_waypoints_lr() {
    // LR layout: source (B) at x=12, target (A) at x=0 — backward
    let src = make_bounds_sized(12, 2, 8, 3);
    let tgt = make_bounds_sized(0, 2, 8, 3);

    let waypoints = generate_backward_waypoints(&src, &tgt, Direction::LeftRight);

    assert!(!waypoints.is_empty(), "should produce waypoints");
    // Waypoints should be below both nodes
    let max_bottom = (src.y + src.height).max(tgt.y + tgt.height);
    for wp in &waypoints {
        assert!(
            wp.1 > max_bottom,
            "waypoint y={} should be below nodes (max_bottom={})",
            wp.1,
            max_bottom
        );
    }
}

#[test]
fn test_generate_backward_waypoints_forward_returns_empty() {
    // Forward edge in TD: src above target — not backward
    let src = make_bounds_sized(4, 0, 8, 3);
    let tgt = make_bounds_sized(4, 6, 8, 3);

    let waypoints = generate_backward_waypoints(&src, &tgt, Direction::TopDown);
    assert!(
        waypoints.is_empty(),
        "forward edge should return empty waypoints"
    );
}

#[test]
fn test_generate_backward_waypoints_bt() {
    // BT layout: source at y=0 (visually bottom), target at y=6 (visually top) — backward
    let src = make_bounds_sized(4, 0, 8, 3);
    let tgt = make_bounds_sized(4, 6, 8, 3);

    let waypoints = generate_backward_waypoints(&src, &tgt, Direction::BottomTop);

    assert!(
        !waypoints.is_empty(),
        "should produce waypoints for BT backward"
    );
    let max_right = (src.x + src.width).max(tgt.x + tgt.width);
    for wp in &waypoints {
        assert!(wp.0 > max_right, "BT waypoint should be right of nodes");
    }
}

#[test]
fn test_generate_backward_waypoints_rl() {
    // RL layout: source at x=0, target at x=12 — backward
    let src = make_bounds_sized(0, 2, 8, 3);
    let tgt = make_bounds_sized(12, 2, 8, 3);

    let waypoints = generate_backward_waypoints(&src, &tgt, Direction::RightLeft);

    assert!(
        !waypoints.is_empty(),
        "should produce waypoints for RL backward"
    );
    let max_bottom = (src.y + src.height).max(tgt.y + tgt.height);
    for wp in &waypoints {
        assert!(wp.1 > max_bottom, "RL waypoint should be below nodes");
    }
}

// === Segment helper tests (Task 1.1) ===

#[test]
fn vertical_segment_length() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.length(), 10);
}

#[test]
fn vertical_segment_length_reversed() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 20,
        y_end: 10,
    };
    assert_eq!(seg.length(), 10);
}

#[test]
fn horizontal_segment_length() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 5,
        x_end: 15,
    };
    assert_eq!(seg.length(), 10);
}

#[test]
fn horizontal_segment_length_reversed() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 15,
        x_end: 5,
    };
    assert_eq!(seg.length(), 10);
}

#[test]
fn zero_length_segment() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 10,
    };
    assert_eq!(seg.length(), 0);
}

#[test]
fn start_point_vertical() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.start_point(), Point { x: 5, y: 10 });
}

#[test]
fn end_point_vertical() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.end_point(), Point { x: 5, y: 20 });
}

#[test]
fn start_point_horizontal() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 5,
        x_end: 15,
    };
    assert_eq!(seg.start_point(), Point { x: 5, y: 3 });
}

#[test]
fn end_point_horizontal() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 5,
        x_end: 15,
    };
    assert_eq!(seg.end_point(), Point { x: 15, y: 3 });
}

#[test]
fn point_at_offset_zero_is_start() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.point_at_offset(0), seg.start_point());
}

#[test]
fn point_at_offset_length_is_end() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.point_at_offset(seg.length()), seg.end_point());
}

#[test]
fn point_at_offset_midpoint_vertical() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.point_at_offset(5), Point { x: 5, y: 15 });
}

#[test]
fn point_at_offset_midpoint_horizontal() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 0,
        x_end: 10,
    };
    assert_eq!(seg.point_at_offset(5), Point { x: 5, y: 3 });
}

#[test]
fn point_at_offset_reversed_vertical() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 20,
        y_end: 10,
    };
    assert_eq!(seg.point_at_offset(5), Point { x: 5, y: 15 });
}

#[test]
fn point_at_offset_reversed_horizontal() {
    let seg = Segment::Horizontal {
        y: 3,
        x_start: 15,
        x_end: 5,
    };
    assert_eq!(seg.point_at_offset(5), Point { x: 10, y: 3 });
}

#[test]
fn point_at_offset_clamped_beyond_length() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 20,
    };
    assert_eq!(seg.point_at_offset(100), seg.end_point());
}

#[test]
fn point_at_offset_zero_length_segment() {
    let seg = Segment::Vertical {
        x: 5,
        y_start: 10,
        y_end: 10,
    };
    assert_eq!(seg.point_at_offset(0), Point { x: 5, y: 10 });
}
