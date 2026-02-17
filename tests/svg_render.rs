use std::collections::HashMap;
use std::fs;
use std::path::Path;

use mmdflux::diagram::{OutputFormat, PathDetail, RenderConfig, RoutingMode, SvgEdgePathStyle};
use mmdflux::diagrams::flowchart::engine::DagreLayoutEngine;
use mmdflux::diagrams::flowchart::routing::route_graph_geometry;
use mmdflux::graph::Stroke;
use mmdflux::registry::DiagramInstance;
use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{EngineConfig, GraphLayoutEngine, build_diagram, parse_flowchart};

/// Extract SVG node center x-coordinates by label text.
///
/// Scans the SVG for `<text ...>Label</text>` elements and returns a map of label -> x coordinate.
fn extract_node_x_positions(svg: &str) -> HashMap<String, f64> {
    let mut positions = HashMap::new();
    for line in svg.lines() {
        let line = line.trim();
        if !line.starts_with("<text") || !line.contains("dominant-baseline") {
            continue;
        }
        // Extract x value from x="..."
        let x_val = line.find("x=\"").and_then(|start| {
            let rest = &line[start + 3..];
            rest.find('"')
                .and_then(|end| rest[..end].parse::<f64>().ok())
        });
        // Extract text content between >...</text>
        let label = line.find("</text>").and_then(|end| {
            let before = &line[..end];
            before
                .rfind('>')
                .map(|start| before[start + 1..].to_string())
        });
        if let (Some(x), Some(label)) = (x_val, label)
            && !label.is_empty()
        {
            positions.insert(label, x);
        }
    }
    positions
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

fn parse_svg_text_position_and_value(line: &str) -> Option<(f64, f64, String)> {
    let line = line.trim();
    if !line.starts_with("<text") {
        return None;
    }
    let x = parse_attr_f64(line, "x")?;
    let y = parse_attr_f64(line, "y")?;
    let end = line.find("</text>")?;
    let before = &line[..end];
    let start = before.rfind('>')?;
    let value = before[start + 1..].to_string();
    Some((x, y, value))
}

fn extract_edge_label_positions(
    svg: &str,
    diagram: &mmdflux::Diagram,
) -> Vec<(String, (f64, f64))> {
    let mut remaining: HashMap<String, usize> = HashMap::new();
    for edge in &diagram.edges {
        if let Some(label) = &edge.label {
            *remaining.entry(label.clone()).or_insert(0) += 1;
        }
    }

    let mut labels = Vec::new();
    for line in svg.lines() {
        let Some((x, y, value)) = parse_svg_text_position_and_value(line) else {
            continue;
        };
        let Some(count) = remaining.get_mut(&value) else {
            continue;
        };
        if *count == 0 {
            continue;
        }
        *count -= 1;
        labels.push((value, (x, y)));
    }
    labels
}

fn euclidean_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn distance_point_to_svg_segment(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let seg_len_sq = dx * dx + dy * dy;
    if seg_len_sq <= 0.000_001 {
        return euclidean_distance(point, a);
    }

    let projection = ((point.0 - a.0) * dx + (point.1 - a.1) * dy) / seg_len_sq;
    let t = projection.clamp(0.0, 1.0);
    let closest = (a.0 + t * dx, a.1 + t * dy);
    euclidean_distance(point, closest)
}

fn distance_point_to_svg_path(point: (f64, f64), path: &[(f64, f64)]) -> f64 {
    if path.is_empty() {
        return f64::INFINITY;
    }
    if path.len() == 1 {
        return euclidean_distance(point, path[0]);
    }
    path.windows(2)
        .map(|segment| distance_point_to_svg_segment(point, segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn q3_svg_label_drift_failures(
    svg: &str,
    diagram: &mmdflux::Diagram,
    max_distance: f64,
) -> Vec<String> {
    let expected_labels = diagram
        .edges
        .iter()
        .filter(|edge| edge.label.is_some())
        .count();
    let label_positions = extract_edge_label_positions(svg, diagram);
    let paths: Vec<Vec<(f64, f64)>> = edge_path_data(svg)
        .iter()
        .map(|path| parse_svg_path_points(path))
        .collect();

    let mut failures = Vec::new();
    if label_positions.len() != expected_labels {
        failures.push(format!(
            "edge-label extraction mismatch: expected={expected_labels}, extracted={}",
            label_positions.len()
        ));
    }

    for (label, point) in label_positions {
        let drift = paths
            .iter()
            .map(|path| distance_point_to_svg_path(point, path))
            .fold(f64::INFINITY, f64::min);
        if drift > max_distance {
            failures.push(format!(
                "label {label:?} at ({:.2}, {:.2}) drift={drift:.2} exceeds {max_distance:.2}",
                point.0, point.1
            ));
        }
    }

    failures
}

fn total_svg_edge_segments(svg: &str) -> usize {
    edge_path_data(svg)
        .iter()
        .map(|d| parse_svg_path_points(d).len().saturating_sub(1))
        .sum()
}

fn svg_point_face(rect: (f64, f64, f64, f64), point: (f64, f64)) -> &'static str {
    let eps = 0.5;
    let (x, y, w, h) = rect;
    let left = x;
    let right = x + w;
    let top = y;
    let bottom = y + h;

    let on_right = (point.0 - right).abs() <= eps;
    let on_left = (point.0 - left).abs() <= eps;
    let on_top = (point.1 - top).abs() <= eps;
    let on_bottom = (point.1 - bottom).abs() <= eps;

    if on_right && point.1 > top + eps && point.1 < bottom - eps {
        "right"
    } else if on_left && point.1 > top + eps && point.1 < bottom - eps {
        "left"
    } else if on_top && point.0 > left + eps && point.0 < right - eps {
        "top"
    } else if on_bottom && point.0 > left + eps && point.0 < right - eps {
        "bottom"
    } else if on_right {
        "right"
    } else if on_left {
        "left"
    } else {
        "interior_or_corner"
    }
}

fn svg_terminal_approach_face(rect: (f64, f64, f64, f64), points: &[(f64, f64)]) -> &'static str {
    if points.is_empty() {
        return "interior_or_corner";
    }

    let end = *points.last().expect("path should have at least one point");
    let direct_face = svg_point_face(rect, end);
    if direct_face != "interior_or_corner" {
        return direct_face;
    }

    if points.len() < 2 {
        return direct_face;
    }

    let prev = points[points.len() - 2];
    let dx = end.0 - prev.0;
    let dy = end.1 - prev.1;
    let (x, y, w, h) = rect;
    let left = x;
    let right = x + w;
    let top = y;
    let bottom = y + h;
    const MARKER_PULLBACK_TOLERANCE: f64 = 6.0;

    // SVG marker pullback can leave the terminal path point just outside the
    // node border. Treat that as the attached face when the terminal tangent
    // points inward toward the node.
    if end.0 > right
        && end.0 - right <= MARKER_PULLBACK_TOLERANCE
        && end.1 >= top - MARKER_PULLBACK_TOLERANCE
        && end.1 <= bottom + MARKER_PULLBACK_TOLERANCE
        && dy.abs() <= 0.5
        && dx < 0.0
    {
        return "right";
    }
    if end.0 < left
        && left - end.0 <= MARKER_PULLBACK_TOLERANCE
        && end.1 >= top - MARKER_PULLBACK_TOLERANCE
        && end.1 <= bottom + MARKER_PULLBACK_TOLERANCE
        && dy.abs() <= 0.5
        && dx > 0.0
    {
        return "left";
    }
    if end.1 > bottom
        && end.1 - bottom <= MARKER_PULLBACK_TOLERANCE
        && end.0 >= left - MARKER_PULLBACK_TOLERANCE
        && end.0 <= right + MARKER_PULLBACK_TOLERANCE
        && dx.abs() <= 0.5
        && dy < 0.0
    {
        return "bottom";
    }
    if end.1 < top
        && top - end.1 <= MARKER_PULLBACK_TOLERANCE
        && end.0 >= left - MARKER_PULLBACK_TOLERANCE
        && end.0 <= right + MARKER_PULLBACK_TOLERANCE
        && dx.abs() <= 0.5
        && dy > 0.0
    {
        return "top";
    }

    if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            "right"
        } else if dx < 0.0 {
            "left"
        } else {
            "interior_or_corner"
        }
    } else if dy > 0.0 {
        "bottom"
    } else if dy < 0.0 {
        "top"
    } else {
        "interior_or_corner"
    }
}

fn svg_terminal_approach_face_relaxed(
    rect: (f64, f64, f64, f64),
    points: &[(f64, f64)],
) -> &'static str {
    if points.is_empty() {
        return "interior_or_corner";
    }

    let end = *points.last().expect("path should have at least one point");
    let direct_face = svg_point_face(rect, end);
    if direct_face != "interior_or_corner" {
        return direct_face;
    }
    if points.len() < 2 {
        return direct_face;
    }

    let prev = points[points.len() - 2];
    let dx = end.0 - prev.0;
    let dy = end.1 - prev.1;
    let (x, y, w, h) = rect;
    let left = x;
    let right = x + w;
    let top = y;
    let bottom = y + h;
    const MARKER_PULLBACK_TOLERANCE: f64 = 6.0;

    if end.0 > right
        && end.0 - right <= MARKER_PULLBACK_TOLERANCE
        && end.1 >= top - MARKER_PULLBACK_TOLERANCE
        && end.1 <= bottom + MARKER_PULLBACK_TOLERANCE
        && dx < 0.0
    {
        return "right";
    }
    if end.0 < left
        && left - end.0 <= MARKER_PULLBACK_TOLERANCE
        && end.1 >= top - MARKER_PULLBACK_TOLERANCE
        && end.1 <= bottom + MARKER_PULLBACK_TOLERANCE
        && dx > 0.0
    {
        return "left";
    }
    if end.1 > bottom
        && end.1 - bottom <= MARKER_PULLBACK_TOLERANCE
        && end.0 >= left - MARKER_PULLBACK_TOLERANCE
        && end.0 <= right + MARKER_PULLBACK_TOLERANCE
        && dy < 0.0
    {
        return "bottom";
    }
    if end.1 < top
        && top - end.1 <= MARKER_PULLBACK_TOLERANCE
        && end.0 >= left - MARKER_PULLBACK_TOLERANCE
        && end.0 <= right + MARKER_PULLBACK_TOLERANCE
        && dy > 0.0
    {
        return "top";
    }

    svg_terminal_approach_face(rect, points)
}

