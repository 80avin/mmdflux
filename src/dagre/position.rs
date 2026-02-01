//! Phase 4: Assign x, y coordinates to nodes.
//!
//! Implements coordinate assignment using the Brandes-Köpf algorithm for
//! optimal horizontal positioning, with y-coordinates based on layer rank.

use super::bk::{BKConfig, position_x};
use super::graph::LayoutGraph;
use super::rank;
use super::types::{Direction, LayoutConfig, Point};

/// Assign positions to all nodes.
pub fn run(graph: &mut LayoutGraph, config: &LayoutConfig) {
    let layers = rank::by_rank_filtered(graph, |node| graph.is_position_node(node));

    // Sort each layer by the computed order
    let sorted_layers: Vec<Vec<usize>> = layers
        .iter()
        .map(|layer| {
            let mut sorted = layer.clone();
            sorted.sort_by_key(|&n| graph.order[n]);
            sorted
        })
        .collect();

    // Assign coordinates based on direction
    match config.direction {
        Direction::TopBottom | Direction::BottomTop => {
            assign_vertical(graph, &sorted_layers, config);
        }
        Direction::LeftRight | Direction::RightLeft => {
            assign_horizontal(graph, &sorted_layers, config);
        }
    }

    // Post-processing: separate overlapping sibling subgraph content
    // in the cross-rank direction. Only applies when sibling subgraph
    // content nodes share the exact same cross-rank position.
    // TODO(plan-0038): Evaluate whether this workaround should be gated/removed.
    if !graph.compound_nodes.is_empty() {
        separate_sibling_subgraph_content(graph, config);
    }

    // Reverse coordinates if needed
    if config.direction.is_reversed() {
        reverse_positions(graph, config);
    }
}

fn assign_vertical(graph: &mut LayoutGraph, layers: &[Vec<usize>], config: &LayoutConfig) {
    if layers.is_empty() {
        return;
    }

    // Use Brandes-Köpf algorithm for x-coordinate assignment
    let bk_config = BKConfig {
        node_sep: config.node_sep,
        edge_sep: config.edge_sep,
        direction: config.direction,
    };
    let mut x_coords = position_x(graph, &bk_config);
    center_source_nodes(graph, &mut x_coords);

    // Find minimum x to shift everything to start at margin
    let min_x = (0..graph.node_ids.len())
        .filter_map(|node| {
            x_coords
                .get(&node)
                .map(|&cx| cx - graph.dimensions[node].0 / 2.0)
        })
        .fold(f64::INFINITY, f64::min);

    let x_shift = config.margin - min_x;

    // Assign Y based on rank, X from BK algorithm
    let mut y = config.margin;

    for layer in layers.iter() {
        for &node in layer {
            let (w, _h) = graph.dimensions[node];
            // BK returns center x, convert to top-left corner
            let center_x = x_coords.get(&node).copied().unwrap_or(0.0);
            let x = center_x - w / 2.0 + x_shift;
            graph.positions[node] = Point { x, y };
        }

        // Y advances by max height in this layer
        let max_height = layer
            .iter()
            .map(|&n| graph.dimensions[n].1)
            .fold(0.0, f64::max);
        y += max_height + config.rank_sep;
    }
}

