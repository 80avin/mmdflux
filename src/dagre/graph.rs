//! Graph representation for layout computation.

use std::collections::{BTreeSet, HashMap};

use petgraph::stable_graph::StableDiGraph;

use super::normalize::{DummyChain, DummyNode};
use super::types::{NodeId, Point, SelfEdge};

/// A directed graph for layout.
///
/// Generic over node data `N` which can store application-specific info.
#[derive(Debug, Clone)]
pub struct DiGraph<N> {
    nodes: Vec<(NodeId, N)>,
    edges: Vec<(NodeId, NodeId)>,
    node_index: HashMap<NodeId, usize>,
    parents: HashMap<NodeId, NodeId>,
    /// Node IDs of compounds with non-empty titles.
    nodes_with_title: BTreeSet<NodeId>,
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
            parents: HashMap::new(),
            nodes_with_title: BTreeSet::new(),
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

    pub fn set_parent(&mut self, node: impl Into<NodeId>, parent: impl Into<NodeId>) {
        self.parents.insert(node.into(), parent.into());
    }

    pub fn parent(&self, node: &NodeId) -> Option<&NodeId> {
        self.parents.get(node)
    }

    pub fn children(&self, parent: &NodeId) -> Vec<&NodeId> {
        self.parents
            .iter()
            .filter(|(_, p)| *p == parent)
            .map(|(n, _)| n)
            .collect()
    }

    pub fn has_compound_nodes(&self) -> bool {
        !self.parents.is_empty()
    }

    pub fn parents_map(&self) -> &HashMap<NodeId, NodeId> {
        &self.parents
    }

    pub fn set_has_title(&mut self, node: impl Into<NodeId>) {
        self.nodes_with_title.insert(node.into());
    }

    pub fn has_title(&self, node: impl Into<NodeId>) -> bool {
        self.nodes_with_title.contains(&node.into())
    }
}