fn svg_source_departure_face(rect: (f64, f64, f64, f64), points: &[(f64, f64)]) -> &'static str {
    if points.is_empty() {
        return "interior_or_corner";
    }

    let start = points[0];
    let direct_face = svg_point_face(rect, start);
    if direct_face != "interior_or_corner" {
        return direct_face;
    }
    if points.len() < 2 {
        return direct_face;
    }

    let next = points[1];
    let dx = next.0 - start.0;
    let dy = next.1 - start.1;
    if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            "right"
        } else if dx < 0.0 {
            "left"
        } else {
            "interior_or_corner"
        }
    } else if dy > 0.0 {
        "bottom"
    } else if dy < 0.0 {
        "top"
    } else {
        "interior_or_corner"
    }
}

fn manhattan_segment_len(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn horizontal_span(points: &[(f64, f64)]) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    max_x - min_x
}

fn horizontal_detour_from_endpoint_axis(points: &[(f64, f64)]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let start = points[0];
    let end = points[points.len() - 1];
    let baseline_min = start.0.min(end.0);
    let baseline_max = start.0.max(end.0);
    let route_min = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let route_max = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    (baseline_min - route_min)
        .max(route_max - baseline_max)
        .max(0.0)
}

fn segment_axis(a: (f64, f64), b: (f64, f64)) -> Option<char> {
    if (a.0 - b.0).abs() < 0.001 && (a.1 - b.1).abs() >= 0.001 {
        Some('V')
    } else if (a.1 - b.1).abs() < 0.001 && (a.0 - b.0).abs() >= 0.001 {
        Some('H')
    } else {
        None
    }
}

fn trailing_segment_run_len(points: &[(f64, f64)], segment_count: usize) -> f64 {
    if points.len() < 2 || segment_count == 0 {
        return 0.0;
    }
    points
        .windows(2)
        .rev()
        .take(segment_count)
        .map(|segment| manhattan_segment_len(segment[0], segment[1]))
        .sum()
}

fn has_immediate_axis_backtrack(points: &[(f64, f64)]) -> bool {
    points.windows(3).any(|triple| {
        let a = triple[0];
        let b = triple[1];
        let c = triple[2];
        match (segment_axis(a, b), segment_axis(b, c)) {
            (Some('V'), Some('V')) => {
                let dy1 = b.1 - a.1;
                let dy2 = c.1 - b.1;
                dy1.abs() > 0.001 && dy2.abs() > 0.001 && dy1.signum() != dy2.signum()
            }
            (Some('H'), Some('H')) => {
                let dx1 = b.0 - a.0;
                let dx2 = c.0 - b.0;
                dx1.abs() > 0.001 && dx2.abs() > 0.001 && dx1.signum() != dx2.signum()
            }
            _ => false,
        }
    })
}

fn has_primary_axis_backtrack(points: &[(f64, f64)], direction: mmdflux::Direction) -> bool {
    const EPS: f64 = 0.001;
    if points.len() < 2 {
        return false;
    }

    match direction {
        mmdflux::Direction::TopDown => points.windows(2).any(|seg| seg[1].1 < seg[0].1 - EPS),
        mmdflux::Direction::BottomTop => points.windows(2).any(|seg| seg[1].1 > seg[0].1 + EPS),
        mmdflux::Direction::LeftRight => points.windows(2).any(|seg| seg[1].0 < seg[0].0 - EPS),
        mmdflux::Direction::RightLeft => points.windows(2).any(|seg| seg[1].0 > seg[0].0 + EPS),
    }
}

#[derive(Debug)]
struct Q5SvgStyleMonitorReport {
    scanned_styled_paths: usize,
    violations: Vec<String>,
    summary_line: String,
}

fn min_svg_segment_len(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|segment| {
            let dx = segment[1].0 - segment[0].0;
            let dy = segment[1].1 - segment[0].1;
            (dx * dx + dy * dy).sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

fn q5_style_segment_monitor_report_for_svg(
    fixtures: &[&str],
    min_segment_threshold: f64,
) -> Q5SvgStyleMonitorReport {
    let mut scanned_styled_paths = 0usize;
    let mut violations = Vec::new();

    for fixture in fixtures {
        let diagram = load_flowchart_fixture_diagram(fixture);
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = SvgEdgePathStyle::Linear;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        options.routing_policies = mmdflux::diagram::RoutingPolicyToggles {
            q5_style_min_segment: true,
            ..mmdflux::diagram::RoutingPolicyToggles::all_enabled()
        };
        let svg = render_svg(&diagram, &options);

        for line in svg.lines().map(str::trim) {
            if !line.starts_with("<path d=\"")
                || !(line.contains("marker-end=") || line.contains("marker-start="))
            {
                continue;
            }
            let is_styled =
                line.contains("stroke-dasharray") || line.contains("stroke-width=\"2.00\"");
            if !is_styled {
                continue;
            }

            let Some(start) = line.find("d=\"") else {
                continue;
            };
            let after = &line[start + 3..];
            let Some(end) = after.find('"') else {
                continue;
            };
            let points = parse_svg_path_points(&after[..end]);
            if points.len() < 2 {
                continue;
            }

            let min_segment = min_svg_segment_len(&points);
            scanned_styled_paths += 1;
            if min_segment < min_segment_threshold {
                violations.push(format!(
                    "{fixture} styled_path min_segment={min_segment:.2} threshold={min_segment_threshold:.2} path={points:?}"
                ));
            }
        }
    }

    Q5SvgStyleMonitorReport {
        scanned_styled_paths,
        summary_line: format!(
            "q5_monitor_svg scanned={} violations={} threshold={:.2}",
            scanned_styled_paths,
            violations.len(),
            min_segment_threshold
        ),
        violations,
    }
}

fn parse_attr_f64(line: &str, attr: &str) -> Option<f64> {
    let marker = format!("{attr}=\"");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    rest[..end].parse::<f64>().ok()
}

fn node_rect_for_label(svg: &str, label: &str) -> Option<(f64, f64, f64, f64)> {
    let (text_x, text_y) = svg.lines().find_map(|line| {
        if !line.contains("<text") || !line.contains(&format!(">{label}<")) {
            return None;
        }
        Some((parse_attr_f64(line, "x")?, parse_attr_f64(line, "y")?))
    })?;

    svg.lines().find_map(|line| {
        if !line.contains("<rect ")
            || !line.contains("stroke=\"#333\"")
            || !line.contains("fill=\"white\"")
        {
            return None;
        }
        let x = parse_attr_f64(line, "x")?;
        let y = parse_attr_f64(line, "y")?;
        let width = parse_attr_f64(line, "width")?;
        let height = parse_attr_f64(line, "height")?;
        let inside = text_x >= x && text_x <= x + width && text_y >= y && text_y <= y + height;
        if inside {
            Some((x, y, width, height))
        } else {
            None
        }
    })
}

fn edge_path_for_svg_order(
    diagram: &mmdflux::Diagram,
    svg: &str,
    edge_index: usize,
) -> Vec<(f64, f64)> {
    let mut visible_edge_indexes: Vec<usize> = diagram
        .edges
        .iter()
        .filter(|edge| edge.stroke != Stroke::Invisible)
        .map(|edge| edge.index)
        .collect();
    visible_edge_indexes.sort_unstable();

    let svg_position = visible_edge_indexes
        .iter()
        .position(|idx| *idx == edge_index)
        .expect("edge index should be visible in SVG");
    let paths = edge_path_data(svg);
    parse_svg_path_points(
        paths
            .get(svg_position)
            .expect("edge path should exist at visible edge position"),
    )
}

fn edge_path_d_for_svg_order(diagram: &mmdflux::Diagram, svg: &str, edge_index: usize) -> String {
    let mut visible_edge_indexes: Vec<usize> = diagram
        .edges
        .iter()
        .filter(|edge| edge.stroke != Stroke::Invisible)
        .map(|edge| edge.index)
        .collect();
    visible_edge_indexes.sort_unstable();

    let svg_position = visible_edge_indexes
        .iter()
        .position(|idx| *idx == edge_index)
        .expect("edge index should be visible in SVG");
    edge_path_data(svg)
        .get(svg_position)
        .expect("edge path should exist at visible edge position")
        .to_string()
}

fn load_flowchart_fixture_diagram(name: &str) -> mmdflux::Diagram {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join(name);
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    build_diagram(&flowchart)
}

fn render_fixture_svg(
    diagram: &mmdflux::Diagram,
    routing_mode: RoutingMode,
    style: SvgEdgePathStyle,
) -> String {
    let mut options = RenderOptions::default_svg();
    options.routing_mode = Some(routing_mode);
    options.svg.edge_path_style = style;
    options.path_detail = PathDetail::Full;
    render_svg(diagram, &options)
}

fn edge_index(diagram: &mmdflux::Diagram, from: &str, to: &str) -> usize {
    diagram
        .edges
        .iter()
        .find(|edge| edge.from == from && edge.to == to)
        .unwrap_or_else(|| panic!("expected edge {from} -> {to}"))
        .index
}

fn node_center_for_id(diagram: &mmdflux::Diagram, node_id: &str) -> (f64, f64) {
    let engine = DagreLayoutEngine::text();
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine
        .layout(diagram, &config)
        .expect("layout should succeed for center lookup");
    let node = geom
        .nodes
        .get(node_id)
        .unwrap_or_else(|| panic!("expected node `{node_id}` in layout geometry"));
    (
        node.rect.x + node.rect.width / 2.0,
        node.rect.y + node.rect.height / 2.0,
    )
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn render_svg_basic_flowchart_has_svg_root() {
    let input = "graph TD\nA[Start] --> B[End]\n";
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);

    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<text"));
    assert!(svg.contains("Start"));
    assert!(svg.contains("End"));
}

#[test]
fn svg_edge_path_style_parses_orthogonal() {
    assert_eq!(
        SvgEdgePathStyle::parse("orthogonal").unwrap(),
        SvgEdgePathStyle::Orthogonal
    );
}

#[test]
fn svg_orthogonal_mode_renders_axis_aligned_path_commands() {
    let input = "graph TD\nA --> B\nA --> C\n";
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    let svg = render_svg(&diagram, &options);

    assert!(!svg.contains("NaN"));

    let edge_paths = edge_path_data(&svg);
    assert!(
        !edge_paths.is_empty(),
        "expected edge path data in SVG output"
    );
    for d in edge_paths {
        let points = parse_svg_path_points(&d);
        assert!(
            points.windows(2).all(|segment| {
                (segment[0].0 - segment[1].0).abs() < 0.001
                    || (segment[0].1 - segment[1].1).abs() < 0.001
            }),
            "orthogonal path should be axis-aligned, got {d}"
        );
    }
}

#[test]
fn svg_compact_path_detail_sits_between_full_and_simplified_for_unified_preview() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);

    let render_with = |path_detail: PathDetail| {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = path_detail;
        render_svg(&diagram, &options)
    };

    let full = render_with(PathDetail::Full);
    let compact = render_with(PathDetail::Compact);
    let simplified = render_with(PathDetail::Simplified);

    let full_segments = total_svg_edge_segments(&full);
    let compact_segments = total_svg_edge_segments(&compact);
    let simplified_segments = total_svg_edge_segments(&simplified);

    assert!(
        full_segments >= compact_segments,
        "compact should not increase total segments: full={full_segments}, compact={compact_segments}"
    );
    assert!(
        full_segments != simplified_segments,
        "simplified should change segment density compared to full: full={full_segments}, simplified={simplified_segments}"
    );
}

