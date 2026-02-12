//! Float-first unified routing preview helpers.
//!
//! This module routes edges in float space first, then optionally applies a
//! deterministic grid snap adapter for text-oriented consumption.

use std::collections::{HashMap, HashSet};

use super::route_policy::effective_edge_direction;
use super::routing_core::{
    Face, Q1OverflowSide, build_orthogonal_path_float, normalize_orthogonal_route_contracts,
    q1_overflow_face_for_slot, q1_primary_face_capacity, q1_primary_target_face,
    q2_backward_channel_face, q4_rank_span_should_use_periphery, q4_required_periphery_detour,
    resolve_q1_q2_face_conflict,
};
use crate::diagram::RoutingPolicyToggles;
use crate::diagrams::flowchart::geometry::{
    EngineHints, FPoint, FRect, GraphGeometry, RoutedEdgeGeometry,
};
use crate::graph::{Diagram, Direction, Shape};

/// Preview options for unified float-first routing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnifiedRoutingOptions {
    /// Keep existing behavior for backward edges while previewing forward routing.
    pub backward_fallback_to_hints: bool,
    /// Optional grid snap `(scale_x, scale_y)` applied after routing.
    pub grid_snap: Option<(f64, f64)>,
    /// Policy toggles for staged rollout.
    pub policy_toggles: RoutingPolicyToggles,
}

impl UnifiedRoutingOptions {
    /// Conservative preview: unified routing for forward edges only.
    pub(crate) fn preview(policy_toggles: RoutingPolicyToggles) -> Self {
        Self {
            backward_fallback_to_hints: true,
            grid_snap: None,
            policy_toggles,
        }
    }
}

/// Route all edges using float-first orthogonal routing.
pub(crate) fn route_edges_unified(
    _diagram: &Diagram,
    geometry: &GraphGeometry,
    options: UnifiedRoutingOptions,
) -> Vec<RoutedEdgeGeometry> {
    let q1_target_conflict = if options.policy_toggles.q1_overflow {
        q1_target_overflow_context(geometry, geometry.direction)
    } else {
        Q1TargetOverflowContext::default()
    };
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
            let q1_policy_target_face = q1_target_conflict
                .target_face_for_edge
                .get(&edge.index)
                .copied();
            let target_overflowed = q1_target_conflict.overflow_targeted.contains(&edge.to);
            let target_has_backward_conflict = q1_target_conflict
                .targets_with_backward_inbound
                .contains(&edge.to);
            let rank_span = edge_rank_span(geometry, edge).unwrap_or(0);
            let q4_rank_span_active = options.policy_toggles.q4_rank_span_periphery
                && !is_backward
                && q4_rank_span_should_use_periphery(rank_span);
            let mut path = build_unified_path(
                edge,
                geometry,
                route_direction,
                is_backward,
                q1_policy_target_face,
                target_overflowed,
                target_has_backward_conflict,
                q4_rank_span_active,
                rank_span,
            );

            if let Some((sx, sy)) = options.grid_snap {
                path = snap_path_to_grid(&path, sx, sy);
            }
            let label_position = if options.policy_toggles.q3_label_revalidation {
                revalidate_label_anchor(edge.label_position, &path)
            } else {
                edge.label_position
            };

            RoutedEdgeGeometry {
                index: edge.index,
                from: edge.from.clone(),
                to: edge.to.clone(),
                path,
                label_position,
                is_backward,
                from_subgraph: edge.from_subgraph.clone(),
                to_subgraph: edge.to_subgraph.clone(),
            }
        })
        .collect()
}

const Q3_LABEL_REVALIDATION_MAX_DISTANCE: f64 = 2.0;
const POINT_EPS: f64 = 0.000_001;

fn revalidate_label_anchor(label_position: Option<FPoint>, path: &[FPoint]) -> Option<FPoint> {
    let Some(anchor) = label_position else {
        return route_derived_label_anchor(path);
    };
    let drift = distance_point_to_path(anchor, path);
    if drift <= Q3_LABEL_REVALIDATION_MAX_DISTANCE {
        return Some(anchor);
    }
    route_derived_label_anchor(path).or(Some(anchor))
}