fn center_source_nodes(graph: &LayoutGraph, x_coords: &mut std::collections::HashMap<usize, f64>) {
    use std::collections::{HashMap, HashSet};

    let n = graph.node_ids.len();
    let mut has_pred = vec![false; n];
    let mut succs: HashMap<usize, Vec<usize>> = HashMap::new();

    for (idx, &(from, to, _)) in graph.edges.iter().enumerate() {
        if graph.excluded_edges.contains(&idx) {
            continue;
        }
        let (from, to) = if graph.reversed_edges.contains(&idx) {
            (to, from)
        } else {
            (from, to)
        };
        if !graph.is_position_node(from) || !graph.is_position_node(to) {
            continue;
        }
        has_pred[to] = true;
        succs.entry(from).or_default().push(to);
    }

    let resolve_target = |start: usize| -> Option<usize> {
        let mut current = start;
        let mut visited = HashSet::new();
        loop {
            if !graph.is_dummy_index(current) {
                return Some(current);
            }
            if !visited.insert(current) {
                return None;
            }
            let nexts = succs.get(&current)?;
            if nexts.is_empty() {
                return None;
            }
            current = nexts[0];
        }
    };

    for node in 0..n {
        if !graph.is_position_node(node) || has_pred[node] || graph.original_has_predecessor[node] {
            continue;
        }
        let mut targets = HashSet::new();
        if let Some(nexts) = succs.get(&node) {
            for &succ in nexts {
                if let Some(target) = resolve_target(succ) {
                    targets.insert(target);
                }
            }
        }
        if targets.len() < 2 {
            continue;
        }
        let xs: Vec<f64> = targets
            .iter()
            .filter_map(|&t| x_coords.get(&t).copied())
            .collect();
        if xs.is_empty() {
            continue;
        }
        let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        x_coords.insert(node, (min_x + max_x) / 2.0);
    }
}

fn assign_horizontal(graph: &mut LayoutGraph, layers: &[Vec<usize>], config: &LayoutConfig) {
    if layers.is_empty() {
        return;
    }

    // Use Brandes-Köpf algorithm for y-coordinate assignment (perpendicular to rank)
    // BK always optimizes the "horizontal" axis (perpendicular to layer direction)
    let bk_config = BKConfig {
        node_sep: config.node_sep,
        edge_sep: config.edge_sep,
        direction: config.direction,
    };
    let mut y_coords = position_x(graph, &bk_config);

    // Post-BK centering: center layer-0 source nodes among their successors.
    // BK aligns via predecessors; layer-0 nodes have none, so they may default to
    // their first child's position instead of being centered.
    for &node in &layers[0] {
        let has_predecessors = graph.edges.iter().any(|&(_, to, _)| to == node);
        if has_predecessors {
            continue;
        }

        let succ_ys: Vec<f64> = graph
            .edges
            .iter()
            .filter(|&&(from, _, _)| from == node)
            .filter_map(|&(_, to, _)| y_coords.get(&to).copied())
            .collect();

        if succ_ys.len() >= 2 {
            let min_y = succ_ys.iter().copied().fold(f64::INFINITY, f64::min);
            let max_y = succ_ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            y_coords.insert(node, (min_y + max_y) / 2.0);
        }
    }

    // Find minimum y to shift everything to start at margin
    let min_y = (0..graph.node_ids.len())
        .filter_map(|node| {
            y_coords
                .get(&node)
                .map(|&cy| cy - graph.dimensions[node].1 / 2.0)
        })
        .fold(f64::INFINITY, f64::min);

    let y_shift = config.margin - min_y;

    // Assign X based on rank, Y from BK algorithm
    let mut x = config.margin;

    for layer in layers.iter() {
        for &node in layer {
            let (_w, h) = graph.dimensions[node];
            // BK returns center position, convert to top-left corner
            let center_y = y_coords.get(&node).copied().unwrap_or(0.0);
            let y = center_y - h / 2.0 + y_shift;
            graph.positions[node] = Point { x, y };
        }

        // X advances by max width in this layer
        let max_width = layer
            .iter()
            .map(|&n| graph.dimensions[n].0)
            .fold(0.0, f64::max);
        x += max_width + config.rank_sep;
    }
}

