//! Node types and shape definitions.

/// Shape of a node in the diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// Rectangle shape: [text]
    #[default]
    Rectangle,
    /// Rounded rectangle shape: (text)
    Round,
    /// Diamond/decision shape: {text}
    Diamond,
}

/// A node in the flowchart diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Unique identifier for this node.
    pub id: String,
    /// Display label (defaults to id if not specified).
    pub label: String,
    /// Shape of the node.
    pub shape: Shape,
}

impl Node {
    /// Create a new node with just an ID (label defaults to ID, shape to Rectangle).
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            shape: Shape::default(),
        }
    }

    /// Set the label for this node.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Set the shape for this node.
    pub fn with_shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }
}
