//! Phase 1: Make the graph acyclic by identifying back-edges.
//!
//! Uses petgraph's greedy_feedback_arc_set (Eades, Lin, Smyth 1993) -
//! the same algorithm Dagre uses for Mermaid compatibility.

use std::collections::HashSet;

use petgraph::algo::greedy_feedback_arc_set;

use super::graph::LayoutGraph;

/// Identify back-edges that need to be reversed for acyclicity.
/// Marks edges in the LayoutGraph's reversed_edges set.
pub fn run(graph: &mut LayoutGraph) {
    let pg = graph.to_petgraph();
    let back_edges: HashSet<usize> = greedy_feedback_arc_set(&pg)
        .map(|edge_ref| *edge_ref.weight())
        .collect();

    graph.reversed_edges = back_edges;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::DiGraph;

    #[test]
    fn test_acyclic_no_cycle() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg);

        assert!(lg.reversed_edges.is_empty());
    }

    #[test]
    fn test_acyclic_simple_cycle() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "A"); // Cycle

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg);

        // One edge should be marked for reversal
        assert_eq!(lg.reversed_edges.len(), 1);
    }

    #[test]
    fn test_acyclic_triangle_cycle() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");
        graph.add_edge("C", "A"); // Creates cycle

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg);

        // One edge should be reversed to break the cycle
        assert_eq!(lg.reversed_edges.len(), 1);
    }

    #[test]
    fn test_acyclic_self_loop() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_edge("A", "A"); // Self-loop

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg);

        // Self-loop should be marked for reversal
        assert_eq!(lg.reversed_edges.len(), 1);
    }

    #[test]
    fn test_acyclic_disconnected() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_edge("A", "B");
        graph.add_edge("C", "D");
        // Two disconnected components, no cycles

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg);

        assert!(lg.reversed_edges.is_empty());
    }
}
