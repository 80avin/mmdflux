//! MMDS (Machine-Mediated Diagram Specification) JSON output.
//!
//! Produces structured JSON from graph-family geometry with two levels:
//! - `layout`: Node geometry + edge topology/semantics (no edge paths).
//! - `routed`: Everything from layout + routed edge paths and bounds.

use serde::{Deserialize, Serialize};

use crate::diagram::GeometryLevel;
use crate::diagrams::flowchart::geometry::{GraphGeometry, PositionedNode, RoutedGraphGeometry};
use crate::graph::{Arrow, Diagram, Direction, Shape, Stroke};

/// Serialize a graph-family diagram to MMDS JSON at layout level.
///
/// Uses `GraphGeometry` for node positions and `Diagram` for edge semantics.
/// Edge paths are excluded at layout level.
pub fn to_mmds_layout(diagram: &Diagram, geometry: &GraphGeometry) -> String {
    to_mmds_layout_typed("flowchart", diagram, geometry)
}

/// Serialize a graph-family diagram to MMDS JSON at layout level with explicit type.
pub fn to_mmds_layout_typed(
    diagram_type: &str,
    diagram: &Diagram,
    geometry: &GraphGeometry,
) -> String {
    let output = build_mmds_output(diagram_type, diagram, geometry, None);
    serde_json::to_string_pretty(&output).expect("MMDS serialization should not fail")
}

/// Serialize a graph-family diagram to MMDS JSON at routed level.
///
/// Includes everything from layout level plus routed edge paths and
/// subgraph bounds.
pub fn to_mmds_routed(
    diagram: &Diagram,
    geometry: &GraphGeometry,
    routed: &RoutedGraphGeometry,
) -> String {
    to_mmds_routed_typed("flowchart", diagram, geometry, routed)
}

/// Serialize a graph-family diagram to MMDS JSON at routed level with explicit type.
pub fn to_mmds_routed_typed(
    diagram_type: &str,
    diagram: &Diagram,
    geometry: &GraphGeometry,
    routed: &RoutedGraphGeometry,
) -> String {
    let output = build_mmds_output(diagram_type, diagram, geometry, Some(routed));
    serde_json::to_string_pretty(&output).expect("MMDS serialization should not fail")
}

/// Serialize a diagram to MMDS JSON at the specified geometry level.
pub fn to_mmds_json(
    diagram: &Diagram,
    geometry: &GraphGeometry,
    routed: Option<&RoutedGraphGeometry>,
    level: GeometryLevel,
) -> String {
    to_mmds_json_typed("flowchart", diagram, geometry, routed, level)
}

/// Serialize a diagram to MMDS JSON at the specified geometry level with explicit type.
pub fn to_mmds_json_typed(
    diagram_type: &str,
    diagram: &Diagram,
    geometry: &GraphGeometry,
    routed: Option<&RoutedGraphGeometry>,
    level: GeometryLevel,
) -> String {
    match level {
        GeometryLevel::Layout => to_mmds_layout_typed(diagram_type, diagram, geometry),
        GeometryLevel::Routed => {
            if let Some(routed) = routed {
                to_mmds_routed_typed(diagram_type, diagram, geometry, routed)
            } else {
                to_mmds_layout_typed(diagram_type, diagram, geometry)
            }
        }
    }
}

fn build_mmds_output(
    diagram_type: &str,
    diagram: &Diagram,
    geometry: &GraphGeometry,
    routed: Option<&RoutedGraphGeometry>,
) -> MmdsOutput {
    let level = if routed.is_some() { "routed" } else { "layout" };

    let metadata = MmdsMetadata {
        diagram_type: diagram_type.to_string(),
        direction: direction_str(diagram.direction).to_string(),
        bounds: if routed.is_some() {
            Some(MmdsBounds {
                width: geometry.bounds.width,
                height: geometry.bounds.height,
            })
        } else {
            None
        },
    };

    // Build nodes from geometry (float positions)
    let mut nodes: Vec<MmdsNode> = geometry.nodes.values().map(mmds_node).collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    // Build edges
    let edges: Vec<MmdsEdge> = diagram
        .edges
        .iter()
        .enumerate()
        .map(|(i, edge)| {
            let mut mmds_edge = MmdsEdge {
                source: edge.from.clone(),
                target: edge.to.clone(),
                label: edge.label.clone(),
                stroke: stroke_str(edge.stroke).to_string(),
                arrow_start: arrow_str(edge.arrow_start).to_string(),
                arrow_end: arrow_str(edge.arrow_end).to_string(),
                path: None,
                label_position: None,
                is_backward: None,
            };

            // Add routed fields only at routed level
            if let Some(routed) = routed
                && let Some(re) = routed.edges.iter().find(|e| e.index == i)
            {
                mmds_edge.path = Some(re.path.iter().map(|p| [p.x, p.y]).collect());
                mmds_edge.label_position =
                    re.label_position.map(|p| MmdsPosition { x: p.x, y: p.y });
                mmds_edge.is_backward = Some(re.is_backward);
            }

            mmds_edge
        })
        .collect();

    // Build subgraphs
    let mut subgraphs: Vec<MmdsSubgraph> = diagram
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

            let bounds = routed.and_then(|r| {
                r.subgraphs.get(&sg.id).map(|sg_geom| MmdsBounds {
                    width: sg_geom.rect.width,
                    height: sg_geom.rect.height,
                })
            });

            MmdsSubgraph {
                id: sg.id.clone(),
                title: sg.title.clone(),
                children: direct_children,
                parent: sg.parent.clone(),
                bounds,
            }
        })
        .collect();
    subgraphs.sort_by(|a, b| a.id.cmp(&b.id));

    MmdsOutput {
        version: 2,
        geometry_level: level.to_string(),
        metadata,
        nodes,
        edges,
        subgraphs,
    }
}

