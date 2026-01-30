//! Nesting graph setup and cleanup for compound graph layout.
//!
//! Creates border top/bottom nodes and weighted nesting edges that constrain
//! compound node children to be ranked between the border nodes. After ranking,
//! cleanup removes the nesting edges and root node.

use super::graph::LayoutGraph;
use super::types::NodeId;

/// Add nesting structure to the layout graph for compound nodes.
///
/// For each compound node:
/// - Creates border_top and border_bottom dummy nodes
/// - Adds nesting edges: top -> each child and each child -> bottom
///
/// Also creates a nesting root node connected to all top-level nodes.
/// Nesting edges use high weights to dominate ranking.
pub fn run(lg: &mut LayoutGraph) {
    if lg.compound_nodes.is_empty() {
        return;
    }

    let n = lg.node_count();
    let nesting_weight = (n * 2) as f64;

    // For each compound node, create border top/bottom and nesting edges
    let compound_indices: Vec<usize> = lg.compound_nodes.iter().copied().collect();
    for compound_idx in compound_indices {
        let compound_id = lg.node_ids[compound_idx].0.clone();

        // Create border top node
        let top_id = NodeId(format!("_bt_{}", compound_id));
        let top_idx = lg.add_nesting_node(top_id);
        lg.border_top.insert(compound_idx, top_idx);

        // Create title node for compounds with titles
        if lg.compound_titles.contains(&compound_idx) {
            let title_id = NodeId(format!("_tt_{}", compound_id));
            let title_idx = lg.add_nesting_node(title_id);
            lg.border_title.insert(compound_idx, title_idx);
        }

        // Create border bottom node
        let bot_id = NodeId(format!("_bb_{}", compound_id));
        let bot_idx = lg.add_nesting_node(bot_id);
        lg.border_bottom.insert(compound_idx, bot_idx);

        // Find children of this compound node
        let children: Vec<usize> = lg
            .parents
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == Some(compound_idx))
            .map(|(i, _)| i)
            .collect();

        // Add nesting edges: top -> child, child -> bottom
        for child in children {
            let e1 = lg.add_nesting_edge(top_idx, child, nesting_weight);
            lg.nesting_edges.insert(e1);
            let e2 = lg.add_nesting_edge(child, bot_idx, nesting_weight);
            lg.nesting_edges.insert(e2);
        }
    }

    // Create root node connecting to all top-level nodes and compound border_tops
    let root_id = NodeId("_nesting_root".to_string());
    let root_idx = lg.add_nesting_node(root_id);
    lg.nesting_root = Some(root_idx);

    // Connect root to all top-level nodes (nodes without parents)
    // and to border_top nodes of compound nodes
    let top_level: Vec<usize> = (0..n)
        .filter(|&i| lg.parents[i].is_none() && !lg.compound_nodes.contains(&i))
        .collect();
    for idx in top_level {
        let e = lg.add_nesting_edge(root_idx, idx, nesting_weight);
        lg.nesting_edges.insert(e);
    }
    let compound_indices_for_roots: Vec<usize> = lg.compound_nodes.iter().copied().collect();
    for compound_idx in compound_indices_for_roots {
        let top_idx = lg.border_top[&compound_idx];
        if let Some(&title_idx) = lg.border_title.get(&compound_idx) {
            // root → title → border_top
            let e = lg.add_nesting_edge(root_idx, title_idx, nesting_weight);
            lg.nesting_edges.insert(e);
            let e = lg.add_nesting_edge(title_idx, top_idx, nesting_weight);
            lg.nesting_edges.insert(e);
        } else {
            // root → border_top (existing behavior)
            let e = lg.add_nesting_edge(root_idx, top_idx, nesting_weight);
            lg.nesting_edges.insert(e);
        }
    }
}

/// Compute min_rank and max_rank for each compound node from border node ranks.
///
/// Must be called after ranking and nesting cleanup. Border top/bottom nodes
/// retain their assigned ranks, which define the vertical span of each compound node.
pub fn assign_rank_minmax(lg: &mut LayoutGraph) {
    let compound_indices: Vec<usize> = lg.compound_nodes.iter().copied().collect();
    for compound_idx in compound_indices {
        // Use title rank if available, otherwise border_top
        if let Some(&title_idx) = lg.border_title.get(&compound_idx) {
            lg.min_rank.insert(compound_idx, lg.ranks[title_idx]);
        } else if let Some(&top_idx) = lg.border_top.get(&compound_idx) {
            lg.min_rank.insert(compound_idx, lg.ranks[top_idx]);
        }
        if let Some(&bot_idx) = lg.border_bottom.get(&compound_idx) {
            lg.max_rank.insert(compound_idx, lg.ranks[bot_idx]);
        }
    }
}

