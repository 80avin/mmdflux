//! Shared routing primitives used by text and SVG routing paths.

use std::collections::HashMap;

use super::layout::{Layout, SubgraphBounds};
use super::shape::NodeBounds;
use crate::diagrams::flowchart::geometry::{FPoint, FRect};
use crate::graph::{Direction, Edge, Shape, Stroke};
use crate::render::intersect::{NodeFace, classify_face};

/// Which face of a rectangular node an edge attaches to.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) enum Face {
    Top,
    Bottom,
    Left,
    Right,
}

impl Face {
    /// Convert shared routing-core face to the text router face type.
    pub(crate) fn to_node_face(self) -> NodeFace {
        match self {
            Face::Top => NodeFace::Top,
            Face::Bottom => NodeFace::Bottom,
            Face::Left => NodeFace::Left,
            Face::Right => NodeFace::Right,
        }
    }

    /// Convert text router face type to the shared routing-core face.
    pub(crate) fn from_node_face(face: NodeFace) -> Self {
        match face {
            NodeFace::Top => Face::Top,
            NodeFace::Bottom => Face::Bottom,
            NodeFace::Left => Face::Left,
            NodeFace::Right => Face::Right,
        }
    }
}

/// Per-edge attachment location on a node face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EdgeAttachment {
    pub face: Face,
    pub fraction: f64,
}

/// Source and target attachment assignments for one edge.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlannedEdgeAttachments {
    pub source: Option<EdgeAttachment>,
    pub target: Option<EdgeAttachment>,
}

/// Deterministic attachment assignments for all planned edges.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AttachmentPlan {
    edge_attachments: HashMap<usize, PlannedEdgeAttachments>,
    group_sizes: HashMap<(String, Face), usize>,
    source_fractions: HashMap<(String, Face), Vec<f64>>,
    target_fractions: HashMap<(String, Face), Vec<f64>>,
}

impl AttachmentPlan {
    /// Return source-side fractions for a node face in deterministic order.
    #[cfg(test)]
    pub(crate) fn source_fractions_for(&self, node_id: &str, face: Face) -> Vec<f64> {
        self.source_fractions
            .get(&(node_id.to_string(), face))
            .cloned()
            .unwrap_or_default()
    }

    /// Return the edge-specific source/target assignments.
    pub(crate) fn edge(&self, edge_index: usize) -> Option<&PlannedEdgeAttachments> {
        self.edge_attachments.get(&edge_index)
    }

