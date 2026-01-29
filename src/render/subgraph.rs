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

        // Top edge with embedded title: ┌─ Title ─┐
        canvas.set_subgraph_border(x, y, charset.corner_tl);
        canvas.set_subgraph_border(x + w - 1, y, charset.corner_tr);

        let inner_width = w.saturating_sub(2); // space between corners
        if !bounds.title.is_empty() && inner_width >= 5 {
            // Prefix: "─ " (2 chars), Suffix: " ─" (2 chars) = 4 overhead
            canvas.set_subgraph_border(x + 1, y, charset.horizontal);
            canvas.set_subgraph_border(x + 2, y, ' ');

            let max_title_len = inner_width.saturating_sub(4);
            let title: String = bounds.title.chars().take(max_title_len).collect();
            for (i, ch) in title.chars().enumerate() {
                canvas.set_subgraph_border(x + 3 + i, y, ch);
            }
            let title_end = x + 3 + title.len();
            canvas.set_subgraph_border(title_end, y, ' ');

            // Fill remaining with horizontal lines
            for dx in (title_end + 1)..(x + w - 1) {
                canvas.set_subgraph_border(dx, y, charset.horizontal);
            }
        } else {
            // No title or too narrow: plain horizontal
            for dx in 1..(w - 1) {
                canvas.set_subgraph_border(x + dx, y, charset.horizontal);
            }
        }

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
            width: 13,
            height: 5,
            title: "Group".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);
        let charset = CharSet::unicode();

        render_subgraph_borders(&mut canvas, &map, &charset);

        // Verify corners
        assert_eq!(canvas.get(2, 3).unwrap().ch, charset.corner_tl);
        assert_eq!(canvas.get(14, 3).unwrap().ch, charset.corner_tr);
        assert_eq!(canvas.get(2, 7).unwrap().ch, charset.corner_bl);
        assert_eq!(canvas.get(14, 7).unwrap().ch, charset.corner_br);

        // Verify embedded title in top border: ┌─ Group ─...─┐
        // x+1=3 → '─', x+2=4 → ' ', x+3..x+7=5..9 → "Group", x+8=10 → ' '
        assert_eq!(canvas.get(3, 3).unwrap().ch, charset.horizontal);
        assert_eq!(canvas.get(4, 3).unwrap().ch, ' ');
        assert_eq!(canvas.get(5, 3).unwrap().ch, 'G');
        assert_eq!(canvas.get(9, 3).unwrap().ch, 'p');
        assert_eq!(canvas.get(10, 3).unwrap().ch, ' ');

        // Verify vertical edges
        assert_eq!(canvas.get(2, 5).unwrap().ch, charset.vertical);

        // Verify is_subgraph_border flag
        assert!(canvas.get(2, 3).unwrap().is_subgraph_border);
    }

    #[test]
    fn test_render_subgraph_title_embedded_in_border() {
        let mut canvas = Canvas::new(20, 10);
        let bounds = SubgraphBounds {
            x: 2,
            y: 3,
            width: 13,
            height: 5,
            title: "Group".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &CharSet::unicode());

        // Title embedded in top border row (y=3), not above it
        assert_eq!(canvas.get(5, 3).unwrap().ch, 'G');
        assert_eq!(canvas.get(6, 3).unwrap().ch, 'r');
        assert_eq!(canvas.get(7, 3).unwrap().ch, 'o');
        assert_eq!(canvas.get(8, 3).unwrap().ch, 'u');
        assert_eq!(canvas.get(9, 3).unwrap().ch, 'p');

        // Row above border should NOT have the title
        assert_ne!(canvas.get(5, 2).unwrap().ch, 'G');
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
        // ASCII mode: embedded title in top border
        assert_eq!(canvas.get(3, 3).unwrap().ch, '-'); // prefix dash
        assert_eq!(canvas.get(5, 3).unwrap().ch, 'T'); // title start
        assert_eq!(canvas.get(2, 5).unwrap().ch, '|');
    }

    // =========================================================================
    // Embedded Title Tests (Plan 0026, Task 2.1)
    // =========================================================================

    #[test]
    fn test_render_subgraph_embedded_title() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(20, 7);
        let bounds = SubgraphBounds {
            x: 2,
            y: 2,
            width: 14,
            height: 5,
            title: "Group".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &charset);

        let output = canvas.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // First line (after empty row trimming) is the top border with embedded title
        assert!(
            lines[0].contains("─ Group ─"),
            "Expected embedded title in top border, got: {}",
            lines[0]
        );

        // Side rows should NOT contain title text
        assert!(
            !lines[1].contains("Group"),
            "Title should not appear in side row, got: {}",
            lines[1]
        );
    }

    #[test]
    fn test_render_subgraph_title_at_y0() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(20, 7);
        let bounds = SubgraphBounds {
            x: 0,
            y: 0,
            width: 16,
            height: 5,
            title: "TopGroup".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &charset);

        let output = canvas.to_string();
        let lines: Vec<&str> = output.lines().collect();

        // Title should be visible even at y=0 (embedded in border)
        assert!(
            lines[0].contains("TopGroup"),
            "Title should render at y=0, got: {}",
            lines[0]
        );
    }

    #[test]
    fn test_render_subgraph_narrow_border_truncates_title() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(15, 5);
        let bounds = SubgraphBounds {
            x: 0,
            y: 0,
            width: 8,
            height: 5,
            title: "Very Long Title".to_string(),
        };
        let mut map = HashMap::new();
        map.insert("sg1".to_string(), bounds);

        render_subgraph_borders(&mut canvas, &map, &charset);

        let output = canvas.to_string();
        // Title should be truncated to fit within border
        assert!(
            !output.contains("Very Long Title"),
            "Full title should not appear in narrow border"
        );
        // Border corners should still be intact
        assert!(output.contains("┌"), "Top-left corner should exist");
        assert!(output.contains("┐"), "Top-right corner should exist");
    }
}
