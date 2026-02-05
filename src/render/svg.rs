//! SVG rendering for flowchart diagrams.

use std::collections::HashMap;
use std::fmt::Write;

use super::layout::build_dagre_layout;
use super::svg_metrics::SvgTextMetrics;
use super::{RenderOptions, SvgEdgeCurve, layout_config_for_diagram};
use crate::dagre::{LayoutResult, Point, Rect};
use crate::graph::{Arrow, Diagram, Direction, Edge, Node, Shape, Stroke};

const STROKE_COLOR: &str = "#333";
const SUBGRAPH_STROKE: &str = "#888";
const NODE_FILL: &str = "white";
const TEXT_COLOR: &str = "#333";

pub fn render_svg(diagram: &Diagram, options: &RenderOptions) -> String {
    let svg_options = &options.svg;
    let scale = svg_options.scale;
    let metrics = SvgTextMetrics::new(
        svg_options.font_size,
        svg_options.node_padding_x,
        svg_options.node_padding_y,
    );

    let mut config = layout_config_for_diagram(diagram, options);
    config.ranker = options.ranker;
    if options.cluster_ranksep.is_none() {
        // Mermaid's dagre renderer does not add extra rank separation for clusters.
        // Keep the default for text output but disable it for SVG unless overridden.
        config.dagre_cluster_rank_sep = 0.0;
    }

    let layout = build_dagre_layout(
        diagram,
        &config,
        |node| svg_node_dimensions(&metrics, node),
        |edge| {
            edge.label
                .as_ref()
                .map(|label| metrics.edge_label_dimensions(label))
        },
    );

    let self_edge_paths = compute_self_edge_paths(diagram, &layout, &metrics);
    let bounds = compute_svg_bounds(diagram, &layout, &metrics, &self_edge_paths);
    let padding = svg_options.diagram_padding;
    let (min_x, min_y, max_x, max_y) = bounds.finalize(layout.width, layout.height);
    let width = (max_x - min_x + padding * 2.0) * scale;
    let height = (max_y - min_y + padding * 2.0) * scale;
    let offset_x = (-min_x + padding) * scale;
    let offset_y = (-min_y + padding) * scale;

    let mut writer = SvgWriter::new();
    writer.start_svg(
        width,
        height,
        &svg_options.font_family,
        svg_options.font_size * scale,
    );

    render_defs(&mut writer, scale);
    writer.start_group_transform(offset_x, offset_y);
    render_subgraphs(&mut writer, diagram, &layout, &metrics, scale);
    render_edges(
        &mut writer,
        diagram,
        &layout,
        &self_edge_paths,
        svg_options.edge_curve,
        svg_options.edge_curve_radius,
        scale,
    );
    render_edge_labels(
        &mut writer,
        diagram,
        &layout,
        &self_edge_paths,
        &metrics,
        scale,
    );
    render_nodes(&mut writer, diagram, &layout, &metrics, scale);
    writer.end_group();

    writer.end_svg();
    writer.finish()
}

fn svg_node_dimensions(metrics: &SvgTextMetrics, node: &Node) -> (f64, f64) {
    let (label_w, label_h) = metrics.measure_text_with_padding(&node.label, 0.0, 0.0);

    let (mut width, mut height) = match node.shape {
        Shape::Rectangle => (
            label_w + metrics.node_padding_x * 4.0,
            label_h + metrics.node_padding_y * 2.0,
        ),
        Shape::Diamond => {
            let w = label_w + metrics.node_padding_x;
            let h = label_h + metrics.node_padding_y;
            let size = w + h;
            (size, size)
        }
        _ => (
            label_w + metrics.node_padding_x * 2.0,
            label_h + metrics.node_padding_y * 2.0,
        ),
    };

    match node.shape {
        Shape::Hexagon | Shape::Trapezoid | Shape::InvTrapezoid | Shape::Asymmetric => {
            width *= 1.15;
        }
        Shape::Circle | Shape::DoubleCircle => {
            let size = width.max(height);
            width = size;
            height = size;
        }
        _ => {}
    }

    (width, height)
}

fn render_defs(writer: &mut SvgWriter, scale: f64) {
    let base = 10.0;
    let half = base / 2.0;
    let marker_size = 8.0 * scale;

    writer.start_tag("<defs>");
    let marker = format!(
        "<marker id=\"arrowhead\" viewBox=\"0 0 {base} {base}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        base = fmt_f64(base),
        ref_x = fmt_f64(half),
        ref_y = fmt_f64(half),
        mw = fmt_f64(marker_size),
        mh = fmt_f64(marker_size)
    );
    writer.start_tag(&marker);
    let path = format!(
        "<path d=\"M 0 0 L {tip} {mid} L 0 {size} z\" fill=\"{color}\" />",
        tip = fmt_f64(base),
        mid = fmt_f64(half),
        size = fmt_f64(base),
        color = STROKE_COLOR
    );
    writer.push_line(&path);
    writer.end_tag("</marker>");
    writer.end_tag("</defs>");
}

