//! Subgraph border rendering.

use std::collections::HashMap;

use super::canvas::Canvas;
use super::chars::CharSet;
use super::layout::SubgraphBounds;

/// Render subgraph border rectangles on the canvas.
///
/// Draws borders BEFORE nodes and edges so they appear in the background.
/// Cells are marked as `is_subgraph_border` (not protected from overwrite).
/// Title is placed above the top-left corner of the border.
pub fn render_subgraph_borders(
    canvas: &mut Canvas,
    subgraph_bounds: &HashMap<String, SubgraphBounds>,
    charset: &CharSet,
) {
    for bounds in subgraph_bounds.values() {
        let x = bounds.x;
        let y = bounds.y;
        let w = bounds.width;
        let h = bounds.height;

        // Draw title above the border
        if y > 0 {
            for (i, ch) in bounds.title.chars().enumerate() {
                canvas.set(x + i, y - 1, ch);
            }
        }

        // Top edge
        canvas.set_subgraph_border(x, y, charset.corner_tl);
        for dx in 1..w - 1 {
            canvas.set_subgraph_border(x + dx, y, charset.horizontal);
        }
        canvas.set_subgraph_border(x + w - 1, y, charset.corner_tr);

        // Sides
        for dy in 1..h - 1 {
            canvas.set_subgraph_border(x, y + dy, charset.vertical);
            canvas.set_subgraph_border(x + w - 1, y + dy, charset.vertical);
        }

        // Bottom edge
        canvas.set_subgraph_border(x, y + h - 1, charset.corner_bl);
        for dx in 1..w - 1 {
            canvas.set_subgraph_border(x + dx, y + h - 1, charset.horizontal);
        }
        canvas.set_subgraph_border(x + w - 1, y + h - 1, charset.corner_br);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_subgraph_border_characters() {
        let mut canvas = Canvas::new(20, 10);
        let bounds = SubgraphBounds {
            x: 2,
            y: 3,
            width: 10,
            height: 5,
            title: "Group".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);
        let charset = CharSet::unicode();

        render_subgraph_borders(&mut canvas, &map, &charset);

        // Verify corners
        assert_eq!(canvas.get(2, 3).unwrap().ch, charset.corner_tl);
        assert_eq!(canvas.get(11, 3).unwrap().ch, charset.corner_tr);
        assert_eq!(canvas.get(2, 7).unwrap().ch, charset.corner_bl);
        assert_eq!(canvas.get(11, 7).unwrap().ch, charset.corner_br);

        // Verify horizontal edges
        assert_eq!(canvas.get(5, 3).unwrap().ch, charset.horizontal);

        // Verify vertical edges
        assert_eq!(canvas.get(2, 5).unwrap().ch, charset.vertical);

        // Verify is_subgraph_border flag
        assert!(canvas.get(2, 3).unwrap().is_subgraph_border);
    }

    #[test]
    fn test_render_subgraph_title() {
        let mut canvas = Canvas::new(20, 10);
        let bounds = SubgraphBounds {
            x: 2,
            y: 3,
            width: 10,
            height: 5,
            title: "Group".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &CharSet::unicode());

        // Title should appear above the border (y - 1)
        assert_eq!(canvas.get(2, 2).unwrap().ch, 'G');
        assert_eq!(canvas.get(3, 2).unwrap().ch, 'r');
        assert_eq!(canvas.get(4, 2).unwrap().ch, 'o');
        assert_eq!(canvas.get(5, 2).unwrap().ch, 'u');
        assert_eq!(canvas.get(6, 2).unwrap().ch, 'p');
    }

    #[test]
    fn test_render_subgraph_ascii_mode() {
        let mut canvas = Canvas::new(20, 10);
        let bounds = SubgraphBounds {
            x: 2,
            y: 3,
            width: 10,
            height: 5,
            title: "Test".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &CharSet::ascii());

        assert_eq!(canvas.get(2, 3).unwrap().ch, '+');
        assert_eq!(canvas.get(5, 3).unwrap().ch, '-');
        assert_eq!(canvas.get(2, 5).unwrap().ch, '|');
    }
}
