//! SVG edge routing for direction-override subgraphs.
//!
//! After sublayout reconciliation repositions nodes inside direction-override
//! subgraphs, dagre's pre-computed Bézier paths are stale. This module computes
//! fresh orthogonal edge paths in float coordinates for all edges touching
//! override subgraphs.

use std::collections::HashMap;

use crate::dagre::{LayoutResult, NodeId, Point, Rect};
use crate::graph::{Diagram, Direction};

/// Build a per-node direction map for SVG rendering.
///
/// Nodes inside a direction-override subgraph get the subgraph's direction;
/// all other nodes get the diagram's root direction.
///
/// Processes subgraphs in depth order (shallowest first) so the deepest
/// override deterministically wins for nested subgraphs.
pub fn build_node_directions_svg(diagram: &Diagram) -> HashMap<String, Direction> {
    let mut node_directions: HashMap<String, Direction> = HashMap::new();
    for node_id in diagram.nodes.keys() {
        node_directions.insert(node_id.clone(), diagram.direction);
    }

    let mut dir_sg_ids: Vec<&String> = diagram
        .subgraphs
        .iter()
        .filter(|(_, sg)| sg.dir.is_some())
        .map(|(id, _)| id)
        .collect();
    dir_sg_ids.sort_by(|a, b| {
        diagram
            .subgraph_depth(a)
            .cmp(&diagram.subgraph_depth(b))
            .then_with(|| a.cmp(b))
    });
    for sg_id in dir_sg_ids {
        let sg = &diagram.subgraphs[sg_id];
        let override_dir = sg.dir.unwrap();
        for node_id in &sg.nodes {
            if !diagram.is_subgraph(node_id) {
                node_directions.insert(node_id.clone(), override_dir);
            }
        }
    }

    node_directions
}

/// Determine the effective direction for an edge in SVG rendering.
///
/// If both endpoints share the same direction override, returns that direction.
/// Otherwise returns the fallback (diagram root direction).
pub fn effective_edge_direction_svg(
    node_directions: &HashMap<String, Direction>,
    from: &str,
    to: &str,
    fallback: Direction,
) -> Direction {
    let src_dir = node_directions.get(from).copied().unwrap_or(fallback);
    let tgt_dir = node_directions.get(to).copied().unwrap_or(fallback);
    if src_dir == tgt_dir {
        src_dir
    } else {
        fallback
    }
}

/// Compute the exit point from a rectangular node along a given direction.
fn exit_point(rect: &Rect, direction: Direction) -> Point {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    match direction {
        Direction::TopDown => Point {
            x: cx,
            y: rect.y + rect.height,
        },
        Direction::BottomTop => Point { x: cx, y: rect.y },
        Direction::LeftRight => Point {
            x: rect.x + rect.width,
            y: cy,
        },
        Direction::RightLeft => Point { x: rect.x, y: cy },
    }
}

/// Compute the entry point into a rectangular node along a given direction.
fn entry_point(rect: &Rect, direction: Direction) -> Point {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    match direction {
        Direction::TopDown => Point { x: cx, y: rect.y },
        Direction::BottomTop => Point {
            x: cx,
            y: rect.y + rect.height,
        },
        Direction::LeftRight => Point { x: rect.x, y: cy },
        Direction::RightLeft => Point {
            x: rect.x + rect.width,
            y: cy,
        },
    }
}

/// Route an orthogonal edge path between two nodes in float space.
///
/// Computes a straight or L-shaped path using the effective direction.
pub fn route_svg_edge_direct(
    from_rect: &Rect,
    to_rect: &Rect,
    direction: Direction,
) -> Vec<Point> {
    let start = exit_point(from_rect, direction);
    let end = entry_point(to_rect, direction);

    let is_vertical = matches!(direction, Direction::TopDown | Direction::BottomTop);
    let aligned = if is_vertical {
        (start.x - end.x).abs() < 0.5
    } else {
        (start.y - end.y).abs() < 0.5
    };

    if aligned {
        vec![start, end]
    } else {
        // L-shaped elbow: go along primary axis to midpoint, then turn
        let elbow = if is_vertical {
            let mid_y = (start.y + end.y) / 2.0;
            vec![
                start,
                Point {
                    x: start.x,
                    y: mid_y,
                },
                Point {
                    x: end.x,
                    y: mid_y,
                },
                end,
            ]
        } else {
            let mid_x = (start.x + end.x) / 2.0;
            vec![
                start,
                Point {
                    x: mid_x,
                    y: start.y,
                },
                Point {
                    x: mid_x,
                    y: end.y,
                },
                end,
            ]
        };
        elbow
    }
}

