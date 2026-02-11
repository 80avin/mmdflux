//! Shared routing primitives used by text and SVG routing paths.

use crate::diagrams::flowchart::geometry::{FPoint, FRect};
use crate::graph::Direction;
use crate::render::intersect::NodeFace;

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
    #[cfg(test)]
    pub(crate) fn from_node_face(face: NodeFace) -> Self {
        match face {
            NodeFace::Top => Face::Top,
            NodeFace::Bottom => Face::Bottom,
            NodeFace::Left => Face::Left,
            NodeFace::Right => Face::Right,
        }
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
