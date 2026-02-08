use std::fs;
use std::path::Path;

use mmdflux::diagrams::mmds::{MmdsHydrationError, from_mmds_str};
use mmdflux::graph::{Arrow, Stroke};
use mmdflux::{Direction, Shape};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mmds")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

#[test]
fn hydration_applies_defaults_to_omitted_node_and_edge_fields() {
    let payload = fixture("defaults-minimal.json");
    let diagram = from_mmds_str(&payload).expect("valid hydration");

    assert_eq!(diagram.nodes["A"].shape, Shape::Round);
    assert_eq!(diagram.edges[0].stroke, Stroke::Dotted);
    assert_eq!(diagram.edges[0].arrow_start, Arrow::Circle);
    assert_eq!(diagram.edges[0].arrow_end, Arrow::Cross);
    assert_eq!(diagram.edges[0].minlen, 2);
}

#[test]
fn hydration_maps_direction_subgraphs_and_minlen() {
    let payload = fixture("layout-with-subgraphs.json");
    let diagram = from_mmds_str(&payload).expect("valid hydration");

    assert_eq!(diagram.direction, Direction::LeftRight);
    assert_eq!(diagram.edges[0].minlen, 2);
    assert_eq!(diagram.edges[0].label.as_deref(), Some("go"));
    assert!(diagram.subgraphs.contains_key("sg1"));
    assert!(diagram.subgraphs.contains_key("sg2"));
    assert_eq!(diagram.subgraphs["sg1"].dir, Some(Direction::BottomTop));
    assert_eq!(diagram.subgraphs["sg2"].parent.as_deref(), Some("sg1"));
}

#[test]
fn hydration_rejects_dangling_edge_reference() {
    let payload = fixture("invalid/dangling-edge-target.json");
    let err = from_mmds_str(&payload).unwrap_err();

    assert!(matches!(err, MmdsHydrationError::DanglingEdgeTarget { .. }));
}

#[test]
fn hydration_rejects_missing_required_id() {
    let payload = fixture("invalid/missing-node-id.json");
    let err = from_mmds_str(&payload).unwrap_err();

    assert!(matches!(err, MmdsHydrationError::MissingNodeId { .. }));
}

#[test]
fn hydration_rejects_invalid_enum_value() {
    let payload = fixture("invalid/invalid-shape.json");
    let err = from_mmds_str(&payload).unwrap_err();

    assert!(matches!(err, MmdsHydrationError::InvalidShape { .. }));
}

#[test]
fn hydration_rejects_cyclic_subgraph_parent_chain() {
    let payload = fixture("invalid/cyclic-subgraph-parent.json");
    let err = from_mmds_str(&payload).unwrap_err();

    assert!(matches!(
        err,
        MmdsHydrationError::CyclicSubgraphParentChain { .. }
    ));
}

#[test]
fn hydration_rejects_unsupported_mmds_core_version() {
    let payload = fixture("invalid/unsupported-version.json");
    let err = from_mmds_str(&payload).unwrap_err();

    assert!(matches!(err, MmdsHydrationError::UnsupportedVersion { .. }));
}

#[test]
fn hydration_preserves_deterministic_edge_order_by_edge_id() {
    let payload = fixture("layout-unsorted-edges.json");
    let diagram1 = from_mmds_str(&payload).unwrap();
    let diagram2 = from_mmds_str(&payload).unwrap();

    assert_eq!(diagram1.edges, diagram2.edges);
    let edge_pairs: Vec<(&str, &str)> = diagram1
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    assert_eq!(edge_pairs, vec![("A", "B"), ("C", "A"), ("A", "C")]);
}

#[test]
fn hydration_ignores_unknown_extension_namespace() {
    let payload = fixture("layout-with-unknown-extension.json");
    assert!(from_mmds_str(&payload).is_ok());
}

#[test]
fn mmds_fixture_matrix_covers_valid_and_invalid_payloads() {
    let cases = [
        ("layout-valid-flowchart.json", true),
        ("layout-valid-class.json", true),
        ("invalid/dangling-edge-target.json", false),
        ("invalid/dangling-subgraph-parent.json", false),
        ("invalid/invalid-shape.json", false),
        ("invalid/unsupported-version.json", false),
    ];

    for (path, should_pass) in cases {
        let payload = fixture(path);
        assert_eq!(
            from_mmds_str(&payload).is_ok(),
            should_pass,
            "fixture {} expected pass={}",
            path,
            should_pass
        );
    }
}

#[test]
fn dangling_edge_error_message_matches_docs_example() {
    let payload = fixture("invalid/dangling-edge-target.json");
    let err = from_mmds_str(&payload).unwrap_err();
    assert_eq!(
        err.to_string(),
        "MMDS validation error: edge e0 target 'X' not found"
    );
}
