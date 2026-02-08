//! MMDS conformance harness.
//!
//! Tiered conformance checks comparing the direct render pipeline
//! (Mermaid text → Diagram → render) against the MMDS roundtrip pipeline
//! (Mermaid text → Diagram → MMDS JSON → hydrate → Diagram → render).
//!
//! Three tiers:
//! - **Semantic**: graph structure equivalence (nodes, edges, subgraphs, direction)
//! - **Layout**: geometry-level equivalence (node positions, edge topology)
//! - **Visual**: rendered text output equivalence

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::diagrams::flowchart::engine::layout_with_selected_engine;
use mmdflux::diagrams::flowchart::geometry::GraphGeometry;
use mmdflux::diagrams::mmds::from_mmds_str;
use mmdflux::graph::{Diagram, Subgraph};
use mmdflux::registry::DiagramInstance;
use mmdflux::render::{RenderOptions, render};

// ---------------------------------------------------------------------------
// Conformance report model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum TierStatus {
    Pass,
    Fail(String),
}

impl TierStatus {
    fn is_pass(&self) -> bool {
        matches!(self, TierStatus::Pass)
    }
}

#[derive(Debug, Clone)]
struct TierResult {
    tier: &'static str,
    status: TierStatus,
}

#[derive(Debug)]
struct ConformanceReport {
    fixture_path: String,
    semantic: TierResult,
    layout: TierResult,
    visual: TierResult,
}

impl ConformanceReport {
    fn tiers(&self) -> [&TierResult; 3] {
        [&self.semantic, &self.layout, &self.visual]
    }
}

// ---------------------------------------------------------------------------
// Harness: run a single conformance case
// ---------------------------------------------------------------------------

fn fixture_input(family: &str, name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(family)
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

/// Compare two Diagrams for semantic equivalence.
///
/// Checks direction, nodes (sorted by ID), edges (by index), and subgraphs.
fn check_semantic(direct: &Diagram, roundtrip: &Diagram) -> TierResult {
    let mut mismatches = Vec::new();

    if direct.direction != roundtrip.direction {
        mismatches.push(format!(
            "direction: {:?} vs {:?}",
            direct.direction, roundtrip.direction
        ));
    }

    // Compare nodes (sorted by ID for determinism)
    let direct_nodes: BTreeMap<_, _> = direct.nodes.iter().collect();
    let roundtrip_nodes: BTreeMap<_, _> = roundtrip.nodes.iter().collect();

    if direct_nodes.len() != roundtrip_nodes.len() {
        mismatches.push(format!(
            "node count: {} vs {}",
            direct_nodes.len(),
            roundtrip_nodes.len()
        ));
    } else {
        for (id, d_node) in &direct_nodes {
            match roundtrip_nodes.get(id) {
                None => mismatches.push(format!("node {id} missing in roundtrip")),
                Some(r_node) => {
                    if d_node != r_node {
                        mismatches.push(format!("node {id} differs"));
                    }
                }
            }
        }
    }

    // Compare edges (by index order)
    if direct.edges.len() != roundtrip.edges.len() {
        mismatches.push(format!(
            "edge count: {} vs {}",
            direct.edges.len(),
            roundtrip.edges.len()
        ));
    } else {
        for (i, (d_edge, r_edge)) in direct.edges.iter().zip(&roundtrip.edges).enumerate() {
            if d_edge != r_edge {
                mismatches.push(format!("edge {i} differs"));
            }
        }
    }

    // Compare subgraphs (sorted by ID).
    //
    // Normalize node lists to direct children only. The Mermaid parser puts
    // all descendants into each subgraph's node list, while MMDS correctly
    // uses direct children. We filter the direct diagram's node lists to
    // only those nodes whose parent matches the subgraph ID.
    let direct_sgs: BTreeMap<_, _> = direct.subgraphs.iter().collect();
    let roundtrip_sgs: BTreeMap<_, _> = roundtrip.subgraphs.iter().collect();

    if direct_sgs.len() != roundtrip_sgs.len() {
        mismatches.push(format!(
            "subgraph count: {} vs {}",
            direct_sgs.len(),
            roundtrip_sgs.len()
        ));
    } else {
        for (id, d_sg) in &direct_sgs {
            match roundtrip_sgs.get(id) {
                None => mismatches.push(format!("subgraph {id} missing in roundtrip")),
                Some(r_sg) => {
                    // Normalize: filter direct diagram's node list to direct children
                    let direct_children: Vec<String> = d_sg
                        .nodes
                        .iter()
                        .filter(|node_id| {
                            direct.nodes.get(*node_id).and_then(|n| n.parent.as_deref())
                                == Some(&d_sg.id)
                        })
                        .cloned()
                        .collect();
                    let normalized_d = Subgraph {
                        nodes: direct_children,
                        ..(*d_sg).clone()
                    };
                    if &normalized_d != *r_sg {
                        mismatches.push(format!("subgraph {id} differs"));
                    }
                }
            }
        }
    }

    TierResult {
        tier: "semantic",
        status: if mismatches.is_empty() {
            TierStatus::Pass
        } else {
            TierStatus::Fail(mismatches.join("; "))
        },
    }
}

/// Compare rendered text output for visual equivalence.
fn check_visual(direct: &Diagram, roundtrip: &Diagram) -> TierResult {
    let options = RenderOptions::default();
    let direct_text = render(direct, &options);
    let roundtrip_text = render(roundtrip, &options);

    TierResult {
        tier: "visual",
        status: if direct_text == roundtrip_text {
            TierStatus::Pass
        } else {
            TierStatus::Fail("rendered text differs".into())
        },
    }
}

/// Float tolerance for geometry comparison.
const GEOMETRY_TOLERANCE: f64 = 0.01;

fn floats_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < GEOMETRY_TOLERANCE
}

