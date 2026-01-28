//! Pest parser implementation for Mermaid flowcharts.

use pest::Parser;
use pest_derive::Parser;

use super::ast::{ConnectorSpec, EdgeSpec, ShapeSpec, Statement, Vertex};
use super::error::ParseError;

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct FlowchartParser;

/// Direction of the flowchart layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomTop,
    LeftRight,
    RightLeft,
}

impl Direction {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "TD" | "TB" => Some(Direction::TopDown),
            "BT" => Some(Direction::BottomTop),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }
}

/// Parsed flowchart containing direction and statements.
#[derive(Debug, Clone)]
pub struct Flowchart {
    pub direction: Direction,
    pub statements: Vec<Statement>,
}

impl Flowchart {
    /// Get all vertices (from both standalone vertex statements and edges).
    pub fn vertices(&self) -> Vec<&Vertex> {
        let mut result = Vec::new();
        for stmt in &self.statements {
            match stmt {
                Statement::Vertex(v) => result.push(v),
                Statement::Edge(e) => {
                    result.push(&e.from);
                    result.push(&e.to);
                }
            }
        }
        result
    }

    /// Get all edges.
    pub fn edges(&self) -> Vec<&EdgeSpec> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Edge(e) => Some(e),
                _ => None,
            })
            .collect()
    }
}

/// Parse a flowchart string.
pub fn parse_flowchart(input: &str) -> Result<Flowchart, ParseError> {
    let pairs =
        FlowchartParser::parse(Rule::flowchart, input).map_err(ParseError::from_pest_error)?;

    let mut direction = Direction::TopDown;
    let mut statements = Vec::new();

    for pair in pairs {
        if pair.as_rule() == Rule::flowchart {
            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::header => {
                        for header_part in inner.into_inner() {
                            if header_part.as_rule() == Rule::direction {
                                direction = Direction::from_str(header_part.as_str())
                                    .unwrap_or(Direction::TopDown);
                            }
                        }
                    }
                    Rule::statement => {
                        let stmts = parse_statement(inner);
                        statements.extend(stmts);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(Flowchart {
        direction,
        statements,
    })
}

fn parse_statement(pair: pest::iterators::Pair<Rule>) -> Vec<Statement> {
    let mut statements = Vec::new();

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::vertex_statement {
            statements.extend(parse_vertex_statement(inner));
        }
    }

    statements
}

fn parse_vertex_statement(pair: pest::iterators::Pair<Rule>) -> Vec<Statement> {
    let mut statements = Vec::new();
    let mut current_nodes: Vec<Vertex> = Vec::new();
    let mut segments: Vec<(ConnectorSpec, Vec<Vertex>)> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::node_group => {
                if segments.is_empty() {
                    // This is the first node group (source nodes)
                    current_nodes = parse_node_group(inner);
                }
            }
            Rule::edge_segment => {
                let (connector, nodes) = parse_edge_segment(inner);
                segments.push((connector, nodes));
            }
            _ => {}
        }
    }

    if segments.is_empty() {
        // No edges, just standalone node(s)
        for node in current_nodes {
            statements.push(Statement::Vertex(node));
        }
    } else {
        // Process chain of edges
        let mut source_nodes = current_nodes;

        for (connector, target_nodes) in segments {
            // Create edges from each source to each target (cartesian product for &)
            for source in &source_nodes {
                for target in &target_nodes {
                    statements.push(Statement::Edge(EdgeSpec {
                        from: source.clone(),
                        connector: connector.clone(),
                        to: target.clone(),
                    }));
                }
            }
            // For chains, the targets become the sources for the next segment
            source_nodes = target_nodes;
        }
    }

    statements
}

fn parse_node_group(pair: pest::iterators::Pair<Rule>) -> Vec<Vertex> {
    let mut nodes = Vec::new();

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::node {
            nodes.push(parse_node(inner));
        }
    }

    nodes
}

fn parse_edge_segment(pair: pest::iterators::Pair<Rule>) -> (ConnectorSpec, Vec<Vertex>) {
    let mut connector = ConnectorSpec::SolidArrow;
    let mut nodes = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::edge_connector => {
                connector = parse_connector(inner);
            }
            Rule::node_group => {
                nodes = parse_node_group(inner);
            }
            _ => {}
        }
    }

    (connector, nodes)
}

