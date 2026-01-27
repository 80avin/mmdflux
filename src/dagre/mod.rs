//! Dagre-style hierarchical graph layout.
//!
//! Implements the Sugiyama framework:
//! 1. Cycle removal (make graph acyclic)
//! 2. Layer assignment (rank nodes)
//! 3. Crossing reduction (order nodes within layers)
//! 4. Coordinate assignment (x, y positions)
//!
//! # Example
//!
//! ```
//! use mmdflux::dagre::{DiGraph, layout, LayoutConfig, Direction};
//!
//! // Create a graph with node dimensions
//! let mut graph = DiGraph::new();
//! graph.add_node("A", (100.0, 50.0)); // (width, height)
//! graph.add_node("B", (100.0, 50.0));
//! graph.add_edge("A", "B");
//!
//! // Configure and run layout
//! let config = LayoutConfig {
//!     direction: Direction::TopBottom,
//!     ..Default::default()
//! };
//!
//! let result = layout(&graph, &config, |_, dims| *dims);
//!
//! // Access positioned nodes
//! for (node_id, rect) in &result.nodes {
//!     println!("{}: ({}, {})", node_id, rect.x, rect.y);
//! }
//! ```

mod acyclic;
mod bk;
mod graph;
pub mod normalize;
mod order;
mod position;
mod rank;
pub mod types;

use std::collections::HashMap;

pub use graph::DiGraph;
use graph::LayoutGraph;
pub use types::{Direction, EdgeLayout, LayoutConfig, LayoutResult, NodeId, Point, Rect};

/// Main entry point for layout computation.
///
/// Takes a directed graph, configuration options, and a function to get node dimensions.
/// Returns a `LayoutResult` with positioned nodes and edge paths.
pub fn layout<N, F>(graph: &DiGraph<N>, config: &LayoutConfig, get_dimensions: F) -> LayoutResult
where
    F: Fn(&NodeId, &N) -> (f64, f64),
{
    layout_with_labels(graph, config, get_dimensions, &HashMap::new())
}