fn route_derived_label_anchor(path: &[FPoint]) -> Option<FPoint> {
    if path.is_empty() {
        return None;
    }
    if path.len() == 1 {
        return path.first().copied();
    }

    let total_len: f64 = path
        .windows(2)
        .map(|segment| point_distance(segment[0], segment[1]))
        .sum();
    if total_len <= POINT_EPS {
        return path.get(path.len() / 2).copied();
    }

    let target = total_len / 2.0;
    let mut traversed = 0.0;
    for segment in path.windows(2) {
        let a = segment[0];
        let b = segment[1];
        let seg_len = point_distance(a, b);
        if seg_len <= POINT_EPS {
            continue;
        }
        if traversed + seg_len >= target {
            let t = (target - traversed) / seg_len;
            return Some(FPoint::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
        traversed += seg_len;
    }
    path.last().copied()
}

fn point_distance(a: FPoint, b: FPoint) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn distance_point_to_segment(point: FPoint, a: FPoint, b: FPoint) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let seg_len_sq = dx * dx + dy * dy;
    if seg_len_sq <= POINT_EPS {
        return point_distance(point, a);
    }
    let projection = ((point.x - a.x) * dx + (point.y - a.y) * dy) / seg_len_sq;
    let t = projection.clamp(0.0, 1.0);
    let closest = FPoint::new(a.x + t * dx, a.y + t * dy);
    point_distance(point, closest)
}

fn distance_point_to_path(point: FPoint, path: &[FPoint]) -> f64 {
    if path.is_empty() {
        return f64::INFINITY;
    }
    if path.len() == 1 {
        return point_distance(point, path[0]);
    }
    path.windows(2)
        .map(|segment| distance_point_to_segment(point, segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn build_unified_path(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    is_backward: bool,
    q1_policy_target_face: Option<Face>,
    target_overflowed: bool,
    target_has_backward_conflict: bool,
    q4_rank_span_active: bool,
    q4_rank_span: usize,
) -> Vec<FPoint> {
    let control_points = build_path_from_hints(edge, geometry);
    let mut path = build_contracted_path(&control_points, direction);
    anchor_path_endpoints_to_endpoint_faces(
        &mut path,
        edge,
        geometry,
        direction,
        is_backward,
        q1_policy_target_face,
        target_overflowed,
        target_has_backward_conflict,
    );
    prefer_lateral_exit_for_off_center_primary_axis_sources(
        &mut path,
        edge,
        geometry,
        direction,
        is_backward,
    );
    ensure_endpoint_segments_axis_aligned(&mut path);
    collapse_source_turnback_spikes(&mut path);
    if !is_backward {
        enforce_primary_axis_terminal_direction(&mut path, direction, 8.0);
    }
    let mut normalized = normalize_orthogonal_route_contracts(&path, direction);
    if is_backward {
        ensure_backward_outer_lane_clearance(&mut normalized, direction, 12.0);
    }
    if q4_rank_span_active {
        let required_detour = q4_required_periphery_detour(q4_rank_span);
        apply_q4_rank_span_periphery_detour(&mut normalized, direction, required_detour);
        if path_has_diagonal_segments(&normalized) {
            normalized = build_contracted_path(&normalized, direction);
            anchor_path_endpoints_to_endpoint_faces(
                &mut normalized,
                edge,
                geometry,
                direction,
                is_backward,
                q1_policy_target_face,
                target_overflowed,
                target_has_backward_conflict,
            );
            ensure_endpoint_segments_axis_aligned(&mut normalized);
        }
    }
    collapse_source_turnback_spikes(&mut normalized);
    let mut finalized = normalize_orthogonal_route_contracts(&normalized, direction);
    if is_backward {
        enforce_backward_terminal_tangent_direction(&mut finalized, edge, geometry, direction);
    }
    finalized
}

#[derive(Default)]
struct Q1TargetOverflowContext {
    target_face_for_edge: HashMap<usize, Face>,
    overflow_targeted: HashSet<String>,
    targets_with_backward_inbound: HashSet<String>,
}

fn q1_target_overflow_context(
    geometry: &GraphGeometry,
    direction: Direction,
) -> Q1TargetOverflowContext {
    let mut incoming_by_target: HashMap<
        String,
        Vec<&crate::diagrams::flowchart::geometry::LayoutEdge>,
    > = HashMap::new();
    for edge in &geometry.edges {
        incoming_by_target
            .entry(edge.to.clone())
            .or_default()
            .push(edge);
    }

    let capacity = q1_primary_face_capacity(direction);
    let primary_face = q1_primary_target_face(direction);
    let mut target_face_for_edge: HashMap<usize, Face> = HashMap::new();
    let mut overflow_targeted: HashSet<String> = HashSet::new();
    let mut targets_with_backward_inbound: HashSet<String> = HashSet::new();

    for (target_id, mut incoming_edges) in incoming_by_target {
        incoming_edges.sort_unstable_by_key(|edge| edge.index);
        let mut forward_edges: Vec<&crate::diagrams::flowchart::geometry::LayoutEdge> = Vec::new();
        let mut backward_edge_count = 0usize;
        for edge in incoming_edges {
            if geometry.reversed_edges.contains(&edge.index) {
                backward_edge_count += 1;
            } else {
                forward_edges.push(edge);
            }
        }

        if backward_edge_count > 0 {
            targets_with_backward_inbound.insert(target_id.clone());
        }

        if forward_edges.len() <= 1 {
            continue;
        }

        let primary_count = forward_edges.len().min(capacity);
        for edge in &forward_edges[..primary_count] {
            target_face_for_edge.insert(edge.index, primary_face);
        }

        if forward_edges.len() <= capacity {
            continue;
        }

        overflow_targeted.insert(target_id);
        let overflow_edges = &forward_edges[capacity..];
        for (idx, edge) in overflow_edges.iter().enumerate() {
            let overflow_slot = if idx % 2 == 0 {
                Q1OverflowSide::LeftOrTop
            } else {
                Q1OverflowSide::RightOrBottom
            };
            let face = q1_overflow_face_for_slot(direction, overflow_slot);
            target_face_for_edge.insert(edge.index, face);
        }
    }

    Q1TargetOverflowContext {
        target_face_for_edge,
        overflow_targeted,
        targets_with_backward_inbound,
    }
}

fn edge_rank_span(
    geometry: &GraphGeometry,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
) -> Option<usize> {
    let EngineHints::Dagre(hints) = geometry.engine_hints.as_ref()?;
    let src_rank = *hints.node_ranks.get(&edge.from)?;
    let dst_rank = *hints.node_ranks.get(&edge.to)?;
    Some(src_rank.abs_diff(dst_rank) as usize)
}

fn apply_q4_rank_span_periphery_detour(
    path: &mut [FPoint],
    direction: Direction,
    required_detour: f64,
) {
    const EPS: f64 = 0.000_001;
    if path.len() < 3 || required_detour <= 0.0 {
        return;
    }

    let last = path.len() - 1;
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            let baseline_min = path[0].x.min(path[last].x);
            let baseline_max = path[0].x.max(path[last].x);
            let route_min = path.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
            let route_max = path
                .iter()
                .map(|point| point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let detour_left = baseline_min - route_min;
            let detour_right = route_max - baseline_max;

            let interior_at_min: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.x - route_min).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();
            let interior_at_max: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.x - route_max).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();

            let can_expand_left = !interior_at_min.is_empty();
            let can_expand_right = !interior_at_max.is_empty();
            if !can_expand_left && !can_expand_right {
                return;
            }

            let mut expand_right = detour_right >= detour_left;
            if expand_right && !can_expand_right && can_expand_left {
                expand_right = false;
            } else if !expand_right && !can_expand_left && can_expand_right {
                expand_right = true;
            }

            if expand_right {
                if detour_right + EPS >= required_detour {
                    return;
                }
                let target_x = baseline_max + required_detour;
                for idx in interior_at_max {
                    path[idx].x = target_x;
                }
            } else {
                if detour_left + EPS >= required_detour {
                    return;
                }
                let target_x = baseline_min - required_detour;
                for idx in interior_at_min {
                    path[idx].x = target_x;
                }
            }
        }
        Direction::LeftRight | Direction::RightLeft => {
            let baseline_min = path[0].y.min(path[last].y);
            let baseline_max = path[0].y.max(path[last].y);
            let route_min = path.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
            let route_max = path
                .iter()
                .map(|point| point.y)
                .fold(f64::NEG_INFINITY, f64::max);
            let detour_top = baseline_min - route_min;
            let detour_bottom = route_max - baseline_max;

            let interior_at_min: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.y - route_min).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();
            let interior_at_max: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.y - route_max).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();

            let can_expand_top = !interior_at_min.is_empty();
            let can_expand_bottom = !interior_at_max.is_empty();
            if !can_expand_top && !can_expand_bottom {
                return;
            }

            let mut expand_bottom = detour_bottom >= detour_top;
            if expand_bottom && !can_expand_bottom && can_expand_top {
                expand_bottom = false;
            } else if !expand_bottom && !can_expand_top && can_expand_bottom {
                expand_bottom = true;
            }

            if expand_bottom {
                if detour_bottom + EPS >= required_detour {
                    return;
                }
                let target_y = baseline_max + required_detour;
                for idx in interior_at_max {
                    path[idx].y = target_y;
                }
            } else {
                if detour_top + EPS >= required_detour {
                    return;
                }
                let target_y = baseline_min - required_detour;
                for idx in interior_at_min {
                    path[idx].y = target_y;
                }
            }
        }
    }
}

