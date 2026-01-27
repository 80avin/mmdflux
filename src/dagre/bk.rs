//! Brandes-Köpf algorithm for horizontal coordinate assignment.
//!
//! This module implements the algorithm described in:
//! Brandes, U. and Köpf, B. (2001). Fast and Simple Horizontal Coordinate Assignment.
//!
//! The algorithm produces x-coordinates that minimize total edge length while
//! respecting node separation constraints.

use std::collections::HashMap;

use super::graph::LayoutGraph;
use super::types::Direction;

/// Index type for nodes in the layout graph
pub type NodeIndex = usize;

/// A conflict between edges that prevents alignment.
///
/// Conflicts occur when aligning a node with its median neighbor would cause
/// edge crossings with inner segments (long edges through dummy nodes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The layer where the conflict occurs
    pub layer: usize,
    /// Position of first conflicting node in layer
    pub pos1: usize,
    /// Position of second conflicting node in layer
    pub pos2: usize,
}

/// Set of conflicts indexed by (layer, pos1, pos2) for O(1) lookup.
/// We use a nested HashMap: layer -> (pos1, pos2) -> Conflict
pub type ConflictSet = HashMap<(usize, usize, usize), Conflict>;

/// Represents a vertical alignment of nodes into blocks.
///
/// A "block" is a set of nodes that are vertically aligned (same x-coordinate).
/// The alignment is represented as a linked list through the `align` map,
/// with each block having a single root node.
#[derive(Debug, Clone)]
pub struct BlockAlignment {
    /// Maps each node to its block root (representative node).
    /// All nodes in the same block share the same root.
    pub root: HashMap<NodeIndex, NodeIndex>,

    /// Maps each node to the next node in its alignment chain.
    /// Forms a linked list within each block.
    pub align: HashMap<NodeIndex, NodeIndex>,
}

impl BlockAlignment {
    /// Create a new alignment where each node is its own singleton block.
    pub fn new(nodes: &[NodeIndex]) -> Self {
        let mut root = HashMap::new();
        let mut align = HashMap::new();

        // Initially, each node is its own root and aligns to itself
        for &node in nodes {
            root.insert(node, node);
            align.insert(node, node);
        }

        Self { root, align }
    }

    /// Get the root of the block containing `node`.
    pub fn get_root(&self, node: NodeIndex) -> NodeIndex {
        self.root.get(&node).copied().unwrap_or(node)
    }

    /// Align node `v` with node `w`.
    ///
    /// This adds `v` to the block containing `w`. The `align` pointer of `v`
    /// points to `w`, and `v`'s root becomes `w`'s root.
    pub fn align_nodes(&mut self, v: NodeIndex, w: NodeIndex) {
        // Set alignment: v points to w
        self.align.insert(v, w);
        // Set root: v's root becomes w's root
        let w_root = self.get_root(w);
        self.root.insert(v, w_root);
    }
}

/// Result of horizontal compaction for one alignment.
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// X coordinate for each node.
    pub x: HashMap<NodeIndex, f64>,

    /// Sink (class representative) for each block root.
    /// Used during compaction to track which "class" a block belongs to.
    pub sink: HashMap<NodeIndex, NodeIndex>,

    /// Shift amount for each class (keyed by sink).
    /// Applied during the final coordinate assignment phase.
    pub shift: HashMap<NodeIndex, f64>,
}

impl CompactionResult {
    pub fn new() -> Self {
        Self {
            x: HashMap::new(),
            sink: HashMap::new(),
            shift: HashMap::new(),
        }
    }
}

impl Default for CompactionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// The four alignment directions used by Brandes-Köpf.
///
/// The algorithm computes four different alignments and takes the median
/// of all four to produce balanced coordinates. Each direction represents
/// a combination of:
/// - Sweep direction: top-to-bottom (downward) or bottom-to-top (upward)
/// - Neighbor preference: prefer left or right median neighbor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignmentDirection {
    /// Up-Left: sweep top-to-bottom, prefer left neighbors
    UL,
    /// Up-Right: sweep top-to-bottom, prefer right neighbors
    UR,
    /// Down-Left: sweep bottom-to-top, prefer left neighbors
    DL,
    /// Down-Right: sweep bottom-to-top, prefer right neighbors
    DR,
}

