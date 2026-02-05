//! Converts AST to graph data structures.

use std::collections::HashSet;

use super::diagram::{Diagram, Direction, Subgraph};
use super::edge::{Arrow, Edge, Stroke};
use super::node::{Node, Shape};
use crate::parser::{
    ArrowHead, ConnectorSpec, Direction as ParseDirection, EdgeSpec, Flowchart, ShapeSpec,
    Statement, StrokeSpec, Vertex,
};

/// Build a Diagram from a parsed Flowchart.
pub fn build_diagram(flowchart: &Flowchart) -> Diagram {
    let direction = convert_direction(flowchart.direction);
    let mut diagram = Diagram::new(direction);
    process_statements(&mut diagram, &flowchart.statements, None);
    resolve_subgraph_edges(&mut diagram);
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
                diagram.subgraph_order.push(sg_spec.id.clone());
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
        // Update existing node if this vertex has more specific shape info
        if let Some(shape_spec) = &vertex.shape
            && existing.label == existing.id
        {
            let shape = convert_shape(shape_spec);
            existing.label = normalize_shape_label(&vertex.id, shape_spec, shape);
            existing.shape = shape;
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

/// Replace edge endpoints that reference subgraph IDs with representative child nodes.
fn resolve_subgraph_edges(diagram: &mut Diagram) {
    let mut resolved_edges = Vec::new();

    for edge in &diagram.edges {
        let from = if diagram.is_subgraph(&edge.from) {
            match find_non_cluster_child(diagram, &edge.from) {
                Some(child) => child,
                None => continue,
            }
        } else {
            edge.from.clone()
        };

        let to = if diagram.is_subgraph(&edge.to) {
            match find_non_cluster_child(diagram, &edge.to) {
                Some(child) => child,
                None => continue,
            }
        } else {
            edge.to.clone()
        };

        resolved_edges.push(Edge {
            from,
            to,
            stroke: edge.stroke,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            label: edge.label.clone(),
        });
    }

    diagram.edges = resolved_edges;

    // Remove spurious regular nodes created for subgraph IDs during edge parsing
    let subgraph_ids: Vec<String> = diagram.subgraphs.keys().cloned().collect();
    for sg_id in &subgraph_ids {
        if let Some(node) = diagram.nodes.get(sg_id)
            && node.parent.is_none()
            && node.label == *sg_id
        {
            diagram.nodes.remove(sg_id);
        }
    }
}

/// Find a non-compound child node within a subgraph.
///
/// Walks the subgraph's children, returning the first leaf node that is not
/// itself a subgraph. Returns `None` for empty subgraphs or nonexistent IDs.
///
/// This is the Rust equivalent of Mermaid's `findNonClusterChild()`.
pub fn find_non_cluster_child(diagram: &Diagram, subgraph_id: &str) -> Option<String> {
    let sg = diagram.subgraphs.get(subgraph_id)?;
    sg.nodes.iter().find(|id| !diagram.is_subgraph(id)).cloned()
}

fn collect_node_ids(statements: &[Statement]) -> Vec<String> {
    let mut seen = HashSet::new();
    statements
        .iter()
        .flat_map(|stmt| match stmt {
            Statement::Vertex(v) => vec![v.id.clone()],
            Statement::Edge(e) => vec![e.from.id.clone(), e.to.id.clone()],
            Statement::Subgraph(sg) => collect_node_ids(&sg.statements),
        })
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn convert_vertex(vertex: &Vertex) -> Node {
    match &vertex.shape {
        Some(shape_spec) => {
            let shape = convert_shape(shape_spec);
            let label = normalize_shape_label(&vertex.id, shape_spec, shape);
            Node::new(&vertex.id).with_label(label).with_shape(shape)
        }
        None => Node::new(&vertex.id),
    }
}

fn normalize_shape_label(id: &str, shape_spec: &ShapeSpec, shape: Shape) -> String {
    let text = shape_spec.text();
    if text.is_empty()
        && !matches!(
            shape,
            Shape::SmallCircle | Shape::FramedCircle | Shape::CrossedCircle | Shape::ForkJoin
        )
    {
        id.to_string()
    } else {
        text.to_string()
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
        ShapeSpec::Document(_) => Shape::Document,
        ShapeSpec::Documents(_) => Shape::Documents,
        ShapeSpec::TaggedDocument(_) => Shape::TaggedDocument,
        ShapeSpec::Card(_) => Shape::Card,
        ShapeSpec::TaggedRect(_) => Shape::TaggedRect,
        ShapeSpec::Circle(_) => Shape::Circle,
        ShapeSpec::DoubleCircle(_) => Shape::DoubleCircle,
        ShapeSpec::Hexagon(_) => Shape::Hexagon,
        ShapeSpec::Parallelogram(_) => Shape::Parallelogram,
        ShapeSpec::InvParallelogram(_) => Shape::InvParallelogram,
        ShapeSpec::ManualInput(_) => Shape::ManualInput,
        ShapeSpec::Asymmetric(_) => Shape::Asymmetric,
        ShapeSpec::Trapezoid(_) => Shape::Trapezoid,
        ShapeSpec::InvTrapezoid(_) => Shape::InvTrapezoid,
        ShapeSpec::SmallCircle(_) => Shape::SmallCircle,
        ShapeSpec::FramedCircle(_) => Shape::FramedCircle,
        ShapeSpec::CrossedCircle(_) => Shape::CrossedCircle,
        ShapeSpec::TextBlock(_) => Shape::TextBlock,
        ShapeSpec::ForkJoin(_) => Shape::ForkJoin,
    }
}

fn convert_edge(edge_spec: &EdgeSpec) -> Edge {
    let (stroke, mut arrow_start, mut arrow_end, label) = convert_connector(&edge_spec.connector);

    let (from, to) = if arrow_start != Arrow::None && arrow_end == Arrow::None {
        // If only the left arrow is present, treat it as a reversed edge.
        std::mem::swap(&mut arrow_start, &mut arrow_end);
        (edge_spec.to.id.clone(), edge_spec.from.id.clone())
    } else {
        (edge_spec.from.id.clone(), edge_spec.to.id.clone())
    };

    let edge = Edge::new(from, to)
        .with_stroke(stroke)
        .with_arrows(arrow_start, arrow_end);

    if let Some(lbl) = label {
        edge.with_label(lbl)
    } else {
        edge
    }
}

fn convert_connector(connector: &ConnectorSpec) -> (Stroke, Arrow, Arrow, Option<String>) {
    let stroke = match connector.stroke {
        StrokeSpec::Solid => Stroke::Solid,
        StrokeSpec::Dotted => Stroke::Dotted,
        StrokeSpec::Thick => Stroke::Thick,
    };

    // Map arrow heads to the graph-layer Arrow type.
    // Cross and Circle are not yet rendered differently, so map to Normal.
    let arrow_start = map_arrow_head(connector.left);
    let arrow_end = map_arrow_head(connector.right);

    (stroke, arrow_start, arrow_end, connector.label.clone())
}

fn map_arrow_head(head: ArrowHead) -> Arrow {
    match head {
        ArrowHead::None => Arrow::None,
        ArrowHead::Normal | ArrowHead::Cross | ArrowHead::Circle => Arrow::Normal,
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
        assert_eq!(diagram.edges[0].arrow_end, Arrow::Normal);

        assert_eq!(diagram.edges[1].stroke, Stroke::Dotted);
        assert_eq!(diagram.edges[1].arrow_end, Arrow::Normal);

        assert_eq!(diagram.edges[2].stroke, Stroke::Thick);
        assert_eq!(diagram.edges[2].arrow_end, Arrow::Normal);

        assert_eq!(diagram.edges[3].stroke, Stroke::Solid);
        assert_eq!(diagram.edges[3].arrow_end, Arrow::None);
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

    #[test]
    fn test_find_non_cluster_child_simple() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let child = find_non_cluster_child(&diagram, "sg1");
        assert!(child.is_some());
        let child_id = child.unwrap();
        assert!(child_id == "A" || child_id == "B");
    }

    #[test]
    fn test_find_non_cluster_child_nested() {
        let input = "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA --> B\nend\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let child = find_non_cluster_child(&diagram, "outer");
        assert!(child.is_some());
        let child_id = child.unwrap();
        assert!(child_id == "A" || child_id == "B");
    }

    #[test]
    fn test_find_non_cluster_child_empty_subgraph() {
        let input = "graph TD\nsubgraph sg1[Empty]\nend\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let child = find_non_cluster_child(&diagram, "sg1");
        assert!(child.is_none());
    }

    #[test]
    fn test_find_non_cluster_child_nonexistent() {
        let input = "graph TD\nA --> B\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let child = find_non_cluster_child(&diagram, "no_such_sg");
        assert!(child.is_none());
    }

    #[test]
    fn test_edge_to_subgraph_resolved() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\nC --> sg1\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let c_edges: Vec<_> = diagram.edges.iter().filter(|e| e.from == "C").collect();
        assert_eq!(c_edges.len(), 1);
        assert!(
            c_edges[0].to == "A" || c_edges[0].to == "B",
            "Edge to subgraph should resolve to child, got: {}",
            c_edges[0].to
        );
    }

    #[test]
    fn test_edge_from_subgraph_resolved() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\nsg1 --> C\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let c_edges: Vec<_> = diagram.edges.iter().filter(|e| e.to == "C").collect();
        assert_eq!(c_edges.len(), 1);
        assert!(
            c_edges[0].from == "A" || c_edges[0].from == "B",
            "Edge from subgraph should resolve to child, got: {}",
            c_edges[0].from
        );
    }

    #[test]
    fn test_edge_between_subgraphs_resolved() {
        let input = "graph TD\nsubgraph sg1[G1]\nA\nend\nsubgraph sg2[G2]\nB\nend\nsg1 --> sg2\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let edges: Vec<_> = diagram.edges.iter().collect();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "A");
        assert_eq!(edges[0].to, "B");
    }

    #[test]
    fn test_edge_to_subgraph_no_duplicate_node() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\nC --> sg1\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert!(
            !diagram.nodes.contains_key("sg1") || diagram.subgraphs.contains_key("sg1"),
            "sg1 should be a subgraph, not a regular node"
        );
    }

    #[test]
    fn test_edge_to_empty_subgraph_dropped() {
        let input = "graph TD\nsubgraph sg1[Empty]\nend\nC --> sg1\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let c_edges: Vec<_> = diagram.edges.iter().filter(|e| e.from == "C").collect();
        assert_eq!(c_edges.len(), 0, "Edge to empty subgraph should be dropped");
    }

    #[test]
    fn test_build_diagram_shape_config_label_defaults() {
        let input = "graph TD\nA@{shape: doc}\nJ@{shape: sm-circ}\n";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        let node_a = diagram.get_node("A").unwrap();
        assert_eq!(node_a.shape, Shape::Document);
        assert_eq!(node_a.label, "A");

        let node_j = diagram.get_node("J").unwrap();
        assert_eq!(node_j.shape, Shape::SmallCircle);
        assert_eq!(node_j.label, "");
    }
}
