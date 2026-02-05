//! JSON output format for LLM integration.
//!
//! Provides structured graph topology output as JSON, designed for
//! machine consumption in LLM pipelines and agentic workflows.

use serde::{Deserialize, Serialize};

use crate::graph::{Arrow, Diagram, Direction, Shape, Stroke};
use crate::render::Layout;

/// Convert a Diagram (and optional Layout) to JSON string.
///
/// If `layout` is provided, node positions and canvas dimensions are included.
/// If `layout` is None, only topology is included.
pub fn to_json(diagram: &Diagram, layout: Option<&Layout>) -> String {
    let output = build_json_output(diagram, layout);
    serde_json::to_string_pretty(&output).expect("JSON serialization should not fail")
}

fn build_json_output(diagram: &Diagram, layout: Option<&Layout>) -> JsonOutput {
    let metadata = GraphMetadata {
        direction: direction_str(diagram.direction),
        width: layout.map(|l| l.width),
        height: layout.map(|l| l.height),
    };

    let mut nodes: Vec<JsonNode> = diagram
        .nodes
        .values()
        .map(|node| {
            let position = layout.and_then(|l| {
                l.node_bounds
                    .get(&node.id)
                    .map(|bounds| JsonPosition {
                        x: bounds.center_x(),
                        y: bounds.center_y(),
                    })
            });
            JsonNode {
                id: node.id.clone(),
                label: node.label.clone(),
                shape: shape_str(node.shape),
                parent: node.parent.clone(),
                position,
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let edges: Vec<JsonEdge> = diagram
        .edges
        .iter()
        .map(|edge| JsonEdge {
            source: edge.from.clone(),
            target: edge.to.clone(),
            label: edge.label.clone(),
            stroke: stroke_str(edge.stroke),
            arrow_start: arrow_str(edge.arrow_start),
            arrow_end: arrow_str(edge.arrow_end),
        })
        .collect();

    let mut subgraphs: Vec<JsonSubgraph> = diagram
        .subgraphs
        .values()
        .map(|sg| {
            let direct_children: Vec<String> = sg
                .nodes
                .iter()
                .filter(|node_id| {
                    diagram
                        .nodes
                        .get(*node_id)
                        .and_then(|n| n.parent.as_deref())
                        == Some(&sg.id)
                })
                .cloned()
                .collect();
            JsonSubgraph {
                id: sg.id.clone(),
                title: sg.title.clone(),
                children: direct_children,
                parent: sg.parent.clone(),
            }
        })
        .collect();
    subgraphs.sort_by(|a, b| a.id.cmp(&b.id));

    JsonOutput {
        version: 1,
        metadata,
        nodes,
        edges,
        subgraphs,
    }
}

fn direction_str(dir: Direction) -> String {
    match dir {
        Direction::TopDown => "TD",
        Direction::BottomTop => "BT",
        Direction::LeftRight => "LR",
        Direction::RightLeft => "RL",
    }
    .to_string()
}

fn shape_str(shape: Shape) -> String {
    match shape {
        Shape::Rectangle => "rectangle",
        Shape::Round => "round",
        Shape::Stadium => "stadium",
        Shape::Subroutine => "subroutine",
        Shape::Cylinder => "cylinder",
        Shape::Document => "document",
        Shape::Documents => "documents",
        Shape::TaggedDocument => "tagged_document",
        Shape::Card => "card",
        Shape::TaggedRect => "tagged_rect",
        Shape::Diamond => "diamond",
        Shape::Hexagon => "hexagon",
        Shape::Trapezoid => "trapezoid",
        Shape::InvTrapezoid => "inv_trapezoid",
        Shape::Parallelogram => "parallelogram",
        Shape::InvParallelogram => "inv_parallelogram",
        Shape::ManualInput => "manual_input",
        Shape::Asymmetric => "asymmetric",
        Shape::Circle => "circle",
        Shape::DoubleCircle => "double_circle",
        Shape::SmallCircle => "small_circle",
        Shape::FramedCircle => "framed_circle",
        Shape::CrossedCircle => "crossed_circle",
        Shape::TextBlock => "text_block",
        Shape::ForkJoin => "fork_join",
    }
    .to_string()
}

fn stroke_str(stroke: Stroke) -> String {
    match stroke {
        Stroke::Solid => "solid",
        Stroke::Dotted => "dotted",
        Stroke::Thick => "thick",
    }
    .to_string()
}

fn arrow_str(arrow: Arrow) -> String {
    match arrow {
        Arrow::Normal => "normal",
        Arrow::None => "none",
    }
    .to_string()
}

/// Top-level JSON output structure.
///
/// This is a versioned API contract. The `version` field allows
/// consumers to handle schema evolution.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonOutput {
    /// Schema version (currently 1).
    pub version: u32,
    /// Graph metadata.
    pub metadata: GraphMetadata,
    /// Node inventory.
    pub nodes: Vec<JsonNode>,
    /// Edge inventory.
    pub edges: Vec<JsonEdge>,
    /// Subgraph inventory.
    pub subgraphs: Vec<JsonSubgraph>,
}

/// Graph-level metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphMetadata {
    /// Layout direction: "TD", "BT", "LR", or "RL".
    pub direction: String,
    /// Canvas width in characters (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub width: Option<usize>,
    /// Canvas height in characters (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub height: Option<usize>,
}

/// A node in the JSON output.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonNode {
    /// Node identifier (from Mermaid source).
    pub id: String,
    /// Display label.
    pub label: String,
    /// Shape name (snake_case).
    pub shape: String,
    /// Parent subgraph ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub parent: Option<String>,
    /// Layout position (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub position: Option<JsonPosition>,
}

