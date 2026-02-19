use std::fs;
use std::path::{Path, PathBuf};

use mmdflux::diagram::{
    CornerStyle, EngineAlgorithmId, InterpolationStyle, OutputFormat, PathDetail, RenderConfig,
    RoutingStyle,
};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::diagrams::mmds::from_mmds_str;
use mmdflux::registry::DiagramInstance;
use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{build_diagram, parse_flowchart};

const ORTHOGONAL_PARITY_FIXTURE_SUBSET: &[&str] = &[
    "simple.mmd",
    "chain.mmd",
    "simple_cycle.mmd",
    "decision.mmd",
    "fan_out.mmd",
    "left_right.mmd",
    "subgraph_direction_cross_boundary.mmd",
    "multi_subgraph_direction_override.mmd",
];

const ORTHOGONAL_PARITY_ACCEPTED_DELTAS: &[&str] = &[
    "simple.mmd",
    "chain.mmd",
    "simple_cycle.mmd",
    "decision.mmd",
    "fan_out.mmd",
    "left_right.mmd",
    "subgraph_direction_cross_boundary.mmd",
    "multi_subgraph_direction_override.mmd",
];

const ORTHOGONAL_PARITY_MUST_MATCH: &[&str] = &[];
const ORTHOGONAL_FEEDBACK_BASELINE_FILE: &str = "docs/orthogonal_feedback_baseline.tsv";
const ORTHOGONAL_PROMOTION_RECORD_FILE: &str = "docs/ORTHOGONAL_ROUTING_PROMOTION.md";
const ORTHOGONAL_FEEDBACK_BASELINE_COLUMNS: &[&str] = &[
    "fixture",
    "style",
    "status",
    "diff_lines",
    "full_viewbox_width",
    "full_viewbox_height",
    "orthogonal_viewbox_width",
    "orthogonal_viewbox_height",
    "viewbox_width_delta",
    "viewbox_height_delta",
    "full_route_envelope_width",
    "full_route_envelope_height",
    "orthogonal_route_envelope_width",
    "orthogonal_route_envelope_height",
    "route_envelope_width_delta",
    "route_envelope_height_delta",
    "full_edge_label_count",
    "orthogonal_edge_label_count",
    "edge_label_count_delta",
    "label_position_max_drift",
    "label_position_mean_drift",
];
const ORTHOGONAL_FEEDBACK_BASELINE_FIXTURES: &[&str] = &[
    "fan_in.mmd",
    "five_fan_in.mmd",
    "stacked_fan_in.mmd",
    "fan_in_lr.mmd",
    "labeled_edges.mmd",
    "inline_label_flowchart.mmd",
    "double_skip.mmd",
    "skip_edge_collision.mmd",
];

fn list_fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart");
    let mut fixtures: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("Failed to read fixtures dir: {e}"))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().is_some_and(|e| e == "mmd") {
                Some(path.file_name()?.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    fixtures.sort();
    fixtures
}

fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read fixture {name}: {e}"))
}

fn render_svg_fixture(name: &str) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    let mut options = RenderOptions::default_svg();
    options.path_detail = PathDetail::Full;
    render_svg(&diagram, &options)
}

fn render_svg_fixture_with_curve(
    name: &str,
    routing: RoutingStyle,
    interp: InterpolationStyle,
    corner: CornerStyle,
) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    let mut options = RenderOptions::default_svg();
    options.svg.routing_style = routing;
    options.svg.interpolation_style = interp;
    options.svg.corner_style = corner;
    options.path_detail = PathDetail::Full;
    render_svg(&diagram, &options)
}

fn render_svg_fixture_with_engine(name: &str, engine: &str) -> String {
    let input = load_fixture(name);
    let mut instance = FlowchartInstance::new();
    instance.parse(&input).expect("Failed to parse fixture");
    let config = RenderConfig {
        path_detail: PathDetail::Full,
        layout_engine: Some(EngineAlgorithmId::parse(engine).unwrap()),
        ..RenderConfig::default()
    };
    instance
        .render(OutputFormat::Svg, &config)
        .expect("Failed to render SVG fixture")
}

fn render_svg_fixture_full_vs_orthogonal(name: &str) -> (String, String) {
    (
        render_svg_fixture_with_engine(name, "mermaid-layered"),
        render_svg_fixture_with_engine(name, "flux-layered"),
    )
}

fn assert_orthogonal_parity_classification_is_complete() {
    let mut classified: Vec<&str> = ORTHOGONAL_PARITY_ACCEPTED_DELTAS
        .iter()
        .chain(ORTHOGONAL_PARITY_MUST_MATCH.iter())
        .copied()
        .collect();
    classified.sort_unstable();

    let mut fixture_subset = ORTHOGONAL_PARITY_FIXTURE_SUBSET.to_vec();
    fixture_subset.sort_unstable();

    assert_eq!(
        classified, fixture_subset,
        "fixture subset classification is incomplete or duplicated"
    );
}

