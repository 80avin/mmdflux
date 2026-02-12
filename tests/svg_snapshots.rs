use std::fs;
use std::path::{Path, PathBuf};

use mmdflux::diagram::{
    OutputFormat, PathDetail, RenderConfig, RoutingMode, RoutingPolicyToggles, SvgEdgePathStyle,
};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::diagrams::mmds::from_mmds_str;
use mmdflux::registry::DiagramInstance;
use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{build_diagram, parse_flowchart};

const UNIFIED_PARITY_FIXTURE_SUBSET: &[&str] = &[
    "simple.mmd",
    "chain.mmd",
    "simple_cycle.mmd",
    "decision.mmd",
    "fan_out.mmd",
    "left_right.mmd",
    "subgraph_direction_cross_boundary.mmd",
    "multi_subgraph_direction_override.mmd",
];

const UNIFIED_PARITY_ACCEPTED_DELTAS: &[&str] = &[
    "simple.mmd",
    "chain.mmd",
    "simple_cycle.mmd",
    "decision.mmd",
    "fan_out.mmd",
    "left_right.mmd",
    "subgraph_direction_cross_boundary.mmd",
    "multi_subgraph_direction_override.mmd",
];

const UNIFIED_PARITY_MUST_MATCH: &[&str] = &[];
const Q1_Q2_TOGGLE_FIXTURE_SUBSET: &[&str] = &[
    "stacked_fan_in.mmd",
    "fan_in.mmd",
    "five_fan_in.mmd",
    "multiple_cycles.mmd",
    "http_request.mmd",
    "git_workflow.mmd",
];
const Q1_Q2_TOGGLE_MUST_DIFF: &[&str] = &["fan_in.mmd", "five_fan_in.mmd", "http_request.mmd"];
const Q1_Q2_TOGGLE_MUST_MATCH: &[&str] = &[
    "stacked_fan_in.mmd",
    "multiple_cycles.mmd",
    "git_workflow.mmd",
];
const Q4_RANK_SPAN_TOGGLE_FIXTURE_SUBSET: &[&str] = &[
    "double_skip.mmd",
    "skip_edge_collision.mmd",
    "inline_label_flowchart.mmd",
];
const Q4_RANK_SPAN_TOGGLE_MUST_DIFF: &[&str] = &[
    "double_skip.mmd",
    "skip_edge_collision.mmd",
    "inline_label_flowchart.mmd",
];
const Q4_RANK_SPAN_TOGGLE_MUST_MATCH: &[&str] = &[];
const Q4_ROUTE_ENVELOPE_ABS_DELTA_GATE_PX: f64 = 24.0;
const Q4_LABEL_POSITION_MAX_DRIFT_GATE_PX: f64 = 40.0;
const Q4_LABEL_POSITION_MEAN_DRIFT_GATE_PX: f64 = 20.0;
const UNIFIED_FEEDBACK_BASELINE_FILE: &str = "docs/unified_feedback_baseline.tsv";
const UNIFIED_FEEDBACK_BASELINE_COLUMNS: &[&str] = &[
    "fixture",
    "style",
    "status",
    "diff_lines",
    "full_viewbox_width",
    "full_viewbox_height",
    "unified_viewbox_width",
    "unified_viewbox_height",
    "viewbox_width_delta",
    "viewbox_height_delta",
    "full_route_envelope_width",
    "full_route_envelope_height",
    "unified_route_envelope_width",
    "unified_route_envelope_height",
    "route_envelope_width_delta",
    "route_envelope_height_delta",
    "full_edge_label_count",
    "unified_edge_label_count",
    "edge_label_count_delta",
    "label_position_max_drift",
    "label_position_mean_drift",
];
const UNIFIED_FEEDBACK_BASELINE_FIXTURES: &[&str] = &[
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

fn render_svg_fixture_with_curve(name: &str, curve: SvgEdgePathStyle) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = curve;
    options.path_detail = PathDetail::Full;
    render_svg(&diagram, &options)
}

fn render_svg_fixture_with_routing_mode(name: &str, mode: RoutingMode) -> String {
    render_svg_fixture_with_routing_mode_and_policies(
        name,
        mode,
        RoutingPolicyToggles::all_enabled(),
    )
}