/// Position coordinates.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonPosition {
    /// X coordinate (center of node).
    pub x: usize,
    /// Y coordinate (center of node).
    pub y: usize,
}

/// An edge in the JSON output.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Edge label, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub label: Option<String>,
    /// Stroke style: "solid", "dotted", or "thick".
    pub stroke: String,
    /// Arrow at source end: "none" or "normal".
    pub arrow_start: String,
    /// Arrow at target end: "none" or "normal".
    pub arrow_end: String,
}

/// A subgraph in the JSON output.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonSubgraph {
    /// Subgraph identifier.
    pub id: String,
    /// Display title.
    pub title: String,
    /// IDs of nodes directly in this subgraph.
    pub children: Vec<String>,
    /// Parent subgraph ID, if nested.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub parent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_diagram;
    use crate::parser::parse_flowchart;

    #[test]
    fn test_json_output_has_version() {
        let output = JsonOutput {
            version: 1,
            metadata: GraphMetadata {
                direction: "TD".to_string(),
                width: None,
                height: None,
            },
            nodes: vec![],
            edges: vec![],
            subgraphs: vec![],
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"version\":1"));
    }

    #[test]
    fn test_json_node_serialization() {
        let node = JsonNode {
            id: "A".to_string(),
            label: "Start".to_string(),
            shape: "rectangle".to_string(),
            parent: None,
            position: Some(JsonPosition { x: 20, y: 2 }),
        };
        let json = serde_json::to_string_pretty(&node).unwrap();
        assert!(json.contains("\"id\": \"A\""));
        assert!(json.contains("\"shape\": \"rectangle\""));
        assert!(json.contains("\"position\""));
    }

    #[test]
    fn test_json_edge_serialization() {
        let edge = JsonEdge {
            source: "A".to_string(),
            target: "B".to_string(),
            label: Some("yes".to_string()),
            stroke: "solid".to_string(),
            arrow_start: "none".to_string(),
            arrow_end: "normal".to_string(),
        };
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"source\":\"A\""));
        assert!(json.contains("\"label\":\"yes\""));
    }

    #[test]
    fn test_json_subgraph_serialization() {
        let sg = JsonSubgraph {
            id: "sg1".to_string(),
            title: "My Group".to_string(),
            children: vec!["A".to_string(), "B".to_string()],
            parent: None,
        };
        let json = serde_json::to_string(&sg).unwrap();
        assert!(json.contains("\"children\":[\"A\",\"B\"]"));
    }

    #[test]
    fn test_json_position_omitted_when_none() {
        let node = JsonNode {
            id: "A".to_string(),
            label: "Start".to_string(),
            shape: "rectangle".to_string(),
            parent: None,
            position: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(!json.contains("position"));
    }

    #[test]
    fn test_to_json_simple_diagram() {
        let input = "graph TD\nA[Start] --> B[End]\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        assert_eq!(output.version, 1);
        assert_eq!(output.metadata.direction, "TD");
        assert_eq!(output.nodes.len(), 2);
        assert_eq!(output.edges.len(), 1);

        let node_a = output.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(node_a.label, "Start");
        assert_eq!(node_a.shape, "rectangle");
        assert!(node_a.position.is_none());

        assert_eq!(output.edges[0].source, "A");
        assert_eq!(output.edges[0].target, "B");
        assert_eq!(output.edges[0].stroke, "solid");
        assert_eq!(output.edges[0].arrow_end, "normal");
    }

    #[test]
    fn test_to_json_with_subgraphs() {
        let input = "graph TD\nsubgraph sg1[Group]\nA --> B\nend\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        assert_eq!(output.subgraphs.len(), 1);
        assert_eq!(output.subgraphs[0].id, "sg1");
        assert_eq!(output.subgraphs[0].title, "Group");
        assert!(output.subgraphs[0].children.contains(&"A".to_string()));
        assert!(output.subgraphs[0].children.contains(&"B".to_string()));
    }

    #[test]
    fn test_to_json_edge_styles() {
        let input = "graph TD\nA -.-> B\nB ==> C\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        let dotted = output.edges.iter().find(|e| e.source == "A").unwrap();
        assert_eq!(dotted.stroke, "dotted");

        let thick = output.edges.iter().find(|e| e.source == "B").unwrap();
        assert_eq!(thick.stroke, "thick");
    }

    #[test]
    fn test_to_json_direction_variants() {
        for (dir_str, expected) in [("TD", "TD"), ("LR", "LR"), ("BT", "BT"), ("RL", "RL")] {
            let input = format!("graph {}\nA --> B\n", dir_str);
            let fc = parse_flowchart(&input).unwrap();
            let diagram = build_diagram(&fc);
            let json_str = to_json(&diagram, None);
            let output: JsonOutput = serde_json::from_str(&json_str).unwrap();
            assert_eq!(output.metadata.direction, expected);
        }
    }

    #[test]
    fn test_to_json_node_shapes() {
        let input = "graph TD\nA[Rect]\nB(Round)\nC{Diamond}\nD([Stadium])\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        let shapes: std::collections::HashMap<String, String> = output
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.shape.clone()))
            .collect();
        assert_eq!(shapes["A"], "rectangle");
        assert_eq!(shapes["B"], "round");
        assert_eq!(shapes["C"], "diamond");
        assert_eq!(shapes["D"], "stadium");
    }

    #[test]
    fn test_to_json_nested_subgraph_children_are_direct_only() {
        let input =
            "graph TD\nsubgraph outer[Outer]\nsubgraph inner[Inner]\nA --> B\nend\nC\nend\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        let outer = output.subgraphs.iter().find(|s| s.id == "outer").unwrap();
        let inner = output.subgraphs.iter().find(|s| s.id == "inner").unwrap();

        // outer's direct children should be C only (A and B belong to inner)
        assert!(
            outer.children.contains(&"C".to_string()),
            "outer should contain direct child C"
        );
        assert!(
            !outer.children.contains(&"A".to_string()),
            "outer should NOT contain A (belongs to inner)"
        );
        assert!(
            !outer.children.contains(&"B".to_string()),
            "outer should NOT contain B (belongs to inner)"
        );

        // inner's direct children should be A and B
        assert!(inner.children.contains(&"A".to_string()));
        assert!(inner.children.contains(&"B".to_string()));
    }

    #[test]
    fn test_to_json_nodes_sorted_by_id() {
        let input = "graph TD\nC --> B\nB --> A\n";
        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let json_str = to_json(&diagram, None);
        let output: JsonOutput = serde_json::from_str(&json_str).unwrap();

        let ids: Vec<&str> = output.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }
}
