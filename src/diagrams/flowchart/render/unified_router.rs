//! Float-first unified routing preview helpers.
//!
//! This module routes edges in float space first, then optionally applies a
//! deterministic grid snap adapter for text-oriented consumption.

use super::route_policy::effective_edge_direction;
use super::routing_core::{build_orthogonal_path_float, normalize_orthogonal_route_contracts};
use crate::diagrams::flowchart::geometry::{FPoint, GraphGeometry, RoutedEdgeGeometry};
use crate::graph::Diagram;
use crate::graph::Direction;

/// Preview options for unified float-first routing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnifiedRoutingOptions {
    /// Keep existing behavior for backward edges while previewing forward routing.
    pub backward_fallback_to_hints: bool,
    /// Optional grid snap `(scale_x, scale_y)` applied after routing.
    pub grid_snap: Option<(f64, f64)>,
}

impl UnifiedRoutingOptions {
    /// Conservative preview: unified routing for forward edges only.
    pub(crate) fn preview() -> Self {
        Self {
            backward_fallback_to_hints: true,
            grid_snap: None,
        }
    }
}

/// Route all edges using float-first orthogonal routing.
pub(crate) fn route_edges_unified(
    _diagram: &Diagram,
    geometry: &GraphGeometry,
    options: UnifiedRoutingOptions,
) -> Vec<RoutedEdgeGeometry> {
    geometry
        .edges
        .iter()
        .map(|edge| {
            let is_backward = geometry.reversed_edges.contains(&edge.index);
            let edge_direction = effective_edge_direction(
                &geometry.node_directions,
                &edge.from,
                &edge.to,
                geometry.direction,
            );
            let route_direction = if is_backward && options.backward_fallback_to_hints {
                geometry.direction
            } else {
                edge_direction
            };
            let mut path = build_unified_path(edge, geometry, route_direction);

            if let Some((sx, sy)) = options.grid_snap {
                path = snap_path_to_grid(&path, sx, sy);
            }

            RoutedEdgeGeometry {
                index: edge.index,
                from: edge.from.clone(),
                to: edge.to.clone(),
                path,
                label_position: edge.label_position,
                is_backward,
                from_subgraph: edge.from_subgraph.clone(),
                to_subgraph: edge.to_subgraph.clone(),
            }
        })
        .collect()
}

fn build_unified_path(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
) -> Vec<FPoint> {
    let control_points = build_path_from_hints(edge, geometry);
    build_contracted_path(&control_points, direction)
}

pub(crate) fn build_path_from_hints(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
) -> Vec<FPoint> {
    if let Some(ref path) = edge.layout_path_hint {
        return path.clone();
    }

    let mut path = Vec::new();
    if let Some(from_node) = geometry.nodes.get(&edge.from) {
        path.push(FPoint::new(
            from_node.rect.center_x(),
            from_node.rect.center_y(),
        ));
    }
    path.extend(edge.waypoints.iter().copied());
    if let Some(to_node) = geometry.nodes.get(&edge.to) {
        path.push(FPoint::new(
            to_node.rect.center_x(),
            to_node.rect.center_y(),
        ));
    }
    path
}

/// Deterministically snap float path points onto a fixed grid.
pub(crate) fn snap_path_to_grid(path: &[FPoint], scale_x: f64, scale_y: f64) -> Vec<FPoint> {
    let sx = if scale_x.abs() < f64::EPSILON {
        1.0
    } else {
        scale_x.abs()
    };
    let sy = if scale_y.abs() < f64::EPSILON {
        1.0
    } else {
        scale_y.abs()
    };

    path.iter()
        .map(|p| FPoint::new((p.x / sx).round() * sx, (p.y / sy).round() * sy))
        .collect()
}

fn build_contracted_path(control_points: &[FPoint], direction: Direction) -> Vec<FPoint> {
    if control_points.len() < 2 {
        return control_points.to_vec();
    }

    let start = control_points[0];
    let end = control_points[control_points.len() - 1];
    let waypoints = &control_points[1..(control_points.len() - 1)];
    let orthogonal = build_orthogonal_path_float(start, end, direction, waypoints);
    normalize_orthogonal_route_contracts(&orthogonal, direction)
}
