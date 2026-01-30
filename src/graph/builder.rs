//! Converts AST to graph data structures.

use std::collections::HashSet;

use super::diagram::{Diagram, Direction, Subgraph};
use super::edge::{Arrow, Edge, Stroke};
use super::node::{Node, Shape};
use crate::parser::{
    ConnectorSpec, Direction as ParseDirection, EdgeSpec, Flowchart, ShapeSpec, Statement, Vertex,
};

/// Build a Diagram from a parsed Flowchart.
pub fn build_diagram(flowchart: &Flowchart) -> Diagram {
    let direction = convert_direction(flowchart.direction);
    let mut diagram = Diagram::new(direction);
    process_statements(&mut diagram, &flowchart.statements, None);
    diagram
}

fn process_statements(
    diagram: &mut Diagram,
    statements: &[Statement],
    parent_subgraph: Option<&str>,
) {
    for statement in statements {
        match statement {
            Statement::Vertex(vertex) => {
                add_vertex_to_diagram(diagram, vertex, parent_subgraph);
            }
            Statement::Edge(edge_spec) => {
                add_vertex_to_diagram(diagram, &edge_spec.from, parent_subgraph);
                add_vertex_to_diagram(diagram, &edge_spec.to, parent_subgraph);
                let edge = convert_edge(edge_spec);
                diagram.add_edge(edge);
            }
            Statement::Subgraph(sg_spec) => {
                process_statements(diagram, &sg_spec.statements, Some(&sg_spec.id));
                let node_ids = collect_node_ids(&sg_spec.statements);
                diagram.subgraphs.insert(
                    sg_spec.id.clone(),
                    Subgraph {
                        id: sg_spec.id.clone(),
                        title: sg_spec.title.clone(),
                        nodes: node_ids,
                        parent: parent_subgraph.map(|s| s.to_string()),
                    },
                );
            }
        }
    }
}

fn convert_direction(dir: ParseDirection) -> Direction {
    match dir {
        ParseDirection::TopDown => Direction::TopDown,
        ParseDirection::BottomTop => Direction::BottomTop,
        ParseDirection::LeftRight => Direction::LeftRight,
        ParseDirection::RightLeft => Direction::RightLeft,
    }
}

fn add_vertex_to_diagram(diagram: &mut Diagram, vertex: &Vertex, parent: Option<&str>) {
    if let Some(existing) = diagram.nodes.get_mut(&vertex.id) {
        // Update existing node if this vertex has more info
        if vertex.shape.is_some()
            && existing.label == existing.id
            && let Some(shape_spec) = &vertex.shape
        {
            existing.label = shape_spec.text().to_string();
            existing.shape = convert_shape(shape_spec);
        }
        // Set parent if provided and not already set
        if parent.is_some() && existing.parent.is_none() {
            existing.parent = parent.map(|s| s.to_string());
        }
    } else {
        let mut node = convert_vertex(vertex);
        node.parent = parent.map(|s| s.to_string());
        diagram.add_node(node);
    }
}

fn collect_node_ids(statements: &[Statement]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for stmt in statements {
        let new_ids: Vec<String> = match stmt {
            Statement::Vertex(v) => vec![v.id.clone()],
            Statement::Edge(e) => vec![e.from.id.clone(), e.to.id.clone()],
            Statement::Subgraph(sg) => collect_node_ids(&sg.statements),
        };
        for id in new_ids {
            if seen.insert(id.clone()) {
                ids.push(id);
            }
        }
    }
    ids
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
        ShapeSpec::Stadium(_) => Shape::Stadium,
        ShapeSpec::Subroutine(_) => Shape::Subroutine,
        ShapeSpec::Cylinder(_) => Shape::Cylinder,
        ShapeSpec::Circle(_) => Shape::Circle,
        ShapeSpec::DoubleCircle(_) => Shape::DoubleCircle,
        ShapeSpec::Hexagon(_) => Shape::Hexagon,
        ShapeSpec::Asymmetric(_) => Shape::Asymmetric,
        ShapeSpec::Trapezoid(_) => Shape::Trapezoid,
        ShapeSpec::InvTrapezoid(_) => Shape::InvTrapezoid,
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

    #[test]
    fn test_nested_subgraph_outer_contains_inner_nodes() {
        let input = "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA --> B\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        assert!(diagram.subgraphs["outer"].nodes.contains(&"A".to_string()));
        assert!(diagram.subgraphs["outer"].nodes.contains(&"B".to_string()));
        assert!(diagram.subgraphs["inner"].nodes.contains(&"A".to_string()));
        assert!(diagram.subgraphs["inner"].nodes.contains(&"B".to_string()));
    }

    #[test]
    fn test_nested_subgraph_parent_set() {
        let input = "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA --> B\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        assert_eq!(diagram.subgraphs["inner"].parent, Some("outer".to_string()));
        assert_eq!(diagram.subgraphs["outer"].parent, None);
    }

    #[test]
    fn test_build_diagram_with_subgraph() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert!(diagram.has_subgraphs());
        assert!(diagram.subgraphs.contains_key("sg1"));
        let sg = &diagram.subgraphs["sg1"];
        assert_eq!(sg.title, "Group");
        assert!(sg.nodes.contains(&"A".to_string()));
        assert!(sg.nodes.contains(&"B".to_string()));
    }

    #[test]
    fn test_build_diagram_node_parent_set() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\nC --> A\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes["A"].parent, Some("sg1".to_string()));
        assert_eq!(diagram.nodes["B"].parent, Some("sg1".to_string()));
        assert_eq!(diagram.nodes["C"].parent, None);
    }

    #[test]
    fn test_build_diagram_subgraph_edges_cross_boundary() {
        let input = "graph TD\nsubgraph sg1[Group]\nA\nB\nend\nA --> C\nC --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.edges.len(), 2);
        assert_eq!(diagram.nodes["A"].parent, Some("sg1".to_string()));
        assert_eq!(diagram.nodes["C"].parent, None);
    }
}