fn render_subgraphs(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    layout: &LayoutResult,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    if layout.subgraph_bounds.is_empty() {
        return;
    }

    let mut subgraphs: Vec<_> = layout
        .subgraph_bounds
        .iter()
        .filter_map(|(id, rect)| {
            diagram.subgraphs.get(id).map(|sg| {
                let depth = diagram.subgraph_depth(id);
                (id, rect, &sg.title, depth)
            })
        })
        .collect();

    subgraphs.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.0.cmp(b.0)));

    writer.start_group("clusters");
    for (_id, rect, title, _depth) in subgraphs {
        let rect = scale_rect(rect, scale);
        let stroke_width = fmt_f64(1.0 * scale);
        let rect_line = format!(
            "<rect class=\"subgraph\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" />",
            x = fmt_f64(rect.x),
            y = fmt_f64(rect.y),
            w = fmt_f64(rect.width),
            h = fmt_f64(rect.height),
            stroke = SUBGRAPH_STROKE,
            stroke_width = stroke_width
        );
        writer.push_line(&rect_line);

        if !title.trim().is_empty() {
            let title_x = rect.x + rect.width / 2.0;
            let title_y = rect.y + metrics.font_size * 0.25;
            let text = format!(
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" dominant-baseline=\"hanging\" fill=\"{color}\">{label}</text>",
                x = fmt_f64(title_x),
                y = fmt_f64(title_y),
                color = TEXT_COLOR,
                label = escape_text(title)
            );
            writer.push_line(&text);
        }
    }
    writer.end_group();
}

fn render_edges(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    layout: &LayoutResult,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
    edge_curve: SvgEdgeCurve,
    edge_curve_radius: f64,
    scale: f64,
) {
    let mut edge_paths: Vec<(usize, Vec<Point>)> = layout
        .edges
        .iter()
        .map(|edge| (edge.index, edge.points.clone()))
        .collect();
    edge_paths.extend(layout.self_edges.iter().map(|edge| {
        let points = self_edge_paths
            .get(&edge.edge_index)
            .cloned()
            .unwrap_or_else(|| edge.points.clone());
        (edge.edge_index, points)
    }));
    edge_paths.sort_by_key(|(index, _)| *index);

    writer.start_group("edgePaths");
    for (index, points) in edge_paths {
        let Some(edge) = diagram.edges.get(index) else {
            continue;
        };
        let mut points = adjust_edge_points_for_shapes(diagram, layout, edge, &points);
        points = fix_corner_points(&points);
        points = apply_marker_offsets(&points, edge);
        let d = path_from_points(&points, scale, edge_curve, edge_curve_radius);
        if d.is_empty() {
            continue;
        }
        let mut attrs = edge_style_attrs(edge, scale);
        attrs.push_str(&edge_marker_attrs(edge));
        let line = format!("<path d=\"{d}\"{attrs} />", d = d, attrs = attrs);
        writer.push_line(&line);
    }
    writer.end_group();
}

fn render_edge_labels(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    layout: &LayoutResult,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    writer.start_group("edgeLabels");

    for (index, edge) in diagram.edges.iter().enumerate() {
        let Some(label) = edge.label.as_ref() else {
            continue;
        };
        let position = layout
            .label_positions
            .get(&index)
            .map(|pos| pos.point)
            .or_else(|| fallback_label_position(layout, index, self_edge_paths));
        let Some(point) = position else {
            continue;
        };
        render_text_centered(
            writer,
            point.x * scale,
            point.y * scale,
            label,
            metrics,
            scale,
        );
    }

    writer.end_group();
}

fn render_nodes(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    layout: &LayoutResult,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    writer.start_group("nodes");

    let mut node_ids: Vec<&String> = diagram.nodes.keys().collect();
    node_ids.sort();

    for node_id in node_ids {
        let node = &diagram.nodes[node_id];
        let Some(rect) = layout.nodes.get(&crate::dagre::NodeId(node_id.clone())) else {
            continue;
        };
        render_node_shape(writer, node, rect, scale);

        let center = rect.center();
        render_text_centered(
            writer,
            center.x * scale,
            center.y * scale,
            &node.label,
            metrics,
            scale,
        );
    }

    writer.end_group();
}

