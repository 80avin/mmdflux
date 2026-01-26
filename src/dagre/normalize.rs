//! Edge normalization for hierarchical graph layout.
//!
//! This module implements the normalization step of the Sugiyama framework,
//! which breaks long edges (spanning multiple ranks) into chains of short
//! edges (spanning exactly 1 rank each) by inserting dummy nodes.
//!
//! The key benefit is that after normalization, all edges span exactly one
//! rank, which enables:
//! - Proper crossing reduction (dummies participate like real nodes)
//! - Waypoint generation for edge routing
//! - Label placement on isolated edge segments

use std::collections::HashMap;

use super::graph::LayoutGraph;
use super::types::{NodeId, Point};

/// The type of dummy node inserted during normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DummyType {
    /// Regular dummy node with zero dimensions.
    /// Used to break long edges into single-rank segments.
    Edge,
    /// Dummy node that carries an edge label.
    /// Has non-zero dimensions based on the label text.
    EdgeLabel,
}

/// Label position relative to the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelPos {
    /// Label positioned to the left of the edge.
    Left,
    /// Label centered on the edge.
    #[default]
    Center,
    /// Label positioned to the right of the edge.
    Right,
}

/// Metadata for a dummy node inserted during normalization.
#[derive(Debug, Clone)]
pub struct DummyNode {
    /// The type of this dummy node.
    pub dummy_type: DummyType,
    /// Index of the original edge this dummy belongs to.
    pub edge_index: usize,
    /// The rank (layer) this dummy occupies.
    pub rank: i32,
    /// Width of the dummy (0 for Edge, label width for EdgeLabel).
    pub width: f64,
    /// Height of the dummy (0 for Edge, label height for EdgeLabel).
    pub height: f64,
    /// Label position (only meaningful for EdgeLabel dummies).
    pub label_pos: LabelPos,
}

impl DummyNode {
    /// Create a new regular edge dummy with zero dimensions.
    pub fn edge(edge_index: usize, rank: i32) -> Self {
        Self {
            dummy_type: DummyType::Edge,
            edge_index,
            rank,
            width: 0.0,
            height: 0.0,
            label_pos: LabelPos::default(),
        }
    }

    /// Create a new edge label dummy with the given dimensions.
    pub fn edge_label(
        edge_index: usize,
        rank: i32,
        width: f64,
        height: f64,
        label_pos: LabelPos,
    ) -> Self {
        Self {
            dummy_type: DummyType::EdgeLabel,
            edge_index,
            rank,
            width,
            height,
            label_pos,
        }
    }

    /// Returns true if this is a label-carrying dummy.
    pub fn is_label(&self) -> bool {
        self.dummy_type == DummyType::EdgeLabel
    }
}

/// A chain of dummy nodes representing a normalized long edge.
///
/// The chain starts at the source node and ends at the target node,
/// with dummy nodes at each intermediate rank.
#[derive(Debug, Clone)]
pub struct DummyChain {
    /// Index of the original edge in the input graph.
    pub edge_index: usize,
    /// IDs of the dummy nodes in this chain, in order from source to target.
    /// Does not include the original source/target nodes.
    pub dummy_ids: Vec<NodeId>,
    /// Index of the label dummy within dummy_ids (if any).
    pub label_dummy_index: Option<usize>,
}

impl DummyChain {
    /// Create a new empty dummy chain for an edge.
    pub fn new(edge_index: usize) -> Self {
        Self {
            edge_index,
            dummy_ids: Vec::new(),
            label_dummy_index: None,
        }
    }

    /// Returns true if this chain contains a label dummy.
    pub fn has_label(&self) -> bool {
        self.label_dummy_index.is_some()
    }
}

/// Information about edge label dimensions, used during normalization.
#[derive(Debug, Clone, Default)]
pub struct EdgeLabelInfo {
    /// Width of the label in layout units.
    pub width: f64,
    /// Height of the label in layout units.
    pub height: f64,
    /// Preferred position of the label.
    pub label_pos: LabelPos,
}

