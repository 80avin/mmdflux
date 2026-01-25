//! Character sets for ASCII and Unicode box-drawing.

use super::canvas::Connections;

/// Character set for rendering.
///
/// Provides characters for lines, corners, junctions, and arrows.
#[derive(Debug, Clone)]
pub struct CharSet {
    // Straight lines
    pub horizontal: char,
    pub vertical: char,

    // Corners
    pub corner_tl: char, // top-left
    pub corner_tr: char, // top-right
    pub corner_bl: char, // bottom-left
    pub corner_br: char, // bottom-right

    // T-junctions
    pub tee_down: char,  // ┬ (connects left, right, down)
    pub tee_up: char,    // ┴ (connects left, right, up)
    pub tee_right: char, // ├ (connects up, down, right)
    pub tee_left: char,  // ┤ (connects up, down, left)

    // Cross
    pub cross: char, // ┼ (all four directions)

    // Arrows
    pub arrow_up: char,
    pub arrow_down: char,
    pub arrow_left: char,
    pub arrow_right: char,

    // Dotted lines
    pub dotted_horizontal: char,
    pub dotted_vertical: char,
}

impl CharSet {
    /// Unicode box-drawing character set.
    pub fn unicode() -> Self {
        Self {
            horizontal: '─',
            vertical: '│',
            corner_tl: '┌',
            corner_tr: '┐',
            corner_bl: '└',
            corner_br: '┘',
            tee_down: '┬',
            tee_up: '┴',
            tee_right: '├',
            tee_left: '┤',
            cross: '┼',
            arrow_up: '▲',
            arrow_down: '▼',
            arrow_left: '◄',
            arrow_right: '►',
            dotted_horizontal: '┄',
            dotted_vertical: '┆',
        }
    }

    /// ASCII-only character set.
    pub fn ascii() -> Self {
        Self {
            horizontal: '-',
            vertical: '|',
            corner_tl: '+',
            corner_tr: '+',
            corner_bl: '+',
            corner_br: '+',
            tee_down: '+',
            tee_up: '+',
            tee_right: '+',
            tee_left: '+',
            cross: '+',
            arrow_up: '^',
            arrow_down: 'v',
            arrow_left: '<',
            arrow_right: '>',
            dotted_horizontal: '-',
            dotted_vertical: ':',
        }
    }

    /// Get the appropriate junction character based on connections.
    ///
    /// This handles all combinations of up/down/left/right connections
    /// and returns the correct box-drawing character.
    pub fn junction(&self, conn: Connections) -> char {
        match (conn.up, conn.down, conn.left, conn.right) {
            // Four-way
            (true, true, true, true) => self.cross,

            // T-junctions (three connections)
            (true, true, false, true) => self.tee_right,  // ├
            (true, true, true, false) => self.tee_left,   // ┤
            (false, true, true, true) => self.tee_down,   // ┬
            (true, false, true, true) => self.tee_up,     // ┴

            // Corners (two connections, perpendicular)
            (false, true, false, true) => self.corner_tl, // ┌
            (false, true, true, false) => self.corner_tr, // ┐
            (true, false, false, true) => self.corner_bl, // └
            (true, false, true, false) => self.corner_br, // ┘

            // Straight lines (two connections, parallel)
            (true, true, false, false) => self.vertical,
            (false, false, true, true) => self.horizontal,

            // Single connections (endpoints)
            (true, false, false, false) => self.vertical,
            (false, true, false, false) => self.vertical,
            (false, false, true, false) => self.horizontal,
            (false, false, false, true) => self.horizontal,

            // No connections
            (false, false, false, false) => ' ',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unicode_charset() {
        let cs = CharSet::unicode();
        assert_eq!(cs.horizontal, '─');
        assert_eq!(cs.vertical, '│');
        assert_eq!(cs.corner_tl, '┌');
        assert_eq!(cs.arrow_down, '▼');
    }

    #[test]
    fn test_ascii_charset() {
        let cs = CharSet::ascii();
        assert_eq!(cs.horizontal, '-');
        assert_eq!(cs.vertical, '|');
        assert_eq!(cs.corner_tl, '+');
        assert_eq!(cs.arrow_down, 'v');
    }

    #[test]
    fn test_junction_cross() {
        let cs = CharSet::unicode();
        let conn = Connections {
            up: true,
            down: true,
            left: true,
            right: true,
        };
        assert_eq!(cs.junction(conn), '┼');
    }

    #[test]
    fn test_junction_tee_down() {
        let cs = CharSet::unicode();
        let conn = Connections {
            up: false,
            down: true,
            left: true,
            right: true,
        };
        assert_eq!(cs.junction(conn), '┬');
    }

    #[test]
    fn test_junction_tee_up() {
        let cs = CharSet::unicode();
        let conn = Connections {
            up: true,
            down: false,
            left: true,
            right: true,
        };
        assert_eq!(cs.junction(conn), '┴');
    }

    #[test]
    fn test_junction_tee_right() {
        let cs = CharSet::unicode();
        let conn = Connections {
            up: true,
            down: true,
            left: false,
            right: true,
        };
        assert_eq!(cs.junction(conn), '├');
    }

    #[test]
    fn test_junction_tee_left() {
        let cs = CharSet::unicode();
        let conn = Connections {
            up: true,
            down: true,
            left: true,
            right: false,
        };
        assert_eq!(cs.junction(conn), '┤');
    }

    #[test]
    fn test_junction_corners() {
        let cs = CharSet::unicode();

        // Top-left corner: down and right
        assert_eq!(
            cs.junction(Connections {
                up: false,
                down: true,
                left: false,
                right: true
            }),
            '┌'
        );

        // Top-right corner: down and left
        assert_eq!(
            cs.junction(Connections {
                up: false,
                down: true,
                left: true,
                right: false
            }),
            '┐'
        );

        // Bottom-left corner: up and right
        assert_eq!(
            cs.junction(Connections {
                up: true,
                down: false,
                left: false,
                right: true
            }),
            '└'
        );

        // Bottom-right corner: up and left
        assert_eq!(
            cs.junction(Connections {
                up: true,
                down: false,
                left: true,
                right: false
            }),
            '┘'
        );
    }

    #[test]
    fn test_junction_straight_lines() {
        let cs = CharSet::unicode();

        // Vertical
        assert_eq!(
            cs.junction(Connections {
                up: true,
                down: true,
                left: false,
                right: false
            }),
            '│'
        );

        // Horizontal
        assert_eq!(
            cs.junction(Connections {
                up: false,
                down: false,
                left: true,
                right: true
            }),
            '─'
        );
    }

    #[test]
    fn test_junction_no_connections() {
        let cs = CharSet::unicode();
        assert_eq!(cs.junction(Connections::none()), ' ');
    }
}
