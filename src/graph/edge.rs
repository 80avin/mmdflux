//! Edge types including stroke styles and arrow heads.

/// Style of the edge line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stroke {
    /// Normal solid line: --
    #[default]
    Solid,
    /// Dotted line: -.
    Dotted,
    /// Thick/bold line: ==
    Thick,
}

/// Type of arrow head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arrow {
    /// Arrow head pointing to target: >
    #[default]
    Normal,
    /// No arrow head (open line): -
    None,
}

/// An edge connecting two nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Optional label on the edge.
    pub label: Option<String>,
    /// Line style.
    pub stroke: Stroke,
    /// Arrow head type.
    pub arrow: Arrow,
}

impl Edge {
    /// Create a new edge with default style (solid line with arrow).
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
            stroke: Stroke::default(),
            arrow: Arrow::default(),
        }
    }

    /// Set the label for this edge.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the stroke style.
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = stroke;
        self
    }

    /// Set the arrow type.
    pub fn with_arrow(mut self, arrow: Arrow) -> Self {
        self.arrow = arrow;
        self
    }
}
