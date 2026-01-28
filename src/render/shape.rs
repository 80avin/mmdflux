//! Node shape rendering.

use super::canvas::Canvas;
use super::chars::CharSet;
use super::intersect::NodeFace;
use crate::graph::{Node, Shape};

/// Bounding box for a rendered node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeBounds {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl NodeBounds {
    /// Get the center x coordinate.
    pub fn center_x(&self) -> usize {
        self.x + self.width / 2
    }

    /// Get the center y coordinate.
    pub fn center_y(&self) -> usize {
        self.y + self.height / 2
    }

    /// Get the top attachment point (center of top edge).
    pub fn top(&self) -> (usize, usize) {
        (self.center_x(), self.y)
    }

    /// Get the bottom attachment point (center of bottom edge).
    pub fn bottom(&self) -> (usize, usize) {
        (self.center_x(), self.y + self.height - 1)
    }

    /// Get the left attachment point (center of left edge).
    pub fn left(&self) -> (usize, usize) {
        (self.x, self.center_y())
    }

    /// Get the right attachment point (center of right edge).
    pub fn right(&self) -> (usize, usize) {
        (self.x + self.width - 1, self.center_y())
    }

    /// Returns the usable range (start, end) along a face for edge attachment.
    /// For Top/Bottom: x-range excluding corner cells (border characters).
    /// For Left/Right: y-range (full height).
    pub fn face_extent(&self, face: &NodeFace) -> (usize, usize) {
        match face {
            NodeFace::Top | NodeFace::Bottom => {
                // Exclude corner columns (first and last chars are corner/bracket chars)
                let start = self.x + 1;
                let end = (self.x + self.width).saturating_sub(2);
                (start, end.max(start))
            }
            NodeFace::Left | NodeFace::Right => {
                // Exclude corner rows (first and last rows are corner chars)
                let start = self.y + 1;
                let end = (self.y + self.height).saturating_sub(2);
                (start, end.max(start))
            }
        }
    }

    /// Returns the fixed coordinate for a face.
    /// Top/Bottom: the y-coordinate of that edge. Left/Right: the x-coordinate.
    pub fn face_fixed_coord(&self, face: &NodeFace) -> usize {
        match face {
            NodeFace::Top => self.y,
            NodeFace::Bottom => self.y + self.height.saturating_sub(1),
            NodeFace::Left => self.x,
            NodeFace::Right => self.x + self.width.saturating_sub(1),
        }
    }
}

/// Calculate the dimensions needed to render a node.
///
/// All shapes use the same formula: width = label_len + 4 (2 for borders/delimiters,
/// 2 for padding), height = 3 (top border, label row, bottom border).
pub fn node_dimensions(node: &Node) -> (usize, usize) {
    let label_len = node.label.chars().count();
    (label_len + 4, 3)
}

/// Render a node at the specified position.
///
/// Returns the bounding box of the rendered node.
pub fn render_node(
    canvas: &mut Canvas,
    node: &Node,
    x: usize,
    y: usize,
    charset: &CharSet,
) -> NodeBounds {
    let (width, height) = node_dimensions(node);
    let label = &node.label;
    let label_len = label.chars().count();

    match node.shape {
        Shape::Rectangle => {
            let corners = (
                charset.corner_tl,
                charset.corner_tr,
                charset.corner_bl,
                charset.corner_br,
            );
            render_box(canvas, x, y, width, height, label, charset, corners);
        }
        Shape::Round => {
            let corners = (
                charset.round_tl,
                charset.round_tr,
                charset.round_bl,
                charset.round_br,
            );
            render_box(canvas, x, y, width, height, label, charset, corners);
        }
        Shape::Diamond => {
            render_diamond(canvas, x, y, width, label_len, label, charset);
        }
    }

    // Mark all cells as part of a node
    for dy in 0..height {
        for dx in 0..width {
            canvas.mark_as_node(x + dx, y + dy);
        }
    }

    NodeBounds {
        x,
        y,
        width,
        height,
    }
}

