//! Dagre parity tests.
//!
//! These tests compare mmdflux's dagre layout output against captured dagre.js output
//! to ensure layout parity for subgraph bounds, border node positions, and other metrics.

use std::fs;
use std::path::Path;

use mmdflux::dagre::{DiGraph, LayoutConfig, layout};
use serde::Deserialize;

// =============================================================================
// Test Data Types
// =============================================================================

/// Input graph format matching `mmdflux-dagre-input.json`.
#[derive(Debug, Deserialize)]
struct InputGraph {
    nodes: Vec<InputNode>,
    edges: Vec<InputEdge>,
}

#[derive(Debug, Deserialize)]
struct InputNode {
    id: String,
    width: f64,
    height: f64,
    parent: Option<String>,
    is_subgraph: bool,
}

#[derive(Debug, Deserialize)]
struct InputEdge {
    from: String,
    to: String,
}

/// Expected layout output format matching `dagre-layout.json`.
#[derive(Debug, Deserialize)]
struct DagreLayout {
    nodes: Vec<DagreNode>,
}

#[derive(Debug, Deserialize)]
struct DagreNode {
    id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    is_compound: bool,
}

// =============================================================================
// Helpers
// =============================================================================

fn load_json<T: for<'a> Deserialize<'a>>(path: &str) -> T {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let full_path = Path::new(manifest).join(path);
    let content =
        fs::read_to_string(&full_path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

fn build_digraph_from_input(input: &InputGraph) -> DiGraph<(f64, f64)> {
    let mut graph: DiGraph<(f64, f64)> = DiGraph::new();

    // Add all nodes first
    for node in &input.nodes {
        graph.add_node(node.id.as_str(), (node.width, node.height));
    }

    // Set parent relationships
    for node in &input.nodes {
        if let Some(parent) = &node.parent {
            graph.set_parent(node.id.as_str(), parent.as_str());
        }
    }

    // Add edges
    for edge in &input.edges {
        graph.add_edge(edge.from.as_str(), edge.to.as_str());
    }

    graph
}

// =============================================================================
// Parity Tests
// =============================================================================

mod subgraph_bounds {
    use super::*;

    const INPUT_PATH: &str = ".gumbo/research/0032-bk-compaction-divergence/full-debug-2026-02-02/fixtures/external_node_subgraph/mmdflux-dagre-input.json";
    const EXPECTED_PATH: &str = ".gumbo/research/0032-bk-compaction-divergence/full-debug-2026-02-02/fixtures/external_node_subgraph/dagre-layout.json";

    /// Task 2.1: Regression test for subgraph bounds parity.
    ///
    /// This test asserts that mmdflux's computed bounds for the Cloud subgraph
    /// match dagre.js's expected output. Currently fails due to border bottom
    /// ordering divergence (see findings/phase2-border-ordering-investigation.md).
    #[test]
    fn external_node_subgraph_bounds_match_dagre() {
        let input: InputGraph = load_json(INPUT_PATH);
        let expected: DagreLayout = load_json(EXPECTED_PATH);

        let graph = build_digraph_from_input(&input);

        // Use LayoutConfig matching the fixture's parameters
        let config = LayoutConfig {
            node_sep: 50.0,
            edge_sep: 20.0,
            rank_sep: 75.0,
            margin: 8.0,
            ..Default::default()
        };

        let result = layout(&graph, &config, |_, dims| *dims);

        // Find Cloud's expected bounds
        let expected_cloud = expected
            .nodes
            .iter()
            .find(|n| n.id == "Cloud" && n.is_compound)
            .expect("Should find Cloud compound node in expected output");

        // Get Cloud's actual bounds
        let actual_cloud = result
            .subgraph_bounds
            .get("Cloud")
            .expect("Should have bounds for Cloud subgraph");

        // Assert bounds match (within floating-point tolerance)
        let tolerance = 1.0; // Allow 1px tolerance for rounding

        assert!(
            (actual_cloud.width - expected_cloud.width).abs() < tolerance,
            "Cloud width mismatch: actual={}, expected={} (diff={})",
            actual_cloud.width,
            expected_cloud.width,
            actual_cloud.width - expected_cloud.width
        );
        assert!(
            (actual_cloud.height - expected_cloud.height).abs() < tolerance,
            "Cloud height mismatch: actual={}, expected={} (diff={})",
            actual_cloud.height,
            expected_cloud.height,
            actual_cloud.height - expected_cloud.height
        );
        assert!(
            (actual_cloud.x - expected_cloud.x).abs() < tolerance,
            "Cloud x mismatch: actual={}, expected={} (diff={})",
            actual_cloud.x,
            expected_cloud.x,
            actual_cloud.x - expected_cloud.x
        );
        assert!(
            (actual_cloud.y - expected_cloud.y).abs() < tolerance,
            "Cloud y mismatch: actual={}, expected={} (diff={})",
            actual_cloud.y,
            expected_cloud.y,
            actual_cloud.y - expected_cloud.y
        );
    }

    /// Task 2.1 (extended): Verify all compound node bounds WIDTH/HEIGHT match dagre.
    ///
    /// Note: x/y positions may differ due to sibling subgraph ordering divergence.
    /// us-east and us-west are currently swapped relative to dagre. This is a
    /// separate ordering bug from the bottom border ordering issue.
    #[test]
    fn external_node_subgraph_all_compound_dimensions_match_dagre() {
        let input: InputGraph = load_json(INPUT_PATH);
        let expected: DagreLayout = load_json(EXPECTED_PATH);

        let graph = build_digraph_from_input(&input);

        let config = LayoutConfig {
            node_sep: 50.0,
            edge_sep: 20.0,
            rank_sep: 75.0,
            margin: 8.0,
            ..Default::default()
        };

        let result = layout(&graph, &config, |_, dims| *dims);

        let tolerance = 1.0;

        // Check dimensions (width/height) of all compound nodes
        for expected_node in expected.nodes.iter().filter(|n| n.is_compound) {
            let actual = result
                .subgraph_bounds
                .get(&expected_node.id)
                .unwrap_or_else(|| panic!("Missing bounds for compound {}", expected_node.id));

            let width_diff = (actual.width - expected_node.width).abs();
            let height_diff = (actual.height - expected_node.height).abs();

            assert!(
                width_diff < tolerance,
                "{} width mismatch: actual={}, expected={} (diff={})",
                expected_node.id,
                actual.width,
                expected_node.width,
                width_diff
            );
            assert!(
                height_diff < tolerance,
                "{} height mismatch: actual={}, expected={} (diff={})",
                expected_node.id,
                actual.height,
                expected_node.height,
                height_diff
            );
        }
    }

    /// Note: Sibling subgraph ordering differs from dagre by design.
    ///
    /// In dagre.js, us-west is on the left and us-east is on the right. Mermaid's
    /// renderer then flips them. mmdflux handles this flip in the dagre layer
    /// instead, so us-east is on the left and us-west is on the right. This is
    /// intentional and NOT a bug.
    ///
    /// This test documents the expected difference in sibling ordering.
    #[test]
    fn external_node_subgraph_sibling_ordering_is_intentionally_different() {
        let input: InputGraph = load_json(INPUT_PATH);
        let expected: DagreLayout = load_json(EXPECTED_PATH);

        let graph = build_digraph_from_input(&input);

        let config = LayoutConfig {
            node_sep: 50.0,
            edge_sep: 20.0,
            rank_sep: 75.0,
            margin: 8.0,
            ..Default::default()
        };

        let result = layout(&graph, &config, |_, dims| *dims);

        // Get sibling subgraphs' x positions
        let us_east_actual = result
            .subgraph_bounds
            .get("us-east")
            .expect("us-east bounds");
        let us_west_actual = result
            .subgraph_bounds
            .get("us-west")
            .expect("us-west bounds");

        let us_east_expected = expected
            .nodes
            .iter()
            .find(|n| n.id == "us-east")
            .expect("us-east expected");
        let us_west_expected = expected
            .nodes
            .iter()
            .find(|n| n.id == "us-west")
            .expect("us-west expected");

        // dagre.js: us-west.x (28) < us-east.x (132) - us-west is on the left
        // mmdflux: us-east is on the left, us-west is on the right (INTENTIONALLY SWAPPED)
        // mmdflux handles the flip in dagre rather than in the renderer like mermaid does.
        let dagre_order_is_west_then_east = us_west_expected.x < us_east_expected.x;
        let mmdflux_order_is_west_then_east = us_west_actual.x < us_east_actual.x;

        // These SHOULD be different - mmdflux intentionally flips the sibling order
        assert_ne!(
            dagre_order_is_west_then_east, mmdflux_order_is_west_then_east,
            "Sibling ordering should be intentionally different (mmdflux handles flip in dagre)"
        );

        // Verify mmdflux has us-east on the left
        assert!(
            us_east_actual.x < us_west_actual.x,
            "mmdflux should have us-east on the left"
        );
    }
}

mod border_ordering {
    use super::*;

    const INPUT_PATH: &str = ".gumbo/research/0032-bk-compaction-divergence/full-debug-2026-02-02/fixtures/external_node_subgraph/mmdflux-dagre-input.json";
    const EXPECTED_PATH: &str = ".gumbo/research/0032-bk-compaction-divergence/full-debug-2026-02-02/fixtures/external_node_subgraph/dagre-layout.json";

    /// Task 2.2: Characterization test for border node x positions.
    ///
    /// This test verifies that border bottom nodes are positioned correctly
    /// relative to their compound's left/right borders. The bottom border
    /// should be between left and right, not outside.
    #[test]
    fn cloud_bottom_border_x_between_left_and_right() {
        let input: InputGraph = load_json(INPUT_PATH);
        let expected: DagreLayout = load_json(EXPECTED_PATH);

        let graph = build_digraph_from_input(&input);

        let config = LayoutConfig {
            node_sep: 50.0,
            edge_sep: 20.0,
            rank_sep: 75.0,
            margin: 8.0,
            ..Default::default()
        };

        let result = layout(&graph, &config, |_, dims| *dims);

        // Get Cloud's bounds from dagre
        let expected_cloud = expected
            .nodes
            .iter()
            .find(|n| n.id == "Cloud" && n.is_compound)
            .expect("Should find Cloud compound node");

        let actual_cloud = result
            .subgraph_bounds
            .get("Cloud")
            .expect("Should have bounds for Cloud");

        // The bottom border's x position affects the right edge of the bounds.
        // If bottom is ordered after right, the bounds will be wider.
        // Expected Cloud width is 228, but if ordering is wrong, we get 248 (dx=20).
        let tolerance = 1.0;
        let width_diff = (actual_cloud.width - expected_cloud.width).abs();

        assert!(
            width_diff < tolerance,
            "Cloud bounds width divergence suggests border ordering issue: \
             actual width={}, expected width={}, diff={}\n\
             This is caused by _bb_Cloud being ordered AFTER _br_Cloud instead of BETWEEN \
             _bl_Cloud and _br_Cloud at rank 8.",
            actual_cloud.width,
            expected_cloud.width,
            width_diff
        );
    }
}