/// Compare layout geometry for equivalence.
///
/// Runs dagre layout on both diagrams and compares node positions/sizes,
/// edge count, and overall bounds.
fn check_layout(direct: &Diagram, roundtrip: &Diagram) -> TierResult {
    let config = RenderConfig::default();

    let direct_geom = match layout_with_selected_engine(direct, &config) {
        Ok(result) => result.geometry,
        Err(e) => {
            return TierResult {
                tier: "layout",
                status: TierStatus::Fail(format!("direct layout failed: {e}")),
            };
        }
    };

    let roundtrip_geom = match layout_with_selected_engine(roundtrip, &config) {
        Ok(result) => result.geometry,
        Err(e) => {
            return TierResult {
                tier: "layout",
                status: TierStatus::Fail(format!("roundtrip layout failed: {e}")),
            };
        }
    };

    let mismatches = compare_geometry(&direct_geom, &roundtrip_geom);

    TierResult {
        tier: "layout",
        status: if mismatches.is_empty() {
            TierStatus::Pass
        } else {
            TierStatus::Fail(mismatches.join("; "))
        },
    }
}

fn compare_geometry(direct: &GraphGeometry, roundtrip: &GraphGeometry) -> Vec<String> {
    let mut mismatches = Vec::new();

    // Compare node count
    if direct.nodes.len() != roundtrip.nodes.len() {
        mismatches.push(format!(
            "node count: {} vs {}",
            direct.nodes.len(),
            roundtrip.nodes.len()
        ));
        return mismatches;
    }

    // Compare node positions and sizes
    let mut direct_nodes: Vec<_> = direct.nodes.iter().collect();
    direct_nodes.sort_by_key(|(id, _)| (*id).clone());
    for (id, d_node) in &direct_nodes {
        match roundtrip.nodes.get(*id) {
            None => mismatches.push(format!("node {id} missing in roundtrip geometry")),
            Some(r_node) => {
                if !floats_eq(d_node.rect.x, r_node.rect.x)
                    || !floats_eq(d_node.rect.y, r_node.rect.y)
                {
                    mismatches.push(format!(
                        "node {id} position: ({:.1},{:.1}) vs ({:.1},{:.1})",
                        d_node.rect.x, d_node.rect.y, r_node.rect.x, r_node.rect.y
                    ));
                }
                if !floats_eq(d_node.rect.width, r_node.rect.width)
                    || !floats_eq(d_node.rect.height, r_node.rect.height)
                {
                    mismatches.push(format!(
                        "node {id} size: ({:.1}x{:.1}) vs ({:.1}x{:.1})",
                        d_node.rect.width,
                        d_node.rect.height,
                        r_node.rect.width,
                        r_node.rect.height
                    ));
                }
            }
        }
    }

    // Compare edge count
    if direct.edges.len() != roundtrip.edges.len() {
        mismatches.push(format!(
            "edge count: {} vs {}",
            direct.edges.len(),
            roundtrip.edges.len()
        ));
    }

    // Compare bounds
    if !floats_eq(direct.bounds.width, roundtrip.bounds.width)
        || !floats_eq(direct.bounds.height, roundtrip.bounds.height)
    {
        mismatches.push(format!(
            "bounds: ({:.1}x{:.1}) vs ({:.1}x{:.1})",
            direct.bounds.width,
            direct.bounds.height,
            roundtrip.bounds.width,
            roundtrip.bounds.height
        ));
    }

    mismatches
}

