//! JSON output format for LLM integration.
//!
//! Provides structured graph topology output as JSON, designed for
//! machine consumption in LLM pipelines and agentic workflows.

use serde::Serialize;

/// Top-level JSON output structure.
///
/// This is a versioned API contract. The `version` field allows
/// consumers to handle schema evolution.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub struct GraphMetadata {
    /// Layout direction: "TD", "BT", "LR", or "RL".
    pub direction: String,
    /// Canvas width in characters (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<usize>,
    /// Canvas height in characters (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<usize>,
}

/// A node in the JSON output.
#[derive(Debug, Serialize)]
pub struct JsonNode {
    /// Node identifier (from Mermaid source).
    pub id: String,
    /// Display label.
    pub label: String,
    /// Shape name (snake_case).
    pub shape: String,
    /// Parent subgraph ID, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Layout position (present when layout is computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<JsonPosition>,
}

/// Position coordinates.
#[derive(Debug, Serialize)]
pub struct JsonPosition {
    /// X coordinate (center of node).
    pub x: usize,
    /// Y coordinate (center of node).
    pub y: usize,
}

/// An edge in the JSON output.
#[derive(Debug, Serialize)]
pub struct JsonEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Edge label, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Stroke style: "solid", "dotted", or "thick".
    pub stroke: String,
    /// Arrow at source end: "none" or "normal".
    pub arrow_start: String,
    /// Arrow at target end: "none" or "normal".
    pub arrow_end: String,
}

/// A subgraph in the JSON output.
#[derive(Debug, Serialize)]
pub struct JsonSubgraph {
    /// Subgraph identifier.
    pub id: String,
    /// Display title.
    pub title: String,
    /// IDs of nodes directly in this subgraph.
    pub children: Vec<String>,
    /// Parent subgraph ID, if nested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_json_output_has_version() {
        let output = super::JsonOutput {
            version: 1,
            metadata: super::GraphMetadata {
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
        let node = super::JsonNode {
            id: "A".to_string(),
            label: "Start".to_string(),
            shape: "rectangle".to_string(),
            parent: None,
            position: Some(super::JsonPosition { x: 20, y: 2 }),
        };
        let json = serde_json::to_string_pretty(&node).unwrap();
        assert!(json.contains("\"id\": \"A\""));
        assert!(json.contains("\"shape\": \"rectangle\""));
        assert!(json.contains("\"position\""));
    }

    #[test]
    fn test_json_edge_serialization() {
        let edge = super::JsonEdge {
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
        let sg = super::JsonSubgraph {
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
        let node = super::JsonNode {
            id: "A".to_string(),
            label: "Start".to_string(),
            shape: "rectangle".to_string(),
            parent: None,
            position: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(!json.contains("position"));
    }
}
