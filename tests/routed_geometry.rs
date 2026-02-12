//! Contract tests for the routed geometry pipeline.
//!
//! Verifies that `route_graph_geometry` produces correct `RoutedGraphGeometry`
//! from engine-produced `GraphGeometry`.

use std::fs;
use std::path::Path;

use mmdflux::diagrams::flowchart::engine::{DagreLayoutEngine, MeasurementMode};
use mmdflux::diagrams::flowchart::geometry::*;
use mmdflux::diagrams::flowchart::routing::{route_graph_geometry, snap_path_to_grid_preview};
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

fn point_inside_rect(rect: FRect, point: FPoint) -> bool {
    let eps = 0.01;
    point.x > rect.x + eps
        && point.x < rect.x + rect.width - eps
        && point.y > rect.y + eps
        && point.y < rect.y + rect.height - eps
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
fn unified_route_contracts_prefer_outward_first_source_exits_for_selected_fixtures() {
    let td_cases: &[(&str, &[(&str, &str, f64)])] = &[
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
