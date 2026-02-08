//! MMDS hydration and validation.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

use serde_json::{Map, Value};

use crate::graph::{Arrow, Diagram, Direction, Edge, Node, Shape, Stroke, Subgraph};
use crate::mmds::{MmdsEdge, MmdsOutput};

/// Placeholder hydration entrypoint for future MMDS input work.
pub fn stub_hydrate() {}

/// Parse MMDS JSON input into the typed output envelope.
///
/// Unlike a plain deserialize, this expands omitted node/edge fields using
/// the top-level `defaults` block before constructing `MmdsOutput`.
pub fn parse_mmds_input(input: &str) -> Result<MmdsOutput, MmdsParseError> {
    let mut value: Value = serde_json::from_str(input).map_err(|err| MmdsParseError {
        message: format!("MMDS parse error: {err}"),
    })?;

    expand_defaults_in_value(&mut value)?;

    serde_json::from_value::<MmdsOutput>(value).map_err(|err| MmdsParseError {
        message: format!("MMDS parse error: {err}"),
    })
}

/// Hydrate a graph `Diagram` from MMDS JSON text.
pub fn from_mmds_str(input: &str) -> Result<Diagram, MmdsHydrationError> {
    let output = parse_mmds_input(input).map_err(|err| MmdsHydrationError::Parse {
        message: err.to_string(),
    })?;
    from_mmds_output(&output)
}

/// Hydrate a graph `Diagram` from a parsed MMDS envelope.
pub fn from_mmds_output(output: &MmdsOutput) -> Result<Diagram, MmdsHydrationError> {
    validate_output(output)?;

    let direction = parse_direction(&output.metadata.direction).ok_or_else(|| {
        MmdsHydrationError::InvalidDirection {
            context: "metadata.direction".to_string(),
            value: output.metadata.direction.clone(),
        }
    })?;
    let mut diagram = Diagram::new(direction);

    for (index, subgraph) in output.subgraphs.iter().enumerate() {
        if subgraph.id.trim().is_empty() {
            return Err(MmdsHydrationError::MissingSubgraphId { index });
        }
        let dir = if let Some(direction) = &subgraph.direction {
            Some(parse_direction(direction).ok_or_else(|| {
                MmdsHydrationError::InvalidDirection {
                    context: format!("subgraph {} direction", subgraph.id),
                    value: direction.clone(),
                }
            })?)
        } else {
            None
        };
        diagram.subgraphs.insert(
            subgraph.id.clone(),
            Subgraph {
                id: subgraph.id.clone(),
                title: subgraph.title.clone(),
                nodes: subgraph.children.clone(),
                parent: subgraph.parent.clone(),
                dir,
            },
        );
        diagram.subgraph_order.push(subgraph.id.clone());
    }

    for (index, node) in output.nodes.iter().enumerate() {
        if node.id.trim().is_empty() {
            return Err(MmdsHydrationError::MissingNodeId { index });
        }
        let shape = parse_shape(&node.shape).ok_or_else(|| MmdsHydrationError::InvalidShape {
            node_id: node.id.clone(),
            value: node.shape.clone(),
        })?;

        let mut hydrated = Node::new(node.id.clone())
            .with_label(node.label.clone())
            .with_shape(shape);
        hydrated.parent = node.parent.clone();
        diagram.add_node(hydrated);
    }

    for node in diagram.nodes.values() {
        if let Some(parent) = &node.parent
            && !diagram.subgraphs.contains_key(parent)
        {
            return Err(MmdsHydrationError::DanglingNodeParent {
                node_id: node.id.clone(),
                parent: parent.clone(),
            });
        }
    }

    for subgraph in diagram.subgraphs.values() {
        if let Some(parent) = &subgraph.parent
            && !diagram.subgraphs.contains_key(parent)
        {
            return Err(MmdsHydrationError::DanglingSubgraphParent {
                subgraph_id: subgraph.id.clone(),
                parent: parent.clone(),
            });
        }

        for child in &subgraph.nodes {
            if !diagram.nodes.contains_key(child) {
                return Err(MmdsHydrationError::DanglingSubgraphChild {
                    subgraph_id: subgraph.id.clone(),
                    child: child.clone(),
                });
            }
        }
    }

    for subgraph_id in diagram.subgraphs.keys() {
        let mut seen = std::collections::HashSet::new();
        let mut current = subgraph_id.as_str();
        while let Some(parent) = diagram
            .subgraphs
            .get(current)
            .and_then(|subgraph| subgraph.parent.as_deref())
        {
            if !seen.insert(current) {
                return Err(MmdsHydrationError::CyclicSubgraphParentChain {
                    subgraph_id: subgraph_id.clone(),
                });
            }
            current = parent;
        }
    }

    let mut edges: Vec<(usize, &MmdsEdge)> = output.edges.iter().enumerate().collect();
    edges.sort_by(|(left_index, left), (right_index, right)| {
        compare_edge_ids(&left.id, &right.id).then(left_index.cmp(right_index))
    });

    for (index, edge) in edges {
        if edge.id.trim().is_empty() {
            return Err(MmdsHydrationError::MissingEdgeId { index });
        }
        if edge.source.trim().is_empty() {
            return Err(MmdsHydrationError::MissingEdgeSource {
                edge_id: edge.id.clone(),
            });
        }
        if edge.target.trim().is_empty() {
            return Err(MmdsHydrationError::MissingEdgeTarget {
                edge_id: edge.id.clone(),
            });
        }

        if !diagram.nodes.contains_key(&edge.source) {
            return Err(MmdsHydrationError::DanglingEdgeSource {
                edge_id: edge.id.clone(),
                source: edge.source.clone(),
            });
        }
        if !diagram.nodes.contains_key(&edge.target) {
            return Err(MmdsHydrationError::DanglingEdgeTarget {
                edge_id: edge.id.clone(),
                target: edge.target.clone(),
            });
        }
        if let Some(from_subgraph) = &edge.from_subgraph
            && !diagram.subgraphs.contains_key(from_subgraph)
        {
            return Err(MmdsHydrationError::DanglingEdgeFromSubgraphIntent {
                edge_id: edge.id.clone(),
                subgraph: from_subgraph.clone(),
            });
        }
        if let Some(to_subgraph) = &edge.to_subgraph
            && !diagram.subgraphs.contains_key(to_subgraph)
        {
            return Err(MmdsHydrationError::DanglingEdgeToSubgraphIntent {
                edge_id: edge.id.clone(),
                subgraph: to_subgraph.clone(),
            });
        }

        let stroke =
            parse_stroke(&edge.stroke).ok_or_else(|| MmdsHydrationError::InvalidStroke {
                edge_id: edge.id.clone(),
                value: edge.stroke.clone(),
            })?;
        let arrow_start =
            parse_arrow(&edge.arrow_start).ok_or_else(|| MmdsHydrationError::InvalidArrow {
                edge_id: edge.id.clone(),
                endpoint: "start".to_string(),
                value: edge.arrow_start.clone(),
            })?;
        let arrow_end =
            parse_arrow(&edge.arrow_end).ok_or_else(|| MmdsHydrationError::InvalidArrow {
                edge_id: edge.id.clone(),
                endpoint: "end".to_string(),
                value: edge.arrow_end.clone(),
            })?;

        let mut hydrated = Edge::new(edge.source.clone(), edge.target.clone())
            .with_stroke(stroke)
            .with_arrows(arrow_start, arrow_end)
            .with_minlen(edge.minlen);
        if let Some(label) = &edge.label {
            hydrated = hydrated.with_label(label.clone());
        }
        hydrated.from_subgraph = edge.from_subgraph.clone();
        hydrated.to_subgraph = edge.to_subgraph.clone();
        diagram.add_edge(hydrated);
    }

    Ok(diagram)
}

