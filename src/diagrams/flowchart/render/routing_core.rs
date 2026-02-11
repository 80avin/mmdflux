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
pub(crate) fn build_orthogonal_path_float(
    start: FPoint,
    end: FPoint,
    direction: Direction,
    waypoints: &[FPoint],
) -> Vec<FPoint> {
    const ALIGN_EPS: f64 = 0.5;
    const DUP_EPS: f64 = 0.000_001;

    let primary_vertical = matches!(direction, Direction::TopDown | Direction::BottomTop);
    let mut control_points: Vec<FPoint> = Vec::with_capacity(waypoints.len() + 2);
    control_points.push(start);
    control_points.extend_from_slice(waypoints);
    control_points.push(end);

    let mut output: Vec<FPoint> = Vec::with_capacity(control_points.len() * 3);
    output.push(start);

    for target in control_points.into_iter().skip(1) {
        let current = output.last().copied().unwrap_or(start);

        if (current.x - target.x).abs() < DUP_EPS && (current.y - target.y).abs() < DUP_EPS {
            continue;
        }

        let x_aligned = (current.x - target.x).abs() < ALIGN_EPS;
        let y_aligned = (current.y - target.y).abs() < ALIGN_EPS;
        if x_aligned || y_aligned {
            output.push(target);
            continue;
        }

        if primary_vertical {
            let mid_y = (current.y + target.y) / 2.0;
            output.push(FPoint::new(current.x, mid_y));
            output.push(FPoint::new(target.x, mid_y));
        } else {
            let mid_x = (current.x + target.x) / 2.0;
            output.push(FPoint::new(mid_x, current.y));
            output.push(FPoint::new(mid_x, target.y));
        }
        output.push(target);
    }

    output.dedup_by(|a, b| (a.x - b.x).abs() < DUP_EPS && (a.y - b.y).abs() < DUP_EPS);
    output
}