/// Find the intersection point where a line segment from `inside` to `outside`
/// crosses the boundary of a rectangle.
fn rect_boundary_crossing(inside: Point, outside: Point, rect: &Rect) -> Point {
    let dx = outside.x - inside.x;
    let dy = outside.y - inside.y;
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return inside;
    }

    let x_min = rect.x;
    let x_max = rect.x + rect.width;
    let y_min = rect.y;
    let y_max = rect.y + rect.height;

    let mut best_t = f64::INFINITY;

    // Check each edge of the rectangle
    if dx.abs() > f64::EPSILON {
        for &x_edge in &[x_min, x_max] {
            let t = (x_edge - inside.x) / dx;
            if t > 0.0 && t <= 1.0 {
                let y = inside.y + t * dy;
                if y >= y_min - 0.01 && y <= y_max + 0.01 && t < best_t {
                    best_t = t;
                }
            }
        }
    }
    if dy.abs() > f64::EPSILON {
        for &y_edge in &[y_min, y_max] {
            let t = (y_edge - inside.y) / dy;
            if t > 0.0 && t <= 1.0 {
                let x = inside.x + t * dx;
                if x >= x_min - 0.01 && x <= x_max + 0.01 && t < best_t {
                    best_t = t;
                }
            }
        }
    }

    if best_t.is_finite() {
        Point {
            x: inside.x + best_t * dx,
            y: inside.y + best_t * dy,
        }
    } else {
        inside
    }
}

/// Route an edge that crosses a subgraph boundary.
///
/// Computes a path from a node inside an override subgraph through the subgraph
/// boundary to a node outside (or vice versa).
///
/// The `inside_direction` is the override direction for the inside portion.
/// The `outside_direction` is the diagram direction for the outside portion.
pub fn route_svg_edge_with_boundary(
    from_rect: &Rect,
    to_rect: &Rect,
    sg_rect: &Rect,
    from_is_inside: bool,
    inside_direction: Direction,
    outside_direction: Direction,
) -> Vec<Point> {
    let (inside_rect, outside_rect) = if from_is_inside {
        (from_rect, to_rect)
    } else {
        (to_rect, from_rect)
    };

    let inside_center = inside_rect.center();
    let outside_center = outside_rect.center();

    // Compute the boundary crossing point
    let boundary = rect_boundary_crossing(inside_center, outside_center, sg_rect);

    // Route inside portion: from inside node to boundary using inside direction
    let inside_exit = exit_point(inside_rect, inside_direction);
    let outside_entry = entry_point(outside_rect, outside_direction);

    let mut points = Vec::new();

    if from_is_inside {
        // inside -> boundary -> outside
        points.push(inside_exit);
        // Add elbow if needed between exit and boundary
        let is_inside_vertical =
            matches!(inside_direction, Direction::TopDown | Direction::BottomTop);
        if is_inside_vertical {
            if (inside_exit.x - boundary.x).abs() > 0.5 {
                points.push(Point {
                    x: inside_exit.x,
                    y: boundary.y,
                });
            }
        } else if (inside_exit.y - boundary.y).abs() > 0.5 {
            points.push(Point {
                x: boundary.x,
                y: inside_exit.y,
            });
        }
        points.push(boundary);
        // Add elbow if needed between boundary and entry
        let is_outside_vertical =
            matches!(outside_direction, Direction::TopDown | Direction::BottomTop);
        if is_outside_vertical {
            if (boundary.x - outside_entry.x).abs() > 0.5 {
                points.push(Point {
                    x: outside_entry.x,
                    y: boundary.y,
                });
            }
        } else if (boundary.y - outside_entry.y).abs() > 0.5 {
            points.push(Point {
                x: boundary.x,
                y: outside_entry.y,
            });
        }
        points.push(outside_entry);
    } else {
        // outside -> boundary -> inside
        points.push(outside_entry);
        let is_outside_vertical =
            matches!(outside_direction, Direction::TopDown | Direction::BottomTop);
        if is_outside_vertical {
            if (outside_entry.x - boundary.x).abs() > 0.5 {
                points.push(Point {
                    x: outside_entry.x,
                    y: boundary.y,
                });
            }
        } else if (outside_entry.y - boundary.y).abs() > 0.5 {
            points.push(Point {
                x: boundary.x,
                y: outside_entry.y,
            });
        }
        points.push(boundary);
        let is_inside_vertical =
            matches!(inside_direction, Direction::TopDown | Direction::BottomTop);
        if is_inside_vertical {
            if (boundary.x - inside_exit.x).abs() > 0.5 {
                points.push(Point {
                    x: inside_exit.x,
                    y: boundary.y,
                });
            }
        } else if (boundary.y - inside_exit.y).abs() > 0.5 {
            points.push(Point {
                x: boundary.x,
                y: inside_exit.y,
            });
        }
        points.push(inside_exit);
    }

    // Collapse collinear points
    collapse_collinear(&mut points);
    points
}