fn mmds_node(pn: &PositionedNode) -> MmdsNode {
    MmdsNode {
        id: pn.id.clone(),
        label: pn.label.clone(),
        shape: shape_str(pn.shape).to_string(),
        parent: pn.parent.clone(),
        position: MmdsPosition {
            x: pn.rect.x,
            y: pn.rect.y,
        },
        size: MmdsSize {
            width: pn.rect.width,
            height: pn.rect.height,
        },
    }
}

fn direction_str(dir: Direction) -> &'static str {
    match dir {
        Direction::TopDown => "TD",
        Direction::BottomTop => "BT",
        Direction::LeftRight => "LR",
        Direction::RightLeft => "RL",
    }
}

fn shape_str(shape: Shape) -> &'static str {
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
}

fn stroke_str(stroke: Stroke) -> &'static str {
    match stroke {
        Stroke::Solid => "solid",
        Stroke::Dotted => "dotted",
        Stroke::Thick => "thick",
        Stroke::Invisible => "invisible",
    }
}

fn arrow_str(arrow: Arrow) -> &'static str {
    match arrow {
        Arrow::Normal => "normal",
        Arrow::None => "none",
        Arrow::Cross => "cross",
        Arrow::Circle => "circle",
    }
}

// ---------------------------------------------------------------------------
// MMDS data types
// ---------------------------------------------------------------------------

/// Top-level MMDS output envelope.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsOutput {
    /// Schema version (2 for MMDS).
    pub version: u32,
    /// Geometry level: "layout" or "routed".
    pub geometry_level: String,
    /// Diagram metadata.
    pub metadata: MmdsMetadata,
    /// Node inventory with positions.
    pub nodes: Vec<MmdsNode>,
    /// Edge inventory (topology at layout, paths at routed).
    pub edges: Vec<MmdsEdge>,
    /// Subgraph inventory.
    pub subgraphs: Vec<MmdsSubgraph>,
}

/// Diagram-level metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsMetadata {
    /// Diagram type (e.g., "flowchart", "class").
    pub diagram_type: String,
    /// Layout direction: "TD", "BT", "LR", or "RL".
    pub direction: String,
    /// Overall layout bounds (routed level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub bounds: Option<MmdsBounds>,
}

/// Bounding box dimensions.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsBounds {
    pub width: f64,
    pub height: f64,
}

/// A node in MMDS output.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsNode {
    /// Node identifier.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Shape name (snake_case).
    pub shape: String,
    /// Parent subgraph ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub parent: Option<String>,
    /// Center position in layout float space.
    pub position: MmdsPosition,
    /// Node dimensions.
    pub size: MmdsSize,
}

/// Float-precision position.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsPosition {
    pub x: f64,
    pub y: f64,
}

/// Float-precision dimensions.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsSize {
    pub width: f64,
    pub height: f64,
}

/// An edge in MMDS output.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Edge label, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub label: Option<String>,
    /// Stroke style.
    pub stroke: String,
    /// Arrow at source end.
    pub arrow_start: String,
    /// Arrow at target end.
    pub arrow_end: String,
    /// Routed edge path (routed level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub path: Option<Vec<[f64; 2]>>,
    /// Label center position (routed level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub label_position: Option<MmdsPosition>,
    /// Whether edge flows backward (routed level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub is_backward: Option<bool>,
}

/// A subgraph in MMDS output.
#[derive(Debug, Serialize, Deserialize)]
pub struct MmdsSubgraph {
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
    /// Subgraph bounding box (routed level only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub bounds: Option<MmdsBounds>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_diagram;
    use crate::parser::parse_flowchart;

