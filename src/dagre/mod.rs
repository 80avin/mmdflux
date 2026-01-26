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
    // Build internal layout graph
    let mut lg = LayoutGraph::from_digraph(graph, get_dimensions);

    // Phase 1: Make graph acyclic
    if config.acyclic {
        acyclic::run(&mut lg);
    }

    // Phase 2: Assign ranks (layers)
    rank::run(&mut lg);
    rank::normalize(&mut lg);

    // Phase 3: Reduce crossings
    order::run(&mut lg);

    // Phase 4: Assign coordinates
    position::run(&mut lg, config);

    // Build result
    let (width, height) = position::calculate_dimensions(&lg, config);
    let reversed_edges: Vec<usize> = lg.reversed_edges.iter().copied().collect();

    let nodes = lg
        .node_ids
        .iter()
        .enumerate()
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

    let edges = lg
        .edges
        .iter()
        .map(|&(from, to, orig_idx)| {
            let from_pos = lg.positions[from];
            let to_pos = lg.positions[to];
            let from_dim = lg.dimensions[from];
            let to_dim = lg.dimensions[to];

            // Simple direct path (center to center)
            let from_center = Point {
                x: from_pos.x + from_dim.0 / 2.0,
                y: from_pos.y + from_dim.1 / 2.0,
            };
            let to_center = Point {
                x: to_pos.x + to_dim.0 / 2.0,
                y: to_pos.y + to_dim.1 / 2.0,
            };

            EdgeLayout {
                from: lg.node_ids[from].clone(),
                to: lg.node_ids[to].clone(),
                points: vec![from_center, to_center],
                index: orig_idx,
            }
        })
        .collect();

    LayoutResult {
        nodes,
        edges,
        reversed_edges,
        width,
        height,
        edge_waypoints: HashMap::new(),
        label_positions: HashMap::new(),
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
}