/// Separate overlapping sibling subgraph content in the cross-rank direction.
///
/// When sibling subgraphs share rank ranges, the BK algorithm may place their
/// content at the same cross-rank position (e.g., same x for TD layouts).
/// This function detects such overlaps and shifts content apart.
///
/// Only shifts when content nodes from different sibling subgraphs share the
/// exact same cross-rank center position, indicating they've been collapsed.
fn separate_sibling_subgraph_content(graph: &mut LayoutGraph, config: &LayoutConfig) {
    use std::collections::HashMap;

    if graph.node_rank_factor.is_some() {
        return;
    }

    // For TD/BT: cross-rank axis is x. For LR/RL: cross-rank axis is y.
    let is_vertical = matches!(
        config.direction,
        Direction::TopBottom | Direction::BottomTop
    );

    // Group compounds by their parent (to find siblings)
    let mut siblings: HashMap<Option<usize>, Vec<usize>> = HashMap::new();
    for &compound in &graph.compound_nodes {
        let parent = graph.parents.get(compound).copied().flatten();
        siblings.entry(parent).or_default().push(compound);
    }

    for (_, group) in &siblings {
        if group.len() < 2 {
            continue;
        }

        // For each pair of sibling compounds, check content overlap
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let c1 = group[i];
                let c2 = group[j];

                // Get content nodes (children that aren't compounds or border nodes)
                let content1: Vec<usize> = get_content_nodes(graph, c1);
                let content2: Vec<usize> = get_content_nodes(graph, c2);

                if content1.is_empty() || content2.is_empty() {
                    continue;
                }

                // Check if any content nodes at the same rank share the same
                // cross-rank center position (collapsed on top of each other).
                let mut has_collapsed = false;
                for &n1 in &content1 {
                    for &n2 in &content2 {
                        if graph.ranks[n1] != graph.ranks[n2] {
                            continue;
                        }
                        let pos1 = if is_vertical {
                            graph.positions[n1].x
                        } else {
                            graph.positions[n1].y
                        };
                        let pos2 = if is_vertical {
                            graph.positions[n2].x
                        } else {
                            graph.positions[n2].y
                        };
                        if (pos1 - pos2).abs() < 0.5 {
                            has_collapsed = true;
                            break;
                        }
                    }
                    if has_collapsed {
                        break;
                    }
                }

                if !has_collapsed {
                    continue;
                }

                // Compute shift needed: width of c1's content + separation
                let (_, max1) = content_range(graph, &content1, is_vertical);
                let (min2, _) = content_range(graph, &content2, is_vertical);
                let overlap = max1 - min2 + config.node_sep;
                if overlap > 0.0 {
                    shift_compound_content(graph, c2, overlap, is_vertical);
                }
            }
        }
    }
}

