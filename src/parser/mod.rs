mod ast;
mod error;
mod flowchart;
pub mod info;
pub mod packet;
pub mod pie;

pub use ast::*;
pub use error::*;
pub use flowchart::*;
pub use info::parse_info;
pub use packet::parse_packet;
pub use pie::parse_pie;

/// The type of Mermaid diagram detected from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramType {
    Flowchart,
    Pie,
    Info,
    Packet,
}

/// Detect the diagram type from the first significant keyword in the input.
///
/// Skips leading whitespace, blank lines, and `%%` comment lines.
/// Returns `None` for unrecognized or unsupported diagram types.
pub fn detect_diagram_type(input: &str) -> Option<DiagramType> {
    let input = flowchart::strip_frontmatter(input);
    let first_word = input
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .and_then(|l| l.split_whitespace().next())?;

    match first_word.to_lowercase().as_str() {
        "graph" | "flowchart" => Some(DiagramType::Flowchart),
        "pie" => Some(DiagramType::Pie),
        "info" => Some(DiagramType::Info),
        "packet" | "packet-beta" => Some(DiagramType::Packet),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_flowchart_graph() {
        assert_eq!(
            detect_diagram_type("graph TD\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }

    #[test]
    fn test_detect_flowchart_keyword() {
        assert_eq!(
            detect_diagram_type("flowchart LR\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }

    #[test]
    fn test_detect_pie() {
        assert_eq!(
            detect_diagram_type("pie\n\"ash\": 100\n"),
            Some(DiagramType::Pie)
        );
    }

    #[test]
    fn test_detect_info() {
        assert_eq!(
            detect_diagram_type("info\nshowInfo\n"),
            Some(DiagramType::Info)
        );
    }

    #[test]
    fn test_detect_packet() {
        assert_eq!(
            detect_diagram_type("packet-beta\n0-7: \"Header\"\n"),
            Some(DiagramType::Packet)
        );
    }

    #[test]
    fn test_detect_packet_short() {
        assert_eq!(
            detect_diagram_type("packet\n0-7: \"Header\"\n"),
            Some(DiagramType::Packet)
        );
    }

    #[test]
    fn test_detect_skips_whitespace() {
        assert_eq!(
            detect_diagram_type("  \n  graph TD\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }

    #[test]
    fn test_detect_skips_comments() {
        assert_eq!(
            detect_diagram_type("%% comment\ngraph TD\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }

    #[test]
    fn test_detect_unknown() {
        assert_eq!(detect_diagram_type("sequence\nA->>B: hello\n"), None);
    }

    #[test]
    fn test_detect_skips_frontmatter() {
        assert_eq!(
            detect_diagram_type("---\nconfig:\n  theme: dark\n---\ngraph TD\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }

    #[test]
    fn test_detect_case_insensitive() {
        assert_eq!(
            detect_diagram_type("GRAPH TD\nA-->B\n"),
            Some(DiagramType::Flowchart)
        );
    }
}