#[test]
fn routed_svg_defaults_to_full_path_detail() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let mut default_options = RenderOptions::default_svg();
    default_options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    default_options.routing_mode = Some(RoutingMode::UnifiedPreview);
    let default_svg = render_svg(&diagram, &default_options);
    let default_points = edge_path_for_svg_order(&diagram, &default_svg, edge_index);

    let mut full_options = default_options;
    full_options.path_detail = PathDetail::Full;
    let full_svg = render_svg(&diagram, &full_options);
    let full_points = edge_path_for_svg_order(&diagram, &full_svg, edge_index);

    let mut simplified_options = full_options;
    simplified_options.path_detail = PathDetail::Simplified;
    let simplified_svg = render_svg(&diagram, &simplified_options);
    let simplified_points = edge_path_for_svg_order(&diagram, &simplified_svg, edge_index);

    assert_eq!(
        default_points, full_points,
        "default routed SVG path detail should match full output"
    );
    assert!(
        default_points.len() >= simplified_points.len(),
        "default full detail should not have fewer points than simplified: default={}, simplified={}",
        default_points.len(),
        simplified_points.len()
    );
    if default_points.len() == simplified_points.len() {
        assert!(
            default_points.len() <= 3,
            "default/simplified point counts should only match when the routed path is already minimal: default={}, simplified={}, points={:?}",
            default_points.len(),
            simplified_points.len(),
            default_points
        );
    }
}

const Q3_MAX_SVG_LABEL_DISTANCE_TO_ACTIVE_SEGMENT: f64 = 2.0;

#[test]
fn svg_orthogonal_unified_preview_labeled_edges_labels_remain_attached_to_active_segments() {
    let diagram = load_flowchart_fixture_diagram("labeled_edges.mmd");
    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    let failures =
        q3_svg_label_drift_failures(&svg, &diagram, Q3_MAX_SVG_LABEL_DISTANCE_TO_ACTIVE_SEGMENT);
    assert!(
        failures.is_empty(),
        "Q3 regression: labeled_edges rendered off-path edge labels:\n{}",
        failures.join("\n")
    );
}

#[test]
fn svg_orthogonal_unified_preview_inline_label_flowchart_labels_remain_attached_to_active_segments()
{
    let diagram = load_flowchart_fixture_diagram("inline_label_flowchart.mmd");
    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    let failures =
        q3_svg_label_drift_failures(&svg, &diagram, Q3_MAX_SVG_LABEL_DISTANCE_TO_ACTIVE_SEGMENT);
    assert!(
        failures.is_empty(),
        "Q3 regression: inline_label_flowchart rendered off-path edge labels:\n{}",
        failures.join("\n")
    );
}

#[test]
fn path_detail_monotonicity_holds_full_compact_simplified() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let render_for = |path_detail: PathDetail| {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = path_detail;
        let svg = render_svg(&diagram, &options);
        edge_path_for_svg_order(&diagram, &svg, edge_index).len()
    };

    let full = render_for(PathDetail::Full);
    let compact = render_for(PathDetail::Compact);
    let simplified = render_for(PathDetail::Simplified);

    assert!(
        full >= compact && compact >= simplified,
        "path-detail monotonicity violated for SVG: full={full}, compact={compact}, simplified={simplified}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_preserves_clear_terminal_stem_into_arrowhead() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_index);
    assert!(
        points.len() >= 2,
        "expected routed SVG points for Bmid -> F"
    );

    let prev = points[points.len() - 2];
    let end = points[points.len() - 1];
    let axis = segment_axis(prev, end).expect("terminal segment should be axis-aligned");
    let stem_len = manhattan_segment_len(prev, end);
    assert_eq!(
        axis, 'V',
        "Bmid -> F terminal segment should be vertical in TD layout: {points:?}"
    );
    assert!(
        end.1 > prev.1,
        "Bmid -> F terminal segment should point downward into F (arrow-support direction), got prev={prev:?}, end={end:?}, points={points:?}"
    );
    assert!(
        !has_immediate_axis_backtrack(&points),
        "Bmid -> F path should not include an immediate axis backtrack near the elbow: {points:?}"
    );
    assert!(
        stem_len >= 8.0,
        "Bmid -> F terminal stem should retain extra buffer beyond arrow pullback (>= 8px), got {stem_len} with {points:?}"
    );

    let (_fx, fy, _fw, _fh) = node_rect_for_label(&svg, "f").expect("expected SVG rect for node f");
    let expected_endpoint_y = fy - 4.0;
    assert!(
        (end.1 - expected_endpoint_y).abs() <= 0.5,
        "Bmid -> F endpoint should be pulled back so arrow tip touches F border: endpoint_y={}, expected_y={} (f_top={fy}) points={points:?}",
        end.1,
        expected_endpoint_y
    );
}

#[test]
fn svg_orthogonal_unified_preview_does_not_add_short_staircase_jogs_after_adjustment() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);

    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let engine = DagreLayoutEngine::text();
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine
        .layout(&diagram, &config)
        .expect("layout should succeed");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let routed_edge = routed
        .edges
        .iter()
        .find(|edge| edge.index == edge_index)
        .expect("unified routed edge should exist");
    let routed_segments = routed_edge.path.len().saturating_sub(1);

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_index);
    let svg_segments = points.len().saturating_sub(1);
    assert!(
        svg_segments <= routed_segments + 1,
        "SVG conversion should not add staircase jogs for Bmid -> F: routed_segments={routed_segments}, svg_segments={svg_segments}, svg_points={points:?}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_multiple_cycles_avoids_tiny_terminal_staircase_jogs() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multiple_cycles.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edges = [
        edge_index(&diagram, "C", "A"),
        edge_index(&diagram, "C", "B"),
    ];

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    for edge_idx in edges {
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        assert!(
            points.len() >= 3,
            "multiple_cycles edge should keep at least one terminal elbow in orthogonal mode: {points:?}"
        );
        let terminal_support =
            manhattan_segment_len(points[points.len() - 2], points[points.len() - 1]);
        let pre_terminal =
            manhattan_segment_len(points[points.len() - 3], points[points.len() - 2]);
        assert!(
            terminal_support >= 10.0 && pre_terminal >= 10.0,
            "multiple_cycles orthogonal tail should avoid tiny terminal staircase jogs; terminal_support={terminal_support}, pre_terminal={pre_terminal}, points={points:?}"
        );
    }
}

#[test]
fn svg_orthogonal_unified_preview_nested_subgraph_edge_avoids_large_lateral_detour() {
    let diagram = load_flowchart_fixture_diagram("nested_subgraph_edge.mmd");
    let edges = [
        edge_index(&diagram, "Client", "Server1"),
        edge_index(&diagram, "Server1", "Monitoring"),
    ];

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    for edge_idx in edges {
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        let span = horizontal_span(&points);
        assert!(
            span <= 20.0,
            "nested_subgraph_edge orthogonal path should not make a large horizontal detour: span={span}, points={points:?}"
        );
    }
}

