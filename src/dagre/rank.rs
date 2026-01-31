//! Phase 2: Assign nodes to ranks (layers).
//!
//! Dispatches to the configured ranking algorithm:
//! - NetworkSimplex (default): optimal rank assignment minimizing total edge length
//! - LongestPath: fast heuristic via Kahn's topological sort

use std::collections::VecDeque;

use super::graph::LayoutGraph;
use super::types::{LayoutConfig, Ranker};

/// Assign ranks to nodes by dispatching to the configured ranker.
pub fn run(graph: &mut LayoutGraph, config: &LayoutConfig) {
    match config.ranker {
        Ranker::NetworkSimplex => super::network_simplex::run(graph),
        Ranker::LongestPath => longest_path(graph),
    }
}

/// Assign ranks to nodes using longest-path algorithm.
pub fn longest_path(graph: &mut LayoutGraph) {
    let n = graph.node_ids.len();
    if n == 0 {
        return;
    }

    // Get effective edges (with reversals applied)
    let edges = graph.effective_edges();

    // Build adjacency and compute in-degrees
    let mut in_degree = vec![0usize; n];
    let mut successors: Vec<Vec<(usize, i32)>> = vec![Vec::new(); n];

    for (edge_idx, &(from, to)) in edges.iter().enumerate() {
        let minlen = graph.edge_minlens[edge_idx];
        successors[from].push((to, minlen));
        in_degree[to] += 1;
    }

    // Kahn's algorithm with rank tracking
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut ranks = vec![0i32; n];

    // Start with nodes that have no predecessors
    for node in 0..n {
        if in_degree[node] == 0 {
            queue.push_back(node);
            ranks[node] = 0;
        }
    }

    // Handle disconnected nodes (no edges at all)
    if queue.is_empty() {
        // All nodes have incoming edges - must be cycles only
        // Start with the first node
        queue.push_back(0);
        ranks[0] = 0;
    }

    let mut processed = 0;
    while let Some(node) = queue.pop_front() {
        processed += 1;
        for &(succ, minlen) in &successors[node] {
            ranks[succ] = ranks[succ].max(ranks[node] + minlen);

            in_degree[succ] -= 1;
            if in_degree[succ] == 0 {
                queue.push_back(succ);
            }
        }
    }

    // Handle remaining unprocessed nodes (shouldn't happen after acyclic phase)
    if processed < n {
        let max_rank = *ranks.iter().max().unwrap_or(&0);
        for node in 0..n {
            if ranks[node] == 0 && in_degree[node] > 0 {
                ranks[node] = max_rank + 1;
            }
        }
    }

    graph.ranks = ranks;
}

/// Normalize ranks so minimum is 0.
pub fn normalize(graph: &mut LayoutGraph) {
    if let Some(&min) = graph.ranks.iter().min() {
        for rank in &mut graph.ranks {
            *rank -= min;
        }
    }
}

/// Get nodes grouped by rank.
pub fn by_rank(graph: &LayoutGraph) -> Vec<Vec<usize>> {
    let max_rank = graph.ranks.iter().max().copied().unwrap_or(0) as usize;
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];

    for (node, &rank) in graph.ranks.iter().enumerate() {
        layers[rank as usize].push(node);
    }

    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::DiGraph;

    #[test]
    fn test_run_with_longest_path_config() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_edge("A", "B");
        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        let config = LayoutConfig {
            ranker: Ranker::LongestPath,
            ..Default::default()
        };
        run(&mut lg, &config);
        normalize(&mut lg);
        assert_eq!(lg.ranks[lg.node_index[&"A".into()]], 0);
        assert_eq!(lg.ranks[lg.node_index[&"B".into()]], 1);
    }

    #[test]
    fn test_rank_linear_chain() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg, &LayoutConfig::default());
        normalize(&mut lg);

        // A=0, B=1, C=2
        assert_eq!(lg.ranks[lg.node_index[&"A".into()]], 0);
        assert_eq!(lg.ranks[lg.node_index[&"B".into()]], 1);
        assert_eq!(lg.ranks[lg.node_index[&"C".into()]], 2);
    }

    #[test]
    fn test_rank_diamond() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");
        graph.add_edge("B", "D");
        graph.add_edge("C", "D");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg, &LayoutConfig::default());
        normalize(&mut lg);

        // A=0, B=C=1, D=2
        assert_eq!(lg.ranks[lg.node_index[&"A".into()]], 0);
        assert_eq!(lg.ranks[lg.node_index[&"B".into()]], 1);
        assert_eq!(lg.ranks[lg.node_index[&"C".into()]], 1);
        assert_eq!(lg.ranks[lg.node_index[&"D".into()]], 2);
    }

    #[test]
    fn test_rank_disconnected() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        // No edges - all disconnected

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg, &LayoutConfig::default());
        normalize(&mut lg);

        // All should be at rank 0
        assert_eq!(lg.ranks[0], 0);
        assert_eq!(lg.ranks[1], 0);
        assert_eq!(lg.ranks[2], 0);
    }

    #[test]
    fn test_longest_path_respects_minlen() {
        // A -> B with minlen=2, B -> C with minlen=1
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("B", "C");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        lg.edge_minlens[0] = 2; // A->B needs minlen=2

        run(&mut lg, &LayoutConfig::default());
        normalize(&mut lg);

        let a = lg.node_index[&"A".into()];
        let b = lg.node_index[&"B".into()];
        let c = lg.node_index[&"C".into()];

        assert_eq!(lg.ranks[a], 0);
        assert_eq!(lg.ranks[b], 2); // minlen=2 from A
        assert_eq!(lg.ranks[c], 3); // minlen=1 from B
    }

    #[test]
    fn test_by_rank() {
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");
        graph.add_edge("B", "D");
        graph.add_edge("C", "D");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        run(&mut lg, &LayoutConfig::default());
        normalize(&mut lg);

        let layers = by_rank(&lg);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].len(), 1); // A
        assert_eq!(layers[1].len(), 2); // B, C
        assert_eq!(layers[2].len(), 1); // D
    }
}
