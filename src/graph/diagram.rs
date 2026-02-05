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
    /// Display title (defaults to id if not specified via bracket syntax).
    pub title: String,
    /// IDs of nodes belonging to this subgraph.
    pub nodes: Vec<String>,
    /// Parent subgraph ID (None if top-level).
    pub parent: Option<String>,
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
    /// Subgraph IDs in parse order (inner-first / post-order).
    pub subgraph_order: Vec<String>,
}

impl Diagram {
    /// Create a new empty diagram.
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            nodes: HashMap::new(),
            edges: Vec::new(),
            subgraphs: HashMap::new(),
            subgraph_order: Vec::new(),
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

    /// Check if an ID corresponds to a subgraph (compound node).
    pub fn is_subgraph(&self, id: &str) -> bool {
        self.subgraphs.contains_key(id)
    }

    /// Return the IDs of subgraphs whose parent is `parent_id`.
    pub fn subgraph_children(&self, parent_id: &str) -> Vec<&String> {
        self.subgraphs
            .values()
            .filter(|sg| sg.parent.as_deref() == Some(parent_id))
            .map(|sg| &sg.id)
            .collect()
    }

    /// Return the nesting depth of a subgraph (0 = top-level).
    pub fn subgraph_depth(&self, sg_id: &str) -> usize {
        let mut depth = 0;
        let mut current = sg_id;
        while let Some(parent) = self
            .subgraphs
            .get(current)
            .and_then(|sg| sg.parent.as_deref())
        {
            depth += 1;
            current = parent;
        }
        depth
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

            nodes: vec!["A".to_string(), "B".to_string()],
            parent: None,
        };
        assert_eq!(sg.id, "sg1");
        assert_eq!(sg.title, "My Group");
        assert_eq!(sg.nodes.len(), 2);
    }

    #[test]
    fn test_subgraph_children() {
        use crate::graph::builder::build_diagram;
        use crate::parser::parse_flowchart;
        let input = "graph TD\nsubgraph outer[Outer]\nA\nsubgraph inner[Inner]\nB\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        let children = diagram.subgraph_children("outer");
        assert_eq!(children.len(), 1);
        assert!(children.contains(&&"inner".to_string()));
        assert!(diagram.subgraph_children("inner").is_empty());
    }

    #[test]
    fn test_subgraph_depth() {
        use crate::graph::builder::build_diagram;
        use crate::parser::parse_flowchart;
        let input = "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        assert_eq!(diagram.subgraph_depth("outer"), 0);
        assert_eq!(diagram.subgraph_depth("inner"), 1);
    }

    #[test]
    fn test_subgraph_has_parent_field() {
        let sg = Subgraph {
            id: "inner".to_string(),
            title: "Inner".to_string(),
            nodes: vec!["A".to_string()],
            parent: Some("outer".to_string()),
        };
        assert_eq!(sg.parent, Some("outer".to_string()));
    }

    #[test]
    fn subgraph_parse_order_is_postorder() {
        use crate::graph::builder::build_diagram;
        use crate::parser::parse_flowchart;
        let input = include_str!("../../tests/fixtures/external_node_subgraph.mmd");
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(
            diagram.subgraph_order,
            vec![
                "us-east".to_string(),
                "us-west".to_string(),
                "Cloud".to_string(),
            ]
        );
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

                nodes: vec![],
                parent: None,
            },
        );
        assert!(diagram.has_subgraphs());
    }
}