    fn layout_geometry(input: &str) -> (Diagram, GraphGeometry) {
        use crate::diagram::{EngineConfig, GraphLayoutEngine};
        use crate::diagrams::flowchart::engine::DagreLayoutEngine;

        let fc = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&fc);
        let engine = DagreLayoutEngine;
        let config = EngineConfig::Dagre(crate::dagre::types::LayoutConfig::default());
        let geom = engine.layout(&diagram, &config).unwrap();
        (diagram, geom)
    }

    fn routed_geometry(diagram: &Diagram, geometry: &GraphGeometry) -> RoutedGraphGeometry {
        use crate::diagram::RoutingMode;
        use crate::diagrams::flowchart::routing::route_graph_geometry;
        route_graph_geometry(diagram, geometry, RoutingMode::FullCompute)
    }

    #[test]
    fn layout_json_has_version_and_level() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.version, 2);
        assert_eq!(output.geometry_level, "layout");
    }

    #[test]
    fn layout_json_has_metadata() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.metadata.diagram_type, "flowchart");
        assert_eq!(output.metadata.direction, "TD");
        assert!(output.metadata.bounds.is_none());
    }

    #[test]
    fn layout_json_has_nodes_with_positions() {
        let (diagram, geom) = layout_geometry("graph TD\nA[Start]-->B[End]");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(output.nodes.len(), 2);
        let node_a = output.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(node_a.label, "Start");
        assert_eq!(node_a.shape, "rectangle");
        assert!(node_a.size.width > 0.0);
        assert!(node_a.size.height > 0.0);
    }

    #[test]
    fn layout_json_edges_have_no_paths() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let json = to_mmds_layout(&diagram, &geom);

        // Layout-level: no edge geometry fields
        assert!(!json.contains("\"path\""));
        assert!(!json.contains("\"label_position\""));
        assert!(!json.contains("\"is_backward\""));

        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.edges.len(), 1);
        assert_eq!(output.edges[0].source, "A");
        assert_eq!(output.edges[0].target, "B");
        assert!(output.edges[0].path.is_none());
    }

    #[test]
    fn layout_json_edge_semantics() {
        let (diagram, geom) = layout_geometry("graph TD\nA-.label.->B");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        let edge = &output.edges[0];
        assert_eq!(edge.stroke, "dotted");
        assert_eq!(edge.label, Some("label".to_string()));
        assert_eq!(edge.arrow_end, "normal");
    }

    #[test]
    fn layout_json_subgraphs() {
        let (diagram, geom) = layout_geometry("graph TD\nsubgraph sg1[Group]\nA-->B\nend");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(output.subgraphs.len(), 1);
        assert_eq!(output.subgraphs[0].id, "sg1");
        assert_eq!(output.subgraphs[0].title, "Group");
        assert!(output.subgraphs[0].bounds.is_none());
    }

    #[test]
    fn layout_json_nodes_sorted_by_id() {
        let (diagram, geom) = layout_geometry("graph TD\nC-->B\nB-->A");
        let json = to_mmds_layout(&diagram, &geom);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        let ids: Vec<&str> = output.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }

    #[test]
    fn layout_json_direction_variants() {
        for (dir_str, expected) in [("TD", "TD"), ("LR", "LR"), ("BT", "BT"), ("RL", "RL")] {
            let input = format!("graph {dir_str}\nA-->B");
            let (diagram, geom) = layout_geometry(&input);
            let json = to_mmds_layout(&diagram, &geom);
            let output: MmdsOutput = serde_json::from_str(&json).unwrap();
            assert_eq!(output.metadata.direction, expected);
        }
    }

    #[test]
    fn routed_json_has_version_and_level() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let routed = routed_geometry(&diagram, &geom);
        let json = to_mmds_routed(&diagram, &geom, &routed);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.version, 2);
        assert_eq!(output.geometry_level, "routed");
    }

    #[test]
    fn routed_json_includes_edge_paths() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let routed = routed_geometry(&diagram, &geom);
        let json = to_mmds_routed(&diagram, &geom, &routed);

        assert!(json.contains("\"path\""));

        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        let edge = &output.edges[0];
        assert!(edge.path.is_some());
        assert!(edge.path.as_ref().unwrap().len() >= 2);
        assert!(edge.is_backward.is_some());
    }

    #[test]
    fn routed_json_includes_metadata_bounds() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let routed = routed_geometry(&diagram, &geom);
        let json = to_mmds_routed(&diagram, &geom, &routed);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        let bounds = output.metadata.bounds.as_ref().unwrap();
        assert!(bounds.width > 0.0);
        assert!(bounds.height > 0.0);
    }

    #[test]
    fn routed_json_subgraph_bounds() {
        let (diagram, geom) = layout_geometry("graph TD\nsubgraph sg1[Group]\nA-->B\nend");
        let routed = routed_geometry(&diagram, &geom);
        let json = to_mmds_routed(&diagram, &geom, &routed);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();

        let sg = &output.subgraphs[0];
        assert!(sg.bounds.is_some());
        let bounds = sg.bounds.as_ref().unwrap();
        assert!(bounds.width > 0.0);
    }

    #[test]
    fn to_mmds_json_dispatches_by_level() {
        let (diagram, geom) = layout_geometry("graph TD\nA-->B");
        let routed = routed_geometry(&diagram, &geom);

        let layout_json = to_mmds_json(&diagram, &geom, Some(&routed), GeometryLevel::Layout);
        assert!(!layout_json.contains("\"path\""));

        let routed_json = to_mmds_json(&diagram, &geom, Some(&routed), GeometryLevel::Routed);
        assert!(routed_json.contains("\"path\""));
    }
}
