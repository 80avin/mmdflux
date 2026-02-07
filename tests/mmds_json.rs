//! MMDS JSON contract tests.
//!
//! Verifies that `--format mmds` output (with `json` alias) matches the MMDS specification:
//! - Default output is `geometry_level: "layout"` with no edge geometry.
//! - Routed output is explicit opt-in with edge paths and bounds.

use std::path::Path;

use mmdflux::diagram::{GeometryLevel, OutputFormat, RenderConfig};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::mmds::MmdsOutput;
use mmdflux::registry::DiagramInstance;

fn render_json(input: &str) -> String {
    let mut instance = FlowchartInstance::new();
    instance.parse(input).unwrap();
    instance
        .render(OutputFormat::Json, &RenderConfig::default())
        .unwrap()
}

fn render_json_with_level(input: &str, level: GeometryLevel) -> String {
    let mut instance = FlowchartInstance::new();
    instance.parse(input).unwrap();
    let config = RenderConfig {
        geometry_level: level,
        ..RenderConfig::default()
    };
    instance.render(OutputFormat::Json, &config).unwrap()
}

// -----------------------------------------------------------------------
// Contract: MMDS envelope
// -----------------------------------------------------------------------

#[test]
fn mmds_default_has_version_1() {
    let json = render_json("graph TD\nA-->B");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.version, 1);
}

#[test]
fn mmds_default_geometry_level_is_layout() {
    let json = render_json("graph TD\nA-->B");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.geometry_level, "layout");
}

#[test]
fn mmds_has_metadata_with_direction() {
    let json = render_json("graph LR\nA-->B");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.metadata.diagram_type, "flowchart");
    assert_eq!(output.metadata.direction, "LR");
}

#[test]
fn mmds_has_nodes_and_edges() {
    let json = render_json("graph TD\nA[Start]-->B[End]");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.nodes.len(), 2);
    assert_eq!(output.edges.len(), 1);
}

// -----------------------------------------------------------------------
// Contract: layout-level node geometry
// -----------------------------------------------------------------------

#[test]
fn mmds_layout_nodes_have_positions_and_sizes() {
    let json = render_json("graph TD\nA[Start]-->B[End]");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let node_a = output.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(node_a.label, "Start");
    assert_eq!(node_a.shape, "rectangle");
    assert!(node_a.size.width > 0.0);
    assert!(node_a.size.height > 0.0);
}

#[test]
fn mmds_layout_nodes_sorted_by_id() {
    let json = render_json("graph TD\nC-->B\nB-->A");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    let ids: Vec<&str> = output.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["A", "B", "C"]);
}

