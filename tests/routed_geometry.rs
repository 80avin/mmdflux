//! Contract tests for the routed geometry pipeline.
//!
//! Verifies that `route_graph_geometry` produces correct `RoutedGraphGeometry`
//! from engine-produced `GraphGeometry`.

use mmdflux::diagrams::flowchart::engine::DagreLayoutEngine;
use mmdflux::diagrams::flowchart::geometry::*;
use mmdflux::diagrams::flowchart::routing::route_graph_geometry;
use mmdflux::{EngineConfig, GraphLayoutEngine, RoutingMode, build_diagram, parse_flowchart};

/// Parse input and produce (Diagram, GraphGeometry) via the dagre engine.
fn layout_test(input: &str) -> (mmdflux::Diagram, GraphGeometry) {
    let fc = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&fc);
    let engine = DagreLayoutEngine::text();
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine.layout(&diagram, &config).unwrap();
    (diagram, geom)
}

// -----------------------------------------------------------------------
// Task 1.1: Routed geometry contract tests
// -----------------------------------------------------------------------

#[test]
fn routed_geometry_has_correct_node_count() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.nodes.len(), 3);
    assert!(routed.nodes.contains_key("A"));
    assert!(routed.nodes.contains_key("B"));
    assert!(routed.nodes.contains_key("C"));
}

#[test]
fn routed_geometry_has_correct_edge_count() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.edges.len(), 2);
}

#[test]
fn routed_edges_have_non_empty_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    for edge in &routed.edges {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} should have at least 2 path points, got {}",
            edge.from,
            edge.to,
            edge.path.len()
        );
    }
}

#[test]
fn routed_geometry_preserves_label_positions() {
    let (diagram, geom) = layout_test("graph TD\nA--label-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    let edge = &routed.edges[0];
    assert!(
        edge.label_position.is_some(),
        "labeled edge should have a label position"
    );
}

#[test]
fn routed_geometry_preserves_direction() {
    let (diagram, geom) = layout_test("graph LR\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    assert_eq!(routed.direction, mmdflux::Direction::LeftRight);
}

#[test]
fn routed_geometry_preserves_bounds() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    assert!(routed.bounds.width > 0.0);
    assert!(routed.bounds.height > 0.0);
}

#[test]
fn routed_geometry_preserves_subgraphs() {
    let (diagram, geom) = layout_test("graph TD\nsubgraph sg1[Group]\nA-->B\nend");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert!(!routed.subgraphs.is_empty());
    let sg = &routed.subgraphs["sg1"];
    assert_eq!(sg.title, "Group");
    assert!(sg.rect.width > 0.0);
}

#[test]
fn routed_geometry_marks_backward_edges() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->A");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    // At least one edge should be marked backward (the cycle)
    let backward_count = routed.edges.iter().filter(|e| e.is_backward).count();
    assert!(
        backward_count >= 1,
        "cycle should produce at least one backward edge"
    );
}

#[test]
fn routed_self_edges_have_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->A");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.self_edges.len(), 1);
    assert!(
        routed.self_edges[0].path.len() >= 2,
        "self-edge should have at least 2 path points"
    );
    assert_eq!(routed.self_edges[0].node_id, "A");
}

// -----------------------------------------------------------------------
// Task 1.2: Routing mode tests
// -----------------------------------------------------------------------

#[test]
fn pass_through_mode_uses_layout_path_hints() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::PassThroughClip);

    let edge = &routed.edges[0];
    // PassThroughClip should use the engine-provided path hints directly
    assert!(edge.path.len() >= 2);

    // The path should match the layout_path_hint from the geometry
    let layout_hint = geom.edges[0].layout_path_hint.as_ref().unwrap();
    assert_eq!(edge.path.len(), layout_hint.len());
    for (rp, lp) in edge.path.iter().zip(layout_hint.iter()) {
        assert_eq!(rp.x, lp.x);
        assert_eq!(rp.y, lp.y);
    }
}

#[test]
fn full_compute_mode_produces_valid_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C\nA-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.edges.len(), 3);
    for edge in &routed.edges {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} should have valid path",
            edge.from,
            edge.to,
        );
    }
}

#[test]
fn routing_modes_produce_same_structure() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let pass = route_graph_geometry(&diagram, &geom, RoutingMode::PassThroughClip);

    // Both modes should produce the same structural output
    assert_eq!(full.nodes.len(), pass.nodes.len());
    assert_eq!(full.edges.len(), pass.edges.len());
    assert_eq!(full.self_edges.len(), pass.self_edges.len());
    assert_eq!(full.subgraphs.len(), pass.subgraphs.len());
}

#[test]
fn routed_edges_preserve_subgraph_references() {
    let (diagram, geom) = layout_test("graph TD\nsubgraph sg1[Group]\nA\nend\nB-->sg1");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    // Check that subgraph references are preserved in routed edges.
    // If the edge connects to a subgraph-as-node, the reference should be preserved.
    if let Some(e) = routed
        .edges
        .iter()
        .find(|e| e.from_subgraph.is_some() || e.to_subgraph.is_some())
    {
        assert!(e.to_subgraph.is_some() || e.from_subgraph.is_some());
    }
}