#[test]
fn svg_basis_unified_preview_ampersand_avoids_tiny_terminal_hook_before_arrow() {
    let diagram = load_flowchart_fixture_diagram("ampersand.mmd");
    let merge_in_edges = [
        edge_index(&diagram, "A", "C"),
        edge_index(&diagram, "B", "C"),
    ];

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Basis;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    for edge_idx in merge_in_edges {
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        assert!(
            points.len() >= 2,
            "ampersand edge should contain at least two path points: {points:?}"
        );
        let terminal = manhattan_segment_len(points[points.len() - 2], points[points.len() - 1]);
        assert!(
            terminal >= 3.5,
            "basis unified terminal segment should avoid tiny hook before marker; terminal={terminal}, points={points:?}"
        );
    }
}

#[test]
fn svg_non_orth_unified_preview_backward_edges_terminal_tangent_points_toward_target() {
    let cases = [
        ("decision.mmd", "D", "A"),
        ("git_workflow.mmd", "Remote", "Working"),
        ("http_request.mmd", "Response", "Client"),
        ("labeled_edges.mmd", "Error", "Setup"),
    ];
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for (fixture_name, from, to) in cases {
        let diagram = load_flowchart_fixture_diagram(fixture_name);
        let edge_idx = edge_index(&diagram, from, to);
        let target_center = node_center_for_id(&diagram, to);

        for style in styles {
            let mut options = RenderOptions::default_svg();
            options.svg.edge_path_style = style;
            options.routing_mode = Some(RoutingMode::UnifiedPreview);
            options.path_detail = PathDetail::Full;
            let svg = render_svg(&diagram, &options);
            let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);

            assert!(
                points.len() >= 2,
                "{fixture_name} {from}->{to} should have at least two SVG path points for {style:?}: {points:?}"
            );

            let prev = points[points.len() - 2];
            let end = points[points.len() - 1];
            let toward_target = distance(end, target_center) < distance(prev, target_center);
            assert!(
                toward_target,
                "{fixture_name} {from}->{to} terminal tangent should point toward target center for {style:?}: prev={prev:?}, end={end:?}, target_center={target_center:?}, points={points:?}"
            );
        }
    }
}

#[test]
fn svg_linear_unified_preview_avoids_primary_axis_backtrack_for_bmid_to_f() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Linear;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_index);

    assert!(
        !has_primary_axis_backtrack(&points, diagram.direction),
        "Bmid -> F should not backtrack along TD primary axis in linear SVG: {points:?}"
    );
}

#[test]
fn svg_basis_unified_preview_avoids_primary_axis_backtrack_for_bmid_to_f() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Basis;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_index);

    assert!(
        !has_primary_axis_backtrack(&points, diagram.direction),
        "Bmid -> F should not backtrack along TD primary axis in basis SVG: {points:?}"
    );
}

#[test]
fn svg_rounded_unified_preview_avoids_primary_axis_backtrack_for_bmid_to_f() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Rounded;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_index);

    assert!(
        !has_primary_axis_backtrack(&points, diagram.direction),
        "Bmid -> F should not backtrack along TD primary axis in rounded SVG: {points:?}"
    );
}

#[test]
fn svg_non_orth_unified_preview_keeps_endpoint_pulled_back_for_visible_arrow_tip() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("multi_subgraph_direction_override.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_index = diagram
        .edges
        .iter()
        .find(|edge| edge.from == "Bmid" && edge.to == "F")
        .expect("fixture should contain edge Bmid -> F")
        .index;

    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);
        let points = edge_path_for_svg_order(&diagram, &svg, edge_index);
        let end = points
            .last()
            .copied()
            .expect("Bmid -> F should have SVG path points");
        let (_fx, fy, _fw, _fh) =
            node_rect_for_label(&svg, "f").expect("expected SVG rect for node f");
        let expected_endpoint_y = fy - 4.0;

        assert!(
            (end.1 - expected_endpoint_y).abs() <= 0.5,
            "non-orth {style:?} endpoint should be pulled back so arrow tip lands on F border: endpoint_y={}, expected_y={} (f_top={fy}) points={points:?}",
            end.1,
            expected_endpoint_y
        );
    }
}

#[test]
fn svg_non_orth_unified_preview_fan_in_lr_terminal_arrowheads_do_not_end_inside_target() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("fan_in_lr.mmd");
    let input = fs::read_to_string(fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);

    let top_edge = edge_index(&diagram, "A", "D");
    let bottom_edge = edge_index(&diagram, "C", "D");
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);
        let (tx, ty, tw, th) =
            node_rect_for_label(&svg, "Target").expect("target rect should exist");

        for edge_idx in [top_edge, bottom_edge] {
            let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
            let end = points
                .last()
                .copied()
                .expect("edge should have path points");
            let inside = end.0 > tx + 0.5
                && end.0 < tx + tw - 0.5
                && end.1 > ty + 0.5
                && end.1 < ty + th - 0.5;

            assert!(
                !inside,
                "fan_in_lr edge endpoint should not be inside target interior for {style:?}: end={end:?}, target_rect=({tx}, {ty}, {tw}, {th}), points={points:?}"
            );
        }
    }
}

#[test]
fn svg_non_orth_unified_preview_backward_edges_keep_terminal_arrowheads_visible() {
    let cases = [
        ("decision.mmd", "D", "A", "Start"),
        ("labeled_edges.mmd", "Error", "Setup", "Setup"),
        ("http_request.mmd", "Response", "Client", "Client"),
        ("complex.mmd", "E", "A", "Input"),
    ];
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for (fixture_name, from, to, target_label) in cases {
        let diagram = load_flowchart_fixture_diagram(fixture_name);
        let edge_idx = edge_index(&diagram, from, to);

        for style in styles {
            let mut options = RenderOptions::default_svg();
            options.svg.edge_path_style = style;
            options.routing_mode = Some(RoutingMode::UnifiedPreview);
            options.path_detail = PathDetail::Full;
            let svg = render_svg(&diagram, &options);
            let (tx, ty, tw, th) = node_rect_for_label(&svg, target_label)
                .unwrap_or_else(|| panic!("target rect should exist for {target_label}"));
            let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
            let end = points
                .last()
                .copied()
                .expect("edge should have path points");
            let inside = end.0 > tx + 0.5
                && end.0 < tx + tw - 0.5
                && end.1 > ty + 0.5
                && end.1 < ty + th - 0.5;

            assert!(
                !inside,
                "{fixture_name} {from}->{to} endpoint should stay outside target interior for {style:?}: end={end:?}, target_rect=({tx}, {ty}, {tw}, {th}), points={points:?}"
            );
        }
    }
}

#[test]
fn svg_non_orth_unified_preview_backward_in_subgraph_avoids_tiny_terminal_tail_hooks() {
    const MIN_TERMINAL_SUPPORT: f64 = 3.5;
    let diagram = load_flowchart_fixture_diagram("backward_in_subgraph.mmd");
    let edge_idx = edge_index(&diagram, "B", "A");
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        assert!(
            points.len() >= 2,
            "backward_in_subgraph B->A should have at least two points for {style:?}: {points:?}"
        );

        let rect = node_rect_for_label(&svg, "Node").expect("target rect should exist for Node");
        let end_face = svg_terminal_approach_face_relaxed(rect, &points);
        assert_eq!(
            end_face, "bottom",
            "backward_in_subgraph B->A should enter Node on bottom face for {style:?}: points={points:?}"
        );

        let terminal_support =
            manhattan_segment_len(points[points.len() - 2], points[points.len() - 1]);
        let min_terminal_support = if matches!(style, SvgEdgePathStyle::Basis) {
            // Basis rendering intentionally tapers the final linear cap segment.
            1.0
        } else {
            MIN_TERMINAL_SUPPORT
        };
        assert!(
            terminal_support >= min_terminal_support,
            "backward_in_subgraph B->A should avoid tiny terminal tail hooks before the arrowhead for {style:?}: terminal_support={terminal_support}, min={min_terminal_support}, points={points:?}"
        );
    }
}

