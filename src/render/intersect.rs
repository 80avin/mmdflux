//! Edge-node intersection calculation.
//!
//! This module implements dynamic edge attachment points based on the approach
//! angle of an edge. Instead of always attaching edges at fixed center points,
//! we calculate where a line from an external point would intersect the node's
//! boundary.
//!
//! This is a key part of the dagre/Sugiyama framework that enables edges to
//! fan out naturally from nodes rather than overlapping at the center.

use super::shape::NodeBounds;
use crate::graph::Shape;

/// A point in 2D space with floating-point coordinates.
///
/// Used for intermediate calculations before rounding to integer grid.
#[derive(Debug, Clone, Copy)]
pub struct FloatPoint {
    pub x: f64,
    pub y: f64,
}

impl FloatPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Convert to integer coordinates by rounding.
    pub fn to_usize(self) -> (usize, usize) {
        (self.x.round() as usize, self.y.round() as usize)
    }
}

impl From<(usize, usize)> for FloatPoint {
    fn from((x, y): (usize, usize)) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
        }
    }
}

/// Calculate where a line from an external point to the rectangle's center
/// intersects the rectangle's boundary.
///
/// # Arguments
/// * `bounds` - The node's bounding box
/// * `point` - The external point (e.g., a waypoint or the center of another node)
///
/// # Returns
/// The intersection point on the rectangle's boundary.
pub fn intersect_rect(bounds: &NodeBounds, point: FloatPoint) -> FloatPoint {
    let x = bounds.center_x() as f64;
    let y = bounds.center_y() as f64;
    let dx = point.x - x;
    let dy = point.y - y;
    let w = bounds.width as f64 / 2.0;
    let h = bounds.height as f64 / 2.0;

    // Edge case: point is at center
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        // Return bottom center as a sensible default
        return FloatPoint::new(x, y + h);
    }

    let (sx, sy) = if dy.abs() * w > dx.abs() * h {
        // Line is steeper than the rectangle's diagonal
        // Intersection is on TOP or BOTTOM edge
        let h = if dy < 0.0 { -h } else { h };
        (h * dx / dy, h)
    } else {
        // Line is shallower than the rectangle's diagonal
        // Intersection is on LEFT or RIGHT edge
        let w = if dx < 0.0 { -w } else { w };
        (w, w * dy / dx)
    };

    FloatPoint::new(x + sx, y + sy)
}

/// Calculate where a line from an external point to a diamond's center
/// intersects the diamond's boundary.
///
/// A diamond is a rhombus with vertices at the center of each edge of its
/// bounding box. The boundary equation is: |dx|/w + |dy|/h = 1
///
/// # Arguments
/// * `bounds` - The node's bounding box
/// * `point` - The external point
///
/// # Returns
/// The intersection point on the diamond's boundary.
pub fn intersect_diamond(bounds: &NodeBounds, point: FloatPoint) -> FloatPoint {
    let x = bounds.center_x() as f64;
    let y = bounds.center_y() as f64;
    let dx = point.x - x;
    let dy = point.y - y;

    // Diamond half-diagonals
    let w = bounds.width as f64 / 2.0;
    let h = bounds.height as f64 / 2.0;

    // Edge case: point is at center
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        // Return bottom vertex as a sensible default
        return FloatPoint::new(x, y + h);
    }

    // For a diamond with equation |x/w| + |y/h| = 1,
    // the intersection with line from center is at parameter t where:
    // |t*dx|/w + |t*dy|/h = 1
    // Solving: t = 1 / (|dx|/w + |dy|/h)
    let t = 1.0 / (dx.abs() / w + dy.abs() / h);

    FloatPoint::new(x + t * dx, y + t * dy)
}

/// Calculate the intersection point for any node shape.
///
/// This dispatches to the appropriate intersection function based on the
/// node's shape.
///
/// # Arguments
/// * `bounds` - The node's bounding box
/// * `point` - The external point (waypoint or other node center)
/// * `shape` - The node's shape
///
/// # Returns
/// The intersection point on the node's boundary, as integer coordinates.
pub fn intersect_node(bounds: &NodeBounds, point: (usize, usize), shape: Shape) -> (usize, usize) {
    let float_point = FloatPoint::from(point);

    let result = match shape {
        Shape::Rectangle | Shape::Round => intersect_rect(bounds, float_point),
        Shape::Diamond => intersect_diamond(bounds, float_point),
    };

    result.to_usize()
}

