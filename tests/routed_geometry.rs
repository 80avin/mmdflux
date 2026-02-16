//! Contract tests for the routed geometry pipeline.
//!
//! Verifies that `route_graph_geometry` produces correct `RoutedGraphGeometry`
//! from engine-produced `GraphGeometry`.

use std::fs;
use std::path::Path;

use mmdflux::diagram::RoutingPolicyToggles;
use mmdflux::diagrams::flowchart::engine::{DagreLayoutEngine, MeasurementMode};
use mmdflux::diagrams::flowchart::geometry::*;
use mmdflux::diagrams::flowchart::routing::{
    route_graph_geometry, route_graph_geometry_with_policies, snap_path_to_grid_preview,
};
use mmdflux::{
    EngineConfig, GraphLayoutEngine, OutputFormat, RenderConfig, RoutingMode, build_diagram,
    parse_flowchart,
};

/// Parse input and produce (Diagram, GraphGeometry) via the dagre engine.
fn layout_test(input: &str) -> (mmdflux::Diagram, GraphGeometry) {
    let fc = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&fc);
    let engine = DagreLayoutEngine::text();
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine.layout(&diagram, &config).unwrap();
    (diagram, geom)
}

fn layout_fixture(name: &str) -> (mmdflux::Diagram, GraphGeometry) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join(name);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    layout_test(&input)
}

fn layout_test_svg(input: &str) -> (mmdflux::Diagram, GraphGeometry) {
    let fc = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&fc);
    let mode = MeasurementMode::for_format(OutputFormat::Svg, &RenderConfig::default());
    let engine = DagreLayoutEngine::with_mode(mode);
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine.layout(&diagram, &config).unwrap();
    (diagram, geom)
}

fn layout_fixture_svg(name: &str) -> (mmdflux::Diagram, GraphGeometry) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join(name);
    let input = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    layout_test_svg(&input)
}

const ROUTE_EPS: f64 = 0.000_001;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= ROUTE_EPS
}

fn segment_is_axis_aligned(a: FPoint, b: FPoint) -> bool {
    approx_eq(a.x, b.x) || approx_eq(a.y, b.y)
}

fn segment_is_non_degenerate(a: FPoint, b: FPoint) -> bool {
    !approx_eq(a.x, b.x) || !approx_eq(a.y, b.y)
}

fn points_approx_equal(a: FPoint, b: FPoint) -> bool {
    approx_eq(a.x, b.x) && approx_eq(a.y, b.y)
}

fn bend_count(path: &[FPoint]) -> usize {
    path.len().saturating_sub(2)
}

fn point_distance(a: FPoint, b: FPoint) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn distance_point_to_segment(point: FPoint, a: FPoint, b: FPoint) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let seg_len_sq = dx * dx + dy * dy;
    if seg_len_sq <= ROUTE_EPS {
        return point_distance(point, a);
    }

    let projection = ((point.x - a.x) * dx + (point.y - a.y) * dy) / seg_len_sq;
    let t = projection.clamp(0.0, 1.0);
    let closest = FPoint::new(a.x + t * dx, a.y + t * dy);
    point_distance(point, closest)
}

fn distance_point_to_path(point: FPoint, path: &[FPoint]) -> f64 {
    if path.is_empty() {
        return f64::INFINITY;
    }
    if path.len() == 1 {
        return point_distance(point, path[0]);
    }
    path.windows(2)
        .map(|segment| distance_point_to_segment(point, segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

#[derive(Debug)]
struct Q5RoutedStyleMonitorReport {
    scanned_styled_edges: usize,
    violations: Vec<String>,
    summary_line: String,
}

fn min_segment_len(path: &[FPoint]) -> f64 {
    path.windows(2)
        .map(|segment| point_distance(segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn q5_style_segment_monitor_report_for_routed_geometry(
    fixtures: &[&str],
    min_segment_threshold: f64,
) -> Q5RoutedStyleMonitorReport {
    use mmdflux::graph::Stroke;

    let mut scanned_styled_edges = 0usize;
    let mut violations = Vec::new();

    for fixture in fixtures {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let routed = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q5_style_min_segment: true,
                ..RoutingPolicyToggles::all_enabled()
            },
        );

        for edge in diagram
            .edges
            .iter()
            .filter(|edge| matches!(edge.stroke, Stroke::Dotted | Stroke::Thick))
        {
            let routed_edge = routed
                .edges
                .iter()
                .find(|candidate| candidate.index == edge.index)
                .unwrap_or_else(|| {
                    panic!(
                        "fixture {fixture} should route styled edge index {}",
                        edge.index
                    )
                });
            let min_segment = min_segment_len(&routed_edge.path);
            scanned_styled_edges += 1;
            if min_segment < min_segment_threshold {
                violations.push(format!(
                    "{fixture} {}->{} stroke={:?} min_segment={min_segment:.2} threshold={min_segment_threshold:.2}",
                    edge.from, edge.to, edge.stroke
                ));
            }
        }
    }

    Q5RoutedStyleMonitorReport {
        scanned_styled_edges,
        summary_line: format!(
            "q5_monitor_routed scanned={} violations={} threshold={:.2}",
            scanned_styled_edges,
            violations.len(),
            min_segment_threshold
        ),
        violations,
    }
}

fn labeled_edge_label_drift_failures(
    diagram: &mmdflux::Diagram,
    routed: &RoutedGraphGeometry,
    max_distance: f64,
) -> Vec<String> {
    let mut failures = Vec::new();
    for edge in diagram.edges.iter().filter(|edge| edge.label.is_some()) {
        let routed_edge = routed
            .edges
            .iter()
            .find(|candidate| candidate.index == edge.index)
            .unwrap_or_else(|| panic!("missing routed edge for index {}", edge.index));
        let Some(label_position) = routed_edge.label_position else {
            failures.push(format!(
                "{} -> {} (index {}) has edge label but no routed label_position",
                edge.from, edge.to, edge.index
            ));
            continue;
        };
        let drift = distance_point_to_path(label_position, &routed_edge.path);
        if drift > max_distance {
            failures.push(format!(
                "{} -> {} label {:?} drift={drift:.2} exceeds {max_distance:.2}; label_position={label_position:?}, path={:?}",
                edge.from,
                edge.to,
                edge.label,
                routed_edge.path
            ));
        }
    }
    failures
}

fn point_inside_rect(rect: FRect, point: FPoint) -> bool {
    let eps = 0.01;
    point.x > rect.x + eps
        && point.x < rect.x + rect.width - eps
        && point.y > rect.y + eps
        && point.y < rect.y + rect.height - eps
}

fn point_on_target_face(rect: FRect, point: FPoint) -> &'static str {
    let eps = 0.5;
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    let on_right = (point.x - right).abs() <= eps;
    let on_left = (point.x - left).abs() <= eps;
    let on_top = (point.y - top).abs() <= eps;
    let on_bottom = (point.y - bottom).abs() <= eps;

    if on_right && point.y > top + eps && point.y < bottom - eps {
        "right"
    } else if on_left && point.y > top + eps && point.y < bottom - eps {
        "left"
    } else if on_top && point.x > left + eps && point.x < right - eps {
        "top"
    } else if on_bottom && point.x > left + eps && point.x < right - eps {
        "bottom"
    } else if on_right {
        "right"
    } else if on_left {
        "left"
    } else {
        "interior_or_corner"
    }
}

fn terminal_support_is_normal_to_attached_rect_face(
    rect: FRect,
    prev: FPoint,
    end: FPoint,
) -> bool {
    let eps = 0.01;
    let on_top = (end.y - rect.y).abs() <= eps;
    let on_bottom = (end.y - (rect.y + rect.height)).abs() <= eps;
    let on_left = (end.x - rect.x).abs() <= eps;
    let on_right = (end.x - (rect.x + rect.width)).abs() <= eps;

    let vertical_segment = (prev.x - end.x).abs() <= eps && (prev.y - end.y).abs() > eps;
    let horizontal_segment = (prev.y - end.y).abs() <= eps && (prev.x - end.x).abs() > eps;

    (on_top || on_bottom) && vertical_segment || (on_left || on_right) && horizontal_segment
}

fn source_support_is_normal_to_attached_rect_face(
    rect: FRect,
    start: FPoint,
    next: FPoint,
) -> bool {
    let eps = 0.01;
    let on_top = (start.y - rect.y).abs() <= eps;
    let on_bottom = (start.y - (rect.y + rect.height)).abs() <= eps;
    let on_left = (start.x - rect.x).abs() <= eps;
    let on_right = (start.x - (rect.x + rect.width)).abs() <= eps;

    let vertical_segment = (start.x - next.x).abs() <= eps && (start.y - next.y).abs() > eps;
    let horizontal_segment = (start.y - next.y).abs() <= eps && (start.x - next.x).abs() > eps;

    (on_top || on_bottom) && vertical_segment || (on_left || on_right) && horizontal_segment
}

fn path_has_source_turnback_spike(path: &[FPoint]) -> bool {
    if path.len() < 4 {
        return false;
    }

    let p0 = path[0];
    let p1 = path[1];
    let p2 = path[2];

    points_approx_equal(p0, p2)
        && segment_is_axis_aligned(p0, p1)
        && segment_is_axis_aligned(p1, p2)
        && segment_is_non_degenerate(p0, p1)
        && segment_is_non_degenerate(p1, p2)
}

fn path_has_immediate_axial_turnback(path: &[FPoint]) -> bool {
    if path.len() < 3 {
        return false;
    }

    path.windows(3).any(|w| {
        let a = w[0];
        let b = w[1];
        let c = w[2];
        if !segment_is_axis_aligned(a, b) || !segment_is_axis_aligned(b, c) {
            return false;
        }

        let dx1 = b.x - a.x;
        let dy1 = b.y - a.y;
        let dx2 = c.x - b.x;
        let dy2 = c.y - b.y;
        let cross = dx1 * dy2 - dy1 * dx2;
        if cross.abs() > ROUTE_EPS {
            return false;
        }

        let dot = dx1 * dx2 + dy1 * dy2;
        dot < -ROUTE_EPS
    })
}

fn effective_edge_direction_for_test(
    node_directions: &std::collections::HashMap<String, mmdflux::Direction>,
    from: &str,
    to: &str,
    fallback: mmdflux::Direction,
) -> mmdflux::Direction {
    let src_dir = node_directions.get(from).copied().unwrap_or(fallback);
    let tgt_dir = node_directions.get(to).copied().unwrap_or(fallback);
    if src_dir == tgt_dir {
        src_dir
    } else {
        fallback
    }
}

fn edge_rank_span_from_dagre_hints(geom: &GraphGeometry, edge_index: usize) -> Option<usize> {
    let EngineHints::Dagre(hints) = geom.engine_hints.as_ref()? else {
        return None;
    };
    let edge = geom.edges.iter().find(|edge| edge.index == edge_index)?;
    let src_rank = *hints.node_ranks.get(&edge.from)?;
    let dst_rank = *hints.node_ranks.get(&edge.to)?;
    Some(src_rank.abs_diff(dst_rank) as usize)
}

fn lateral_detour_from_endpoint_axis(path: &[FPoint], direction: mmdflux::Direction) -> f64 {
    if path.len() < 2 {
        return 0.0;
    }
    let start = path[0];
    let end = path[path.len() - 1];

    match direction {
        mmdflux::Direction::TopDown | mmdflux::Direction::BottomTop => {
            let baseline_min = start.x.min(end.x);
            let baseline_max = start.x.max(end.x);
            let route_min = path
                .iter()
                .map(|point| point.x)
                .fold(f64::INFINITY, f64::min);
            let route_max = path
                .iter()
                .map(|point| point.x)
                .fold(f64::NEG_INFINITY, f64::max);
            (baseline_min - route_min)
                .max(route_max - baseline_max)
                .max(0.0)
        }
        mmdflux::Direction::LeftRight | mmdflux::Direction::RightLeft => {
            let baseline_min = start.y.min(end.y);
            let baseline_max = start.y.max(end.y);
            let route_min = path
                .iter()
                .map(|point| point.y)
                .fold(f64::INFINITY, f64::min);
            let route_max = path
                .iter()
                .map(|point| point.y)
                .fold(f64::NEG_INFINITY, f64::max);
            (baseline_min - route_min)
                .max(route_max - baseline_max)
                .max(0.0)
        }
    }
}

// -----------------------------------------------------------------------
// Task 1.1: Routed geometry contract tests
// -----------------------------------------------------------------------

#[test]
fn routed_geometry_has_correct_node_count() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.nodes.len(), 3);
    assert!(routed.nodes.contains_key("A"));
    assert!(routed.nodes.contains_key("B"));
    assert!(routed.nodes.contains_key("C"));
}

#[test]
fn routed_geometry_has_correct_edge_count() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.edges.len(), 2);
}