    /// Return the number of attachments planned for a node face.
    pub(crate) fn group_size(&self, node_id: &str, face: Face) -> usize {
        self.group_sizes
            .get(&(node_id.to_string(), face))
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AttachmentSide {
    Source,
    Target,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttachmentCandidate {
    pub edge_index: usize,
    pub node_id: String,
    pub side: AttachmentSide,
    pub face: Face,
    pub cross_axis: f64,
}

/// Build a deterministic per-edge attachment-fraction plan from text layout bounds.
pub(crate) fn plan_attachments(
    edges: &[Edge],
    layout: &Layout,
    fallback_direction: Direction,
) -> AttachmentPlan {
    let mut candidates: Vec<AttachmentCandidate> = Vec::with_capacity(edges.len() * 2);

    for edge in edges {
        if edge.from == edge.to || edge.stroke == Stroke::Invisible {
            continue;
        }

        let (src_bounds, tgt_bounds) = match resolve_edge_bounds(layout, edge) {
            Some(bounds) => bounds,
            None => continue,
        };
        let src_shape = if edge.from_subgraph.is_some() {
            Shape::Rectangle
        } else {
            layout
                .node_shapes
                .get(&edge.from)
                .copied()
                .unwrap_or(Shape::Rectangle)
        };
        let tgt_shape = if edge.to_subgraph.is_some() {
            Shape::Rectangle
        } else {
            layout
                .node_shapes
                .get(&edge.to)
                .copied()
                .unwrap_or(Shape::Rectangle)
        };

        let allow_waypoints = edge.from_subgraph.is_none() && edge.to_subgraph.is_none();
        let waypoints = if allow_waypoints {
            layout.edge_waypoints.get(&edge.index)
        } else {
            None
        };
        let src_approach = waypoints
            .and_then(|wps| wps.first().copied())
            .unwrap_or((tgt_bounds.center_x(), tgt_bounds.center_y()));
        let tgt_approach = waypoints
            .and_then(|wps| wps.last().copied())
            .unwrap_or((src_bounds.center_x(), src_bounds.center_y()));

        let edge_dir = layout.effective_edge_direction(&edge.from, &edge.to, fallback_direction);
        let is_subgraph_edge = edge.from_subgraph.is_some() || edge.to_subgraph.is_some();
        let is_backward = is_backward_edge(&src_bounds, &tgt_bounds, edge_dir);
        let has_dagre_waypoints = waypoints.is_some_and(|wps| !wps.is_empty());
        let (mut src_face, mut tgt_face) = if is_backward && !has_dagre_waypoints {
            backward_routing_faces(edge_dir)
        } else if matches!(edge_dir, Direction::TopDown | Direction::BottomTop)
            && !is_backward
            && !is_subgraph_edge
        {
            edge_faces(edge_dir, false)
        } else {
            match edge_dir {
                Direction::LeftRight | Direction::RightLeft => edge_faces(edge_dir, is_backward),
                _ => (
                    Face::from_node_face(classify_face(&src_bounds, src_approach, src_shape)),
                    Face::from_node_face(classify_face(&tgt_bounds, tgt_approach, tgt_shape)),
                ),
            }
        };
        if edge.from_subgraph.is_some() {
            src_face = subgraph_edge_face(&src_bounds, &tgt_bounds, edge_dir);
        }
        if edge.to_subgraph.is_some() {
            tgt_face = subgraph_edge_face(&tgt_bounds, &src_bounds, edge_dir);
        }

        let src_id = edge.from_subgraph.as_deref().unwrap_or(edge.from.as_str());
        let tgt_id = edge.to_subgraph.as_deref().unwrap_or(edge.to.as_str());
        let tgt_in_override = layout
            .node_directions
            .get(&edge.to)
            .is_some_and(|d| *d != fallback_direction);
        let src_cross = if tgt_in_override {
            match src_face {
                Face::Top | Face::Bottom => tgt_bounds.center_x() as f64,
                Face::Left | Face::Right => tgt_bounds.center_y() as f64,
            }
        } else {
            match src_face {
                Face::Top | Face::Bottom => src_approach.0 as f64,
                Face::Left | Face::Right => src_approach.1 as f64,
            }
        };
        let tgt_cross = match tgt_face {
            Face::Top | Face::Bottom => tgt_approach.0 as f64,
            Face::Left | Face::Right => tgt_approach.1 as f64,
        };

        candidates.push(AttachmentCandidate {
            edge_index: edge.index,
            node_id: src_id.to_string(),
            side: AttachmentSide::Source,
            face: src_face,
            cross_axis: src_cross,
        });
        candidates.push(AttachmentCandidate {
            edge_index: edge.index,
            node_id: tgt_id.to_string(),
            side: AttachmentSide::Target,
            face: tgt_face,
            cross_axis: tgt_cross,
        });
    }

    plan_attachment_candidates(candidates)
}

/// Build a deterministic attachment plan from precomputed face candidates.
pub(crate) fn plan_attachment_candidates(candidates: Vec<AttachmentCandidate>) -> AttachmentPlan {
    let mut groups: HashMap<(String, Face), Vec<AttachmentCandidate>> = HashMap::new();
    for candidate in candidates {
        groups
            .entry((candidate.node_id.clone(), candidate.face))
            .or_default()
            .push(candidate);
    }

    let mut plan = AttachmentPlan::default();
    for ((node_id, face), mut group) in groups {
        group.sort_by(compare_attachment_candidates);
        plan.group_sizes
            .insert((node_id.clone(), face), group.len());

        for (idx, candidate) in group.iter().enumerate() {
            let fraction = if group.len() <= 1 {
                0.5
            } else {
                idx as f64 / (group.len() - 1) as f64
            };
            let attachment = EdgeAttachment { face, fraction };
            let edge_entry = plan.edge_attachments.entry(candidate.edge_index).or_insert(
                PlannedEdgeAttachments {
                    source: None,
                    target: None,
                },
            );

            match candidate.side {
                AttachmentSide::Source => {
                    edge_entry.source = Some(attachment);
                    plan.source_fractions
                        .entry((candidate.node_id.clone(), candidate.face))
                        .or_default()
                        .push(fraction);
                }
                AttachmentSide::Target => {
                    edge_entry.target = Some(attachment);
                    plan.target_fractions
                        .entry((candidate.node_id.clone(), candidate.face))
                        .or_default()
                        .push(fraction);
                }
            }
        }
    }
    plan
}

fn compare_attachment_candidates(
    a: &AttachmentCandidate,
    b: &AttachmentCandidate,
) -> std::cmp::Ordering {
    a.cross_axis
        .total_cmp(&b.cross_axis)
        .then_with(|| a.edge_index.cmp(&b.edge_index))
        .then_with(|| a.side.cmp(&b.side))
}

fn subgraph_edge_face(bounds: &NodeBounds, other: &NodeBounds, direction: Direction) -> Face {
    let bounds_right = bounds.x + bounds.width.saturating_sub(1);
    let bounds_bottom = bounds.y + bounds.height.saturating_sub(1);
    let other_right = other.x + other.width.saturating_sub(1);
    let other_bottom = other.y + other.height.saturating_sub(1);

    match direction {
        Direction::TopDown | Direction::BottomTop => {
            if other_bottom < bounds.y {
                return Face::Top;
            }
            if other.y > bounds_bottom {
                return Face::Bottom;
            }
            if other_right < bounds.x {
                return Face::Left;
            }
            if other.x > bounds_right {
                return Face::Right;
            }
        }
        Direction::LeftRight | Direction::RightLeft => {
            if other_right < bounds.x {
                return Face::Left;
            }
            if other.x > bounds_right {
                return Face::Right;
            }
            if other_bottom < bounds.y {
                return Face::Top;
            }
            if other.y > bounds_bottom {
                return Face::Bottom;
            }
        }
    }

    Face::from_node_face(classify_face(
        bounds,
        (other.center_x(), other.center_y()),
        Shape::Rectangle,
    ))
}

fn backward_routing_faces(direction: Direction) -> (Face, Face) {
    match direction {
        Direction::TopDown | Direction::BottomTop => (Face::Right, Face::Right),
        Direction::LeftRight | Direction::RightLeft => (Face::Bottom, Face::Bottom),
    }
}

fn resolve_edge_bounds(layout: &Layout, edge: &Edge) -> Option<(NodeBounds, NodeBounds)> {
    let from_bounds = if let Some(sg_id) = edge.from_subgraph.as_ref() {
        layout
            .subgraph_bounds
            .get(sg_id)
            .map(subgraph_bounds_as_node)?
    } else {
        *layout.get_bounds(&edge.from)?
    };
    let to_bounds = if let Some(sg_id) = edge.to_subgraph.as_ref() {
        layout
            .subgraph_bounds
            .get(sg_id)
            .map(subgraph_bounds_as_node)?
    } else {
        *layout.get_bounds(&edge.to)?
    };
    Some((from_bounds, to_bounds))
}

fn subgraph_bounds_as_node(bounds: &SubgraphBounds) -> NodeBounds {
    NodeBounds {
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
        dagre_center_x: None,
        dagre_center_y: None,
    }
}

fn is_backward_edge(
    from_bounds: &NodeBounds,
    to_bounds: &NodeBounds,
    direction: Direction,
) -> bool {
    match direction {
        Direction::TopDown => to_bounds.y < from_bounds.y,
        Direction::BottomTop => to_bounds.y > from_bounds.y,
        Direction::LeftRight => to_bounds.x < from_bounds.x,
        Direction::RightLeft => to_bounds.x > from_bounds.x,
    }
}

/// Determine source and target attachment faces for the flow direction.
pub(crate) fn edge_faces(direction: Direction, is_backward: bool) -> (Face, Face) {
    let (forward_src, forward_tgt) = match direction {
        Direction::TopDown => (Face::Bottom, Face::Top),
        Direction::BottomTop => (Face::Top, Face::Bottom),
        Direction::LeftRight => (Face::Right, Face::Left),
        Direction::RightLeft => (Face::Left, Face::Right),
    };

    if is_backward {
        (forward_tgt, forward_src)
    } else {
        (forward_src, forward_tgt)
    }
}

/// Classify which face a point approaches, using slope-vs-diagonal comparison.
pub(crate) fn classify_face_float(center: FPoint, rect: FRect, approach: FPoint) -> Face {
    let dx = approach.x - center.x;
    let dy = approach.y - center.y;

    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        return Face::Bottom;
    }

    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;

    if dy.abs() * half_w > dx.abs() * half_h {
        if dy < 0.0 { Face::Top } else { Face::Bottom }
    } else if dx < 0.0 {
        Face::Left
    } else {
        Face::Right
    }
}

/// Compute a point on a rectangle face at the given fraction.
pub(crate) fn point_on_face_float(rect: FRect, face: Face, fraction: f64) -> FPoint {
    let fraction = fraction.clamp(0.0, 1.0);
    match face {
        Face::Top => FPoint::new(rect.x + rect.width * fraction, rect.y),
        Face::Bottom => FPoint::new(rect.x + rect.width * fraction, rect.y + rect.height),
        Face::Left => FPoint::new(rect.x, rect.y + rect.height * fraction),
        Face::Right => FPoint::new(rect.x + rect.width, rect.y + rect.height * fraction),
    }
}

/// Build an orthogonal polyline in float space from start to end through optional waypoints.
///
/// Diagonal spans are split into two elbows using midpoint routing on the
/// diagram's primary axis to keep paths axis-aligned and symmetric.
pub(crate) const ROUTE_ALIGN_EPS: f64 = 0.5;
pub(crate) const ROUTE_POINT_EPS: f64 = 0.000_001;
pub(crate) const LARGE_HORIZONTAL_OFFSET_THRESHOLD: usize = 30;
const MIN_TERMINAL_SUPPORT: f64 = 8.0;

pub(crate) fn build_orthogonal_path_float(
    start: FPoint,
    end: FPoint,
    direction: Direction,
    waypoints: &[FPoint],
) -> Vec<FPoint> {
    let primary_vertical = matches!(direction, Direction::TopDown | Direction::BottomTop);
    let mut control_points: Vec<FPoint> = Vec::with_capacity(waypoints.len() + 2);
    control_points.push(start);
    control_points.extend_from_slice(waypoints);
    control_points.push(end);

    let mut output: Vec<FPoint> = Vec::with_capacity(control_points.len() * 3);
    output.push(start);
    let span_count = control_points.len().saturating_sub(1);

    for (span_idx, target) in control_points.into_iter().skip(1).enumerate() {
        let current = output.last().copied().unwrap_or(start);
        let is_first_span = span_idx == 0;
        let is_last_span = span_idx + 1 == span_count;

        if (current.x - target.x).abs() < ROUTE_POINT_EPS
            && (current.y - target.y).abs() < ROUTE_POINT_EPS
        {
            continue;
        }

        let x_aligned = (current.x - target.x).abs() < ROUTE_ALIGN_EPS;
        let y_aligned = (current.y - target.y).abs() < ROUTE_ALIGN_EPS;
        if x_aligned && y_aligned {
            continue;
        }

        if x_aligned {
            output.push(FPoint::new(current.x, target.y));
            continue;
        }

        if y_aligned {
            output.push(FPoint::new(target.x, current.y));
            continue;
        }

        // For diagonal spans, choose elbow orientation by span role:
        // - first span: preserve source-face normal support
        // - last span: preserve target-face normal support
        // - single diagonal span: keep balanced V-H-V / H-V-H fallback
        if primary_vertical && is_first_span && is_last_span {
            let mid_y = (current.y + target.y) / 2.0;
            output.push(FPoint::new(current.x, mid_y));
            output.push(FPoint::new(target.x, mid_y));
        } else if !primary_vertical && is_first_span && is_last_span {
            let mid_x = (current.x + target.x) / 2.0;
            output.push(FPoint::new(mid_x, current.y));
            output.push(FPoint::new(mid_x, target.y));
        } else if primary_vertical {
            if is_first_span {
                output.push(FPoint::new(current.x, target.y));
            } else {
                output.push(FPoint::new(target.x, current.y));
            }
        } else if is_first_span {
            output.push(FPoint::new(target.x, current.y));
        } else {
            output.push(FPoint::new(current.x, target.y));
        }
        output.push(target);
    }

    output.dedup_by(|a, b| {
        (a.x - b.x).abs() < ROUTE_POINT_EPS && (a.y - b.y).abs() < ROUTE_POINT_EPS
    });
    output
}

/// Enforce shared polyline contracts used by routed-preview outputs.
///
/// Contracts:
/// - no adjacent duplicate points
/// - no zero-length segments
/// - no redundant collinear interior points
/// - non-zero terminal support segment on the primary axis
pub(crate) fn normalize_orthogonal_route_contracts(
    points: &[FPoint],
    direction: Direction,
) -> Vec<FPoint> {
    if points.len() <= 1 {
        return points.to_vec();
    }

    let mut normalized = dedupe_adjacent_points(points);
    if normalized.len() <= 1 {
        return normalized;
    }

    normalized = remove_collinear_points(&normalized);
    normalized = reduce_midfield_jogs_for_large_horizontal_offset(&normalized, direction);
    normalized = compact_terminal_staircase(&normalized, direction);
    normalized = remove_axial_turnbacks(&normalized);
    ensure_terminal_support_segment(&mut normalized, direction);
    normalized = remove_axial_turnbacks(&normalized);
    normalized = dedupe_adjacent_points(&normalized);
    remove_collinear_points(&normalized)
}

fn dedupe_adjacent_points(points: &[FPoint]) -> Vec<FPoint> {
    let mut deduped = Vec::with_capacity(points.len());
    for point in points {
        let keep = deduped.last().is_none_or(|prev: &FPoint| {
            (prev.x - point.x).abs() > ROUTE_POINT_EPS || (prev.y - point.y).abs() > ROUTE_POINT_EPS
        });
        if keep {
            deduped.push(*point);
        }
    }
    deduped
}

fn remove_collinear_points(points: &[FPoint]) -> Vec<FPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut result = Vec::with_capacity(points.len());
    result.push(points[0]);
    for idx in 1..(points.len() - 1) {
        let prev = result.last().copied().expect("result is non-empty");
        let curr = points[idx];
        let next = points[idx + 1];

        let dx1 = curr.x - prev.x;
        let dy1 = curr.y - prev.y;
        let dx2 = next.x - curr.x;
        let dy2 = next.y - curr.y;
        let cross = dx1 * dy2 - dy1 * dx2;
        let dot = dx1 * dx2 + dy1 * dy2;
        let collinear_same_direction = cross.abs() <= ROUTE_POINT_EPS && dot >= -ROUTE_POINT_EPS;

        if !collinear_same_direction {
            result.push(curr);
        }
    }
    result.push(*points.last().expect("points has at least two elements"));
    result
}

fn remove_axial_turnbacks(points: &[FPoint]) -> Vec<FPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut current = points.to_vec();
    loop {
        let mut changed = false;
        let mut result = Vec::with_capacity(current.len());
        result.push(current[0]);

        for idx in 1..(current.len() - 1) {
            let prev = *result.last().expect("result is non-empty");
            let curr = current[idx];
            let next = current[idx + 1];
            let dx1 = curr.x - prev.x;
            let dy1 = curr.y - prev.y;
            let dx2 = next.x - curr.x;
            let dy2 = next.y - curr.y;
            let cross = dx1 * dy2 - dy1 * dx2;
            let dot = dx1 * dx2 + dy1 * dy2;
            let is_collinear = cross.abs() <= ROUTE_POINT_EPS;
            let reverses_direction = dot < -ROUTE_POINT_EPS;
            if is_collinear && reverses_direction {
                changed = true;
                continue;
            }
            result.push(curr);
        }

        result.push(*current.last().expect("points has at least two elements"));
        let deduped = dedupe_adjacent_points(&result);
        if !changed {
            return deduped;
        }
        current = deduped;
        if current.len() <= 2 {
            return current;
        }
    }
}