/// Run a full conformance case for a flowchart fixture.
fn run_flowchart_conformance(name: &str) -> ConformanceReport {
    let input = fixture_input("flowchart", name);

    // Direct path: parse → build → Diagram
    let direct_diagram = {
        let fc = mmdflux::parse_flowchart(&input).unwrap();
        mmdflux::build_diagram(&fc)
    };

    // MMDS roundtrip: parse → build → layout → JSON → hydrate → Diagram
    let roundtrip_diagram = {
        let mut instance = FlowchartInstance::new();
        instance.parse(&input).unwrap();
        let json = instance
            .render(OutputFormat::Json, &RenderConfig::default())
            .unwrap();
        from_mmds_str(&json).unwrap()
    };

    ConformanceReport {
        fixture_path: format!("flowchart/{name}"),
        semantic: check_semantic(&direct_diagram, &roundtrip_diagram),
        layout: check_layout(&direct_diagram, &roundtrip_diagram),
        visual: check_visual(&direct_diagram, &roundtrip_diagram),
    }
}

/// Run a full conformance case for a class diagram fixture.
fn run_class_conformance(name: &str) -> ConformanceReport {
    use mmdflux::diagrams::class::parser::parse_class_diagram;
    use mmdflux::diagrams::class::{ClassInstance, compiler};

    let input = fixture_input("class", name);

    // Direct path: parse → compile → Diagram
    let direct_diagram = {
        let model = parse_class_diagram(&input).unwrap();
        compiler::compile(&model)
    };

    // MMDS roundtrip: parse → compile → layout → JSON → hydrate → Diagram
    let roundtrip_diagram = {
        let mut instance = ClassInstance::new();
        instance.parse(&input).unwrap();
        let json = instance
            .render(OutputFormat::Json, &RenderConfig::default())
            .unwrap();
        from_mmds_str(&json).unwrap()
    };

    ConformanceReport {
        fixture_path: format!("class/{name}"),
        semantic: check_semantic(&direct_diagram, &roundtrip_diagram),
        layout: check_layout(&direct_diagram, &roundtrip_diagram),
        visual: check_visual(&direct_diagram, &roundtrip_diagram),
    }
}

// ---------------------------------------------------------------------------
// Conformance report assertions
// ---------------------------------------------------------------------------

#[test]
fn conformance_report_has_three_tiers() {
    let report = run_flowchart_conformance("simple.mmd");
    assert_eq!(report.tiers().len(), 3);
    assert_eq!(report.tiers()[0].tier, "semantic");
    assert_eq!(report.tiers()[1].tier, "layout");
    assert_eq!(report.tiers()[2].tier, "visual");
}

#[test]
fn conformance_report_contains_fixture_path() {
    let report = run_flowchart_conformance("simple.mmd");
    assert!(report.fixture_path.ends_with("simple.mmd"));
}

// ---------------------------------------------------------------------------
// Flowchart conformance: basic fixtures
// ---------------------------------------------------------------------------