fn render_node_shape(writer: &mut SvgWriter, node: &Node, rect: &Rect, scale: f64) {
    let rect = scale_rect(rect, scale);
    let stroke_width = fmt_f64(1.0 * scale);
    let style = format!(
        " fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" stroke-linejoin=\"round\"",
        fill = NODE_FILL,
        stroke = STROKE_COLOR,
        stroke_width = stroke_width
    );

    match node.shape {
        Shape::Rectangle => {
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Round => {
            let radius = (rect.height.min(rect.width) * 0.2).max(4.0 * scale);
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{rx}\" ry=\"{ry}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                rx = fmt_f64(radius),
                ry = fmt_f64(radius),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Stadium => {
            let radius = rect.height / 2.0;
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{rx}\" ry=\"{ry}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                rx = fmt_f64(radius),
                ry = fmt_f64(radius),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Document
        | Shape::Documents
        | Shape::TaggedDocument
        | Shape::Card
        | Shape::TaggedRect => {
            // Base rectangle
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                style = style
            );
            writer.push_line(&line);

            // Optional folded corner for card/tagged shapes
            if matches!(
                node.shape,
                Shape::Card | Shape::TaggedRect | Shape::TaggedDocument
            ) {
                let fold = (rect.width.min(rect.height) * 0.2).max(4.0 * scale);
                let x1 = rect.x + rect.width - fold;
                let y1 = rect.y;
                let x2 = rect.x + rect.width;
                let y2 = rect.y + fold;
                let stroke = format!(
                    " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" fill=\"none\"",
                    stroke = STROKE_COLOR,
                    stroke_width = stroke_width
                );
                let fold_path = format!(
                    "<path d=\"M{x1},{y1} L{x2},{y1} L{x2},{y2}\"{stroke} />",
                    x1 = fmt_f64(x1),
                    y1 = fmt_f64(y1),
                    x2 = fmt_f64(x2),
                    y2 = fmt_f64(y2),
                    stroke = stroke
                );
                writer.push_line(&fold_path);
            }

            // Optional wavy bottom for document shapes
            if matches!(
                node.shape,
                Shape::Document | Shape::Documents | Shape::TaggedDocument
            ) {
                let wave_height = (rect.height * 0.12).max(3.0 * scale);
                let y = rect.y + rect.height - wave_height;
                let wave_count = 2;
                let wave_width = rect.width / wave_count as f64;
                let mut d = String::new();
                let _ = write!(d, "M{},{}", fmt_f64(rect.x), fmt_f64(y));
                for i in 0..wave_count {
                    let x0 = rect.x + (i as f64) * wave_width;
                    let x1 = x0 + wave_width / 2.0;
                    let x2 = x0 + wave_width;
                    let y1 = if i % 2 == 0 {
                        rect.y + rect.height
                    } else {
                        rect.y + rect.height - wave_height
                    };
                    let _ = write!(
                        d,
                        " Q{},{} {},{}",
                        fmt_f64(x1),
                        fmt_f64(y1),
                        fmt_f64(x2),
                        fmt_f64(y)
                    );
                }
                let wave = format!(
                    "<path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" />",
                    d = d,
                    stroke = STROKE_COLOR,
                    stroke_width = stroke_width
                );
                writer.push_line(&wave);
            }

            // Optional shadow for stacked documents
            if matches!(node.shape, Shape::Documents) {
                let offset = (rect.height * 0.12).max(3.0 * scale);
                let shadow = format!(
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" opacity=\"0.4\" />",
                    x = fmt_f64(rect.x + offset),
                    y = fmt_f64(rect.y + offset),
                    w = fmt_f64(rect.width),
                    h = fmt_f64(rect.height),
                    stroke = STROKE_COLOR,
                    stroke_width = stroke_width
                );
                writer.push_line(&shadow);
            }
        }
        Shape::Diamond => {
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let points = vec![
                (cx, rect.y),
                (rect.x + rect.width, cy),
                (cx, rect.y + rect.height),
                (rect.x, cy),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Hexagon => {
            let indent = rect.width * 0.2;
            let cy = rect.y + rect.height / 2.0;
            let points = vec![
                (rect.x + indent, rect.y),
                (rect.x + rect.width - indent, rect.y),
                (rect.x + rect.width, cy),
                (rect.x + rect.width - indent, rect.y + rect.height),
                (rect.x + indent, rect.y + rect.height),
                (rect.x, cy),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Asymmetric => {
            let indent = rect.width * 0.2;
            let cy = rect.y + rect.height / 2.0;
            let points = vec![
                (rect.x + indent, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x + indent, rect.y + rect.height),
                (rect.x, cy),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Parallelogram => {
            let indent = rect.width * 0.2;
            let points = vec![
                (rect.x + indent, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x + rect.width - indent, rect.y + rect.height),
                (rect.x, rect.y + rect.height),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::InvParallelogram => {
            let indent = rect.width * 0.2;
            let points = vec![
                (rect.x, rect.y),
                (rect.x + rect.width - indent, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x + indent, rect.y + rect.height),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::ManualInput => {
            let slant = rect.height * 0.25;
            let points = vec![
                (rect.x + slant, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x, rect.y + rect.height),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Trapezoid => {
            let indent = rect.width * 0.2;
            let points = vec![
                (rect.x + indent, rect.y),
                (rect.x + rect.width - indent, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
                (rect.x, rect.y + rect.height),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::InvTrapezoid => {
            let indent = rect.width * 0.2;
            let points = vec![
                (rect.x, rect.y),
                (rect.x + rect.width, rect.y),
                (rect.x + rect.width - indent, rect.y + rect.height),
                (rect.x + indent, rect.y + rect.height),
            ];
            let line = format!(
                "<polygon points=\"{points}\"{style} />",
                points = polygon_points(&points),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::Circle => {
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let rx = rect.width / 2.0;
            let ry = rect.height / 2.0;
            let line = format!(
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                rx = fmt_f64(rx),
                ry = fmt_f64(ry),
                style = style
            );
            writer.push_line(&line);
        }
        Shape::DoubleCircle => {
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let rx = rect.width / 2.0;
            let ry = rect.height / 2.0;
            let line = format!(
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                rx = fmt_f64(rx),
                ry = fmt_f64(ry),
                style = style
            );
            writer.push_line(&line);

            let inset = (rect.width.min(rect.height) * 0.12).max(3.0 * scale);
            let inner_rx = (rx - inset).max(0.0);
            let inner_ry = (ry - inset).max(0.0);
            let inner = format!(
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                rx = fmt_f64(inner_rx),
                ry = fmt_f64(inner_ry),
                style = style
            );
            writer.push_line(&inner);
        }
        Shape::SmallCircle | Shape::FramedCircle | Shape::CrossedCircle => {
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let radius = rect.width.min(rect.height) * 0.35;
            let circle = format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                r = fmt_f64(radius),
                style = style
            );
            writer.push_line(&circle);

            if matches!(node.shape, Shape::FramedCircle) {
                let inner = format!(
                    "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"{style} />",
                    cx = fmt_f64(cx),
                    cy = fmt_f64(cy),
                    r = fmt_f64(radius * 0.65),
                    style = style
                );
                writer.push_line(&inner);
            }

            if matches!(node.shape, Shape::CrossedCircle) {
                let stroke = format!(
                    " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"",
                    stroke = STROKE_COLOR,
                    stroke_width = stroke_width
                );
                let x1 = cx - radius * 0.6;
                let x2 = cx + radius * 0.6;
                let y1 = cy - radius * 0.6;
                let y2 = cy + radius * 0.6;
                let line1 = format!(
                    "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{stroke} />",
                    x1 = fmt_f64(x1),
                    y1 = fmt_f64(y1),
                    x2 = fmt_f64(x2),
                    y2 = fmt_f64(y2),
                    stroke = stroke
                );
                let line2 = format!(
                    "<line x1=\"{x1}\" y1=\"{y2}\" x2=\"{x2}\" y2=\"{y1}\"{stroke} />",
                    x1 = fmt_f64(x1),
                    y1 = fmt_f64(y1),
                    x2 = fmt_f64(x2),
                    y2 = fmt_f64(y2),
                    stroke = stroke
                );
                writer.push_line(&line1);
                writer.push_line(&line2);
            }
        }
        Shape::Subroutine => {
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                style = style
            );
            writer.push_line(&line);

            let inset = (rect.width * 0.1).max(4.0 * scale).min(rect.width / 3.0);
            let x1 = rect.x + inset;
            let x2 = rect.x + rect.width - inset;
            let stroke = format!(
                " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"",
                stroke = STROKE_COLOR,
                stroke_width = stroke_width
            );
            let left_line = format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x1}\" y2=\"{y2}\"{stroke} />",
                x1 = fmt_f64(x1),
                y1 = fmt_f64(rect.y),
                y2 = fmt_f64(rect.y + rect.height),
                stroke = stroke
            );
            let right_line = format!(
                "<line x1=\"{x2}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{stroke} />",
                x2 = fmt_f64(x2),
                y1 = fmt_f64(rect.y),
                y2 = fmt_f64(rect.y + rect.height),
                stroke = stroke
            );
            writer.push_line(&left_line);
            writer.push_line(&right_line);
        }
        Shape::Cylinder => {
            let rx = rect.width / 2.0;
            let ry = rect.height * 0.2;
            let top_y = rect.y + ry;
            let bottom_y = rect.y + rect.height - ry;
            let d = format!(
                "M{x},{top} A{rx},{ry} 0 0 1 {x2},{top} L{x2},{bottom} A{rx},{ry} 0 0 1 {x},{bottom} Z",
                x = fmt_f64(rect.x),
                top = fmt_f64(top_y),
                rx = fmt_f64(rx),
                ry = fmt_f64(ry),
                x2 = fmt_f64(rect.x + rect.width),
                bottom = fmt_f64(bottom_y)
            );
            let line = format!("<path d=\"{d}\"{style} />", d = d, style = style);
            writer.push_line(&line);
        }
        Shape::TextBlock => {
            // Borderless: only text will be drawn.
        }
        Shape::ForkJoin => {
            let y = rect.y + rect.height / 2.0;
            let stroke = format!(
                " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" stroke-linecap=\"square\"",
                stroke = STROKE_COLOR,
                stroke_width = fmt_f64((rect.height * 0.3).max(3.0 * scale))
            );
            let line = format!(
                "<line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\"{stroke} />",
                x1 = fmt_f64(rect.x),
                x2 = fmt_f64(rect.x + rect.width),
                y = fmt_f64(y),
                stroke = stroke
            );
            writer.push_line(&line);
        }
    }
}

fn render_text_centered(
    writer: &mut SvgWriter,
    x: f64,
    y: f64,
    text: &str,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() == 1 {
        let line = format!(
            "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"{color}\">{text}</text>",
            x = fmt_f64(x),
            y = fmt_f64(y),
            color = TEXT_COLOR,
            text = escape_text(text)
        );
        writer.push_line(&line);
        return;
    }

    let line_height = metrics.line_height * scale;
    let total_height = line_height * (lines.len().saturating_sub(1) as f64);
    let start_y = y - total_height / 2.0;

    for (idx, line_text) in lines.iter().enumerate() {
        let line_y = start_y + line_height * idx as f64;
        let line = format!(
            "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"{color}\">{text}</text>",
            x = fmt_f64(x),
            y = fmt_f64(line_y),
            color = TEXT_COLOR,
            text = escape_text(line_text)
        );
        writer.push_line(&line);
    }
}

struct SvgBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SvgBounds {
    fn new() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn update_point(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn update_rect(&mut self, rect: &Rect) {
        self.update_point(rect.x, rect.y);
        self.update_point(rect.x + rect.width, rect.y + rect.height);
    }

    fn finalize(&self, fallback_width: f64, fallback_height: f64) -> (f64, f64, f64, f64) {
        if !self.min_x.is_finite() || !self.min_y.is_finite() {
            return (0.0, 0.0, fallback_width, fallback_height);
        }
        (self.min_x, self.min_y, self.max_x, self.max_y)
    }
}

fn compute_svg_bounds(
    diagram: &Diagram,
    layout: &LayoutResult,
    metrics: &SvgTextMetrics,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
) -> SvgBounds {
    let mut bounds = SvgBounds::new();

    for rect in layout.nodes.values() {
        bounds.update_rect(rect);
    }

    for rect in layout.subgraph_bounds.values() {
        bounds.update_rect(rect);
    }

    let edge_count = diagram.edges.len();
    for edge in &layout.edges {
        if edge.index >= edge_count {
            continue;
        }
        for point in &edge.points {
            bounds.update_point(point.x, point.y);
        }
    }

    for edge in &layout.self_edges {
        let points = self_edge_paths
            .get(&edge.edge_index)
            .map(Vec::as_slice)
            .unwrap_or_else(|| edge.points.as_slice());
        for point in points {
            bounds.update_point(point.x, point.y);
        }
    }

    for (index, edge) in diagram.edges.iter().enumerate() {
        let Some(label) = edge.label.as_ref() else {
            continue;
        };
        let position = layout
            .label_positions
            .get(&index)
            .map(|pos| pos.point)
            .or_else(|| fallback_label_position(layout, index, self_edge_paths));
        let Some(point) = position else {
            continue;
        };
        let (w, h) = metrics.edge_label_dimensions(label);
        let rect = Rect {
            x: point.x - w / 2.0,
            y: point.y - h / 2.0,
            width: w,
            height: h,
        };
        bounds.update_rect(&rect);
    }

    bounds
}

fn edge_style_attrs(edge: &Edge, scale: f64) -> String {
    let stroke_width = match edge.stroke {
        Stroke::Thick => 2.0 * scale,
        _ => 1.0 * scale,
    };
    let mut attrs = format!(
        " stroke=\"{stroke}\" stroke-width=\"{width}\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
        stroke = STROKE_COLOR,
        width = fmt_f64(stroke_width)
    );
    if edge.stroke == Stroke::Dotted {
        let dash = fmt_f64(2.0 * scale);
        let gap = fmt_f64(4.0 * scale);
        let _ = write!(attrs, " stroke-dasharray=\"{dash},{gap}\"");
    }
    attrs
}

fn edge_marker_attrs(edge: &Edge) -> String {
    let mut attrs = String::new();
    if edge.arrow_start == Arrow::Normal {
        attrs.push_str(" marker-start=\"url(#arrowhead)\"");
    }
    if edge.arrow_end == Arrow::Normal {
        attrs.push_str(" marker-end=\"url(#arrowhead)\"");
    }
    attrs
}

fn path_from_points(
    points: &[Point],
    scale: f64,
    curve: SvgEdgeCurve,
    curve_radius: f64,
) -> String {
    if points.is_empty() {
        return String::new();
    }
    let scaled: Vec<(f64, f64)> = points
        .iter()
        .map(|point| (point.x * scale, point.y * scale))
        .collect();
    match curve {
        SvgEdgeCurve::Basis => path_from_points_basis(&scaled),
        SvgEdgeCurve::Rounded => path_from_points_rounded(&scaled, curve_radius * scale),
        SvgEdgeCurve::Linear => path_from_points_linear(&scaled),
    }
}

fn adjust_edge_points_for_shapes(
    diagram: &Diagram,
    layout: &LayoutResult,
    edge: &Edge,
    points: &[Point],
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let Some(from_rect) = layout.nodes.get(&crate::dagre::NodeId(edge.from.clone())) else {
        return points.to_vec();
    };
    let Some(to_rect) = layout.nodes.get(&crate::dagre::NodeId(edge.to.clone())) else {
        return points.to_vec();
    };
    let Some(from_node) = diagram.nodes.get(&edge.from) else {
        return points.to_vec();
    };
    let Some(to_node) = diagram.nodes.get(&edge.to) else {
        return points.to_vec();
    };

    let mut adjusted = points.to_vec();
    let from_target = if points.len() > 1 {
        points[1]
    } else {
        from_rect.center()
    };
    let to_target = if points.len() > 1 {
        points[points.len() - 2]
    } else {
        to_rect.center()
    };

    adjusted[0] = intersect_svg_node(from_rect, from_target, from_node.shape);
    let last = adjusted.len() - 1;
    adjusted[last] = intersect_svg_node(to_rect, to_target, to_node.shape);

    adjusted
}

fn fix_corner_points(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let mut corner_positions = Vec::new();
    for i in 1..points.len() - 1 {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];
        let dx_prev = (curr.x - prev.x).abs();
        let dy_prev = (curr.y - prev.y).abs();
        let dx_next = (next.x - curr.x).abs();
        let dy_next = (next.y - curr.y).abs();

        let is_corner =
            (prev.x == curr.x && (curr.y - next.y).abs() > 5.0 && dx_next > 5.0 && dy_prev > 5.0)
                || (prev.y == curr.y
                    && (curr.x - next.x).abs() > 5.0
                    && dx_prev > 5.0
                    && dy_next > 5.0);

        if is_corner {
            corner_positions.push(i);
        }
    }

    if corner_positions.is_empty() {
        return points.to_vec();
    }

    let mut out = Vec::new();
    for i in 0..points.len() {
        if !corner_positions.contains(&i) {
            out.push(points[i]);
            continue;
        }

        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];

        let new_prev = find_adjacent_point(prev, curr, 5.0);
        let new_next = find_adjacent_point(next, curr, 5.0);

        let x_diff = new_next.x - new_prev.x;
        let y_diff = new_next.y - new_prev.y;
        out.push(new_prev);

        let mut new_corner = curr;
        let a = (2.0_f64).sqrt() * 2.0;
        if (next.x - prev.x).abs() > 10.0 && (next.y - prev.y).abs() >= 10.0 {
            let r = 5.0;
            if (curr.x - new_prev.x).abs() < f64::EPSILON {
                new_corner = Point {
                    x: if x_diff < 0.0 {
                        new_prev.x - r + a
                    } else {
                        new_prev.x + r - a
                    },
                    y: if y_diff < 0.0 {
                        new_prev.y - a
                    } else {
                        new_prev.y + a
                    },
                };
            } else {
                new_corner = Point {
                    x: if x_diff < 0.0 {
                        new_prev.x - a
                    } else {
                        new_prev.x + a
                    },
                    y: if y_diff < 0.0 {
                        new_prev.y - r + a
                    } else {
                        new_prev.y + r - a
                    },
                };
            }
        }

        out.push(new_corner);
        out.push(new_next);
    }

    out
}

fn find_adjacent_point(point_a: Point, point_b: Point, distance: f64) -> Point {
    let x_diff = point_b.x - point_a.x;
    let y_diff = point_b.y - point_a.y;
    let length = (x_diff * x_diff + y_diff * y_diff).sqrt();
    if length <= f64::EPSILON {
        return point_b;
    }
    let ratio = distance / length;
    Point {
        x: point_b.x - ratio * x_diff,
        y: point_b.y - ratio * y_diff,
    }
}

fn apply_marker_offsets(points: &[Point], edge: &Edge) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let start_offset = if edge.arrow_start == Arrow::Normal {
        4.0
    } else {
        0.0
    };
    let end_offset = if edge.arrow_end == Arrow::Normal {
        4.0
    } else {
        0.0
    };

    let start = points[0];
    let end = points[points.len() - 1];
    let direction_x = if start.x < end.x { "left" } else { "right" };
    let direction_y = if start.y < end.y { "down" } else { "up" };

    let mut out = Vec::with_capacity(points.len());
    for (i, point) in points.iter().enumerate() {
        let mut offset_x = 0.0;
        if i == 0 && start_offset > 0.0 {
            offset_x = marker_offset_component(points[0], points[1], start_offset, true);
        } else if i == points.len() - 1 && end_offset > 0.0 {
            offset_x = marker_offset_component(
                points[points.len() - 1],
                points[points.len() - 2],
                end_offset,
                true,
            );
        }

        let diff_end = (point.x - end.x).abs();
        let diff_in_y_end = (point.y - end.y).abs();
        let diff_start = (point.x - start.x).abs();
        let diff_in_y_start = (point.y - start.y).abs();
        let extra_room = 1.0;

        if end_offset > 0.0 && diff_end < end_offset && diff_end > 0.0 && diff_in_y_end < end_offset
        {
            let mut adjustment = end_offset + extra_room - diff_end;
            if direction_x == "right" {
                adjustment *= -1.0;
            }
            offset_x -= adjustment;
        }

        if start_offset > 0.0
            && diff_start < start_offset
            && diff_start > 0.0
            && diff_in_y_start < start_offset
        {
            let mut adjustment = start_offset + extra_room - diff_start;
            if direction_x == "right" {
                adjustment *= -1.0;
            }
            offset_x += adjustment;
        }

        let mut offset_y = 0.0;
        if i == 0 && start_offset > 0.0 {
            offset_y = marker_offset_component(points[0], points[1], start_offset, false);
        } else if i == points.len() - 1 && end_offset > 0.0 {
            offset_y = marker_offset_component(
                points[points.len() - 1],
                points[points.len() - 2],
                end_offset,
                false,
            );
        }

        let diff_end_y = (point.y - end.y).abs();
        let diff_in_x_end = (point.x - end.x).abs();
        let diff_start_y = (point.y - start.y).abs();
        let diff_in_x_start = (point.x - start.x).abs();

        if end_offset > 0.0
            && diff_end_y < end_offset
            && diff_end_y > 0.0
            && diff_in_x_end < end_offset
        {
            let mut adjustment = end_offset + extra_room - diff_end_y;
            if direction_y == "up" {
                adjustment *= -1.0;
            }
            offset_y -= adjustment;
        }

        if start_offset > 0.0
            && diff_start_y < start_offset
            && diff_start_y > 0.0
            && diff_in_x_start < start_offset
        {
            let mut adjustment = start_offset + extra_room - diff_start_y;
            if direction_y == "up" {
                adjustment *= -1.0;
            }
            offset_y += adjustment;
        }

        out.push(Point {
            x: point.x + offset_x,
            y: point.y + offset_y,
        });
    }

    out
}

fn marker_offset_component(point_a: Point, point_b: Point, offset: f64, use_x: bool) -> f64 {
    let delta_x = point_b.x - point_a.x;
    let delta_y = point_b.y - point_a.y;
    let angle = if delta_x.abs() < f64::EPSILON {
        if delta_y >= 0.0 {
            std::f64::consts::FRAC_PI_2
        } else {
            -std::f64::consts::FRAC_PI_2
        }
    } else {
        (delta_y / delta_x).atan()
    };

    if use_x {
        offset * angle.cos() * if delta_x >= 0.0 { 1.0 } else { -1.0 }
    } else {
        offset * angle.sin().abs() * if delta_y >= 0.0 { 1.0 } else { -1.0 }
    }
}

fn intersect_svg_node(rect: &Rect, point: Point, shape: Shape) -> Point {
    match shape {
        Shape::Diamond | Shape::Hexagon => intersect_svg_diamond(rect, point),
        _ => intersect_svg_rect(rect, point),
    }
}

fn intersect_svg_rect(rect: &Rect, point: Point) -> Point {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let dx = point.x - cx;
    let dy = point.y - cy;
    let w = rect.width / 2.0;
    let h = rect.height / 2.0;

    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return Point { x: cx, y: cy + h };
    }

    let (sx, sy) = if dy.abs() * w > dx.abs() * h {
        let h = if dy < 0.0 { -h } else { h };
        (h * dx / dy, h)
    } else {
        let w = if dx < 0.0 { -w } else { w };
        (w, w * dy / dx)
    };

    Point {
        x: cx + sx,
        y: cy + sy,
    }
}

fn intersect_svg_diamond(rect: &Rect, point: Point) -> Point {
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let dx = point.x - cx;
    let dy = point.y - cy;
    let w = rect.width / 2.0;
    let h = rect.height / 2.0;

    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return Point { x: cx, y: cy + h };
    }

    let t = 1.0 / (dx.abs() / w + dy.abs() / h);
    Point {
        x: cx + t * dx,
        y: cy + t * dy,
    }
}

fn path_from_points_linear(points: &[(f64, f64)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            let _ = write!(d, "M{},{}", fmt_f64(*x), fmt_f64(*y));
        } else {
            let _ = write!(d, " L{},{}", fmt_f64(*x), fmt_f64(*y));
        }
    }
    d
}

fn path_from_points_basis(points: &[(f64, f64)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    if points.len() == 1 {
        let (x, y) = points[0];
        return format!("M{},{}", fmt_f64(x), fmt_f64(y));
    }

    let mut d = String::new();
    let mut x0 = f64::NAN;
    let mut x1 = f64::NAN;
    let mut y0 = f64::NAN;
    let mut y1 = f64::NAN;
    let mut point = 0;

    for &(x, y) in points {
        match point {
            0 => {
                point = 1;
                let _ = write!(d, "M{},{}", fmt_f64(x), fmt_f64(y));
            }
            1 => {
                point = 2;
            }
            2 => {
                point = 3;
                let px = (5.0 * x0 + x1) / 6.0;
                let py = (5.0 * y0 + y1) / 6.0;
                let _ = write!(d, " L{},{}", fmt_f64(px), fmt_f64(py));
                basis_bezier(&mut d, x0, y0, x1, y1, x, y);
            }
            _ => {
                basis_bezier(&mut d, x0, y0, x1, y1, x, y);
            }
        }
        x0 = x1;
        x1 = x;
        y0 = y1;
        y1 = y;
    }

    match point {
        3 => {
            basis_bezier(&mut d, x0, y0, x1, y1, x1, y1);
            let _ = write!(d, " L{},{}", fmt_f64(x1), fmt_f64(y1));
        }
        2 => {
            let _ = write!(d, " L{},{}", fmt_f64(x1), fmt_f64(y1));
        }
        _ => {}
    }

    d
}

fn basis_bezier(d: &mut String, x0: f64, y0: f64, x1: f64, y1: f64, x: f64, y: f64) {
    let c1x = (2.0 * x0 + x1) / 3.0;
    let c1y = (2.0 * y0 + y1) / 3.0;
    let c2x = (x0 + 2.0 * x1) / 3.0;
    let c2y = (y0 + 2.0 * y1) / 3.0;
    let ex = (x0 + 4.0 * x1 + x) / 6.0;
    let ey = (y0 + 4.0 * y1 + y) / 6.0;
    let _ = write!(
        d,
        " C{},{} {},{} {},{}",
        fmt_f64(c1x),
        fmt_f64(c1y),
        fmt_f64(c2x),
        fmt_f64(c2y),
        fmt_f64(ex),
        fmt_f64(ey)
    );
}

fn path_from_points_rounded(points: &[(f64, f64)], radius: f64) -> String {
    if points.is_empty() {
        return String::new();
    }
    if points.len() < 3 || radius <= 0.0 {
        return path_from_points_linear(points);
    }

    let mut d = String::new();
    let (x0, y0) = points[0];
    let _ = write!(d, "M{},{}", fmt_f64(x0), fmt_f64(y0));

    for i in 1..points.len() - 1 {
        let (px, py) = points[i - 1];
        let (cx, cy) = points[i];
        let (nx, ny) = points[i + 1];

        let v1x = cx - px;
        let v1y = cy - py;
        let v2x = nx - cx;
        let v2y = ny - cy;

        let len1 = (v1x * v1x + v1y * v1y).sqrt();
        let len2 = (v2x * v2x + v2y * v2y).sqrt();
        if len1 <= f64::EPSILON || len2 <= f64::EPSILON {
            let _ = write!(d, " L{},{}", fmt_f64(cx), fmt_f64(cy));
            continue;
        }

        let v1nx = v1x / len1;
        let v1ny = v1y / len1;
        let v2nx = v2x / len2;
        let v2ny = v2y / len2;

        let cross = v1nx * v2ny - v1ny * v2nx;
        let dot = v1nx * v2nx + v1ny * v2ny;
        if cross.abs() < 1e-3 && dot.abs() > 0.999 {
            let _ = write!(d, " L{},{}", fmt_f64(cx), fmt_f64(cy));
            continue;
        }

        let r = radius.min(len1 / 2.0).min(len2 / 2.0);
        if r <= f64::EPSILON {
            let _ = write!(d, " L{},{}", fmt_f64(cx), fmt_f64(cy));
            continue;
        }

        let p1x = cx - v1nx * r;
        let p1y = cy - v1ny * r;
        let p2x = cx + v2nx * r;
        let p2y = cy + v2ny * r;

        let _ = write!(d, " L{},{}", fmt_f64(p1x), fmt_f64(p1y));
        let _ = write!(
            d,
            " Q{},{} {},{}",
            fmt_f64(cx),
            fmt_f64(cy),
            fmt_f64(p2x),
            fmt_f64(p2y)
        );
    }

    let (lx, ly) = points[points.len() - 1];
    let _ = write!(d, " L{},{}", fmt_f64(lx), fmt_f64(ly));
    d
}

fn polygon_points(points: &[(f64, f64)]) -> String {
    let mut out = String::new();
    for (idx, (x, y)) in points.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{x},{y}", x = fmt_f64(*x), y = fmt_f64(*y));
    }
    out
}

fn scale_rect(rect: &Rect, scale: f64) -> Rect {
    Rect {
        x: rect.x * scale,
        y: rect.y * scale,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

fn compute_self_edge_paths(
    diagram: &Diagram,
    layout: &LayoutResult,
    metrics: &SvgTextMetrics,
) -> HashMap<usize, Vec<Point>> {
    let pad = metrics.node_padding_x.max(metrics.node_padding_y).max(4.0);
    let mut paths = HashMap::new();

    for edge in &layout.self_edges {
        let Some(rect) = layout.nodes.get(&edge.node) else {
            continue;
        };
        if edge.points.is_empty() {
            continue;
        }
        let adjusted = adjust_self_edge_points(rect, &edge.points, diagram.direction, pad);
        paths.insert(edge.edge_index, adjusted);
    }

    paths
}

fn adjust_self_edge_points(
    rect: &Rect,
    points: &[Point],
    direction: Direction,
    pad: f64,
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    match direction {
        Direction::TopDown => {
            let loop_x = points
                .iter()
                .map(|point| point.x)
                .fold(right, f64::max)
                .max(right + pad);
            vec![
                Point { x: right, y: top },
                Point { x: loop_x, y: top },
                Point {
                    x: loop_x,
                    y: bottom,
                },
                Point {
                    x: right,
                    y: bottom,
                },
            ]
        }
        Direction::BottomTop => {
            let loop_x = points
                .iter()
                .map(|point| point.x)
                .fold(right, f64::max)
                .max(right + pad);
            vec![
                Point {
                    x: right,
                    y: bottom,
                },
                Point {
                    x: loop_x,
                    y: bottom,
                },
                Point { x: loop_x, y: top },
                Point { x: right, y: top },
            ]
        }
        Direction::LeftRight => {
            let loop_y = points
                .iter()
                .map(|point| point.y)
                .fold(bottom, f64::max)
                .max(bottom + pad);
            vec![
                Point {
                    x: right,
                    y: bottom,
                },
                Point {
                    x: right,
                    y: loop_y,
                },
                Point { x: left, y: loop_y },
                Point { x: left, y: bottom },
            ]
        }
        Direction::RightLeft => {
            let loop_y = points
                .iter()
                .map(|point| point.y)
                .fold(bottom, f64::max)
                .max(bottom + pad);
            vec![
                Point { x: left, y: bottom },
                Point { x: left, y: loop_y },
                Point {
                    x: right,
                    y: loop_y,
                },
                Point {
                    x: right,
                    y: bottom,
                },
            ]
        }
    }
}

fn fallback_label_position(
    layout: &LayoutResult,
    edge_index: usize,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
) -> Option<Point> {
    if let Some(points) = self_edge_paths.get(&edge_index) {
        return points.get(points.len() / 2).copied();
    }

    let points = layout
        .edges
        .iter()
        .find(|edge| edge.index == edge_index)
        .map(|edge| edge.points.as_slice())
        .or_else(|| {
            layout
                .self_edges
                .iter()
                .find(|edge| edge.edge_index == edge_index)
                .map(|edge| edge.points.as_slice())
        })?;

    points.get(points.len() / 2).copied()
}

fn fmt_f64(value: f64) -> String {
    let mut v = value;
    if v.abs() < 0.005 {
        v = 0.0;
    }
    format!("{:.2}", v)
}

fn escape_text(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

struct SvgWriter {
    buf: String,
    indent: usize,
}

impl SvgWriter {
    fn new() -> Self {
        Self {
            buf: String::new(),
            indent: 0,
        }
    }

    fn start_svg(&mut self, width: f64, height: f64, font_family: &str, font_size: f64) {
        let view_width = fmt_f64(width);
        let view_height = fmt_f64(height);
        let view_box = format!("0 0 {view_width} {view_height}");
        let style = format!("max-width: {view_width}px; background-color: transparent;");
        let line = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" viewBox=\"{view_box}\" style=\"{style}\" font-family=\"{font}\" font-size=\"{font_size}\">",
            view_box = view_box,
            style = style,
            font = escape_text(font_family),
            font_size = fmt_f64(font_size)
        );
        self.push_line(&line);
        self.indent += 1;
    }

    fn end_svg(&mut self) {
        self.indent = self.indent.saturating_sub(1);
        self.push_line("</svg>");
    }

    fn start_tag(&mut self, line: &str) {
        self.push_line(line);
        self.indent += 1;
    }

    fn end_tag(&mut self, line: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.push_line(line);
    }

    fn start_group(&mut self, class_name: &str) {
        let line = format!("<g class=\"{class}\">", class = escape_text(class_name));
        self.start_tag(&line);
    }

    fn start_group_transform(&mut self, dx: f64, dy: f64) {
        let line = format!(
            "<g transform=\"translate({x},{y})\">",
            x = fmt_f64(dx),
            y = fmt_f64(dy)
        );
        self.start_tag(&line);
    }

    fn end_group(&mut self) {
        self.end_tag("</g>");
    }

    fn push_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.buf.push_str("  ");
        }
        self.buf.push_str(line);
        self.buf.push('\n');
    }

    fn finish(self) -> String {
        self.buf
    }
}