/// Calculate intersection points for both ends of an edge, given waypoints.
///
/// # Arguments
/// * `source_bounds` - Bounding box of the source node
/// * `source_shape` - Shape of the source node
/// * `target_bounds` - Bounding box of the target node
/// * `target_shape` - Shape of the target node
/// * `waypoints` - Intermediate waypoints (may be empty)
///
/// # Returns
/// A tuple of (source_attachment, target_attachment) points.
pub fn calculate_attachment_points(
    source_bounds: &NodeBounds,
    source_shape: Shape,
    target_bounds: &NodeBounds,
    target_shape: Shape,
    waypoints: &[(usize, usize)],
) -> ((usize, usize), (usize, usize)) {
    let source_center = (source_bounds.center_x(), source_bounds.center_y());
    let target_center = (target_bounds.center_x(), target_bounds.center_y());

    // Source attachment: intersect towards first waypoint or target center
    let source_attach = if let Some(&first_wp) = waypoints.first() {
        intersect_node(source_bounds, first_wp, source_shape)
    } else {
        intersect_node(source_bounds, target_center, source_shape)
    };

    // Target attachment: intersect towards last waypoint or source center
    let target_attach = if let Some(&last_wp) = waypoints.last() {
        intersect_node(target_bounds, last_wp, target_shape)
    } else {
        intersect_node(target_bounds, source_center, target_shape)
    };

    (source_attach, target_attach)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bounds() -> NodeBounds {
        NodeBounds {
            x: 10,
            y: 5,
            width: 10,
            height: 5,
        }
    }

    #[test]
    fn test_intersect_rect_from_below() {
        let bounds = test_bounds();
        // Point directly below center
        let point = FloatPoint::new(15.0, 20.0);
        let result = intersect_rect(&bounds, point);

        // Should hit bottom edge at x=15
        // center_y = 5 + 5/2 = 7.5 (as f64), half_height = 2.5
        // intersection y = 7.5 + 2.5 = 10
        assert_eq!(result.x.round() as usize, 15);
        assert_eq!(result.y.round() as usize, 10);
    }

    #[test]
    fn test_intersect_rect_from_above() {
        let bounds = test_bounds();
        // Point directly above center
        let point = FloatPoint::new(15.0, 0.0);
        let result = intersect_rect(&bounds, point);

        // Should hit top edge
        assert_eq!(result.x.round() as usize, 15);
        assert!(result.y < bounds.center_y() as f64);
    }

    #[test]
    fn test_intersect_rect_from_right() {
        let bounds = test_bounds();
        // Point directly to the right
        let point = FloatPoint::new(30.0, 7.5);
        let result = intersect_rect(&bounds, point);

        // Should hit right edge
        assert!(result.x > bounds.center_x() as f64);
        assert_eq!(result.y.round() as usize, bounds.center_y());
    }

    #[test]
    fn test_intersect_rect_from_left() {
        let bounds = test_bounds();
        // Point directly to the left
        let point = FloatPoint::new(0.0, 7.5);
        let result = intersect_rect(&bounds, point);

        // Should hit left edge
        assert!(result.x < bounds.center_x() as f64);
        assert_eq!(result.y.round() as usize, bounds.center_y());
    }

    #[test]
    fn test_intersect_rect_diagonal() {
        let bounds = test_bounds();
        // Point at a diagonal
        let point = FloatPoint::new(25.0, 15.0);
        let result = intersect_rect(&bounds, point);

        // Should be on the boundary
        let on_right = (result.x - (bounds.x + bounds.width) as f64).abs() < 1.0;
        let on_bottom = (result.y - (bounds.y + bounds.height) as f64).abs() < 1.0;
        assert!(on_right || on_bottom);
    }

    #[test]
    fn test_intersect_diamond_from_below() {
        let bounds = test_bounds();
        // Point directly below center
        let point = FloatPoint::new(15.0, 20.0);
        let result = intersect_diamond(&bounds, point);

        // Should hit bottom vertex
        assert_eq!(result.x.round() as usize, bounds.center_x());
        // y should be at bottom vertex
        assert!(result.y > bounds.center_y() as f64);
    }

    #[test]
    fn test_intersect_diamond_from_right() {
        let bounds = test_bounds();
        // Point directly to the right
        let point = FloatPoint::new(30.0, 7.5);
        let result = intersect_diamond(&bounds, point);

        // Should hit right vertex
        assert!(result.x > bounds.center_x() as f64);
        assert_eq!(result.y.round() as usize, bounds.center_y());
    }

    #[test]
    fn test_intersect_node_rectangle() {
        let bounds = test_bounds();
        let point = (15, 20);
        let result = intersect_node(&bounds, point, Shape::Rectangle);

        // Should be on the boundary
        assert!(result.1 >= bounds.y);
        assert!(result.1 <= bounds.y + bounds.height);
    }

    #[test]
    fn test_intersect_node_diamond() {
        let bounds = test_bounds();
        let point = (15, 20);
        let result = intersect_node(&bounds, point, Shape::Diamond);

        // Should be on the boundary
        assert!(result.1 >= bounds.y);
        assert!(result.1 <= bounds.y + bounds.height);
    }

    #[test]
    fn test_calculate_attachment_points_direct() {
        let source = NodeBounds {
            x: 10,
            y: 5,
            width: 10,
            height: 3,
        };
        let target = NodeBounds {
            x: 10,
            y: 15,
            width: 10,
            height: 3,
        };

        let (src_attach, tgt_attach) =
            calculate_attachment_points(&source, Shape::Rectangle, &target, Shape::Rectangle, &[]);

        // Source should attach at bottom
        assert!(src_attach.1 > source.y);
        // Target should attach at top
        assert!(tgt_attach.1 < target.y + target.height);
    }

    #[test]
    fn test_calculate_attachment_points_with_waypoints() {
        let source = NodeBounds {
            x: 10,
            y: 5,
            width: 10,
            height: 3,
        };
        let target = NodeBounds {
            x: 30,
            y: 15,
            width: 10,
            height: 3,
        };
        let waypoints = [(20, 10), (25, 12)];

        let (src_attach, tgt_attach) = calculate_attachment_points(
            &source,
            Shape::Rectangle,
            &target,
            Shape::Rectangle,
            &waypoints,
        );

        // Source attaches towards first waypoint
        // Target attaches towards last waypoint
        assert!(src_attach.0 >= source.x && src_attach.0 <= source.x + source.width);
        assert!(tgt_attach.0 >= target.x && tgt_attach.0 <= target.x + target.width);
    }

    #[test]
    fn test_intersect_diamond_from_above() {
        let bounds = test_bounds();
        // Point directly above center
        let point = FloatPoint::new(15.0, 0.0);
        let result = intersect_diamond(&bounds, point);

        // Should hit top vertex
        assert_eq!(result.x.round() as usize, bounds.center_x());
        // y should be at top vertex
        assert!(result.y < bounds.center_y() as f64);
    }

    #[test]
    fn test_intersect_diamond_from_left() {
        let bounds = test_bounds();
        // Point directly to the left
        let point = FloatPoint::new(0.0, 7.5);
        let result = intersect_diamond(&bounds, point);

        // Should hit left vertex
        assert!(result.x < bounds.center_x() as f64);
        assert_eq!(result.y.round() as usize, bounds.center_y());
    }

    #[test]
    fn test_intersect_diamond_diagonal() {
        let bounds = test_bounds();
        // Point at a diagonal (bottom-right quadrant)
        let point = FloatPoint::new(25.0, 15.0);
        let result = intersect_diamond(&bounds, point);

        // Should be on the diamond boundary
        // For a diamond, |dx|/w + |dy|/h = 1 at the boundary
        let center_x = bounds.center_x() as f64;
        let center_y = bounds.center_y() as f64;
        let dx = (result.x - center_x).abs();
        let dy = (result.y - center_y).abs();
        let w = bounds.width as f64 / 2.0;
        let h = bounds.height as f64 / 2.0;

        let boundary_check = dx / w + dy / h;
        assert!(
            (boundary_check - 1.0).abs() < 0.1,
            "Point should be on diamond boundary, got {}",
            boundary_check
        );
    }

    #[test]
    fn test_intersect_rect_point_at_center() {
        let bounds = test_bounds();
        // Point exactly at center
        let point = FloatPoint::new(bounds.center_x() as f64, bounds.center_y() as f64);
        let result = intersect_rect(&bounds, point);

        // Should return bottom center as default
        assert_eq!(result.x.round() as usize, bounds.center_x());
    }

    #[test]
    fn test_intersect_diamond_point_at_center() {
        let bounds = test_bounds();
        // Point exactly at center
        let point = FloatPoint::new(bounds.center_x() as f64, bounds.center_y() as f64);
        let result = intersect_diamond(&bounds, point);

        // Should return bottom vertex as default
        assert_eq!(result.x.round() as usize, bounds.center_x());
    }

    #[test]
    fn test_intersect_node_round_uses_rect() {
        let bounds = test_bounds();
        let point = (15, 20);

        // Round shape should use rectangle intersection (approximation)
        let rect_result = intersect_node(&bounds, point, Shape::Rectangle);
        let round_result = intersect_node(&bounds, point, Shape::Round);

        assert_eq!(rect_result, round_result);
    }

    #[test]
    fn test_calculate_attachment_points_diamond_source() {
        let source = NodeBounds {
            x: 10,
            y: 5,
            width: 10,
            height: 5,
        };
        let target = NodeBounds {
            x: 10,
            y: 20,
            width: 10,
            height: 3,
        };

        let (src_attach, tgt_attach) =
            calculate_attachment_points(&source, Shape::Diamond, &target, Shape::Rectangle, &[]);

        // Source (diamond) should attach at bottom vertex
        assert_eq!(src_attach.0, source.center_x());
        // Target should attach at top
        assert!(tgt_attach.1 < target.y + target.height);
    }

    #[test]
    fn test_float_point_to_usize() {
        let p = FloatPoint::new(10.4, 20.6);
        let (x, y) = p.to_usize();
        assert_eq!(x, 10);
        assert_eq!(y, 21);
    }

    #[test]
    fn test_float_point_from_tuple() {
        let p = FloatPoint::from((15_usize, 25_usize));
        assert_eq!(p.x, 15.0);
        assert_eq!(p.y, 25.0);
    }
}