fn parse_connector(pair: pest::iterators::Pair<Rule>) -> ConnectorSpec {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::arrow_solid => return ConnectorSpec::SolidArrow,
            Rule::arrow_solid_label => {
                if let Some(label) = extract_edge_label(inner) {
                    return ConnectorSpec::SolidArrowLabel(label);
                }
                return ConnectorSpec::SolidArrow;
            }
            Rule::arrow_dotted => return ConnectorSpec::DottedArrow,
            Rule::arrow_dotted_label => {
                if let Some(label) = extract_edge_label(inner) {
                    return ConnectorSpec::DottedArrowLabel(label);
                }
                return ConnectorSpec::DottedArrow;
            }
            Rule::arrow_thick => return ConnectorSpec::ThickArrow,
            Rule::arrow_thick_label => {
                if let Some(label) = extract_edge_label(inner) {
                    return ConnectorSpec::ThickArrowLabel(label);
                }
                return ConnectorSpec::ThickArrow;
            }
            Rule::line_open => return ConnectorSpec::OpenLine,
            Rule::line_open_label => {
                if let Some(label) = extract_edge_label(inner) {
                    return ConnectorSpec::OpenLineLabel(label);
                }
                return ConnectorSpec::OpenLine;
            }
            _ => {}
        }
    }
    ConnectorSpec::SolidArrow
}

fn extract_edge_label(pair: pest::iterators::Pair<Rule>) -> Option<String> {
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::edge_label {
            for text in inner.into_inner() {
                if text.as_rule() == Rule::edge_label_text {
                    return Some(text.as_str().to_string());
                }
            }
        }
    }
    None
}

fn parse_node(pair: pest::iterators::Pair<Rule>) -> Vertex {
    let mut id = String::new();
    let mut shape = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier => {
                id = inner.as_str().to_string();
            }
            Rule::shape => {
                shape = parse_shape(inner);
            }
            _ => {}
        }
    }

    Vertex { id, shape }
}

