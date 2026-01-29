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
    /// Parent subgraph ID, if this node belongs to a subgraph.
    pub parent: Option<String>,
}

impl Node {
    /// Create a new node with just an ID (label defaults to ID, shape to Rectangle).
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            shape: Shape::default(),
            parent: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_parent_default_none() {
        let node = Node::new("A");
        assert_eq!(node.parent, None);
    }

    #[test]
    fn test_node_parent_set() {
        let mut node = Node::new("A");
        node.parent = Some("sg1".to_string());
        assert_eq!(node.parent, Some("sg1".to_string()));
    }
}
