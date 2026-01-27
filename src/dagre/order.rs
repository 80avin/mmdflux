//! Phase 3: Reduce edge crossings by reordering nodes within ranks.
//!
//! Implements the barycenter heuristic with iterative sweeping.

use super::graph::LayoutGraph;
use super::rank;

/// DFS-based initial ordering matching Dagre's initOrder().
///
/// Visits nodes sorted by rank, adding each to its layer in DFS visit order.
/// This groups connected nodes together, providing a better starting point
/// for crossing minimization than arbitrary insertion order.
///
/// Reference: Gansner et al., "A Technique for Drawing Directed Graphs"
fn init_order(graph: &mut LayoutGraph) {
    let edges = graph.effective_edges();
    let n = graph.node_ids.len();

    // Build successor adjacency list
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in &edges {
        successors[from].push(to);
    }

    // Get all nodes sorted by rank (ascending), matching Dagre's
    // `simpleNodes.sort((a, b) => g.node(a).rank - g.node(b).rank)`
    let mut start_nodes: Vec<usize> = (0..n).collect();
    start_nodes.sort_by_key(|&node| graph.ranks[node]);

    // Track visit state and per-rank insertion index
    let mut visited = vec![false; n];
    let max_rank = graph.ranks.iter().max().copied().unwrap_or(0) as usize;
    let mut layer_next_idx: Vec<usize> = vec![0; max_rank + 1];

    // Iterative DFS to avoid stack overflow on deep graphs.
    // Push successors in reverse so first successor is visited first,
    // matching recursive DFS visit order.
    for &root in &start_nodes {
        if visited[root] {
            continue;
        }
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;

            let rank = graph.ranks[node] as usize;
            graph.order[node] = layer_next_idx[rank];
            layer_next_idx[rank] += 1;

            // Push successors in reverse for correct DFS order
            for &succ in successors[node].iter().rev() {
                if !visited[succ] {
                    stack.push(succ);
                }
            }
        }
    }
}

/// Build layer vectors sorted by node order.
///
/// `rank::by_rank()` returns layers with nodes in insertion order.
/// This function sorts each layer by `graph.order[node]` so the
/// vectors reflect the current ordering.
fn layers_sorted_by_order(graph: &LayoutGraph) -> Vec<Vec<usize>> {
    let mut layers = rank::by_rank(graph);
    for layer in &mut layers {
        layer.sort_by_key(|&node| graph.order[node]);
    }
    layers
}

/// Run crossing reduction using Dagre-style adaptive ordering.
///
/// Matches Dagre's `order()` function in `lib/order/index.js`:
/// - DFS-based initial ordering
/// - Alternating up/down sweeps (one per iteration)
/// - Alternating left/right bias (pattern: false, false, true, true)
/// - Best-order tracking across iterations
/// - Terminates after 4 consecutive non-improving iterations
pub fn run(graph: &mut LayoutGraph) {
    let layers = rank::by_rank(graph);
    if layers.len() < 2 {
        return;
    }

    // DFS-based initial ordering
    init_order(graph);

    // Rebuild layers sorted by the new DFS order
    let layers = layers_sorted_by_order(graph);
    let edges = graph.effective_edges();

    let mut best_cc = usize::MAX;
    let mut best_order: Vec<usize> = Vec::new();

    // Dagre-style adaptive loop.
    //
    // Direction: i % 2 == 0 -> sweep_up, i % 2 == 1 -> sweep_down
    // Bias: i % 4 >= 2 -> bias_right = true
    // last_best increments every iteration, resets to 0 on strict improvement
    let mut i: usize = 0;
    let mut last_best: usize = 0;

    while last_best < 4 {
        let bias_right = (i % 4) >= 2;

        if i % 2 == 0 {
            sweep_up(graph, &layers, &edges, bias_right);
        } else {
            sweep_down(graph, &layers, &edges, bias_right);
        }

        let cc = count_all_crossings(graph, &layers, &edges);

        if cc < best_cc {
            last_best = 0;
            best_cc = cc;
            best_order = graph.order.clone();
        }

        i += 1;
        last_best += 1;
    }

    // Restore best ordering found
    if !best_order.is_empty() {
        graph.order = best_order;
    }
}

fn sweep_down(
    graph: &mut LayoutGraph,
    layers: &[Vec<usize>],
    edges: &[(usize, usize)],
    bias_right: bool,
) {
    for i in 1..layers.len() {
        let fixed = &layers[i - 1];
        let free = &layers[i];
        reorder_layer(graph, fixed, free, edges, true, bias_right);
    }
}

fn sweep_up(
    graph: &mut LayoutGraph,
    layers: &[Vec<usize>],
    edges: &[(usize, usize)],
    bias_right: bool,
) {
    for i in (0..layers.len() - 1).rev() {
        let fixed = &layers[i + 1];
        let free = &layers[i];
        reorder_layer(graph, fixed, free, edges, false, bias_right);
    }
}

