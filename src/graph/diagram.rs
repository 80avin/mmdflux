//! Diagram container holding nodes, edges, and layout direction.

use std::collections::HashMap;

use super::edge::Edge;
use super::node::Node;

/// Direction of the diagram layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Top to bottom (vertical, downward).
    #[default]
    TopDown,
    /// Bottom to top (vertical, upward).
    BottomTop,
    /// Left to right (horizontal, rightward).
    LeftRight,
    /// Right to left (horizontal, leftward).
    RightLeft,
}

/// A subgraph grouping of nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subgraph {
    /// Unique identifier for this subgraph.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Whether the title was explicitly set via bracket syntax `[Title]`.
    pub has_explicit_title: bool,
    /// IDs of nodes belonging to this subgraph.
    pub nodes: Vec<String>,
}

/// A complete flowchart diagram.
#[derive(Debug, Clone)]
pub struct Diagram {
    /// Layout direction.
    pub direction: Direction,
    /// Nodes indexed by their ID.
    pub nodes: HashMap<String, Node>,
    /// Edges connecting nodes.
    pub edges: Vec<Edge>,
    /// Subgraphs indexed by their ID.
    pub subgraphs: HashMap<String, Subgraph>,
}

impl Diagram {
    /// Create a new empty diagram.
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            nodes: HashMap::new(),
            edges: Vec::new(),
            subgraphs: HashMap::new(),
        }
    }

    /// Add a node to the diagram.
    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add an edge to the diagram.
    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Get all node IDs.
    pub fn node_ids(&self) -> impl Iterator<Item = &String> {
        self.nodes.keys()
    }

    /// Check if the diagram contains any subgraphs.
    pub fn has_subgraphs(&self) -> bool {
        !self.subgraphs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subgraph_construction() {
        let sg = Subgraph {
            id: "sg1".to_string(),
            title: "My Group".to_string(),
            has_explicit_title: true,
            nodes: vec!["A".to_string(), "B".to_string()],
        };
        assert_eq!(sg.id, "sg1");
        assert_eq!(sg.title, "My Group");
        assert_eq!(sg.nodes.len(), 2);
    }

    #[test]
    fn test_diagram_subgraphs_empty() {
        let diagram = Diagram::new(Direction::TopDown);
        assert!(diagram.subgraphs.is_empty());
        assert!(!diagram.has_subgraphs());
    }

    #[test]
    fn test_diagram_has_subgraphs() {
        let mut diagram = Diagram::new(Direction::TopDown);
        diagram.subgraphs.insert(
            "sg1".to_string(),
            Subgraph {
                id: "sg1".to_string(),
                title: "Group".to_string(),
                has_explicit_title: true,
                nodes: vec![],
            },
        );
        assert!(diagram.has_subgraphs());
    }
}