fn parse_attr_f64(line: &str, attr: &str) -> Option<f64> {
    let marker = format!("{attr}=\"");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    rest[..end].parse::<f64>().ok()
}

fn subgraph_rect_ys(svg: &str) -> Vec<f64> {
    svg.lines()
        .filter(|line| line.contains("class=\"subgraph\""))
        .filter_map(|line| parse_attr_f64(line, "y"))
        .collect()
}

fn render_svg_mmds_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mmds")
        .join(name);
    let payload = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read MMDS fixture {}: {e}", path.display()));
    let diagram = from_mmds_str(&payload).expect("MMDS fixture should hydrate");
    let mut options = RenderOptions::default_svg();
    options.path_detail = PathDetail::Full;
    render_svg(&diagram, &options)
}

fn render_svg_positioned_mmds_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mmds")
        .join(name);
    let payload = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read MMDS fixture {}: {e}", path.display()));
    let mut instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    instance.parse(&payload).expect("MMDS fixture should parse");
    instance
        .render(
            OutputFormat::Svg,
            &RenderConfig {
                path_detail: PathDetail::Full,
                ..RenderConfig::default()
            },
        )
        .expect("positioned MMDS should render SVG")
}

fn assert_direct_vs_mmds_svg_parity(flowchart_fixture: &str, mmds_fixture: &str) {
    let direct_svg = render_svg_fixture(flowchart_fixture);
    let replay_svg = render_svg_mmds_fixture(mmds_fixture);
    assert_eq!(
        replay_svg, direct_svg,
        "MMDS replay diverged for flowchart fixture {flowchart_fixture} and MMDS fixture {mmds_fixture}"
    );
}

fn snapshot_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("svg-snapshots")
        .join("flowchart")
        .join(format!("{stem}.svg"))
}

fn orthogonal_snapshot_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("svg-snapshots")
        .join("flowchart-orthogonal")
        .join(format!("{stem}.svg"))
}

fn mmds_snapshot_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("svg-snapshots")
        .join("mmds")
        .join(format!("{stem}.svg"))
}

fn orthogonal_feedback_baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(ORTHOGONAL_FEEDBACK_BASELINE_FILE)
}

fn orthogonal_promotion_record_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(ORTHOGONAL_PROMOTION_RECORD_FILE)
}

fn parse_tsv_record(line: &str) -> Vec<&str> {
    line.split('\t').collect()
}