fn path_has_diagonal_segments(path: &[FPoint]) -> bool {
    const EPS: f64 = 0.000_001;
    path.windows(2).any(|segment| {
        let a = segment[0];
        let b = segment[1];
        (a.x - b.x).abs() > EPS && (a.y - b.y).abs() > EPS
    })
}

fn prefer_lateral_exit_for_off_center_primary_axis_sources(
    path: &mut [FPoint],
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    is_backward: bool,
) {
    const MIN_OFF_CENTER_ABS: f64 = 1.0;
    const SEG_EPS: f64 = 0.000_001;

    if is_backward
        || path.len() < 4
        || !matches!(direction, Direction::TopDown | Direction::BottomTop)
    {
        return;
    }

    let Some((source_rect, _source_shape)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
    else {
        return;
    };

    let start = path[0];
    let first = path[1];
    let second = path[2];
    let center_x = source_rect.x + source_rect.width / 2.0;
    let source_offset = (start.x - center_x).abs();
    if source_offset < MIN_OFF_CENTER_ABS {
        return;
    }

    let first_is_vertical =
        (start.x - first.x).abs() <= SEG_EPS && (start.y - first.y).abs() > SEG_EPS;
    let second_is_horizontal =
        (first.y - second.y).abs() <= SEG_EPS && (first.x - second.x).abs() > SEG_EPS;
    if !first_is_vertical || !second_is_horizontal {
        return;
    }

    let progresses_along_primary = match direction {
        Direction::TopDown => first.y > start.y + SEG_EPS,
        Direction::BottomTop => first.y < start.y - SEG_EPS,
        _ => false,
    };
    if !progresses_along_primary {
        return;
    }

    let lateral_delta = second.x - first.x;
    if lateral_delta.abs() <= SEG_EPS {
        return;
    }
    let outward_sign = (start.x - center_x).signum();
    if outward_sign.abs() <= SEG_EPS || lateral_delta.signum() != outward_sign {
        return;
    }

    let replacement = FPoint::new(second.x, start.y);
    if (replacement.x - start.x).abs() <= SEG_EPS || (replacement.y - second.y).abs() <= SEG_EPS {
        return;
    }

    path[1] = replacement;
}

fn collapse_source_turnback_spikes(path: &mut Vec<FPoint>) {
    const SEG_EPS: f64 = 0.000_001;
    if path.len() < 4 {
        return;
    }

    let start = path[0];
    let step = path[1];
    let back = path[2];

    let out_is_axis = (start.x - step.x).abs() <= SEG_EPS || (start.y - step.y).abs() <= SEG_EPS;
    let back_is_axis = (step.x - back.x).abs() <= SEG_EPS || (step.y - back.y).abs() <= SEG_EPS;
    if !out_is_axis || !back_is_axis {
        return;
    }
    if points_match(start, back) {
        let resume = path[3];
        let collapsed_is_axis =
            (start.x - resume.x).abs() <= SEG_EPS || (start.y - resume.y).abs() <= SEG_EPS;
        if collapsed_is_axis && !points_match(start, resume) {
            path.drain(1..3);
        }
    }
}

fn ensure_backward_outer_lane_clearance(
    path: &mut [FPoint],
    direction: Direction,
    min_clearance: f64,
) {
    const EPS: f64 = 0.000_001;
    if path.len() < 3 || min_clearance <= 0.0 {
        return;
    }

    let last = path.len() - 1;
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            let baseline = path[0].x.max(path[last].x);
            let route_max = path
                .iter()
                .map(|point| point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            if route_max - baseline + EPS >= min_clearance {
                return;
            }
            let interior_at_max: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.x - route_max).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();
            if interior_at_max.is_empty() {
                return;
            }
            let target_x = baseline + min_clearance;
            for idx in interior_at_max {
                path[idx].x = target_x;
            }
        }
        Direction::LeftRight | Direction::RightLeft => {
            let baseline = path[0].y.max(path[last].y);
            let route_max = path
                .iter()
                .map(|point| point.y)
                .fold(f64::NEG_INFINITY, f64::max);
            if route_max - baseline + EPS >= min_clearance {
                return;
            }
            let interior_at_max: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(idx, point)| {
                    *idx > 0 && *idx < last && (point.y - route_max).abs() <= EPS
                })
                .map(|(idx, _)| idx)
                .collect();
            if interior_at_max.is_empty() {
                return;
            }
            let target_y = baseline + min_clearance;
            for idx in interior_at_max {
                path[idx].y = target_y;
            }
        }
    }
}

