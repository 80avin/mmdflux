//! Graph representation for layout computation.

use std::collections::{HashMap, HashSet};

use petgraph::stable_graph::StableDiGraph;

use super::normalize::{DummyChain, DummyNode};
use super::types::{NodeId, Point};

/// A directed graph for layout.
///
/// Generic over node data `N` which can store application-specific info.
#[derive(Debug, Clone)]
pub struct DiGraph<N> {
    nodes: Vec<(NodeId, N)>,
    edges: Vec<(NodeId, NodeId)>,
    node_index: HashMap<NodeId, usize>,
}

impl<N> Default for DiGraph<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> DiGraph<N> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_index: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, id: impl Into<NodeId>, data: N) {
        let id = id.into();
        if self.node_index.contains_key(&id) {
            return; // Node already exists
        }
        let index = self.nodes.len();
        self.node_index.insert(id.clone(), index);
        self.nodes.push((id, data));
    }

    pub fn add_edge(&mut self, from: impl Into<NodeId>, to: impl Into<NodeId>) {
        let from = from.into();
        let to = to.into();
        self.edges.push((from, to));
    }

    pub fn node_ids(&self) -> impl Iterator<Item = &NodeId> {
        self.nodes.iter().map(|(id, _)| id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = (&NodeId, &N)> {
        self.nodes.iter().map(|(id, data)| (id, data))
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn edges(&self) -> &[(NodeId, NodeId)] {
        &self.edges
    }

    pub fn get_node(&self, id: &NodeId) -> Option<&N> {
        self.node_index.get(id).map(|&idx| &self.nodes[idx].1)
    }

    pub fn successors(&self, id: &NodeId) -> Vec<&NodeId> {
        self.edges
            .iter()
            .filter(|(from, _)| from == id)
            .map(|(_, to)| to)
            .collect()
    }

    pub fn predecessors(&self, id: &NodeId) -> Vec<&NodeId> {
        self.edges
            .iter()
            .filter(|(_, to)| to == id)
            .map(|(from, _)| from)
            .collect()
    }

    pub fn in_degree(&self, id: &NodeId) -> usize {
        self.edges.iter().filter(|(_, to)| to == id).count()
    }

    pub fn out_degree(&self, id: &NodeId) -> usize {
        self.edges.iter().filter(|(from, _)| from == id).count()
    }
}

/// Internal graph representation with additional layout metadata.
#[derive(Debug)]
pub(crate) struct LayoutGraph {
    /// Node IDs in the graph (in insertion order).
    pub node_ids: Vec<NodeId>,

    /// Edges as (from_index, to_index, original_edge_index).
    pub edges: Vec<(usize, usize, usize)>,

    /// Node index lookup.
    #[allow(dead_code)] // Used in tests
    pub node_index: HashMap<NodeId, usize>,

    /// Reversed edges (for cycle removal).
    pub reversed_edges: HashSet<usize>,

    /// Rank (layer) assigned to each node.
    pub ranks: Vec<i32>,

    /// Order within rank for each node.
    pub order: Vec<usize>,

    /// Final positions.
    pub positions: Vec<Point>,

    /// Node dimensions (width, height).
    pub dimensions: Vec<(f64, f64)>,

    // --- Dummy node tracking (for normalization) ---
    /// Metadata for dummy nodes, keyed by node ID.
    pub dummy_nodes: HashMap<NodeId, DummyNode>,

    /// Chains of dummy nodes for each normalized long edge.
    pub dummy_chains: Vec<DummyChain>,

    /// Number of original edges before normalization.
    /// Used to distinguish original edges from edges created during normalization.
    #[allow(dead_code)]
    pub original_edge_count: usize,
}

impl LayoutGraph {
    /// Create a LayoutGraph from a DiGraph.
    pub fn from_digraph<N, F>(graph: &DiGraph<N>, get_dimensions: F) -> Self
    where
        F: Fn(&NodeId, &N) -> (f64, f64),
    {
        let node_ids: Vec<_> = graph.node_ids().cloned().collect();
        let node_index: HashMap<_, _> = node_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();

        let edges: Vec<_> = graph
            .edges()
            .iter()
            .enumerate()
            .filter_map(|(i, (from, to))| {
                let from_idx = node_index.get(from)?;
                let to_idx = node_index.get(to)?;
                Some((*from_idx, *to_idx, i))
            })
            .collect();

        let dimensions: Vec<_> = graph
            .nodes()
            .map(|(id, data)| get_dimensions(id, data))
            .collect();

        let n = node_ids.len();
        let edge_count = edges.len();

        Self {
            node_ids,
            edges,
            node_index,
            reversed_edges: HashSet::new(),
            ranks: vec![0; n],
            order: (0..n).collect(),
            positions: vec![Point::default(); n],
            dimensions,
            dummy_nodes: HashMap::new(),
            dummy_chains: Vec::new(),
            original_edge_count: edge_count,
        }
    }

    /// Convert to petgraph StableDiGraph for algorithm use.
    #[allow(dead_code)] // May be used for future algorithm improvements
    pub fn to_petgraph(&self) -> StableDiGraph<usize, usize> {
        let mut pg = StableDiGraph::new();

        // Add nodes (using index as weight)
        let node_indices: Vec<_> = (0..self.node_ids.len()).map(|i| pg.add_node(i)).collect();

        // Add edges (using edge index as weight), respecting reversals
        for (edge_idx, &(from, to, _)) in self.edges.iter().enumerate() {
            if self.reversed_edges.contains(&edge_idx) {
                pg.add_edge(node_indices[to], node_indices[from], edge_idx);
            } else {
                pg.add_edge(node_indices[from], node_indices[to], edge_idx);
            }
        }

        pg
    }

    /// Get effective edges (with reversals applied).
    pub fn effective_edges(&self) -> Vec<(usize, usize)> {
        self.edges
            .iter()
            .enumerate()
            .map(|(idx, &(from, to, _))| {
                if self.reversed_edges.contains(&idx) {
                    (to, from)
                } else {
                    (from, to)
                }
            })
            .collect()
    }

    /// Check if a node is a dummy node.
    pub fn is_dummy(&self, node_id: &NodeId) -> bool {
        self.dummy_nodes.contains_key(node_id)
    }

    /// Get dummy node metadata if the node is a dummy.
    #[allow(dead_code)]
    pub fn get_dummy(&self, node_id: &NodeId) -> Option<&DummyNode> {
        self.dummy_nodes.get(node_id)
    }

    /// Check if a node index corresponds to a dummy node.
    pub fn is_dummy_index(&self, idx: usize) -> bool {
        self.node_ids.get(idx).is_some_and(|id| self.is_dummy(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digraph_basic_operations() {
        let mut graph: DiGraph<u32> = DiGraph::new();
        graph.add_node("A", 10);
        graph.add_node("B", 20);
        graph.add_node("C", 30);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.get_node(&"A".into()), Some(&10));
        assert_eq!(graph.get_node(&"D".into()), None);
    }

    #[test]
    fn test_digraph_edges() {
        let mut graph: DiGraph<u32> = DiGraph::new();
        graph.add_node("A", 1);
        graph.add_node("B", 2);
        graph.add_node("C", 3);
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");
        graph.add_edge("B", "C");

        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.out_degree(&"A".into()), 2);
        assert_eq!(graph.out_degree(&"B".into()), 1);
        assert_eq!(graph.out_degree(&"C".into()), 0);
        assert_eq!(graph.in_degree(&"A".into()), 0);
        assert_eq!(graph.in_degree(&"B".into()), 1);
        assert_eq!(graph.in_degree(&"C".into()), 2);
    }

    #[test]
    fn test_digraph_successors_predecessors() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");

        let succs: Vec<_> = graph.successors(&"A".into());
        assert_eq!(succs.len(), 2);

        let preds: Vec<_> = graph.predecessors(&"B".into());
        assert_eq!(preds.len(), 1);
        assert_eq!(preds[0], &NodeId::from("A"));
    }

    #[test]
    fn test_digraph_duplicate_node() {
        let mut graph: DiGraph<u32> = DiGraph::new();
        graph.add_node("A", 10);
        graph.add_node("A", 20); // Should be ignored

        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.get_node(&"A".into()), Some(&10));
    }

    #[test]
    fn test_layout_graph_from_digraph() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        let lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);

        assert_eq!(lg.node_ids.len(), 2);
        assert_eq!(lg.edges.len(), 1);
        assert_eq!(lg.dimensions[0], (100.0, 50.0));
    }

    #[test]
    fn test_layout_graph_to_petgraph() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");

        let lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        let pg = lg.to_petgraph();

        assert_eq!(pg.node_count(), 3);
        assert_eq!(pg.edge_count(), 2);
    }

    #[test]
    fn test_layout_graph_dummy_tracking() {
        use crate::dagre::normalize::{DummyNode, LabelPos};

        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);

        // Initially no dummies
        assert!(!lg.is_dummy(&"A".into()));
        assert!(!lg.is_dummy(&"B".into()));
        assert!(lg.dummy_chains.is_empty());
        assert_eq!(lg.original_edge_count, 1);

        // Add a dummy node
        let dummy_id = NodeId::from("_d0");
        lg.dummy_nodes.insert(
            dummy_id.clone(),
            DummyNode::edge_label(0, 1, 10.0, 5.0, LabelPos::Center),
        );

        // Now the dummy should be detected
        assert!(lg.is_dummy(&dummy_id));
        assert!(!lg.is_dummy(&"A".into()));

        // get_dummy should return the metadata
        let dummy = lg.get_dummy(&dummy_id).unwrap();
        assert!(dummy.is_label());
        assert_eq!(dummy.edge_index, 0);
        assert_eq!(dummy.rank, 1);
    }

    #[test]
    fn test_layout_graph_is_dummy_index() {
        use crate::dagre::normalize::DummyNode;

        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        graph.add_node("B", (100.0, 50.0));
        graph.add_edge("A", "B");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);

        // No dummies initially
        assert!(!lg.is_dummy_index(0)); // A
        assert!(!lg.is_dummy_index(1)); // B
        assert!(!lg.is_dummy_index(99)); // Out of bounds

        // Add a dummy (simulating normalization adding a node)
        let dummy_id = NodeId::from("_d0");
        lg.node_ids.push(dummy_id.clone());
        lg.dummy_nodes.insert(dummy_id, DummyNode::edge(0, 1));

        // Now index 2 is a dummy
        assert!(lg.is_dummy_index(2));
        assert!(!lg.is_dummy_index(0));
    }
}
