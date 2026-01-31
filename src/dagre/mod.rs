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
pub(crate) mod border;
mod graph;
pub(crate) mod nesting;
pub(crate) mod network_simplex;
pub mod normalize;
mod order;
mod position;
mod rank;
pub mod types;

use std::collections::HashMap;

pub use graph::DiGraph;
use graph::LayoutGraph;
pub use types::{
    Direction, EdgeLayout, LayoutConfig, LayoutResult, NodeId, Point, Ranker, Rect, SelfEdge,
    SelfEdgeLayout,
};

/// Double all edge minlens when any edge has a label, creating a uniform rank grid.
///
/// This matches dagre.js's `makeSpaceForEdgeLabels()` which globally doubles minlens
/// rather than selectively inflating labeled edges. The uniform grid ensures downstream
/// Sugiyama phases (normalization, ordering, positioning) see consistent rank spacing.
fn make_space_for_edge_labels(
    lg: &mut LayoutGraph,
    edge_labels: &HashMap<usize, normalize::EdgeLabelInfo>,
) {
    if edge_labels.is_empty() {
        return;
    }
    for minlen in &mut lg.edge_minlens {
        *minlen *= 2;
    }
}

/// Remove self-edges (from == to) from the graph before the Sugiyama pipeline.
///
/// Self-edges confuse cycle detection and ranking. They are stashed on
/// `lg.self_edges` and removed from `lg.edges` (and parallel arrays).
fn extract_self_edges(lg: &mut LayoutGraph) {
    debug_assert!(
        lg.reversed_edges.is_empty(),
        "extract_self_edges must run before acyclic::run()"
    );

    let mut to_remove = Vec::new();
    for (pos, &(from, to, orig_idx)) in lg.edges.iter().enumerate() {
        if from == to {
            lg.self_edges.push(SelfEdge {
                node_index: from,
                orig_edge_index: orig_idx,
                dummy_index: None,
            });
            to_remove.push(pos);
        }
    }

    // Remove in reverse order to preserve indices
    for &pos in to_remove.iter().rev() {
        lg.edges.remove(pos);
        lg.edge_weights.remove(pos);
        lg.edge_minlens.remove(pos);
    }
}

/// Insert dummy nodes for self-edges after ordering, before positioning.
///
/// Each self-edge gets a small dummy at the same rank, ordered right after
/// the self-edge's node. The BK algorithm will position it adjacent to the
/// node, establishing the loop extent.
fn insert_self_edge_dummies(lg: &mut LayoutGraph) {
    for (i, se) in lg.self_edges.clone().iter().enumerate() {
        let node_rank = lg.ranks[se.node_index];
        let node_order = lg.order[se.node_index];
        let dummy_id: NodeId = format!("_self_edge_dummy_{}", i).into();
        let dummy_idx = lg.node_ids.len();

        // Add dummy node to all parallel arrays
        lg.node_ids.push(dummy_id.clone());
        lg.node_index.insert(dummy_id.clone(), dummy_idx);
        lg.ranks.push(node_rank);
        lg.positions.push(Point::default());
        lg.dimensions.push((1.0, 1.0));
        lg.parents.push(lg.parents[se.node_index]);

        // Insert into ordering: place dummy right after the node
        // Shift all nodes at this rank with order > node_order
        for idx in 0..lg.order.len() - 1 {
            // -1 because we haven't pushed yet
            if lg.ranks[idx] == node_rank && lg.order[idx] > node_order {
                lg.order[idx] += 1;
            }
        }
        lg.order.push(node_order + 1);

        // Register as dummy
        lg.dummy_nodes.insert(
            dummy_id,
            normalize::DummyNode::edge(se.orig_edge_index, node_rank),
        );

        lg.self_edges[i].dummy_index = Some(dummy_idx);
    }
}