fn validate_output(output: &MmdsOutput) -> Result<(), MmdsHydrationError> {
    if output.version != 1 {
        return Err(MmdsHydrationError::UnsupportedVersion {
            version: output.version,
        });
    }

    if !matches!(output.geometry_level.as_str(), "layout" | "routed") {
        return Err(MmdsHydrationError::InvalidGeometryLevel {
            value: output.geometry_level.clone(),
        });
    }

    if !matches!(output.metadata.diagram_type.as_str(), "flowchart" | "class") {
        return Err(MmdsHydrationError::UnsupportedDiagramType {
            value: output.metadata.diagram_type.clone(),
        });
    }

    Ok(())
}

fn compare_edge_ids(left: &str, right: &str) -> Ordering {
    let left_number = parse_edge_index(left);
    let right_number = parse_edge_index(right);

    match (left_number, right_number) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

fn parse_edge_index(value: &str) -> Option<u64> {
    value.strip_prefix('e')?.parse::<u64>().ok()
}

fn expand_defaults_in_value(value: &mut Value) -> Result<(), MmdsParseError> {
    let root = value.as_object_mut().ok_or_else(|| MmdsParseError {
        message: "MMDS parse error: top-level JSON value must be an object".to_string(),
    })?;

    let node_shape = default_string(
        root,
        &["defaults", "node", "shape"],
        Value::String("rectangle".to_string()),
    );
    let edge_stroke = default_string(
        root,
        &["defaults", "edge", "stroke"],
        Value::String("solid".to_string()),
    );
    let edge_arrow_start = default_string(
        root,
        &["defaults", "edge", "arrow_start"],
        Value::String("none".to_string()),
    );
    let edge_arrow_end = default_string(
        root,
        &["defaults", "edge", "arrow_end"],
        Value::String("normal".to_string()),
    );
    let edge_minlen = default_number(root, &["defaults", "edge", "minlen"], Value::from(1));

    if let Some(nodes) = root.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            if let Some(node_obj) = node.as_object_mut() {
                node_obj
                    .entry("shape".to_string())
                    .or_insert_with(|| node_shape.clone());
            }
        }
    }

    if let Some(edges) = root.get_mut("edges").and_then(Value::as_array_mut) {
        for edge in edges {
            if let Some(edge_obj) = edge.as_object_mut() {
                edge_obj
                    .entry("stroke".to_string())
                    .or_insert_with(|| edge_stroke.clone());
                edge_obj
                    .entry("arrow_start".to_string())
                    .or_insert_with(|| edge_arrow_start.clone());
                edge_obj
                    .entry("arrow_end".to_string())
                    .or_insert_with(|| edge_arrow_end.clone());
                edge_obj
                    .entry("minlen".to_string())
                    .or_insert_with(|| edge_minlen.clone());
            }
        }
    }

    Ok(())
}