#[test]
fn routed_edges_have_non_empty_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    for edge in &routed.edges {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} should have at least 2 path points, got {}",
            edge.from,
            edge.to,
            edge.path.len()
        );
    }
}

#[test]
fn routed_geometry_preserves_label_positions() {
    let (diagram, geom) = layout_test("graph TD\nA--label-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    let edge = &routed.edges[0];
    assert!(
        edge.label_position.is_some(),
        "labeled edge should have a label position"
    );
}

const Q3_MAX_LABEL_DISTANCE_TO_ACTIVE_SEGMENT: f64 = 2.0;

#[test]
fn unified_labels_remain_attached_to_active_segments_labeled_edges() {
    let (diagram, geom) = layout_fixture_svg("labeled_edges.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let failures = labeled_edge_label_drift_failures(
        &diagram,
        &routed,
        Q3_MAX_LABEL_DISTANCE_TO_ACTIVE_SEGMENT,
    );
    assert!(
        failures.is_empty(),
        "Q3 regression: labeled_edges has off-path labels:\n{}",
        failures.join("\n")
    );
}

#[test]
fn unified_labels_remain_attached_to_active_segments_inline_label_flowchart() {
    let (diagram, geom) = layout_fixture_svg("inline_label_flowchart.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let failures = labeled_edge_label_drift_failures(
        &diagram,
        &routed,
        Q3_MAX_LABEL_DISTANCE_TO_ACTIVE_SEGMENT,
    );
    assert!(
        failures.is_empty(),
        "Q3 regression: inline_label_flowchart has off-path labels:\n{}",
        failures.join("\n")
    );
}

#[test]
fn stale_label_anchor_is_replaced_with_valid_route_anchor() {
    let (diagram, geom) = layout_fixture_svg("labeled_edges.mmd");
    let stale_edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Config" && edge.to == "Error")
        .expect("fixture should contain Config -> Error")
        .index;
    let original_anchor = geom
        .edges
        .iter()
        .find(|edge| edge.index == stale_edge_index)
        .and_then(|edge| edge.label_position)
        .expect("fixture should carry layout label anchor for Config -> Error");

    let routed = route_graph_geometry_with_policies(
        &diagram,
        &geom,
        RoutingMode::UnifiedPreview,
        RoutingPolicyToggles {
            q3_label_revalidation: true,
            ..RoutingPolicyToggles::default()
        },
    );
    let routed_edge = routed
        .edges
        .iter()
        .find(|edge| edge.index == stale_edge_index)
        .expect("routed geometry should contain Config -> Error");
    let validated_anchor = routed_edge
        .label_position
        .expect("validated routed label anchor should be present");

    let original_drift = distance_point_to_path(original_anchor, &routed_edge.path);
    let validated_drift = distance_point_to_path(validated_anchor, &routed_edge.path);
    assert!(
        original_drift > Q3_MAX_LABEL_DISTANCE_TO_ACTIVE_SEGMENT,
        "fixture contract invalid: original anchor should be stale for this test (drift={original_drift}, path={:?})",
        routed_edge.path
    );
    assert!(
        validated_drift <= Q3_MAX_LABEL_DISTANCE_TO_ACTIVE_SEGMENT,
        "validated anchor should be on/near active segment after fallback (drift={validated_drift}, anchor={validated_anchor:?}, path={:?})",
        routed_edge.path
    );
}

#[test]
fn routed_geometry_preserves_direction() {
    let (diagram, geom) = layout_test("graph LR\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    assert_eq!(routed.direction, mmdflux::Direction::LeftRight);
}

#[test]
fn routed_geometry_preserves_bounds() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    assert!(routed.bounds.width > 0.0);
    assert!(routed.bounds.height > 0.0);
}

#[test]
fn routed_geometry_preserves_subgraphs() {
    let (diagram, geom) = layout_test("graph TD\nsubgraph sg1[Group]\nA-->B\nend");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert!(!routed.subgraphs.is_empty());
    let sg = &routed.subgraphs["sg1"];
    assert_eq!(sg.title, "Group");
    assert!(sg.rect.width > 0.0);
}

#[test]
fn routed_geometry_marks_backward_edges() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->A");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    // At least one edge should be marked backward (the cycle)
    let backward_count = routed.edges.iter().filter(|e| e.is_backward).count();
    assert!(
        backward_count >= 1,
        "cycle should produce at least one backward edge"
    );
}

#[test]
fn routed_self_edges_have_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->A");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.self_edges.len(), 1);
    assert!(
        routed.self_edges[0].path.len() >= 2,
        "self-edge should have at least 2 path points"
    );
    assert_eq!(routed.self_edges[0].node_id, "A");
}

// -----------------------------------------------------------------------
// Task 1.2: Routing mode tests
// -----------------------------------------------------------------------

#[test]
fn pass_through_mode_uses_layout_path_hints() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::PassThroughClip);

    let edge = &routed.edges[0];
    // PassThroughClip should use the engine-provided path hints directly
    assert!(edge.path.len() >= 2);

    // The path should match the layout_path_hint from the geometry
    let layout_hint = geom.edges[0].layout_path_hint.as_ref().unwrap();
    assert_eq!(edge.path.len(), layout_hint.len());
    for (rp, lp) in edge.path.iter().zip(layout_hint.iter()) {
        assert_eq!(rp.x, lp.x);
        assert_eq!(rp.y, lp.y);
    }
}

#[test]
fn full_compute_mode_produces_valid_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nB-->C\nA-->C");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    assert_eq!(routed.edges.len(), 3);
    for edge in &routed.edges {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} should have valid path",
            edge.from,
            edge.to,
        );
    }
}