fn ensure_terminal_support_segment(points: &mut Vec<FPoint>, direction: Direction) {
    if points.len() < 2 {
        return;
    }

    let primary_vertical = matches!(direction, Direction::TopDown | Direction::BottomTop);
    let end = *points.last().expect("len >= 2");
    let prev = points[points.len() - 2];

    if primary_vertical {
        let already_supported =
            (prev.x - end.x).abs() <= ROUTE_POINT_EPS && (prev.y - end.y).abs() > ROUTE_POINT_EPS;
        if already_supported {
            return;
        }

        if (prev.y - end.y).abs() > ROUTE_POINT_EPS {
            points.insert(points.len() - 1, FPoint::new(end.x, prev.y));
            return;
        }

        let support_y = match direction {
            Direction::TopDown => end.y - MIN_TERMINAL_SUPPORT,
            Direction::BottomTop => end.y + MIN_TERMINAL_SUPPORT,
            _ => end.y - MIN_TERMINAL_SUPPORT,
        };
        points.insert(points.len() - 1, FPoint::new(prev.x, support_y));
        points.insert(points.len() - 1, FPoint::new(end.x, support_y));
    } else {
        let already_supported =
            (prev.y - end.y).abs() <= ROUTE_POINT_EPS && (prev.x - end.x).abs() > ROUTE_POINT_EPS;
        if already_supported {
            return;
        }

        if (prev.x - end.x).abs() > ROUTE_POINT_EPS {
            points.insert(points.len() - 1, FPoint::new(prev.x, end.y));
            return;
        }

        let support_x = match direction {
            Direction::LeftRight => end.x - MIN_TERMINAL_SUPPORT,
            Direction::RightLeft => end.x + MIN_TERMINAL_SUPPORT,
            _ => end.x - MIN_TERMINAL_SUPPORT,
        };
        points.insert(points.len() - 1, FPoint::new(support_x, prev.y));
        points.insert(points.len() - 1, FPoint::new(support_x, end.y));
    }
}

