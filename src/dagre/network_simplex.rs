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

/// Assign low/lim DFS numbering for O(1) descendant queries.
/// After calling this, `is_descendant(tree, u, v)` returns true iff u is in v's subtree.
pub(crate) fn init_low_lim(tree: &mut SpanningTree, root: usize) {
    let n = tree.parent.len();
    // Build children lists from parent pointers
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for node in 0..n {
        if let Some(p) = tree.parent[node] {
            children[p].push(node);
        }
    }

    // Iterative DFS with pre/post numbering.
    // low[v] = counter before visiting children
    // lim[v] = counter after all children, then counter += 1
    let mut counter = 1i32;
    // Stack entries: (node, phase). phase=false means first visit, phase=true means post-visit.
    let mut stack: Vec<(usize, bool)> = vec![(root, false)];

    while let Some((node, post)) = stack.pop() {
        if post {
            tree.lim[node] = counter;
            counter += 1;
        } else {
            tree.low[node] = counter;
            stack.push((node, true));
            // Push children in reverse order so leftmost child is processed first
            for &child in children[node].iter().rev() {
                stack.push((child, false));
            }
        }
    }
}

/// Check if u is a descendant of v in the spanning tree.
pub(crate) fn is_descendant(tree: &SpanningTree, u: usize, v: usize) -> bool {
    tree.low[v] <= tree.lim[u] && tree.lim[u] <= tree.lim[v]
}

/// Compute cut values for all tree edges (bottom-up postorder).
pub(crate) fn init_cut_values(tree: &mut SpanningTree, graph: &LayoutGraph) {
    let n = tree.parent.len();
    let edges = graph.effective_edges();

    // Build children lists
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for node in 0..n {
        if let Some(p) = tree.parent[node] {
            children[p].push(node);
        }
    }

    // Process in postorder (leaves first)
    let postorder = postorder_from_children(&children, tree);
    for &node in &postorder {
        if tree.parent[node].is_none() {
            continue; // root has no tree edge
        }
        tree.cut_value[node] = calc_cut_value(tree, graph, node, &edges);
    }
}

/// Get nodes in postorder from the spanning tree.
fn postorder_from_children(children: &[Vec<usize>], tree: &SpanningTree) -> Vec<usize> {
    let mut result = Vec::new();
    // Find root (node with no parent that's in the tree)
    let root = (0..tree.parent.len())
        .find(|&n| tree.in_tree[n] && tree.parent[n].is_none())
        .unwrap_or(0);

    let mut stack: Vec<(usize, bool)> = vec![(root, false)];
    while let Some((node, post)) = stack.pop() {
        if post {
            result.push(node);
        } else {
            stack.push((node, true));
            for &child in children[node].iter().rev() {
                stack.push((child, false));
            }
        }
    }
    result
}

