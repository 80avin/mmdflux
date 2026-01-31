//! Network simplex ranking algorithm.
//!
//! Implements optimal rank assignment minimizing total weighted edge length.
//! Reference: Gansner et al., "A Technique for Drawing Directed Graphs"
//! Dagre.js: lib/rank/network-simplex.js, lib/rank/feasible-tree.js

use super::graph::LayoutGraph;

/// Compute slack for edge at `edge_idx`: rank(target) - rank(source) - minlen.
/// A tight edge has slack = 0.
pub(crate) fn slack(graph: &LayoutGraph, edge_idx: usize) -> i32 {
    let edges = graph.effective_edges();
    let (from, to) = edges[edge_idx];
    graph.ranks[to] - graph.ranks[from] - graph.edge_minlens[edge_idx]
}

/// A spanning tree for network simplex.
pub(crate) struct SpanningTree {
    /// Parent of each node in the tree (None for root).
    pub parent: Vec<Option<usize>>,
    /// Edge index connecting node to its parent (None for root).
    pub parent_edge: Vec<Option<usize>>,
    /// Set of nodes currently in the tree.
    pub in_tree: Vec<bool>,
    /// Number of nodes in the tree.
    size: usize,
    /// Low value for DFS numbering (populated in Phase 4).
    pub low: Vec<i32>,
    /// Lim value for DFS numbering (populated in Phase 4).
    pub lim: Vec<i32>,
    /// Cut value for tree edges, indexed by child node (populated in Phase 5).
    pub cut_value: Vec<f64>,
}

impl SpanningTree {
    fn new(n: usize) -> Self {
        SpanningTree {
            parent: vec![None; n],
            parent_edge: vec![None; n],
            in_tree: vec![false; n],
            size: 0,
            low: vec![0; n],
            lim: vec![0; n],
            cut_value: vec![0.0; n],
        }
    }

    fn add_node(&mut self, node: usize) {
        if !self.in_tree[node] {
            self.in_tree[node] = true;
            self.size += 1;
        }
    }

    fn add_edge(&mut self, parent: usize, child: usize, edge_idx: usize) {
        self.add_node(child);
        self.parent[child] = Some(parent);
        self.parent_edge[child] = Some(edge_idx);
    }