fn render_svg_fixture_with_routing_mode_and_policies(
    name: &str,
    mode: RoutingMode,
    policies: RoutingPolicyToggles,
) -> String {
    let input = load_fixture(name);
    let mut instance = FlowchartInstance::new();
    instance.parse(&input).expect("Failed to parse fixture");
    let config = RenderConfig {
        svg_edge_path_style: Some(SvgEdgePathStyle::Linear),
        path_detail: PathDetail::Full,
        routing_mode: Some(mode),
        routing_policies: policies,
        ..RenderConfig::default()
    };
    instance
        .render(OutputFormat::Svg, &config)
        .expect("Failed to render SVG fixture")
}

fn render_svg_fixture_full_vs_unified(name: &str) -> (String, String) {
    (
        render_svg_fixture_with_routing_mode(name, RoutingMode::FullCompute),
        render_svg_fixture_with_routing_mode(name, RoutingMode::UnifiedPreview),
    )
}

fn assert_unified_parity_classification_is_complete() {
    let mut classified: Vec<&str> = UNIFIED_PARITY_ACCEPTED_DELTAS
        .iter()
        .chain(UNIFIED_PARITY_MUST_MATCH.iter())
        .copied()
        .collect();
    classified.sort_unstable();

    let mut fixture_subset = UNIFIED_PARITY_FIXTURE_SUBSET.to_vec();
    fixture_subset.sort_unstable();

    assert_eq!(
        classified, fixture_subset,
        "fixture subset classification is incomplete or duplicated"
    );
}

fn assert_q1_q2_toggle_classification_is_complete() {
    let mut classified: Vec<&str> = Q1_Q2_TOGGLE_MUST_DIFF
        .iter()
        .chain(Q1_Q2_TOGGLE_MUST_MATCH.iter())
        .copied()
        .collect();
    classified.sort_unstable();

    let mut fixture_subset = Q1_Q2_TOGGLE_FIXTURE_SUBSET.to_vec();
    fixture_subset.sort_unstable();

    assert_eq!(
        classified, fixture_subset,
        "Q1/Q2 toggle classification is incomplete or duplicated"
    );
}

fn assert_q4_rank_span_toggle_classification_is_complete() {
    let mut classified: Vec<&str> = Q4_RANK_SPAN_TOGGLE_MUST_DIFF
        .iter()
        .chain(Q4_RANK_SPAN_TOGGLE_MUST_MATCH.iter())
        .copied()
        .collect();
    classified.sort_unstable();

    let mut fixture_subset = Q4_RANK_SPAN_TOGGLE_FIXTURE_SUBSET.to_vec();
    fixture_subset.sort_unstable();

    assert_eq!(
        classified, fixture_subset,
        "Q4 rank-span toggle classification is incomplete or duplicated"
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

fn edge_path_data(svg: &str) -> Vec<String> {
    svg.lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("<path d=\"")
                && (line.contains("marker-end=") || line.contains("marker-start="))
        })
        .filter_map(|line| {
            let start = line.find("d=\"")?;
            let after = &line[start + 3..];
            let end = after.find('"')?;
            Some(after[..end].to_string())
        })
        .collect()
}

fn parse_svg_path_points(path_data: &str) -> Vec<(f64, f64)> {
    path_data
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_start_matches(|c: char| c.is_ascii_alphabetic());
            let (x, y) = token.split_once(',')?;
            let x = x.parse::<f64>().ok()?;
            let y = y.parse::<f64>().ok()?;
            Some((x, y))
        })
        .collect()
}

fn route_envelope_dims(svg: &str) -> (f64, f64) {
    let mut points: Vec<(f64, f64)> = Vec::new();
    for path in edge_path_data(svg) {
        points.extend(parse_svg_path_points(&path));
    }
    if points.is_empty() {
        return (0.0, 0.0);
    }

    let min_x = points.iter().map(|point| point.0).fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|point| point.1).fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);

    (max_x - min_x, max_y - min_y)
}