impl AlignmentDirection {
    /// Returns all four alignment directions.
    pub fn all() -> [Self; 4] {
        [Self::UL, Self::UR, Self::DL, Self::DR]
    }

    /// Whether this direction sweeps from top to bottom (downward).
    ///
    /// UL and UR sweep downward (processing layers from top to bottom).
    /// DL and DR sweep upward (processing layers from bottom to top).
    pub fn is_downward(&self) -> bool {
        matches!(self, Self::UL | Self::UR)
    }

    /// Whether this direction prefers left neighbors when there are two medians.
    ///
    /// When a node has an even number of neighbors, there are two median values.
    /// UL and DL prefer the left (lower index) median.
    /// UR and DR prefer the right (higher index) median.
    pub fn prefers_left(&self) -> bool {
        matches!(self, Self::UL | Self::DL)
    }
}

/// Configuration for the Brandes-Köpf algorithm.
#[derive(Debug, Clone)]
pub struct BKConfig {
    /// Minimum separation between adjacent nodes.
    pub node_sep: f64,

    /// Layout direction (affects which axis is "horizontal").
    pub direction: Direction,
}

impl Default for BKConfig {
    fn default() -> Self {
        Self {
            node_sep: 50.0,
            direction: Direction::TopBottom,
        }
    }
}

// =============================================================================
// Helper Functions for Layer/Neighbor Traversal
// =============================================================================

/// Get all nodes grouped by layer (rank), sorted by position within each layer.
///
/// Returns a vector where index is the layer number and value is a vector of
/// node indices in that layer, sorted by their order within the layer.
pub fn get_layers(graph: &LayoutGraph) -> Vec<Vec<NodeIndex>> {
    let max_rank = graph.ranks.iter().max().copied().unwrap_or(0) as usize;
    let mut layers: Vec<Vec<NodeIndex>> = vec![Vec::new(); max_rank + 1];

    for (node, &rank) in graph.ranks.iter().enumerate() {
        layers[rank as usize].push(node);
    }

    // Sort each layer by order (position within layer)
    for layer in &mut layers {
        layer.sort_by_key(|&node| graph.order[node]);
    }

    layers
}

/// Get the layer indices in sweep order.
///
/// For downward sweep (UL, UR): layers 0, 1, 2, ... (top to bottom)
/// For upward sweep (DL, DR): layers n, n-1, ... 0 (bottom to top)
pub fn get_layers_in_order(num_layers: usize, downward: bool) -> Vec<usize> {
    if downward {
        (0..num_layers).collect()
    } else {
        (0..num_layers).rev().collect()
    }
}

/// Get the predecessors of a node (nodes in the layer above that connect to this node).
///
/// Returns node indices sorted by their position in their layer.
pub fn get_predecessors(graph: &LayoutGraph, node: NodeIndex) -> Vec<NodeIndex> {
    let effective_edges = graph.effective_edges();
    let mut preds: Vec<NodeIndex> = effective_edges
        .iter()
        .filter(|&&(_, to)| to == node)
        .map(|&(from, _)| from)
        .collect();

    preds.sort_by_key(|&n| graph.order[n]);
    preds
}

/// Get the successors of a node (nodes in the layer below that this node connects to).
///
/// Returns node indices sorted by their position in their layer.
pub fn get_successors(graph: &LayoutGraph, node: NodeIndex) -> Vec<NodeIndex> {
    let effective_edges = graph.effective_edges();
    let mut succs: Vec<NodeIndex> = effective_edges
        .iter()
        .filter(|&&(from, _)| from == node)
        .map(|&(_, to)| to)
        .collect();

    succs.sort_by_key(|&n| graph.order[n]);
    succs
}

/// Get neighbors based on sweep direction.
///
/// - Downward sweep (UL, UR): use predecessors (upper neighbors)
/// - Upward sweep (DL, DR): use successors (lower neighbors)
///
/// Returns neighbors sorted by position in their layer.
pub fn get_neighbors(graph: &LayoutGraph, node: NodeIndex, downward: bool) -> Vec<NodeIndex> {
    if downward {
        get_predecessors(graph, node)
    } else {
        get_successors(graph, node)
    }
}