fn default_string(root: &Map<String, Value>, path: &[&str], fallback: Value) -> Value {
    traverse_value(root, path).cloned().unwrap_or(fallback)
}

fn default_number(root: &Map<String, Value>, path: &[&str], fallback: Value) -> Value {
    traverse_value(root, path).cloned().unwrap_or(fallback)
}

fn traverse_value<'a>(root: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut current = root.get(*first)?;
    for key in rest {
        current = current.get(*key)?;
    }
    Some(current)
}

fn parse_direction(value: &str) -> Option<Direction> {
    match value {
        "TD" => Some(Direction::TopDown),
        "BT" => Some(Direction::BottomTop),
        "LR" => Some(Direction::LeftRight),
        "RL" => Some(Direction::RightLeft),
        _ => None,
    }
}

fn parse_shape(value: &str) -> Option<Shape> {
    match value {
        "rectangle" => Some(Shape::Rectangle),
        "round" => Some(Shape::Round),
        "stadium" => Some(Shape::Stadium),
        "subroutine" => Some(Shape::Subroutine),
        "cylinder" => Some(Shape::Cylinder),
        "document" => Some(Shape::Document),
        "documents" => Some(Shape::Documents),
        "tagged_document" => Some(Shape::TaggedDocument),
        "card" => Some(Shape::Card),
        "tagged_rect" => Some(Shape::TaggedRect),
        "diamond" => Some(Shape::Diamond),
        "hexagon" => Some(Shape::Hexagon),
        "trapezoid" => Some(Shape::Trapezoid),
        "inv_trapezoid" => Some(Shape::InvTrapezoid),
        "parallelogram" => Some(Shape::Parallelogram),
        "inv_parallelogram" => Some(Shape::InvParallelogram),
        "manual_input" => Some(Shape::ManualInput),
        "asymmetric" => Some(Shape::Asymmetric),
        "circle" => Some(Shape::Circle),
        "double_circle" => Some(Shape::DoubleCircle),
        "small_circle" => Some(Shape::SmallCircle),
        "framed_circle" => Some(Shape::FramedCircle),
        "crossed_circle" => Some(Shape::CrossedCircle),
        "text_block" => Some(Shape::TextBlock),
        "fork_join" => Some(Shape::ForkJoin),
        _ => None,
    }
}

fn parse_stroke(value: &str) -> Option<Stroke> {
    match value {
        "solid" => Some(Stroke::Solid),
        "dotted" => Some(Stroke::Dotted),
        "thick" => Some(Stroke::Thick),
        "invisible" => Some(Stroke::Invisible),
        _ => None,
    }
}

fn parse_arrow(value: &str) -> Option<Arrow> {
    match value {
        "normal" => Some(Arrow::Normal),
        "none" => Some(Arrow::None),
        "cross" => Some(Arrow::Cross),
        "circle" => Some(Arrow::Circle),
        _ => None,
    }
}

/// Parse-time error for MMDS scaffolding input.
#[derive(Debug, Clone)]
pub struct MmdsParseError {
    message: String,
}

impl fmt::Display for MmdsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for MmdsParseError {}

