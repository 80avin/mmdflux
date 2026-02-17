//! Float-first unified routing preview helpers.
//!
//! This module routes edges in float space first, then optionally applies a
//! deterministic grid snap adapter for text-oriented consumption.

use std::collections::{HashMap, HashSet};

use super::route_policy::effective_edge_direction;
use super::routing_core::{
    Face, Q1OverflowSide, build_orthogonal_path_float, intersect_shape_boundary_float,
    normalize_orthogonal_route_contracts, q1_overflow_face_for_slot, q1_primary_face_capacity,
    q1_primary_target_face, q2_backward_channel_face, q4_rank_span_should_use_periphery,
    q4_required_periphery_detour, resolve_q1_q2_face_conflict,
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
const MIN_PORT_CORNER_INSET_FORWARD: f64 = 8.0;
const MIN_PORT_CORNER_INSET_BACKWARD: f64 = 12.0;

fn clamp_face_coordinate_with_corner_inset(value: f64, min: f64, max: f64, max_inset: f64) -> f64 {
    let lo = min.min(max);
    let hi = min.max(max);
    let span = hi - lo;
    if span <= POINT_EPS {
        (lo + hi) / 2.0
    } else {
        // Scale inset with face span so text-sized nodes keep usable side lanes,
        // while SVG-sized nodes still enforce a visibly distinct stem offset.
        let inset = (span * 0.2).clamp(1.0, max_inset);
        if span <= inset * 2.0 {
            (lo + hi) / 2.0
        } else {
            value.clamp(lo + inset, hi - inset)
        }
    }
}

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

#[allow(clippy::too_many_arguments)]
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
    let (backward_source_face_override, backward_target_face_override) =
        backward_td_bt_face_overrides(edge, geometry, direction, is_backward, target_overflowed);
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
    ensure_primary_stem_for_flat_off_center_fanout_sources(
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
    let base_finalized = normalize_orthogonal_route_contracts(&normalized, direction);
    let mut finalized = base_finalized.clone();
    if is_backward {
        enforce_backward_source_tangent_direction(
            &mut finalized,
            edge,
            geometry,
            direction,
            backward_source_face_override,
        );
        ensure_backward_outer_lane_clearance(&mut finalized, direction, 12.0);
        align_backward_source_stem_to_outer_lane(&mut finalized, edge, geometry, direction);
        enforce_backward_terminal_tangent_direction(
            &mut finalized,
            edge,
            geometry,
            direction,
            target_overflowed,
            backward_target_face_override,
        );
        let parity_override_active =
            backward_source_face_override.is_some() || backward_target_face_override.is_some();
        if parity_override_active {
            finalized = normalize_orthogonal_route_contracts(&finalized, direction);
        }
        if parity_override_active && has_immediate_axial_turnback(&finalized) {
            finalized = base_finalized;
            enforce_backward_source_tangent_direction(
                &mut finalized,
                edge,
                geometry,
                direction,
                None,
            );
            ensure_backward_outer_lane_clearance(&mut finalized, direction, 12.0);
            align_backward_source_stem_to_outer_lane(&mut finalized, edge, geometry, direction);
            enforce_backward_terminal_tangent_direction(
                &mut finalized,
                edge,
                geometry,
                direction,
                target_overflowed,
                None,
            );
        }
        collapse_tiny_backward_terminal_staircase(&mut finalized, direction, 8.0);
        align_backward_outer_lane_to_hint(
            &mut finalized,
            edge.layout_path_hint.as_deref(),
            direction,
            edge,
            geometry,
        );
        collapse_tiny_backward_terminal_staircase(&mut finalized, direction, 8.0);
        enforce_backward_terminal_corner_inset(&mut finalized, edge, geometry);
        let canonical_terminal_face =
            backward_target_face_override.unwrap_or_else(|| q2_backward_channel_face(direction));
        if let Some((target_rect, _)) =
            endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
        {
            expand_compact_td_bt_backward_terminal_return(
                &mut finalized,
                canonical_terminal_face,
                direction,
                target_rect,
            );
        }
        if matches!(direction, Direction::LeftRight | Direction::RightLeft) {
            collapse_collinear_interior_points(&mut finalized);
        }
    }
    finalized
}

fn backward_td_bt_face_overrides(
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    is_backward: bool,
    _target_overflowed: bool,
) -> (Option<Face>, Option<Face>) {
    const MIN_OVERRIDE_RECT_SPAN: f64 = 20.0;
    if !is_backward || !matches!(direction, Direction::TopDown | Direction::BottomTop) {
        return (None, None);
    }
    if edge.from_subgraph.is_some() || edge.to_subgraph.is_some() {
        return (None, None);
    }
    let hint = edge.layout_path_hint.as_ref();
    let Some(hint) = hint else {
        return (None, None);
    };
    if hint.len() < 2 {
        return (None, None);
    }

    let Some((source_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
    else {
        return (None, None);
    };
    let Some((target_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
    else {
        return (None, None);
    };
    // Restrict parity overrides to geometry with stable floating-point extents.
    // Tiny text-grid rectangles are too coarse and can overfit corner hints.
    if source_rect.width < MIN_OVERRIDE_RECT_SPAN
        || source_rect.height < MIN_OVERRIDE_RECT_SPAN
        || target_rect.width < MIN_OVERRIDE_RECT_SPAN
        || target_rect.height < MIN_OVERRIDE_RECT_SPAN
    {
        return (None, None);
    }

    let source_hint = hint[0];
    let target_hint = hint[hint.len() - 1];
    let source_override = hint_face_for_td_bt_parity(source_hint, source_rect)
        .filter(|face| matches!(face, Face::Top | Face::Bottom));
    let target_override = hint_face_for_td_bt_parity(target_hint, target_rect)
        .filter(|face| matches!(face, Face::Top | Face::Bottom));
    if target_override.is_none() {
        return (None, None);
    }

    (source_override, target_override)
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
            let route_min = path
                .iter()
                .map(|point| point.x)
                .fold(f64::INFINITY, f64::min);
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
            let route_min = path
                .iter()
                .map(|point| point.y)
                .fold(f64::INFINITY, f64::min);
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

fn ensure_primary_stem_for_flat_off_center_fanout_sources(
    path: &mut Vec<FPoint>,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    is_backward: bool,
) {
    const MIN_OFF_CENTER_ABS: f64 = 1.0;
    const MIN_PRIMARY_STEM: f64 = 8.0;
    const FANOUT_LANE_EPS: f64 = 1.0;
    const SEG_EPS: f64 = 0.000_001;

    if is_backward
        || path.len() < 3
        || !matches!(direction, Direction::TopDown | Direction::BottomTop)
    {
        return;
    }

    let fanout_outbound: Vec<&crate::diagrams::flowchart::geometry::LayoutEdge> = geometry
        .edges
        .iter()
        .filter(|candidate| candidate.from == edge.from)
        .collect();
    if fanout_outbound.len() != 3 {
        return;
    }
    if fanout_outbound
        .iter()
        .any(|candidate| geometry.reversed_edges.contains(&candidate.index))
    {
        return;
    }

    let Some((source_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
    else {
        return;
    };
    let center_x = source_rect.x + source_rect.width / 2.0;
    let start = path[0];
    let first = path[1];
    let second = path[2];
    let source_offset = start.x - center_x;
    if source_offset.abs() < MIN_OFF_CENTER_ABS {
        return;
    }

    let first_is_horizontal =
        (start.y - first.y).abs() <= SEG_EPS && (start.x - first.x).abs() > SEG_EPS;
    let second_is_vertical =
        (first.x - second.x).abs() <= SEG_EPS && (first.y - second.y).abs() > SEG_EPS;
    if !first_is_horizontal || !second_is_vertical {
        return;
    }

    let progresses_along_primary = match direction {
        Direction::TopDown => second.y > start.y + SEG_EPS,
        Direction::BottomTop => second.y < start.y - SEG_EPS,
        _ => false,
    };
    if !progresses_along_primary {
        return;
    }

    let lateral_delta = first.x - start.x;
    if lateral_delta.abs() <= SEG_EPS || lateral_delta.signum() != source_offset.signum() {
        return;
    }

    let mut outbound_target_primary_axis: Vec<f64> = Vec::with_capacity(fanout_outbound.len());
    for candidate in fanout_outbound {
        let Some((target_rect, _)) =
            endpoint_rect_and_shape(geometry, &candidate.to, candidate.to_subgraph.as_deref())
        else {
            return;
        };
        outbound_target_primary_axis.push(target_rect.y);
    }
    let baseline_primary = outbound_target_primary_axis[0];
    if outbound_target_primary_axis
        .iter()
        .any(|primary| (primary - baseline_primary).abs() > FANOUT_LANE_EPS)
    {
        return;
    }

    let stem_y = match direction {
        Direction::TopDown => start.y + MIN_PRIMARY_STEM,
        Direction::BottomTop => start.y - MIN_PRIMARY_STEM,
        _ => start.y,
    };
    let stem = FPoint::new(start.x, stem_y);
    let sweep = FPoint::new(first.x, stem_y);
    if (stem.y - start.y).abs() <= SEG_EPS
        || (sweep.x - stem.x).abs() <= SEG_EPS
        || (second.y - sweep.y).abs() <= SEG_EPS
    {
        return;
    }
    let stem_stays_before_terminal_drop = match direction {
        Direction::TopDown => stem.y < second.y - SEG_EPS,
        Direction::BottomTop => stem.y > second.y + SEG_EPS,
        _ => false,
    };
    if !stem_stays_before_terminal_drop {
        return;
    }

    path[1] = stem;
    path.insert(2, sweep);
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

fn has_immediate_axial_turnback(path: &[FPoint]) -> bool {
    const EPS: f64 = 0.000_001;
    path.windows(3).any(|triple| {
        let a = triple[0];
        let b = triple[1];
        let c = triple[2];

        let first_vertical = (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() > EPS;
        let second_vertical = (b.x - c.x).abs() <= EPS && (b.y - c.y).abs() > EPS;
        if first_vertical && second_vertical {
            let dy1 = b.y - a.y;
            let dy2 = c.y - b.y;
            return dy1.abs() > EPS && dy2.abs() > EPS && dy1.signum() != dy2.signum();
        }

        let first_horizontal = (a.y - b.y).abs() <= EPS && (a.x - b.x).abs() > EPS;
        let second_horizontal = (b.y - c.y).abs() <= EPS && (b.x - c.x).abs() > EPS;
        if first_horizontal && second_horizontal {
            let dx1 = b.x - a.x;
            let dx2 = c.x - b.x;
            return dx1.abs() > EPS && dx2.abs() > EPS && dx1.signum() != dx2.signum();
        }

        false
    })
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

fn align_backward_source_stem_to_outer_lane(
    path: &mut [FPoint],
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
) {
    const EPS: f64 = 0.000_001;
    const FACE_MARGIN: f64 = 1.0;
    if !matches!(direction, Direction::TopDown | Direction::BottomTop) || path.len() < 3 {
        return;
    }

    let Some((source_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
    else {
        return;
    };

    let top = source_rect.y;
    let bottom = source_rect.y + source_rect.height;
    let mut start = path[0];
    let support = path[1];
    let next = path[2];

    let start_on_top_or_bottom = (start.y - top).abs() <= EPS || (start.y - bottom).abs() <= EPS;
    if !start_on_top_or_bottom {
        return;
    }

    let stem_is_diagonal = (start.x - support.x).abs() > EPS && (start.y - support.y).abs() > EPS;
    if !stem_is_diagonal {
        return;
    }

    let support_to_next_is_horizontal =
        (support.y - next.y).abs() <= EPS && (support.x - next.x).abs() > EPS;
    if !support_to_next_is_horizontal {
        return;
    }

    let left = source_rect.x;
    let right = source_rect.x + source_rect.width;
    let min_x = left + FACE_MARGIN;
    let max_x = right - FACE_MARGIN;
    let lane_x = support.x;
    if lane_x < min_x - EPS || lane_x > max_x + EPS {
        return;
    }

    start.x = lane_x.clamp(min_x, max_x);
    path[0] = start;
}

fn align_backward_outer_lane_to_hint(
    path: &mut [FPoint],
    hint: Option<&[FPoint]>,
    direction: Direction,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
) {
    const EPS: f64 = 0.000_001;
    if path.len() < 3 {
        return;
    }
    let Some(hint) = hint else {
        return;
    };
    if hint.len() < 2 {
        return;
    }

    let last = path.len() - 1;
    match direction {
        Direction::TopDown | Direction::BottomTop => {
            let Some((target_rect, _)) =
                endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
            else {
                return;
            };
            let hint_target = hint[hint.len() - 1];
            if hint_side_face_for_td_alignment(hint_target, target_rect).is_none() {
                return;
            }

            let hint_outer = hint
                .iter()
                .map(|point| point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let route_outer = path
                .iter()
                .map(|point| point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            if (hint_outer - route_outer).abs() <= EPS {
                return;
            }

            let mut aligned = false;
            for (idx, point) in path.iter_mut().enumerate() {
                if idx == 0 || idx == last {
                    continue;
                }
                if (point.x - route_outer).abs() <= EPS {
                    point.x = hint_outer;
                    aligned = true;
                }
            }
            if !aligned {
            }
        }
        Direction::LeftRight | Direction::RightLeft => {
            let hint_outer = hint
                .iter()
                .map(|point| point.y)
                .fold(f64::NEG_INFINITY, f64::max);
            let route_outer = path
                .iter()
                .map(|point| point.y)
                .fold(f64::NEG_INFINITY, f64::max);
            if (hint_outer - route_outer).abs() <= EPS {
                return;
            }

            let mut aligned = false;
            for (idx, point) in path.iter_mut().enumerate() {
                if idx == 0 || idx == last {
                    continue;
                }
                if (point.y - route_outer).abs() <= EPS {
                    point.y = hint_outer;
                    aligned = true;
                }
            }
            if !aligned {
                return;
            }

            // Keep residual terminal hooks from drifting too far below the
            // hint-derived backward lane envelope in LR/RL.
            let max_allowed = hint_outer + 3.0;
            for (idx, point) in path.iter_mut().enumerate() {
                if idx == 0 || idx == last {
                    continue;
                }
                if point.y > max_allowed {
                    point.y = max_allowed;
                }
            }
        }
    }
}

fn hint_side_face_for_td_alignment(point: FPoint, rect: FRect) -> Option<Face> {
    const FACE_EPS: f64 = 2.0;
    const CORNER_BIAS: f64 = 0.5;
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let dist_left = (point.x - left).abs();
    let dist_right = (point.x - right).abs();
    let dist_top = (point.y - top).abs();
    let dist_bottom = (point.y - bottom).abs();

    let side_dist = dist_left.min(dist_right);
    let vertical_dist = dist_top.min(dist_bottom);
    if side_dist <= FACE_EPS && side_dist + CORNER_BIAS < vertical_dist {
        if dist_left <= dist_right {
            Some(Face::Left)
        } else {
            Some(Face::Right)
        }
    } else {
        None
    }
}

fn enforce_backward_terminal_tangent_direction(
    path: &mut Vec<FPoint>,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    preserve_terminal_lane_on_overflow_target: bool,
    preferred_target_face: Option<Face>,
) {
    const EPS: f64 = 0.000_001;
    const TANGENT_STEP: f64 = 12.0;
    if path.len() < 2 {
        return;
    }

    let Some((target_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
    else {
        return;
    };

    let last = path.len() - 1;
    let canonical_face =
        preferred_target_face.unwrap_or_else(|| q2_backward_channel_face(direction));
    let left = target_rect.x;
    let right = target_rect.x + target_rect.width;
    let top = target_rect.y;
    let bottom = target_rect.y + target_rect.height;

    let existing_support = if path.len() > 2 {
        Some(path[last - 1])
    } else {
        None
    };

    let mut end = path[last];
    let mut support = match canonical_face {
        Face::Left => {
            end.x = left;
            end.y = clamp_face_coordinate_with_corner_inset(
                end.y,
                top,
                bottom,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            FPoint::new(end.x - TANGENT_STEP, end.y)
        }
        Face::Right => {
            end.x = right;
            end.y = clamp_face_coordinate_with_corner_inset(
                end.y,
                top,
                bottom,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            FPoint::new(end.x + TANGENT_STEP, end.y)
        }
        Face::Top => {
            end.x = clamp_face_coordinate_with_corner_inset(
                end.x,
                left,
                right,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            end.y = top;
            FPoint::new(end.x, end.y - TANGENT_STEP)
        }
        Face::Bottom => {
            end.x = clamp_face_coordinate_with_corner_inset(
                end.x,
                left,
                right,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            end.y = bottom;
            FPoint::new(end.x, end.y + TANGENT_STEP)
        }
    };

    if let Some(existing) = existing_support {
        match canonical_face {
            Face::Left => {
                if (existing.y - end.y).abs() <= EPS && existing.x < end.x - EPS {
                    support.x = support.x.min(existing.x);
                }
            }
            Face::Right => {
                if (existing.y - end.y).abs() <= EPS && existing.x > end.x + EPS {
                    support.x = support.x.max(existing.x);
                }
            }
            Face::Top => {
                if (existing.x - end.x).abs() <= EPS && existing.y < end.y - EPS {
                    support.y = support.y.min(existing.y);
                }
            }
            Face::Bottom => {
                if (existing.x - end.x).abs() <= EPS && existing.y > end.y + EPS {
                    support.y = support.y.max(existing.y);
                }
            }
        }
    }

    if path.len() >= 3 {
        let prev = path[last - 2];
        match canonical_face {
            Face::Left => {
                if prev.x < support.x - EPS {
                    support.x = prev.x;
                }
            }
            Face::Right => {
                if prev.x > support.x + EPS {
                    support.x = prev.x;
                }
            }
            Face::Top => {
                if prev.y < support.y - EPS {
                    support.y = prev.y;
                }
            }
            Face::Bottom => {
                if prev.y > support.y + EPS {
                    support.y = prev.y;
                }
            }
        }
    }

    if path.len() >= 4 {
        let pre_prev = path[last - 3];
        match canonical_face {
            Face::Left => {
                if (pre_prev.y - end.y).abs() <= EPS && pre_prev.x < support.x - EPS {
                    support.x = pre_prev.x;
                }
            }
            Face::Right => {
                if (pre_prev.y - end.y).abs() <= EPS && pre_prev.x > support.x + EPS {
                    support.x = pre_prev.x;
                }
            }
            Face::Top => {
                if (pre_prev.x - end.x).abs() <= EPS && pre_prev.y < support.y - EPS {
                    support.y = pre_prev.y;
                }
            }
            Face::Bottom => {
                if (pre_prev.x - end.x).abs() <= EPS && pre_prev.y > support.y + EPS {
                    support.y = pre_prev.y;
                }
            }
        }
    }

    path[last] = end;

    if path.len() == 2 {
        path.insert(last, support);
    } else {
        path[last - 1] = support;
    }

    if path.len() < 3 {
        return;
    }

    let support_idx = path.len() - 2;
    let prev_idx = support_idx - 1;
    let prev = path[prev_idx];
    let support = path[support_idx];
    let support_is_axis_aligned =
        (prev.x - support.x).abs() <= EPS || (prev.y - support.y).abs() <= EPS;
    if support_is_axis_aligned {
        expand_compact_td_bt_backward_terminal_return(path, canonical_face, direction, target_rect);
        if !preserve_terminal_lane_on_overflow_target {
            collapse_terminal_turnback_spikes(path, canonical_face);
        }
        return;
    }

    let primary_elbow = FPoint::new(prev.x, support.y);
    let fallback_elbow = FPoint::new(support.x, prev.y);

    let can_use_primary =
        !points_match(primary_elbow, prev) && !points_match(primary_elbow, support);
    let can_use_fallback =
        !points_match(fallback_elbow, prev) && !points_match(fallback_elbow, support);

    let prefer_outer_corner_first = matches!(canonical_face, Face::Left | Face::Right);
    if prefer_outer_corner_first {
        if can_use_fallback {
            path.insert(support_idx, fallback_elbow);
        } else if can_use_primary {
            path.insert(support_idx, primary_elbow);
        }
    } else if can_use_primary {
        path.insert(support_idx, primary_elbow);
    } else if can_use_fallback {
        path.insert(support_idx, fallback_elbow);
    }

    if !preserve_terminal_lane_on_overflow_target {
        collapse_terminal_turnback_spikes(path, canonical_face);
    }
    expand_compact_td_bt_backward_terminal_return(path, canonical_face, direction, target_rect);
}

fn expand_compact_td_bt_backward_terminal_return(
    path: &mut Vec<FPoint>,
    canonical_face: Face,
    direction: Direction,
    target_rect: FRect,
) {
    const EPS: f64 = 0.000_001;
    const MIN_VERTICAL_SPLIT: f64 = 4.0;
    if path.len() != 4
        || !matches!(direction, Direction::TopDown | Direction::BottomTop)
        || !matches!(canonical_face, Face::Top | Face::Bottom)
    {
        return;
    }

    let start = path[0];
    let pre = path[1];
    let lane = path[2];
    let end = path[3];
    let start_to_pre_is_vertical = (start.x - pre.x).abs() <= EPS && (start.y - pre.y).abs() > EPS;
    let pre_to_lane_is_horizontal = (pre.y - lane.y).abs() <= EPS && (pre.x - lane.x).abs() > EPS;
    let lane_to_end_is_vertical = (lane.x - end.x).abs() <= EPS && (lane.y - end.y).abs() > EPS;
    if !start_to_pre_is_vertical || !pre_to_lane_is_horizontal || !lane_to_end_is_vertical {
        return;
    }

    let delta_y = end.y - start.y;
    let total_gap = delta_y.abs();
    if total_gap <= MIN_VERTICAL_SPLIT * 3.0 + EPS {
        return;
    }

    let left = target_rect.x;
    let right = target_rect.x + target_rect.width;
    let target_x =
        clamp_face_coordinate_with_corner_inset(pre.x, left, right, MIN_PORT_CORNER_INSET_BACKWARD);
    if (target_x - lane.x).abs() <= EPS {
        return;
    }

    // Place the right jog near the source and the left jog near the target,
    // while keeping the outer vertical lane centered between endpoints.
    let mut source_jog_y = start.y + delta_y * 0.30;
    let mut target_jog_y = start.y + delta_y * 0.70;

    let midpoint_y = (start.y + end.y) / 2.0;
    if matches!(canonical_face, Face::Bottom) {
        source_jog_y = source_jog_y.max(midpoint_y + MIN_VERTICAL_SPLIT);
        target_jog_y = target_jog_y.min(midpoint_y - MIN_VERTICAL_SPLIT);
    } else {
        source_jog_y = source_jog_y.min(midpoint_y - MIN_VERTICAL_SPLIT);
        target_jog_y = target_jog_y.max(midpoint_y + MIN_VERTICAL_SPLIT);
    }

    let source_stem = FPoint::new(start.x, source_jog_y);
    let right_jog = FPoint::new(lane.x, source_jog_y);
    let outer_vertical = FPoint::new(lane.x, target_jog_y);
    let left_jog = FPoint::new(target_x, target_jog_y);
    let terminal = FPoint::new(target_x, end.y);

    let valid_progress = match canonical_face {
        Face::Bottom => {
            source_stem.y < start.y - EPS
                && right_jog.y < start.y - EPS
                && outer_vertical.y < right_jog.y - EPS
                && left_jog.y < right_jog.y - EPS
                && terminal.y < left_jog.y - EPS
        }
        Face::Top => {
            source_stem.y > start.y + EPS
                && right_jog.y > start.y + EPS
                && outer_vertical.y > right_jog.y + EPS
                && left_jog.y > right_jog.y + EPS
                && terminal.y > left_jog.y + EPS
        }
        _ => false,
    };
    if !valid_progress {
        return;
    }

    *path = vec![
        start,
        source_stem,
        right_jog,
        outer_vertical,
        left_jog,
        terminal,
    ];
}

fn collapse_terminal_turnback_spikes(path: &mut Vec<FPoint>, canonical_face: Face) {
    const EPS: f64 = 0.000_001;
    if path.len() < 4 {
        return;
    }

    #[derive(Copy, Clone, Eq, PartialEq)]
    enum Axis {
        Horizontal,
        Vertical,
    }

    let segment_axis = |a: FPoint, b: FPoint| -> Option<Axis> {
        if (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() > EPS {
            Some(Axis::Vertical)
        } else if (a.y - b.y).abs() <= EPS && (a.x - b.x).abs() > EPS {
            Some(Axis::Horizontal)
        } else {
            None
        }
    };
    let deltas_for_axis = |a: FPoint, b: FPoint, axis: Axis| -> f64 {
        match axis {
            Axis::Horizontal => b.x - a.x,
            Axis::Vertical => b.y - a.y,
        }
    };

    // Pattern: pre -> turn -> support reverses along one axis before the
    // terminal support segment. Preserve the already-defined lane at `turn`.
    if path.len() >= 4 {
        let n = path.len();
        let pre = path[n - 4];
        let turn = path[n - 3];
        let mut support = path[n - 2];
        let mut end = path[n - 1];
        if let (Some(axis1), Some(axis2), Some(axis3)) = (
            segment_axis(pre, turn),
            segment_axis(turn, support),
            segment_axis(support, end),
        ) {
            let d1 = deltas_for_axis(pre, turn, axis1);
            let d2 = deltas_for_axis(turn, support, axis2);
            let has_reversal = axis1 == axis2
                && axis2 != axis3
                && d1.abs() > EPS
                && d2.abs() > EPS
                && d1.signum() != d2.signum();
            if has_reversal {
                match canonical_face {
                    Face::Left | Face::Right => {
                        support.y = turn.y;
                        end.y = turn.y;
                        path[n - 2] = support;
                        path[n - 1] = end;
                    }
                    Face::Top | Face::Bottom => {
                        support.x = turn.x;
                        end.x = turn.x;
                        path[n - 2] = support;
                        path[n - 1] = end;
                    }
                }
            }
        }
    }

    // Pattern: turn -> support -> end immediately reverses on the same axis.
    // Replace `turn` with an outer-corner elbow so the terminal approach
    // remains monotonic toward the endpoint.
    if path.len() >= 4 {
        let n = path.len();
        let pre = path[n - 4];
        let turn = path[n - 3];
        let support = path[n - 2];
        let end = path[n - 1];
        if let (Some(axis1), Some(axis2)) =
            (segment_axis(turn, support), segment_axis(support, end))
        {
            let d1 = deltas_for_axis(turn, support, axis1);
            let d2 = deltas_for_axis(support, end, axis2);
            let has_reversal =
                axis1 == axis2 && d1.abs() > EPS && d2.abs() > EPS && d1.signum() != d2.signum();
            if has_reversal {
                let candidate = match canonical_face {
                    Face::Left | Face::Right => FPoint::new(support.x, pre.y),
                    Face::Top | Face::Bottom => FPoint::new(pre.x, support.y),
                };
                let candidate_is_valid = !points_match(candidate, pre)
                    && !points_match(candidate, support)
                    && segment_axis(pre, candidate).is_some()
                    && segment_axis(candidate, support).is_some();
                if candidate_is_valid {
                    path[n - 3] = candidate;
                }
            }
        }
    }

    let mut idx = 1usize;
    while idx < path.len() {
        if points_match(path[idx - 1], path[idx]) {
            path.remove(idx);
        } else {
            idx += 1;
        }
    }
}

fn collapse_tiny_backward_terminal_staircase(
    path: &mut Vec<FPoint>,
    direction: Direction,
    min_lateral_run: f64,
) {
    const EPS: f64 = 0.000_001;
    if !matches!(direction, Direction::TopDown | Direction::BottomTop)
        || path.len() < 3
        || min_lateral_run <= 0.0
    {
        return;
    }

    let n = path.len();
    let a = path[n - 3];
    let b = path[n - 2];
    let mut c = path[n - 1];

    let ab_is_horizontal = (a.y - b.y).abs() <= EPS && (a.x - b.x).abs() > EPS;
    let bc_is_vertical = (b.x - c.x).abs() <= EPS && (b.y - c.y).abs() > EPS;
    if !ab_is_horizontal || !bc_is_vertical {
        return;
    }

    let lateral_run = (b.x - a.x).abs();
    if lateral_run + EPS >= min_lateral_run {
        return;
    }

    c.x = a.x;
    path[n - 1] = c;
    path[n - 2] = FPoint::new(a.x, b.y);

    let mut idx = 1usize;
    while idx < path.len() {
        if points_match(path[idx - 1], path[idx]) {
            path.remove(idx);
        } else {
            idx += 1;
        }
    }
}

fn enforce_backward_source_tangent_direction(
    path: &mut Vec<FPoint>,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
    direction: Direction,
    preferred_source_face: Option<Face>,
) {
    const EPS: f64 = 0.000_001;
    const TANGENT_STEP: f64 = 8.0;
    if path.len() < 2 {
        return;
    }

    let Some((source_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.from, edge.from_subgraph.as_deref())
    else {
        return;
    };

    let canonical_face =
        preferred_source_face.unwrap_or_else(|| q2_backward_channel_face(direction));
    let left = source_rect.x;
    let right = source_rect.x + source_rect.width;
    let top = source_rect.y;
    let bottom = source_rect.y + source_rect.height;

    let existing_support = if path.len() > 2 { Some(path[1]) } else { None };

    let mut start = path[0];
    match canonical_face {
        Face::Left => {
            start.x = left;
            start.y = clamp_face_coordinate_with_corner_inset(
                start.y,
                top,
                bottom,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
        }
        Face::Right => {
            start.x = right;
            start.y = clamp_face_coordinate_with_corner_inset(
                start.y,
                top,
                bottom,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
        }
        Face::Top => {
            start.x = clamp_face_coordinate_with_corner_inset(
                start.x,
                left,
                right,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            start.y = top;
        }
        Face::Bottom => {
            start.x = clamp_face_coordinate_with_corner_inset(
                start.x,
                left,
                right,
                MIN_PORT_CORNER_INSET_BACKWARD,
            );
            start.y = bottom;
        }
    }
    if matches!(canonical_face, Face::Left | Face::Right) {
        start = bias_face_coordinate_toward_center(
            start,
            source_rect,
            0.84,
            MIN_PORT_CORNER_INSET_BACKWARD,
        );
    }
    let mut support = match canonical_face {
        Face::Left => FPoint::new(start.x - TANGENT_STEP, start.y),
        Face::Right => FPoint::new(start.x + TANGENT_STEP, start.y),
        Face::Top => FPoint::new(start.x, start.y - TANGENT_STEP),
        Face::Bottom => FPoint::new(start.x, start.y + TANGENT_STEP),
    };

    if let Some(existing) = existing_support {
        match canonical_face {
            Face::Left => {
                if (existing.y - start.y).abs() <= EPS && existing.x < start.x - EPS {
                    support.x = support.x.min(existing.x);
                }
            }
            Face::Right => {
                if (existing.y - start.y).abs() <= EPS && existing.x > start.x + EPS {
                    support.x = support.x.max(existing.x);
                }
            }
            Face::Top => {
                if (existing.x - start.x).abs() <= EPS && existing.y < start.y - EPS {
                    support.y = support.y.min(existing.y);
                }
            }
            Face::Bottom => {
                if (existing.x - start.x).abs() <= EPS && existing.y > start.y + EPS {
                    support.y = support.y.max(existing.y);
                }
            }
        }
    }

    if path.len() >= 3 {
        let next = path[2];
        match canonical_face {
            Face::Left => {
                if next.x < support.x - EPS {
                    support.x = next.x;
                }
            }
            Face::Right => {
                if next.x > support.x + EPS {
                    support.x = next.x;
                }
            }
            Face::Top => {
                if next.y < support.y - EPS {
                    support.y = next.y;
                }
            }
            Face::Bottom => {
                if next.y > support.y + EPS {
                    support.y = next.y;
                }
            }
        }
    }

    if path.len() >= 4 {
        let next_next = path[3];
        match canonical_face {
            Face::Left => {
                if (next_next.y - start.y).abs() <= EPS && next_next.x < support.x - EPS {
                    support.x = next_next.x;
                }
            }
            Face::Right => {
                if (next_next.y - start.y).abs() <= EPS && next_next.x > support.x + EPS {
                    support.x = next_next.x;
                }
            }
            Face::Top => {
                if (next_next.x - start.x).abs() <= EPS && next_next.y < support.y - EPS {
                    support.y = next_next.y;
                }
            }
            Face::Bottom => {
                if (next_next.x - start.x).abs() <= EPS && next_next.y > support.y + EPS {
                    support.y = next_next.y;
                }
            }
        }
    }

    path[0] = start;
    if path.len() == 2 {
        path.insert(1, support);
    } else {
        path[1] = support;
    }

    if path.len() < 3 {
        return;
    }

    let support_idx = 1;
    let next_idx = 2;
    let support = path[support_idx];
    let next = path[next_idx];
    let support_is_axis_aligned =
        (support.x - next.x).abs() <= EPS || (support.y - next.y).abs() <= EPS;
    if support_is_axis_aligned {
        return;
    }

    let primary_elbow = FPoint::new(support.x, next.y);
    if !points_match(primary_elbow, support) && !points_match(primary_elbow, next) {
        path.insert(next_idx, primary_elbow);
        return;
    }

    let fallback_elbow = FPoint::new(next.x, support.y);
    if !points_match(fallback_elbow, support) && !points_match(fallback_elbow, next) {
        path.insert(next_idx, fallback_elbow);
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

#[allow(clippy::too_many_arguments)]
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
    {
        let start = path[0];
        let next = path[1];
        if point_on_or_inside_rect(start, &from_rect, EPS) {
            let clipped = clip_point_to_axis_face(
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
            path[0] = project_endpoint_to_shape(clipped, next, from_rect, from_shape);
        }
    }

    if let Some((to_rect, to_shape)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
    {
        let last = path.len() - 1;
        let end = path[last];
        let prev = path[last - 1];
        if point_on_or_inside_rect(end, &to_rect, EPS) {
            let clipped = clip_point_to_axis_face(
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
            path[last] = project_endpoint_to_shape(clipped, prev, to_rect, to_shape);
        }
    }
}

/// Project a rect-clipped endpoint to the actual shape boundary for non-rect shapes.
/// For rectangles, returns the rect-clipped point unchanged.
fn project_endpoint_to_shape(
    rect_clipped: FPoint,
    approach: FPoint,
    rect: FRect,
    shape: Shape,
) -> FPoint {
    match shape {
        Shape::Diamond | Shape::Hexagon => intersect_shape_boundary_float(rect, shape, approach),
        _ => rect_clipped,
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

fn hint_face_for_td_bt_parity(point: FPoint, rect: FRect) -> Option<Face> {
    const FACE_EPS: f64 = 2.0;
    const CORNER_BIAS: f64 = 0.5;
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let dist_left = (point.x - left).abs();
    let dist_right = (point.x - right).abs();
    let dist_top = (point.y - top).abs();
    let dist_bottom = (point.y - bottom).abs();

    let horizontal_dist = dist_left.min(dist_right);
    let vertical_dist = dist_top.min(dist_bottom);

    // Reject corner-ambiguous hints where side/top proximity is comparable.
    if vertical_dist <= FACE_EPS && vertical_dist + CORNER_BIAS < horizontal_dist {
        return if dist_top <= dist_bottom {
            Some(Face::Top)
        } else {
            Some(Face::Bottom)
        };
    }
    if horizontal_dist <= FACE_EPS && horizontal_dist + CORNER_BIAS < vertical_dist {
        return if dist_left <= dist_right {
            Some(Face::Left)
        } else {
            Some(Face::Right)
        };
    }

    None
}

#[allow(clippy::too_many_arguments)]
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
    let max_corner_inset = if preserve_existing_face {
        MIN_PORT_CORNER_INSET_BACKWARD
    } else {
        MIN_PORT_CORNER_INSET_FORWARD
    };

    if preserve_existing_face && is_target_endpoint {
        let canonical_face = q2_backward_channel_face(direction);
        let resolved_face = resolve_q1_q2_face_conflict(
            direction,
            true,
            target_has_backward_conflict,
            q1_policy_face,
            canonical_face,
        );
        let mut clipped = clip_point_to_rect_face_with_inset(
            endpoint,
            rect,
            map_face_to_rect_face(resolved_face),
            max_corner_inset,
        );
        if matches!(resolved_face, Face::Bottom) {
            match direction {
                Direction::LeftRight => {
                    clipped.x = clamp_face_coordinate_with_corner_inset(
                        adjacent.x - 1.0,
                        rect.x,
                        rect.x + rect.width,
                        max_corner_inset,
                    );
                }
                Direction::RightLeft => {
                    clipped.x = clamp_face_coordinate_with_corner_inset(
                        adjacent.x + 1.0,
                        rect.x,
                        rect.x + rect.width,
                        max_corner_inset,
                    );
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
            return clip_point_to_rect_face_with_inset(
                endpoint,
                rect,
                resolved_rect_face,
                max_corner_inset,
            );
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
        return clip_point_to_rect_face_with_inset(
            endpoint,
            rect,
            map_face_to_rect_face(resolved_fallback),
            max_corner_inset,
        );
    }

    if !preserve_existing_face && target_overflowed {
        let canonical = map_face_to_rect_face(q2_backward_channel_face(direction));
        if let Some(actual_face) = boundary_face_excluding_corners(endpoint, rect, 0.5)
            && actual_face == canonical
        {
            return clip_point_to_rect_face_with_inset(
                endpoint,
                rect,
                opposite_rect_face(canonical),
                max_corner_inset,
            );
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
                RectFace::Left => clip_point_to_rect_face_with_inset(
                    endpoint,
                    rect,
                    RectFace::Left,
                    max_corner_inset,
                ),
                RectFace::Right => clip_point_to_rect_face_with_inset(
                    endpoint,
                    rect,
                    RectFace::Right,
                    max_corner_inset,
                ),
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
            return if x <= left + EPS {
                clip_point_to_rect_face_with_inset(
                    FPoint::new(x, y),
                    rect,
                    RectFace::Left,
                    max_corner_inset,
                )
            } else {
                clip_point_to_rect_face_with_inset(
                    FPoint::new(x, y),
                    rect,
                    RectFace::Right,
                    max_corner_inset,
                )
            };
        }
    }

    // Terminal segment is horizontal: anchor endpoint on left/right face.
    if dy.abs() <= EPS && dx.abs() > EPS {
        let face = if adjacent.x < endpoint.x {
            RectFace::Left
        } else {
            RectFace::Right
        };
        return clip_point_to_rect_face_with_inset(endpoint, rect, face, max_corner_inset);
    }

    // Terminal segment is vertical: anchor endpoint on top/bottom face.
    if dx.abs() <= EPS && dy.abs() > EPS {
        let face = if adjacent.y < endpoint.y {
            RectFace::Top
        } else {
            RectFace::Bottom
        };
        return clip_point_to_rect_face_with_inset(endpoint, rect, face, max_corner_inset);
    }

    // Fallback: clamp interior drift to the rectangle boundary box.
    FPoint::new(
        endpoint.x.clamp(x_min, x_max),
        endpoint.y.clamp(y_min, y_max),
    )
}

fn enforce_backward_terminal_corner_inset(
    path: &mut Vec<FPoint>,
    edge: &crate::diagrams::flowchart::geometry::LayoutEdge,
    geometry: &GraphGeometry,
) {
    const EPS: f64 = 0.000_001;
    if path.len() < 2 {
        return;
    }
    let Some((target_rect, _)) =
        endpoint_rect_and_shape(geometry, &edge.to, edge.to_subgraph.as_deref())
    else {
        return;
    };

    let last = path.len() - 1;
    let end = path[last];
    let prev = path[last - 1];
    let dx = (end.x - prev.x).abs();
    let dy = (end.y - prev.y).abs();
    let face = if dx <= EPS && dy > EPS {
        let dist_top = (end.y - target_rect.y).abs();
        let dist_bottom = (end.y - (target_rect.y + target_rect.height)).abs();
        if dist_top <= dist_bottom {
            RectFace::Top
        } else {
            RectFace::Bottom
        }
    } else if dy <= EPS && dx > EPS {
        let dist_left = (end.x - target_rect.x).abs();
        let dist_right = (end.x - (target_rect.x + target_rect.width)).abs();
        if dist_left <= dist_right {
            RectFace::Left
        } else {
            RectFace::Right
        }
    } else if let Some(boundary_face) = boundary_face_excluding_corners(end, target_rect, 0.5)
        .or_else(|| boundary_face_including_corners(end, target_rect, 0.5))
    {
        boundary_face
    } else {
        return;
    };

    let clipped =
        clip_point_to_rect_face_with_inset(end, target_rect, face, MIN_PORT_CORNER_INSET_BACKWARD);
    if !points_match(clipped, end) {
        path[last] = clipped;
        ensure_endpoint_axis_aligned(path, false);
    }
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

fn clip_point_to_rect_face_with_inset(
    endpoint: FPoint,
    rect: FRect,
    face: RectFace,
    max_corner_inset: f64,
) -> FPoint {
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    match face {
        RectFace::Top => FPoint::new(
            clamp_face_coordinate_with_corner_inset(endpoint.x, left, right, max_corner_inset),
            top,
        ),
        RectFace::Bottom => FPoint::new(
            clamp_face_coordinate_with_corner_inset(endpoint.x, left, right, max_corner_inset),
            bottom,
        ),
        RectFace::Left => FPoint::new(
            left,
            clamp_face_coordinate_with_corner_inset(endpoint.y, top, bottom, max_corner_inset),
        ),
        RectFace::Right => FPoint::new(
            right,
            clamp_face_coordinate_with_corner_inset(endpoint.y, top, bottom, max_corner_inset),
        ),
    }
}

fn boundary_face_including_corners(point: FPoint, rect: FRect, eps: f64) -> Option<RectFace> {
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let dist_left = (point.x - left).abs();
    let dist_right = (point.x - right).abs();
    let dist_top = (point.y - top).abs();
    let dist_bottom = (point.y - bottom).abs();
    let min_dist = dist_left.min(dist_right).min(dist_top).min(dist_bottom);
    if min_dist > eps {
        return None;
    }
    if (min_dist - dist_left).abs() <= eps {
        return Some(RectFace::Left);
    }
    if (min_dist - dist_right).abs() <= eps {
        return Some(RectFace::Right);
    }
    if (min_dist - dist_top).abs() <= eps {
        return Some(RectFace::Top);
    }
    Some(RectFace::Bottom)
}

fn bias_face_coordinate_toward_center(
    point: FPoint,
    rect: FRect,
    preserve_factor: f64,
    max_corner_inset: f64,
) -> FPoint {
    let factor = preserve_factor.clamp(0.0, 1.0);
    let center_x = rect.x + rect.width / 2.0;
    let center_y = rect.y + rect.height / 2.0;
    let face = boundary_face_excluding_corners(point, rect, 0.5)
        .or_else(|| boundary_face_including_corners(point, rect, 0.5));

    let biased = match face {
        Some(RectFace::Top) | Some(RectFace::Bottom) => {
            FPoint::new(center_x + (point.x - center_x) * factor, point.y)
        }
        Some(RectFace::Left) | Some(RectFace::Right) => {
            FPoint::new(point.x, center_y + (point.y - center_y) * factor)
        }
        None => point,
    };

    match face {
        Some(face) => clip_point_to_rect_face_with_inset(biased, rect, face, max_corner_inset),
        None => biased,
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

fn collapse_collinear_interior_points(path: &mut Vec<FPoint>) {
    const EPS: f64 = 0.000_001;
    if path.len() <= 2 {
        return;
    }

    let mut collapsed = Vec::with_capacity(path.len());
    collapsed.push(path[0]);
    for idx in 1..(path.len() - 1) {
        let prev = *collapsed.last().expect("collapsed is non-empty");
        let curr = path[idx];
        let next = path[idx + 1];

        let dx1 = curr.x - prev.x;
        let dy1 = curr.y - prev.y;
        let dx2 = next.x - curr.x;
        let dy2 = next.y - curr.y;
        let cross = dx1 * dy2 - dy1 * dx2;
        let dot = dx1 * dx2 + dy1 * dy2;
        let collinear_same_direction = cross.abs() <= EPS && dot >= -EPS;
        if !collinear_same_direction {
            collapsed.push(curr);
        }
    }
    collapsed.push(*path.last().expect("path has at least two points"));
    *path = collapsed;
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