/// Border node type (left or right border of a compound node).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BorderType {
    Left,
    Right,
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
    pub reversed_edges: BTreeSet<usize>,

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

    /// Edge weights, indexed by position in `edges` vec. Default 1.0 for all edges.
    pub edge_weights: Vec<f64>,

    // --- Compound graph fields ---
    /// Parent node index for each node (None if no parent).
    pub parents: Vec<Option<usize>>,

    /// Minimum rank for compound nodes.
    pub min_rank: HashMap<usize, i32>,

    /// Maximum rank for compound nodes.
    pub max_rank: HashMap<usize, i32>,

    /// Top border node index for compound nodes.
    pub border_top: HashMap<usize, usize>,

    /// Bottom border node index for compound nodes.
    pub border_bottom: HashMap<usize, usize>,

    /// Title border node index for compound nodes with titles.
    pub border_title: HashMap<usize, usize>,

    /// Left border node indices per rank for compound nodes.
    pub border_left: HashMap<usize, Vec<usize>>,

    /// Right border node indices per rank for compound nodes.
    pub border_right: HashMap<usize, Vec<usize>>,

    /// Border type for border nodes.
    pub border_type: HashMap<usize, BorderType>,

    /// Root node index for the nesting tree.
    pub nesting_root: Option<usize>,

    /// Edge indices that are nesting edges (to be removed after ranking).
    pub nesting_edges: BTreeSet<usize>,

    /// Edge indices excluded from effective_edges (nesting edges after cleanup).
    /// These edges exist in the `edges` vec but are ignored by downstream phases
    /// (normalization, ordering, BK alignment).
    pub excluded_edges: BTreeSet<usize>,

    /// Nodes excluded from positioning (dagre `asNonCompoundGraph` semantics).
    pub position_excluded_nodes: BTreeSet<usize>,

    /// Node indices that are compound (subgraph) nodes.
    pub compound_nodes: BTreeSet<usize>,

    /// Compound node indices that have non-empty titles.
    pub compound_titles: BTreeSet<usize>,

    /// Minimum rank span for each edge. Default 1 for all edges.
    pub edge_minlens: Vec<i32>,

    /// Self-edges extracted before the acyclic phase and reinserted after ordering.
    pub self_edges: Vec<SelfEdge>,

    /// Node rank factor from nesting graph (used by `remove_empty_ranks`).
    /// Set when nesting multiplies edge minlens by this factor.
    pub node_rank_factor: Option<i32>,
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

        let edge_weights = vec![1.0; edge_count];

        // Build compound_titles set
        let compound_titles: BTreeSet<usize> = graph
            .nodes_with_title
            .iter()
            .filter_map(|id| node_index.get(id).copied())
            .collect();

        // Build parent index mapping and compound node set
        let mut parents = vec![None; n];
        let mut compound_nodes = BTreeSet::new();
        for (child_id, parent_id) in graph.parents_map() {
            if let (Some(&child_idx), Some(&parent_idx)) =
                (node_index.get(child_id), node_index.get(parent_id))
            {
                parents[child_idx] = Some(parent_idx);
                compound_nodes.insert(parent_idx);
            }
        }

        Self {
            node_ids,
            edges,
            node_index,
            reversed_edges: BTreeSet::new(),
            ranks: vec![0; n],
            order: (0..n).collect(),
            positions: vec![Point::default(); n],
            dimensions,
            dummy_nodes: HashMap::new(),
            dummy_chains: Vec::new(),
            original_edge_count: edge_count,
            edge_weights,
            parents,
            min_rank: HashMap::new(),
            max_rank: HashMap::new(),
            border_top: HashMap::new(),
            border_bottom: HashMap::new(),
            border_title: HashMap::new(),
            border_left: HashMap::new(),
            border_right: HashMap::new(),
            border_type: HashMap::new(),
            nesting_root: None,
            nesting_edges: BTreeSet::new(),
            excluded_edges: BTreeSet::new(),
            position_excluded_nodes: BTreeSet::new(),
            compound_nodes,
            compound_titles,
            edge_minlens: vec![1; edge_count],
            self_edges: Vec::new(),
            node_rank_factor: None,
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

    /// Get the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.node_ids.len()
    }

    /// Add a nesting dummy node with zero dimensions. Returns the node index.
    pub fn add_nesting_node(&mut self, id: NodeId) -> usize {
        let idx = self.node_ids.len();
        self.node_index.insert(id.clone(), idx);
        self.node_ids.push(id);
        self.ranks.push(0);
        self.order.push(idx);
        self.positions.push(Point::default());
        self.dimensions.push((0.0, 0.0));
        self.parents.push(None);
        self.position_excluded_nodes.remove(&idx);
        idx
    }

    /// Returns true if a node should participate in positioning.
    ///
    /// This excludes compound parents and any nodes explicitly excluded via
    /// `position_excluded_nodes` (dagre `asNonCompoundGraph` semantics).
    pub fn is_position_node(&self, node: usize) -> bool {
        if self.compound_nodes.contains(&node) {
            return false;
        }
        if self.position_excluded_nodes.contains(&node) {
            return false;
        }
        true
    }

    /// Add an edge and return its index.
    pub fn add_nesting_edge(&mut self, from: usize, to: usize, weight: f64) -> usize {
        self.add_nesting_edge_with_minlen(from, to, weight, 1)
    }

    /// Add an edge with a specific minlen and return its index.
    pub fn add_nesting_edge_with_minlen(
        &mut self,
        from: usize,
        to: usize,
        weight: f64,
        minlen: i32,
    ) -> usize {
        let idx = self.edges.len();
        self.edges.push((from, to, idx));
        self.edge_weights.push(weight);
        self.edge_minlens.push(minlen);
        idx
    }

    /// Check if an edge exists between two node indices.
    #[cfg(test)]
    pub fn has_edge(&self, from: usize, to: usize) -> bool {
        self.edges.iter().any(|&(f, t, _)| f == from && t == to)
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
    fn test_add_nesting_node_does_not_shift_edge_weights() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_edge("A", "B");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        let edges_before = lg.edges.len();
        let weights_before = lg.edge_weights.len();

        lg.add_nesting_node("_nesting_root".into());

        assert_eq!(lg.edges.len(), edges_before);
        assert_eq!(lg.edge_weights.len(), weights_before);
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
    fn test_digraph_set_parent() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("sg1", ());
        g.set_parent("A", "sg1");
        assert_eq!(g.parent(&"A".into()), Some(&"sg1".into()));
    }

    #[test]
    fn test_digraph_children() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("sg1", ());
        g.set_parent("A", "sg1");
        g.set_parent("B", "sg1");
        let children = g.children(&"sg1".into());
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_digraph_has_compound_nodes_false() {
        let g: DiGraph<()> = DiGraph::new();
        assert!(!g.has_compound_nodes());
    }

    #[test]
    fn test_digraph_has_compound_nodes_true() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("sg1", ());
        g.set_parent("A", "sg1");
        assert!(g.has_compound_nodes());
    }

    #[test]
    fn test_layout_graph_compound_fields_propagated() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("B", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.add_edge("A", "B");
        g.set_parent("A", "sg1");
        g.set_parent("B", "sg1");

        let lg = LayoutGraph::from_digraph(&g, |_, dims| *dims);
        let sg1_idx = lg.node_index[&"sg1".into()];
        let a_idx = lg.node_index[&"A".into()];
        let b_idx = lg.node_index[&"B".into()];

        assert_eq!(lg.parents[a_idx], Some(sg1_idx));
        assert_eq!(lg.parents[b_idx], Some(sg1_idx));
        assert_eq!(lg.parents[sg1_idx], None);
        assert!(lg.compound_nodes.contains(&sg1_idx));
    }

    #[test]
    fn test_is_position_node_excludes_compound_parents_and_root() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("sg", ());
        g.add_node("A", ());
        g.set_parent("A", "sg");

        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        let sg_idx = lg.node_index[&"sg".into()];
        let a_idx = lg.node_index[&"A".into()];

        let root_idx = lg.add_nesting_node("_nesting_root".into());
        lg.position_excluded_nodes.insert(root_idx);

        assert!(
            !lg.is_position_node(sg_idx),
            "compound parent should be excluded"
        );
        assert!(lg.is_position_node(a_idx), "leaf node should be included");
        assert!(
            !lg.is_position_node(root_idx),
            "nesting root should be excluded"
        );
    }

    #[test]
    fn test_digraph_set_has_title() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("sg1", (0.0, 0.0));
        g.set_has_title("sg1");
        assert!(g.has_title("sg1"));
    }

    #[test]
    fn test_compound_titles_propagated_to_layout_graph() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.set_parent("A", "sg1");
        g.set_has_title("sg1");
        let lg = LayoutGraph::from_digraph(&g, |_, dims| *dims);
        let sg1_idx = lg.node_index[&"sg1".into()];
        assert!(lg.compound_titles.contains(&sg1_idx));
    }

    #[test]
    fn test_layout_graph_self_edges_default_empty() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (100.0, 50.0));
        let lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        assert!(lg.self_edges.is_empty());
    }

    #[test]
    fn test_untitled_compound_not_in_compound_titles() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.set_parent("A", "sg1");
        let lg = LayoutGraph::from_digraph(&g, |_, dims| *dims);
        let sg1_idx = lg.node_index[&"sg1".into()];
        assert!(!lg.compound_titles.contains(&sg1_idx));
    }

    #[test]
    fn test_layout_graph_border_title_initially_empty() {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        let lg = LayoutGraph::from_digraph(&g, |_, dims| *dims);
        assert!(lg.border_title.is_empty());
    }

    #[test]
    fn test_layout_graph_has_edge_minlens() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (5.0, 3.0));
        graph.add_node("B", (5.0, 3.0));
        graph.add_edge("A", "B");

        let lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        // Every edge should have minlen = 1 by default
        assert_eq!(lg.edge_minlens.len(), 1);
        assert_eq!(lg.edge_minlens[0], 1);
    }

    #[test]
    fn test_layout_graph_minlen_can_be_set() {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        graph.add_node("A", (5.0, 3.0));
        graph.add_node("B", (5.0, 3.0));
        graph.add_edge("A", "B");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        lg.edge_minlens[0] = 2;
        assert_eq!(lg.edge_minlens[0], 2);
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