#[test]
fn svg_orthogonal_unified_preview_complex_backward_edge_keeps_arrowhead_visible() {
    let diagram = load_flowchart_fixture_diagram("complex.mmd");
    let edge_idx = edge_index(&diagram, "E", "A");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let (tx, ty, tw, th) =
        node_rect_for_label(&svg, "Input").expect("target rect should exist for Input");
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
    let end = points
        .last()
        .copied()
        .expect("complex E->A should have SVG path points");

    let ends_on_target_border_or_inside =
        end.0 >= tx - 0.5 && end.0 <= tx + tw + 0.5 && end.1 >= ty - 0.5 && end.1 <= ty + th + 0.5;
    assert!(
        !ends_on_target_border_or_inside,
        "complex E->A orthogonal endpoint should be pulled outside the Input node envelope so arrowhead remains visible; end={end:?}, target_rect=({tx}, {ty}, {tw}, {th}), points={points:?}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_complex_backward_edge_terminal_tangent_points_toward_target() {
    let diagram = load_flowchart_fixture_diagram("complex.mmd");
    let edge_idx = edge_index(&diagram, "E", "A");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let rect = node_rect_for_label(&svg, "Input").expect("target rect should exist for Input");
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
    assert!(
        points.len() >= 2,
        "complex E->A should have at least two path points in orthogonal mode: {points:?}"
    );
    let prev = points[points.len() - 2];
    let end = points[points.len() - 1];
    let end_face = svg_terminal_approach_face_relaxed(rect, &points);

    match end_face {
        "right" => assert!(
            (end.1 - prev.1).abs() <= 0.5 && end.0 < prev.0,
            "complex E->A orthogonal terminal tangent on right face should point left into Input; prev={prev:?}, end={end:?}, points={points:?}"
        ),
        "left" => assert!(
            (end.1 - prev.1).abs() <= 0.5 && end.0 > prev.0,
            "complex E->A orthogonal terminal tangent on left face should point right into Input; prev={prev:?}, end={end:?}, points={points:?}"
        ),
        "top" => assert!(
            (end.0 - prev.0).abs() <= 0.5 && end.1 > prev.1,
            "complex E->A orthogonal terminal tangent on top face should point down into Input; prev={prev:?}, end={end:?}, points={points:?}"
        ),
        "bottom" => assert!(
            (end.0 - prev.0).abs() <= 0.5 && end.1 < prev.1,
            "complex E->A orthogonal terminal tangent on bottom face should point up into Input; prev={prev:?}, end={end:?}, points={points:?}"
        ),
        other => panic!(
            "complex E->A orthogonal terminal approach should resolve to a concrete Input face, got {other}; prev={prev:?}, end={end:?}, points={points:?}"
        ),
    }
}

#[test]
fn svg_unified_preview_complex_top_diamond_loop_avoids_single_edge_micro_jogs() {
    const MIN_SEGMENT_LEN: f64 = 6.0;

    let diagram = load_flowchart_fixture_diagram("complex.mmd");
    let mut linear_options = RenderOptions::default_svg();
    linear_options.svg.edge_path_style = SvgEdgePathStyle::Linear;
    linear_options.routing_mode = Some(RoutingMode::UnifiedPreview);
    linear_options.path_detail = PathDetail::Full;
    let linear_svg = render_svg(&diagram, &linear_options);

    for (from, to) in [("C", "E"), ("E", "A")] {
        let edge_idx = edge_index(&diagram, from, to);
        let points = edge_path_for_svg_order(&diagram, &linear_svg, edge_idx);
        assert!(
            points.len() >= 2,
            "complex {from}->{to} should emit at least one segment in linear mode: {points:?}"
        );
        let min_segment = min_svg_segment_len(&points);
        assert!(
            min_segment >= MIN_SEGMENT_LEN,
            "complex {from}->{to} should avoid tiny elbow jog segments in unified linear mode (min {MIN_SEGMENT_LEN}): min_segment={min_segment}, points={points:?}"
        );
    }

    let mut orth_options = RenderOptions::default_svg();
    orth_options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    orth_options.routing_mode = Some(RoutingMode::UnifiedPreview);
    orth_options.path_detail = PathDetail::Full;
    let orth_svg = render_svg(&diagram, &orth_options);
    let backward_idx = edge_index(&diagram, "E", "A");
    let backward_points = edge_path_for_svg_order(&diagram, &orth_svg, backward_idx);
    assert!(
        !has_immediate_axis_backtrack(&backward_points),
        "complex E->A should not include an immediate axis backtrack in orthogonal unified mode: {backward_points:?}"
    );
}

#[test]
fn svg_non_orth_unified_preview_complex_backward_edge_avoids_center_biased_input_attachment() {
    const MIN_CENTER_OFFSET: f64 = 12.0;

    let diagram = load_flowchart_fixture_diagram("complex.mmd");
    let edge_idx = edge_index(&diagram, "E", "A");
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);

        let rect = node_rect_for_label(&svg, "Input").expect("target rect should exist for Input");
        let center_x = rect.0 + rect.2 / 2.0;
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        let end = *points
            .last()
            .expect("complex E->A should have path points for non-orth style");
        let end_face = svg_terminal_approach_face_relaxed(rect, &points);

        if end_face == "bottom" || end_face == "top" {
            let center_offset = (end.0 - center_x).abs();
            assert!(
                center_offset >= MIN_CENTER_OFFSET,
                "complex E->A {style:?} should avoid center-biased vertical attachment on Input when approaching from a backward top-loop lane; end={end:?}, center_x={center_x}, center_offset={center_offset}, min_offset={MIN_CENTER_OFFSET}, points={points:?}"
            );
        }
    }
}

#[test]
fn svg_linear_unified_preview_ci_pipeline_diamond_exits_avoid_extra_elbow_jogs() {
    let diagram = load_flowchart_fixture_diagram("ci_pipeline.mmd");
    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Linear;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    for (from, to) in [("Deploy", "Staging"), ("Deploy", "Prod")] {
        let edge_idx = edge_index(&diagram, from, to);
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        assert!(
            points.len() >= 3,
            "ci_pipeline {from}->{to} should have at least three points for elbow checks: {points:?}"
        );
        let first = points[0];
        let second = points[1];
        let third = points[2];
        let first_axis = segment_axis(first, second);
        let second_axis = segment_axis(second, third);
        if points.len() >= 4 {
            let fourth = points[3];
            let third_axis = segment_axis(third, fourth);
            assert!(
                !(first_axis.is_none() && second_axis.is_some() && third_axis.is_some()),
                "ci_pipeline {from}->{to} should avoid extra elbow jogs right after Deploy? in unified linear mode (prefer direct diagonal-to-lane): points={points:?}"
            );
        }
    }
}

#[test]
fn svg_unified_preview_backward_edges_preserve_selected_non_orth_style() {
    let diagram = load_flowchart_fixture_diagram("simple_cycle.mmd");
    let edge_idx = edge_index(&diagram, "C", "A");

    let basis_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Basis,
    );
    let basis_d = edge_path_d_for_svg_order(&diagram, &basis_svg, edge_idx);
    assert!(
        basis_d.contains('C'),
        "simple_cycle C->A backward edge should use basis-style cubic segments in unified preview: d={basis_d}"
    );

    let rounded_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Rounded,
    );
    let rounded_d = edge_path_d_for_svg_order(&diagram, &rounded_svg, edge_idx);
    assert!(
        rounded_d.contains('Q'),
        "simple_cycle C->A backward edge should use rounded corner commands in unified preview: d={rounded_d}"
    );
    let rounded_points = edge_path_for_svg_order(&diagram, &rounded_svg, edge_idx);
    assert!(
        rounded_points.len() >= 2,
        "simple_cycle C->A backward edge should expose at least two rounded points: {rounded_points:?}"
    );
    let rounded_prev = rounded_points[rounded_points.len() - 2];
    let rounded_end = rounded_points[rounded_points.len() - 1];
    let rounded_dx = (rounded_end.0 - rounded_prev.0).abs();
    let rounded_dy = (rounded_end.1 - rounded_prev.1).abs();
    assert!(
        rounded_dx <= 0.5 || rounded_dy <= 0.5,
        "simple_cycle C->A rounded backward terminal approach should stay axis-aligned (no diagonal terminal tail): prev={rounded_prev:?}, end={rounded_end:?}, d={rounded_d}"
    );

    let linear_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Linear,
    );
    let linear_d = edge_path_d_for_svg_order(&diagram, &linear_svg, edge_idx);
    assert!(
        !linear_d.contains('Q') && !linear_d.contains('C'),
        "simple_cycle C->A backward edge should remain polyline in linear mode: d={linear_d}"
    );
    let linear_points = edge_path_for_svg_order(&diagram, &linear_svg, edge_idx);
    assert!(
        linear_points.len() >= 2,
        "simple_cycle C->A backward edge should expose at least two linear points: {linear_points:?}"
    );
    let linear_prev = linear_points[linear_points.len() - 2];
    let linear_end = linear_points[linear_points.len() - 1];
    let linear_dx = (linear_end.0 - linear_prev.0).abs();
    let linear_dy = (linear_end.1 - linear_prev.1).abs();
    assert!(
        linear_dx <= 0.5 || linear_dy <= 0.5,
        "simple_cycle C->A linear backward terminal approach should stay axis-aligned (no diagonal terminal tail): prev={linear_prev:?}, end={linear_end:?}, d={linear_d}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_label_spacing_keeps_td_departure_stems_from_source() {
    let diagram = load_flowchart_fixture_diagram("label_spacing.mmd");
    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);

    for (from, to) in [("A", "B"), ("A", "C")] {
        let edge_idx = edge_index(&diagram, from, to);
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        assert!(
            points.len() >= 2,
            "label_spacing {from}->{to} should expose at least two points in orthogonal mode: {points:?}"
        );
        let start = points[0];
        let next = points[1];
        assert!(
            (next.0 - start.0).abs() <= 0.5 && (next.1 - start.1).abs() > 0.5,
            "label_spacing {from}->{to} unified orthogonal route should depart A along TD primary axis (vertical stem first), not lateral-first: start={start:?}, next={next:?}, points={points:?}"
        );
    }
}

#[test]
fn svg_unified_preview_multi_edge_labeled_preserves_parallel_lane_shape() {
    const MAX_LANE_DETOUR_LOSS_FROM_FULL: f64 = 2.0;

    let diagram = load_flowchart_fixture_diagram("multi_edge_labeled.mmd");
    let mut full_options = RenderOptions::default_svg();
    full_options.svg.edge_path_style = SvgEdgePathStyle::Linear;
    full_options.routing_mode = Some(RoutingMode::FullCompute);
    full_options.path_detail = PathDetail::Full;

    let mut unified_options = RenderOptions::default_svg();
    unified_options.svg.edge_path_style = SvgEdgePathStyle::Linear;
    unified_options.routing_mode = Some(RoutingMode::UnifiedPreview);
    unified_options.path_detail = PathDetail::Full;

    let full_svg = render_svg(&diagram, &full_options);
    let unified_svg = render_svg(&diagram, &unified_options);

    let mut ab_edge_indexes: Vec<usize> = diagram
        .edges
        .iter()
        .filter(|edge| edge.from == "A" && edge.to == "B")
        .map(|edge| edge.index)
        .collect();
    ab_edge_indexes.sort_unstable();
    assert_eq!(
        ab_edge_indexes.len(),
        2,
        "fixture contract invalid: multi_edge_labeled should keep exactly two A->B edges"
    );

    for edge_idx in ab_edge_indexes {
        let full_points = edge_path_for_svg_order(&diagram, &full_svg, edge_idx);
        let unified_points = edge_path_for_svg_order(&diagram, &unified_svg, edge_idx);
        let full_detour = horizontal_detour_from_endpoint_axis(&full_points);
        let unified_detour = horizontal_detour_from_endpoint_axis(&unified_points);

        assert!(
            full_detour >= 8.0,
            "fixture contract changed unexpectedly: full-compute SVG A->B edge {edge_idx} should keep bowed lane detour (>= 8): detour={full_detour}, points={full_points:?}"
        );
        assert!(
            unified_detour + MAX_LANE_DETOUR_LOSS_FROM_FULL >= full_detour,
            "unified-preview SVG A->B edge {edge_idx} should preserve lane-shape detour close to full-compute (loss <= {MAX_LANE_DETOUR_LOSS_FROM_FULL}): full_detour={full_detour}, unified_detour={unified_detour}, full_points={full_points:?}, unified_points={unified_points:?}"
        );
    }
}