fn enforce_backward_terminal_tangent_direction(
    path: &mut [FPoint],
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
) {
    const EPS: f64 = 0.000_001;
    const TANGENT_STEP: f64 = 8.0;
    if path.len() < 2 || !matches!(direction, Direction::LeftRight | Direction::RightLeft) {
        return;
    }

    let Some((target_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
    else {
        return;
    };

    let last = path.len() - 1;
    let prev = path[last - 1];
    let end = path[last];
    if (prev.y - end.y).abs() > EPS {
        return;
    }

    let left = target_rect.x + 1.0;
    let right = target_rect.x + target_rect.width - 1.0;
    if left >= right {
        return;
    }

    match direction {
        Direction::LeftRight if end.x >= prev.x - EPS => {
            let target_x = (prev.x - TANGENT_STEP).clamp(left, right);
            if target_x < prev.x - EPS {
                path[last].x = target_x;
            }
        }
        Direction::RightLeft if end.x <= prev.x + EPS => {
            let target_x = (prev.x + TANGENT_STEP).clamp(left, right);
            if target_x > prev.x + EPS {
                path[last].x = target_x;
            }
        }
        _ => {}
    }
}

pub(crate) fn build_path_from_hints(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
) -> Vec<FPoint> {
    if let Some(ref path) = edge.layout_path_hint {
        if hint_has_non_degenerate_span(path)
            && hint_endpoints_attach_to_layout_bounds(edge, geometry, path)
        {
            return path.clone();
        }

        let fallback = build_path_from_nodes_and_waypoints(edge, geometry);
        if fallback.len() >= 2 {
            return fallback;
        }

        return path.clone();
    }

    build_path_from_nodes_and_waypoints(edge, geometry)
}

fn build_path_from_nodes_and_waypoints(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
) -> Vec<FPoint> {
    let mut path = Vec::new();
    if let Some(from_node) = geometry.nodes.get(&edge.from) {
        let center = rect_center(&from_node.rect);
        path.push(FPoint::new(center.x, center.y));
    }
    path.extend(edge.waypoints.iter().copied());
    if let Some(to_node) = geometry.nodes.get(&edge.to) {
        let center = rect_center(&to_node.rect);
        path.push(FPoint::new(center.x, center.y));
    }
    path
}

fn hint_has_non_degenerate_span(path: &[FPoint]) -> bool {
    if path.len() < 2 {
        return false;
    }
    path.windows(2).any(|segment| {
        let a = segment[0];
        let b = segment[1];
        (a.x - b.x).abs() > f64::EPSILON || (a.y - b.y).abs() > f64::EPSILON
    })
}

fn hint_endpoints_attach_to_layout_bounds(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    path: &[FPoint],
) -> bool {
    const MAX_HINT_ENDPOINT_DRIFT: f64 = 20.0;
    if path.len() < 2 {
        return false;
    }

    let Some(from_rect) = endpoint_rect(geometry, &edge.from, edge.from_subgraph.as_deref()) else {
        return false;
    };
    let Some(to_rect) = endpoint_rect(geometry, &edge.to, edge.to_subgraph.as_deref()) else {
        return false;
    };

    let start = path[0];
    let end = path[path.len() - 1];
    point_on_or_inside_rect(start, from_rect, MAX_HINT_ENDPOINT_DRIFT)
        && point_on_or_inside_rect(end, to_rect, MAX_HINT_ENDPOINT_DRIFT)
}

fn endpoint_rect<'a>(
    geometry: &'a GraphGeometry,
    node_id: &str,
    subgraph_id: Option<&str>,
) -> Option<&'a crate::diagrams::flowchart::geometry::FRect> {
    if let Some(sg_id) = subgraph_id {
        geometry.subgraphs.get(sg_id).map(|sg| &sg.rect)
    } else {
        geometry.nodes.get(node_id).map(|node| &node.rect)
    }
}