/// Reorder nodes in `free` layer based on barycenter of connections to `fixed` layer.
fn reorder_layer(
    graph: &mut LayoutGraph,
    fixed: &[usize],
    free: &[usize],
    edges: &[(usize, usize)],
    downward: bool,
    bias_right: bool,
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

    // Sort by barycenter with bias-aware tie-breaking
    barycenters.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                if bias_right {
                    b.2.cmp(&a.2) // Prefer larger original_pos (right bias)
                } else {
                    a.2.cmp(&b.2) // Prefer smaller original_pos (left bias)
                }
            })
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
    use crate::dagre::NodeId;
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
    fn test_bias_right_changes_order() {
        // A fans out to B and C, giving both equal barycenters.
        //   A
        //  / \
        // B   C
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        let layers = rank::by_rank(&lg);
        for layer in &layers {
            for (idx, &node) in layer.iter().enumerate() {
                lg.order[node] = idx;
            }
        }

        let edges = lg.effective_edges();
        let fixed = &layers[0]; // [A]
        let free = &layers[1]; // [B, C]

        let b = lg.node_index[&NodeId::from("B")];
        let c = lg.node_index[&NodeId::from("C")];

        // Left bias (bias_right = false)
        reorder_layer(&mut lg, fixed, free, &edges, true, false);
        let left_order_b = lg.order[b];
        let left_order_c = lg.order[c];

        // Reset orders
        for (idx, &node) in free.iter().enumerate() {
            lg.order[node] = idx;
        }

        // Right bias (bias_right = true)
        reorder_layer(&mut lg, fixed, free, &edges, true, true);
        let right_order_b = lg.order[b];
        let right_order_c = lg.order[c];

        // Left bias: B before C (smaller original_pos wins)
        assert!(
            left_order_b < left_order_c,
            "Left bias should put B before C"
        );
        // Right bias: C before B (larger original_pos wins)
        assert!(
            right_order_b > right_order_c,
            "Right bias should put C before B"
        );
    }

    #[test]
    fn test_init_order_groups_connected() {
        // Diamond graph:
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
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
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        init_order(&mut lg);

        // All nodes should have valid consecutive order values per layer
        let layers = rank::by_rank(&lg);
        for layer in &layers {
            let mut orders: Vec<usize> = layer.iter().map(|&n| lg.order[n]).collect();
            orders.sort();
            let expected: Vec<usize> = (0..layer.len()).collect();
            assert_eq!(
                orders, expected,
                "Orders should be consecutive starting from 0"
            );
        }
    }

    #[test]
    fn test_init_order_disconnected() {
        // Two disconnected chains: A->B, C->D
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_edge("A", "B");
        graph.add_edge("C", "D");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        init_order(&mut lg);

        // All nodes should have valid order values, no panics
        let layers = rank::by_rank(&lg);
        for layer in &layers {
            let mut orders: Vec<usize> = layer.iter().map(|&n| lg.order[n]).collect();
            orders.sort();
            let expected: Vec<usize> = (0..layer.len()).collect();
            assert_eq!(orders, expected);
        }
    }

    #[test]
    fn test_adaptive_selects_best() {
        // Crossing graph: A->D, B->C
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

        let layers = rank::by_rank(&lg);
        let edges = lg.effective_edges();
        let crossings = count_all_crossings(&lg, &layers, &edges);
        assert_eq!(
            crossings, 0,
            "Adaptive loop should find zero-crossing ordering"
        );
    }

    #[test]
    fn test_adaptive_converges() {
        //     A
        //    / \
        //   B   C
        //   |   |
        //   D   E
        //    \ /
        //     F
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());
        graph.add_node("C", ());
        graph.add_node("D", ());
        graph.add_node("E", ());
        graph.add_node("F", ());
        graph.add_edge("A", "B");
        graph.add_edge("A", "C");
        graph.add_edge("B", "D");
        graph.add_edge("C", "E");
        graph.add_edge("D", "F");
        graph.add_edge("E", "F");

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        run(&mut lg);

        let layers = rank::by_rank(&lg);
        for layer in &layers {
            let mut orders: Vec<usize> = layer.iter().map(|&n| lg.order[n]).collect();
            orders.sort();
            let expected: Vec<usize> = (0..layer.len()).collect();
            assert_eq!(
                orders, expected,
                "Orders should be consecutive in each layer"
            );
        }

        let edges = lg.effective_edges();
        assert_eq!(count_all_crossings(&lg, &layers, &edges), 0);
    }

    #[test]
    fn test_adaptive_single_layer() {
        // All nodes at same rank - should exit early
        let mut graph: DiGraph<()> = DiGraph::new();
        graph.add_node("A", ());
        graph.add_node("B", ());

        let mut lg = LayoutGraph::from_digraph(&graph, |_, _| (10.0, 10.0));
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        run(&mut lg);
        // Should not panic
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