fn reduce_midfield_jogs_for_large_horizontal_offset(
    points: &[FPoint],
    direction: Direction,
) -> Vec<FPoint> {
    if !matches!(direction, Direction::TopDown | Direction::BottomTop) || points.len() <= 4 {
        return points.to_vec();
    }

    let start = points[0];
    let end = *points.last().expect("points has at least two elements");
    let horizontal_offset = (start.x - end.x).abs();
    if horizontal_offset <= LARGE_HORIZONTAL_OFFSET_THRESHOLD as f64 {
        return points.to_vec();
    }

    let mid_y = preferred_mid_y_for_vertical_layout(start, end, direction);
    vec![
        start,
        FPoint::new(start.x, mid_y),
        FPoint::new(end.x, mid_y),
        end,
    ]
}

fn compact_terminal_staircase(points: &[FPoint], direction: Direction) -> Vec<FPoint> {
    // Keep short 4-point routes intact so source-face support is not converted
    // into a border-slide start segment.
    if points.len() <= 4 {
        return points.to_vec();
    }

    let mut compacted = points.to_vec();
    let len = compacted.len();
    let a = compacted[len - 4];
    let b = compacted[len - 3];
    let c = compacted[len - 2];
    let d = compacted[len - 1];

    if matches!(direction, Direction::TopDown | Direction::BottomTop) {
        if segment_is_vertical(a, b)
            && segment_is_horizontal(b, c)
            && segment_is_vertical(c, d)
            && segment_sign(b.y - a.y) == segment_sign(d.y - c.y)
            && segment_sign(b.y - a.y) != 0
        {
            let elbow = FPoint::new(c.x, a.y);
            let would_reverse_with_prefix =
                would_introduce_axial_turnback_with_prefix(&compacted, len - 4, a, elbow);
            if !points_equal(a, elbow) && !points_equal(elbow, d) && !would_reverse_with_prefix {
                compacted.truncate(len - 3);
                compacted.push(elbow);
                compacted.push(d);
            }
        }
    } else if segment_is_horizontal(a, b)
        && segment_is_vertical(b, c)
        && segment_is_horizontal(c, d)
        && segment_sign(b.x - a.x) == segment_sign(d.x - c.x)
        && segment_sign(b.x - a.x) != 0
    {
        let elbow = FPoint::new(a.x, c.y);
        let would_reverse_with_prefix =
            would_introduce_axial_turnback_with_prefix(&compacted, len - 4, a, elbow);
        if !points_equal(a, elbow) && !points_equal(elbow, d) && !would_reverse_with_prefix {
            compacted.truncate(len - 3);
            compacted.push(elbow);
            compacted.push(d);
        }
    }

    compacted
}

