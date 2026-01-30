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

/// A subgraph block in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphSpec {
    /// The subgraph identifier.
    pub id: String,
    /// The display title.
    pub title: String,
    /// Statements contained within the subgraph.
    pub statements: Vec<Statement>,
}

/// A statement in the flowchart AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A standalone vertex definition.
    Vertex(Vertex),
    /// An edge connecting two vertices.
    Edge(EdgeSpec),
    /// A subgraph block.
    Subgraph(SubgraphSpec),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subgraph_spec_construction() {
        let sg = SubgraphSpec {
            id: "sg1".to_string(),
            title: "My Group".to_string(),
            statements: vec![],
        };
        assert_eq!(sg.id, "sg1");
        assert_eq!(sg.title, "My Group");
        assert!(sg.statements.is_empty());
    }

    #[test]
    fn test_statement_subgraph_variant() {
        let sg = SubgraphSpec {
            id: "sg1".to_string(),
            title: "Title".to_string(),
            statements: vec![Statement::Vertex(Vertex {
                id: "A".to_string(),
                shape: None,
            })],
        };
        let stmt = Statement::Subgraph(sg);
        match &stmt {
            Statement::Subgraph(s) => {
                assert_eq!(s.id, "sg1");
                assert_eq!(s.statements.len(), 1);
            }
            _ => panic!("Expected Subgraph variant"),
        }
    }
}
