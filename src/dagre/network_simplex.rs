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

    #[test]
    fn test_slack_tight_edge() {
        let lg = make_chain_graph();
        // Edge A->B: rank(B) - rank(A) - minlen = 1 - 0 - 1 = 0
        assert_eq!(slack(&lg, 0), 0);
    }

    #[test]
    fn test_slack_non_tight_edge() {
        let lg = make_chain_graph();
        // Edge B->C: rank(C) - rank(B) - minlen = 3 - 1 - 1 = 1
        assert_eq!(slack(&lg, 1), 1);
    }

    #[test]
    fn test_slack_with_custom_minlen() {
        let mut lg = make_chain_graph();
        lg.edge_minlens[0] = 2; // A->B minlen=2
        // Edge A->B: rank(B) - rank(A) - minlen = 1 - 0 - 2 = -1
        assert_eq!(slack(&lg, 0), -1);
    }
}