/// Get the median neighbor of a node.
///
/// For odd number of neighbors, returns the true median.
/// For even number of neighbors, returns either the left-middle or right-middle
/// depending on `prefer_left`.
///
/// Returns `None` if the node has no neighbors in the specified direction.
pub fn get_median_neighbor(
    graph: &LayoutGraph,
    node: NodeIndex,
    downward: bool,
    prefer_left: bool,
) -> Option<NodeIndex> {
    let neighbors = get_neighbors(graph, node, downward);

    if neighbors.is_empty() {
        return None;
    }

    let len = neighbors.len();
    if len == 1 {
        return Some(neighbors[0]);
    }

    // For even length, choose based on preference
    let median_idx = if len % 2 == 0 {
        if prefer_left {
            len / 2 - 1 // Left-middle
        } else {
            len / 2 // Right-middle
        }
    } else {
        len / 2 // True middle for odd length
    };

    Some(neighbors[median_idx])
}

/// Get the position (order) of a node within its layer.
#[inline]
pub fn get_position(graph: &LayoutGraph, node: NodeIndex) -> usize {
    graph.order[node]
}

/// Get the layer (rank) of a node.
#[inline]
pub fn get_layer(graph: &LayoutGraph, node: NodeIndex) -> usize {
    graph.ranks[node] as usize
}

/// Check if a node is a dummy node (from edge normalization).
#[inline]
pub fn is_dummy(graph: &LayoutGraph, node: NodeIndex) -> bool {
    graph.is_dummy_index(node)
}

/// Get the width of a node.
#[inline]
pub fn get_width(graph: &LayoutGraph, node: NodeIndex) -> f64 {
    graph.dimensions[node].0
}

// =============================================================================
// Conflict Detection
// =============================================================================

/// Check if two segments cross.
///
/// Segments are defined by (upper_position, lower_position) where positions
/// are the node's order within its layer.
///
/// Two segments cross if one starts left and ends right of the other, or vice versa.
#[inline]
fn segments_cross(u1: usize, l1: usize, u2: usize, l2: usize) -> bool {
    (u1 < u2 && l1 > l2) || (u1 > u2 && l1 < l2)
}

/// Check if an edge is an inner segment (both endpoints are dummy nodes).
///
/// Inner segments are part of long edges that span multiple layers.
#[inline]
fn is_inner_segment(graph: &LayoutGraph, from: NodeIndex, to: NodeIndex) -> bool {
    is_dummy(graph, from) && is_dummy(graph, to)
}

/// Find all inner segments (edges between dummy nodes) between two adjacent layers.
///
/// Returns a vector of (upper_position, lower_position) tuples.
fn find_inner_segments(
    graph: &LayoutGraph,
    upper_layer: usize,
    lower_layer: usize,
) -> Vec<(usize, usize)> {
    let effective_edges = graph.effective_edges();
    let mut segments = Vec::new();

    for &(from, to) in &effective_edges {
        let from_layer = get_layer(graph, from);
        let to_layer = get_layer(graph, to);

        // Check if edge spans from upper to lower layer
        if from_layer != upper_layer || to_layer != lower_layer {
            continue;
        }

        // Check if both endpoints are dummy nodes (inner segment)
        if is_inner_segment(graph, from, to) {
            let from_pos = get_position(graph, from);
            let to_pos = get_position(graph, to);
            segments.push((from_pos, to_pos));
        }
    }

    segments
}