/// Render a box shape (rectangle or rounded rectangle).
///
/// The only difference between rectangle and rounded shapes is the corner
/// characters, so this shared function handles both.
#[allow(clippy::too_many_arguments)]
fn render_box(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label: &str,
    charset: &CharSet,
    corners: (char, char, char, char),
) {
    let (tl, tr, bl, br) = corners;

    // Top border
    canvas.set(x, y, tl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, y, charset.horizontal);
    }
    canvas.set(x + width - 1, y, tr);

    // Middle row with label
    let mid_y = y + height / 2;
    canvas.set(x, mid_y, charset.vertical);
    let label_start = x + (width - label.chars().count()) / 2;
    canvas.write_str(label_start, mid_y, label);
    canvas.set(x + width - 1, mid_y, charset.vertical);

    // Bottom border
    let bot_y = y + height - 1;
    canvas.set(x, bot_y, bl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, bot_y, charset.horizontal);
    }
    canvas.set(x + width - 1, bot_y, br);
}

/// Render a diamond shape.
///
/// Rendered as a rectangle with angle brackets on the sides:
/// ```text
/// ┌───────────┐
/// < Christmas >
/// └───────────┘
/// ```
fn render_diamond(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label_len: usize,
    label: &str,
    charset: &CharSet,
) {
    // Top border
    canvas.set(x, y, charset.corner_tl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, y, charset.horizontal);
    }
    canvas.set(x + width - 1, y, charset.corner_tr);

    // Middle row with label and angle brackets
    let mid_y = y + 1;
    canvas.set(x, mid_y, '<');
    let label_start = x + (width - label_len) / 2;
    canvas.write_str(label_start, mid_y, label);
    canvas.set(x + width - 1, mid_y, '>');

    // Bottom border
    let bot_y = y + 2;
    canvas.set(x, bot_y, charset.corner_bl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, bot_y, charset.horizontal);
    }
    canvas.set(x + width - 1, bot_y, charset.corner_br);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_dimensions_rectangle() {
        let node = Node::new("A").with_label("Start");
        let (w, h) = node_dimensions(&node);
        // "Start" is 5 chars, +4 = 9 width
        assert_eq!(w, 9);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_node_dimensions_round() {
        let node = Node::new("B")
            .with_label("Process")
            .with_shape(Shape::Round);
        let (w, h) = node_dimensions(&node);
        // "Process" is 7 chars, +4 = 11 width
        assert_eq!(w, 11);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_node_dimensions_diamond() {
        let node = Node::new("C").with_label("Yes").with_shape(Shape::Diamond);
        let (w, h) = node_dimensions(&node);
        // "Yes" is 3 chars, +4 = 7 width
        assert_eq!(w, 7);
        assert_eq!(h, 3);
    }

    #[test]
    fn test_render_rectangle() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("A").with_label("Start");
        let charset = CharSet::unicode();

        let bounds = render_node(&mut canvas, &node, 2, 1, &charset);

        assert_eq!(bounds.x, 2);
        assert_eq!(bounds.y, 1);
        assert_eq!(bounds.width, 9);
        assert_eq!(bounds.height, 3);

        let output = canvas.to_string();
        assert!(output.contains("┌───────┐"));
        assert!(output.contains("│ Start │"));
        assert!(output.contains("└───────┘"));
    }

    #[test]
    fn test_render_rectangle_ascii() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("A").with_label("Start");
        let charset = CharSet::ascii();

        render_node(&mut canvas, &node, 2, 1, &charset);

        let output = canvas.to_string();
        assert!(output.contains("+-------+"));
        assert!(output.contains("| Start |"));
    }

    #[test]
    fn test_render_round() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("B").with_label("Hi").with_shape(Shape::Round);
        let charset = CharSet::unicode();

        render_node(&mut canvas, &node, 2, 1, &charset);

        let output = canvas.to_string();
        assert!(output.contains("╭────╮"));
        assert!(output.contains("│ Hi │"));
        assert!(output.contains("╰────╯"));
    }

    #[test]
    fn test_render_diamond() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("C").with_label("?").with_shape(Shape::Diamond);
        let charset = CharSet::unicode();

        render_node(&mut canvas, &node, 2, 1, &charset);

        let output = canvas.to_string();
        assert!(output.contains("┌───┐"));
        assert!(output.contains("< ? >"));
        assert!(output.contains("└───┘"));
    }

    #[test]
    fn test_render_diamond_wide() {
        let mut canvas = Canvas::new(20, 5);
        let node = Node::new("B")
            .with_label("Decision")
            .with_shape(Shape::Diamond);
        let charset = CharSet::unicode();

        let bounds = render_node(&mut canvas, &node, 1, 1, &charset);

        assert_eq!(bounds.width, 12);
        assert_eq!(bounds.height, 3);

        let output = canvas.to_string();
        assert!(output.contains("┌──────────┐"));
        assert!(output.contains("< Decision >"));
        assert!(output.contains("└──────────┘"));
    }

    #[test]
    fn test_node_bounds_attachment_points() {
        let bounds = NodeBounds {
            x: 10,
            y: 5,
            width: 8,
            height: 3,
        };

        assert_eq!(bounds.center_x(), 14); // 10 + 8/2 = 14
        assert_eq!(bounds.center_y(), 6); // 5 + 3/2 = 6

        assert_eq!(bounds.top(), (14, 5));
        assert_eq!(bounds.bottom(), (14, 7)); // y + height - 1 = 5 + 3 - 1 = 7
        assert_eq!(bounds.left(), (10, 6));
        assert_eq!(bounds.right(), (17, 6)); // x + width - 1 = 10 + 8 - 1 = 17
    }

    #[test]
    fn test_face_extent_top_bottom() {
        let bounds = NodeBounds {
            x: 5,
            y: 10,
            width: 10,
            height: 3,
        };
        // Top/Bottom: exclude corners => x+1 to x+width-2 = 6 to 13
        assert_eq!(bounds.face_extent(&NodeFace::Top), (6, 13));
        assert_eq!(bounds.face_extent(&NodeFace::Bottom), (6, 13));
    }

    #[test]
    fn test_face_extent_left_right() {
        let bounds = NodeBounds {
            x: 5,
            y: 10,
            width: 10,
            height: 3,
        };
        // Left/Right: exclude corner rows => 11 to 11 (only middle row)
        assert_eq!(bounds.face_extent(&NodeFace::Left), (11, 11));
        assert_eq!(bounds.face_extent(&NodeFace::Right), (11, 11));
    }

    #[test]
    fn test_face_extent_narrow_node() {
        let bounds = NodeBounds {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        // width=2: start=1, end=max(0,1)=1 => (1, 1)
        assert_eq!(bounds.face_extent(&NodeFace::Top), (1, 1));
    }

    #[test]
    fn test_face_fixed_coord() {
        let bounds = NodeBounds {
            x: 5,
            y: 10,
            width: 10,
            height: 3,
        };
        assert_eq!(bounds.face_fixed_coord(&NodeFace::Top), 10);
        assert_eq!(bounds.face_fixed_coord(&NodeFace::Bottom), 12);
        assert_eq!(bounds.face_fixed_coord(&NodeFace::Left), 5);
        assert_eq!(bounds.face_fixed_coord(&NodeFace::Right), 14);
    }

    #[test]
    fn test_node_cells_marked_as_protected() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("A").with_label("X");
        let charset = CharSet::unicode();

        let bounds = render_node(&mut canvas, &node, 2, 1, &charset);

        // Check that cells within the node bounds are marked as protected
        for dy in 0..bounds.height {
            for dx in 0..bounds.width {
                let cell = canvas.get(bounds.x + dx, bounds.y + dy).unwrap();
                assert!(
                    cell.is_node,
                    "Cell at ({}, {}) should be marked as node",
                    dx, dy
                );
            }
        }
    }
}
