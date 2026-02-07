//! Graph-family routing stage.
//!
//! Produces `RoutedGraphGeometry` (Layer 2) from `GraphGeometry` (Layer 1).
//! Supports two modes:
//! - `FullCompute`: Build edge paths from layout hints + node positions.
//! - `PassThroughClip`: Use engine-provided paths directly.

use super::geometry::*;
use crate::diagram::RoutingMode;
use crate::graph::Diagram;

/// Route graph geometry to produce fully-routed edge paths.
///
/// Consumes engine-agnostic `GraphGeometry` and produces `RoutedGraphGeometry`
/// with polyline paths for every edge.
pub fn route_graph_geometry(
    _diagram: &Diagram,
    geometry: &GraphGeometry,
    routing_mode: RoutingMode,
) -> RoutedGraphGeometry {
    let edges: Vec<RoutedEdgeGeometry> = geometry
        .edges
        .iter()
        .map(|edge| {
            let path = match routing_mode {
                RoutingMode::PassThroughClip => {
                    // Engine provides complete paths; use them directly.
                    edge.layout_path_hint
                        .clone()
                        .unwrap_or_else(|| build_path_from_hints(edge, geometry))
                }
                RoutingMode::FullCompute => {
                    // Engine provides layout hints only; build paths.
                    build_path_from_hints(edge, geometry)
                }
            };
            let is_backward = geometry.reversed_edges.contains(&edge.index);
            RoutedEdgeGeometry {
                index: edge.index,
                from: edge.from.clone(),
                to: edge.to.clone(),
                path,
                label_position: edge.label_position,
                is_backward,
                from_subgraph: edge.from_subgraph.clone(),
                to_subgraph: edge.to_subgraph.clone(),
            }
        })
        .collect();

    let self_edges: Vec<RoutedSelfEdge> = geometry
        .self_edges
        .iter()
        .map(|se| RoutedSelfEdge {
            node_id: se.node_id.clone(),
            edge_index: se.edge_index,
            path: se.points.clone(),
        })
        .collect();

    RoutedGraphGeometry {
        nodes: geometry.nodes.clone(),
        edges,
        subgraphs: geometry.subgraphs.clone(),
        self_edges,
        direction: geometry.direction,
        bounds: geometry.bounds,
    }
}