/// Compute 6-point orthogonal loop paths for self-edges using positioned node/dummy coordinates.
fn position_self_edges(lg: &LayoutGraph, config: &LayoutConfig) -> Vec<SelfEdgeLayout> {
    let gap = 1.0;

    lg.self_edges
        .iter()
        .filter_map(|se| {
            let dummy_idx = se.dummy_index?;
            let node_pos = lg.positions[se.node_index];
            let (nw, nh) = lg.dimensions[se.node_index];
            let dummy_pos = lg.positions[dummy_idx];

            let node_id = lg.node_ids[se.node_index].clone();
            let node_cx = node_pos.x + nw / 2.0;
            let node_cy = node_pos.y + nh / 2.0;

            let points = match config.direction {
                Direction::TopBottom => {
                    let loop_x = dummy_pos.x + 0.5;
                    let bot = node_pos.y + nh;
                    let top = node_pos.y;
                    vec![
                        Point { x: node_cx, y: bot },
                        Point {
                            x: node_cx,
                            y: bot + gap,
                        },
                        Point {
                            x: loop_x,
                            y: bot + gap,
                        },
                        Point {
                            x: loop_x,
                            y: top - gap,
                        },
                        Point {
                            x: node_cx,
                            y: top - gap,
                        },
                        Point { x: node_cx, y: top },
                    ]
                }
                Direction::BottomTop => {
                    let loop_x = dummy_pos.x + 0.5;
                    let top = node_pos.y;
                    let bot = node_pos.y + nh;
                    vec![
                        Point { x: node_cx, y: top },
                        Point {
                            x: node_cx,
                            y: top - gap,
                        },
                        Point {
                            x: loop_x,
                            y: top - gap,
                        },
                        Point {
                            x: loop_x,
                            y: bot + gap,
                        },
                        Point {
                            x: node_cx,
                            y: bot + gap,
                        },
                        Point { x: node_cx, y: bot },
                    ]
                }
                Direction::LeftRight => {
                    let loop_y = dummy_pos.y + 0.5;
                    let right = node_pos.x + nw;
                    let left = node_pos.x;
                    vec![
                        Point {
                            x: right,
                            y: node_cy,
                        },
                        Point {
                            x: right + gap,
                            y: node_cy,
                        },
                        Point {
                            x: right + gap,
                            y: loop_y,
                        },
                        Point {
                            x: left - gap,
                            y: loop_y,
                        },
                        Point {
                            x: left - gap,
                            y: node_cy,
                        },
                        Point {
                            x: left,
                            y: node_cy,
                        },
                    ]
                }
                Direction::RightLeft => {
                    let loop_y = dummy_pos.y + 0.5;
                    let left = node_pos.x;
                    let right = node_pos.x + nw;
                    vec![
                        Point {
                            x: left,
                            y: node_cy,
                        },
                        Point {
                            x: left - gap,
                            y: node_cy,
                        },
                        Point {
                            x: left - gap,
                            y: loop_y,
                        },
                        Point {
                            x: right + gap,
                            y: loop_y,
                        },
                        Point {
                            x: right + gap,
                            y: node_cy,
                        },
                        Point {
                            x: right,
                            y: node_cy,
                        },
                    ]
                }
            };

            Some(SelfEdgeLayout {
                node: node_id,
                edge_index: se.orig_edge_index,
                points,
            })
        })
        .collect()
}

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
    let has_compound = !lg.compound_nodes.is_empty();

    // Phase 0: Remove self-edges before acyclic detection
    extract_self_edges(&mut lg);

    // Phase 1: Make graph acyclic
    if config.acyclic {
        acyclic::run(&mut lg);
    }

    // Compound: add nesting structure (border top/bottom, nesting edges)
    if has_compound {
        nesting::run(&mut lg);
    }

    // Phase 1.5: Set minlen=2 for labeled edges so ranking creates a gap
    make_space_for_edge_labels(&mut lg, edge_labels);

    // Phase 2: Assign ranks (layers)
    rank::run(&mut lg, config);
    rank::normalize(&mut lg);

    // Compound: cleanup nesting edges, insert title nodes, compute rank spans
    if has_compound {
        nesting::cleanup(&mut lg);
        nesting::insert_title_nodes(&mut lg);
        nesting::assign_rank_minmax(&mut lg);
    }

    // Capture original edge indices of reversed edges BEFORE normalization,
    // because normalization removes long edges (and their reversed_edges entries).
    // The rendering layer needs these to identify backward edges for waypoint routing.
    let reversed_orig_edges: Vec<usize> = lg
        .reversed_edges
        .iter()
        .map(|&pos| lg.edges[pos].2)
        .collect();

    // Phase 2.5: Normalize long edges (insert dummy nodes)
    normalize::run(&mut lg, edge_labels);

    // Compound: add border segments (left/right border nodes per rank)
    if has_compound {
        border::add_segments(&mut lg);
    }

    // Phase 3: Reduce crossings (now includes dummy nodes and border segments)
    order::run(&mut lg);

    // Phase 3.5: Insert self-edge dummies (after ordering, before positioning)
    insert_self_edge_dummies(&mut lg);

    // Phase 4: Assign coordinates
    position::run(&mut lg, config);

    // Phase 4.5: Compute self-edge loop paths
    let self_edge_layouts = position_self_edges(&lg, config);

    // Compound: extract subgraph bounding boxes from border node positions
    let subgraph_bounds = if has_compound {
        border::remove_nodes(&mut lg)
    } else {
        HashMap::new()
    };

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
        subgraph_bounds,
        self_edges: self_edge_layouts,
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
    fn test_make_space_doubles_all_minlens() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (5.0, 3.0));
        graph.add_node("B", (5.0, 3.0));
        graph.add_node("C", (5.0, 3.0));
        graph.add_edge("A", "B"); // edge 0: labeled
        graph.add_edge("B", "C"); // edge 1: unlabeled

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);

        let mut edge_labels = HashMap::new();
        edge_labels.insert(0, normalize::EdgeLabelInfo::new(5.0, 1.0));

        make_space_for_edge_labels(&mut lg, &edge_labels);

        // ALL edges should be doubled, not just the labeled one
        assert_eq!(lg.edge_minlens[0], 2); // labeled edge: 1 * 2 = 2
        assert_eq!(lg.edge_minlens[1], 2); // unlabeled edge: 1 * 2 = 2
    }

    #[test]
    fn test_make_space_noop_when_no_labels() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (5.0, 3.0));
        graph.add_node("B", (5.0, 3.0));
        graph.add_edge("A", "B");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        let edge_labels = HashMap::new(); // empty

        make_space_for_edge_labels(&mut lg, &edge_labels);

        assert_eq!(lg.edge_minlens[0], 1); // unchanged
    }

    #[test]
    fn test_bk_allocates_space_for_label_dummy() {
        // Verify that label dummies with non-zero width influence layout spacing
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 3.0));
        graph.add_node("B", (10.0, 3.0));
        graph.add_node("C", (10.0, 3.0));
        // A -> B and A -> C: two parallel edges on same ranks
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");

        // Label on A->B with significant width
        let mut edge_labels = HashMap::new();
        edge_labels.insert(0, normalize::EdgeLabelInfo::new(50.0, 5.0));

        let config = LayoutConfig::default();
        let result = layout_with_labels(&graph, &config, |_, dims| *dims, &edge_labels);

        // The label position should exist and have a valid x coordinate
        assert!(result.label_positions.contains_key(&0));
        let label_pos = result.label_positions.get(&0).unwrap();

        // Label dummy width (50.0) should be accounted for — the label
        // position should be at a reasonable x coordinate
        let a_rect = result.nodes.get(&"A".into()).unwrap();
        // Label should be in the general vicinity of the edge path
        assert!(
            label_pos.point.x >= 0.0,
            "Label x should be non-negative, got {}",
            label_pos.point.x
        );
        assert!(
            label_pos.point.y > a_rect.y,
            "Label should be below A in TD layout"
        );
    }

    #[test]
    fn test_denorm_extracts_label_position_between_nodes() {
        // A -> B with label: verify label position is geometrically between A and B
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        let mut edge_labels = HashMap::new();
        edge_labels.insert(0, normalize::EdgeLabelInfo::new(50.0, 20.0));

        let config = LayoutConfig::default();
        let result = layout_with_labels(&graph, &config, |_, dims| *dims, &edge_labels);

        assert!(result.label_positions.contains_key(&0));
        let label_pos = result.label_positions.get(&0).unwrap();

        let a_y = result.nodes.get(&"A".into()).unwrap().y;
        let b_y = result.nodes.get(&"B".into()).unwrap().y;
        assert!(
            label_pos.point.y > a_y && label_pos.point.y < b_y,
            "Label y={} should be between A y={} and B y={}",
            label_pos.point.y,
            a_y,
            b_y
        );
    }

    #[test]
    fn test_layout_with_labels_short_edge_gets_label_position() {
        // A -> B (short edge, 1-rank span) with label
        // After make_space, it should span 2 ranks and get a label dummy
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B"); // Edge 0 - labeled

        let mut edge_labels = HashMap::new();
        edge_labels.insert(0, normalize::EdgeLabelInfo::new(50.0, 20.0));

        let config = LayoutConfig::default();
        let result = layout_with_labels(&graph, &config, |_, dims| *dims, &edge_labels);

        // Short labeled edge should now have a label position
        assert!(
            result.label_positions.contains_key(&0),
            "Short labeled edge should have a label position"
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
            label_pos.point.y > a_y && label_pos.point.y < c_y,
            "Label should be between A and C"
        );
    }

    #[test]
    fn test_layout_compound_graph_end_to_end() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("sg1", (0.0, 0.0));
        graph.add_node("A", (40.0, 20.0));
        graph.add_node("B", (40.0, 20.0));
        graph.add_edge("A", "B");
        graph.set_parent("A", "sg1");
        graph.set_parent("B", "sg1");

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        // Nodes should be laid out
        assert!(result.nodes.contains_key(&"A".into()));
        assert!(result.nodes.contains_key(&"B".into()));

        // Subgraph bounds should exist
        assert!(
            result.subgraph_bounds.contains_key("sg1"),
            "Should have subgraph bounds for sg1"
        );
        let bounds = &result.subgraph_bounds["sg1"];
        assert!(bounds.width > 0.0, "Subgraph width should be positive");
        assert!(bounds.height > 0.0, "Subgraph height should be positive");
    }

    #[test]
    fn test_layout_compound_titled_end_to_end() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("sg1", (0.0, 0.0));
        graph.add_node("A", (40.0, 20.0));
        graph.add_node("B", (40.0, 20.0));
        graph.add_edge("A", "B");
        graph.set_parent("A", "sg1");
        graph.set_parent("B", "sg1");
        graph.set_has_title("sg1");

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        assert!(result.nodes.contains_key(&"A".into()));
        assert!(result.nodes.contains_key(&"B".into()));
        assert!(result.subgraph_bounds.contains_key("sg1"));
    }

    #[test]
    fn test_layout_multi_level_compound_nesting() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (40.0, 20.0));
        g.add_node("B", (40.0, 20.0));
        g.add_node("inner", (0.0, 0.0));
        g.add_node("outer", (0.0, 0.0));
        g.add_edge("A", "B");
        g.set_parent("A", "inner");
        g.set_parent("B", "inner");
        g.set_parent("inner", "outer");

        let config = LayoutConfig::default();
        let result = layout(&g, &config, |_, dims| *dims);

        assert!(result.nodes.contains_key(&"A".into()));
        assert!(result.nodes.contains_key(&"B".into()));
    }

    #[test]
    fn test_layout_simple_graph_no_subgraph_bounds() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (40.0, 20.0));
        graph.add_node("B", (40.0, 20.0));
        graph.add_edge("A", "B");

        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);

        assert!(
            result.subgraph_bounds.is_empty(),
            "Simple graph should have no subgraph bounds"
        );
    }

    // --- Self-edge extraction tests ---

    fn build_lg_from_edges(edges: &[(&str, &str)]) -> LayoutGraph {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        let mut seen = std::collections::HashSet::new();
        for (from, to) in edges {
            if seen.insert(*from) {
                graph.add_node(*from, (10.0, 5.0));
            }
            if seen.insert(*to) {
                graph.add_node(*to, (10.0, 5.0));
            }
            graph.add_edge(*from, *to);
        }
        LayoutGraph::from_digraph(&graph, |_, dims| *dims)
    }

    #[test]
    fn test_extract_self_edges_single() {
        let mut lg = build_lg_from_edges(&[("A", "A")]);
        assert_eq!(lg.edges.len(), 1);
        extract_self_edges(&mut lg);
        assert_eq!(lg.self_edges.len(), 1);
        assert_eq!(lg.self_edges[0].node_index, lg.node_index[&"A".into()]);
        assert!(lg.edges.is_empty(), "self-edge should be removed");
    }

    #[test]
    fn test_extract_self_edges_mixed() {
        let mut lg = build_lg_from_edges(&[("A", "B"), ("A", "A"), ("B", "C")]);
        assert_eq!(lg.edges.len(), 3);
        extract_self_edges(&mut lg);
        assert_eq!(lg.self_edges.len(), 1);
        assert_eq!(lg.edges.len(), 2, "only non-self edges remain");
        // Parallel arrays should be in sync
        assert_eq!(lg.edge_weights.len(), 2);
        assert_eq!(lg.edge_minlens.len(), 2);
    }

    #[test]
    fn test_extract_self_edges_none() {
        let mut lg = build_lg_from_edges(&[("A", "B")]);
        extract_self_edges(&mut lg);
        assert!(lg.self_edges.is_empty());
        assert_eq!(lg.edges.len(), 1);
    }

    #[test]
    fn test_extract_self_edges_multiple() {
        let mut lg = build_lg_from_edges(&[("A", "A"), ("B", "B"), ("A", "B")]);
        extract_self_edges(&mut lg);
        assert_eq!(lg.self_edges.len(), 2);
        assert_eq!(lg.edges.len(), 1);
    }

    #[test]
    fn test_layout_with_self_edge_does_not_panic() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 5.0));
        graph.add_edge("A", "A");
        let config = LayoutConfig::default();
        let result = layout(&graph, &config, |_, dims| *dims);
        assert!(result.nodes.contains_key(&"A".into()));
    }

    #[test]
    fn test_insert_self_edge_dummy_creates_node() {
        let mut lg = build_lg_from_edges(&[("A", "B"), ("A", "A")]);
        extract_self_edges(&mut lg);
        // Simulate ranking and ordering
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        normalize::run(&mut lg, &HashMap::new());
        order::run(&mut lg);

        let node_count_before = lg.node_ids.len();
        assert_eq!(lg.self_edges.len(), 1);

        insert_self_edge_dummies(&mut lg);

        assert_eq!(lg.node_ids.len(), node_count_before + 1);
        assert!(lg.self_edges[0].dummy_index.is_some());
    }

    #[test]
    fn test_insert_self_edge_dummy_same_rank() {
        let mut lg = build_lg_from_edges(&[("A", "B"), ("A", "A")]);
        let a_idx = lg.node_index[&"A".into()];
        extract_self_edges(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        normalize::run(&mut lg, &HashMap::new());
        order::run(&mut lg);

        let node_rank = lg.ranks[a_idx];
        insert_self_edge_dummies(&mut lg);

        let dummy_idx = lg.self_edges[0].dummy_index.unwrap();
        assert_eq!(lg.ranks[dummy_idx], node_rank);
    }

    #[test]
    fn test_insert_self_edge_dummy_order_after_node() {
        let mut lg = build_lg_from_edges(&[("A", "B"), ("A", "A")]);
        let a_idx = lg.node_index[&"A".into()];
        extract_self_edges(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        normalize::run(&mut lg, &HashMap::new());
        order::run(&mut lg);

        let node_order = lg.order[a_idx];
        insert_self_edge_dummies(&mut lg);

        let dummy_idx = lg.self_edges[0].dummy_index.unwrap();
        assert_eq!(lg.order[dummy_idx], node_order + 1);
    }

    #[test]
    fn test_layout_result_contains_self_edge_layout() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 5.0));
        graph.add_edge("A", "A");
        let result = layout(&graph, &LayoutConfig::default(), |_, dims| *dims);
        assert_eq!(result.self_edges.len(), 1);
        assert_eq!(result.self_edges[0].node, "A".into());
        assert_eq!(result.self_edges[0].points.len(), 6);
    }

    #[test]
    fn test_layout_result_no_self_edges_when_none_exist() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 5.0));
        graph.add_node("B", (10.0, 5.0));
        graph.add_edge("A", "B");
        let result = layout(&graph, &LayoutConfig::default(), |_, dims| *dims);
        assert!(result.self_edges.is_empty());
    }

    #[test]
    fn test_position_self_edges_td_produces_6_points() {
        let mut lg = build_lg_from_edges(&[("A", "B"), ("A", "A")]);
        let a_idx = lg.node_index[&"A".into()];
        extract_self_edges(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        normalize::run(&mut lg, &HashMap::new());
        order::run(&mut lg);
        insert_self_edge_dummies(&mut lg);
        let config = LayoutConfig::default(); // TopBottom
        position::run(&mut lg, &config);
        let layouts = position_self_edges(&lg, &config);
        assert_eq!(layouts.len(), 1);
        assert_eq!(layouts[0].points.len(), 6);

        // Verify exit from bottom, enter from top (TD)
        let node_pos = lg.positions[a_idx];
        let (_nw, nh) = lg.dimensions[a_idx];
        let bot = node_pos.y + nh;
        let top = node_pos.y;
        assert!(
            layouts[0].points[0].y >= bot - 0.1,
            "first point should exit bottom"
        );
        assert!(
            layouts[0].points[5].y <= top + 0.1,
            "last point should enter top"
        );
    }

    #[test]
    fn test_layout_self_edge_dummy_not_in_result_nodes() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 5.0));
        graph.add_node("B", (10.0, 5.0));
        graph.add_edge("A", "B");
        graph.add_edge("A", "A");
        let result = layout(&graph, &LayoutConfig::default(), |_, dims| *dims);
        // Dummy should not appear in result nodes
        assert_eq!(result.nodes.len(), 2);
        assert!(result.nodes.contains_key(&"A".into()));
        assert!(result.nodes.contains_key(&"B".into()));
    }

    #[test]
    fn test_layout_self_edge_not_in_reversed_edges() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (10.0, 5.0));
        graph.add_node("B", (10.0, 5.0));
        graph.add_edge("A", "B");
        graph.add_edge("A", "A");
        let result = layout(&graph, &LayoutConfig::default(), |_, dims| *dims);
        // Self-edge orig index is 1. It should not appear in reversed_edges.
        assert!(
            !result.reversed_edges.contains(&1),
            "self-edge should not be in reversed_edges"
        );
    }
}