/// Remove collinear points from a path.
fn collapse_collinear(points: &mut Vec<Point>) {
    if points.len() < 3 {
        return;
    }
    let mut i = 1;
    while i + 1 < points.len() {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];
        let collinear = ((prev.x - curr.x).abs() < 0.01 && (curr.x - next.x).abs() < 0.01)
            || ((prev.y - curr.y).abs() < 0.01 && (curr.y - next.y).abs() < 0.01);
        if collinear {
            points.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Statistics about rerouted edges for debugging.
#[derive(Debug, Default)]
pub struct RerouteStats {
    pub unaffected: usize,
    pub internal: usize,
    pub cross_boundary: usize,
}

/// Reroute all edges affected by direction-override subgraphs.
///
/// Modifies the `LayoutResult` in-place with fresh paths for edges touching
/// override subgraphs. Edges where both endpoints are outside override subgraphs
/// are left untouched.
pub fn reroute_override_edges(
    diagram: &Diagram,
    layout: &mut LayoutResult,
    node_directions: &HashMap<String, Direction>,
) -> RerouteStats {
    // Check if any subgraphs have direction overrides
    let has_overrides = diagram.subgraphs.values().any(|sg| sg.dir.is_some());
    if !has_overrides {
        return RerouteStats::default();
    }

    // Build override node map: node_id -> subgraph_id (deepest wins)
    let override_nodes = build_override_node_map_internal(diagram);

    let mut stats = RerouteStats::default();

    for edge_layout in &mut layout.edges {
        let Some(edge) = diagram.edges.get(edge_layout.index) else {
            continue;
        };

        // Skip subgraph-as-node edges
        if edge.from_subgraph.is_some() || edge.to_subgraph.is_some() {
            stats.unaffected += 1;
            continue;
        }

        let from_sg = override_nodes.get(&edge.from);
        let to_sg = override_nodes.get(&edge.to);

        match (from_sg, to_sg) {
            (None, None) => {
                // Neither endpoint in an override subgraph
                stats.unaffected += 1;
            }
            (Some(sg_a), Some(sg_b)) if sg_a == sg_b => {
                // Both in same override subgraph: route with override direction
                stats.internal += 1;
                let dir = effective_edge_direction_svg(
                    node_directions,
                    &edge.from,
                    &edge.to,
                    diagram.direction,
                );
                if let (Some(from_rect), Some(to_rect)) = (
                    layout.nodes.get(&NodeId(edge.from.clone())),
                    layout.nodes.get(&NodeId(edge.to.clone())),
                ) {
                    edge_layout.points = route_svg_edge_direct(from_rect, to_rect, dir);
                }
            }
            _ => {
                // Cross-boundary edge
                stats.cross_boundary += 1;
                let (inside_node, outside_node, from_is_inside) =
                    if from_sg.is_some() && (to_sg.is_none() || from_sg != to_sg) {
                        (&edge.from, &edge.to, true)
                    } else {
                        (&edge.to, &edge.from, false)
                    };

                let sg_id = override_nodes
                    .get(inside_node)
                    .expect("inside node must be in override");

                let inside_dir = node_directions
                    .get(inside_node)
                    .copied()
                    .unwrap_or(diagram.direction);
                let outside_dir = node_directions
                    .get(outside_node)
                    .copied()
                    .unwrap_or(diagram.direction);

                if let (Some(from_rect), Some(to_rect), Some(sg_rect)) = (
                    layout.nodes.get(&NodeId(edge.from.clone())),
                    layout.nodes.get(&NodeId(edge.to.clone())),
                    layout.subgraph_bounds.get(sg_id.as_str()),
                ) {
                    edge_layout.points = route_svg_edge_with_boundary(
                        from_rect,
                        to_rect,
                        sg_rect,
                        from_is_inside,
                        inside_dir,
                        outside_dir,
                    );
                }
            }
        }
    }

    stats
}

/// Build the override node map: node_id -> subgraph_id.
///
/// Processes subgraphs in depth order so the deepest override wins.
fn build_override_node_map_internal(diagram: &Diagram) -> HashMap<String, String> {
    let mut override_nodes = HashMap::new();
    let mut sg_ids: Vec<&String> = diagram
        .subgraphs
        .iter()
        .filter(|(_, sg)| sg.dir.is_some())
        .map(|(id, _)| id)
        .collect();
    sg_ids.sort_by(|a, b| {
        diagram
            .subgraph_depth(a)
            .cmp(&diagram.subgraph_depth(b))
            .then_with(|| a.cmp(b))
    });
    for sg_id in sg_ids {
        let sg = &diagram.subgraphs[sg_id];
        for node_id in &sg.nodes {
            if !diagram.is_subgraph(node_id) {
                override_nodes.insert(node_id.clone(), sg_id.clone());
            }
        }
    }
    override_nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_diagram;
    use crate::parser::parse_flowchart;

    #[test]
    fn test_build_node_directions_svg_basic() {
        let input = "graph TD\nsubgraph sg1\ndirection LR\nA --> B\nend\nC --> D\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let dirs = build_node_directions_svg(&diagram);

        assert_eq!(dirs.get("A").copied(), Some(Direction::LeftRight));
        assert_eq!(dirs.get("B").copied(), Some(Direction::LeftRight));
        assert_eq!(dirs.get("C").copied(), Some(Direction::TopDown));
        assert_eq!(dirs.get("D").copied(), Some(Direction::TopDown));
    }

    #[test]
    fn test_build_node_directions_svg_nested_deepest_wins() {
        let input = "graph TD\nsubgraph outer\ndirection LR\nsubgraph inner\ndirection BT\nA --> B\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let dirs = build_node_directions_svg(&diagram);

        // Deepest override wins
        assert_eq!(dirs.get("A").copied(), Some(Direction::BottomTop));
        assert_eq!(dirs.get("B").copied(), Some(Direction::BottomTop));
    }

    #[test]
    fn test_effective_edge_direction_svg_same_override() {
        let mut dirs = HashMap::new();
        dirs.insert("A".to_string(), Direction::LeftRight);
        dirs.insert("B".to_string(), Direction::LeftRight);
        dirs.insert("C".to_string(), Direction::TopDown);

        assert_eq!(
            effective_edge_direction_svg(&dirs, "A", "B", Direction::TopDown),
            Direction::LeftRight,
        );
        // Cross-boundary: falls back to root
        assert_eq!(
            effective_edge_direction_svg(&dirs, "A", "C", Direction::TopDown),
            Direction::TopDown,
        );
    }

    #[test]
    fn test_route_svg_edge_direct_aligned_td() {
        let from = Rect {
            x: 90.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let to = Rect {
            x: 90.0,
            y: 60.0,
            width: 20.0,
            height: 20.0,
        };
        let points = route_svg_edge_direct(&from, &to, Direction::TopDown);
        assert_eq!(points.len(), 2);
        assert!((points[0].x - 100.0).abs() < 0.01);
        assert!((points[0].y - 30.0).abs() < 0.01);
        assert!((points[1].x - 100.0).abs() < 0.01);
        assert!((points[1].y - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_route_svg_edge_direct_aligned_lr() {
        let from = Rect {
            x: 10.0,
            y: 90.0,
            width: 20.0,
            height: 20.0,
        };
        let to = Rect {
            x: 60.0,
            y: 90.0,
            width: 20.0,
            height: 20.0,
        };
        let points = route_svg_edge_direct(&from, &to, Direction::LeftRight);
        assert_eq!(points.len(), 2);
        assert!((points[0].x - 30.0).abs() < 0.01);
        assert!((points[0].y - 100.0).abs() < 0.01);
        assert!((points[1].x - 60.0).abs() < 0.01);
        assert!((points[1].y - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_route_svg_edge_direct_offset_needs_elbow() {
        let from = Rect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let to = Rect {
            x: 60.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let points = route_svg_edge_direct(&from, &to, Direction::TopDown);
        // Offset: needs elbow
        assert!(points.len() >= 3);
    }

    #[test]
    fn test_route_svg_edge_with_boundary_exit() {
        let from = Rect {
            x: 40.0,
            y: 40.0,
            width: 20.0,
            height: 20.0,
        };
        let to = Rect {
            x: 40.0,
            y: 150.0,
            width: 20.0,
            height: 20.0,
        };
        let sg = Rect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
        };
        let points = route_svg_edge_with_boundary(
            &from,
            &to,
            &sg,
            true,
            Direction::LeftRight,
            Direction::TopDown,
        );
        assert!(!points.is_empty());
        // No NaN
        for p in &points {
            assert!(p.x.is_finite() && p.y.is_finite(), "point has NaN: {:?}", p);
        }
    }
}
