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
        let e = lg.add_nesting_edge(root_idx, top_idx, nesting_weight);
        lg.nesting_edges.insert(e);
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

/// Insert title dummy nodes at correct ranks after ranking is complete.
///
/// For each titled compound, creates a title node at `border_top_rank - 1`.
/// Must be called after rank::run() + rank::normalize() + nesting::cleanup()
/// and before assign_rank_minmax().
pub fn insert_title_nodes(lg: &mut LayoutGraph) {
    let compounds: Vec<usize> = lg.compound_titles.iter().copied().collect();
    for compound_idx in compounds {
        let compound_id = lg.node_ids[compound_idx].0.clone();
        let bt_idx = lg.border_top[&compound_idx];
        let title_rank = lg.ranks[bt_idx] - 1;

        let title_id = NodeId(format!("_tt_{}", compound_id));
        let title_idx = lg.add_nesting_node(title_id);
        lg.ranks[title_idx] = title_rank;
        lg.parents[title_idx] = Some(compound_idx);
        lg.border_title.insert(compound_idx, title_idx);

        // Add edge title → border_top so the title participates in
        // ordering and positioning (without an edge it would float freely)
        let edge_idx = lg.add_nesting_edge(title_idx, bt_idx, 0.0);
        // Don't mark as nesting edge — it should survive cleanup and be
        // visible to normalization, ordering, and positioning
        let _ = edge_idx;
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
    use crate::dagre::LayoutConfig;
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
    fn test_nesting_run_does_not_create_title_node() {
        let mut lg = build_test_titled_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);

        // After run(), border_title should NOT be populated
        // (title nodes are created post-rank, not during nesting)
        assert!(
            !lg.border_title.contains_key(&sg1_idx),
            "run() should not create title nodes"
        );
        // But border_top and border_bottom should still exist
        assert!(lg.border_top.contains_key(&sg1_idx));
        assert!(lg.border_bottom.contains_key(&sg1_idx));
    }

    #[test]
    fn test_titled_compound_gets_title_node_after_insert() {
        use crate::dagre::rank;

        let mut lg = build_test_titled_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        cleanup(&mut lg);
        insert_title_nodes(&mut lg);

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
        rank::run(&mut lg, &LayoutConfig::default());
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
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        cleanup(&mut lg);
        insert_title_nodes(&mut lg);
        assign_rank_minmax(&mut lg);

        let title_idx = lg.border_title[&sg1_idx];
        let top_idx = lg.border_top[&sg1_idx];

        // min_rank should be the title's rank, not border_top's rank
        assert_eq!(lg.min_rank[&sg1_idx], lg.ranks[title_idx]);
        // title rank should be strictly less than border_top rank
        assert!(lg.ranks[title_idx] < lg.ranks[top_idx]);
    }

    #[test]
    fn test_insert_title_nodes_sets_correct_rank() {
        use crate::dagre::rank;

        let mut lg = build_test_titled_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        cleanup(&mut lg);

        let bt_rank_before = lg.ranks[lg.border_top[&sg1_idx]];

        insert_title_nodes(&mut lg);

        // Title node should exist
        assert!(lg.border_title.contains_key(&sg1_idx));
        let title_idx = lg.border_title[&sg1_idx];

        // Title rank should be border_top_rank - 1
        assert_eq!(lg.ranks[title_idx], bt_rank_before - 1);

        // Title should be a child of the compound
        assert_eq!(lg.parents[title_idx], Some(sg1_idx));
    }

    #[test]
    fn test_insert_title_nodes_multi_subgraph_no_collision() {
        use crate::dagre::rank;

        let mut g: DiGraph<(f64, f64)> = DiGraph::new();
        g.add_node("A", (10.0, 10.0));
        g.add_node("B", (10.0, 10.0));
        g.add_node("C", (10.0, 10.0));
        g.add_node("D", (10.0, 10.0));
        g.add_node("sg1", (0.0, 0.0));
        g.add_node("sg2", (0.0, 0.0));
        g.add_edge("A", "B");
        g.add_edge("C", "D");
        g.add_edge("A", "C"); // cross-subgraph edge
        g.set_parent("A", "sg1");
        g.set_parent("B", "sg1");
        g.set_parent("C", "sg2");
        g.set_parent("D", "sg2");
        g.set_has_title("sg1");
        g.set_has_title("sg2");

        let mut lg = LayoutGraph::from_digraph(&g, |_, dims| *dims);
        run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        cleanup(&mut lg);
        insert_title_nodes(&mut lg);

        let sg1_idx = lg.node_index[&"sg1".into()];
        let sg2_idx = lg.node_index[&"sg2".into()];

        let tt1 = lg.border_title[&sg1_idx];
        let tt2 = lg.border_title[&sg2_idx];
        let bt1 = lg.border_top[&sg1_idx];
        let bt2 = lg.border_top[&sg2_idx];

        // Each title is one rank above its own border_top
        assert_eq!(lg.ranks[tt1], lg.ranks[bt1] - 1);
        assert_eq!(lg.ranks[tt2], lg.ranks[bt2] - 1);

        assert!(lg.ranks[tt1] >= 0);
        assert!(lg.ranks[tt2] >= 0);
    }

    #[test]
    fn test_insert_title_nodes_skips_untitled() {
        use crate::dagre::rank;

        let mut lg = build_test_compound_layout_graph();
        let sg1_idx = lg.node_index[&"sg1".into()];

        run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        cleanup(&mut lg);
        insert_title_nodes(&mut lg);

        assert!(!lg.border_title.contains_key(&sg1_idx));
    }

    #[test]
    fn test_assign_rank_minmax_noop_simple() {
        use crate::dagre::rank;

        let mut lg = build_test_simple_layout_graph();
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);

        assign_rank_minmax(&mut lg);

        assert!(lg.min_rank.is_empty());
        assert!(lg.max_rank.is_empty());
    }
}