#[test]
fn mmds_layout_node_shapes() {
    let json = render_json("graph TD\nA[Rect]\nB(Round)\nC{Diamond}\nD([Stadium])");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

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

// -----------------------------------------------------------------------
// Contract: layout-level edges have NO geometry
// -----------------------------------------------------------------------

#[test]
fn mmds_layout_edges_exclude_path() {
    let json = render_json("graph TD\nA-->B");
    assert!(
        !json.contains("\"path\""),
        "layout JSON must not contain path"
    );
}

#[test]
fn mmds_layout_edges_exclude_is_backward() {
    let json = render_json("graph TD\nA-->B\nB-->A");
    assert!(
        !json.contains("\"is_backward\""),
        "layout JSON must not contain is_backward"
    );
}

#[test]
fn mmds_layout_edges_exclude_label_position() {
    let json = render_json("graph TD\nA--label-->B");
    assert!(
        !json.contains("\"label_position\""),
        "layout JSON must not contain label_position"
    );
}

#[test]
fn mmds_layout_edges_have_topology() {
    let json = render_json("graph TD\nA-.label.->B");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let edge = &output.edges[0];
    assert_eq!(edge.source, "A");
    assert_eq!(edge.target, "B");
    assert_eq!(edge.stroke, "dotted");
    assert_eq!(edge.label, Some("label".to_string()));
    assert_eq!(edge.arrow_start, "none");
    assert_eq!(edge.arrow_end, "normal");
}

#[test]
fn mmds_layout_metadata_has_no_bounds() {
    let json = render_json("graph TD\nA-->B");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert!(output.metadata.bounds.is_none());
}

// -----------------------------------------------------------------------
// Contract: layout-level subgraphs
// -----------------------------------------------------------------------

#[test]
fn mmds_layout_subgraphs() {
    let json = render_json("graph TD\nsubgraph sg1[Group]\nA-->B\nend");
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    assert_eq!(output.subgraphs.len(), 1);
    assert_eq!(output.subgraphs[0].id, "sg1");
    assert_eq!(output.subgraphs[0].title, "Group");
    assert!(output.subgraphs[0].bounds.is_none());
}

// -----------------------------------------------------------------------
// Contract: routed-level output
// -----------------------------------------------------------------------

#[test]
fn mmds_routed_has_geometry_level_routed() {
    let json = render_json_with_level("graph TD\nA-->B", GeometryLevel::Routed);
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(output.geometry_level, "routed");
}

#[test]
fn mmds_routed_includes_edge_paths() {
    let json = render_json_with_level("graph TD\nA-->B", GeometryLevel::Routed);
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let edge = &output.edges[0];
    assert!(edge.path.is_some());
    assert!(edge.path.as_ref().unwrap().len() >= 2);
    assert!(edge.is_backward.is_some());
}

#[test]
fn mmds_routed_includes_metadata_bounds() {
    let json = render_json_with_level("graph TD\nA-->B", GeometryLevel::Routed);
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let bounds = output.metadata.bounds.as_ref().unwrap();
    assert!(bounds.width > 0.0);
    assert!(bounds.height > 0.0);
}

#[test]
fn mmds_routed_subgraph_bounds() {
    let json = render_json_with_level(
        "graph TD\nsubgraph sg1[Group]\nA-->B\nend",
        GeometryLevel::Routed,
    );
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let sg = &output.subgraphs[0];
    assert!(sg.bounds.is_some());
    assert!(sg.bounds.as_ref().unwrap().width > 0.0);
}

#[test]
fn mmds_routed_label_position_for_labeled_edge() {
    let json = render_json_with_level("graph TD\nA--label-->B", GeometryLevel::Routed);
    let output: MmdsOutput = serde_json::from_str(&json).unwrap();

    let edge = &output.edges[0];
    assert!(edge.label_position.is_some());
}

// -----------------------------------------------------------------------
// Contract: direction variants
// -----------------------------------------------------------------------

#[test]
fn mmds_direction_variants() {
    for (dir_str, expected) in [("TD", "TD"), ("LR", "LR"), ("BT", "BT"), ("RL", "RL")] {
        let input = format!("graph {dir_str}\nA-->B");
        let json = render_json(&input);
        let output: MmdsOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(output.metadata.direction, expected);
    }
}

// -----------------------------------------------------------------------
// Contract: class diagram MMDS output
// -----------------------------------------------------------------------

#[test]
fn mmds_class_diagram_produces_json() {
    use mmdflux::diagrams::class::ClassInstance;

    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA --> B").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Json, &config).unwrap();
    let parsed: MmdsOutput = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.geometry_level, "layout");
    assert_eq!(parsed.metadata.diagram_type, "class");
    assert!(!output.contains("\"path\""));
}

#[test]
fn mmds_class_diagram_routed_level() {
    use mmdflux::diagrams::class::ClassInstance;

    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA --> B").unwrap();

    let config = RenderConfig {
        geometry_level: GeometryLevel::Routed,
        ..RenderConfig::default()
    };
    let output = instance.render(OutputFormat::Json, &config).unwrap();
    let parsed: MmdsOutput = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed.geometry_level, "routed");
    assert!(output.contains("\"path\""));
}

// -----------------------------------------------------------------------
// Schema and documentation artifacts
// -----------------------------------------------------------------------

#[test]
fn mmds_schema_exists_and_has_required_fields() {
    let schema = std::fs::read_to_string("docs/mmds.schema.json").unwrap();
    assert!(schema.contains("\"$schema\""));
    assert!(schema.contains("\"properties\""));
    assert!(schema.contains("\"geometry_level\""));
    assert!(schema.contains("\"layout\""));
    assert!(schema.contains("\"routed\""));
}

#[test]
fn mmds_spec_doc_exists() {
    assert!(Path::new("docs/mmds.md").exists());
}

#[test]
fn mmds_examples_exist() {
    assert!(Path::new("examples/mmds/react_flow.js").exists());
    assert!(Path::new("examples/mmds/cytoscape.js").exists());
    assert!(Path::new("examples/mmds/d3.js").exists());
    assert!(Path::new("examples/mmds/svg_passthrough.js").exists());
}

#[test]
fn readme_mentions_mmds() {
    let readme = std::fs::read_to_string("README.md").unwrap();
    assert!(readme.contains("MMDS"));
}