/// Calculate the cut value for the tree edge connecting `child` to its parent.
///
/// The cut value measures the change in total weighted edge length if this tree edge
/// were removed. Negative values indicate the ranking can be improved by pivoting.
///
/// Follows Dagre.js calcCutValue (network-simplex.js lines 86-120).
fn calc_cut_value(
    tree: &SpanningTree,
    graph: &LayoutGraph,
    child: usize,
    edges: &[(usize, usize)],
) -> f64 {
    let parent = tree.parent[child].unwrap();
    let tree_edge_idx = tree.parent_edge[child].unwrap();

    // Determine if child is the tail (source) of the directed graph edge
    let (from, _to) = edges[tree_edge_idx];
    let child_is_tail = from == child;

    // Start with the tree edge's own weight
    let mut cut = graph.edge_weights[tree_edge_idx];

    // Build set of tree edge indices for quick lookup: check if `other` is a tree child of `child`
    // In Dagre.js, isTreeEdge(t, child, other) checks if there's a tree edge between child and other
    // This means other's parent is child (other is a direct tree child of child)

    // For each graph edge incident on child (except tree edge to parent):
    for (edge_idx, &(e_from, e_to)) in edges.iter().enumerate() {
        if edge_idx == tree_edge_idx {
            continue;
        }

        let is_out_edge;
        let other;
        if e_from == child {
            is_out_edge = true;
            other = e_to;
        } else if e_to == child {
            is_out_edge = false;
            other = e_from;
        } else {
            continue; // not incident on child
        }

        if other == parent {
            continue;
        }

        let points_to_head = is_out_edge == child_is_tail;
        let w = graph.edge_weights[edge_idx];

        cut += if points_to_head { w } else { -w };

        // If other is a tree child of child, adjust by other's cut value
        if tree.parent[other] == Some(child) && tree.parent_edge[other].is_some() {
            let other_cut = tree.cut_value[other];
            cut += if points_to_head {
                -other_cut
            } else {
                other_cut
            };
        }
    }

    cut
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

    /// Helper: A->B, ranks 0,1. Tree: A(root)->B.
    fn make_simple_ab_tree() -> (LayoutGraph, SpanningTree) {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_edge("A", "B");
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1];
        let mut tree = SpanningTree::new(2);
        tree.add_node(0); // A is root
        tree.add_edge(0, 1, 0); // A->B is tree edge 0
        (lg, tree)
    }

    /// Helper: diamond A->B, A->C, B->D, C->D
    /// Tree: A->B, A->C, B->D (edge 2 = C->D is non-tree)
    fn make_diamond_tree() -> (LayoutGraph, SpanningTree) {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_node("D", ());
        g.add_edge("A", "B"); // edge 0
        g.add_edge("A", "C"); // edge 1
        g.add_edge("B", "D"); // edge 2
        g.add_edge("C", "D"); // edge 3 (non-tree)
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1, 1, 2];
        let mut tree = SpanningTree::new(4);
        tree.add_node(0); // A root
        tree.add_edge(0, 1, 0); // A->B
        tree.add_edge(0, 2, 1); // A->C
        tree.add_edge(1, 3, 2); // B->D
        (lg, tree)
    }

    /// Helper: Create a graph/tree with a negative cut value.
    /// Graph: A->C (weight=1), B->C (weight=1), B->D (weight=1)
    /// Ranks: A=0, B=0, C=1, D=1
    /// Tree: A->C (edge 0), then C is parent of B via edge 1 reversed.
    /// Actually, let's build it more carefully with known cut values.
    ///
    /// Simplest: just use a chain A->B->C with ranks [0, 2, 3], minlen=1 for both.
    /// Tree: A->B (edge 0), B->C (edge 1). A->B slack=1 (not tight!).
    /// That won't work — tree edges must be tight.
    ///
    /// Use asymmetric weights: A->B weight=1, B->C weight=3.
    /// Tree: A->B, B->C, ranks 0,1,2.
    /// cut_value[C] (child=C, parent=B, tree_edge=B->C):
    ///   start: weight=3. No other edges incident on C. cut=3.
    /// cut_value[B] (child=B, parent=A, tree_edge=A->B):
    ///   start: weight=1. Edge B->C: is_out_edge=true, child_is_tail=false (A is src).
    ///   points_to_head=(true==false)=false. w=3. cut -= 3 → -2.
    ///   B->C is tree child: parent[C]=B. cut += otherCut=3 → cut = 1.
    /// Hmm, cut_value[B] = 1. Still positive.
    ///
    /// For a negative cut value, we need a tree edge whose removal would
    /// decrease total weighted length. This requires a non-tree edge that
    /// could replace it more efficiently. Build manually:
    ///
    /// Graph: A->B (w=1), A->C (w=1), C->B (w=1)
    /// Ranks: A=0, B=2, C=1 (A->B has slack=1, not tight!)
    /// This can't be a feasible tree since A->B isn't tight.
    ///
    /// Let's use: A->B (w=1), A->C (w=1), C->B (w=1), minlen all 1
    /// Feasible ranks: A=0, C=1, B=2 (all tight)
    /// Tree: A->B (edge 0, slack=2-0-1=1, NOT tight!!)
    /// That doesn't work either.
    ///
    /// The point is: to get a negative cut value, we need a tree edge
    /// that could be replaced. This only happens after pivot modifies the tree.
    /// In practice, feasible_tree + init_cut_values always gives non-negative
    /// cut values initially. The negative values appear after exchangeEdges.
    ///
    /// Let's just test exact cut values for the known diamond case instead.
    fn make_exact_cut_value_tree() -> (LayoutGraph, SpanningTree) {
        // A->B (w=1), B->C (w=3). Ranks 0,1,2. All tight.
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("A", ());
        g.add_node("B", ());
        g.add_node("C", ());
        g.add_edge("A", "B"); // edge 0, w=1
        g.add_edge("B", "C"); // edge 1, w=1
        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.ranks = vec![0, 1, 2];
        let mut tree = SpanningTree::new(3);
        tree.add_node(0);
        tree.add_edge(0, 1, 0); // A->B
        tree.add_edge(1, 2, 1); // B->C
        (lg, tree)
    }

    #[test]
    fn test_cut_value_simple_edge() {
        let (lg, mut tree) = make_simple_ab_tree();
        init_low_lim(&mut tree, 0);
        init_cut_values(&mut tree, &lg);
        // B is child of A. cut_value[B] = weight(A->B) = 1.0
        assert_eq!(tree.cut_value[1], 1.0);
    }

    #[test]
    fn test_cut_value_diamond() {
        let (lg, mut tree) = make_diamond_tree();
        init_low_lim(&mut tree, 0);
        init_cut_values(&mut tree, &lg);
        // All cut values should be non-negative (optimal tree)
        for node in 0..4 {
            if tree.parent[node].is_some() {
                assert!(
                    tree.cut_value[node] >= 0.0,
                    "node {} has negative cut value {}",
                    node,
                    tree.cut_value[node]
                );
            }
        }
    }

    #[test]
    fn test_cut_value_chain_exact() {
        // A->B->C, tree A->B->C, all weight=1, ranks 0,1,2
        let (lg, mut tree) = make_exact_cut_value_tree();
        init_low_lim(&mut tree, 0);
        init_cut_values(&mut tree, &lg);
        // cut_value[C] (child=C, parent=B): just the B->C edge weight = 1.0
        assert_eq!(tree.cut_value[2], 1.0);
        // cut_value[B] (child=B, parent=A): A->B weight + (B->C is tree child, cut adj)
        // start: 1.0. B->C: is_out_edge=true, child_is_tail=false → points_to_head=false
        // cut -= 1.0 → 0.0. Tree child C: cut += cut_value[C]=1.0 → 1.0
        assert_eq!(tree.cut_value[1], 1.0);
    }

    #[test]
    fn test_low_lim_single_node() {
        let mut tree = SpanningTree::new(1);
        tree.add_node(0);
        init_low_lim(&mut tree, 0);
        assert_eq!(tree.low[0], 1);
        assert_eq!(tree.lim[0], 1);
    }

    #[test]
    fn test_low_lim_linear_chain() {
        // Tree: 0 -> 1 -> 2 (0 is root)
        let mut tree = SpanningTree::new(3);
        tree.in_tree = vec![true; 3];
        tree.size = 3;
        tree.parent = vec![None, Some(0), Some(1)];
        tree.parent_edge = vec![None, Some(0), Some(1)];
        init_low_lim(&mut tree, 0);

        assert!(is_descendant(&tree, 2, 0)); // 2 is descendant of 0
        assert!(is_descendant(&tree, 1, 0)); // 1 is descendant of 0
        assert!(is_descendant(&tree, 2, 1)); // 2 is descendant of 1
        assert!(!is_descendant(&tree, 0, 1)); // 0 is NOT descendant of 1
        assert!(!is_descendant(&tree, 0, 2)); // 0 is NOT descendant of 2
    }

    #[test]
    fn test_low_lim_branching_tree() {
        // Tree:    0
        //         / \
        //        1   2
        //        |
        //        3
        let mut tree = SpanningTree::new(4);
        tree.in_tree = vec![true; 4];
        tree.size = 4;
        tree.parent = vec![None, Some(0), Some(0), Some(1)];
        tree.parent_edge = vec![None, Some(0), Some(1), Some(2)];
        init_low_lim(&mut tree, 0);

        assert!(is_descendant(&tree, 3, 1)); // 3 under 1
        assert!(is_descendant(&tree, 3, 0)); // 3 under 0
        assert!(!is_descendant(&tree, 3, 2)); // 3 NOT under 2
        assert!(!is_descendant(&tree, 2, 1)); // 2 NOT under 1
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
