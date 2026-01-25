//! Abstract Syntax Tree types for parsed Mermaid flowcharts.

/// Shape specification from parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeSpec {
    /// Rectangle: [text]
    Rectangle(String),
    /// Rounded: (text)
    Round(String),
    /// Diamond: {text}
    Diamond(String),
}

impl ShapeSpec {
    /// Get the text content of the shape.
    pub fn text(&self) -> &str {
        match self {
            ShapeSpec::Rectangle(s) | ShapeSpec::Round(s) | ShapeSpec::Diamond(s) => s,
        }
    }
}

/// A vertex (node definition) in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vertex {
    /// The node identifier.
    pub id: String,
    /// Optional shape with label text.
    pub shape: Option<ShapeSpec>,
}

impl Vertex {
    /// Create a new vertex with just an ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            shape: None,
        }
    }

    /// Create a vertex with a shape.
    pub fn with_shape(id: impl Into<String>, shape: ShapeSpec) -> Self {
        Self {
            id: id.into(),
            shape: Some(shape),
        }
    }
}

/// Edge connector type from parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorSpec {
    /// Solid arrow: -->
    SolidArrow,
    /// Solid arrow with label: --|label|>
    SolidArrowLabel(String),
    /// Dotted arrow: -.->
    DottedArrow,
    /// Dotted arrow with label: -.|label|.->
    DottedArrowLabel(String),
    /// Thick arrow: ==>
    ThickArrow,
    /// Thick arrow with label: ==|label|=>
    ThickArrowLabel(String),
    /// Open line (no arrow): ---
    OpenLine,
    /// Open line with label: --|label|-
    OpenLineLabel(String),
}

impl ConnectorSpec {
    /// Get the label if present.
    pub fn label(&self) -> Option<&str> {
        match self {
            ConnectorSpec::SolidArrowLabel(s)
            | ConnectorSpec::DottedArrowLabel(s)
            | ConnectorSpec::ThickArrowLabel(s)
            | ConnectorSpec::OpenLineLabel(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this connector has an arrow head.
    pub fn has_arrow(&self) -> bool {
        !matches!(
            self,
            ConnectorSpec::OpenLine | ConnectorSpec::OpenLineLabel(_)
        )
    }
}

/// An edge statement in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeSpec {
    /// Source vertex.
    pub from: Vertex,
    /// Edge connector type.
    pub connector: ConnectorSpec,
    /// Target vertex.
    pub to: Vertex,
}

/// A statement in the flowchart AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A standalone vertex definition.
    Vertex(Vertex),
    /// An edge connecting two vertices.
    Edge(EdgeSpec),
}