#[test]
fn routing_modes_produce_same_structure() {
    let (diagram, geom) = layout_test("graph TD\nA-->B");

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let pass = route_graph_geometry(&diagram, &geom, RoutingMode::PassThroughClip);

    // Both modes should produce the same structural output
    assert_eq!(full.nodes.len(), pass.nodes.len());
    assert_eq!(full.edges.len(), pass.edges.len());
    assert_eq!(full.self_edges.len(), pass.self_edges.len());
    assert_eq!(full.subgraphs.len(), pass.subgraphs.len());
}

#[test]
fn routed_edges_preserve_subgraph_references() {
    let (diagram, geom) = layout_test("graph TD\nsubgraph sg1[Group]\nA\nend\nB-->sg1");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);

    // Check that subgraph references are preserved in routed edges.
    // If the edge connects to a subgraph-as-node, the reference should be preserved.
    if let Some(e) = routed
        .edges
        .iter()
        .find(|e| e.from_subgraph.is_some() || e.to_subgraph.is_some())
    {
        assert!(e.to_subgraph.is_some() || e.from_subgraph.is_some());
    }
}

// -----------------------------------------------------------------------
// Task 4.1: Unified preview routing contracts
// -----------------------------------------------------------------------

#[test]
fn unified_router_produces_axis_aligned_forward_paths() {
    let (diagram, geom) = layout_test("graph TD\nA-->B\nA-->C\nB-->D\nC-->D");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    for edge in routed.edges.iter().filter(|edge| !edge.is_backward) {
        assert!(
            edge.path
                .windows(2)
                .all(|seg| seg[0].x == seg[1].x || seg[0].y == seg[1].y),
            "edge {} -> {} has diagonal segment in unified preview: {:?}",
            edge.from,
            edge.to,
            edge.path
        );
    }
}

#[test]
fn snap_path_to_grid_preserves_start_and_end_nodes() {
    let path = vec![
        FPoint::new(10.2, 20.8),
        FPoint::new(10.2, 40.4),
        FPoint::new(35.7, 40.4),
    ];
    let snapped = snap_path_to_grid_preview(&path, 1.0, 1.0);

    assert_eq!(snapped.first(), Some(&FPoint::new(10.0, 21.0)));
    assert_eq!(snapped.last(), Some(&FPoint::new(36.0, 40.0)));
}

#[test]
fn unified_preview_preserves_core_routed_geometry_contracts() {
    for fixture in ["simple.mmd", "chain.mmd", "simple_cycle.mmd"] {
        let (diagram, geom) = layout_fixture(fixture);
        let legacy = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
        let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

        assert_eq!(
            unified.edges.len(),
            legacy.edges.len(),
            "edge count diverged for fixture {fixture}"
        );
        assert_eq!(
            unified.self_edges.len(),
            legacy.self_edges.len(),
            "self-edge count diverged for fixture {fixture}"
        );

        for (u, l) in unified.edges.iter().zip(legacy.edges.iter()) {
            assert_eq!(u.index, l.index, "edge index mismatch in fixture {fixture}");
            assert_eq!(u.from, l.from, "edge source mismatch in fixture {fixture}");
            assert_eq!(u.to, l.to, "edge target mismatch in fixture {fixture}");
            assert_eq!(
                u.is_backward, l.is_backward,
                "backward-edge flag mismatch in fixture {fixture}"
            );
            assert!(
                u.path.len() >= 2,
                "unified path too short for {} -> {} in fixture {fixture}",
                u.from,
                u.to
            );
        }
    }
}

#[test]
fn unified_route_contracts_are_axis_aligned_and_non_degenerate() {
    let (diagram, geom) = layout_fixture("simple_cycle.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    for edge in &routed.edges {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} has too few points: {:?}",
            edge.from,
            edge.to,
            edge.path
        );

        for seg in edge.path.windows(2) {
            let a = seg[0];
            let b = seg[1];
            assert!(
                segment_is_axis_aligned(a, b),
                "edge {} -> {} contains diagonal segment: {:?}",
                edge.from,
                edge.to,
                edge.path
            );
            assert!(
                segment_is_non_degenerate(a, b),
                "edge {} -> {} contains duplicate or zero-length segment: {:?}",
                edge.from,
                edge.to,
                edge.path
            );
        }
    }
}