fn extract_edge_label_positions(svg: &str) -> Vec<(f64, f64)> {
    let mut in_edge_labels = false;
    let mut labels = Vec::new();
    for line in svg.lines() {
        if line.contains("<g class=\"edgeLabels\">") {
            in_edge_labels = true;
            continue;
        }
        if in_edge_labels && line.contains("</g>") {
            in_edge_labels = false;
            continue;
        }
        if !in_edge_labels || !line.contains("<text ") {
            continue;
        }
        if let (Some(x), Some(y)) = (parse_attr_f64(line, "x"), parse_attr_f64(line, "y")) {
            labels.push((x, y));
        }
    }
    labels
}

fn label_drift_stats(reference: &[(f64, f64)], candidate: &[(f64, f64)]) -> (f64, f64) {
    let paired = reference.len().min(candidate.len());
    if paired == 0 {
        return (0.0, 0.0);
    }

    let mut max_drift: f64 = 0.0;
    let mut drift_sum: f64 = 0.0;
    for idx in 0..paired {
        let dx = reference[idx].0 - candidate[idx].0;
        let dy = reference[idx].1 - candidate[idx].1;
        let drift = (dx * dx + dy * dy).sqrt();
        max_drift = max_drift.max(drift);
        drift_sum += drift;
    }

    (max_drift, drift_sum / paired as f64)
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

fn unified_feedback_baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(UNIFIED_FEEDBACK_BASELINE_FILE)
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
    let output = render_svg_fixture_with_curve(fixture, SvgEdgePathStyle::Orthogonal);
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
fn svg_unified_preview_parity_fixture_subset_matches_expected_classification() {
    assert_unified_parity_classification_is_complete();

    let mut differing: Vec<&str> = Vec::new();

    for fixture in UNIFIED_PARITY_FIXTURE_SUBSET {
        let (legacy, unified) = render_svg_fixture_full_vs_unified(fixture);
        if legacy != unified {
            differing.push(fixture);
        }
    }

    assert_eq!(
        differing, UNIFIED_PARITY_ACCEPTED_DELTAS,
        "accepted-delta set changed; reclassify fixture subset"
    );

    for fixture in UNIFIED_PARITY_MUST_MATCH {
        let (legacy, unified) = render_svg_fixture_full_vs_unified(fixture);
        assert_eq!(
            unified, legacy,
            "fixture {fixture} is classified as must-match but diverged"
        );
    }
}

#[test]
fn unified_preview_svg_output_is_deterministic_for_fixture_subset() {
    for fixture in UNIFIED_PARITY_FIXTURE_SUBSET {
        let first = render_svg_fixture_with_routing_mode(fixture, RoutingMode::UnifiedPreview);
        let second = render_svg_fixture_with_routing_mode(fixture, RoutingMode::UnifiedPreview);
        assert_eq!(
            second, first,
            "unified-preview SVG output is nondeterministic for fixture {fixture}"
        );
    }
}

#[test]
fn q1_q2_outputs_are_deterministic_under_toggle_matrix() {
    for fixture in Q1_Q2_TOGGLE_FIXTURE_SUBSET {
        for q1_enabled in [true, false] {
            let policies = RoutingPolicyToggles {
                q1_overflow: q1_enabled,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            };
            let first = render_svg_fixture_with_routing_mode_and_policies(
                fixture,
                RoutingMode::UnifiedPreview,
                policies,
            );
            let second = render_svg_fixture_with_routing_mode_and_policies(
                fixture,
                RoutingMode::UnifiedPreview,
                policies,
            );
            assert_eq!(
                second, first,
                "Q1/Q2 toggle matrix replay is nondeterministic for fixture {fixture} (q1={q1_enabled})"
            );
        }
    }
}

#[test]
fn q1_q2_toggle_matrix_fixture_subset_matches_expected_classification() {
    assert_q1_q2_toggle_classification_is_complete();

    let mut differing: Vec<&str> = Vec::new();
    for fixture in Q1_Q2_TOGGLE_FIXTURE_SUBSET {
        let on = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: true,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let off = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: false,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        if on != off {
            differing.push(fixture);
        }
    }

    assert_eq!(
        differing, Q1_Q2_TOGGLE_MUST_DIFF,
        "Q1/Q2 toggle expected-delta set changed; reclassify fixture subset"
    );

    for fixture in Q1_Q2_TOGGLE_MUST_MATCH {
        let on = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: true,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let off = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: false,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        assert_eq!(
            on, off,
            "fixture {fixture} is classified as Q1 toggle must-match but diverged"
        );
    }
}

#[test]
fn q4_rank_span_outputs_are_deterministic_under_toggle_matrix() {
    for fixture in Q4_RANK_SPAN_TOGGLE_FIXTURE_SUBSET {
        for q4_enabled in [true, false] {
            let policies = RoutingPolicyToggles {
                q4_rank_span_periphery: q4_enabled,
                ..RoutingPolicyToggles::all_enabled()
            };
            let first = render_svg_fixture_with_routing_mode_and_policies(
                fixture,
                RoutingMode::UnifiedPreview,
                policies,
            );
            let second = render_svg_fixture_with_routing_mode_and_policies(
                fixture,
                RoutingMode::UnifiedPreview,
                policies,
            );
            assert_eq!(
                second, first,
                "Q4 rank-span toggle replay is nondeterministic for fixture {fixture} (q4={q4_enabled})"
            );
        }
    }
}

#[test]
fn q4_rank_span_toggle_fixture_subset_matches_expected_classification() {
    assert_q4_rank_span_toggle_classification_is_complete();

    let mut differing: Vec<&str> = Vec::new();
    for fixture in Q4_RANK_SPAN_TOGGLE_FIXTURE_SUBSET {
        let on = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: true,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let off = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        if on != off {
            differing.push(fixture);
        }
    }

    assert_eq!(
        differing, Q4_RANK_SPAN_TOGGLE_MUST_DIFF,
        "Q4 rank-span toggle expected-delta set changed; reclassify fixture subset"
    );

    for fixture in Q4_RANK_SPAN_TOGGLE_MUST_MATCH {
        let on = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: true,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let off = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        assert_eq!(
            on, off,
            "fixture {fixture} is classified as Q4 toggle must-match but diverged"
        );
    }
}

#[test]
fn q4_rank_span_policy_respects_q6_metric_gates_for_fixture_subset() {
    let mut max_route_abs_delta: f64 = 0.0;
    let mut max_label_max_drift: f64 = 0.0;
    let mut max_label_mean_drift: f64 = 0.0;

    for fixture in Q4_RANK_SPAN_TOGGLE_FIXTURE_SUBSET {
        let q4_off = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let q4_on = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: true,
                ..RoutingPolicyToggles::all_enabled()
            },
        );

        let (off_width, off_height) = route_envelope_dims(&q4_off);
        let (on_width, on_height) = route_envelope_dims(&q4_on);
        let route_abs_delta = (on_width - off_width).abs().max((on_height - off_height).abs());
        max_route_abs_delta = max_route_abs_delta.max(route_abs_delta);

        let off_labels = extract_edge_label_positions(&q4_off);
        let on_labels = extract_edge_label_positions(&q4_on);
        let (label_max_drift, label_mean_drift) = label_drift_stats(&off_labels, &on_labels);
        max_label_max_drift = max_label_max_drift.max(label_max_drift);
        max_label_mean_drift = max_label_mean_drift.max(label_mean_drift);
    }

    assert!(
        max_route_abs_delta <= Q4_ROUTE_ENVELOPE_ABS_DELTA_GATE_PX,
        "Q4 route-envelope delta exceeded gate: max_route_abs_delta={max_route_abs_delta}, gate={Q4_ROUTE_ENVELOPE_ABS_DELTA_GATE_PX}"
    );
    assert!(
        max_label_max_drift <= Q4_LABEL_POSITION_MAX_DRIFT_GATE_PX,
        "Q4 label max drift exceeded gate: max_label_max_drift={max_label_max_drift}, gate={Q4_LABEL_POSITION_MAX_DRIFT_GATE_PX}"
    );
    assert!(
        max_label_mean_drift <= Q4_LABEL_POSITION_MEAN_DRIFT_GATE_PX,
        "Q4 label mean drift exceeded gate: max_label_mean_drift={max_label_mean_drift}, gate={Q4_LABEL_POSITION_MEAN_DRIFT_GATE_PX}"
    );
}

