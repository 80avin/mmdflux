//! Phase 3: Reduce edge crossings by reordering nodes within ranks.
//!
//! Implements the barycenter heuristic with iterative sweeping.

use super::graph::LayoutGraph;
use super::rank;

const MAX_ITERATIONS: usize = 24;

/// Run crossing reduction using barycenter heuristic.
pub fn run(graph: &mut LayoutGraph) {
    let layers = rank::by_rank(graph);
    if layers.len() < 2 {
        // No crossings possible with 0 or 1 layers
        return;
    }

    // Initialize order based on current layer positions
    for layer in &layers {
        for (idx, &node) in layer.iter().enumerate() {
            graph.order[node] = idx;
        }
    }

    // Get effective edges for crossing computation
    let edges = graph.effective_edges();

    // Sweep up and down to minimize crossings
    let mut best_crossings = count_all_crossings(graph, &layers, &edges);

    for _ in 0..MAX_ITERATIONS {
        let prev_crossings = best_crossings;

        sweep_down(graph, &layers, &edges);
        sweep_up(graph, &layers, &edges);

        best_crossings = count_all_crossings(graph, &layers, &edges);

        // Stop if no improvement
        if best_crossings >= prev_crossings {
            break;
        }
    }
}

fn sweep_down(graph: &mut LayoutGraph, layers: &[Vec<usize>], edges: &[(usize, usize)]) {
    for i in 1..layers.len() {
        let fixed = &layers[i - 1];
        let free = &layers[i];
        reorder_layer(graph, fixed, free, edges, true);
    }
}

fn sweep_up(graph: &mut LayoutGraph, layers: &[Vec<usize>], edges: &[(usize, usize)]) {
    for i in (0..layers.len() - 1).rev() {
        let fixed = &layers[i + 1];
        let free = &layers[i];
        reorder_layer(graph, fixed, free, edges, false);
    }
}

/// Reorder nodes in `free` layer based on barycenter of connections to `fixed` layer.
fn reorder_layer(
    graph: &mut LayoutGraph,
    fixed: &[usize],
    free: &[usize],
    edges: &[(usize, usize)],
    downward: bool,
) {
    // Calculate barycenter for each node in free layer
    let mut barycenters: Vec<(usize, f64, usize)> = Vec::new();

    for (original_pos, &node) in free.iter().enumerate() {
        let neighbors: Vec<usize> = if downward {
            // Looking at predecessors (nodes in fixed layer that point to this node)
            edges
                .iter()
                .filter(|&&(_, to)| to == node)
                .map(|&(from, _)| from)
                .filter(|n| fixed.contains(n))
                .collect()
        } else {
            // Looking at successors (nodes in fixed layer that this node points to)
            edges
                .iter()
                .filter(|&&(from, _)| from == node)
                .map(|&(_, to)| to)
                .filter(|n| fixed.contains(n))
                .collect()
        };

        let barycenter = if neighbors.is_empty() {
            // Keep current position
            graph.order[node] as f64
        } else {
            // Average position of neighbors
            let sum: f64 = neighbors.iter().map(|&n| graph.order[n] as f64).sum();
            sum / neighbors.len() as f64
        };

        barycenters.push((node, barycenter, original_pos));
    }

    // Stable sort by barycenter (preserves original order for ties)
    barycenters.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
    });

    // Update order
    for (new_pos, (node, _, _)) in barycenters.iter().enumerate() {
        graph.order[*node] = new_pos;
    }
}

/// Count total crossings between all adjacent layer pairs.
fn count_all_crossings(
    graph: &LayoutGraph,
    layers: &[Vec<usize>],
    edges: &[(usize, usize)],
) -> usize {
    let mut total = 0;
    for i in 0..layers.len().saturating_sub(1) {
        total += count_crossings_between(graph, &layers[i], &layers[i + 1], edges);
    }
    total
}

/// Count crossings between two adjacent layers.
fn count_crossings_between(
    graph: &LayoutGraph,
    layer1: &[usize],
    layer2: &[usize],
    edges: &[(usize, usize)],
) -> usize {
    // Collect edges between these layers with their positions
    let mut edge_positions: Vec<(usize, usize)> = Vec::new();

    for &(from, to) in edges {
        if layer1.contains(&from) && layer2.contains(&to) {
            edge_positions.push((graph.order[from], graph.order[to]));
        } else if layer1.contains(&to) && layer2.contains(&from) {
            edge_positions.push((graph.order[to], graph.order[from]));
        }
    }

    // Count crossings using simple O(e^2) algorithm
    let mut crossings = 0;
    for i in 0..edge_positions.len() {
        for j in i + 1..edge_positions.len() {
            let (u1, v1) = edge_positions[i];
            let (u2, v2) = edge_positions[j];

            // Edges cross if one goes up while the other goes down
            if (u1 < u2 && v1 > v2) || (u1 > u2 && v1 < v2) {
                crossings += 1;
            }
        }
    }

    crossings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::DiGraph;

    fn setup_graph_and_run(
        nodes: &[&str],
        edges_list: &[(&str, &str)],
    ) -> (LayoutGraph, Vec<Vec<usize>>) {
        let mut graph: DiGraph<()> = DiGraph::new();
        for &n in nodes {
            graph.add_node(n, ());
        }
        for &(from, to) in edges_list {
            graph.add_edge(from, to);
        }

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);
        let layers = rank::by_rank(&lg);
        (lg, layers)
    }

    #[test]
    fn test_order_no_crossings() {
        let (mut lg, _) = setup_graph_and_run(&["A", "B", "C"], &[("A", "B"), ("B", "C")]);

        run(&mut lg);

        // Simple chain should have no crossings
        let layers = rank::by_rank(&lg);
        let edges = lg.effective_edges();
        assert_eq!(count_all_crossings(&lg, &layers, &edges), 0);
    }

    #[test]
    fn test_order_reduces_crossings() {
        // Create a graph that initially has crossings
        // Layer 0: A, B
        // Layer 1: C, D
        // Edges: A->D, B->C (crosses if A,B and C,D are in wrong order)
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_edge("A", "D");
        graph.add_edge("B", "C");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        run(&mut lg);

        // After ordering, crossings should be minimized
        let layers = rank::by_rank(&lg);
        let edges = lg.effective_edges();
        let crossings = count_all_crossings(&lg, &layers, &edges);
        assert_eq!(crossings, 0);
    }

    #[test]
    fn test_order_with_disconnected() {
        let (mut lg, _) = setup_graph_and_run(
            &["A", "B", "C", "D"],
            &[
                ("A", "C"),
                ("B", "D"),
                // Two parallel paths, no connections between them
            ],
        );

        run(&mut lg);

        // Should complete without errors
        let layers = rank::by_rank(&lg);
        assert!(!layers.is_empty());
    }
}
