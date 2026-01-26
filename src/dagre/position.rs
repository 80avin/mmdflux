//! Phase 4: Assign x, y coordinates to nodes.
//!
//! Implements a simplified coordinate assignment with layer centering.

use super::graph::LayoutGraph;
use super::rank;
use super::types::{Direction, LayoutConfig, Point};

/// Assign positions to all nodes.
pub fn run(graph: &mut LayoutGraph, config: &LayoutConfig) {
    let layers = rank::by_rank(graph);

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

    // Reverse coordinates if needed
    if config.direction.is_reversed() {
        reverse_positions(graph, config);
    }
}

fn assign_vertical(graph: &mut LayoutGraph, layers: &[Vec<usize>], config: &LayoutConfig) {
    if layers.is_empty() {
        return;
    }

    // Calculate max width per layer for centering
    let layer_widths: Vec<f64> = layers
        .iter()
        .map(|layer| {
            let content: f64 = layer.iter().map(|&n| graph.dimensions[n].0).sum();
            let spacing = if layer.len() > 1 {
                (layer.len() - 1) as f64 * config.node_sep
            } else {
                0.0
            };
            content + spacing
        })
        .collect();

    let max_width = layer_widths.iter().cloned().fold(0.0, f64::max);

    // Assign Y based on rank, X based on order within layer
    let mut y = config.margin;

    for (rank, layer) in layers.iter().enumerate() {
        let layer_width = layer_widths[rank];
        let start_x = config.margin + (max_width - layer_width) / 2.0;

        let mut x = start_x;
        for &node in layer {
            let (w, _h) = graph.dimensions[node];
            graph.positions[node] = Point { x, y };
            x += w + config.node_sep;
        }

        // Y advances by max height in this layer
        let max_height = layer
            .iter()
            .map(|&n| graph.dimensions[n].1)
            .fold(0.0, f64::max);
        y += max_height + config.rank_sep;
    }
}

fn assign_horizontal(graph: &mut LayoutGraph, layers: &[Vec<usize>], config: &LayoutConfig) {
    if layers.is_empty() {
        return;
    }

    // Calculate max height per layer for centering
    let layer_heights: Vec<f64> = layers
        .iter()
        .map(|layer| {
            let content: f64 = layer.iter().map(|&n| graph.dimensions[n].1).sum();
            let spacing = if layer.len() > 1 {
                (layer.len() - 1) as f64 * config.node_sep
            } else {
                0.0
            };
            content + spacing
        })
        .collect();

    let max_height = layer_heights.iter().cloned().fold(0.0, f64::max);

    // Assign X based on rank, Y based on order within layer
    let mut x = config.margin;

    for (rank, layer) in layers.iter().enumerate() {
        let layer_height = layer_heights[rank];
        let start_y = config.margin + (max_height - layer_height) / 2.0;

        let mut y = start_y;
        for &node in layer {
            let (_w, h) = graph.dimensions[node];
            graph.positions[node] = Point { x, y };
            y += h + config.node_sep;
        }

        // X advances by max width in this layer
        let max_width = layer
            .iter()
            .map(|&n| graph.dimensions[n].0)
            .fold(0.0, f64::max);
        x += max_width + config.rank_sep;
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
        rank::run(&mut lg);
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
