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

fn segment_axis(a: (f64, f64), b: (f64, f64)) -> Option<char> {
    if (a.0 - b.0).abs() < 0.001 && (a.1 - b.1).abs() >= 0.001 {
        Some('V')
    } else if (a.1 - b.1).abs() < 0.001 && (a.0 - b.0).abs() >= 0.001 {
        Some('H')
    } else {
        None
    }
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
        let end_face = svg_terminal_approach_face(rect, &points);

        assert_eq!(
            end_face, "right",
            "Q2-conflict edge should keep canonical backward lane on right for {style:?}: end={end:?}, rect={rect:?}, points={points:?}"
        );
    }
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
    let edge_index = edge_index(&diagram, "B", "D");

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
    let dx = (full_start.0 - unified_start.0).abs();
    let dy = (full_start.1 - unified_start.1).abs();
    let displacement = (dx * dx + dy * dy).sqrt();

    assert!(
        displacement <= 24.0,
        "diamond exit clipping should avoid large endpoint displacement from full-compute (<=24px); full_start={full_start:?}, unified_start={unified_start:?}, displacement={displacement}, full_points={full_points:?}, unified_points={unified_points:?}"
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
