//! Canvas for ASCII rendering with cell-based drawing.

use super::chars::CharSet;

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
}

impl Cell {
    /// Create an empty cell (space character).
    pub fn empty() -> Self {
        Self {
            ch: ' ',
            connections: Connections::none(),
            is_node: false,
        }
    }

    /// Create a cell with a character.
    pub fn with_char(ch: char) -> Self {
        Self {
            ch,
            connections: Connections::none(),
            is_node: false,
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
    /// charset to determine the appropriate junction character.
    ///
    /// Returns `false` if the position is out of bounds or the cell is protected.
    pub fn set_with_connection(
        &mut self,
        x: usize,
        y: usize,
        connections: Connections,
        charset: &CharSet,
    ) -> bool {
        if let Some(cell) = self.get_mut(x, y) {
            if cell.is_node {
                return false;
            }
            cell.connections.merge(connections);
            cell.ch = charset.junction(cell.connections);
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

    /// Write a string starting at the given position (left to right).
    ///
    /// Characters that fall outside the canvas are ignored.
    pub fn write_str(&mut self, x: usize, y: usize, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i, y, ch);
        }
    }

    /// Convert the canvas to a string.
    ///
    /// Trailing spaces on each line are trimmed, but blank lines are preserved.
    pub fn to_string(&self) -> String {
        self.cells
            .iter()
            .map(|row| {
                let line: String = row.iter().map(|cell| cell.ch).collect();
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
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
        assert!(!canvas.set_with_connection(2, 2, connections, &charset));
        // Original character should be preserved
        assert_eq!(canvas.get(2, 2).unwrap().ch, '#');
    }
}