fn assert_snapshot(fixture: &str) {
    let stem = fixture.trim_end_matches(".mmd");
    let output = render_svg_fixture(fixture);
    let path = snapshot_path(stem);

    if std::env::var("GENERATE_SVG_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, &output).unwrap();
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Missing snapshot: {}", path.display()));
    assert_eq!(output, expected, "Snapshot mismatch for {fixture}");
}

fn assert_orthogonal_snapshot(fixture: &str) {
    let stem = fixture.trim_end_matches(".mmd");
    let output = render_svg_fixture_with_curve(
        fixture,
        RoutingStyle::Orthogonal,
        InterpolationStyle::Linear,
        CornerStyle::Rounded,
    );
    let path = orthogonal_snapshot_path(stem);

    if std::env::var("GENERATE_SVG_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, &output).unwrap();
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Missing snapshot: {}", path.display()));
    assert_eq!(
        output, expected,
        "Orthogonal snapshot mismatch for {fixture}"
    );
}

fn assert_mmds_snapshot(fixture: &str, snapshot_stem: &str) {
    let output = render_svg_positioned_mmds_fixture(fixture);
    let path = mmds_snapshot_path(snapshot_stem);

    if std::env::var("GENERATE_SVG_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, &output).unwrap();
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Missing snapshot: {}", path.display()));
    assert_eq!(output, expected, "Snapshot mismatch for {fixture}");
}

#[test]
fn svg_snapshot_all_fixtures() {
    for fixture in list_fixtures() {
        assert_snapshot(&fixture);
    }
}

#[test]
fn mmds_replay_without_endpoint_intent_diverges_on_subgraph_as_node_edge_fixture() {
    let direct_svg = render_svg_fixture("subgraph_as_node_edge.mmd");
    let replay_svg = render_svg_mmds_fixture("subgraph-endpoint-intent-missing.json");

    assert_ne!(replay_svg, direct_svg);
}

#[test]
fn mmds_replay_without_endpoint_intent_diverges_on_subgraph_to_subgraph_fixture() {
    let direct_svg = render_svg_fixture("subgraph_to_subgraph_edge.mmd");
    let replay_svg = render_svg_mmds_fixture("subgraph-endpoint-subgraph-to-subgraph-missing.json");

    assert_ne!(replay_svg, direct_svg);
}

#[test]
fn mmds_replay_with_endpoint_intent_matches_subgraph_as_node_fixture() {
    assert_direct_vs_mmds_svg_parity(
        "subgraph_as_node_edge.mmd",
        "subgraph-endpoint-intent-present.json",
    );
}

#[test]
fn mmds_replay_with_endpoint_intent_matches_subgraph_to_subgraph_fixture() {
    assert_direct_vs_mmds_svg_parity(
        "subgraph_to_subgraph_edge.mmd",
        "subgraph-endpoint-subgraph-to-subgraph-present.json",
    );
}

#[test]
fn direct_and_mmds_replay_match_for_subgraph_endpoint_fixture_set() {
    // `subgraph_as_node_edge` covers both subgraph-as-target and subgraph-as-source
    // endpoint-intent cases. `subgraph_to_subgraph_edge` covers subgraph-to-subgraph.
    for (flowchart_fixture, mmds_fixture) in [
        (
            "subgraph_as_node_edge.mmd",
            "subgraph-endpoint-intent-present.json",
        ),
        (
            "subgraph_to_subgraph_edge.mmd",
            "subgraph-endpoint-subgraph-to-subgraph-present.json",
        ),
    ] {
        assert_direct_vs_mmds_svg_parity(flowchart_fixture, mmds_fixture);
    }
}

#[test]
fn positioned_mmds_svg_snapshot_routed_basic() {
    assert_mmds_snapshot("positioned/routed-basic.json", "routed-basic");
}

#[test]
fn svg_snapshot_orthogonal_fixture_subset() {
    for fixture in [
        "simple.mmd",
        "fan_out.mmd",
        "subgraph_direction_cross_boundary.mmd",
    ] {
        assert_orthogonal_snapshot(fixture);
    }
}

#[test]
fn svg_orthogonal_route_parity_fixture_subset_matches_expected_classification() {
    assert_orthogonal_parity_classification_is_complete();

    let mut differing: Vec<&str> = Vec::new();

    for fixture in ORTHOGONAL_PARITY_FIXTURE_SUBSET {
        let (legacy, orthogonal) = render_svg_fixture_full_vs_orthogonal(fixture);
        if legacy != orthogonal {
            differing.push(fixture);
        }
    }

    assert_eq!(
        differing, ORTHOGONAL_PARITY_ACCEPTED_DELTAS,
        "accepted-delta set changed; reclassify fixture subset"
    );

    for fixture in ORTHOGONAL_PARITY_MUST_MATCH {
        let (legacy, orthogonal) = render_svg_fixture_full_vs_orthogonal(fixture);
        assert_eq!(
            orthogonal, legacy,
            "fixture {fixture} is classified as must-match but diverged"
        );
    }
}

#[test]
fn orthogonal_route_svg_output_is_deterministic_for_fixture_subset() {
    for fixture in ORTHOGONAL_PARITY_FIXTURE_SUBSET {
        let first = render_svg_fixture_with_engine(fixture, "flux-layered");
        let second = render_svg_fixture_with_engine(fixture, "flux-layered");
        assert_eq!(
            second, first,
            "orthogonal SVG output is nondeterministic for fixture {fixture}"
        );
    }
}

#[test]
fn svg_polyline_route_rollback_is_stable_across_repeated_renders() {
    for fixture in ["simple.mmd", "chain.mmd", "simple_cycle.mmd"] {
        let baseline = render_svg_fixture_with_engine(fixture, "mermaid-layered");
        let output = render_svg_fixture_with_engine(fixture, "mermaid-layered");
        assert_eq!(
            output, baseline,
            "polyline rollback should be stable for fixture {fixture}"
        );
    }
}

#[test]
fn svg_orthogonal_route_preserves_subgraph_vertical_order_on_multi_override_fixture() {
    let legacy =
        render_svg_fixture_with_engine("multi_subgraph_direction_override.mmd", "mermaid-layered");
    let orthogonal =
        render_svg_fixture_with_engine("multi_subgraph_direction_override.mmd", "flux-layered");

    let legacy_ys = subgraph_rect_ys(&legacy);
    let orthogonal_ys = subgraph_rect_ys(&orthogonal);
    assert!(
        legacy_ys.len() >= 2 && orthogonal_ys.len() >= 2,
        "expected at least two subgraph rects in fixture output"
    );
    // Top subgraph A should remain above bottom subgraph G.
    assert!(
        orthogonal_ys[1] > orthogonal_ys[0],
        "orthogonal routing collapsed subgraph ordering: ys={:?}",
        orthogonal_ys
    );
    // Orthogonal routing should keep the same top-to-bottom ordering as legacy.
    assert!(
        legacy_ys[1] > legacy_ys[0],
        "legacy output unexpectedly lacks top-to-bottom subgraph ordering: ys={:?}",
        legacy_ys
    );
}

#[test]
fn orthogonal_feedback_baseline_contains_required_fixtures_and_metrics() {
    let baseline_path = orthogonal_feedback_baseline_path();
    let raw = fs::read_to_string(&baseline_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read orthogonal feedback baseline {}: {e}",
            baseline_path.display()
        )
    });

    let mut lines = raw.lines();
    let header_line = lines
        .next()
        .expect("baseline file must include a header row");
    let header_columns = parse_tsv_record(header_line);

    for required in ORTHOGONAL_FEEDBACK_BASELINE_COLUMNS {
        assert!(
            header_columns.contains(required),
            "baseline is missing required column: {required}"
        );
    }

    let fixture_column_index = header_columns
        .iter()
        .position(|column| *column == "fixture")
        .expect("baseline header must include fixture column");

    let mut baseline_fixtures: Vec<&str> = lines
        .filter(|line| !line.trim().is_empty())
        .map(parse_tsv_record)
        .filter_map(|row| row.get(fixture_column_index).copied())
        .collect();
    baseline_fixtures.sort_unstable();
    baseline_fixtures.dedup();

    for fixture in ORTHOGONAL_FEEDBACK_BASELINE_FIXTURES {
        assert!(
            baseline_fixtures.binary_search(fixture).is_ok(),
            "baseline is missing required fixture row: {fixture}"
        );
    }
}

#[test]
fn non_viewbox_metrics_include_route_envelope_and_label_drift() {
    let baseline_path = orthogonal_feedback_baseline_path();
    let raw = fs::read_to_string(&baseline_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read orthogonal feedback baseline {}: {e}",
            baseline_path.display()
        )
    });

    let mut lines = raw.lines();
    let header_line = lines
        .next()
        .expect("baseline file must include a header row");
    let header_columns = parse_tsv_record(header_line);

    let route_width_delta_index = header_columns
        .iter()
        .position(|column| *column == "route_envelope_width_delta")
        .expect("baseline header must include route_envelope_width_delta");
    let route_height_delta_index = header_columns
        .iter()
        .position(|column| *column == "route_envelope_height_delta")
        .expect("baseline header must include route_envelope_height_delta");
    let label_max_drift_index = header_columns
        .iter()
        .position(|column| *column == "label_position_max_drift")
        .expect("baseline header must include label_position_max_drift");
    let label_mean_drift_index = header_columns
        .iter()
        .position(|column| *column == "label_position_mean_drift")
        .expect("baseline header must include label_position_mean_drift");

    let mut has_non_viewbox_signal = false;
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let row = parse_tsv_record(line);
        let route_width_delta = row
            .get(route_width_delta_index)
            .expect("row must include route_envelope_width_delta")
            .parse::<f64>()
            .expect("route_envelope_width_delta should parse as f64");
        let route_height_delta = row
            .get(route_height_delta_index)
            .expect("row must include route_envelope_height_delta")
            .parse::<f64>()
            .expect("route_envelope_height_delta should parse as f64");
        let label_max_drift = row
            .get(label_max_drift_index)
            .expect("row must include label_position_max_drift")
            .parse::<f64>()
            .expect("label_position_max_drift should parse as f64");
        let label_mean_drift = row
            .get(label_mean_drift_index)
            .expect("row must include label_position_mean_drift")
            .parse::<f64>()
            .expect("label_position_mean_drift should parse as f64");

        if route_width_delta.abs() > 0.01
            || route_height_delta.abs() > 0.01
            || label_max_drift > 0.01
            || label_mean_drift > 0.01
        {
            has_non_viewbox_signal = true;
        }
    }

    assert!(
        has_non_viewbox_signal,
        "baseline must include at least one non-viewBox route/label signal"
    );
}

#[test]
fn promotion_record_has_rollback_validation() {
    let record_path = orthogonal_promotion_record_path();
    let raw = fs::read_to_string(&record_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read promotion record {}: {e}",
            record_path.display()
        )
    });

    let required_markers = [
        "### Rollback Playbook (Task 5.1)",
        "--edge-routing full-compute",
        "./scripts/tests/07-plan-0076-orthogonal-routing-quality-qa.sh",
    ];

    for marker in required_markers {
        assert!(
            raw.contains(marker),
            "promotion record is missing required marker: {marker}"
        );
    }
}