#[test]
fn flowchart_simple_all_tiers_pass() {
    let report = run_flowchart_conformance("simple.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.layout.status.is_pass(),
        "layout: {:?}",
        report.layout.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_chain_all_tiers_pass() {
    let report = run_flowchart_conformance("chain.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_decision_all_tiers_pass() {
    let report = run_flowchart_conformance("decision.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_labeled_edges_all_tiers_pass() {
    let report = run_flowchart_conformance("labeled_edges.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_edge_styles_all_tiers_pass() {
    let report = run_flowchart_conformance("edge_styles.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Flowchart conformance: direction variants
// ---------------------------------------------------------------------------

#[test]
fn flowchart_left_right_all_tiers_pass() {
    let report = run_flowchart_conformance("left_right.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_right_left_all_tiers_pass() {
    let report = run_flowchart_conformance("right_left.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_bottom_top_all_tiers_pass() {
    let report = run_flowchart_conformance("bottom_top.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Flowchart conformance: subgraphs
// ---------------------------------------------------------------------------

#[test]
fn flowchart_simple_subgraph_all_tiers_pass() {
    let report = run_flowchart_conformance("simple_subgraph.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_subgraph_edges_all_tiers_pass() {
    let report = run_flowchart_conformance("subgraph_edges.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_nested_subgraph_all_tiers_pass() {
    let report = run_flowchart_conformance("nested_subgraph.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Flowchart conformance: cycles and backward edges
// ---------------------------------------------------------------------------

#[test]
fn flowchart_simple_cycle_all_tiers_pass() {
    let report = run_flowchart_conformance("simple_cycle.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Flowchart conformance: complex fixtures
// ---------------------------------------------------------------------------

#[test]
fn flowchart_complex_all_tiers_pass() {
    let report = run_flowchart_conformance("complex.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

#[test]
fn flowchart_shapes_all_tiers_pass() {
    let report = run_flowchart_conformance("shapes.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Class diagram conformance
// ---------------------------------------------------------------------------

#[test]
fn class_simple_all_tiers_pass() {
    let report = run_class_conformance("simple.mmd");
    assert!(
        report.semantic.status.is_pass(),
        "semantic: {:?}",
        report.semantic.status
    );
    assert!(
        report.visual.status.is_pass(),
        "visual: {:?}",
        report.visual.status
    );
}

// ---------------------------------------------------------------------------
// Fixture matrix: broad coverage across flowchart fixtures
// ---------------------------------------------------------------------------

/// Flowchart fixtures expected to pass all three conformance tiers.
const FLOWCHART_CONFORMANCE_MATRIX: &[&str] = &[
    "simple.mmd",
    "chain.mmd",
    "decision.mmd",
    "shapes.mmd",
    "edge_styles.mmd",
    "labeled_edges.mmd",
    "left_right.mmd",
    "right_left.mmd",
    "bottom_top.mmd",
    "fan_in.mmd",
    "fan_out.mmd",
    "simple_cycle.mmd",
    "multiple_cycles.mmd",
    "simple_subgraph.mmd",
    "subgraph_edges.mmd",
    "nested_subgraph.mmd",
    "multi_subgraph.mmd",
    "complex.mmd",
    "ampersand.mmd",
    "diamond_fan.mmd",
    "self_loop.mmd",
    "bidirectional.mmd",
    "cross_circle_arrows.mmd",
];

#[test]
fn flowchart_matrix_semantic_tier() {
    let mut failures = Vec::new();
    for fixture in FLOWCHART_CONFORMANCE_MATRIX {
        let report = run_flowchart_conformance(fixture);
        if !report.semantic.status.is_pass() {
            failures.push(format!("{}: {:?}", fixture, report.semantic.status));
        }
    }
    assert!(
        failures.is_empty(),
        "semantic tier failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn flowchart_matrix_layout_tier() {
    let mut failures = Vec::new();
    for fixture in FLOWCHART_CONFORMANCE_MATRIX {
        let report = run_flowchart_conformance(fixture);
        if !report.layout.status.is_pass() {
            failures.push(format!("{}: {:?}", fixture, report.layout.status));
        }
    }
    assert!(
        failures.is_empty(),
        "layout tier failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn flowchart_matrix_visual_tier() {
    let mut failures = Vec::new();
    for fixture in FLOWCHART_CONFORMANCE_MATRIX {
        let report = run_flowchart_conformance(fixture);
        if !report.visual.status.is_pass() {
            failures.push(format!("{}: {:?}", fixture, report.visual.status));
        }
    }
    assert!(
        failures.is_empty(),
        "visual tier failures:\n{}",
        failures.join("\n")
    );
}