/// Layout computation with edge label support.
///
/// This variant allows specifying label dimensions for edges, which will be
/// used during normalization to create label dummies with appropriate sizes.
pub fn layout_with_labels<N, F>(
    graph: &DiGraph<N>,
    config: &LayoutConfig,
    get_dimensions: F,
    edge_labels: &HashMap<usize, normalize::EdgeLabelInfo>,
) -> LayoutResult
where
    F: Fn(&NodeId, &N) -> (f64, f64),
{
    // Build internal layout graph
    let mut lg = LayoutGraph::from_digraph(graph, get_dimensions);
    let original_node_count = lg.node_ids.len();

    // Phase 1: Make graph acyclic
    if config.acyclic {
        acyclic::run(&mut lg);
    }

    // Phase 2: Assign ranks (layers)
    rank::run(&mut lg);
    rank::normalize(&mut lg);

    // Capture original edge indices of reversed edges BEFORE normalization,
    // because normalization removes long edges (and their reversed_edges entries).
    // The rendering layer needs these to identify backward edges for corridor routing.
    let reversed_orig_edges: Vec<usize> = lg
        .reversed_edges
        .iter()
        .map(|&pos| lg.edges[pos].2)
        .collect();

    // Phase 2.5: Normalize long edges (insert dummy nodes)
    normalize::run(&mut lg, edge_labels);

    // Phase 3: Reduce crossings (now includes dummy nodes)
    order::run(&mut lg);

    // Phase 4: Assign coordinates
    position::run(&mut lg, config);

    // Extract waypoints from dummy positions
    let edge_waypoints = normalize::denormalize(&lg);

    // Extract label positions
    let mut label_positions = HashMap::new();
    for chain in &lg.dummy_chains {
        if let Some(pos) = normalize::get_label_position(&lg, chain.edge_index) {
            label_positions.insert(chain.edge_index, pos);
        }
    }

    // Build result
    let (width, height) = position::calculate_dimensions(&lg, config);
    let reversed_edges = reversed_orig_edges;

    // Only include real nodes (not dummies) in the output
    let nodes = lg
        .node_ids
        .iter()
        .enumerate()
        .take(original_node_count)
        .map(|(i, id)| {
            let pos = lg.positions[i];
            let (w, h) = lg.dimensions[i];
            (
                id.clone(),
                Rect {
                    x: pos.x,
                    y: pos.y,
                    width: w,
                    height: h,
                },
            )
        })
        .collect();

    // Build edge layouts, using waypoints for normalized edges
    let mut edges_by_orig_idx: HashMap<usize, EdgeLayout> = HashMap::new();

    for &(from, to, orig_idx) in &lg.edges {
        // Skip if this edge is already processed (part of a chain)
        if edges_by_orig_idx.contains_key(&orig_idx) {
            continue;
        }

        // Check if this edge has waypoints (was normalized)
        if let Some(waypoints) = edge_waypoints.get(&orig_idx) {
            // Find the original source and target nodes
            // For normalized edges, we need to find the chain endpoints
            // The source is the non-dummy node in the first segment
            // The target is the non-dummy node in the last segment
            let first_segment = lg
                .edges
                .iter()
                .find(|&&(f, _, idx)| idx == orig_idx && !lg.is_dummy_index(f));

            let last_segment = lg
                .edges
                .iter()
                .rev()
                .find(|&&(_, t, idx)| idx == orig_idx && !lg.is_dummy_index(t));

            if let (Some(&(src, _, _)), Some(&(_, tgt, _))) = (first_segment, last_segment) {
                let src_pos = lg.positions[src];
                let src_dim = lg.dimensions[src];
                let tgt_pos = lg.positions[tgt];
                let tgt_dim = lg.dimensions[tgt];

                let mut points = Vec::new();

                // Start point (center of source)
                points.push(Point {
                    x: src_pos.x + src_dim.0 / 2.0,
                    y: src_pos.y + src_dim.1 / 2.0,
                });

                // Add waypoints (extract just the point, not the rank info)
                points.extend(waypoints.iter().map(|wp| wp.point));

                // End point (center of target)
                points.push(Point {
                    x: tgt_pos.x + tgt_dim.0 / 2.0,
                    y: tgt_pos.y + tgt_dim.1 / 2.0,
                });

                edges_by_orig_idx.insert(
                    orig_idx,
                    EdgeLayout {
                        from: lg.node_ids[src].clone(),
                        to: lg.node_ids[tgt].clone(),
                        points,
                        index: orig_idx,
                    },
                );
            }
        } else {
            // Direct edge (not normalized)
            let from_pos = lg.positions[from];
            let to_pos = lg.positions[to];
            let from_dim = lg.dimensions[from];
            let to_dim = lg.dimensions[to];

            let from_center = Point {
                x: from_pos.x + from_dim.0 / 2.0,
                y: from_pos.y + from_dim.1 / 2.0,
            };
            let to_center = Point {
                x: to_pos.x + to_dim.0 / 2.0,
                y: to_pos.y + to_dim.1 / 2.0,
            };

            edges_by_orig_idx.insert(
                orig_idx,
                EdgeLayout {
                    from: lg.node_ids[from].clone(),
                    to: lg.node_ids[to].clone(),
                    points: vec![from_center, to_center],
                    index: orig_idx,
                },
            );
        }
    }

    // Sort edges by original index to maintain consistent ordering
    let mut edges: Vec<EdgeLayout> = edges_by_orig_idx.into_values().collect();
    edges.sort_by_key(|e| e.index);

    LayoutResult {
        nodes,
        edges,
        reversed_edges,
        width,
        height,
        edge_waypoints,
        label_positions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_layout() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);

        // A should be above B in TopBottom layout
        let a_rect = result.nodes.get(&"A".into()).unwrap();
        let b_rect = result.nodes.get(&"B".into()).unwrap();
        assert!(a_rect.y < b_rect.y);
    }

    #[test]
    fn test_layout_with_cycle() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");
        graph.add_edge("B", "A"); // Cycle

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        // Should still produce a valid layout
        assert_eq!(result.nodes.len(), 2);
        // One edge should be reversed
        assert_eq!(result.reversed_edges.len(), 1);
    }

    #[test]
    fn test_layout_directions() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        // Test LR direction
        let config = LayoutConfig {
            direction: Direction::LeftRight,
            ..Default::default()
        };
        let result = layout(&graph, &config, |_, dims| *dims);

        let a_rect = result.nodes.get(&"A".into()).unwrap();
        let b_rect = result.nodes.get(&"B".into()).unwrap();
        // A should be left of B in LeftRight layout
        assert!(a_rect.x < b_rect.x);
    }

    #[test]
    fn test_http_request_cycle() {
        // Simulates http_request.mmd graph
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("Client", (100.0, 50.0));
        graph.add_node("Server", (100.0, 50.0));
        graph.add_node("Auth", (100.0, 50.0));
        graph.add_node("Process", (100.0, 50.0));
        graph.add_node("Reject", (100.0, 50.0));
        graph.add_node("Response", (100.0, 50.0));

        // Edges in order from the mmd file
        graph.add_edge("Client", "Server");
        graph.add_edge("Server", "Auth");
        graph.add_edge("Auth", "Process");
        graph.add_edge("Auth", "Reject");
        graph.add_edge("Process", "Response");
        graph.add_edge("Reject", "Response");
        graph.add_edge("Response", "Client"); // Creates cycle

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        // Client should be at the top (smallest y)
        let client_y = result.nodes.get(&"Client".into()).unwrap().y;
        let server_y = result.nodes.get(&"Server".into()).unwrap().y;
        let auth_y = result.nodes.get(&"Auth".into()).unwrap().y;
        let process_y = result.nodes.get(&"Process".into()).unwrap().y;
        let response_y = result.nodes.get(&"Response".into()).unwrap().y;

        assert!(client_y < server_y, "Client should be above Server");
        assert!(server_y < auth_y, "Server should be above Auth");
        assert!(auth_y < process_y, "Auth should be above Process");
        assert!(process_y < response_y, "Process should be above Response");
    }

    #[test]
    fn test_layout_with_long_edge() {
        // A -> B -> C -> D, and A -> D (long edge spanning 3 ranks)
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_node("C", (100.0, 50.0));
        graph.add_node("D", (100.0, 50.0));
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");
        graph.add_edge("C", "D");
        graph.add_edge("A", "D"); // Long edge: spans 3 ranks

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        // Should have 4 nodes
        assert_eq!(result.nodes.len(), 4);

        // Should have 4 edges
        assert_eq!(result.edges.len(), 4);

        // The A->D edge should have waypoints
        let ad_edge = result
            .edges
            .iter()
            .find(|e| e.from.0 == "A" && e.to.0 == "D");
        assert!(ad_edge.is_some(), "Should have A->D edge");

        let ad_edge = ad_edge.unwrap();
        // A->D spans 3 ranks (A=0, D=3), needs 2 dummies
        // So the edge should have: start + 2 waypoints + end = 4 points
        assert_eq!(
            ad_edge.points.len(),
            4,
            "A->D edge should have 4 points (start + 2 waypoints + end)"
        );

        // Verify waypoints were extracted
        assert!(
            result.edge_waypoints.contains_key(&ad_edge.index),
            "Should have waypoints for long edge"
        );
    }

    #[test]
    fn test_layout_with_labels() {
        // A -> B -> C, and A -> C with label
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_node("C", (100.0, 50.0));
        graph.add_edge("A", "B"); // Edge 0
        graph.add_edge("B", "C"); // Edge 1
        graph.add_edge("A", "C"); // Edge 2 - long edge with label

        let mut edge_labels = HashMap::new();
        edge_labels.insert(2, normalize::EdgeLabelInfo::new(50.0, 20.0));

        let config = LayoutConfig::default();
        let result = layout_with_labels(&graph, &config, |_, dims| *dims, &edge_labels);

        // Should have label position for edge 2
        assert!(
            result.label_positions.contains_key(&2),
            "Should have label position for edge 2"
        );

        let label_pos = result.label_positions.get(&2).unwrap();
        // Label should be at an intermediate y position
        let a_y = result.nodes.get(&"A".into()).unwrap().y;
        let c_y = result.nodes.get(&"C".into()).unwrap().y;
        assert!(
            label_pos.y > a_y && label_pos.y < c_y,
            "Label should be between A and C"
        );
    }
}