    pub fn node_count(&self) -> usize {
        self.in_tree.len()
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

/// Build adjacency lists for each node: (neighbor, edge_index) in both directions.
fn build_adjacency(graph: &LayoutGraph) -> Vec<Vec<(usize, usize)>> {
    let n = graph.node_count();
    let edges = graph.effective_edges();
    let mut adj = vec![Vec::new(); n];
    for (edge_idx, &(from, to)) in edges.iter().enumerate() {
        adj[from].push((to, edge_idx));
        adj[to].push((from, edge_idx));
    }
    adj
}

/// DFS from all tree nodes, greedily adding neighbors connected by tight edges.
/// Returns the number of nodes in the tree after this pass.
fn tight_tree(tree: &mut SpanningTree, graph: &LayoutGraph, adj: &[Vec<(usize, usize)>]) -> usize {
    // DFS from each current tree node to find tight edges to non-tree nodes.
    // We iterate tree nodes via a stack to avoid borrowing issues.
    let tree_nodes: Vec<usize> = (0..tree.node_count())
        .filter(|&n| tree.in_tree[n])
        .collect();

    let mut stack: Vec<usize> = tree_nodes;
    while let Some(v) = stack.pop() {
        for &(w, edge_idx) in &adj[v] {
            if !tree.in_tree[w] && slack(graph, edge_idx) == 0 {
                tree.add_edge(v, w, edge_idx);
                stack.push(w);
            }
        }
    }

    tree.size()
}

/// Find the edge with minimum absolute slack that crosses the tree boundary
/// (one endpoint in tree, one outside). Returns (edge_idx, delta) where delta
/// is the value to add to all tree node ranks to make this edge tight.
fn find_min_slack_crossing(tree: &SpanningTree, graph: &LayoutGraph) -> (usize, i32) {
    let edges = graph.effective_edges();
    let mut best_edge = 0;
    let mut best_slack = i32::MAX;

    for (edge_idx, &(from, to)) in edges.iter().enumerate() {
        let from_in = tree.in_tree[from];
        let to_in = tree.in_tree[to];
        if from_in == to_in {
            continue; // both in or both out
        }
        let s = slack(graph, edge_idx).abs();
        if s < best_slack {
            best_slack = s;
            best_edge = edge_idx;
        }
    }

    // Compute delta: we need rank(to) - rank(from) - minlen = 0
    // If from is in tree: shift tree ranks by +slack (make edge tight)
    // If to is in tree: shift tree ranks by -slack
    let (from, _to) = edges[best_edge];
    let raw_slack = slack(graph, best_edge);
    let delta = if tree.in_tree[from] {
        raw_slack
    } else {
        -raw_slack
    };

    (best_edge, delta)
}

/// Shift all tree node ranks by delta.
fn shift_ranks(tree: &SpanningTree, graph: &mut LayoutGraph, delta: i32) {
    for (node, &in_tree) in tree.in_tree.iter().enumerate() {
        if in_tree {
            graph.ranks[node] += delta;
        }
    }
}

/// Construct a feasible spanning tree of tight edges.
/// Modifies graph ranks to ensure the tree spans all nodes.
pub(crate) fn feasible_tree(graph: &mut LayoutGraph) -> SpanningTree {
    let n = graph.node_count();
    let mut tree = SpanningTree::new(n);
    let adj = build_adjacency(graph);

    // Start from node 0
    tree.add_node(0);

    loop {
        let size = tight_tree(&mut tree, graph, &adj);
        if size >= n {
            break;
        }
        // Find min-slack edge crossing tree boundary and shift ranks
        let (_edge_idx, delta) = find_min_slack_crossing(&tree, graph);
        shift_ranks(&tree, graph, delta);
    }

    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::{DiGraph, LayoutGraph};

    fn make_chain_graph() -> LayoutGraph {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_edge("A", "B");
        g.add_edge("B", "C");
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        // Manually set ranks: A=0, B=1, C=3
        lg.ranks = vec![0, 1, 3];
        lg
    }

    fn make_ranked_chain() -> LayoutGraph {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_edge("A", "B");
        g.add_edge("B", "C");
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1, 2]; // all tight
        lg
    }

    fn make_ranked_diamond() -> LayoutGraph {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_node("D", ());
        g.add_edge("A", "B");
        g.add_edge("A", "C");
        g.add_edge("B", "D");
        g.add_edge("C", "D");
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1, 1, 2]; // A=0, B=1, C=1, D=2
        lg
    }

    #[test]
    fn test_slack_tight_edge() {
        let lg = make_chain_graph();
        assert_eq!(slack(&lg, 0), 0);
    }

    #[test]
    fn test_slack_non_tight_edge() {
        let lg = make_chain_graph();
        assert_eq!(slack(&lg, 1), 1);
    }

    #[test]
    fn test_slack_with_custom_minlen() {
        let mut lg = make_chain_graph();
        lg.edge_minlens[0] = 2;
        assert_eq!(slack(&lg, 0), -1);
    }

    #[test]
    fn test_feasible_tree_linear_chain() {
        let mut lg = make_ranked_chain();
        let tree = feasible_tree(&mut lg);
        assert_eq!(tree.node_count(), 3);
        for node in 0..3 {
            if let Some(edge_idx) = tree.parent_edge[node] {
                assert_eq!(slack(&lg, edge_idx), 0);
            }
        }
    }

    #[test]
    fn test_feasible_tree_needs_rank_shift() {
        // A->B->D, C->D. Ranks: A=0, B=1, C=0, D=2
        // C->D has slack=1, needs shifting
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_node("D", ());
        g.add_edge("A", "B");
        g.add_edge("B", "D");
        g.add_edge("C", "D");
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1, 0, 2]; // A=0, B=1, C=0, D=2

        let tree = feasible_tree(&mut lg);
        assert_eq!(tree.size(), 4);
        // All tree edges should be tight
        for node in 0..4 {
            if let Some(edge_idx) = tree.parent_edge[node] {
                assert_eq!(
                    slack(&lg, edge_idx),
                    0,
                    "edge {} has non-zero slack after feasible_tree",
                    edge_idx
                );
            }
        }
    }

    #[test]
    fn test_feasible_tree_spans_all_nodes() {
        let mut lg = make_ranked_diamond();
        let tree = feasible_tree(&mut lg);
        let tree_edge_count = tree.parent_edge.iter().filter(|e| e.is_some()).count();
        assert_eq!(tree_edge_count, 3); // n-1 = 4-1 = 3
    }
}