fn rect_center(rect: &crate::diagrams::flowchart::geometry::FRect) -> FPoint {
    FPoint::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn point_on_or_inside_rect(
    point: FPoint,
    rect: &crate::diagrams::flowchart::geometry::FRect,
    eps: f64,
) -> bool {
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    point.x >= left - eps
        && point.x <= right + eps
        && point.y >= top - eps
        && point.y <= bottom + eps
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

fn anchor_path_endpoints_to_endpoint_faces(
    path: &mut [FPoint],
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    is_backward: bool,
    q1_policy_target_face: Option<Face>,
    target_overflowed: bool,
    target_has_backward_conflict: bool,
) {
    const EPS: f64 = 0.5;
    if path.len() < 2 {
        return;
    }

    if let Some((from_rect, from_shape)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
        && !matches!(from_shape, Shape::Diamond | Shape::Hexagon)
    {
        let start = path[0];
        let next = path[1];
        if point_on_or_inside_rect(start, &from_rect, EPS) {
            path[0] = clip_point_to_axis_face(
                start,
                next,
                from_rect,
                direction,
                is_backward,
                false,
                None,
                false,
                false,
            );
        }
    }

    if let Some((to_rect, to_shape)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
        && !matches!(to_shape, Shape::Diamond | Shape::Hexagon)
    {
        let last = path.len() - 1;
        let end = path[last];
        let prev = path[last - 1];
        if point_on_or_inside_rect(end, &to_rect, EPS) {
            path[last] = clip_point_to_axis_face(
                end,
                prev,
                to_rect,
                direction,
                is_backward,
                true,
                q1_policy_target_face,
                target_overflowed,
                target_has_backward_conflict,
            );
        }
    }
}

fn endpoint_rect_and_shape(
    geometry: &GraphGeometry,
    node_id: &str,
    subgraph_id: Option<&str>,
) -> Option<(FRect, Shape)> {
    if let Some(sg_id) = subgraph_id {
        return geometry
            .subgraphs
            .get(sg_id)
            .map(|sg| (sg.rect, Shape::Rectangle));
    }
    geometry
        .nodes
        .get(node_id)
        .map(|node| (node.rect, node.shape))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RectFace {
    Top,
    Bottom,
    Left,
    Right,
}

fn boundary_face_excluding_corners(point: FPoint, rect: FRect, eps: f64) -> Option<RectFace> {
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let on_left = (point.x - left).abs() <= eps;
    let on_right = (point.x - right).abs() <= eps;
    let on_top = (point.y - top).abs() <= eps;
    let on_bottom = (point.y - bottom).abs() <= eps;

    let within_x = point.x > left + eps && point.x < right - eps;
    let within_y = point.y > top + eps && point.y < bottom - eps;

    if on_left && within_y {
        Some(RectFace::Left)
    } else if on_right && within_y {
        Some(RectFace::Right)
    } else if on_top && within_x {
        Some(RectFace::Top)
    } else if on_bottom && within_x {
        Some(RectFace::Bottom)
    } else {
        None
    }
}

fn clip_point_to_axis_face(
    endpoint: FPoint,
    adjacent: FPoint,
    rect: FRect,
    direction: Direction,
    preserve_existing_face: bool,
    is_target_endpoint: bool,
    q1_policy_face: Option<Face>,
    target_overflowed: bool,
    target_has_backward_conflict: bool,
) -> FPoint {
    const EPS: f64 = 0.000_001;
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let x_min = left.min(right);
    let x_max = left.max(right);
    let y_min = top.min(bottom);
    let y_max = top.max(bottom);

    let dx = endpoint.x - adjacent.x;
    let dy = endpoint.y - adjacent.y;

    if preserve_existing_face && is_target_endpoint {
        let canonical_face = q2_backward_channel_face(direction);
        let resolved_face = resolve_q1_q2_face_conflict(
            direction,
            true,
            target_has_backward_conflict,
            q1_policy_face,
            canonical_face,
        );
        let mut clipped =
            clip_point_to_rect_face(endpoint, rect, map_face_to_rect_face(resolved_face));
        if matches!(resolved_face, Face::Bottom) {
            let left = rect.x + 1.0;
            let right = rect.x + rect.width - 1.0;
            match direction {
                Direction::LeftRight => {
                    clipped.x = (adjacent.x - 1.0).clamp(left, right);
                }
                Direction::RightLeft => {
                    clipped.x = (adjacent.x + 1.0).clamp(left, right);
                }
                _ => {}
            }
        }
        return clipped;
    }

    if let Some(policy_face) = q1_policy_face {
        let terminal_is_horizontal = dy.abs() <= EPS && dx.abs() > EPS;
        let terminal_is_vertical = dx.abs() <= EPS && dy.abs() > EPS;
        let policy_face_is_compatible = match policy_face {
            Face::Left | Face::Right => terminal_is_horizontal,
            Face::Top | Face::Bottom => terminal_is_vertical,
        };
        let resolved_face = resolve_q1_q2_face_conflict(
            direction,
            false,
            target_has_backward_conflict,
            Some(policy_face),
            policy_face,
        );

        if policy_face_is_compatible
            || matches!(direction, Direction::TopDown | Direction::BottomTop)
        {
            let resolved_rect_face = map_face_to_rect_face(resolved_face);
            return clip_point_to_rect_face(endpoint, rect, resolved_rect_face);
        }

        let fallback_face = if terminal_is_horizontal || dx.abs() >= dy.abs() {
            if adjacent.x < endpoint.x {
                Face::Left
            } else {
                Face::Right
            }
        } else if adjacent.y < endpoint.y {
            Face::Top
        } else {
            Face::Bottom
        };
        let resolved_fallback = resolve_q1_q2_face_conflict(
            direction,
            false,
            target_has_backward_conflict,
            Some(policy_face),
            fallback_face,
        );
        return clip_point_to_rect_face(endpoint, rect, map_face_to_rect_face(resolved_fallback));
    }

    if !preserve_existing_face && target_overflowed {
        let canonical = map_face_to_rect_face(q2_backward_channel_face(direction));
        if let Some(actual_face) = boundary_face_excluding_corners(endpoint, rect, 0.5)
            && actual_face == canonical
        {
            return clip_point_to_rect_face(endpoint, rect, opposite_rect_face(canonical));
        }
    }

    // Backward hints often already carry intended side-face attachment.
    // Preserve that face when the endpoint is unambiguously on a non-corner
    // boundary position instead of forcing axis-derived top/bottom clipping.
    // For backward TD/BT edges, preserve side-entry/exit intent carried by
    // hint endpoints. This prevents collapsing to bottom corners while keeping
    // LR/RL backward behavior unchanged.
    if preserve_existing_face && matches!(direction, Direction::TopDown | Direction::BottomTop) {
        if let Some(face) = boundary_face_excluding_corners(endpoint, rect, 0.5)
            && matches!(face, RectFace::Left | RectFace::Right)
        {
            return match face {
                RectFace::Left => FPoint::new(left, endpoint.y.clamp(y_min, y_max)),
                RectFace::Right => FPoint::new(right, endpoint.y.clamp(y_min, y_max)),
                RectFace::Top | RectFace::Bottom => unreachable!("matched above"),
            };
        }

        let dist_left = (endpoint.x - left).abs();
        let dist_right = (endpoint.x - right).abs();
        let side_bias_threshold = (rect.width * 0.2).clamp(1.0, 6.0);
        if dist_left.min(dist_right) <= side_bias_threshold {
            let x = if adjacent.x < endpoint.x {
                left
            } else if adjacent.x > endpoint.x {
                right
            } else if dist_left <= dist_right {
                left
            } else {
                right
            };
            let mut y = endpoint.y.clamp(y_min, y_max);
            if (y - top).abs() <= EPS || (y - bottom).abs() <= EPS {
                y = (top + bottom) / 2.0;
            }
            return FPoint::new(x, y);
        }
    }

    // Terminal segment is horizontal: anchor endpoint on left/right face.
    if dy.abs() <= EPS && dx.abs() > EPS {
        let x = if adjacent.x < endpoint.x { left } else { right };
        return FPoint::new(x, endpoint.y.clamp(y_min, y_max));
    }

    // Terminal segment is vertical: anchor endpoint on top/bottom face.
    if dx.abs() <= EPS && dy.abs() > EPS {
        let y = if adjacent.y < endpoint.y { top } else { bottom };
        return FPoint::new(endpoint.x.clamp(x_min, x_max), y);
    }

    // Fallback: clamp interior drift to the rectangle boundary box.
    FPoint::new(
        endpoint.x.clamp(x_min, x_max),
        endpoint.y.clamp(y_min, y_max),
    )
}

fn map_face_to_rect_face(face: Face) -> RectFace {
    match face {
        Face::Top => RectFace::Top,
        Face::Bottom => RectFace::Bottom,
        Face::Left => RectFace::Left,
        Face::Right => RectFace::Right,
    }
}

fn opposite_rect_face(face: RectFace) -> RectFace {
    match face {
        RectFace::Top => RectFace::Bottom,
        RectFace::Bottom => RectFace::Top,
        RectFace::Left => RectFace::Right,
        RectFace::Right => RectFace::Left,
    }
}

fn clip_point_to_rect_face(endpoint: FPoint, rect: FRect, face: RectFace) -> FPoint {
    const FACE_CORNER_MARGIN: f64 = 1.0;

    fn clamp_interior(value: f64, min: f64, max: f64) -> f64 {
        let lo = min.min(max);
        let hi = min.max(max);
        if (hi - lo) <= FACE_CORNER_MARGIN * 2.0 {
            return (lo + hi) / 2.0;
        }
        value.clamp(lo + FACE_CORNER_MARGIN, hi - FACE_CORNER_MARGIN)
    }

    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    match face {
        RectFace::Top => FPoint::new(clamp_interior(endpoint.x, left, right), top),
        RectFace::Bottom => FPoint::new(clamp_interior(endpoint.x, left, right), bottom),
        RectFace::Left => FPoint::new(left, clamp_interior(endpoint.y, top, bottom)),
        RectFace::Right => FPoint::new(right, clamp_interior(endpoint.y, top, bottom)),
    }
}

fn ensure_endpoint_segments_axis_aligned(path: &mut Vec<FPoint>) {
    if path.len() < 2 {
        return;
    }

    ensure_endpoint_axis_aligned(path, true);
    ensure_endpoint_axis_aligned(path, false);
}

fn ensure_endpoint_axis_aligned(path: &mut Vec<FPoint>, at_start: bool) {
    const EPS: f64 = 0.000_001;
    if path.len() < 2 {
        return;
    }

    let (anchor_idx, adjacent_idx) = if at_start {
        (0usize, 1usize)
    } else {
        let n = path.len();
        (n - 1, n - 2)
    };

    let anchor = path[anchor_idx];
    let adjacent = path[adjacent_idx];
    if (anchor.x - adjacent.x).abs() <= EPS || (anchor.y - adjacent.y).abs() <= EPS {
        return;
    }

    let mut elbow = FPoint::new(anchor.x, adjacent.y);
    let mut use_fallback = points_match(elbow, anchor) || points_match(elbow, adjacent);
    if use_fallback {
        elbow = FPoint::new(adjacent.x, anchor.y);
        use_fallback = points_match(elbow, anchor) || points_match(elbow, adjacent);
    }
    if use_fallback {
        return;
    }

    if at_start {
        path.insert(1, elbow);
    } else {
        let insert_at = path.len() - 1;
        path.insert(insert_at, elbow);
    }
}

fn points_match(a: FPoint, b: FPoint) -> bool {
    const EPS: f64 = 0.000_001;
    (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS
}

fn enforce_primary_axis_terminal_direction(
    points: &mut [FPoint],
    direction: Direction,
    min_terminal_support: f64,
) {
    if points.len() < 2 || min_terminal_support <= 0.0 {
        return;
    }

    let n = points.len();
    let end_idx = n - 1;
    let penult_idx = n - 2;

    match direction {
        Direction::TopDown => {
            let target_penult_y = points[end_idx].y - min_terminal_support;
            if points[penult_idx].y > target_penult_y {
                points[penult_idx].y = target_penult_y;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].y > points[penult_idx].y {
                    points[pre_idx].y = points[penult_idx].y;
                }
            }
        }
        Direction::BottomTop => {
            let target_penult_y = points[end_idx].y + min_terminal_support;
            if points[penult_idx].y < target_penult_y {
                points[penult_idx].y = target_penult_y;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].y < points[penult_idx].y {
                    points[pre_idx].y = points[penult_idx].y;
                }
            }
        }
        Direction::LeftRight => {
            let target_penult_x = points[end_idx].x - min_terminal_support;
            if points[penult_idx].x > target_penult_x {
                points[penult_idx].x = target_penult_x;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].x > points[penult_idx].x {
                    points[pre_idx].x = points[penult_idx].x;
                }
            }
        }
        Direction::RightLeft => {
            let target_penult_x = points[end_idx].x + min_terminal_support;
            if points[penult_idx].x < target_penult_x {
                points[penult_idx].x = target_penult_x;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].x < points[penult_idx].x {
                    points[pre_idx].x = points[penult_idx].x;
                }
            }
        }
    }
}
