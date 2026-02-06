//! Canvas for ASCII rendering with cell-based drawing.

use std::fmt;

use super::chars::CharSet;
use crate::graph::Stroke;

/// Tracks connections in four directions for a cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Connections {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Connections {
    /// Create connections with no directions set.
    pub fn none() -> Self {
        Self::default()
    }

    /// Merge another set of connections into this one.
    pub fn merge(&mut self, other: Connections) {
        self.up |= other.up;
        self.down |= other.down;
        self.left |= other.left;
        self.right |= other.right;
    }

    /// Count how many directions are connected.
    pub fn count(&self) -> u8 {
        self.up as u8 + self.down as u8 + self.left as u8 + self.right as u8
    }
}

/// A single cell on the canvas.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// The character displayed in this cell.
    pub ch: char,
    /// Connection metadata for junction resolution.
    pub connections: Connections,
    /// Whether this cell is part of a node (protected from edge overwrite).
    pub is_node: bool,
    /// Whether this cell is part of an edge path (protected from label overwrite).
    pub is_edge: bool,
    /// Whether this cell is part of a subgraph border (NOT protected from overwrite).
    pub is_subgraph_border: bool,
    /// Whether this cell contains subgraph title text (protected from edge overwrite).
    pub is_subgraph_title: bool,
}

impl Cell {
    /// Create an empty cell (space character).
    pub fn empty() -> Self {
        Self {
            ch: ' ',
            ..Self::default()
        }
    }
}

/// A 2D canvas for ASCII art rendering.
#[derive(Debug, Clone)]
pub struct Canvas {
    cells: Vec<Vec<Cell>>,
    width: usize,
    height: usize,
}

impl Canvas {
    /// Create a new canvas with the given dimensions.
    ///
    /// All cells are initialized to empty (space) characters.
    pub fn new(width: usize, height: usize) -> Self {
        let cells = vec![vec![Cell::empty(); width]; height];
        Self {
            cells,
            width,
            height,
        }
    }

    /// Get the width of the canvas.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the height of the canvas.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Expand the canvas width if the new width exceeds the current width.
    ///
    /// New cells are initialized to empty (space) characters.
    pub fn expand_width(&mut self, new_width: usize) {
        if new_width > self.width {
            for row in &mut self.cells {
                row.resize(new_width, Cell::empty());
            }
            self.width = new_width;
        }
    }

    /// Get the cell at the given position.
    ///
    /// Returns `None` if the position is out of bounds.
    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        self.cells.get(y).and_then(|row| row.get(x))
    }

    /// Get a mutable reference to the cell at the given position.
    ///
    /// Returns `None` if the position is out of bounds.
    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        self.cells.get_mut(y).and_then(|row| row.get_mut(x))
    }

    /// Set the character at the given position.
    ///
    /// Returns `false` if the position is out of bounds.
    pub fn set(&mut self, x: usize, y: usize, ch: char) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            if cell.is_subgraph_title {
                return false;
            }
            cell.ch = ch;
            true
        } else {
            false
        }
    }

    /// Set a cell with full control over all properties.
    ///
    /// Returns `false` if the position is out of bounds.
    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) -> bool {
        if y < self.height && x < self.width {
            self.cells[y][x] = cell;
            true
        } else {
            false
        }
    }

    /// Set a cell with connection tracking for junction resolution.
    ///
    /// This merges the new connections with existing ones and uses the
    /// charset to determine the appropriate junction character (including
    /// dotted/thick stroke variants).
    /// Also marks the cell as an edge cell (protected from label overwrite).
    ///
    /// Returns `false` if the position is out of bounds or the cell is protected.
    pub fn set_with_connection(
        &mut self,
        x: usize,
        y: usize,
        connections: Connections,
        charset: &CharSet,
        stroke: Stroke,
    ) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            if cell.is_node || cell.is_subgraph_title || charset.is_arrow(cell.ch) {
                return false;
            }
            // If overwriting a subgraph border, infer its connections first
            // so the junction merges border + edge directions.
            if cell.is_subgraph_border {
                let border_conns = charset.infer_connections(cell.ch);
                cell.connections.merge(border_conns);
            }
            let existing_heavy = charset.is_heavy(cell.ch);
            cell.connections.merge(connections);
            let merged = cell.connections;
            let heavy = existing_heavy || stroke == Stroke::Thick;
            let horizontal_only = (merged.left || merged.right) && !merged.up && !merged.down;
            let vertical_only = (merged.up || merged.down) && !merged.left && !merged.right;
            cell.ch = if heavy {
                charset.junction_heavy(merged)
            } else if stroke == Stroke::Dotted && (horizontal_only || vertical_only) {
                if horizontal_only {
                    charset.dotted_horizontal
                } else {
                    charset.dotted_vertical
                }
            } else {
                charset.junction(merged)
            };
            cell.is_edge = true;
            true
        } else {
            false
        }
    }

    /// Mark a cell as part of a node (protected from edge overwrite).
    pub fn mark_as_node(&mut self, x: usize, y: usize) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            cell.is_node = true;
            true
        } else {
            false
        }
    }

    /// Set a cell as a subgraph border character.
    ///
    /// Border cells are NOT protected from overwrite by nodes or edges.
    pub fn set_subgraph_border(&mut self, x: usize, y: usize, ch: char) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            cell.ch = ch;
            cell.is_subgraph_border = true;
            true
        } else {
            false
        }
    }

    /// Set a cell as a subgraph title character (protected from edge overwrite).
    pub fn set_subgraph_title_char(&mut self, x: usize, y: usize, ch: char) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            cell.ch = ch;
            cell.is_subgraph_border = true;
            cell.is_subgraph_title = true;
            true
        } else {
            false
        }
    }

    /// Write a string starting at the given position (left to right).
    ///
    /// Characters that fall outside the canvas are ignored.
    pub fn write_str(&mut self, x: usize, y: usize, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i, y, ch);
        }
    }
}