#[test]
fn svg_non_orth_unified_preview_q1_q2_conflict_keeps_backward_canonical_face() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flowchart")
        .join("q1_q2_conflict.mmd");
    let input = fs::read_to_string(&fixture).expect("fixture should load");
    let flowchart = parse_flowchart(&input).expect("fixture should parse");
    let diagram = build_diagram(&flowchart);
    let edge_idx = edge_index(&diagram, "Q2", "B");

    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    let mut rect = None;

    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);
        let (tx, ty, tw, th) = match rect {
            Some(rect) => rect,
            None => {
                let parsed = node_rect_for_label(&svg, "Target")
                    .expect("expected target rect for q1_q2_conflict fixture");
                rect = Some(parsed);
                parsed
            }
        };
        let rect = (tx, ty, tw, th);

        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        let end = points
            .last()
            .copied()
            .expect("backward edge should have path points");
        let end_face = svg_terminal_approach_face_relaxed(rect, &points);

        assert_eq!(
            end_face, "bottom",
            "Q2-conflict edge should follow TD parity target entry (bottom face) for {style:?}: end={end:?}, rect={rect:?}, points={points:?}"
        );
    }
}

#[test]
fn svg_basis_unified_preview_q1_q2_conflict_avoids_tiny_terminal_hook_before_arrow() {
    let diagram = load_flowchart_fixture_diagram("q1_q2_conflict.mmd");
    let edge_idx = edge_index(&diagram, "Q2", "B");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Basis;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);

    assert!(
        points.len() >= 3,
        "q1_q2_conflict backward edge should keep at least one terminal support segment in basis mode: points={points:?}"
    );

    let terminal = manhattan_segment_len(points[points.len() - 2], points[points.len() - 1]);
    let trailing_run = trailing_segment_run_len(&points, 4);
    assert!(
        terminal >= 1.0 && trailing_run >= 6.0,
        "basis unified backward terminal hook should avoid tiny elbow before marker; terminal={terminal}, trailing_run={trailing_run}, points={points:?}"
    );
}

#[test]
fn svg_non_orth_unified_preview_q1_q2_conflict_preserves_lower_terminal_lane() {
    let diagram = load_flowchart_fixture_diagram("q1_q2_conflict.mmd");
    let edge_idx = edge_index(&diagram, "Q2", "B");
    let styles = [
        SvgEdgePathStyle::Linear,
        SvgEdgePathStyle::Rounded,
        SvgEdgePathStyle::Basis,
    ];

    let mut rect = None;
    for style in styles {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = style;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        let svg = render_svg(&diagram, &options);
        let (_tx, ty, _tw, th) = match rect {
            Some(rect) => rect,
            None => {
                let parsed = node_rect_for_label(&svg, "Target")
                    .expect("expected target rect for q1_q2_conflict fixture");
                rect = Some(parsed);
                parsed
            }
        };
        let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
        let end = points
            .last()
            .copied()
            .expect("q1_q2_conflict backward edge should have path points");

        assert!(
            end.1 >= ty + th - 2.0,
            "Q2-conflict non-orth terminal lane should stay near lower right-face channel for {style:?}: end={end:?}, target_rect_y={ty}, target_rect_h={th}, points={points:?}"
        );
    }
}

#[test]
fn svg_orthogonal_unified_preview_q1_q2_conflict_avoids_terminal_axis_backtrack() {
    let diagram = load_flowchart_fixture_diagram("q1_q2_conflict.mmd");
    let edge_idx = edge_index(&diagram, "Q2", "B");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);

    assert!(
        !has_immediate_axis_backtrack(&points),
        "q1_q2_conflict orthogonal backward edge should not axis-backtrack near the terminal hook; points={points:?}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_decision_backward_edge_avoids_source_elbow_axis_backtrack() {
    let diagram = load_flowchart_fixture_diagram("decision.mmd");
    let edge_idx = edge_index(&diagram, "D", "A");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);

    assert!(
        !has_immediate_axis_backtrack(&points),
        "decision D->A orthogonal backward edge should avoid source-elbow axis backtrack spikes; points={points:?}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_decision_backward_edge_keeps_bottom_target_face_parity() {
    let diagram = load_flowchart_fixture_diagram("decision.mmd");
    let edge_idx = edge_index(&diagram, "D", "A");

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
    let start_rect =
        node_rect_for_label(&svg, "Start").expect("missing Start rect in decision fixture");
    let target_face = svg_terminal_approach_face_relaxed(start_rect, &points);

    assert_eq!(
        target_face, "bottom",
        "decision D->A orthogonal backward edge should keep TD backward target-entry parity on Start bottom face; face={target_face}, points={points:?}"
    );
}

#[test]
fn svg_orthogonal_unified_preview_decision_backward_edge_preserves_routed_terminal_lane_x() {
    const MAX_TERMINAL_LANE_X_DRIFT: f64 = 8.0;

    let diagram = load_flowchart_fixture_diagram("decision.mmd");
    let edge_idx = edge_index(&diagram, "D", "A");

    let measurement_mode = mmdflux::diagrams::flowchart::engine::MeasurementMode::for_format(
        OutputFormat::Svg,
        &RenderConfig::default(),
    );
    let engine = DagreLayoutEngine::with_mode(measurement_mode);
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine
        .layout(&diagram, &config)
        .expect("layout should succeed for decision fixture");
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);
    let routed_edge = routed
        .edges
        .iter()
        .find(|edge| edge.from == "D" && edge.to == "A")
        .expect("decision fixture should contain backward edge D -> A");
    assert!(
        routed_edge.path.len() >= 3,
        "routed decision D->A should keep at least one terminal support segment: path={:?}",
        routed_edge.path
    );
    let routed_terminal_support = routed_edge.path[routed_edge.path.len() - 2];

    let mut options = RenderOptions::default_svg();
    options.svg.edge_path_style = SvgEdgePathStyle::Orthogonal;
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;
    let svg = render_svg(&diagram, &options);
    let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
    assert!(
        points.len() >= 3,
        "rendered decision D->A should keep at least one terminal support segment: points={points:?}"
    );
    let svg_terminal_support = points[points.len() - 2];
    let drift = (svg_terminal_support.0 - routed_terminal_support.x).abs();

    assert!(
        drift <= MAX_TERMINAL_LANE_X_DRIFT,
        "decision D->A orthogonal SVG endpoint adjustment should preserve routed terminal lane x (drift <= {MAX_TERMINAL_LANE_X_DRIFT}); routed_terminal_support={routed_terminal_support:?}, svg_terminal_support={svg_terminal_support:?}, drift={drift}, routed_path={:?}, svg_points={points:?}",
        routed_edge.path
    );
}