#[test]
fn unified_route_contracts_preserve_terminal_support_segment() {
    let (diagram, geom) = layout_fixture("ampersand.mmd");
    assert_eq!(geom.direction, mmdflux::Direction::TopDown);

    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    for edge in routed.edges.iter().filter(|edge| !edge.is_backward) {
        assert!(
            edge.path.len() >= 2,
            "edge {} -> {} must have at least two points",
            edge.from,
            edge.to
        );
        let prev = edge.path[edge.path.len() - 2];
        let end = edge.path[edge.path.len() - 1];
        let dx = (end.x - prev.x).abs();
        let dy = (end.y - prev.y).abs();

        assert!(
            dy > ROUTE_EPS,
            "edge {} -> {} terminal segment is zero-length: {:?}",
            edge.from,
            edge.to,
            edge.path
        );
        assert!(
            dx <= ROUTE_EPS,
            "edge {} -> {} terminal segment is not vertical in TD: {:?}",
            edge.from,
            edge.to,
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_are_deterministic_for_repeated_runs() {
    let (diagram, geom) = layout_fixture("multi_subgraph_direction_override.mmd");
    let first = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let second = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    assert_eq!(first.edges.len(), second.edges.len());
    for (lhs, rhs) in first.edges.iter().zip(second.edges.iter()) {
        assert_eq!(lhs.index, rhs.index);
        assert_eq!(lhs.from, rhs.from);
        assert_eq!(lhs.to, rhs.to);
        assert_eq!(lhs.path, rhs.path);
    }
}

#[test]
fn unified_preview_multi_subgraph_bmid_to_f_keeps_terminal_support_clearance() {
    let (diagram, geom) = layout_fixture("multi_subgraph_direction_override.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let edge = routed
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain Bmid -> F");

    assert!(
        edge.path.len() >= 2,
        "Bmid -> F should have routed path points: {:?}",
        edge.path
    );

    let prev = edge.path[edge.path.len() - 2];
    let end = edge.path[edge.path.len() - 1];
    let dx = (end.x - prev.x).abs();
    let dy = (end.y - prev.y).abs();

    assert!(
        dx <= ROUTE_EPS,
        "Bmid -> F terminal segment should stay vertical in TD: {:?}",
        edge.path
    );
    assert!(
        dy >= 12.0,
        "Bmid -> F terminal support should preserve >=12px clearance before endpoint: dy={dy}, path={:?}",
        edge.path
    );
}

#[test]
fn unified_preview_fan_in_lr_target_endpoints_stay_on_or_outside_target_border() {
    let (diagram, geom) = layout_fixture("fan_in_lr.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let target_rect = geom
        .nodes
        .get("D")
        .expect("fan_in_lr should contain target node D")
        .rect;

    for edge in routed
        .edges
        .iter()
        .filter(|edge| (edge.from == "A" || edge.from == "C") && edge.to == "D")
    {
        assert!(
            edge.path.len() >= 2,
            "edge should contain at least two points: {:?}",
            edge.path
        );
        let prev = edge.path[edge.path.len() - 2];
        let end = *edge.path.last().expect("edge should have routed points");
        assert!(
            !point_inside_rect(target_rect, end),
            "unified routed endpoint should not be inside target rect for {} -> {}: end={:?}, target_rect={:?}, path={:?}",
            edge.from,
            edge.to,
            end,
            target_rect,
            edge.path
        );
        assert!(
            terminal_support_is_normal_to_attached_rect_face(target_rect, prev, end),
            "fan_in_lr terminal segment should approach D on the face-normal axis for {} -> {}: prev={:?}, end={:?}, target_rect={:?}, path={:?}",
            edge.from,
            edge.to,
            prev,
            end,
            target_rect,
            edge.path
        );
    }
}

#[test]
fn unified_preview_http_request_backward_edge_preserves_client_side_face_attachment() {
    let (diagram, geom) = layout_fixture("http_request.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let edge = routed
        .edges
        .iter()
        .find(|edge| edge.from == "Response" && edge.to == "Client")
        .expect("fixture should contain Response -> Client");
    assert!(
        edge.path.len() >= 2,
        "Response -> Client should have at least two routed points: {:?}",
        edge.path
    );

    let end = *edge.path.last().expect("edge should have endpoint");
    let client_rect = geom
        .nodes
        .get("Client")
        .expect("fixture should contain Client")
        .rect;
    let right = client_rect.x + client_rect.width;
    let bottom = client_rect.y + client_rect.height;

    let dist_to_right = (right - end.x).abs();
    let dist_to_bottom = (bottom - end.y).abs();
    assert!(
        dist_to_right + 0.5 < dist_to_bottom,
        "Response -> Client endpoint should favor Client right face over bottom face in unified preview: end={end:?}, client_rect={client_rect:?}, dist_to_right={dist_to_right}, dist_to_bottom={dist_to_bottom}, path={:?}",
        edge.path
    );
    assert!(
        end.y < bottom - 0.5,
        "Response -> Client endpoint should not collapse to bottom corner in unified preview: end={end:?}, client_rect={client_rect:?}, path={:?}",
        edge.path
    );
}

#[test]
fn unified_route_contracts_prefer_lateral_exit_for_off_center_td_source_ports() {
    let (diagram, geom) = layout_fixture("compat_kitchen_sink.mmd");
    assert_eq!(geom.direction, mmdflux::Direction::TopDown);
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    for (from, to) in [("check-1", "process-A"), ("check-1", "error-1")] {
        let edge = routed
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture should contain {from} -> {to}"));
        assert!(
            edge.path.len() >= 3,
            "{from} -> {to} should have at least three routed points: {:?}",
            edge.path
        );

        let source_rect = geom
            .nodes
            .get(from)
            .unwrap_or_else(|| panic!("fixture should contain source {from}"))
            .rect;
        let start = edge.path[0];
        let next = edge.path[1];
        let center_x = source_rect.x + source_rect.width / 2.0;
        let source_offset = (start.x - center_x).abs();
        let min_off_center = 1.0;

        assert!(
            source_offset >= min_off_center,
            "fixture expectation invalid: {from} -> {to} source should be off-center, got offset={source_offset}, min={min_off_center}, path={:?}",
            edge.path
        );
        assert!(
            (next.y - start.y).abs() <= ROUTE_EPS && (next.x - start.x).abs() > ROUTE_EPS,
            "off-center TD source should prefer lateral first segment to avoid down-then-sweep artifact for {from} -> {to}: start={start:?}, next={next:?}, path={:?}",
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_keep_primary_stem_before_outward_td_fan_out_sweeps() {
    let (diagram, geom) = layout_fixture("fan_out.mmd");
    assert_eq!(geom.direction, mmdflux::Direction::TopDown);
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    for (from, to) in [("A", "B"), ("A", "D")] {
        let edge = routed
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture should contain {from} -> {to}"));
        assert!(
            edge.path.len() >= 3,
            "{from} -> {to} should have at least three routed points: {:?}",
            edge.path
        );

        let source_rect = geom
            .nodes
            .get(from)
            .unwrap_or_else(|| panic!("fixture should contain source {from}"))
            .rect;
        let start = edge.path[0];
        let next = edge.path[1];
        let third = edge.path[2];
        let center_x = source_rect.x + source_rect.width / 2.0;
        let source_offset = start.x - center_x;
        assert!(
            source_offset.abs() >= 1.0,
            "fixture expectation invalid: {from} -> {to} source should be off-center, offset={source_offset}, path={:?}",
            edge.path
        );
        assert!(
            (next.x - start.x).abs() <= ROUTE_EPS && (next.y - start.y).abs() > ROUTE_EPS,
            "fan-out edge {from} -> {to} should keep a short primary-axis source stem before sweeping: start={start:?}, next={next:?}, path={:?}",
            edge.path
        );
        assert!(
            (third.y - next.y).abs() <= ROUTE_EPS && (third.x - next.x).abs() > ROUTE_EPS,
            "fan-out edge {from} -> {to} should sweep laterally after the source stem: next={next:?}, third={third:?}, path={:?}",
            edge.path
        );
        assert!(
            (third.x - next.x).signum() == source_offset.signum(),
            "fan-out edge {from} -> {to} should sweep outward from source center: source_offset={source_offset}, second_dx={}, path={:?}",
            third.x - next.x,
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_prefer_outward_first_source_exits_for_selected_fixtures() {
    type EdgeExpectation = (&'static str, &'static str, f64);
    type FixtureExpectations = (&'static str, &'static [EdgeExpectation]);

    let td_cases: &[FixtureExpectations] = &[
        ("decision.mmd", &[("B", "D", 1.0)]),
        ("complex.mmd", &[("B", "D", 5.0)]),
        ("double_skip.mmd", &[("A", "D", 1.0)]),
    ];

    for (fixture, edges) in td_cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        assert_eq!(
            geom.direction,
            mmdflux::Direction::TopDown,
            "fixture {fixture} should be TD for outward-first source contract"
        );
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

        for (from, to, min_offset) in *edges {
            let edge = routed
                .edges
                .iter()
                .find(|edge| edge.from == *from && edge.to == *to)
                .unwrap_or_else(|| panic!("fixture {fixture} missing edge {from} -> {to}"));
            assert!(
                edge.path.len() >= 2,
                "fixture {fixture} edge {from} -> {to} should have at least two points: {:?}",
                edge.path
            );

            let source_rect = geom
                .nodes
                .get(*from)
                .unwrap_or_else(|| panic!("fixture {fixture} missing source node {from}"))
                .rect;
            let start = edge.path[0];
            let next = edge.path[1];
            let center_x = source_rect.x + source_rect.width / 2.0;
            let source_offset = start.x - center_x;
            assert!(
                source_offset.abs() >= *min_offset,
                "fixture expectation invalid: {from} -> {to} should start noticeably off-center (offset={source_offset}, min_offset={min_offset}) in {fixture}, path={:?}",
                edge.path
            );

            let first_dx = next.x - start.x;
            let first_dy = next.y - start.y;
            if edge.path.len() >= 3 {
                assert!(
                    first_dy.abs() <= ROUTE_EPS && first_dx.abs() > ROUTE_EPS,
                    "fixture {fixture} edge {from} -> {to} should leave source laterally first in TD when a bend is present: start={start:?}, next={next:?}, path={:?}",
                    edge.path
                );
                assert!(
                    first_dx.signum() == source_offset.signum(),
                    "fixture {fixture} edge {from} -> {to} should move outward from source center on first segment: offset={source_offset}, first_dx={first_dx}, path={:?}",
                    edge.path
                );
            } else {
                assert!(
                    first_dx.abs() <= ROUTE_EPS && first_dy.abs() > ROUTE_EPS,
                    "fixture {fixture} edge {from} -> {to} compact direct path should remain a primary-axis source support segment in TD: start={start:?}, next={next:?}, path={:?}",
                    edge.path
                );
            }
        }
    }

    let (diagram, geom) = layout_fixture_svg("git_workflow.mmd");
    assert_eq!(
        geom.direction,
        mmdflux::Direction::LeftRight,
        "git_workflow fixture should be LR"
    );
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    for (from, to) in [("Working", "Staging"), ("Local", "Remote")] {
        let edge = routed
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("git_workflow missing edge {from} -> {to}"));
        assert!(
            edge.path.len() >= 2,
            "git_workflow edge {from} -> {to} should have at least two points: {:?}",
            edge.path
        );
        let start = edge.path[0];
        let next = edge.path[1];
        assert!(
            (next.y - start.y).abs() <= ROUTE_EPS && (next.x - start.x).abs() > ROUTE_EPS,
            "git_workflow edge {from} -> {to} should leave source on LR primary axis first: start={start:?}, next={next:?}, path={:?}",
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_avoid_source_turnback_spikes_for_selected_fixtures() {
    let cases = [
        ("decision.mmd", "A", "B"),
        ("complex.mmd", "B", "D"),
        ("double_skip.mmd", "A", "D"),
        ("git_workflow.mmd", "Working", "Staging"),
    ];

    for (fixture, from, to) in cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
        let edge = routed
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture {fixture} missing edge {from} -> {to}"));
        assert!(
            !path_has_source_turnback_spike(&edge.path),
            "fixture {fixture} edge {from} -> {to} should not contain source-local A-B-A turnback spike: path={:?}",
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_avoid_immediate_axial_turnbacks() {
    let cases = [
        ("multiple_cycles.mmd", "C", "A"),
        ("git_workflow.mmd", "Remote", "Working"),
        ("simple_cycle.mmd", "C", "A"),
        ("backward_in_subgraph.mmd", "B", "A"),
    ];

    for (fixture, from, to) in cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
        let edge = routed
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture {fixture} missing edge {from} -> {to}"));
        assert!(
            !path_has_immediate_axial_turnback(&edge.path),
            "fixture {fixture} edge {from} -> {to} should not contain immediate axial turnbacks: path={:?}",
            edge.path
        );
    }
}

#[test]
fn unified_route_contracts_preserve_backward_cycle_outer_lane_clearance() {
    const MIN_OUTER_LANE_CLEARANCE: f64 = 12.0;

    let (diagram, geom) = layout_fixture_svg("multiple_cycles.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let edge = routed
        .edges
        .iter()
        .find(|edge| edge.from == "C" && edge.to == "A")
        .expect("multiple_cycles fixture missing edge C -> A");

    assert!(
        edge.path.len() >= 4,
        "multiple_cycles C -> A should have enough routed points to form an outer return lane: path={:?}",
        edge.path
    );

    let start = edge.path[0];
    let end = *edge.path.last().expect("edge path is non-empty");
    let baseline_max_x = start.x.max(end.x);
    let route_max_x = edge
        .path
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let clearance = route_max_x - baseline_max_x;

    assert!(
        clearance >= MIN_OUTER_LANE_CLEARANCE,
        "multiple_cycles C -> A should preserve an outer-lane lateral clearance (>= {MIN_OUTER_LANE_CLEARANCE}) instead of collapsing into a near-vertical return: clearance={clearance}, path={:?}",
        edge.path
    );
}

// -----------------------------------------------------------------------
// Task 1.2: Shared float-route heuristics
// -----------------------------------------------------------------------

#[test]
fn shared_builder_prefers_terminal_segment_matching_layout_entry_axis() {
    let (diagram, geom) = layout_fixture("direction_override.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    for edge in routed.edges.iter().filter(|edge| !edge.is_backward) {
        let expected_direction = effective_edge_direction_for_test(
            &geom.node_directions,
            &edge.from,
            &edge.to,
            geom.direction,
        );
        let prev = edge.path[edge.path.len() - 2];
        let end = edge.path[edge.path.len() - 1];
        let x_aligned = approx_eq(prev.x, end.x);
        let y_aligned = approx_eq(prev.y, end.y);

        match expected_direction {
            mmdflux::Direction::TopDown | mmdflux::Direction::BottomTop => assert!(
                x_aligned && !y_aligned,
                "edge {} -> {} should enter on vertical terminal segment for {:?}, got {:?}",
                edge.from,
                edge.to,
                expected_direction,
                edge.path
            ),
            mmdflux::Direction::LeftRight | mmdflux::Direction::RightLeft => assert!(
                y_aligned && !x_aligned,
                "edge {} -> {} should enter on horizontal terminal segment for {:?}, got {:?}",
                edge.from,
                edge.to,
                expected_direction,
                edge.path
            ),
        }
    }
}

#[test]
fn shared_builder_reduces_midfield_jogs_for_large_horizontal_offset_edges() {
    let (diagram, geom) = layout_fixture("decision.mmd");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let edge = routed
        .edges
        .iter()
        .find(|edge| edge.from == "B" && edge.to == "D")
        .expect("expected B -> D edge in decision fixture");
    let horizontal_offset = (edge.path[0].x - edge.path[edge.path.len() - 1].x).abs();

    assert!(
        horizontal_offset > 30.0,
        "test fixture no longer has large horizontal offset: {horizontal_offset}"
    );
    assert!(
        bend_count(&edge.path) <= 2,
        "expected congestion heuristic to reduce bends for B -> D, got path {:?}",
        edge.path
    );
}

#[test]
fn shared_builder_keeps_alignment_tolerance_stable_for_near_aligned_points() {
    let (diagram, mut geom) = layout_test("graph TD\nA-->B");
    let hint = geom.edges[0]
        .layout_path_hint
        .clone()
        .expect("layout path hint should exist");
    let start = hint[0];
    let end = hint[hint.len() - 1];
    let y_span = end.y - start.y;

    geom.edges[0].layout_path_hint = Some(vec![
        start,
        FPoint::new(start.x + 0.4, start.y + y_span * 0.33),
        FPoint::new(start.x - 0.4, start.y + y_span * 0.66),
        end,
    ]);

    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let edge = &routed.edges[0];
    assert!(
        bend_count(&edge.path) <= 2,
        "near-aligned jitter should not produce extra elbows, got {:?}",
        edge.path
    );
    assert!(
        edge.path
            .windows(2)
            .all(|seg| segment_is_axis_aligned(seg[0], seg[1])),
        "near-aligned jitter produced non-orthogonal segment: {:?}",
        edge.path
    );
}

#[test]
fn unified_route_contracts_keep_td_source_ports_normal_and_compact() {
    let cases: &[(&str, &[(&str, &str)])] = &[(
        "compat_kitchen_sink.mmd",
        &[
            ("start-node", "check-1"),
            ("process-A", "end-node"),
            ("error-1", "end-node"),
        ],
    )];

    for (fixture, edges) in cases {
        let (diagram, geom) = layout_fixture(fixture);
        assert_eq!(
            geom.direction,
            mmdflux::Direction::TopDown,
            "fixture {fixture} should be TD for source-support contract"
        );
        let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

        for (from, to) in *edges {
            let edge = routed
                .edges
                .iter()
                .find(|edge| edge.from == *from && edge.to == *to)
                .unwrap_or_else(|| panic!("fixture {fixture} missing edge {from} -> {to}"));
            assert!(
                edge.path.len() >= 2,
                "fixture {fixture} edge {from} -> {to} should have at least two points: {:?}",
                edge.path
            );
            let source_rect = geom
                .nodes
                .get(*from)
                .unwrap_or_else(|| panic!("fixture {fixture} missing source node {from}"))
                .rect;
            let start = edge.path[0];
            let next = edge.path[1];
            let center_x = source_rect.x + source_rect.width / 2.0;
            let source_offset = (start.x - center_x).abs();
            assert!(
                source_offset <= 1.0,
                "fixture expectation invalid: source should be centered for this contract on {from} -> {to}, offset={source_offset}, path={:?}",
                edge.path
            );
            assert!(
                source_support_is_normal_to_attached_rect_face(source_rect, start, next),
                "fixture {fixture} edge {from} -> {to} should leave source face on its normal axis in TD (avoid bottom-border sliding): start={start:?}, next={next:?}, source_rect={source_rect:?}, path={:?}",
                edge.path
            );
            let bends = bend_count(&edge.path);
            assert!(
                bends <= 2,
                "fixture {fixture} edge {from} -> {to} should stay compact after source-support preservation: bends={bends}, path={:?}",
                edge.path
            );
        }
    }
}

// -----------------------------------------------------------------------
// Task 0.1: TD/BT backward entry-face parity RED regressions
// -----------------------------------------------------------------------

#[test]
fn unified_preview_decision_backward_debug_to_start_supports_td_top_bottom_parity() {
    let fixture = "decision.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::TopDown,
        "fixture {fixture} should be TD for backward entry-face parity"
    );

    let source_rect = geom
        .nodes
        .get("D")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain source node D"))
        .rect;
    let target_rect = geom
        .nodes
        .get("A")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain target node A"))
        .rect;

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let full_edge = full
        .edges
        .iter()
        .find(|edge| edge.from == "D" && edge.to == "A")
        .expect("fixture should contain backward edge D -> A in full-compute mode");
    assert!(
        full_edge.is_backward,
        "fixture contract invalid: D -> A should be backward in full-compute mode"
    );
    let full_start = full_edge
        .path
        .first()
        .copied()
        .expect("full-compute backward edge should have source endpoint");
    let full_end = full_edge
        .path
        .last()
        .copied()
        .expect("full-compute backward edge should have target endpoint");
    let full_source_face = point_on_target_face(source_rect, full_start);
    let full_target_face = point_on_target_face(target_rect, full_end);
    assert_eq!(
        full_source_face, "top",
        "fixture contract changed unexpectedly: full-compute D -> A should depart from source top face for TD top->bottom parity; start={full_start:?}, path={:?}",
        full_edge.path
    );
    assert_eq!(
        full_target_face, "bottom",
        "fixture contract changed unexpectedly: full-compute D -> A should enter target bottom face for TD top->bottom parity; end={full_end:?}, path={:?}",
        full_edge.path
    );

    let unified_edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "D" && edge.to == "A")
        .expect("fixture should contain backward edge D -> A in unified-preview mode");
    assert!(
        unified_edge.is_backward,
        "fixture contract invalid: D -> A should be backward in unified-preview mode"
    );
    let unified_start = unified_edge
        .path
        .first()
        .copied()
        .expect("unified-preview backward edge should have source endpoint");
    let unified_end = unified_edge
        .path
        .last()
        .copied()
        .expect("unified-preview backward edge should have target endpoint");
    let unified_source_face = point_on_target_face(source_rect, unified_start);
    let unified_target_face = point_on_target_face(target_rect, unified_end);

    assert_eq!(
        unified_source_face, full_source_face,
        "unified-preview D -> A should match full-compute source departure face for TD top->bottom parity: full={full_source_face}, unified={unified_source_face}, full_path={:?}, unified_path={:?}",
        full_edge.path, unified_edge.path
    );
    assert_eq!(
        unified_target_face, full_target_face,
        "unified-preview D -> A should match full-compute target entry face for TD top->bottom parity: full={full_target_face}, unified={unified_target_face}, full_path={:?}, unified_path={:?}",
        full_edge.path, unified_edge.path
    );
}

#[test]
fn unified_preview_decision_backward_debug_to_start_keeps_vertical_source_stem_before_elbow() {
    let fixture = "decision.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::TopDown,
        "fixture {fixture} should be TD for source-stem normalization checks"
    );

    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "D" && edge.to == "A")
        .expect("fixture should contain backward edge D -> A in unified-preview mode");
    assert!(
        edge.is_backward,
        "fixture contract invalid: D -> A should be backward in unified-preview mode"
    );
    assert!(
        edge.path.len() >= 3,
        "D -> A should expose source stem and elbow points for this contract: path={:?}",
        edge.path
    );

    let start = edge.path[0];
    let source_stem = edge.path[1];
    assert!(
        approx_eq(start.x, source_stem.x),
        "unified-preview D -> A should keep a vertical source stem before the backward elbow (avoid diagonal stem drift): start={start:?}, source_stem={source_stem:?}, path={:?}",
        edge.path
    );
    assert!(
        source_stem.y < start.y,
        "TD backward source stem should move upward from Debug before elbow: start={start:?}, source_stem={source_stem:?}, path={:?}",
        edge.path
    );
}

#[test]
fn unified_preview_complex_backward_more_data_to_input_supports_td_entry_parity() {
    let fixture = "complex.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::TopDown,
        "fixture {fixture} should be TD for backward target-entry parity"
    );

    let target_rect = geom
        .nodes
        .get("A")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain target node A"))
        .rect;

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let full_edge = full
        .edges
        .iter()
        .find(|edge| edge.from == "E" && edge.to == "A")
        .expect("fixture should contain backward edge E -> A in full-compute mode");
    assert!(
        full_edge.is_backward,
        "fixture contract invalid: E -> A should be backward in full-compute mode"
    );
    let full_end = full_edge
        .path
        .last()
        .copied()
        .expect("full-compute backward edge should have target endpoint");
    let full_target_face = point_on_target_face(target_rect, full_end);
    assert_eq!(
        full_target_face, "bottom",
        "fixture contract changed unexpectedly: full-compute E -> A should enter target bottom face for TD entry parity; end={full_end:?}, path={:?}",
        full_edge.path
    );

    let unified_edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "E" && edge.to == "A")
        .expect("fixture should contain backward edge E -> A in unified-preview mode");
    assert!(
        unified_edge.is_backward,
        "fixture contract invalid: E -> A should be backward in unified-preview mode"
    );
    let unified_end = unified_edge
        .path
        .last()
        .copied()
        .expect("unified-preview backward edge should have target endpoint");
    let unified_target_face = point_on_target_face(target_rect, unified_end);

    assert_eq!(
        unified_target_face, full_target_face,
        "unified-preview E -> A should match full-compute target entry face for TD entry parity: full={full_target_face}, unified={unified_target_face}, full_path={:?}, unified_path={:?}",
        full_edge.path, unified_edge.path
    );
}

#[test]
fn unified_preview_complex_backward_more_data_to_input_avoids_tiny_terminal_staircase_elbow() {
    const MIN_TERMINAL_LATERAL_RUN: f64 = 6.0;

    let fixture = "complex.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::TopDown,
        "fixture {fixture} should be TD for terminal staircase checks"
    );

    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "E" && edge.to == "A")
        .expect("fixture should contain backward edge E -> A in unified-preview mode");
    assert!(
        edge.is_backward,
        "fixture contract invalid: E -> A should be backward in unified-preview mode"
    );
    assert!(
        edge.path.len() >= 3,
        "E -> A should have at least three points for terminal staircase checks: path={:?}",
        edge.path
    );

    let n = edge.path.len();
    let a = edge.path[n - 3];
    let b = edge.path[n - 2];
    let c = edge.path[n - 1];
    let ab_is_horizontal = approx_eq(a.y, b.y) && !approx_eq(a.x, b.x);
    let bc_is_vertical = approx_eq(b.x, c.x) && !approx_eq(b.y, c.y);
    if ab_is_horizontal && bc_is_vertical {
        let lateral_run = (b.x - a.x).abs();
        assert!(
            lateral_run >= MIN_TERMINAL_LATERAL_RUN,
            "unified-preview E -> A should avoid tiny terminal staircase elbows that create acute kinks near the target (min lateral run {MIN_TERMINAL_LATERAL_RUN}): lateral_run={lateral_run}, path={:?}",
            edge.path
        );
    }
}

// -----------------------------------------------------------------------
// Task 0.2: LR/RL backward clearance parity RED regressions
// -----------------------------------------------------------------------

#[test]
fn unified_preview_git_workflow_backward_remote_to_working_preserves_min_lr_channel_spacing() {
    const MAX_CHANNEL_LANE_Y_DRIFT: f64 = 3.0;
    const MAX_BEND_INCREASE_FROM_FULL: usize = 1;

    let fixture = "git_workflow.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::LeftRight,
        "fixture {fixture} should be LR for channel-spacing parity checks"
    );

    let source_rect = geom
        .nodes
        .get("Remote")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain source node Remote"))
        .rect;
    let target_rect = geom
        .nodes
        .get("Working")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain target node Working"))
        .rect;

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let full_edge = full
        .edges
        .iter()
        .find(|edge| edge.from == "Remote" && edge.to == "Working")
        .expect("fixture should contain backward edge Remote -> Working in full-compute mode");
    let unified_edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "Remote" && edge.to == "Working")
        .expect("fixture should contain backward edge Remote -> Working in unified-preview mode");

    assert!(
        full_edge.is_backward && unified_edge.is_backward,
        "fixture contract invalid: Remote -> Working should be backward in both routing modes"
    );

    let full_start = full_edge
        .path
        .first()
        .copied()
        .expect("full-compute backward edge should have source endpoint");
    let full_end = full_edge
        .path
        .last()
        .copied()
        .expect("full-compute backward edge should have target endpoint");
    let unified_start = unified_edge
        .path
        .first()
        .copied()
        .expect("unified-preview backward edge should have source endpoint");
    let unified_end = unified_edge
        .path
        .last()
        .copied()
        .expect("unified-preview backward edge should have target endpoint");

    let _full_source_face = point_on_target_face(source_rect, full_start);
    let _full_target_face = point_on_target_face(target_rect, full_end);
    let unified_source_face = point_on_target_face(source_rect, unified_start);
    let unified_target_face = point_on_target_face(target_rect, unified_end);

    assert_eq!(
        unified_source_face, "bottom",
        "unified-preview Remote -> Working should preserve canonical bottom source face while normalizing spacing parity: start={unified_start:?}, path={:?}",
        unified_edge.path
    );
    assert_eq!(
        unified_target_face, "bottom",
        "unified-preview Remote -> Working should preserve canonical bottom target face while normalizing spacing parity: end={unified_end:?}, path={:?}",
        unified_edge.path
    );

    let full_lane_y = full_edge
        .path
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let unified_lane_y = unified_edge
        .path
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);

    assert!(
        (unified_lane_y - full_lane_y).abs() <= MAX_CHANNEL_LANE_Y_DRIFT,
        "unified-preview Remote -> Working should keep LR backward lane spacing close to full-compute baseline (max y drift <= {MAX_CHANNEL_LANE_Y_DRIFT}): full_lane_y={full_lane_y}, unified_lane_y={unified_lane_y}, full_path={:?}, unified_path={:?}",
        full_edge.path,
        unified_edge.path
    );

    let full_bends = bend_count(&full_edge.path);
    let unified_bends = bend_count(&unified_edge.path);
    assert!(
        unified_bends <= full_bends + MAX_BEND_INCREASE_FROM_FULL,
        "unified-preview Remote -> Working should avoid extra loop compaction bends relative to full-compute baseline: full_bends={full_bends}, unified_bends={unified_bends}, full_path={:?}, unified_path={:?}",
        full_edge.path,
        unified_edge.path
    );
}

#[test]
fn unified_preview_http_request_backward_response_to_client_preserves_min_right_clearance() {
    const MAX_RIGHT_CLEARANCE_SHRINK_FROM_FULL: f64 = 8.0;

    let fixture = "http_request.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    let source_rect = geom
        .nodes
        .get("Response")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain source node Response"))
        .rect;
    let target_rect = geom
        .nodes
        .get("Client")
        .unwrap_or_else(|| panic!("fixture {fixture} should contain target node Client"))
        .rect;

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let full_edge = full
        .edges
        .iter()
        .find(|edge| edge.from == "Response" && edge.to == "Client")
        .expect("fixture should contain backward edge Response -> Client in full-compute mode");
    let unified_edge = unified
        .edges
        .iter()
        .find(|edge| edge.from == "Response" && edge.to == "Client")
        .expect("fixture should contain backward edge Response -> Client in unified-preview mode");

    assert!(
        full_edge.is_backward && unified_edge.is_backward,
        "fixture contract invalid: Response -> Client should be backward in both routing modes"
    );

    let full_start = full_edge
        .path
        .first()
        .copied()
        .expect("full-compute backward edge should have source endpoint");
    let full_end = full_edge
        .path
        .last()
        .copied()
        .expect("full-compute backward edge should have target endpoint");
    let unified_start = unified_edge
        .path
        .first()
        .copied()
        .expect("unified-preview backward edge should have source endpoint");
    let unified_end = unified_edge
        .path
        .last()
        .copied()
        .expect("unified-preview backward edge should have target endpoint");

    let _full_source_face = point_on_target_face(source_rect, full_start);
    let _full_target_face = point_on_target_face(target_rect, full_end);
    let unified_source_face = point_on_target_face(source_rect, unified_start);
    let unified_target_face = point_on_target_face(target_rect, unified_end);

    assert_eq!(
        unified_source_face, "right",
        "unified-preview Response -> Client should preserve canonical right source face while normalizing right-side clearance: start={unified_start:?}, path={:?}",
        unified_edge.path
    );
    assert_eq!(
        unified_target_face, "right",
        "unified-preview Response -> Client should preserve canonical right target face while normalizing right-side clearance: end={unified_end:?}, path={:?}",
        unified_edge.path
    );

    let full_right_lane_x = full_edge
        .path
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let unified_right_lane_x = unified_edge
        .path
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);

    assert!(
        unified_right_lane_x + MAX_RIGHT_CLEARANCE_SHRINK_FROM_FULL >= full_right_lane_x,
        "unified-preview Response -> Client should preserve minimum right-side backward clearance close to full-compute baseline (allowed shrink <= {MAX_RIGHT_CLEARANCE_SHRINK_FROM_FULL}): full_right_lane_x={full_right_lane_x}, unified_right_lane_x={unified_right_lane_x}, full_path={:?}, unified_path={:?}",
        full_edge.path,
        unified_edge.path
    );
}

#[test]
fn unified_preview_multi_edge_labeled_preserves_parallel_lane_separation() {
    const MAX_LANE_DETOUR_LOSS_FROM_FULL: f64 = 2.0;

    let fixture = "multi_edge_labeled.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    assert_eq!(
        geom.direction,
        mmdflux::Direction::TopDown,
        "fixture {fixture} should be TD for multi-edge lane separation checks"
    );

    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let mut full_parallel_edges: Vec<_> = full
        .edges
        .iter()
        .filter(|edge| edge.from == "A" && edge.to == "B")
        .collect();
    let mut unified_parallel_edges: Vec<_> = unified
        .edges
        .iter()
        .filter(|edge| edge.from == "A" && edge.to == "B")
        .collect();
    full_parallel_edges.sort_by_key(|edge| edge.index);
    unified_parallel_edges.sort_by_key(|edge| edge.index);

    assert_eq!(
        full_parallel_edges.len(),
        2,
        "fixture contract invalid: full-compute should keep two A->B parallel edges"
    );
    assert_eq!(
        unified_parallel_edges.len(),
        2,
        "fixture contract invalid: unified-preview should keep two A->B parallel edges"
    );

    for (full_edge, unified_edge) in full_parallel_edges.iter().zip(unified_parallel_edges.iter()) {
        let full_detour = lateral_detour_from_endpoint_axis(&full_edge.path, geom.direction);
        let unified_detour = lateral_detour_from_endpoint_axis(&unified_edge.path, geom.direction);
        assert!(
            full_detour >= 8.0,
            "fixture contract changed unexpectedly: full-compute A->B edge index {} should keep a bowed parallel lane (detour >= 8): detour={full_detour}, path={:?}",
            full_edge.index,
            full_edge.path
        );
        assert!(
            unified_detour + MAX_LANE_DETOUR_LOSS_FROM_FULL >= full_detour,
            "unified-preview A->B edge index {} should preserve parallel lane detour close to full-compute (loss <= {MAX_LANE_DETOUR_LOSS_FROM_FULL}): full_detour={full_detour}, unified_detour={unified_detour}, full_path={:?}, unified_path={:?}",
            full_edge.index,
            full_edge.path,
            unified_edge.path
        );
    }
}

#[test]
fn unified_preview_multi_edge_labeled_preserves_label_spacing_floor() {
    const MAX_LABEL_SPACING_LOSS_FROM_FULL: f64 = 10.0;

    let fixture = "multi_edge_labeled.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    let full = route_graph_geometry(&diagram, &geom, RoutingMode::FullCompute);
    let unified = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    let mut full_labels: Vec<_> = full
        .edges
        .iter()
        .filter(|edge| edge.from == "A" && edge.to == "B")
        .map(|edge| {
            (
                edge.index,
                edge.label_position.unwrap_or_else(|| {
                    panic!(
                        "full-compute multi_edge_labeled edge index {} should include label_position",
                        edge.index
                    )
                }),
            )
        })
        .collect();
    let mut unified_labels: Vec<_> = unified
        .edges
        .iter()
        .filter(|edge| edge.from == "A" && edge.to == "B")
        .map(|edge| {
            (
                edge.index,
                edge.label_position.unwrap_or_else(|| {
                    panic!(
                        "unified-preview multi_edge_labeled edge index {} should include label_position",
                        edge.index
                    )
                }),
            )
        })
        .collect();
    full_labels.sort_by_key(|(index, _)| *index);
    unified_labels.sort_by_key(|(index, _)| *index);

    assert_eq!(
        full_labels.len(),
        2,
        "fixture contract invalid: full-compute should expose two A->B label positions"
    );
    assert_eq!(
        unified_labels.len(),
        2,
        "fixture contract invalid: unified-preview should expose two A->B label positions"
    );

    let full_spacing = point_distance(full_labels[0].1, full_labels[1].1);
    let unified_spacing = point_distance(unified_labels[0].1, unified_labels[1].1);

    assert!(
        full_spacing >= 60.0,
        "fixture contract changed unexpectedly: full-compute A->B label spacing should remain wide (>= 60px), got {full_spacing}; labels={full_labels:?}"
    );
    assert!(
        unified_spacing + MAX_LABEL_SPACING_LOSS_FROM_FULL >= full_spacing,
        "unified-preview should preserve multi-edge label spacing close to full-compute baseline (loss <= {MAX_LABEL_SPACING_LOSS_FROM_FULL}): full_spacing={full_spacing}, unified_spacing={unified_spacing}, full_labels={full_labels:?}, unified_labels={unified_labels:?}"
    );
}

// -----------------------------------------------------------------------
// Task 0.2: Q1 policy spec contracts
// -----------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Q1SpecDirection {
    TopDown,
    BottomTop,
    LeftRight,
    RightLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Q1OverflowSide {
    LeftOrTop,
    RightOrBottom,
}

const Q1_PRIMARY_FACE_CAPACITY_TD_BT: usize = 4;
const Q1_PRIMARY_FACE_CAPACITY_LR_RL: usize = 2;

fn q1_primary_face_capacity(direction: Q1SpecDirection) -> usize {
    match direction {
        Q1SpecDirection::TopDown | Q1SpecDirection::BottomTop => Q1_PRIMARY_FACE_CAPACITY_TD_BT,
        Q1SpecDirection::LeftRight | Q1SpecDirection::RightLeft => Q1_PRIMARY_FACE_CAPACITY_LR_RL,
    }
}

fn q1_overflow_activates(direction: Q1SpecDirection, incoming_degree: usize) -> bool {
    incoming_degree > q1_primary_face_capacity(direction)
}

fn q1_overflow_distribution_order(
    _direction: Q1SpecDirection,
    overflow_count: usize,
) -> Vec<Q1OverflowSide> {
    let mut order = Vec::with_capacity(overflow_count);
    for index in 0..overflow_count {
        if index % 2 == 0 {
            order.push(Q1OverflowSide::LeftOrTop);
        } else {
            order.push(Q1OverflowSide::RightOrBottom);
        }
    }
    order
}

#[test]
fn q1_policy_spec_defines_when_overflow_must_activate() {
    let cases = [
        ("stacked_fan_in.mmd", Q1SpecDirection::TopDown, 2, false),
        ("fan_in.mmd", Q1SpecDirection::TopDown, 3, false),
        ("five_fan_in.mmd", Q1SpecDirection::TopDown, 5, true),
        ("fan_in_lr.mmd", Q1SpecDirection::LeftRight, 3, true),
    ];

    for (fixture, direction, incoming_degree, expected_overflow) in cases {
        let actual = q1_overflow_activates(direction, incoming_degree);
        assert_eq!(
            actual, expected_overflow,
            "Q1 overflow activation contract mismatch for fixture {fixture}: direction={direction:?}, incoming_degree={incoming_degree}"
        );
    }
}

#[test]
fn q1_policy_spec_defines_spill_distribution_order() {
    let td_order = q1_overflow_distribution_order(Q1SpecDirection::TopDown, 4);
    assert_eq!(
        td_order,
        vec![
            Q1OverflowSide::LeftOrTop,
            Q1OverflowSide::RightOrBottom,
            Q1OverflowSide::LeftOrTop,
            Q1OverflowSide::RightOrBottom,
        ],
        "TD/BT overflow slots should alternate side lanes for deterministic spread"
    );

    let lr_order = q1_overflow_distribution_order(Q1SpecDirection::LeftRight, 3);
    assert_eq!(
        lr_order,
        vec![
            Q1OverflowSide::LeftOrTop,
            Q1OverflowSide::RightOrBottom,
            Q1OverflowSide::LeftOrTop,
        ],
        "LR/RL overflow slots should alternate side lanes for deterministic spread"
    );

    let bt_order = q1_overflow_distribution_order(Q1SpecDirection::BottomTop, 2);
    assert_eq!(
        bt_order,
        vec![Q1OverflowSide::LeftOrTop, Q1OverflowSide::RightOrBottom],
        "BT overflow slots should mirror TD side-lane alternation"
    );

    let rl_order = q1_overflow_distribution_order(Q1SpecDirection::RightLeft, 2);
    assert_eq!(
        rl_order,
        vec![Q1OverflowSide::LeftOrTop, Q1OverflowSide::RightOrBottom],
        "RL overflow slots should mirror LR side-lane alternation"
    );
}

#[test]
fn q1_q2_conflict_resolution_is_deterministic_and_documented() {
    let fixture = "q1_q2_conflict.mmd";
    let (diagram, geom) = layout_fixture_svg(fixture);
    let first = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let second = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    assert_eq!(
        first.edges.len(),
        second.edges.len(),
        "routed edge count should be deterministic"
    );

    let target_rect = geom
        .nodes
        .get("B")
        .expect("q1_q2_conflict fixture should contain node B")
        .rect;
    let conflict = first
        .edges
        .iter()
        .find(|edge| edge.from == "Q2" && edge.to == "B")
        .expect("fixture should contain Q2 -> B");

    assert!(
        conflict.is_backward,
        "Q2 -> B must be backward in unified preview layout for this fixture"
    );

    let source_rect = geom
        .nodes
        .get("Q2")
        .expect("q1_q2_conflict fixture should contain node Q2")
        .rect;
    let conflict_start = conflict
        .path
        .first()
        .copied()
        .expect("backward edge should have source endpoint");
    let conflict_start_face = point_on_target_face(source_rect, conflict_start);
    assert_eq!(
        conflict_start_face, "right",
        "Q2 -> B should depart from the canonical TD backward source lane (right face): start={conflict_start:?}, path={:?}",
        conflict.path
    );
    let source_face_margin = match conflict_start_face {
        "top" | "bottom" => {
            let source_left = source_rect.x;
            let source_right = source_rect.x + source_rect.width;
            (conflict_start.x - source_left).min(source_right - conflict_start.x)
        }
        "left" | "right" => {
            let source_top = source_rect.y;
            let source_bottom = source_rect.y + source_rect.height;
            (conflict_start.y - source_top).min(source_bottom - conflict_start.y)
        }
        _ => 0.0,
    };
    assert!(
        source_face_margin >= 5.0,
        "Q2 -> B source departure should stay away from source face borders (closer to center) to avoid cramped hooks: margin={source_face_margin}, source_rect={source_rect:?}, start={conflict_start:?}, path={:?}",
        conflict.path
    );
    let conflict_next = conflict
        .path
        .get(1)
        .copied()
        .expect("backward edge should have source support point");
    assert!(
        source_support_is_normal_to_attached_rect_face(source_rect, conflict_start, conflict_next),
        "Q2 -> B should leave the canonical source face on its outward normal axis: start={conflict_start:?}, next={conflict_next:?}, path={:?}",
        conflict.path
    );

    let conflict_end = *conflict
        .path
        .last()
        .expect("backward edge should have path endpoint");
    let conflict_face = point_on_target_face(target_rect, conflict_end);
    assert_eq!(
        conflict_face,
        "right",
        "Q2 -> B must keep TD backward canonical channel under fan-in pressure: end={conflict_end:?}, path={path:?}",
        conflict_end = conflict_end,
        path = conflict.path
    );
    let conflict_prev = conflict
        .path
        .get(conflict.path.len().saturating_sub(2))
        .copied()
        .expect("backward edge should have terminal support point");
    assert!(
        terminal_support_is_normal_to_attached_rect_face(target_rect, conflict_prev, conflict_end),
        "Q2 -> B should approach the canonical right face with a face-normal terminal segment: prev={conflict_prev:?}, end={conflict_end:?}, path={:?}",
        conflict.path
    );

    let incoming_to_b: Vec<_> = first.edges.iter().filter(|edge| edge.to == "B").collect();
    if std::env::var("MMDFLUX_DEBUG_Q1").is_ok_and(|v| v == "1") {
        for edge in &incoming_to_b {
            let end = edge
                .path
                .last()
                .copied()
                .expect("inbound edge should have endpoint");
            let end_face = point_on_target_face(target_rect, end);
            eprintln!(
                "edge {}->{} index={} backward={} end={:?} face={}",
                edge.from, edge.to, edge.index, edge.is_backward, end, end_face
            );
        }
    }
    assert_eq!(
        incoming_to_b.len(),
        6,
        "q1_q2_conflict should create exactly six inbound edges to B"
    );

    let right_face_count = incoming_to_b
        .iter()
        .filter(|edge| {
            let end = edge
                .path
                .last()
                .copied()
                .expect("inbound edge should have endpoint");
            point_on_target_face(target_rect, end) == "right"
        })
        .count();

    assert_eq!(
        right_face_count, 1,
        "only the backward conflict edge should occupy B's canonical right-backward lane: right_face_count={right_face_count}"
    );
}

#[test]
fn q1_q2_interaction_fixture_matrix_matches_documented_face_policies() {
    let q1_cases = [
        ("stacked_fan_in.mmd", "C", 0usize),
        ("fan_in.mmd", "D", 0usize),
        ("five_fan_in.mmd", "F", 1usize),
    ];

    for (fixture, target, min_side_faces) in q1_cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let routed = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: true,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let target_rect = geom
            .nodes
            .get(target)
            .unwrap_or_else(|| panic!("fixture {fixture} should contain target node {target}"))
            .rect;

        let inbound: Vec<_> = routed
            .edges
            .iter()
            .filter(|edge| edge.to == target && !edge.is_backward)
            .collect();
        assert!(
            !inbound.is_empty(),
            "fixture {fixture} should have inbound edges to {target}"
        );

        let interior_count = inbound
            .iter()
            .filter(|edge| {
                let end = edge
                    .path
                    .last()
                    .copied()
                    .expect("inbound edge should have endpoint");
                point_inside_rect(target_rect, end)
            })
            .count();
        assert_eq!(
            interior_count,
            0,
            "fixture {fixture} should not place inbound endpoints inside target interior under Q1 policy (target={target}, routed={:?})",
            inbound
                .iter()
                .map(|edge| (edge.from.as_str(), edge.path.clone()))
                .collect::<Vec<_>>()
        );

        let side_face_count = inbound
            .iter()
            .filter(|edge| {
                let end = edge
                    .path
                    .last()
                    .copied()
                    .expect("inbound edge should have endpoint");
                matches!(point_on_target_face(target_rect, end), "left" | "right")
            })
            .count();

        if min_side_faces == 0 {
            assert_eq!(
                side_face_count, 0,
                "fixture {fixture} should stay on primary TD incoming face without overflow (target={target})"
            );
        } else {
            assert!(
                side_face_count >= min_side_faces,
                "fixture {fixture} should spill overflow arrivals to side faces under Q1 policy: expected >= {min_side_faces}, actual={side_face_count}, target={target}"
            );
        }
    }

    let q2_cases = [
        ("multiple_cycles.mmd", "C", "A", "right", "right"),
        ("http_request.mmd", "Response", "Client", "right", "right"),
        ("git_workflow.mmd", "Remote", "Working", "bottom", "bottom"),
    ];

    for (fixture, from, to, expected_target_face, expected_source_face) in q2_cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let source_rect = geom
            .nodes
            .get(from)
            .unwrap_or_else(|| panic!("fixture {fixture} should contain source node {from}"))
            .rect;
        let target_rect = geom
            .nodes
            .get(to)
            .unwrap_or_else(|| panic!("fixture {fixture} should contain target node {to}"))
            .rect;

        let routed_q1_on = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: true,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let routed_q1_off = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q1_overflow: false,
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );

        for (mode_label, routed) in [("q1-on", &routed_q1_on), ("q1-off", &routed_q1_off)] {
            let edge = routed
                .edges
                .iter()
                .find(|edge| edge.from == from && edge.to == to)
                .unwrap_or_else(|| panic!("fixture {fixture} missing edge {from} -> {to}"));
            let start = edge
                .path
                .first()
                .copied()
                .expect("backward edge should have source endpoint");
            let start_face = point_on_target_face(source_rect, start);
            assert_eq!(
                start_face, expected_source_face,
                "fixture {fixture} edge {from}->{to} should keep canonical backward source face {expected_source_face} ({mode_label}); start={start:?}, path={:?}",
                edge.path
            );
            let end = edge
                .path
                .last()
                .copied()
                .expect("backward edge should have endpoint");
            let end_face = point_on_target_face(target_rect, end);
            assert_eq!(
                end_face, expected_target_face,
                "fixture {fixture} edge {from}->{to} should keep canonical backward target face {expected_target_face} ({mode_label}); end={end:?}, path={:?}",
                edge.path
            );
        }
    }
}