fn parse_shape(pair: pest::iterators::Pair<Rule>) -> Option<ShapeSpec> {
    for inner in pair.into_inner() {
        let (text_rule, constructor): (Rule, fn(String) -> ShapeSpec) = match inner.as_rule() {
            Rule::shape_rect => (Rule::text_rect, ShapeSpec::Rectangle),
            Rule::shape_round => (Rule::text_round, ShapeSpec::Round),
            Rule::shape_diamond => (Rule::text_diamond, ShapeSpec::Diamond),
            _ => continue,
        };
        for text in inner.into_inner() {
            if text.as_rule() == text_rule {
                return Some(constructor(text.as_str().to_string()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Phase 1: Header tests
    #[test]
    fn test_parse_graph_td() {
        let result = parse_flowchart("graph TD\n").unwrap();
        assert_eq!(result.direction, Direction::TopDown);
    }

    #[test]
    fn test_parse_graph_lr() {
        let result = parse_flowchart("graph LR\n").unwrap();
        assert_eq!(result.direction, Direction::LeftRight);
    }

    #[test]
    fn test_parse_flowchart_tb() {
        let result = parse_flowchart("flowchart TB\n").unwrap();
        assert_eq!(result.direction, Direction::TopDown);
    }

    #[test]
    fn test_parse_flowchart_rl() {
        let result = parse_flowchart("flowchart RL\n").unwrap();
        assert_eq!(result.direction, Direction::RightLeft);
    }

    #[test]
    fn test_parse_graph_bt() {
        let result = parse_flowchart("graph BT\n").unwrap();
        assert_eq!(result.direction, Direction::BottomTop);
    }

    #[test]
    fn test_case_insensitive() {
        let result = parse_flowchart("GRAPH td\n").unwrap();
        assert_eq!(result.direction, Direction::TopDown);
    }

    // Phase 2: Node tests
    #[test]
    fn test_parse_node_bare() {
        let result = parse_flowchart("graph TD\nA\n").unwrap();
        let vertices = result.vertices();
        assert_eq!(vertices.len(), 1);
        assert_eq!(vertices[0].id, "A");
        assert!(vertices[0].shape.is_none());
    }

    #[test]
    fn test_parse_node_rectangle() {
        let result = parse_flowchart("graph TD\nA[Hello World]\n").unwrap();
        let vertices = result.vertices();
        assert_eq!(vertices.len(), 1);
        assert_eq!(vertices[0].id, "A");
        assert_eq!(
            vertices[0].shape,
            Some(ShapeSpec::Rectangle("Hello World".to_string()))
        );
    }

    #[test]
    fn test_parse_node_round() {
        let result = parse_flowchart("graph TD\nB(Rounded Node)\n").unwrap();
        let vertices = result.vertices();
        assert_eq!(vertices.len(), 1);
        assert_eq!(vertices[0].id, "B");
        assert_eq!(
            vertices[0].shape,
            Some(ShapeSpec::Round("Rounded Node".to_string()))
        );
    }

    #[test]
    fn test_parse_node_diamond() {
        let result = parse_flowchart("graph TD\nC{Decision?}\n").unwrap();
        let vertices = result.vertices();
        assert_eq!(vertices.len(), 1);
        assert_eq!(vertices[0].id, "C");
        assert_eq!(
            vertices[0].shape,
            Some(ShapeSpec::Diamond("Decision?".to_string()))
        );
    }

    #[test]
    fn test_parse_multiple_nodes() {
        let result = parse_flowchart("graph TD\nA[Start]\nB(Process)\nC{End?}\n").unwrap();
        let vertices = result.vertices();
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].id, "A");
        assert_eq!(vertices[1].id, "B");
        assert_eq!(vertices[2].id, "C");
    }

    #[test]
    fn test_parse_node_with_underscore() {
        let result = parse_flowchart("graph TD\nmy_node[Label]\n").unwrap();
        assert_eq!(result.vertices()[0].id, "my_node");
    }

    #[test]
    fn test_parse_node_with_numbers() {
        let result = parse_flowchart("graph TD\nnode123[Label]\n").unwrap();
        assert_eq!(result.vertices()[0].id, "node123");
    }

    // Phase 3: Edge tests
    #[test]
    fn test_parse_solid_arrow() {
        let result = parse_flowchart("graph TD\nA --> B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[0].connector, ConnectorSpec::SolidArrow);
    }

    #[test]
    fn test_parse_solid_arrow_with_label() {
        let result = parse_flowchart("graph TD\nA -->|yes| B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[0].connector.label(), Some("yes"));
    }

    #[test]
    fn test_parse_dotted_arrow() {
        let result = parse_flowchart("graph TD\nA -.-> B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].connector, ConnectorSpec::DottedArrow);
    }

    #[test]
    fn test_parse_thick_arrow() {
        let result = parse_flowchart("graph TD\nA ==> B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].connector, ConnectorSpec::ThickArrow);
    }

    #[test]
    fn test_parse_open_line() {
        let result = parse_flowchart("graph TD\nA --- B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].connector, ConnectorSpec::OpenLine);
        assert!(!edges[0].connector.has_arrow());
    }

    #[test]
    fn test_parse_edge_with_node_shapes() {
        let result = parse_flowchart("graph TD\nA[Start] --> B{Decision}\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(
            edges[0].from.shape,
            Some(ShapeSpec::Rectangle("Start".to_string()))
        );
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(
            edges[0].to.shape,
            Some(ShapeSpec::Diamond("Decision".to_string()))
        );
    }

    #[test]
    fn test_parse_multiple_edges() {
        let result = parse_flowchart("graph TD\nA --> B\nB --> C\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[1].from.id, "B");
        assert_eq!(edges[1].to.id, "C");
    }

    #[test]
    fn test_parse_comment() {
        let result = parse_flowchart("graph TD\n%% This is a comment\nA --> B\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 1);
    }

    // Phase 4: Chain and ampersand tests
    #[test]
    fn test_parse_chain() {
        let result = parse_flowchart("graph TD\nA --> B --> C\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[1].from.id, "B");
        assert_eq!(edges[1].to.id, "C");
    }

    #[test]
    fn test_parse_long_chain() {
        let result = parse_flowchart("graph TD\nA --> B --> C --> D\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[1].from.id, "B");
        assert_eq!(edges[1].to.id, "C");
        assert_eq!(edges[2].from.id, "C");
        assert_eq!(edges[2].to.id, "D");
    }

    #[test]
    fn test_parse_ampersand_source() {
        let result = parse_flowchart("graph TD\nA & B --> C\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "C");
        assert_eq!(edges[1].from.id, "B");
        assert_eq!(edges[1].to.id, "C");
    }

    #[test]
    fn test_parse_ampersand_target() {
        let result = parse_flowchart("graph TD\nA --> B & C\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].from.id, "A");
        assert_eq!(edges[0].to.id, "B");
        assert_eq!(edges[1].from.id, "A");
        assert_eq!(edges[1].to.id, "C");
    }

    #[test]
    fn test_parse_ampersand_both() {
        let result = parse_flowchart("graph TD\nA & B --> C & D\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 4);
        // A -> C, A -> D, B -> C, B -> D
        let edge_pairs: Vec<(&str, &str)> = edges
            .iter()
            .map(|e| (e.from.id.as_str(), e.to.id.as_str()))
            .collect();
        assert!(edge_pairs.contains(&("A", "C")));
        assert!(edge_pairs.contains(&("A", "D")));
        assert!(edge_pairs.contains(&("B", "C")));
        assert!(edge_pairs.contains(&("B", "D")));
    }

    #[test]
    fn test_parse_chain_with_labels() {
        let result = parse_flowchart("graph TD\nA -->|step1| B -->|step2| C\n").unwrap();
        let edges = result.edges();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].connector.label(), Some("step1"));
        assert_eq!(edges[1].connector.label(), Some("step2"));
    }
}