#[test]
fn svg_linear_q1_q2_interaction_fixture_matrix_matches_documented_faces() {
    let q1_cases = [
        ("stacked_fan_in.mmd", "C", "Bot", 0usize),
        ("fan_in.mmd", "D", "Target", 0usize),
        ("five_fan_in.mmd", "F", "Target", 1usize),
    ];

    for (fixture_name, target_id, target_label, min_side_faces) in q1_cases {
        let diagram = load_flowchart_fixture_diagram(fixture_name);
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = SvgEdgePathStyle::Linear;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        options.routing_policies = mmdflux::diagram::RoutingPolicyToggles {
            q1_overflow: true,
            q4_rank_span_periphery: false,
            ..mmdflux::diagram::RoutingPolicyToggles::all_enabled()
        };

        let svg = render_svg(&diagram, &options);
        let rect = node_rect_for_label(&svg, target_label)
            .unwrap_or_else(|| panic!("missing target rect for {target_label} in {fixture_name}"));
        let inbound_indices: Vec<usize> = diagram
            .edges
            .iter()
            .filter(|edge| edge.to == target_id)
            .map(|edge| edge.index)
            .collect();
        assert!(
            !inbound_indices.is_empty(),
            "fixture {fixture_name} should have inbound edges to {target_id}"
        );

        let mut side_face_count = 0usize;
        let mut interior_or_corner_count = 0usize;
        for edge_index in inbound_indices {
            let points = edge_path_for_svg_order(&diagram, &svg, edge_index);
            let face = svg_terminal_approach_face(rect, &points);
            if face == "interior_or_corner" {
                interior_or_corner_count += 1;
            }
            if matches!(face, "left" | "right") {
                side_face_count += 1;
            }
        }

        assert_eq!(
            interior_or_corner_count, 0,
            "fixture {fixture_name} should keep inbound endpoints on a concrete target face under Q1 policy"
        );
        if min_side_faces == 0 {
            assert_eq!(
                side_face_count, 0,
                "fixture {fixture_name} should stay on primary TD incoming face when overflow is not required"
            );
        } else {
            assert!(
                side_face_count >= min_side_faces,
                "fixture {fixture_name} should spill overflow arrivals to side faces under Q1 policy: expected >= {min_side_faces}, actual={side_face_count}"
            );
        }
    }

    let q2_cases = [
        (
            "simple_cycle.mmd",
            "C",
            "A",
            "End",
            "Start",
            "top",
            "bottom",
        ),
        (
            "multiple_cycles.mmd",
            "C",
            "A",
            "Bottom",
            "Top",
            "top",
            "bottom",
        ),
        (
            "q1_q2_conflict.mmd",
            "Q2",
            "B",
            "Sink",
            "Target",
            "top",
            "bottom",
        ),
        (
            "http_request.mmd",
            "Response",
            "Client",
            "Send Response",
            "Client",
            "right",
            "right",
        ),
        (
            "git_workflow.mmd",
            "Remote",
            "Working",
            "Remote Repo",
            "Working Dir",
            "bottom",
            "bottom",
        ),
    ];

    for (
        fixture_name,
        from,
        to,
        source_label,
        target_label,
        expected_source_face,
        expected_target_face,
    ) in q2_cases
    {
        let diagram = load_flowchart_fixture_diagram(fixture_name);
        for (mode_label, q1_enabled) in [("q1-on", true), ("q1-off", false)] {
            let mut options = RenderOptions::default_svg();
            options.svg.edge_path_style = SvgEdgePathStyle::Linear;
            options.routing_mode = Some(RoutingMode::UnifiedPreview);
            options.path_detail = PathDetail::Full;
            options.routing_policies = mmdflux::diagram::RoutingPolicyToggles {
                q1_overflow: q1_enabled,
                q4_rank_span_periphery: false,
                ..mmdflux::diagram::RoutingPolicyToggles::all_enabled()
            };

            let svg = render_svg(&diagram, &options);
            let source_rect = node_rect_for_label(&svg, source_label).unwrap_or_else(|| {
                panic!("missing source rect for {source_label} in {fixture_name}")
            });
            let target_rect = node_rect_for_label(&svg, target_label).unwrap_or_else(|| {
                panic!("missing target rect for {target_label} in {fixture_name}")
            });
            let edge_idx = edge_index(&diagram, from, to);
            let points = edge_path_for_svg_order(&diagram, &svg, edge_idx);
            let source_face = svg_source_departure_face(source_rect, &points);
            assert_eq!(
                source_face, expected_source_face,
                "fixture {fixture_name} edge {from}->{to} should keep expected backward source face {expected_source_face} ({mode_label}); points={points:?}"
            );
            let target_face = svg_terminal_approach_face_relaxed(target_rect, &points);
            assert_eq!(
                target_face, expected_target_face,
                "fixture {fixture_name} edge {from}->{to} should keep expected backward target face {expected_target_face} ({mode_label}); points={points:?}"
            );
        }
    }
}

#[test]
fn svg_linear_q4_rank_span_toggle_pushes_known_long_skip_edges_toward_periphery_lane() {
    let long_skip_cases = [
        ("double_skip.mmd", "A", "D"),
        ("skip_edge_collision.mmd", "A", "D"),
    ];

    for (fixture_name, from, to) in long_skip_cases {
        let diagram = load_flowchart_fixture_diagram(fixture_name);
        let edge_idx = edge_index(&diagram, from, to);

        let render_for_q4 = |q4_enabled: bool| {
            let mut options = RenderOptions::default_svg();
            options.svg.edge_path_style = SvgEdgePathStyle::Linear;
            options.routing_mode = Some(RoutingMode::UnifiedPreview);
            options.path_detail = PathDetail::Full;
            options.routing_policies = mmdflux::diagram::RoutingPolicyToggles {
                q4_rank_span_periphery: q4_enabled,
                ..mmdflux::diagram::RoutingPolicyToggles::all_enabled()
            };
            render_svg(&diagram, &options)
        };

        let off_svg = render_for_q4(false);
        let on_svg = render_for_q4(true);
        let off_points = edge_path_for_svg_order(&diagram, &off_svg, edge_idx);
        let on_points = edge_path_for_svg_order(&diagram, &on_svg, edge_idx);
        let off_detour = horizontal_detour_from_endpoint_axis(&off_points);
        let on_detour = horizontal_detour_from_endpoint_axis(&on_points);

        assert!(
            on_detour > off_detour + 0.5,
            "Q4 rank-span policy should increase SVG long-skip periphery detour for {fixture_name} edge {from}->{to}: detour_off={off_detour}, detour_on={on_detour}, off_points={off_points:?}, on_points={on_points:?}"
        );
    }
}

#[test]
fn svg_linear_q4_rank_span_toggle_keeps_short_inline_edge_stable() {
    let fixture_name = "inline_label_flowchart.mmd";
    let diagram = load_flowchart_fixture_diagram(fixture_name);
    let edge_idx = edge_index(&diagram, "start", "ingest");

    let render_for_q4 = |q4_enabled: bool| {
        let mut options = RenderOptions::default_svg();
        options.svg.edge_path_style = SvgEdgePathStyle::Linear;
        options.routing_mode = Some(RoutingMode::UnifiedPreview);
        options.path_detail = PathDetail::Full;
        options.routing_policies = mmdflux::diagram::RoutingPolicyToggles {
            q4_rank_span_periphery: q4_enabled,
            ..mmdflux::diagram::RoutingPolicyToggles::all_enabled()
        };
        render_svg(&diagram, &options)
    };

    let off_svg = render_for_q4(false);
    let on_svg = render_for_q4(true);
    let off_points = edge_path_for_svg_order(&diagram, &off_svg, edge_idx);
    let on_points = edge_path_for_svg_order(&diagram, &on_svg, edge_idx);

    assert_eq!(
        on_points, off_points,
        "Q4 rank-span toggle should not perturb short edge start->ingest in {fixture_name}; off_points={off_points:?}, on_points={on_points:?}"
    );
}

#[test]
fn q5_styled_segment_monitor_reports_actionable_summary_for_svg() {
    let report = q5_style_segment_monitor_report_for_svg(
        &["edge_styles.mmd", "inline_edge_labels.mmd"],
        12.0,
    );
    assert!(
        report.scanned_styled_paths > 0,
        "Q5 SVG monitor should scan at least one styled path; report={report:?}"
    );
    assert!(
        !report.summary_line.is_empty(),
        "Q5 SVG monitor should emit a stable summary line for CI parsing"
    );
    assert!(
        report.violations.is_empty(),
        "Q5 SVG monitor detected styled-segment violations: {:#?}",
        report
    );
}

#[test]
fn svg_linear_unified_preview_self_loop_tail_does_not_collapse_upward_before_arrow() {
    let diagram = load_flowchart_fixture_diagram("self_loop_labeled.mmd");
    let edge_idx = edge_index(&diagram, "B", "B");

    let full_svg = render_fixture_svg(&diagram, RoutingMode::FullCompute, SvgEdgePathStyle::Linear);
    let unified_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Linear,
    );

    let full_points = edge_path_for_svg_order(&diagram, &full_svg, edge_idx);
    let unified_points = edge_path_for_svg_order(&diagram, &unified_svg, edge_idx);

    assert!(
        full_points.len() >= 5 && unified_points.len() >= 5,
        "expected self-loop to contain at least 5 points; full={full_points:?}, unified={unified_points:?}"
    );

    let full_tail_elbow = full_points[full_points.len() - 3];
    let unified_tail_elbow = unified_points[unified_points.len() - 3];
    let delta_y = (full_tail_elbow.1 - unified_tail_elbow.1).abs();

    assert!(
        delta_y <= 12.0,
        "self-loop tail elbow should remain near full-compute in unified linear mode (avoid upward collapse); full_tail_elbow={full_tail_elbow:?}, unified_tail_elbow={unified_tail_elbow:?}, delta_y={delta_y}, full_points={full_points:?}, unified_points={unified_points:?}"
    );
}

#[test]
fn unified_preview_diamond_boundary_clipping_matches_shape_boundary() {
    let diagram = load_flowchart_fixture_diagram("decision.mmd");

    let mut options = RenderOptions::default_svg();
    options.routing_mode = Some(RoutingMode::UnifiedPreview);
    options.path_detail = PathDetail::Full;

    let mode = mmdflux::diagrams::flowchart::engine::MeasurementMode::for_format(
        OutputFormat::Svg,
        &RenderConfig::default(),
    );
    let engine = DagreLayoutEngine::with_mode(mode);
    let config = EngineConfig::Dagre(mmdflux::dagre::types::LayoutConfig::default());
    let geom = engine.layout(&diagram, &config).unwrap();
    let routed = route_graph_geometry(&diagram, &geom, RoutingMode::UnifiedPreview);

    // B is a diamond; B->D is a forward edge — verify source endpoint is on diamond boundary
    let edge = routed
        .edges
        .iter()
        .find(|e| e.from == "B" && e.to == "D")
        .expect("missing B->D edge");
    let start = edge.path.first().unwrap();
    let b_rect = geom.nodes.get("B").unwrap().rect;
    let cx = b_rect.x + b_rect.width / 2.0;
    let cy = b_rect.y + b_rect.height / 2.0;
    let w = b_rect.width / 2.0;
    let h = b_rect.height / 2.0;
    let boundary = (start.x - cx).abs() / w + (start.y - cy).abs() / h;
    assert!(
        (boundary - 1.0).abs() < 0.05,
        "unified-preview B->D source should be on diamond boundary: boundary={boundary}, start={start:?}"
    );
}