// -----------------------------------------------------------------------
// Task 3.1: Q4 rank-span long-skip RED regressions
// -----------------------------------------------------------------------

#[test]
fn q4_rank_span_toggle_pushes_known_long_skip_edges_toward_periphery_lane() {
    let cases = [
        ("double_skip.mmd", "A", "D"),
        ("skip_edge_collision.mmd", "A", "D"),
    ];

    for (fixture, from, to) in cases {
        let (diagram, geom) = layout_fixture_svg(fixture);
        let q4_off = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: false,
                ..RoutingPolicyToggles::all_enabled()
            },
        );
        let q4_on = route_graph_geometry_with_policies(
            &diagram,
            &geom,
            RoutingMode::UnifiedPreview,
            RoutingPolicyToggles {
                q4_rank_span_periphery: true,
                ..RoutingPolicyToggles::all_enabled()
            },
        );

        let edge_off = q4_off
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture {fixture} should contain edge {from} -> {to}"));
        let edge_on = q4_on
            .edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .unwrap_or_else(|| panic!("fixture {fixture} should contain edge {from} -> {to}"));

        let rank_span =
            edge_rank_span_from_dagre_hints(&geom, edge_on.index).unwrap_or_else(|| {
                panic!("fixture {fixture} edge {from}->{to} should have dagre rank metadata")
            });
        assert!(
            rank_span >= 2,
            "fixture contract invalid for Q4: {fixture} edge {from}->{to} must be a long-skip edge with rank_span>=2, got {rank_span}"
        );

        let detour_off = lateral_detour_from_endpoint_axis(&edge_off.path, geom.direction);
        let detour_on = lateral_detour_from_endpoint_axis(&edge_on.path, geom.direction);
        assert!(
            detour_on > detour_off + 0.5,
            "Q4 rank-span policy should increase periphery detour for {fixture} edge {from}->{to}: rank_span={rank_span}, detour_off={detour_off}, detour_on={detour_on}, off_path={:?}, on_path={:?}",
            edge_off.path,
            edge_on.path
        );
    }
}

#[test]
fn q5_styled_segment_monitor_reports_actionable_summary_for_routed_geometry() {
    let report = q5_style_segment_monitor_report_for_routed_geometry(
        &["edge_styles.mmd", "inline_edge_labels.mmd"],
        12.0,
    );
    assert!(
        report.scanned_styled_edges > 0,
        "Q5 monitor should scan at least one styled edge; report={report:?}"
    );
    assert!(
        !report.summary_line.is_empty(),
        "Q5 monitor should emit a stable summary line for CI parsing"
    );
    assert!(
        report.violations.is_empty(),
        "Q5 monitor detected styled-segment violations: {:#?}",
        report
    );
}