/// Get content node indices for a compound (non-compound, non-border children).
fn get_content_nodes(graph: &LayoutGraph, compound: usize) -> Vec<usize> {
    graph
        .parents
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            **p == Some(compound)
                && !graph.compound_nodes.contains(i)
                && !graph.border_type.contains_key(i)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Get the cross-rank range (min, max) of positioned content nodes.
fn content_range(graph: &LayoutGraph, nodes: &[usize], is_vertical: bool) -> (f64, f64) {
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;
    for &n in nodes {
        let pos = if is_vertical {
            graph.positions[n].x
        } else {
            graph.positions[n].y
        };
        let dim = if is_vertical {
            graph.dimensions[n].0
        } else {
            graph.dimensions[n].1
        };
        min_val = min_val.min(pos);
        max_val = max_val.max(pos + dim);
    }
    (min_val, max_val)
}

/// Shift all content of a compound (including border nodes) by `delta` in the cross-rank axis.
fn shift_compound_content(graph: &mut LayoutGraph, compound: usize, delta: f64, is_vertical: bool) {
    // Collect all nodes belonging to this compound (direct children + border nodes)
    let nodes_to_shift: Vec<usize> = graph
        .parents
        .iter()
        .enumerate()
        .filter(|(_, p)| **p == Some(compound))
        .map(|(i, _)| i)
        .collect();

    for n in nodes_to_shift {
        if is_vertical {
            graph.positions[n].x += delta;
        } else {
            graph.positions[n].y += delta;
        }
        // Recursively shift nested compounds
        if graph.compound_nodes.contains(&n) {
            shift_compound_content(graph, n, delta, is_vertical);
        }
    }
}

fn reverse_positions(graph: &mut LayoutGraph, config: &LayoutConfig) {
    // Find bounds
    let max_x = graph
        .positions
        .iter()
        .zip(graph.dimensions.iter())
        .map(|(p, (w, _))| p.x + w)
        .fold(0.0, f64::max);
    let max_y = graph
        .positions
        .iter()
        .zip(graph.dimensions.iter())
        .map(|(p, (_, h))| p.y + h)
        .fold(0.0, f64::max);

    // Flip coordinates
    match config.direction {
        Direction::BottomTop => {
            for (pos, (_, h)) in graph.positions.iter_mut().zip(graph.dimensions.iter()) {
                pos.y = max_y - pos.y - h;
            }
        }
        Direction::RightLeft => {
            for (pos, (w, _)) in graph.positions.iter_mut().zip(graph.dimensions.iter()) {
                pos.x = max_x - pos.x - w;
            }
        }
        _ => {}
    }
}

/// Calculate the total layout dimensions.
pub fn calculate_dimensions(graph: &LayoutGraph, config: &LayoutConfig) -> (f64, f64) {
    if graph.node_ids.is_empty() {
        return (config.margin * 2.0, config.margin * 2.0);
    }

    let max_x = graph
        .positions
        .iter()
        .zip(graph.dimensions.iter())
        .map(|(p, (w, _))| p.x + w)
        .fold(0.0, f64::max);
    let max_y = graph
        .positions
        .iter()
        .zip(graph.dimensions.iter())
        .map(|(p, (_, h))| p.y + h)
        .fold(0.0, f64::max);

    (max_x + config.margin, max_y + config.margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::graph::DiGraph;
    use crate::dagre::{acyclic, order};

    fn run_full_layout(
        nodes: &[(&str, f64, f64)],
        edges: &[(&str, &str)],
        config: &LayoutConfig,
    ) -> LayoutGraph {
        let mut graph: DiGraph<(f64, f64)> = DiGraph::new();
        for &(id, w, h) in nodes {
            graph.add_node(id, (w, h));
        }
        for &(from, to) in edges {
            graph.add_edge(from, to);
        }

        let mut lg = LayoutGraph::from_digraph(&graph, |_, dims| *dims);
        acyclic::run(&mut lg);
        rank::run(&mut lg, &LayoutConfig::default());
        rank::normalize(&mut lg);
        order::run(&mut lg);
        run(&mut lg, config);
        lg
    }

    #[test]
    fn test_position_vertical_linear() {
        let config = LayoutConfig {
            direction: Direction::TopBottom,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(
            &[("A", 50.0, 30.0), ("B", 50.0, 30.0), ("C", 50.0, 30.0)],
            &[("A", "B"), ("B", "C")],
            &config,
        );

        // Verify A is above B is above C
        let a_y = lg.positions[lg.node_index[&"A".into()]].y;
        let b_y = lg.positions[lg.node_index[&"B".into()]].y;
        let c_y = lg.positions[lg.node_index[&"C".into()]].y;

        assert!(a_y < b_y);
        assert!(b_y < c_y);
    }

    #[test]
    fn test_position_horizontal_linear() {
        let config = LayoutConfig {
            direction: Direction::LeftRight,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(
            &[("A", 50.0, 30.0), ("B", 50.0, 30.0), ("C", 50.0, 30.0)],
            &[("A", "B"), ("B", "C")],
            &config,
        );

        // Verify A is left of B is left of C
        let a_x = lg.positions[lg.node_index[&"A".into()]].x;
        let b_x = lg.positions[lg.node_index[&"B".into()]].x;
        let c_x = lg.positions[lg.node_index[&"C".into()]].x;

        assert!(a_x < b_x);
        assert!(b_x < c_x);
    }

    #[test]
    fn test_position_bottom_top() {
        let config = LayoutConfig {
            direction: Direction::BottomTop,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(
            &[("A", 50.0, 30.0), ("B", 50.0, 30.0)],
            &[("A", "B")],
            &config,
        );

        // In BT, A should be below B (higher y)
        let a_y = lg.positions[lg.node_index[&"A".into()]].y;
        let b_y = lg.positions[lg.node_index[&"B".into()]].y;

        assert!(a_y > b_y);
    }

    #[test]
    fn test_position_right_left() {
        let config = LayoutConfig {
            direction: Direction::RightLeft,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(
            &[("A", 50.0, 30.0), ("B", 50.0, 30.0)],
            &[("A", "B")],
            &config,
        );

        // In RL, A should be right of B (higher x)
        let a_x = lg.positions[lg.node_index[&"A".into()]].x;
        let b_x = lg.positions[lg.node_index[&"B".into()]].x;

        assert!(a_x > b_x);
    }

    #[test]
    fn test_position_diamond_centering() {
        let config = LayoutConfig {
            direction: Direction::TopBottom,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(
            &[
                ("A", 50.0, 30.0),
                ("B", 50.0, 30.0),
                ("C", 50.0, 30.0),
                ("D", 50.0, 30.0),
            ],
            &[("A", "B"), ("A", "C"), ("B", "D"), ("C", "D")],
            &config,
        );

        // A and D should be centered horizontally (same x or close)
        let a_x = lg.positions[lg.node_index[&"A".into()]].x;
        let d_x = lg.positions[lg.node_index[&"D".into()]].x;

        // They should be relatively centered
        let a_center = a_x + 25.0; // half of width
        let d_center = d_x + 25.0;
        assert!((a_center - d_center).abs() < 1.0);
    }

    #[test]
    fn test_position_skips_compound_parents() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("sg", ());
        g.add_node("A", ());
        g.set_parent("A", "sg");

        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));

        lg.ranks[lg.node_index[&"sg".into()]] = 0;
        lg.ranks[lg.node_index[&"A".into()]] = 0;

        let config = LayoutConfig::default();
        run(&mut lg, &config);

        let sg_idx = lg.node_index[&"sg".into()];
        let a_idx = lg.node_index[&"A".into()];

        assert_eq!(lg.positions[sg_idx], Point::default());
        assert_ne!(lg.positions[a_idx], Point::default());
    }

    #[test]
    fn test_separate_sibling_content_is_disabled_when_rank_factor_set() {
        let mut g: DiGraph<()> = DiGraph::new();
        g.add_node("sg1", ());
        g.add_node("sg2", ());
        g.add_node("A", ());
        g.add_node("C", ());
        g.set_parent("A", "sg1");
        g.set_parent("C", "sg2");

        let mut lg = LayoutGraph::from_digraph(&g, |_, _| (10.0, 10.0));
        lg.node_rank_factor = Some(3);

        let a_idx = lg.node_index[&"A".into()];
        let c_idx = lg.node_index[&"C".into()];

        lg.positions[a_idx].x = 10.0;
        lg.positions[c_idx].x = 10.0;

        let config = LayoutConfig::default();
        separate_sibling_subgraph_content(&mut lg, &config);

        assert_eq!(lg.positions[a_idx].x, 10.0);
        assert_eq!(lg.positions[c_idx].x, 10.0);
    }

    #[test]
    fn test_calculate_dimensions() {
        let config = LayoutConfig {
            direction: Direction::TopBottom,
            node_sep: 10.0,
            rank_sep: 20.0,
            margin: 5.0,
            ..Default::default()
        };

        let lg = run_full_layout(&[("A", 100.0, 50.0)], &[], &config);

        let (width, height) = calculate_dimensions(&lg, &config);

        // Should be margin + node + margin
        assert!((width - 110.0).abs() < 0.01); // 5 + 100 + 5
        assert!((height - 60.0).abs() < 0.01); // 5 + 50 + 5
    }
}
