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

    // Corners (sharp)
    pub corner_tl: char, // top-left
    pub corner_tr: char, // top-right
    pub corner_bl: char, // bottom-left
    pub corner_br: char, // bottom-right

    // Corners (rounded)
    pub round_tl: char, // top-left rounded
    pub round_tr: char, // top-right rounded
    pub round_bl: char, // bottom-left rounded
    pub round_br: char, // bottom-right rounded

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

    // Heavy lines (thick)
    pub heavy_horizontal: char,
    pub heavy_vertical: char,
    pub heavy_corner_tl: char,
    pub heavy_corner_tr: char,
    pub heavy_corner_bl: char,
    pub heavy_corner_br: char,
    pub heavy_tee_down: char,
    pub heavy_tee_up: char,
    pub heavy_tee_right: char,
    pub heavy_tee_left: char,
    pub heavy_cross: char,
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
            round_tl: '╭',
            round_tr: '╮',
            round_bl: '╰',
            round_br: '╯',
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
            heavy_horizontal: '━',
            heavy_vertical: '┃',
            heavy_corner_tl: '┏',
            heavy_corner_tr: '┓',
            heavy_corner_bl: '┗',
            heavy_corner_br: '┛',
            heavy_tee_down: '┳',
            heavy_tee_up: '┻',
            heavy_tee_right: '┣',
            heavy_tee_left: '┫',
            heavy_cross: '╋',
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
            round_tl: '(',
            round_tr: ')',
            round_bl: '(',
            round_br: ')',
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
            heavy_horizontal: '-',
            heavy_vertical: '|',
            heavy_corner_tl: '+',
            heavy_corner_tr: '+',
            heavy_corner_bl: '+',
            heavy_corner_br: '+',
            heavy_tee_down: '+',
            heavy_tee_up: '+',
            heavy_tee_right: '+',
            heavy_tee_left: '+',
            heavy_cross: '+',
        }
    }

    /// Check if a character is an arrow character in this charset.
    pub fn is_arrow(&self, ch: char) -> bool {
        ch == self.arrow_up
            || ch == self.arrow_down
            || ch == self.arrow_left
            || ch == self.arrow_right
    }

    /// Infer connections from an existing box-drawing character.
    ///
    /// Returns the connections that the character implies (e.g., '─' implies
    /// left+right, '┌' implies down+right). Returns `Connections::none()`
    /// for unrecognized characters.
    pub fn infer_connections(&self, ch: char) -> Connections {
        if ch == self.horizontal || ch == self.heavy_horizontal || ch == self.dotted_horizontal {
            Connections {
                left: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.vertical || ch == self.heavy_vertical || ch == self.dotted_vertical {
            Connections {
                up: true,
                down: true,
                ..Connections::none()
            }
        } else if ch == self.corner_tl || ch == self.heavy_corner_tl {
            Connections {
                down: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.corner_tr || ch == self.heavy_corner_tr {
            Connections {
                down: true,
                left: true,
                ..Connections::none()
            }
        } else if ch == self.corner_bl || ch == self.heavy_corner_bl {
            Connections {
                up: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.corner_br || ch == self.heavy_corner_br {
            Connections {
                up: true,
                left: true,
                ..Connections::none()
            }
        } else if ch == self.tee_down || ch == self.heavy_tee_down {
            Connections {
                down: true,
                left: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.tee_up || ch == self.heavy_tee_up {
            Connections {
                up: true,
                left: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.tee_right || ch == self.heavy_tee_right {
            Connections {
                up: true,
                down: true,
                right: true,
                ..Connections::none()
            }
        } else if ch == self.tee_left || ch == self.heavy_tee_left {
            Connections {
                up: true,
                down: true,
                left: true,
                ..Connections::none()
            }
        } else if ch == self.cross || ch == self.heavy_cross {
            Connections {
                up: true,
                down: true,
                left: true,
                right: true,
            }
        } else {
            Connections::none()
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
            (true, true, false, true) => self.tee_right, // ├
            (true, true, true, false) => self.tee_left,  // ┤
            (false, true, true, true) => self.tee_down,  // ┬
            (true, false, true, true) => self.tee_up,    // ┴

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

    /// Get a heavy junction character based on connections.
    pub fn junction_heavy(&self, conn: Connections) -> char {
        match (conn.up, conn.down, conn.left, conn.right) {
            // Four-way
            (true, true, true, true) => self.heavy_cross,

            // T-junctions (three connections)
            (false, true, true, true) => self.heavy_tee_down,
            (true, false, true, true) => self.heavy_tee_up,
            (true, true, false, true) => self.heavy_tee_right,
            (true, true, true, false) => self.heavy_tee_left,

            // Straight lines (two connections)
            (true, true, false, false) => self.heavy_vertical,
            (false, false, true, true) => self.heavy_horizontal,

            // Corners (two connections)
            (false, true, false, true) => self.heavy_corner_tl,
            (false, true, true, false) => self.heavy_corner_tr,
            (true, false, false, true) => self.heavy_corner_bl,
            (true, false, true, false) => self.heavy_corner_br,

            // Single connection (fallback)
            (true, false, false, false) => self.heavy_vertical,
            (false, true, false, false) => self.heavy_vertical,
            (false, false, true, false) => self.heavy_horizontal,
            (false, false, false, true) => self.heavy_horizontal,

            // None
            _ => ' ',
        }
    }

    /// Check if a character is a heavy line/junction.
    pub fn is_heavy(&self, ch: char) -> bool {
        ch == self.heavy_horizontal
            || ch == self.heavy_vertical
            || ch == self.heavy_corner_tl
            || ch == self.heavy_corner_tr
            || ch == self.heavy_corner_bl
            || ch == self.heavy_corner_br
            || ch == self.heavy_tee_down
            || ch == self.heavy_tee_up
            || ch == self.heavy_tee_right
            || ch == self.heavy_tee_left
            || ch == self.heavy_cross
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

    // =========================================================================
    // infer_connections tests (Plan 0026, Task 4.1)
    // =========================================================================

    #[test]
    fn test_infer_connections_horizontal() {
        let cs = CharSet::unicode();
        let conns = cs.infer_connections('─');
        assert!(conns.left && conns.right);
        assert!(!conns.up && !conns.down);
    }

    #[test]
    fn test_infer_connections_vertical() {
        let cs = CharSet::unicode();
        let conns = cs.infer_connections('│');
        assert!(conns.up && conns.down);
        assert!(!conns.left && !conns.right);
    }

    #[test]
    fn test_infer_connections_corners() {
        let cs = CharSet::unicode();

        let tl = cs.infer_connections('┌');
        assert!(tl.down && tl.right);
        assert!(!tl.up && !tl.left);

        let tr = cs.infer_connections('┐');
        assert!(tr.down && tr.left);
        assert!(!tr.up && !tr.right);

        let bl = cs.infer_connections('└');
        assert!(bl.up && bl.right);
        assert!(!bl.down && !bl.left);

        let br = cs.infer_connections('┘');
        assert!(br.up && br.left);
        assert!(!br.down && !br.right);
    }

    #[test]
    fn test_infer_connections_cross() {
        let cs = CharSet::unicode();
        let conns = cs.infer_connections('┼');
        assert!(conns.up && conns.down && conns.left && conns.right);
    }

    #[test]
    fn test_infer_connections_unknown_returns_none() {
        let cs = CharSet::unicode();
        let conns = cs.infer_connections('X');
        assert_eq!(conns, Connections::none());
    }
}