/// Build an edge path from layout hints (waypoints, node positions, path hints).
///
/// Prefers `layout_path_hint` when available (e.g. dagre edge points).
/// Falls back to node centers connected through waypoints.
fn build_path_from_hints(edge: &LayoutEdge, geometry: &GraphGeometry) -> Vec<FPoint> {
    // Prefer the layout path hint if available.
    if let Some(ref path) = edge.layout_path_hint {
        return path.clone();
    }

    // Build path from source center → waypoints → target center.
    let mut path = Vec::new();

    if let Some(from_node) = geometry.nodes.get(&edge.from) {
        path.push(FPoint::new(
            from_node.rect.center_x(),
            from_node.rect.center_y(),
        ));
    }
    for wp in &edge.waypoints {
        path.push(*wp);
    }
    if let Some(to_node) = geometry.nodes.get(&edge.to) {
        path.push(FPoint::new(
            to_node.rect.center_x(),
            to_node.rect.center_y(),
        ));
    }

    path
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::diagram::RoutingMode;

    fn simple_geometry() -> (Diagram, GraphGeometry) {
        let mut diagram = Diagram::new(crate::graph::Direction::TopDown);
        diagram.add_node(crate::graph::Node::new("A"));
        diagram.add_node(crate::graph::Node::new("B"));
        diagram.add_edge(crate::graph::Edge::new("A", "B"));

        let mut nodes = HashMap::new();
        nodes.insert(
            "A".into(),
            PositionedNode {
                id: "A".into(),
                rect: FRect::new(50.0, 25.0, 40.0, 20.0),
                shape: crate::graph::Shape::Rectangle,
                label: "A".into(),
                parent: None,
            },
        );
        nodes.insert(
            "B".into(),
            PositionedNode {
                id: "B".into(),
                rect: FRect::new(50.0, 75.0, 40.0, 20.0),
                shape: crate::graph::Shape::Rectangle,
                label: "B".into(),
                parent: None,
            },
        );

        let edges = vec![LayoutEdge {
            index: 0,
            from: "A".into(),
            to: "B".into(),
            waypoints: vec![],
            label_position: None,
            from_subgraph: None,
            to_subgraph: None,
            layout_path_hint: Some(vec![FPoint::new(50.0, 35.0), FPoint::new(50.0, 65.0)]),
        }];

        let geom = GraphGeometry {
            nodes,
            edges,
            subgraphs: HashMap::new(),
            self_edges: vec![],
            direction: crate::graph::Direction::TopDown,
            node_directions: HashMap::new(),
            bounds: FRect::new(0.0, 0.0, 100.0, 100.0),
            reversed_edges: vec![],
            engine_hints: None,
        };

        (diagram, geom)
    }

    #[test]
    fn full_compute_produces_routed_edges() {
        let (diagram, geom) = simple_geometry();
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

        assert_eq!(routed.nodes.len(), 2);
        assert_eq!(routed.edges.len(), 1);
        assert!(routed.edges[0].path.len() >= 2);
        assert!(!routed.edges[0].is_backward);
    }

    #[test]
    fn pass_through_uses_layout_path_hints() {
        let (diagram, geom) = simple_geometry();
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::PassThroughClip);

        let edge = &routed.edges[0];
        assert_eq!(edge.path.len(), 2);
        assert_eq!(edge.path[0].x, 50.0);
        assert_eq!(edge.path[0].y, 35.0);
        assert_eq!(edge.path[1].x, 50.0);
        assert_eq!(edge.path[1].y, 65.0);
    }

    #[test]
    fn self_edges_are_routed() {
        let (diagram, mut geom) = simple_geometry();
        geom.self_edges.push(SelfEdgeGeometry {
            node_id: "A".into(),
            edge_index: 1,
            points: vec![
                FPoint::new(70.0, 15.0),
                FPoint::new(80.0, 15.0),
                FPoint::new(80.0, 35.0),
                FPoint::new(70.0, 35.0),
            ],
        });

        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        assert_eq!(routed.self_edges.len(), 1);
        assert_eq!(routed.self_edges[0].path.len(), 4);
        assert_eq!(routed.self_edges[0].node_id, "A");
    }

    #[test]
    fn backward_edges_are_marked() {
        let (diagram, mut geom) = simple_geometry();
        geom.reversed_edges = vec![0];

        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        assert!(routed.edges[0].is_backward);
    }

    #[test]
    fn fallback_path_from_node_centers_and_waypoints() {
        let (diagram, mut geom) = simple_geometry();
        // Remove layout_path_hint to force fallback
        geom.edges[0].layout_path_hint = None;
        geom.edges[0].waypoints = vec![FPoint::new(50.0, 50.0)];

        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        let path = &routed.edges[0].path;
        // Should be: A center → waypoint → B center
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].x, 50.0);
        assert_eq!(path[0].y, 25.0); // A center_y
        assert_eq!(path[1].x, 50.0);
        assert_eq!(path[1].y, 50.0); // waypoint
        assert_eq!(path[2].x, 50.0);
        assert_eq!(path[2].y, 75.0); // B center_y
    }

    #[test]
    fn label_positions_are_preserved() {
        let (diagram, mut geom) = simple_geometry();
        geom.edges[0].label_position = Some(FPoint::new(55.0, 50.0));

        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        let lp = routed.edges[0].label_position.unwrap();
        assert_eq!(lp.x, 55.0);
        assert_eq!(lp.y, 50.0);
    }

    #[test]
    fn nodes_and_subgraphs_are_preserved() {
        let (diagram, mut geom) = simple_geometry();
        geom.subgraphs.insert(
            "sg1".into(),
            SubgraphGeometry {
                id: "sg1".into(),
                rect: FRect::new(10.0, 5.0, 80.0, 90.0),
                title: "Group".into(),
                depth: 0,
            },
        );

        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        assert_eq!(routed.nodes.len(), 2);
        assert_eq!(routed.subgraphs.len(), 1);
        assert_eq!(routed.subgraphs["sg1"].title, "Group");
        assert_eq!(routed.direction, crate::graph::Direction::TopDown);
    }
}