impl fmt::Display for Canvas {
    /// Convert the canvas to a string.
    ///
    /// Trailing spaces on each line are trimmed, and common leading whitespace
    /// is stripped from all lines so the diagram is left-aligned.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self
            .cells
            .iter()
            .map(|row| {
                let line: String = row.iter().map(|cell| cell.ch).collect();
                line.trim_end().to_string()
            })
            .collect();

        // Trim leading and trailing empty rows
        let first_non_empty = lines.iter().position(|line| !line.is_empty()).unwrap_or(0);
        let last_non_empty = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .unwrap_or(lines.len().saturating_sub(1));
        let lines = &lines[first_non_empty..=last_non_empty];

        // Find the minimum leading whitespace across all non-empty lines
        let min_indent = lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        if min_indent == 0 {
            return write!(f, "{}", lines.join("\n"));
        }

        // Strip common leading whitespace
        let result: String = lines
            .iter()
            .map(|line| {
                if line.len() > min_indent {
                    &line[min_indent..]
                } else {
                    line.as_str()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        write!(f, "{}", result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_new() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.width(), 10);
        assert_eq!(canvas.height(), 5);
    }

    #[test]
    fn test_canvas_get_set() {
        let mut canvas = Canvas::new(5, 5);
        assert!(canvas.set(2, 3, 'X'));
        assert_eq!(canvas.get(2, 3).unwrap().ch, 'X');
    }

    #[test]
    fn test_canvas_out_of_bounds() {
        let mut canvas = Canvas::new(5, 5);
        assert!(!canvas.set(10, 10, 'X'));
        assert!(canvas.get(10, 10).is_none());
    }

    #[test]
    fn test_canvas_write_str() {
        let mut canvas = Canvas::new(10, 3);
        canvas.write_str(2, 1, "Hello");
        assert_eq!(canvas.get(2, 1).unwrap().ch, 'H');
        assert_eq!(canvas.get(6, 1).unwrap().ch, 'o');
    }

    #[test]
    fn test_canvas_to_string() {
        let mut canvas = Canvas::new(5, 3);
        canvas.set(0, 0, 'A');
        canvas.set(4, 0, 'B');
        canvas.set(2, 2, 'C');
        let output = canvas.to_string();
        assert_eq!(output, "A   B\n\n  C");
    }

    #[test]
    fn test_canvas_to_string_trims_trailing_spaces() {
        let mut canvas = Canvas::new(10, 2);
        canvas.write_str(0, 0, "Hi");
        canvas.write_str(0, 1, "There");
        let output = canvas.to_string();
        assert_eq!(output, "Hi\nThere");
    }

    #[test]
    fn test_connections_merge() {
        let mut c1 = Connections {
            up: true,
            down: false,
            left: false,
            right: false,
        };
        let c2 = Connections {
            up: false,
            down: true,
            left: false,
            right: true,
        };
        c1.merge(c2);
        assert!(c1.up);
        assert!(c1.down);
        assert!(!c1.left);
        assert!(c1.right);
    }

    #[test]
    fn test_connections_count() {
        let c = Connections {
            up: true,
            down: true,
            left: false,
            right: true,
        };
        assert_eq!(c.count(), 3);
    }

    #[test]
    fn test_canvas_trims_leading_empty_rows() {
        let mut canvas = Canvas::new(5, 5);
        // Content only on rows 2-3, leaving rows 0-1 empty above
        canvas.write_str(0, 2, "Hello");
        canvas.write_str(0, 3, "World");
        let output = canvas.to_string();
        // First line of output should be content, not blank
        assert_eq!(output, "Hello\nWorld");
    }

    #[test]
    fn test_canvas_trims_trailing_empty_rows() {
        let mut canvas = Canvas::new(5, 5);
        // Content on rows 0-1, rows 2-4 empty below
        canvas.write_str(0, 0, "Hello");
        canvas.write_str(0, 1, "World");
        let output = canvas.to_string();
        assert_eq!(output, "Hello\nWorld");
    }

    #[test]
    fn test_canvas_preserves_interior_empty_rows() {
        let mut canvas = Canvas::new(5, 4);
        // Content on rows 0 and 2, row 1 empty (interior gap)
        canvas.set(0, 0, 'A');
        canvas.set(0, 2, 'B');
        let output = canvas.to_string();
        assert_eq!(output, "A\n\nB");
    }

    #[test]
    fn test_cell_subgraph_border_default_false() {
        let cell = Cell::empty();
        assert!(!cell.is_subgraph_border);
    }

    #[test]
    fn test_cell_subgraph_border_overwritable() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set_subgraph_border(3, 2, '─');
        assert_eq!(canvas.get(3, 2).unwrap().ch, '─');
        assert!(canvas.get(3, 2).unwrap().is_subgraph_border);

        // Node rendering should be able to overwrite border cells
        canvas.set(3, 2, '┌');
        assert_eq!(canvas.get(3, 2).unwrap().ch, '┌');
    }

    #[test]
    fn test_cell_is_node_protection() {
        let mut canvas = Canvas::new(5, 5);
        canvas.set(2, 2, '#');
        canvas.mark_as_node(2, 2);

        let charset = CharSet::unicode();
        let connections = Connections {
            up: true,
            down: true,
            left: false,
            right: false,
        };

        // Should return false because cell is protected
        assert!(!canvas.set_with_connection(2, 2, connections, &charset, Stroke::Solid));
        // Original character should be preserved
        assert_eq!(canvas.get(2, 2).unwrap().ch, '#');
    }

    // =========================================================================
    // Edge-border crossing tests (Plan 0026, Task 4.1)
    // =========================================================================

    #[test]
    fn test_edge_over_border_produces_junction() {
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(10, 5);

        // Draw a horizontal border
        canvas.set_subgraph_border(0, 2, '─');
        canvas.set_subgraph_border(1, 2, '─');
        canvas.set_subgraph_border(2, 2, '─');
        canvas.set_subgraph_border(3, 2, '─');

        // Draw a vertical edge crossing the border at x=2
        let conn_ud = Connections {
            up: true,
            down: true,
            left: false,
            right: false,
        };
        canvas.set_with_connection(2, 1, conn_ud, &charset, Stroke::Solid);
        canvas.set_with_connection(2, 2, conn_ud, &charset, Stroke::Solid);
        canvas.set_with_connection(2, 3, conn_ud, &charset, Stroke::Solid);

        // At the crossing point (2, 2), should be a junction ┼
        // (up+down from edge + left+right from border)
        let cell = canvas.get(2, 2).unwrap();
        assert_eq!(
            cell.ch, '┼',
            "Edge crossing border should produce junction, got: {}",
            cell.ch
        );
    }

    // =========================================================================
    // Title Protection Tests (Plan 0028, Task 3.1)
    // =========================================================================

    #[test]
    fn edge_does_not_overwrite_title_text() {
        use crate::render::chars::CharSet;
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(20, 5);

        // Simulate a subgraph border with embedded title at row 0: ┌─ Test ─┐
        canvas.set_subgraph_border(0, 0, '┌');
        canvas.set_subgraph_border(1, 0, '─');
        canvas.set_subgraph_border(2, 0, ' ');
        // Title characters
        canvas.set_subgraph_title_char(3, 0, 'T');
        canvas.set_subgraph_title_char(4, 0, 'e');
        canvas.set_subgraph_title_char(5, 0, 's');
        canvas.set_subgraph_title_char(6, 0, 't');
        canvas.set_subgraph_border(7, 0, ' ');
        canvas.set_subgraph_border(8, 0, '─');
        canvas.set_subgraph_border(9, 0, '┐');

        // Try to draw a vertical edge through the title at column 4
        let conns = Connections {
            up: true,
            down: true,
            left: false,
            right: false,
        };
        let overwritten = canvas.set_with_connection(4, 0, conns, &charset, Stroke::Solid);

        // Title character should be protected
        assert!(
            !overwritten,
            "Title character should not be overwritten by edge"
        );
        assert_eq!(
            canvas.get(4, 0).unwrap().ch,
            'e',
            "Title 'e' should be preserved"
        );
    }

    #[test]
    fn edge_can_merge_with_non_title_border_segment() {
        use crate::render::chars::CharSet;
        let charset = CharSet::unicode();
        let mut canvas = Canvas::new(20, 5);

        // Place a horizontal border line segment (NOT title)
        canvas.set_subgraph_border(5, 0, '─');

        // A vertical edge should merge to form a junction
        let conns = Connections {
            up: true,
            down: true,
            left: false,
            right: false,
        };
        let merged = canvas.set_with_connection(5, 0, conns, &charset, Stroke::Solid);
        assert!(merged, "Edge should merge with non-title border segment");
        // Should produce a junction (┼ or similar)
        assert_ne!(
            canvas.get(5, 0).unwrap().ch,
            '─',
            "Should not remain a plain horizontal after edge merge"
        );
    }
}
