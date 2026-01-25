//! Converts AST to graph data structures.

use super::diagram::{Diagram, Direction};
use super::edge::{Arrow, Edge, Stroke};
use super::node::{Node, Shape};
use crate::parser::{
    ConnectorSpec, Direction as ParseDirection, EdgeSpec, Flowchart, ShapeSpec, Statement, Vertex,
};

/// Build a Diagram from a parsed Flowchart.
pub fn build_diagram(flowchart: &Flowchart) -> Diagram {
    let direction = convert_direction(flowchart.direction);
    let mut diagram = Diagram::new(direction);

    for statement in &flowchart.statements {
        match statement {
            Statement::Vertex(vertex) => {
                add_vertex_to_diagram(&mut diagram, vertex);
            }
            Statement::Edge(edge_spec) => {
                // Add both nodes (they might be new or updates)
                add_vertex_to_diagram(&mut diagram, &edge_spec.from);
                add_vertex_to_diagram(&mut diagram, &edge_spec.to);

                // Create the edge
                let edge = convert_edge(edge_spec);
                diagram.add_edge(edge);
            }
        }
    }

    diagram
}

fn convert_direction(dir: ParseDirection) -> Direction {
    match dir {
        ParseDirection::TopDown => Direction::TopDown,
        ParseDirection::BottomTop => Direction::BottomTop,
        ParseDirection::LeftRight => Direction::LeftRight,
        ParseDirection::RightLeft => Direction::RightLeft,
    }
}

fn add_vertex_to_diagram(diagram: &mut Diagram, vertex: &Vertex) {
    // Check if node already exists
    if let Some(existing) = diagram.nodes.get_mut(&vertex.id) {
        // Update existing node if this vertex has more info
        if vertex.shape.is_some() && existing.label == existing.id {
            // Update label and shape from the vertex
            if let Some(shape_spec) = &vertex.shape {
                existing.label = shape_spec.text().to_string();
                existing.shape = convert_shape(shape_spec);
            }
        }
    } else {
        // Create new node
        let node = convert_vertex(vertex);
        diagram.add_node(node);
    }
}

fn convert_vertex(vertex: &Vertex) -> Node {
    match &vertex.shape {
        Some(shape_spec) => Node::new(&vertex.id)
            .with_label(shape_spec.text())
            .with_shape(convert_shape(shape_spec)),
        None => Node::new(&vertex.id),
    }
}

fn convert_shape(shape_spec: &ShapeSpec) -> Shape {
    match shape_spec {
        ShapeSpec::Rectangle(_) => Shape::Rectangle,
        ShapeSpec::Round(_) => Shape::Round,
        ShapeSpec::Diamond(_) => Shape::Diamond,
    }
}

fn convert_edge(edge_spec: &EdgeSpec) -> Edge {
    let (stroke, arrow, label) = convert_connector(&edge_spec.connector);

    let mut edge = Edge::new(&edge_spec.from.id, &edge_spec.to.id)
        .with_stroke(stroke)
        .with_arrow(arrow);

    if let Some(lbl) = label {
        edge = edge.with_label(lbl);
    }

    edge
}

fn convert_connector(connector: &ConnectorSpec) -> (Stroke, Arrow, Option<String>) {
    match connector {
        ConnectorSpec::SolidArrow => (Stroke::Solid, Arrow::Normal, None),
        ConnectorSpec::SolidArrowLabel(label) => {
            (Stroke::Solid, Arrow::Normal, Some(label.clone()))
        }
        ConnectorSpec::DottedArrow => (Stroke::Dotted, Arrow::Normal, None),
        ConnectorSpec::DottedArrowLabel(label) => {
            (Stroke::Dotted, Arrow::Normal, Some(label.clone()))
        }
        ConnectorSpec::ThickArrow => (Stroke::Thick, Arrow::Normal, None),
        ConnectorSpec::ThickArrowLabel(label) => {
            (Stroke::Thick, Arrow::Normal, Some(label.clone()))
        }
        ConnectorSpec::OpenLine => (Stroke::Solid, Arrow::None, None),
        ConnectorSpec::OpenLineLabel(label) => (Stroke::Solid, Arrow::None, Some(label.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_flowchart;

    #[test]
    fn test_build_simple_diagram() {
        let flowchart = parse_flowchart("graph TD\nA --> B\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::TopDown);
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);

        assert!(diagram.nodes.contains_key("A"));
        assert!(diagram.nodes.contains_key("B"));
    }

    #[test]
    fn test_build_diagram_with_shapes() {
        let flowchart = parse_flowchart("graph LR\nA[Start] --> B{Decision}\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::LeftRight);

        let node_a = diagram.get_node("A").unwrap();
        assert_eq!(node_a.label, "Start");
        assert_eq!(node_a.shape, Shape::Rectangle);

        let node_b = diagram.get_node("B").unwrap();
        assert_eq!(node_b.label, "Decision");
        assert_eq!(node_b.shape, Shape::Diamond);
    }

    #[test]
    fn test_build_diagram_with_edge_label() {
        let flowchart = parse_flowchart("graph TD\nA -->|yes| B\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.edges.len(), 1);
        assert_eq!(diagram.edges[0].label, Some("yes".to_string()));
    }

    #[test]
    fn test_build_diagram_deduplicates_nodes() {
        let flowchart = parse_flowchart("graph TD\nA --> B\nB --> C\n").unwrap();
        let diagram = build_diagram(&flowchart);

        // B appears in both edges but should only be one node
        assert_eq!(diagram.nodes.len(), 3);
        assert_eq!(diagram.edges.len(), 2);
    }

    #[test]
    fn test_build_diagram_node_update() {
        // First edge has A without shape, then A[Start] appears
        let flowchart = parse_flowchart("graph TD\nA --> B\nA[Start] --> C\n").unwrap();
        let diagram = build_diagram(&flowchart);

        let node_a = diagram.get_node("A").unwrap();
        // Should have the shape info from the second occurrence
        assert_eq!(node_a.label, "Start");
        assert_eq!(node_a.shape, Shape::Rectangle);
    }

    #[test]
    fn test_build_diagram_edge_strokes() {
        let flowchart = parse_flowchart("graph TD\nA --> B\nB -.-> C\nC ==> D\nD --- E\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.edges[0].stroke, Stroke::Solid);
        assert_eq!(diagram.edges[0].arrow, Arrow::Normal);

        assert_eq!(diagram.edges[1].stroke, Stroke::Dotted);
        assert_eq!(diagram.edges[1].arrow, Arrow::Normal);

        assert_eq!(diagram.edges[2].stroke, Stroke::Thick);
        assert_eq!(diagram.edges[2].arrow, Arrow::Normal);

        assert_eq!(diagram.edges[3].stroke, Stroke::Solid);
        assert_eq!(diagram.edges[3].arrow, Arrow::None);
    }

    #[test]
    fn test_build_diagram_from_chain() {
        let flowchart = parse_flowchart("graph TD\nA --> B --> C --> D\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes.len(), 4);
        assert_eq!(diagram.edges.len(), 3);
    }

    #[test]
    fn test_build_diagram_from_ampersand() {
        let flowchart = parse_flowchart("graph TD\nA & B --> C\n").unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes.len(), 3);
        assert_eq!(diagram.edges.len(), 2);
    }
}