#[test]
fn unified_preview_subgraph_to_subgraph_edge_keeps_terminal_attachment() {
    let diagram = load_flowchart_fixture_diagram("subgraph_to_subgraph_edge.mmd");
    let edge_index = edge_index(&diagram, "API", "DB");

    let full_svg = render_fixture_svg(&diagram, RoutingMode::FullCompute, SvgEdgePathStyle::Basis);
    let unified_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Basis,
    );

    let full_points = edge_path_for_svg_order(&diagram, &full_svg, edge_index);
    let unified_points = edge_path_for_svg_order(&diagram, &unified_svg, edge_index);
    let full_start = full_points[0];
    let unified_start = unified_points[0];
    let full_end = full_points[full_points.len() - 1];
    let unified_end = unified_points[unified_points.len() - 1];

    assert!(
        (full_start.1 - unified_start.1).abs() <= 1.0 && (full_end.1 - unified_end.1).abs() <= 1.0,
        "API -> DB should keep vertical attachment parity with full-compute; full_points={full_points:?}, unified_points={unified_points:?}"
    );
}

#[test]
fn unified_preview_inner_bt_subgraph_edge_does_not_collapse() {
    let diagram = load_flowchart_fixture_diagram("subgraph_direction_nested_both.mmd");
    let edge_index = edge_index(&diagram, "A", "B");

    let full_svg = render_fixture_svg(&diagram, RoutingMode::FullCompute, SvgEdgePathStyle::Basis);
    let unified_svg = render_fixture_svg(
        &diagram,
        RoutingMode::UnifiedPreview,
        SvgEdgePathStyle::Basis,
    );

    let full_points = edge_path_for_svg_order(&diagram, &full_svg, edge_index);
    let unified_points = edge_path_for_svg_order(&diagram, &unified_svg, edge_index);
    let full_start = full_points[0];
    let unified_start = unified_points[0];
    let full_end = full_points[full_points.len() - 1];
    let unified_end = unified_points[unified_points.len() - 1];
    let full_span = (full_start.1 - full_end.1).abs();
    let unified_span = (unified_start.1 - unified_end.1).abs();

    assert!(
        (full_start.1 - unified_start.1).abs() <= 1.0
            && (full_end.1 - unified_end.1).abs() <= 1.0
            && unified_span >= full_span - 1.0,
        "A -> B in inner BT subgraph should preserve full-compute span; full_points={full_points:?}, unified_points={unified_points:?}, full_span={full_span}, unified_span={unified_span}"
    );
}

#[test]
fn render_svg_edge_styles_and_labels() {
    let input = "graph TD\nA ==>|yes| B\nB -.->|no| C\nC <--> D\n";
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    assert!(svg.contains("stroke-dasharray"));
    assert!(svg.contains("stroke-width"));
    assert!(svg.contains("marker-end"));
    assert!(svg.contains("marker-start"));
    assert!(svg.contains("yes"));
    assert!(svg.contains("no"));
}

#[test]
fn render_svg_subgraphs_and_self_edges() {
    let input = "graph TD\nsubgraph Group\nA-->A\nend\n";
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    assert!(svg.contains("Group"));
    assert!(svg.contains("class=\"subgraph\""));
    assert!(svg.matches("<path").count() >= 2);
}

#[test]
fn render_svg_direction_override_lr_node_positions() {
    // subgraph_direction_lr.mmd: TD graph with LR subgraph containing Step 1 -> Step 2 -> Step 3
    // After direction override, these nodes should be arranged horizontally (increasing x).
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_lr.mmd").unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    let positions = extract_node_x_positions(&svg);
    let x_step1 = positions.get("Step 1").expect("Step 1 not found in SVG");
    let x_step2 = positions.get("Step 2").expect("Step 2 not found in SVG");
    let x_step3 = positions.get("Step 3").expect("Step 3 not found in SVG");

    assert!(
        x_step1 < x_step2 && x_step2 < x_step3,
        "LR direction override: Step 1 ({x_step1}) < Step 2 ({x_step2}) < Step 3 ({x_step3}) expected"
    );
}

#[test]
fn render_svg_direction_override_cross_boundary() {
    // subgraph_direction_cross_boundary.mmd: TD graph with LR subgraph, cross-boundary edges
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_cross_boundary.mmd")
            .unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    // A and B are inside the LR subgraph, should be horizontal
    let positions = extract_node_x_positions(&svg);
    let x_a = positions.get("A").expect("A not found in SVG");
    let x_b = positions.get("B").expect("B not found in SVG");

    assert!(
        x_a < x_b,
        "LR direction override: A ({x_a}) should be left of B ({x_b})"
    );

    // SVG should not contain NaN values
    assert!(!svg.contains("NaN"), "SVG should not contain NaN values");
}

#[test]
fn render_svg_direction_override_cross_boundary_remains_nan_free() {
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_cross_boundary.mmd")
            .unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    assert!(!svg.contains("NaN"), "SVG should not contain NaN values");
    assert!(
        !svg.contains("inf"),
        "SVG should not contain infinite values"
    );
}

#[test]
fn cross_boundary_direction_override_edges_still_render_without_nan() {
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_cross_boundary.mmd")
            .unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    assert!(!svg.contains("NaN"));
}

#[test]
fn render_svg_direction_override_mixed() {
    // subgraph_direction_mixed.mmd: Two subgraphs with different direction overrides
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_mixed.mmd").unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    let positions = extract_node_x_positions(&svg);

    // LR group: A should be left of B
    let x_a = positions.get("A").expect("A not found");
    let x_b = positions.get("B").expect("B not found");
    assert!(x_a < x_b, "LR: A ({x_a}) should be left of B ({x_b})");

    // BT group: C and D should be vertically arranged (same x or close x)
    let x_c = positions.get("C").expect("C not found");
    let x_d = positions.get("D").expect("D not found");
    assert!(
        (x_c - x_d).abs() < 1.0,
        "BT: C ({x_c}) and D ({x_d}) should have similar x (vertically stacked)"
    );

    assert!(!svg.contains("NaN"), "SVG should not contain NaN");
}

#[test]
fn render_svg_direction_override_nested() {
    // subgraph_direction_nested.mmd: Outer (no override) with inner LR subgraph
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_nested.mmd").unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    let positions = extract_node_x_positions(&svg);

    // Inner LR: A -> B -> C should be horizontal
    let x_a = positions.get("A").expect("A not found");
    let x_b = positions.get("B").expect("B not found");
    let x_c = positions.get("C").expect("C not found");
    assert!(
        x_a < x_b && x_b < x_c,
        "Inner LR: A ({x_a}) < B ({x_b}) < C ({x_c})"
    );

    assert!(!svg.contains("NaN"), "SVG should not contain NaN");
}

#[test]
fn render_svg_direction_override_nested_both() {
    // subgraph_direction_nested_both.mmd: Outer LR with inner BT
    let input =
        std::fs::read_to_string("tests/fixtures/flowchart/subgraph_direction_nested_both.mmd")
            .unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    let positions = extract_node_x_positions(&svg);

    // Inner BT: A and B should be vertically arranged (similar x)
    let x_a = positions.get("A").expect("A not found");
    let x_b = positions.get("B").expect("B not found");
    assert!(
        (x_a - x_b).abs() < 1.0,
        "Inner BT: A ({x_a}) and B ({x_b}) should have similar x"
    );

    // Outer LR: C should be to the side of the inner subgraph
    assert!(positions.contains_key("C"), "C should be present");

    assert!(!svg.contains("NaN"), "SVG should not contain NaN");
}

#[test]
fn render_svg_all_direction_override_fixtures_valid() {
    // Run all direction override fixtures and verify no NaN and valid SVG
    let fixtures = [
        "subgraph_direction_lr.mmd",
        "subgraph_direction_cross_boundary.mmd",
        "subgraph_direction_mixed.mmd",
        "subgraph_direction_nested.mmd",
        "subgraph_direction_nested_both.mmd",
    ];
    for fixture in &fixtures {
        let path = format!("tests/fixtures/flowchart/{fixture}");
        let input =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
        let flowchart =
            parse_flowchart(&input).unwrap_or_else(|e| panic!("Failed to parse {path}: {e}"));
        let diagram = build_diagram(&flowchart);
        let svg = render_svg(&diagram, &RenderOptions::default_svg());

        assert!(
            svg.starts_with("<svg"),
            "{fixture}: SVG should start with <svg"
        );
        assert!(
            !svg.contains("NaN"),
            "{fixture}: SVG should not contain NaN"
        );
        // Every fixture should have at least one edge path
        assert!(
            svg.contains("<path"),
            "{fixture}: SVG should contain at least one <path element"
        );
    }
}

#[test]
fn render_svg_direction_override_backward_edge() {
    // Backward edge (B -> Start) crossing subgraph boundary
    let input = r#"graph TD
    Start --> A
    subgraph sg1[Loop Section]
        direction LR
        A --> B
    end
    B --> Start
"#;
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);
    let svg = render_svg(&diagram, &RenderOptions::default_svg());

    let positions = extract_node_x_positions(&svg);

    // LR nodes A and B should be horizontal
    let x_a = positions.get("A").expect("A not found");
    let x_b = positions.get("B").expect("B not found");
    assert!(x_a < x_b, "LR: A ({x_a}) should be left of B ({x_b})");

    assert!(!svg.contains("NaN"), "SVG should not contain NaN");
    assert!(svg.contains("<path"), "SVG should have edge paths");
}

#[test]
fn render_svg_positioned_mmds_routed_basic_includes_paths_and_subgraph() {
    let input = std::fs::read_to_string("tests/fixtures/mmds/positioned/routed-basic.json")
        .expect("positioned fixture should exist");
    let mut instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    instance.parse(&input).expect("MMDS parse should succeed");

    let svg = instance
        .render(OutputFormat::Svg, &RenderConfig::default())
        .expect("routed MMDS should render SVG");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("class=\"subgraph\""));
    assert!(svg.contains("<path"));
    assert!(svg.contains("Start"));
    assert!(svg.contains("Group"));
}