#[test]
fn svg_full_compute_override_matches_legacy_linear_core_subset() {
    for fixture in ["simple.mmd", "chain.mmd", "simple_cycle.mmd"] {
        let legacy = render_svg_fixture_with_curve(fixture, SvgEdgePathStyle::Linear);
        let full_compute = render_svg_fixture_with_routing_mode(fixture, RoutingMode::FullCompute);
        assert_eq!(
            full_compute, legacy,
            "full-compute override should preserve legacy SVG for fixture {fixture}"
        );
    }
}

#[test]
fn svg_full_compute_rollback_is_stable_across_policy_toggle_matrix() {
    let policy_matrix = [
        RoutingPolicyToggles::all_enabled(),
        RoutingPolicyToggles {
            q1_overflow: false,
            ..RoutingPolicyToggles::all_enabled()
        },
        RoutingPolicyToggles {
            q3_label_revalidation: false,
            ..RoutingPolicyToggles::all_enabled()
        },
        RoutingPolicyToggles {
            q4_rank_span_periphery: false,
            ..RoutingPolicyToggles::all_enabled()
        },
        RoutingPolicyToggles {
            q5_style_min_segment: false,
            ..RoutingPolicyToggles::all_enabled()
        },
    ];

    for fixture in ["simple.mmd", "chain.mmd", "simple_cycle.mmd"] {
        let baseline = render_svg_fixture_with_routing_mode_and_policies(
            fixture,
            RoutingMode::FullCompute,
            RoutingPolicyToggles::all_enabled(),
        );
        for policies in policy_matrix {
            let output = render_svg_fixture_with_routing_mode_and_policies(
                fixture,
                RoutingMode::FullCompute,
                policies,
            );
            assert_eq!(
                output, baseline,
                "full-compute rollback should be stable for {fixture} under policy set {:?}",
                policies
            );
        }
    }
}