fn would_introduce_axial_turnback_with_prefix(
    points: &[FPoint],
    anchor_idx: usize,
    anchor: FPoint,
    elbow: FPoint,
) -> bool {
    if anchor_idx == 0 || anchor_idx >= points.len() {
        return false;
    }

    let prefix = points[anchor_idx - 1];
    let dx1 = anchor.x - prefix.x;
    let dy1 = anchor.y - prefix.y;
    let dx2 = elbow.x - anchor.x;
    let dy2 = elbow.y - anchor.y;
    let cross = dx1 * dy2 - dy1 * dx2;
    let dot = dx1 * dx2 + dy1 * dy2;
    cross.abs() <= ROUTE_POINT_EPS && dot < -ROUTE_POINT_EPS
}

fn segment_is_vertical(start: FPoint, end: FPoint) -> bool {
    (start.x - end.x).abs() <= ROUTE_POINT_EPS && (start.y - end.y).abs() > ROUTE_POINT_EPS
}

fn segment_is_horizontal(start: FPoint, end: FPoint) -> bool {
    (start.y - end.y).abs() <= ROUTE_POINT_EPS && (start.x - end.x).abs() > ROUTE_POINT_EPS
}

fn points_equal(a: FPoint, b: FPoint) -> bool {
    (a.x - b.x).abs() <= ROUTE_POINT_EPS && (a.y - b.y).abs() <= ROUTE_POINT_EPS
}

fn segment_sign(delta: f64) -> i8 {
    if delta.abs() <= ROUTE_POINT_EPS {
        0
    } else if delta.is_sign_positive() {
        1
    } else {
        -1
    }
}

fn preferred_mid_y_for_vertical_layout(start: FPoint, end: FPoint, direction: Direction) -> f64 {
    let mut mid_y = (start.y + end.y) / 2.0;

    if (start.x - end.x).abs() > LARGE_HORIZONTAL_OFFSET_THRESHOLD as f64 {
        let target_mid = match direction {
            Direction::TopDown => end.y - (MIN_TERMINAL_SUPPORT * 2.0),
            Direction::BottomTop => end.y + (MIN_TERMINAL_SUPPORT * 2.0),
            _ => mid_y,
        };
        mid_y = match direction {
            Direction::TopDown => target_mid.max(mid_y),
            Direction::BottomTop => target_mid.min(mid_y),
            _ => mid_y,
        };
    }

    if (mid_y - end.y).abs() <= ROUTE_POINT_EPS {
        mid_y = if start.y > end.y {
            end.y + MIN_TERMINAL_SUPPORT
        } else {
            end.y - MIN_TERMINAL_SUPPORT
        };
    }

    mid_y
}