/// Find all Type-1 conflicts in the graph.
///
/// A Type-1 conflict occurs when a non-inner segment crosses an inner segment.
/// Inner segments are edges between dummy nodes (part of long edge normalization).
///
/// These conflicts are used during vertical alignment to prevent alignments
/// that would cause edge crossings.
pub fn find_type1_conflicts(graph: &LayoutGraph) -> ConflictSet {
    let mut conflicts = ConflictSet::new();
    let max_layer = graph.ranks.iter().copied().max().unwrap_or(0) as usize;

    // For each pair of adjacent layers
    for layer in 0..max_layer {
        let upper_layer = layer;
        let lower_layer = layer + 1;

        // Find inner segments in this layer pair
        let inner_segments = find_inner_segments(graph, upper_layer, lower_layer);

        if inner_segments.is_empty() {
            continue;
        }

        // Get all edges between these layers
        let effective_edges = graph.effective_edges();

        for &(from, to) in &effective_edges {
            let from_layer = get_layer(graph, from);
            let to_layer = get_layer(graph, to);

            // Skip if not in this layer pair
            if from_layer != upper_layer || to_layer != lower_layer {
                continue;
            }

            // Skip if this is an inner segment
            if is_inner_segment(graph, from, to) {
                continue;
            }

            // Check for crossings with inner segments
            let from_pos = get_position(graph, from);
            let to_pos = get_position(graph, to);

            for &(inner_upper, inner_lower) in &inner_segments {
                if segments_cross(from_pos, to_pos, inner_upper, inner_lower) {
                    // Record conflict using positions in the upper layer
                    let pos1 = from_pos.min(inner_upper);
                    let pos2 = from_pos.max(inner_upper);
                    conflicts.insert((layer, pos1, pos2), Conflict { layer, pos1, pos2 });
                }
            }
        }
    }

    conflicts
}

/// Find all Type-2 conflicts in the graph.
///
/// A Type-2 conflict occurs between inner segments of different long edges
/// when they cross each other.
pub fn find_type2_conflicts(graph: &LayoutGraph) -> ConflictSet {
    let mut conflicts = ConflictSet::new();
    let max_layer = graph.ranks.iter().copied().max().unwrap_or(0) as usize;

    // For each pair of adjacent layers
    for layer in 0..max_layer {
        let upper_layer = layer;
        let lower_layer = layer + 1;

        // Find all inner segments
        let inner_segments = find_inner_segments(graph, upper_layer, lower_layer);

        // Check each pair of inner segments for crossing
        for i in 0..inner_segments.len() {
            for j in (i + 1)..inner_segments.len() {
                let (u1, l1) = inner_segments[i];
                let (u2, l2) = inner_segments[j];

                if segments_cross(u1, l1, u2, l2) {
                    let pos1 = u1.min(u2);
                    let pos2 = u1.max(u2);
                    conflicts.insert((layer, pos1, pos2), Conflict { layer, pos1, pos2 });
                }
            }
        }
    }

    conflicts
}

/// Find all conflicts (Type-1 and Type-2) in the graph.
pub fn find_all_conflicts(graph: &LayoutGraph) -> ConflictSet {
    let mut conflicts = find_type1_conflicts(graph);

    for ((layer, pos1, pos2), conflict) in find_type2_conflicts(graph) {
        conflicts.entry((layer, pos1, pos2)).or_insert(conflict);
    }

    conflicts
}

/// Check if aligning two positions would violate a conflict.
///
/// Used during vertical alignment to skip alignments that would cause crossings.
pub fn has_conflict(conflicts: &ConflictSet, layer: usize, pos1: usize, pos2: usize) -> bool {
    let min_pos = pos1.min(pos2);
    let max_pos = pos1.max(pos2);

    // Check if there's a conflict that falls within this range
    // A conflict at (layer, p1, p2) blocks alignments where min_pos <= p1 and p2 <= max_pos
    for (&(conf_layer, conf_pos1, conf_pos2), _) in conflicts {
        if conf_layer == layer && min_pos <= conf_pos1 && conf_pos2 <= max_pos {
            return true;
        }
    }

    false
}

// =============================================================================
// Main Algorithm Entry Point
// =============================================================================