#[test]
fn svg_unified_preview_preserves_subgraph_vertical_order_on_multi_override_fixture() {
    let legacy = render_svg_fixture_with_routing_mode(
        "multi_subgraph_direction_override.mmd",
        RoutingMode::FullCompute,
    );
    let unified = render_svg_fixture_with_routing_mode(
        "multi_subgraph_direction_override.mmd",
        RoutingMode::UnifiedPreview,
    );

    let legacy_ys = subgraph_rect_ys(&legacy);
    let unified_ys = subgraph_rect_ys(&unified);
    assert!(
        legacy_ys.len() >= 2 && unified_ys.len() >= 2,
        "expected at least two subgraph rects in fixture output"
    );
    // Top subgraph A should remain above bottom subgraph G.
    assert!(
        unified_ys[1] > unified_ys[0],
        "unified preview collapsed subgraph ordering: ys={:?}",
        unified_ys
    );
    // Unified preview should keep the same top-to-bottom ordering as legacy.
    assert!(
        legacy_ys[1] > legacy_ys[0],
        "legacy output unexpectedly lacks top-to-bottom subgraph ordering: ys={:?}",
        legacy_ys
    );
}

#[test]
fn unified_feedback_baseline_contains_required_fixtures_and_metrics() {
    let baseline_path = unified_feedback_baseline_path();
    let raw = fs::read_to_string(&baseline_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read unified feedback baseline {}: {e}",
            baseline_path.display()
        )
    });

    let mut lines = raw.lines();
    let header_line = lines
        .next()
        .expect("baseline file must include a header row");
    let header_columns = parse_tsv_record(header_line);

    for required in UNIFIED_FEEDBACK_BASELINE_COLUMNS {
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

    for fixture in UNIFIED_FEEDBACK_BASELINE_FIXTURES {
        assert!(
            baseline_fixtures.binary_search(fixture).is_ok(),
            "baseline is missing required fixture row: {fixture}"
        );
    }
}

#[test]
fn q6_metrics_include_route_envelope_and_label_drift() {
    let baseline_path = unified_feedback_baseline_path();
    let raw = fs::read_to_string(&baseline_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read unified feedback baseline {}: {e}",
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
        "Q6 baseline must include at least one non-viewBox route/label signal"
    );
}
