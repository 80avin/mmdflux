//! Node shape rendering.

use crate::graph::{Node, Shape};

use super::canvas::Canvas;
use super::chars::CharSet;

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
}

/// Calculate the dimensions needed to render a node.
pub fn node_dimensions(node: &Node) -> (usize, usize) {
    let label_len = node.label.chars().count();

    match node.shape {
        Shape::Rectangle => {
            // +--label--+
            // |         |
            // +---------+
            let width = label_len + 4; // 2 for borders, 2 for padding
            let height = 3;
            (width, height)
        }
        Shape::Round => {
            // ( label )
            // Rendered as: ( label )
            let width = label_len + 4; // 2 for parens, 2 for padding
            let height = 3;
            (width, height)
        }
        Shape::Diamond => {
            //    /\
            //   /  \
            //  < lbl >
            //   \  /
            //    \/
            // Width needs to accommodate the label plus the diamond slopes
            let width = label_len + 4; // 2 for < >, 2 for padding
            let height = 3;
            (width, height)
        }
    }
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
            render_rectangle(canvas, x, y, width, height, label, charset);
        }
        Shape::Round => {
            render_round(canvas, x, y, width, height, label, charset);
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

/// Render a rectangle shape.
fn render_rectangle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label: &str,
    charset: &CharSet,
) {
    // Top border
    canvas.set(x, y, charset.corner_tl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, y, charset.horizontal);
    }
    canvas.set(x + width - 1, y, charset.corner_tr);

    // Middle row with label
    let mid_y = y + height / 2;
    canvas.set(x, mid_y, charset.vertical);
    // Center the label
    let label_start = x + (width - label.chars().count()) / 2;
    canvas.write_str(label_start, mid_y, label);
    canvas.set(x + width - 1, mid_y, charset.vertical);

    // Bottom border
    let bot_y = y + height - 1;
    canvas.set(x, bot_y, charset.corner_bl);
    for dx in 1..width - 1 {
        canvas.set(x + dx, bot_y, charset.horizontal);
    }
    canvas.set(x + width - 1, bot_y, charset.corner_br);
}

/// Render a round (rounded rectangle) shape.
fn render_round(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label: &str,
    charset: &CharSet,
) {
    // For ASCII, we use ( ) as the round shape indicators
    // Top border with rounded appearance
    canvas.set(x, y, '(');
    for dx in 1..width - 1 {
        canvas.set(x + dx, y, charset.horizontal);
    }
    canvas.set(x + width - 1, y, ')');

    // Middle row with label
    let mid_y = y + height / 2;
    canvas.set(x, mid_y, '(');
    let label_start = x + (width - label.chars().count()) / 2;
    canvas.write_str(label_start, mid_y, label);
    canvas.set(x + width - 1, mid_y, ')');

    // Bottom border
    let bot_y = y + height - 1;
    canvas.set(x, bot_y, '(');
    for dx in 1..width - 1 {
        canvas.set(x + dx, bot_y, charset.horizontal);
    }
    canvas.set(x + width - 1, bot_y, ')');
}

/// Render a diamond shape.
fn render_diamond(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label_len: usize,
    label: &str,
    _charset: &CharSet,
) {
    // Diamond rendered as:
    //   /\
    //  < X >
    //   \/
    // Center the /\ over the middle of the < > span
    // For width W, the center position for /\ is at x + (W-2)/2
    let center_x = x + (width - 2) / 2;

    // Top point
    canvas.set(center_x, y, '/');
    canvas.set(center_x + 1, y, '\\');

    // Middle row with label
    let mid_y = y + 1;
    canvas.set(x, mid_y, '<');
    let label_start = x + (width - label_len) / 2;
    canvas.write_str(label_start, mid_y, label);
    canvas.set(x + width - 1, mid_y, '>');

    // Bottom point
    let bot_y = y + 2;
    canvas.set(center_x, bot_y, '\\');
    canvas.set(center_x + 1, bot_y, '/');
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
        let node = Node::new("B").with_label("Process").with_shape(Shape::Round);
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
        assert!(output.contains("(────)"));
        assert!(output.contains("( Hi )"));
    }

    #[test]
    fn test_render_diamond() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("C").with_label("?").with_shape(Shape::Diamond);
        let charset = CharSet::unicode();

        render_node(&mut canvas, &node, 2, 1, &charset);

        let output = canvas.to_string();
        assert!(output.contains("/\\"));
        assert!(output.contains("< ? >"));
        assert!(output.contains("\\/"));
    }

    #[test]
    fn test_render_diamond_wide() {
        let mut canvas = Canvas::new(20, 5);
        let node = Node::new("B").with_label("Decision").with_shape(Shape::Diamond);
        let charset = CharSet::unicode();

        let bounds = render_node(&mut canvas, &node, 1, 1, &charset);

        assert_eq!(bounds.width, 12);
        assert_eq!(bounds.height, 3);

        let output = canvas.to_string();
        assert!(output.contains("/\\"));
        assert!(output.contains("< Decision >"));
        assert!(output.contains("\\/"));
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
    fn test_node_cells_marked_as_protected() {
        let mut canvas = Canvas::new(15, 5);
        let node = Node::new("A").with_label("X");
        let charset = CharSet::unicode();

        let bounds = render_node(&mut canvas, &node, 2, 1, &charset);

        // Check that cells within the node bounds are marked as protected
        for dy in 0..bounds.height {
            for dx in 0..bounds.width {
                let cell = canvas.get(bounds.x + dx, bounds.y + dy).unwrap();
                assert!(cell.is_node, "Cell at ({}, {}) should be marked as node", dx, dy);
            }
        }
    }
}