/// Main entry point for Brandes-Köpf coordinate assignment.
///
/// Returns x-coordinates for all nodes in the graph that minimize total
/// edge length while respecting separation constraints.
///
/// # Algorithm
///
/// 1. Find Type-1 and Type-2 conflicts between edges
/// 2. For each of 4 alignment directions (UL, UR, DL, DR):
///    a. Compute vertical alignment (group nodes into blocks)
///    b. Compute horizontal compaction (assign x-coordinates)
/// 3. Select the alignment with smallest width
/// 4. Balance by taking median of all 4 alignments
#[allow(unused_variables)] // TODO: Remove when implemented
pub fn position_x(graph: &LayoutGraph, config: &BKConfig) -> HashMap<NodeIndex, f64> {
    // TODO: Implement in subsequent tasks
    // 1. Find conflicts
    // 2. Compute 4 alignments
    // 3. Compact each alignment
    // 4. Balance and return

    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::DiGraph;
    use crate::dagre::rank;

    /// Create a diamond-shaped test graph:
    /// ```text
    /// Layer 0:    [A]
    /// Layer 1:  [B] [C]
    /// Layer 2:    [D]
    /// ```
    /// Edges: A->B, A->C, B->D, C->D
    fn make_diamond_graph() -> LayoutGraph {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_node("C", (100.0, 50.0));
        graph.add_node("D", (100.0, 50.0));
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");
        graph.add_edge("B", "D");
        graph.add_edge("C", "D");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        // Set order within layers: B before C
        // A is alone in layer 0 (order 0)
        // B at order 0, C at order 1 in layer 1
        // D is alone in layer 2 (order 0)
        let a = lg.node_index[&"A".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];
        let d = lg.node_index[&"D".into()];

        lg.order[a] = 0;
        lg.order[b] = 0;
        lg.order[c] = 1;
        lg.order[d] = 0;

        lg
    }

    #[test]
    fn test_block_alignment_new() {
        let nodes = vec![0, 1, 2, 3];
        let alignment = BlockAlignment::new(&nodes);

        // Each node should be its own root
        for &node in &nodes {
            assert_eq!(alignment.get_root(node), node);
            assert_eq!(alignment.align.get(&node), Some(&node));
        }
    }

    #[test]
    fn test_block_alignment_align_nodes() {
        let nodes = vec![0, 1, 2];
        let mut alignment = BlockAlignment::new(&nodes);

        // Align 0 with 1: node 0 joins node 1's block
        alignment.align_nodes(0, 1);
        assert_eq!(alignment.get_root(0), 1);
        assert_eq!(alignment.align.get(&0), Some(&1));

        // Align 2 with 1: node 2 also joins node 1's block
        alignment.align_nodes(2, 1);
        assert_eq!(alignment.get_root(2), 1);
        assert_eq!(alignment.align.get(&2), Some(&1));

        // Node 1 is still its own root
        assert_eq!(alignment.get_root(1), 1);
    }

    #[test]
    fn test_block_alignment_chain() {
        let nodes = vec![0, 1, 2, 3];
        let mut alignment = BlockAlignment::new(&nodes);

        // Build a chain: 0 -> 1 -> 2 -> 3
        // In downward sweep, we'd align upper nodes with lower nodes
        alignment.align_nodes(0, 1);
        alignment.align_nodes(1, 2);
        alignment.align_nodes(2, 3);

        // All nodes should share the same root (3)
        assert_eq!(alignment.get_root(0), 1); // Note: root propagation is shallow
        assert_eq!(alignment.get_root(1), 2);
        assert_eq!(alignment.get_root(2), 3);
        assert_eq!(alignment.get_root(3), 3);
    }

    #[test]
    fn test_alignment_direction_properties() {
        // Downward sweep
        assert!(AlignmentDirection::UL.is_downward());
        assert!(AlignmentDirection::UR.is_downward());
        assert!(!AlignmentDirection::DL.is_downward());
        assert!(!AlignmentDirection::DR.is_downward());

        // Left preference
        assert!(AlignmentDirection::UL.prefers_left());
        assert!(!AlignmentDirection::UR.prefers_left());
        assert!(AlignmentDirection::DL.prefers_left());
        assert!(!AlignmentDirection::DR.prefers_left());
    }

    #[test]
    fn test_alignment_direction_all() {
        let all = AlignmentDirection::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&AlignmentDirection::UL));
        assert!(all.contains(&AlignmentDirection::UR));
        assert!(all.contains(&AlignmentDirection::DL));
        assert!(all.contains(&AlignmentDirection::DR));
    }

    #[test]
    fn test_compaction_result_default() {
        let result = CompactionResult::default();
        assert!(result.x.is_empty());
        assert!(result.sink.is_empty());
        assert!(result.shift.is_empty());
    }

    #[test]
    fn test_bk_config_default() {
        let config = BKConfig::default();
        assert_eq!(config.node_sep, 50.0);
        assert_eq!(config.direction, Direction::TopBottom);
    }

    #[test]
    fn test_conflict_equality() {
        let c1 = Conflict {
            layer: 1,
            pos1: 0,
            pos2: 2,
        };
        let c2 = Conflict {
            layer: 1,
            pos1: 0,
            pos2: 2,
        };
        let c3 = Conflict {
            layer: 1,
            pos1: 0,
            pos2: 3,
        };

        assert_eq!(c1, c2);
        assert_ne!(c1, c3);
    }

    // =========================================================================
    // Helper Function Tests
    // =========================================================================

    #[test]
    fn test_get_layers() {
        let lg = make_diamond_graph();
        let layers = get_layers(&lg);

        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].len(), 1); // A
        assert_eq!(layers[1].len(), 2); // B, C
        assert_eq!(layers[2].len(), 1); // D

        // Check that layer 1 is sorted by order (B before C)
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];
        assert_eq!(layers[1][0], b);
        assert_eq!(layers[1][1], c);
    }

    #[test]
    fn test_get_layers_in_order_downward() {
        let order = get_layers_in_order(3, true);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn test_get_layers_in_order_upward() {
        let order = get_layers_in_order(3, false);
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn test_get_predecessors() {
        let lg = make_diamond_graph();
        let d = lg.node_index[&"D".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        let preds = get_predecessors(&lg, d);
        // D has predecessors B and C, sorted by order (B=0, C=1)
        assert_eq!(preds.len(), 2);
        assert_eq!(preds[0], b);
        assert_eq!(preds[1], c);
    }

    #[test]
    fn test_get_successors() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        let succs = get_successors(&lg, a);
        // A has successors B and C, sorted by order (B=0, C=1)
        assert_eq!(succs.len(), 2);
        assert_eq!(succs[0], b);
        assert_eq!(succs[1], c);
    }

    #[test]
    fn test_get_neighbors_downward() {
        let lg = make_diamond_graph();
        let d = lg.node_index[&"D".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        // Downward sweep: use predecessors
        let neighbors = get_neighbors(&lg, d, true);
        assert_eq!(neighbors.len(), 2);
        assert_eq!(neighbors[0], b);
        assert_eq!(neighbors[1], c);
    }

    #[test]
    fn test_get_neighbors_upward() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        // Upward sweep: use successors
        let neighbors = get_neighbors(&lg, a, false);
        assert_eq!(neighbors.len(), 2);
        assert_eq!(neighbors[0], b);
        assert_eq!(neighbors[1], c);
    }

    #[test]
    fn test_get_median_neighbor_single() {
        let lg = make_diamond_graph();
        let b = lg.node_index[&"B".into()];
        let d = lg.node_index[&"D".into()];

        // B has single successor D
        let median = get_median_neighbor(&lg, b, false, true);
        assert_eq!(median, Some(d));
    }

    #[test]
    fn test_get_median_neighbor_even_prefer_left() {
        let lg = make_diamond_graph();
        let d = lg.node_index[&"D".into()];
        let b = lg.node_index[&"B".into()];

        // D has two predecessors [B, C], prefer_left=true should return B
        let median = get_median_neighbor(&lg, d, true, true);
        assert_eq!(median, Some(b));
    }

    #[test]
    fn test_get_median_neighbor_even_prefer_right() {
        let lg = make_diamond_graph();
        let d = lg.node_index[&"D".into()];
        let c = lg.node_index[&"C".into()];

        // D has two predecessors [B, C], prefer_left=false should return C
        let median = get_median_neighbor(&lg, d, true, false);
        assert_eq!(median, Some(c));
    }

    #[test]
    fn test_get_median_neighbor_none() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];

        // A has no predecessors
        let median = get_median_neighbor(&lg, a, true, true);
        assert_eq!(median, None);
    }

    #[test]
    fn test_get_position() {
        let lg = make_diamond_graph();
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        assert_eq!(get_position(&lg, b), 0);
        assert_eq!(get_position(&lg, c), 1);
    }

    #[test]
    fn test_get_layer() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];
        let b = lg.node_index[&"B".into()];
        let d = lg.node_index[&"D".into()];

        assert_eq!(get_layer(&lg, a), 0);
        assert_eq!(get_layer(&lg, b), 1);
        assert_eq!(get_layer(&lg, d), 2);
    }

    #[test]
    fn test_get_width() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];

        assert_eq!(get_width(&lg, a), 100.0);
    }

    #[test]
    fn test_is_dummy() {
        let lg = make_diamond_graph();
        let a = lg.node_index[&"A".into()];

        // Real nodes are not dummies
        assert!(!is_dummy(&lg, a));
    }

    // =========================================================================
    // Conflict Detection Tests
    // =========================================================================

    #[test]
    fn test_segments_cross_yes() {
        // Segment 1: upper=0, lower=2
        // Segment 2: upper=1, lower=0
        // These cross (one goes right, the other goes left)
        assert!(segments_cross(0, 2, 1, 0));
    }

    #[test]
    fn test_segments_cross_yes_reverse() {
        // Segment 1: upper=1, lower=0
        // Segment 2: upper=0, lower=2
        // These cross (one goes left, the other goes right)
        assert!(segments_cross(1, 0, 0, 2));
    }

    #[test]
    fn test_segments_cross_no_parallel() {
        // Segment 1: upper=0, lower=0
        // Segment 2: upper=1, lower=1
        // These don't cross (parallel/straight down)
        assert!(!segments_cross(0, 0, 1, 1));
    }

    #[test]
    fn test_segments_cross_no_diverging() {
        // Segment 1: upper=0, lower=0
        // Segment 2: upper=1, lower=2
        // These don't cross (diverging)
        assert!(!segments_cross(0, 0, 1, 2));
    }

    #[test]
    fn test_segments_cross_same_start() {
        // Segment 1: upper=0, lower=1
        // Segment 2: upper=0, lower=2
        // Same start, don't cross
        assert!(!segments_cross(0, 1, 0, 2));
    }

    #[test]
    fn test_segments_cross_same_end() {
        // Segment 1: upper=0, lower=2
        // Segment 2: upper=1, lower=2
        // Same end, don't cross
        assert!(!segments_cross(0, 2, 1, 2));
    }

    #[test]
    fn test_has_conflict_basic() {
        let mut conflicts = ConflictSet::new();
        conflicts.insert(
            (1, 0, 2),
            Conflict {
                layer: 1,
                pos1: 0,
                pos2: 2,
            },
        );

        // Alignment that spans the conflict range should be blocked
        assert!(has_conflict(&conflicts, 1, 0, 3));

        // Alignment in different layer should not be blocked
        assert!(!has_conflict(&conflicts, 0, 0, 3));

        // Alignment that doesn't span the conflict should not be blocked
        assert!(!has_conflict(&conflicts, 1, 3, 4));
    }

    #[test]
    fn test_find_inner_segments_no_dummies() {
        let lg = make_diamond_graph();

        // Diamond graph has no dummy nodes, so no inner segments
        let segments = find_inner_segments(&lg, 0, 1);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_find_type1_conflicts_no_dummies() {
        let lg = make_diamond_graph();

        // No dummy nodes means no inner segments, so no Type-1 conflicts
        let conflicts = find_type1_conflicts(&lg);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_find_type2_conflicts_no_dummies() {
        let lg = make_diamond_graph();

        // No dummy nodes means no inner segments, so no Type-2 conflicts
        let conflicts = find_type2_conflicts(&lg);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_find_all_conflicts_no_dummies() {
        let lg = make_diamond_graph();

        // No dummy nodes means no conflicts
        let conflicts = find_all_conflicts(&lg);
        assert!(conflicts.is_empty());
    }
}