/// MMDS hydration and validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmdsHydrationError {
    Parse {
        message: String,
    },
    UnsupportedVersion {
        version: u32,
    },
    UnsupportedDiagramType {
        value: String,
    },
    InvalidGeometryLevel {
        value: String,
    },
    InvalidDirection {
        context: String,
        value: String,
    },
    InvalidShape {
        node_id: String,
        value: String,
    },
    InvalidStroke {
        edge_id: String,
        value: String,
    },
    InvalidArrow {
        edge_id: String,
        endpoint: String,
        value: String,
    },
    MissingNodeId {
        index: usize,
    },
    MissingSubgraphId {
        index: usize,
    },
    MissingEdgeId {
        index: usize,
    },
    MissingEdgeSource {
        edge_id: String,
    },
    MissingEdgeTarget {
        edge_id: String,
    },
    DanglingEdgeSource {
        edge_id: String,
        source: String,
    },
    DanglingEdgeTarget {
        edge_id: String,
        target: String,
    },
    DanglingEdgeFromSubgraphIntent {
        edge_id: String,
        subgraph: String,
    },
    DanglingEdgeToSubgraphIntent {
        edge_id: String,
        subgraph: String,
    },
    DanglingNodeParent {
        node_id: String,
        parent: String,
    },
    DanglingSubgraphParent {
        subgraph_id: String,
        parent: String,
    },
    DanglingSubgraphChild {
        subgraph_id: String,
        child: String,
    },
    CyclicSubgraphParentChain {
        subgraph_id: String,
    },
}

impl fmt::Display for MmdsHydrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MmdsHydrationError::Parse { message } => write!(f, "{message}"),
            MmdsHydrationError::UnsupportedVersion { version } => {
                write!(f, "MMDS validation error: unsupported version {version}")
            }
            MmdsHydrationError::UnsupportedDiagramType { value } => {
                write!(
                    f,
                    "MMDS validation error: unsupported diagram_type '{value}'"
                )
            }
            MmdsHydrationError::InvalidGeometryLevel { value } => {
                write!(f, "MMDS validation error: invalid geometry_level '{value}'")
            }
            MmdsHydrationError::InvalidDirection { context, value } => {
                write!(
                    f,
                    "MMDS validation error: invalid direction '{value}' for {context}"
                )
            }
            MmdsHydrationError::InvalidShape { node_id, value } => write!(
                f,
                "MMDS validation error: node {node_id} has invalid shape '{value}'"
            ),
            MmdsHydrationError::InvalidStroke { edge_id, value } => write!(
                f,
                "MMDS validation error: edge {edge_id} has invalid stroke '{value}'"
            ),
            MmdsHydrationError::InvalidArrow {
                edge_id,
                endpoint,
                value,
            } => write!(
                f,
                "MMDS validation error: edge {edge_id} has invalid {endpoint} arrow '{value}'"
            ),
            MmdsHydrationError::MissingNodeId { index } => {
                write!(
                    f,
                    "MMDS validation error: node at index {index} is missing id"
                )
            }
            MmdsHydrationError::MissingSubgraphId { index } => write!(
                f,
                "MMDS validation error: subgraph at index {index} is missing id"
            ),
            MmdsHydrationError::MissingEdgeId { index } => {
                write!(
                    f,
                    "MMDS validation error: edge at index {index} is missing id"
                )
            }
            MmdsHydrationError::MissingEdgeSource { edge_id } => {
                write!(f, "MMDS validation error: edge {edge_id} is missing source")
            }
            MmdsHydrationError::MissingEdgeTarget { edge_id } => {
                write!(f, "MMDS validation error: edge {edge_id} is missing target")
            }
            MmdsHydrationError::DanglingEdgeSource { edge_id, source } => write!(
                f,
                "MMDS validation error: edge {edge_id} source '{source}' not found"
            ),
            MmdsHydrationError::DanglingEdgeTarget { edge_id, target } => write!(
                f,
                "MMDS validation error: edge {edge_id} target '{target}' not found"
            ),
            MmdsHydrationError::DanglingEdgeFromSubgraphIntent { edge_id, subgraph } => write!(
                f,
                "MMDS validation error: edge {edge_id} from_subgraph '{subgraph}' not found"
            ),
            MmdsHydrationError::DanglingEdgeToSubgraphIntent { edge_id, subgraph } => write!(
                f,
                "MMDS validation error: edge {edge_id} to_subgraph '{subgraph}' not found"
            ),
            MmdsHydrationError::DanglingNodeParent { node_id, parent } => write!(
                f,
                "MMDS validation error: node {node_id} parent subgraph '{parent}' not found"
            ),
            MmdsHydrationError::DanglingSubgraphParent {
                subgraph_id,
                parent,
            } => write!(
                f,
                "MMDS validation error: subgraph {subgraph_id} parent '{parent}' not found"
            ),
            MmdsHydrationError::DanglingSubgraphChild { subgraph_id, child } => write!(
                f,
                "MMDS validation error: subgraph {subgraph_id} child '{child}' not found"
            ),
            MmdsHydrationError::CyclicSubgraphParentChain { subgraph_id } => write!(
                f,
                "MMDS validation error: cyclic subgraph parent chain detected at '{subgraph_id}'"
            ),
        }
    }
}

impl Error for MmdsHydrationError {}