/// Remove nesting edges and root node after ranking.
///
/// Nesting edges are marked for removal (set to zero weight and flagged),
/// and the nesting root is cleared. Border top/bottom nodes remain for
/// rank extraction in assign_rank_minmax.
pub fn cleanup(lg: &mut LayoutGraph) {
    // Mark nesting edges as excluded from downstream processing.
    // They remain in the edges vec (for index stability) but are skipped by
    // normalization, ordering, and BK alignment.
    for &edge_idx in &lg.nesting_edges {
        if edge_idx < lg.edge_weights.len() {
            lg.edge_weights[edge_idx] = 0.0;
        }
        lg.excluded_edges.insert(edge_idx);
    }
    lg.nesting_edges.clear();
    lg.nesting_root = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::{DiGraph, LayoutGraph};

    fn build_test_compound_layout_graph() -> LayoutGraph {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("B", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.add_edge("A", "B");
        g.set_parent("A", "sg1");
        g.set_parent("B", "sg1");
        LayoutGraph::from_digraph(&g, |_, dims| *dims)
    }

    fn build_test_simple_layout_graph() -> LayoutGraph {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("B", (10.0, 10.0));
        g.add_edge("A", "B");
        LayoutGraph::from_digraph(&g, |_, dims| *dims)
    }

    #[test]
    fn test_nesting_run_adds_border_nodes() {
        let mut lg = build_test_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];
        let initial_count = lg.node_count();

        run(&mut lg);

        assert!(lg.border_top.contains_key(&sg1_idx));
        assert!(lg.border_bottom.contains_key(&sg1_idx));
        assert!(lg.node_count() > initial_count);
    }

    #[test]
    fn test_nesting_run_adds_nesting_edges() {
        let mut lg = build_test_compound_layout_graph();

        run(&mut lg);

        assert!(!lg.nesting_edges.is_empty());
    }

    #[test]
    fn test_nesting_run_creates_root() {
        let mut lg = build_test_compound_layout_graph();

        run(&mut lg);

        assert!(lg.nesting_root.is_some());
    }

    #[test]
    fn test_nesting_cleanup_removes_edges() {
        let mut lg = build_test_compound_layout_graph();
        run(&mut lg);
        assert!(!lg.nesting_edges.is_empty());

        cleanup(&mut lg);

        assert!(lg.nesting_root.is_none());
        assert!(lg.nesting_edges.is_empty());
    }

    #[test]
    fn test_nesting_run_noop_simple_graph() {
        let mut lg = build_test_simple_layout_graph();
        let initial = lg.node_count();

        run(&mut lg);

        assert_eq!(lg.node_count(), initial);
    }

    fn build_test_titled_compound_layout_graph() -> LayoutGraph {
        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("B", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.add_edge("A", "B");
        g.set_parent("A", "sg1");
        g.set_parent("B", "sg1");
        g.set_has_title("sg1");
        LayoutGraph::from_digraph(&g, |_, dims| *dims)
    }

    #[test]
    fn test_nesting_run_adds_title_node_for_titled_compound() {
        let mut lg = build_test_titled_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);

        assert!(lg.border_title.contains_key(&sg1_idx));
        let title_idx = lg.border_title[&sg1_idx];
        assert_eq!(lg.node_ids[title_idx], NodeId::from("_tt_sg1"));
    }

    #[test]
    fn test_nesting_run_no_title_node_for_untitled_compound() {
        let mut lg = build_test_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);

        assert!(!lg.border_title.contains_key(&sg1_idx));
    }

    #[test]
    fn test_assign_rank_minmax() {
        use crate::dagre::rank;

        let mut lg = build_test_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];
        run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);
        cleanup(&mut lg);

        assign_rank_minmax(&mut lg);

        assert!(lg.min_rank.contains_key(&sg1_idx));
        assert!(lg.max_rank.contains_key(&sg1_idx));
        assert!(lg.min_rank[&sg1_idx] <= lg.max_rank[&sg1_idx]);
    }

    #[test]
    fn test_assign_rank_minmax_uses_title_rank_for_min() {
        use crate::dagre::rank;

        let mut lg = build_test_titled_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);
        rank::run(&mut lg);
        rank::normalize(&mut lg);
        cleanup(&mut lg);
        assign_rank_minmax(&mut lg);

        let title_idx = lg.border_title[&sg1_idx];
        let top_idx = lg.border_top[&sg1_idx];

        // min_rank should be the title's rank, not border_top's rank
        assert_eq!(lg.min_rank[&sg1_idx], lg.ranks[title_idx]);
        // title rank should be strictly less than border_top rank
        assert!(lg.ranks[title_idx] < lg.ranks[top_idx]);
    }

    #[test]
    fn test_assign_rank_minmax_noop_simple() {
        use crate::dagre::rank;

        let mut lg = build_test_simple_layout_graph();
        rank::run(&mut lg);
        rank::normalize(&mut lg);

        assign_rank_minmax(&mut lg);

        assert!(lg.min_rank.is_empty());
        assert!(lg.max_rank.is_empty());
    }
}