impl EdgeLabelInfo {
    /// Create new edge label info with the given dimensions.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            label_pos: LabelPos::default(),
        }
    }

    /// Set the label position.
    pub fn with_pos(mut self, pos: LabelPos) -> Self {
        self.label_pos = pos;
        self
    }
}

/// Counter for generating unique dummy node IDs.
static DUMMY_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Generate a unique dummy node ID.
fn generate_dummy_id() -> NodeId {
    let id = DUMMY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    NodeId::from(format!("_d{}", id))
}

/// Reset the dummy counter (for testing).
#[cfg(test)]
#[allow(dead_code)]
fn reset_dummy_counter() {
    DUMMY_COUNTER.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// Normalize long edges by inserting dummy nodes.
///
/// This function processes each edge and, if it spans more than one rank,
/// creates a chain of dummy nodes at intermediate ranks. The original edge
/// is replaced with a chain of edges connecting source -> dummies -> target.
///
/// After normalization, all edges span exactly one rank, which is required
/// for proper crossing reduction and coordinate assignment.
///
/// # Arguments
/// * `graph` - The layout graph to normalize
/// * `edge_labels` - Optional label information for edges (keyed by original edge index)
pub(crate) fn run(graph: &mut LayoutGraph, edge_labels: &HashMap<usize, EdgeLabelInfo>) {
    // Clear any existing dummy data
    graph.dummy_nodes.clear();
    graph.dummy_chains.clear();

    // Get effective edges with reversals applied
    let effective = graph.effective_edges();

    // Collect edges that need normalization
    // (from_idx, to_idx, orig_edge_idx, from_rank, to_rank)
    let mut long_edges: Vec<(usize, usize, usize, i32, i32)> = Vec::new();

    for (edge_idx, &(from_idx, to_idx, orig_edge_idx)) in graph.edges.iter().enumerate() {
        // Get effective direction (considering reversals)
        let (eff_from, eff_to) = effective[edge_idx];
        let from_rank = graph.ranks[eff_from];
        let to_rank = graph.ranks[eff_to];

        // Only normalize edges that span more than 1 rank
        // Note: after acyclic phase and ranking, to_rank > from_rank for effective edges
        if to_rank > from_rank + 1 {
            long_edges.push((from_idx, to_idx, orig_edge_idx, from_rank, to_rank));
        }
    }

    // Process each long edge
    for (from_idx, to_idx, orig_edge_idx, from_rank, to_rank) in long_edges {
        normalize_edge(
            graph,
            from_idx,
            to_idx,
            orig_edge_idx,
            from_rank,
            to_rank,
            edge_labels,
        );
    }
}

/// Normalize a single long edge by inserting dummy nodes.
fn normalize_edge(
    graph: &mut LayoutGraph,
    from_idx: usize,
    to_idx: usize,
    orig_edge_idx: usize,
    from_rank: i32,
    to_rank: i32,
    edge_labels: &HashMap<usize, EdgeLabelInfo>,
) {
    // Calculate the label rank (midpoint of the edge)
    let label_rank = if edge_labels.contains_key(&orig_edge_idx) {
        Some((from_rank + to_rank) / 2)
    } else {
        None
    };

    let label_info = edge_labels.get(&orig_edge_idx);

    // Create a dummy chain to track this edge
    let mut chain = DummyChain::new(orig_edge_idx);

    // Remove the original edge from the graph
    // Find and remove it from graph.edges
    let edge_pos = graph
        .edges
        .iter()
        .position(|&(f, t, idx)| f == from_idx && t == to_idx && idx == orig_edge_idx);

    if let Some(pos) = edge_pos {
        graph.edges.remove(pos);
    }

    // Create dummy nodes for each intermediate rank
    let mut prev_idx = from_idx;
    for rank in (from_rank + 1)..to_rank {
        let dummy_id = generate_dummy_id();
        let dummy_idx = graph.node_ids.len();

        // Determine if this is the label dummy
        let is_label_dummy = label_rank == Some(rank);

        let (dummy_node, width, height) = if is_label_dummy {
            let info = label_info.unwrap();
            (
                DummyNode::edge_label(orig_edge_idx, rank, info.width, info.height, info.label_pos),
                info.width,
                info.height,
            )
        } else {
            (DummyNode::edge(orig_edge_idx, rank), 0.0, 0.0)
        };

        // Add dummy to the graph
        graph.node_ids.push(dummy_id.clone());
        graph.node_index.insert(dummy_id.clone(), dummy_idx);
        graph.ranks.push(rank);
        graph.order.push(dummy_idx); // Will be reordered during crossing reduction
        graph.positions.push(Point::default());
        graph.dimensions.push((width, height));
        graph.dummy_nodes.insert(dummy_id.clone(), dummy_node);

        // Track in chain
        if is_label_dummy {
            chain.label_dummy_index = Some(chain.dummy_ids.len());
        }
        chain.dummy_ids.push(dummy_id);

        // Add edge from previous node to this dummy
        graph.edges.push((prev_idx, dummy_idx, orig_edge_idx));
        prev_idx = dummy_idx;
    }

    // Add final edge from last dummy to target
    graph.edges.push((prev_idx, to_idx, orig_edge_idx));

    // Store the chain
    graph.dummy_chains.push(chain);
}

/// Extract waypoints from dummy node positions after coordinate assignment.
///
/// This should be called after the position phase to convert dummy positions
/// into edge waypoints for routing.
///
/// # Returns
/// A map from original edge index to a list of waypoint coordinates.
pub(crate) fn denormalize(graph: &LayoutGraph) -> HashMap<usize, Vec<Point>> {
    let mut waypoints: HashMap<usize, Vec<Point>> = HashMap::new();

    for chain in &graph.dummy_chains {
        let mut points = Vec::new();

        for dummy_id in &chain.dummy_ids {
            if let Some(&dummy_idx) = graph.node_index.get(dummy_id) {
                let pos = graph.positions[dummy_idx];
                let dims = graph.dimensions[dummy_idx];

                // Use center of dummy (for label dummies with non-zero size)
                points.push(Point {
                    x: pos.x + dims.0 / 2.0,
                    y: pos.y + dims.1 / 2.0,
                });
            }
        }

        waypoints.insert(chain.edge_index, points);
    }

    waypoints
}

/// Get the label position for an edge if it has a label dummy.
///
/// # Returns
/// The (x, y) center position of the label, or None if the edge has no label.
pub(crate) fn get_label_position(graph: &LayoutGraph, edge_index: usize) -> Option<Point> {
    for chain in &graph.dummy_chains {
        if chain.edge_index == edge_index {
            if let Some(label_idx) = chain.label_dummy_index {
                let dummy_id = &chain.dummy_ids[label_idx];
                if let Some(&idx) = graph.node_index.get(dummy_id) {
                    let pos = graph.positions[idx];
                    let dims = graph.dimensions[idx];
                    return Some(Point {
                        x: pos.x + dims.0 / 2.0,
                        y: pos.y + dims.1 / 2.0,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::{DiGraph, LayoutGraph};
    use crate::dagre::{acyclic, rank};

    /// Helper to create a layout graph for testing.
    fn create_test_graph(nodes: &[&str], edges: &[(&str, &str)]) -> LayoutGraph {
        let mut graph: DiGraph<()> = DiGraph::new();
        for node in nodes {
            graph.add_node(*node, ());
        }
        for (from, to) in edges {
            graph.add_edge(*from, *to);
        }
        LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0))
    }

    #[test]
    fn test_normalize_short_edge() {
        reset_dummy_counter();
        // A -> B (spans 1 rank, should not be normalized)
        let mut lg = create_test_graph(&["A", "B"], &[("A", "B")]);
        acyclic::run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        let edge_labels = HashMap::new();
        run(&mut lg, &edge_labels);

        // No dummies should be created
        assert!(lg.dummy_chains.is_empty());
        assert!(lg.dummy_nodes.is_empty());
        // Original edge should still exist
        assert_eq!(lg.edges.len(), 1);
    }

    #[test]
    fn test_normalize_long_edge() {
        reset_dummy_counter();
        // A -> B -> C, but also A -> C (spans 2 ranks)
        let mut lg = create_test_graph(&["A", "B", "C"], &[("A", "B"), ("B", "C"), ("A", "C")]);
        acyclic::run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        // Verify ranks: A=0, B=1, C=2
        let a_idx = lg.node_index[&NodeId::from("A")];
        let b_idx = lg.node_index[&NodeId::from("B")];
        let c_idx = lg.node_index[&NodeId::from("C")];
        assert_eq!(lg.ranks[a_idx], 0);
        assert_eq!(lg.ranks[b_idx], 1);
        assert_eq!(lg.ranks[c_idx], 2);

        let edge_labels = HashMap::new();
        run(&mut lg, &edge_labels);

        // A->C should be normalized (spans 2 ranks, needs 1 dummy)
        assert_eq!(lg.dummy_chains.len(), 1);
        assert_eq!(lg.dummy_chains[0].dummy_ids.len(), 1);

        // Should now have 4 nodes (A, B, C, + 1 dummy)
        assert_eq!(lg.node_ids.len(), 4);

        // The dummy should be at rank 1
        let dummy_id = &lg.dummy_chains[0].dummy_ids[0];
        let dummy_idx = lg.node_index[dummy_id];
        assert_eq!(lg.ranks[dummy_idx], 1);
    }

    #[test]
    fn test_normalize_with_label() {
        reset_dummy_counter();
        // A -> B -> C -> D, and A -> D (spans 3 ranks, needs 2 dummies)
        let mut lg = create_test_graph(
            &["A", "B", "C", "D"],
            &[("A", "B"), ("B", "C"), ("C", "D"), ("A", "D")],
        );
        acyclic::run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        // Create label info for edge A->D (which should be edge index 3)
        let mut edge_labels = HashMap::new();
        edge_labels.insert(3, EdgeLabelInfo::new(20.0, 10.0));

        run(&mut lg, &edge_labels);

        // A->D needs 2 dummies (rank 1 and rank 2)
        assert_eq!(lg.dummy_chains.len(), 1);
        assert_eq!(lg.dummy_chains[0].dummy_ids.len(), 2);

        // Label should be at the midpoint (rank 1 or 2)
        assert!(lg.dummy_chains[0].label_dummy_index.is_some());

        // Label dummy should have the specified dimensions
        let label_idx = lg.dummy_chains[0].label_dummy_index.unwrap();
        let label_dummy_id = &lg.dummy_chains[0].dummy_ids[label_idx];
        let label_dummy = lg.dummy_nodes.get(label_dummy_id).unwrap();
        assert!(label_dummy.is_label());
        assert_eq!(label_dummy.width, 20.0);
        assert_eq!(label_dummy.height, 10.0);
    }

    #[test]
    fn test_denormalize() {
        reset_dummy_counter();
        // A -> B -> C, and A -> C
        let mut lg = create_test_graph(&["A", "B", "C"], &[("A", "B"), ("B", "C"), ("A", "C")]);
        acyclic::run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        let edge_labels = HashMap::new();
        run(&mut lg, &edge_labels);

        // Set dummy position manually for testing
        let dummy_id = &lg.dummy_chains[0].dummy_ids[0];
        let dummy_idx = lg.node_index[dummy_id];
        lg.positions[dummy_idx] = Point { x: 50.0, y: 100.0 };

        let waypoints = denormalize(&lg);

        // Should have waypoints for the normalized edge
        assert!(waypoints.contains_key(&lg.dummy_chains[0].edge_index));
        let points = &waypoints[&lg.dummy_chains[0].edge_index];
        assert_eq!(points.len(), 1);
        // Dummy has zero dimensions, so center is the position itself
        assert_eq!(points[0].x, 50.0);
        assert_eq!(points[0].y, 100.0);
    }

    #[test]
    fn test_dummy_node_edge() {
        let dummy = DummyNode::edge(0, 2);
        assert_eq!(dummy.dummy_type, DummyType::Edge);
        assert_eq!(dummy.edge_index, 0);
        assert_eq!(dummy.rank, 2);
        assert_eq!(dummy.width, 0.0);
        assert_eq!(dummy.height, 0.0);
        assert!(!dummy.is_label());
    }

    #[test]
    fn test_dummy_node_edge_label() {
        let dummy = DummyNode::edge_label(1, 3, 10.0, 5.0, LabelPos::Center);
        assert_eq!(dummy.dummy_type, DummyType::EdgeLabel);
        assert_eq!(dummy.edge_index, 1);
        assert_eq!(dummy.rank, 3);
        assert_eq!(dummy.width, 10.0);
        assert_eq!(dummy.height, 5.0);
        assert_eq!(dummy.label_pos, LabelPos::Center);
        assert!(dummy.is_label());
    }

    #[test]
    fn test_dummy_chain() {
        let mut chain = DummyChain::new(0);
        assert_eq!(chain.edge_index, 0);
        assert!(chain.dummy_ids.is_empty());
        assert!(!chain.has_label());

        chain.dummy_ids.push(NodeId::from("_d0"));
        chain.dummy_ids.push(NodeId::from("_d1"));
        chain.label_dummy_index = Some(1);

        assert_eq!(chain.dummy_ids.len(), 2);
        assert!(chain.has_label());
    }

    #[test]
    fn test_edge_label_info() {
        let info = EdgeLabelInfo::new(20.0, 10.0).with_pos(LabelPos::Left);
        assert_eq!(info.width, 20.0);
        assert_eq!(info.height, 10.0);
        assert_eq!(info.label_pos, LabelPos::Left);
    }

    #[test]
    fn test_label_pos_default() {
        let pos = LabelPos::default();
        assert_eq!(pos, LabelPos::Center);
    }

    #[test]
    fn test_dummy_chain_multiple_dummies() {
        // Simulate an edge spanning 4 ranks (needs 3 dummies)
        let mut chain = DummyChain::new(5);
        chain.dummy_ids.push(NodeId::from("_d0"));
        chain.dummy_ids.push(NodeId::from("_d1")); // This is the label dummy
        chain.dummy_ids.push(NodeId::from("_d2"));
        chain.label_dummy_index = Some(1);

        assert_eq!(chain.edge_index, 5);
        assert_eq!(chain.dummy_ids.len(), 3);
        assert!(chain.has_label());
        assert_eq!(
            chain.dummy_ids[chain.label_dummy_index.unwrap()],
            NodeId::from("_d1")
        );
    }

    #[test]
    fn test_dummy_type_equality() {
        assert_eq!(DummyType::Edge, DummyType::Edge);
        assert_eq!(DummyType::EdgeLabel, DummyType::EdgeLabel);
        assert_ne!(DummyType::Edge, DummyType::EdgeLabel);
    }

    #[test]
    fn test_edge_label_info_default() {
        let info = EdgeLabelInfo::default();
        assert_eq!(info.width, 0.0);
        assert_eq!(info.height, 0.0);
        assert_eq!(info.label_pos, LabelPos::Center);
    }

    #[test]
    fn test_dummy_node_clone() {
        let dummy = DummyNode::edge_label(2, 5, 15.0, 8.0, LabelPos::Right);
        let cloned = dummy.clone();

        assert_eq!(cloned.dummy_type, DummyType::EdgeLabel);
        assert_eq!(cloned.edge_index, 2);
        assert_eq!(cloned.rank, 5);
        assert_eq!(cloned.width, 15.0);
        assert_eq!(cloned.height, 8.0);
        assert_eq!(cloned.label_pos, LabelPos::Right);
    }
}
