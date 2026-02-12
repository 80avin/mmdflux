//! SVG rendering for flowchart diagrams.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use super::super::geometry::{self, GraphGeometry};
use super::layout::{
    build_dagre_layout, center_override_subgraphs, compute_sublayouts, dagre_config_for_layout,
    expand_parent_bounds_dagre, reconcile_sublayouts_dagre, resolve_sublayout_overlaps,
};
use super::route_policy::effective_edge_direction;
use super::routing_core::build_orthogonal_path_float;
use super::svg_metrics::SvgTextMetrics;
use super::svg_router;
use super::unified_router::{UnifiedRoutingOptions, route_edges_unified};
use crate::dagre::{LayoutResult, Point, Rect};
use crate::diagram::{PathDetail, RoutingMode, RoutingPolicyToggles, SvgEdgePathStyle};
use crate::graph::{Arrow, Diagram, Direction, Edge, Node, Shape, Stroke};
use crate::render::{RenderOptions, layout_config_for_diagram};

const STROKE_COLOR: &str = "#333";
const SUBGRAPH_STROKE: &str = "#888";
const NODE_FILL: &str = "white";
const TEXT_COLOR: &str = "#333";

pub fn render_svg(diagram: &Diagram, options: &RenderOptions) -> String {
    let svg_options = &options.svg;
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

    let direction = diagram.direction;
    let mut layout = build_dagre_layout(
        diagram,
        &config,
        |node| svg_node_dimensions(&metrics, node, direction),
        |edge| {
            edge.label
                .as_ref()
                .map(|label| metrics.edge_label_dimensions(label))
        },
    );

    let dagre_config = dagre_config_for_layout(diagram, &config);
    let sublayouts = compute_sublayouts(
        diagram,
        &dagre_config,
        |node| svg_node_dimensions(&metrics, node, direction),
        |edge| {
            edge.label
                .as_ref()
                .map(|label| metrics.edge_label_dimensions(label))
        },
    );
    let title_pad_y = metrics.font_size;
    let content_pad_y = metrics.font_size * 0.3;
    reconcile_sublayouts_dagre(
        diagram,
        &mut layout,
        &sublayouts,
        title_pad_y,
        content_pad_y,
    );

    // Shift external predecessors of direction-override subgraphs to align with
    // the subgraph center.  Must happen after reconciliation (sublayout positions
    // finalized) but before overlap resolution and edge rerouting.
    center_override_subgraphs(diagram, &mut layout);

    // Expand parent subgraph bounds to encompass repositioned children.
    // Use node_padding as margin so parent borders don't overlap child borders
    // after apply_subgraph_svg_padding adds equal padding to both.
    // Title margin adds extra top space so child borders clear the parent's title.
    let child_margin = metrics.node_padding_x.max(metrics.node_padding_y);
    let title_margin = metrics.font_size;
    expand_parent_bounds_dagre(diagram, &mut layout, child_margin, title_margin);

    // Push external nodes that now overlap with reconciled subgraph bounds.
    // The gap must account for subgraph padding (added later) plus breathing room.
    let overlap_gap = metrics.node_padding_y + metrics.font_size;
    resolve_sublayout_overlaps(diagram, &mut layout, overlap_gap);

    // Align sibling nodes with their cross-boundary edge targets on the
    // cross-axis of the parent direction.  Must run after reconciliation
    // and overlap resolution but before edge rerouting.
    svg_router::align_cross_boundary_siblings_dagre(diagram, &mut layout);
    expand_parent_bounds_dagre(diagram, &mut layout, child_margin, title_margin);

    // Reroute edges affected by direction-override subgraphs.
    // This must happen after reconciliation moves nodes but before padding,
    // so routes use the reconciled node positions.
    let node_directions = svg_router::build_node_directions_svg(diagram);

    // Push cross-boundary edge endpoints apart before rerouting so that the
    // fresh orthogonal paths have enough room for a visible edge stem.
    svg_router::ensure_cross_boundary_edge_spacing(
        diagram,
        &mut layout,
        &node_directions,
        config.dagre_rank_sep,
    );

    let (_stats, rerouted_edges) =
        svg_router::reroute_override_edges(diagram, &mut layout, &node_directions);

    // Add padding to subgraph bounds for breathing room around nodes.
    let subgraph_pad_x = metrics.node_padding_x;
    let subgraph_pad_y = metrics.node_padding_y;
    apply_subgraph_svg_padding(diagram, &mut layout, subgraph_pad_x, subgraph_pad_y);

    // Push external nodes away from subgraph borders so that subgraph-as-node
    // edges have visible length comparable to normal edges.  Without this,
    // the subgraph padding eats into the gap dagre allocated.
    let min_edge_gap = config.dagre_rank_sep;
    ensure_subgraph_edge_spacing(diagram, &mut layout, min_edge_gap);

    // Reroute subgraph-as-node edges with fresh orthogonal paths computed from
    // padded subgraph bounds.  Must run after padding so endpoints land on the
    // visible subgraph border.
    let sg_node_rerouted = svg_router::reroute_subgraph_node_edges(diagram, &mut layout);
    let mut rerouted_edges = rerouted_edges;
    rerouted_edges.extend(sg_node_rerouted);

    // Convert post-processed LayoutResult to engine-agnostic GraphGeometry.
    // From this point on, rendering reads from `geom` instead of `layout`.
    let geom = geometry::from_dagre_layout(&layout, diagram);
    let geom = if options.routing_mode == Some(RoutingMode::UnifiedPreview) {
        inject_unified_preview_paths(diagram, &geom, options.routing_policies)
    } else {
        geom
    };
    if options.routing_mode == Some(RoutingMode::UnifiedPreview) {
        rerouted_edges.extend(geom.edges.iter().map(|edge| edge.index));
    }

    let override_nodes = svg_router::build_override_node_map(diagram);

    let routing_mode = options.routing_mode.unwrap_or(RoutingMode::FullCompute);
    render_svg_with_geometry_context(
        diagram,
        options,
        &geom,
        &rerouted_edges,
        &override_nodes,
        routing_mode,
    )
}

/// Render SVG directly from precomputed graph geometry.
///
/// This is used by runtime-selected engines that already produce `GraphGeometry`.
pub fn render_svg_from_geometry(
    diagram: &Diagram,
    options: &RenderOptions,
    geom: &GraphGeometry,
    routing_mode: RoutingMode,
) -> String {
    let rerouted_edges =
        rerouted_edge_indexes_for_mode(geom, routing_mode, options.svg.edge_path_style);
    let override_nodes = svg_router::build_override_node_map(diagram);
    render_svg_with_geometry_context(
        diagram,
        options,
        geom,
        &rerouted_edges,
        &override_nodes,
        routing_mode,
    )
}

fn rerouted_edge_indexes_for_mode(
    geom: &GraphGeometry,
    routing_mode: RoutingMode,
    edge_path_style: SvgEdgePathStyle,
) -> HashSet<usize> {
    match routing_mode {
        // Pass-through paths are already positioned by the layout engine
        // and should not receive extra shape clipping.
        RoutingMode::PassThroughClip => geom.edges.iter().map(|e| e.index).collect(),
        // Unified preview routes already encode endpoint intent and should not
        // be shape-adjusted again in SVG (all path styles).
        RoutingMode::UnifiedPreview => {
            let _ = edge_path_style;
            geom.edges.iter().map(|e| e.index).collect()
        }
        RoutingMode::FullCompute => HashSet::new(),
    }
}

fn inject_unified_preview_paths(
    diagram: &Diagram,
    geom: &GraphGeometry,
    policies: RoutingPolicyToggles,
) -> GraphGeometry {
    let routed = route_edges_unified(diagram, geom, UnifiedRoutingOptions::preview(policies));
    let mut updated = geom.clone();
    for edge in routed {
        if let Some(layout_edge) = updated.edges.iter_mut().find(|e| e.index == edge.index) {
            layout_edge.layout_path_hint = Some(edge.path);
        }
    }
    updated
}

fn render_svg_with_geometry_context(
    diagram: &Diagram,
    options: &RenderOptions,
    geom: &GraphGeometry,
    rerouted_edges: &HashSet<usize>,
    override_nodes: &HashMap<String, String>,
    routing_mode: RoutingMode,
) -> String {
    let svg_options = &options.svg;
    let scale = svg_options.scale;
    let metrics = SvgTextMetrics::new(
        svg_options.font_size,
        svg_options.node_padding_x,
        svg_options.node_padding_y,
    );

    let self_edge_paths = compute_self_edge_paths(diagram, geom, &metrics);
    let bounds = compute_svg_bounds(diagram, geom, &metrics, &self_edge_paths);
    let padding = svg_options.diagram_padding;
    let (min_x, min_y, max_x, max_y) = bounds.finalize(geom.bounds.width, geom.bounds.height);
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
    render_subgraphs(&mut writer, diagram, geom, &metrics, scale);
    render_edges(
        &mut writer,
        diagram,
        geom,
        &self_edge_paths,
        rerouted_edges,
        routing_mode,
        svg_options.edge_path_style,
        svg_options.edge_path_radius,
        scale,
        options.path_detail,
    );
    render_edge_labels(
        &mut writer,
        diagram,
        geom,
        &self_edge_paths,
        override_nodes,
        &metrics,
        scale,
    );
    render_nodes(&mut writer, diagram, geom, &metrics, scale);
    writer.end_group();

    writer.end_svg();
    writer.finish()
}

fn apply_subgraph_svg_padding(
    diagram: &Diagram,
    layout: &mut LayoutResult,
    pad_x: f64,
    pad_y: f64,
) {
    if pad_x <= 0.0 && pad_y <= 0.0 {
        return;
    }

    for (id, rect) in layout.subgraph_bounds.iter_mut() {
        rect.x -= pad_x;
        rect.y -= pad_y;
        rect.width = (rect.width + pad_x * 2.0).max(0.0);
        rect.height = (rect.height + pad_y * 2.0).max(0.0);

        if let Some(node_rect) = layout.nodes.get_mut(&crate::dagre::NodeId(id.clone())) {
            *node_rect = *rect;
        }
    }

    // Ensure all subgraph IDs exist in nodes map for bounds updates.
    for (id, rect) in layout.subgraph_bounds.iter() {
        if !layout.nodes.contains_key(&crate::dagre::NodeId(id.clone()))
            && diagram.subgraphs.contains_key(id)
        {
            layout.nodes.insert(crate::dagre::NodeId(id.clone()), *rect);
        }
    }
}

/// Push external nodes away from subgraph borders for subgraph-as-node edges.
///
/// After `apply_subgraph_svg_padding` expands subgraph bounds, the gap between
/// external nodes and the visible subgraph border can be much smaller than a
/// normal inter-rank edge.  This function ensures those gaps are at least
/// `min_gap`, matching the visual weight of normal edges.
fn ensure_subgraph_edge_spacing(diagram: &Diagram, layout: &mut LayoutResult, min_gap: f64) {
    for edge in &diagram.edges {
        if edge.stroke == Stroke::Invisible {
            continue;
        }

        // external node → subgraph
        if let Some(sg_id) = &edge.to_subgraph
            && edge.from_subgraph.is_none()
        {
            push_node_from_subgraph(layout, &edge.from, sg_id, diagram.direction, min_gap, true);
        }

        // subgraph → external node
        if let Some(sg_id) = &edge.from_subgraph
            && edge.to_subgraph.is_none()
        {
            push_node_from_subgraph(layout, &edge.to, sg_id, diagram.direction, min_gap, false);
        }

        // subgraph → subgraph
        if let (Some(from_sg), Some(to_sg)) = (&edge.from_subgraph, &edge.to_subgraph) {
            push_subgraph_from_subgraph(
                diagram,
                layout,
                from_sg,
                to_sg,
                diagram.direction,
                min_gap,
            );
        }
    }
}

/// Push a single node away from a subgraph border if the gap is below `min_gap`.
///
/// `node_is_upstream` is true when the node is the source (exits toward the
/// subgraph) and false when it is the target (the subgraph exits toward it).
fn push_node_from_subgraph(
    layout: &mut LayoutResult,
    node_id: &str,
    sg_id: &str,
    direction: Direction,
    min_gap: f64,
    node_is_upstream: bool,
) {
    let node_key = crate::dagre::NodeId(node_id.to_string());
    let sg_rect = match layout.subgraph_bounds.get(sg_id) {
        Some(r) => *r,
        None => return,
    };
    let node_rect = match layout.nodes.get(&node_key) {
        Some(r) => *r,
        None => return,
    };

    // Compute the gap between the node face and the subgraph face along the
    // flow axis.  "upstream trailing edge → downstream leading edge".
    let gap = if node_is_upstream {
        // node (source) → subgraph (target)
        match direction {
            Direction::TopDown => sg_rect.y - (node_rect.y + node_rect.height),
            Direction::BottomTop => node_rect.y - (sg_rect.y + sg_rect.height),
            Direction::LeftRight => sg_rect.x - (node_rect.x + node_rect.width),
            Direction::RightLeft => node_rect.x - (sg_rect.x + sg_rect.width),
        }
    } else {
        // subgraph (source) → node (target)
        match direction {
            Direction::TopDown => node_rect.y - (sg_rect.y + sg_rect.height),
            Direction::BottomTop => sg_rect.y - (node_rect.y + node_rect.height),
            Direction::LeftRight => node_rect.x - (sg_rect.x + sg_rect.width),
            Direction::RightLeft => sg_rect.x - (node_rect.x + node_rect.width),
        }
    };

    if gap >= min_gap {
        return;
    }

    let shift = min_gap - gap;
    let node_rect = layout.nodes.get_mut(&node_key).unwrap();

    // Push the node away from the subgraph (against flow for upstream,
    // with flow for downstream).
    if node_is_upstream {
        match direction {
            Direction::TopDown => node_rect.y -= shift,
            Direction::BottomTop => node_rect.y += shift,
            Direction::LeftRight => node_rect.x -= shift,
            Direction::RightLeft => node_rect.x += shift,
        }
    } else {
        match direction {
            Direction::TopDown => node_rect.y += shift,
            Direction::BottomTop => node_rect.y -= shift,
            Direction::LeftRight => node_rect.x += shift,
            Direction::RightLeft => node_rect.x -= shift,
        }
    }
}

/// Push the downstream subgraph (and all its member nodes) away from the
/// upstream subgraph so the visible gap between their borders is at least
/// `min_gap`.
fn push_subgraph_from_subgraph(
    diagram: &Diagram,
    layout: &mut LayoutResult,
    from_sg: &str,
    to_sg: &str,
    direction: Direction,
    min_gap: f64,
) {
    let from_rect = match layout.subgraph_bounds.get(from_sg) {
        Some(r) => *r,
        None => return,
    };
    let to_rect = match layout.subgraph_bounds.get(to_sg) {
        Some(r) => *r,
        None => return,
    };

    let gap = match direction {
        Direction::TopDown => to_rect.y - (from_rect.y + from_rect.height),
        Direction::BottomTop => from_rect.y - (to_rect.y + to_rect.height),
        Direction::LeftRight => to_rect.x - (from_rect.x + from_rect.width),
        Direction::RightLeft => from_rect.x - (to_rect.x + to_rect.width),
    };

    if gap >= min_gap {
        return;
    }

    let shift = min_gap - gap;

    // Collect all node IDs in the downstream subgraph (including nested).
    let mut member_nodes = Vec::new();
    let mut sg_stack = vec![to_sg.to_string()];
    while let Some(sg_id) = sg_stack.pop() {
        if let Some(sg) = diagram.subgraphs.get(&sg_id) {
            for node_id in &sg.nodes {
                if diagram.is_subgraph(node_id) {
                    sg_stack.push(node_id.clone());
                } else {
                    member_nodes.push(node_id.clone());
                }
            }
        }
    }

    // Shift each member node.
    for node_id in &member_nodes {
        let key = crate::dagre::NodeId(node_id.clone());
        if let Some(rect) = layout.nodes.get_mut(&key) {
            match direction {
                Direction::TopDown => rect.y += shift,
                Direction::BottomTop => rect.y -= shift,
                Direction::LeftRight => rect.x += shift,
                Direction::RightLeft => rect.x -= shift,
            }
        }
    }

    // Shift the downstream subgraph bounds (and any nested subgraph bounds).
    let mut bounds_to_shift = vec![to_sg.to_string()];
    let mut i = 0;
    while i < bounds_to_shift.len() {
        let children = diagram.subgraph_children(&bounds_to_shift[i]);
        for child in children {
            bounds_to_shift.push(child.clone());
        }
        i += 1;
    }
    for sg_id in &bounds_to_shift {
        if let Some(rect) = layout.subgraph_bounds.get_mut(sg_id.as_str()) {
            match direction {
                Direction::TopDown => rect.y += shift,
                Direction::BottomTop => rect.y -= shift,
                Direction::LeftRight => rect.x += shift,
                Direction::RightLeft => rect.x -= shift,
            }
        }
        // Also update the nodes map entry for the subgraph.
        let key = crate::dagre::NodeId(sg_id.clone());
        if let Some(rect) = layout.nodes.get_mut(&key) {
            match direction {
                Direction::TopDown => rect.y += shift,
                Direction::BottomTop => rect.y -= shift,
                Direction::LeftRight => rect.x += shift,
                Direction::RightLeft => rect.x -= shift,
            }
        }
    }
}

pub(crate) fn svg_node_dimensions(
    metrics: &SvgTextMetrics,
    node: &Node,
    direction: Direction,
) -> (f64, f64) {
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
        Shape::Stadium => {
            // Stadium ends are semicircles that eat into the content area.
            // Add extra width so the text has room between the rounded ends.
            let h = label_h + metrics.node_padding_y * 2.0;
            let radius = h / 2.0;
            (label_w + metrics.node_padding_x * 2.0 + radius, h)
        }
        Shape::Cylinder => {
            // Cylinder needs extra height for the top and bottom ellipse caps.
            let w = label_w + metrics.node_padding_x * 2.0;
            let rx = w / 2.0;
            let ry = rx / (2.5 + w / 50.0);
            (w, label_h + metrics.node_padding_y * 2.0 + ry)
        }
        Shape::Document => {
            let w = label_w + metrics.node_padding_x * 2.0;
            let h = label_h + metrics.node_padding_y * 2.0;
            (w, h + h / 8.0) // wave amplitude = content_h / 8
        }
        Shape::Documents => {
            let w = label_w + metrics.node_padding_x * 2.0;
            let h = label_h + metrics.node_padding_y * 2.0;
            let offset = 5.0;
            (w + 2.0 * offset, h + h / 4.0 + 2.0 * offset) // wave amp = h/4
        }
        Shape::TaggedDocument => {
            let w = label_w + metrics.node_padding_x * 2.0;
            let h = label_h + metrics.node_padding_y * 3.0; // extra padding for taller shape
            (w * 1.1, h + h / 4.0) // 10% wider, wave amp = h/4
        }
        Shape::Card => {
            let w = label_w + metrics.node_padding_x * 2.0;
            let h = label_h + metrics.node_padding_y * 2.0;
            (w + 12.0, h) // extra width for corner fold
        }
        Shape::TaggedRect => {
            let w = label_w + metrics.node_padding_x * 2.0;
            let h = label_h + metrics.node_padding_y * 2.0;
            (w + 0.2 * h, h) // extra width for tag triangle
        }
        Shape::SmallCircle => {
            // UML initial node: fixed small size (Mermaid uses radius=7)
            (14.0, 14.0)
        }
        Shape::FramedCircle => {
            // UML activity final node: medium fixed size
            (28.0, 28.0)
        }
        Shape::CrossedCircle => {
            // UML flow final node: larger fixed size (Mermaid uses radius=30)
            (60.0, 60.0)
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
        Shape::Circle
        | Shape::DoubleCircle
        | Shape::SmallCircle
        | Shape::FramedCircle
        | Shape::CrossedCircle => {
            let size = width.max(height);
            width = size;
            height = size;
        }
        _ => {}
    }

    // ForkJoin bars are perpendicular to flow: swap dimensions for LR/RL.
    if node.shape == Shape::ForkJoin
        && node.label.trim().is_empty()
        && matches!(direction, Direction::LeftRight | Direction::RightLeft)
    {
        std::mem::swap(&mut width, &mut height);
    }

    (width, height)
}

fn render_defs(writer: &mut SvgWriter, scale: f64) {
    let base = 10.0;
    let half = base / 2.0;
    let marker_size = 8.0 * scale;

    writer.start_tag("<defs>");

    // Normal arrowhead (triangle)
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

    // Cross marker (X shape)
    let cross_size = 11.0;
    let cross_marker_size = 11.0 * scale;
    let marker = format!(
        "<marker id=\"crosshead\" viewBox=\"0 0 {size} {size}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        size = fmt_f64(cross_size),
        ref_x = fmt_f64(12.0),
        ref_y = fmt_f64(5.2),
        mw = fmt_f64(cross_marker_size),
        mh = fmt_f64(cross_marker_size)
    );
    writer.start_tag(&marker);
    let path = format!(
        "<path d=\"M 1,1 l 9,9 M 10,1 l -9,9\" stroke=\"{color}\" stroke-width=\"2\" />",
        color = STROKE_COLOR
    );
    writer.push_line(&path);
    writer.end_tag("</marker>");

    // Circle marker (filled circle)
    let circle_size = 10.0;
    let circle_marker_size = 11.0 * scale;
    let marker = format!(
        "<marker id=\"circlehead\" viewBox=\"0 0 {size} {size}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        size = fmt_f64(circle_size),
        ref_x = fmt_f64(11.0),
        ref_y = fmt_f64(5.0),
        mw = fmt_f64(circle_marker_size),
        mh = fmt_f64(circle_marker_size)
    );
    writer.start_tag(&marker);
    let circle = format!(
        "<circle cx=\"5\" cy=\"5\" r=\"5\" stroke=\"{color}\" stroke-width=\"1\" fill=\"{color}\" />",
        color = STROKE_COLOR
    );
    writer.push_line(&circle);
    writer.end_tag("</marker>");

    // Open arrowhead (hollow triangle for inheritance)
    let marker = format!(
        "<marker id=\"open-arrowhead\" viewBox=\"0 0 {base} {base}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        base = fmt_f64(base),
        ref_x = fmt_f64(half),
        ref_y = fmt_f64(half),
        mw = fmt_f64(marker_size),
        mh = fmt_f64(marker_size)
    );
    writer.start_tag(&marker);
    let polygon = format!(
        "<polygon points=\"0,0 {tip},{mid} 0,{size}\" fill=\"white\" stroke=\"{color}\" stroke-width=\"1\" />",
        tip = fmt_f64(base),
        mid = fmt_f64(half),
        size = fmt_f64(base),
        color = STROKE_COLOR
    );
    writer.push_line(&polygon);
    writer.end_tag("</marker>");

    // Diamond marker (filled diamond for composition)
    let diamond_size = 12.0;
    let diamond_half = diamond_size / 2.0;
    let diamond_marker_size = 12.0 * scale;
    let marker = format!(
        "<marker id=\"diamondhead\" viewBox=\"0 0 {size} {size}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        size = fmt_f64(diamond_size),
        ref_x = fmt_f64(diamond_half),
        ref_y = fmt_f64(diamond_half),
        mw = fmt_f64(diamond_marker_size),
        mh = fmt_f64(diamond_marker_size)
    );
    writer.start_tag(&marker);
    let polygon = format!(
        "<polygon points=\"0,{mid} {mid},0 {size},{mid} {mid},{size}\" fill=\"{color}\" />",
        mid = fmt_f64(diamond_half),
        size = fmt_f64(diamond_size),
        color = STROKE_COLOR
    );
    writer.push_line(&polygon);
    writer.end_tag("</marker>");

    // Open diamond marker (hollow diamond for aggregation)
    let marker = format!(
        "<marker id=\"open-diamondhead\" viewBox=\"0 0 {size} {size}\" refX=\"{ref_x}\" refY=\"{ref_y}\" markerWidth=\"{mw}\" markerHeight=\"{mh}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\">",
        size = fmt_f64(diamond_size),
        ref_x = fmt_f64(diamond_half),
        ref_y = fmt_f64(diamond_half),
        mw = fmt_f64(diamond_marker_size),
        mh = fmt_f64(diamond_marker_size)
    );
    writer.start_tag(&marker);
    let polygon = format!(
        "<polygon points=\"0,{mid} {mid},0 {size},{mid} {mid},{size}\" fill=\"white\" stroke=\"{color}\" stroke-width=\"1\" />",
        mid = fmt_f64(diamond_half),
        size = fmt_f64(diamond_size),
        color = STROKE_COLOR
    );
    writer.push_line(&polygon);
    writer.end_tag("</marker>");

    writer.end_tag("</defs>");
}

fn render_subgraphs(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    geom: &GraphGeometry,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    if geom.subgraphs.is_empty() {
        return;
    }

    let mut subgraphs: Vec<_> = geom
        .subgraphs
        .iter()
        .filter_map(|(id, sg_geom)| diagram.subgraphs.get(id).map(|_| (id, sg_geom)))
        .collect();

    subgraphs.sort_by(|a, b| a.1.depth.cmp(&b.1.depth).then_with(|| a.0.cmp(b.0)));

    writer.start_group("clusters");
    for (_id, sg_geom) in subgraphs {
        let rect = scale_rect(&sg_geom.rect.into(), scale);
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

        if !sg_geom.title.trim().is_empty() {
            let title_x = rect.x + rect.width / 2.0;
            let title_y = rect.y + metrics.font_size * 0.25;
            let text = format!(
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"middle\" dominant-baseline=\"hanging\" fill=\"{color}\">{label}</text>",
                x = fmt_f64(title_x),
                y = fmt_f64(title_y),
                color = TEXT_COLOR,
                label = escape_text(&sg_geom.title)
            );
            writer.push_line(&text);
        }
    }
    writer.end_group();
}

#[allow(clippy::too_many_arguments)]
fn render_edges(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    geom: &GraphGeometry,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
    rerouted_edges: &std::collections::HashSet<usize>,
    routing_mode: RoutingMode,
    edge_path_style: SvgEdgePathStyle,
    edge_path_radius: f64,
    scale: f64,
    path_detail: PathDetail,
) {
    let mut edge_paths: Vec<(usize, Vec<Point>)> = geom
        .edges
        .iter()
        .map(|edge| {
            let points: Vec<Point> = edge
                .layout_path_hint
                .as_ref()
                .map(|ps| ps.iter().map(|p| (*p).into()).collect())
                .unwrap_or_default();
            (edge.index, points)
        })
        .collect();
    edge_paths.extend(geom.self_edges.iter().map(|se| {
        let points = self_edge_paths
            .get(&se.edge_index)
            .cloned()
            .unwrap_or_else(|| se.points.iter().map(|p| (*p).into()).collect());
        (se.edge_index, points)
    }));
    edge_paths.sort_by_key(|(index, _)| *index);

    writer.start_group("edgePaths");
    for (index, points) in edge_paths {
        let Some(edge) = diagram.edges.get(index) else {
            continue;
        };
        if edge.stroke == Stroke::Invisible {
            continue;
        }
        let mut points = points;
        let edge_direction = if matches!(routing_mode, RoutingMode::UnifiedPreview) {
            effective_edge_direction(
                &geom.node_directions,
                &edge.from,
                &edge.to,
                diagram.direction,
            )
        } else {
            diagram.direction
        };
        // Clip subgraph-as-node edges to subgraph borders (skip for rerouted
        // edges whose endpoints already land on the subgraph border).
        if !rerouted_edges.contains(&index) {
            if let Some(sg_id) = edge.from_subgraph.as_ref()
                && let Some(sg_geom) = geom.subgraphs.get(sg_id)
            {
                points = clip_points_to_rect_start(&points, &sg_geom.rect.into());
            }
            if let Some(sg_id) = edge.to_subgraph.as_ref()
                && let Some(sg_geom) = geom.subgraphs.get(sg_id)
            {
                points = clip_points_to_rect_end(&points, &sg_geom.rect.into());
            }
        }

        // Preserve prior rerouted-edge behavior for healthy paths, but still
        // reclip when endpoints detach from expected faces or use non-rect
        // shapes (diamond/hexagon).
        let rerouted = rerouted_edges.contains(&index);
        let should_adjust = !matches!(routing_mode, RoutingMode::PassThroughClip)
            && (!rerouted
                || (matches!(routing_mode, RoutingMode::UnifiedPreview)
                    && should_adjust_rerouted_edge_endpoints(
                        diagram,
                        geom,
                        edge,
                        &points,
                        edge_direction,
                    )));
        let mut points = if should_adjust {
            adjust_edge_points_for_shapes(diagram, geom, edge, &points)
        } else {
            points
        };
        // Only densify corners for linear edges; basis and rounded
        // handle smoothing natively from sparse waypoints.
        if matches!(edge_path_style, SvgEdgePathStyle::Linear) {
            points = fix_corner_points(&points);
        }
        let is_backward = geom.reversed_edges.contains(&index);
        let enforce_primary_axis_no_backtrack = matches!(routing_mode, RoutingMode::UnifiedPreview)
            && !matches!(edge_path_style, SvgEdgePathStyle::Orthogonal)
            && !is_backward
            && edge.from != edge.to;
        points = apply_marker_offsets(
            &points,
            edge,
            edge_direction,
            !matches!(edge_path_style, SvgEdgePathStyle::Linear),
            enforce_primary_axis_no_backtrack,
            matches!(edge_path_style, SvgEdgePathStyle::Orthogonal),
        );
        let d = path_from_points(
            &points,
            diagram.direction,
            scale,
            edge_path_style,
            edge_path_radius,
            path_detail,
        );
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

fn point_inside_rect(rect: &Rect, point: Point) -> bool {
    let eps = 0.01;
    point.x > rect.x + eps
        && point.x < rect.x + rect.width - eps
        && point.y > rect.y + eps
        && point.y < rect.y + rect.height - eps
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentAxis {
    Horizontal,
    Vertical,
}

fn segment_axis(start: Point, end: Point) -> Option<SegmentAxis> {
    const EPS: f64 = 1e-6;
    let dx = (start.x - end.x).abs();
    let dy = (start.y - end.y).abs();
    if dx <= EPS && dy > EPS {
        Some(SegmentAxis::Vertical)
    } else if dy <= EPS && dx > EPS {
        Some(SegmentAxis::Horizontal)
    } else {
        None
    }
}

fn segment_manhattan_len(start: Point, end: Point) -> f64 {
    (start.x - end.x).abs() + (start.y - end.y).abs()
}

fn compact_visual_staircases(points: &[Point], short_tol: f64) -> Vec<Point> {
    if points.len() < 4 {
        return points.to_vec();
    }

    let mut compacted = points.to_vec();
    let mut i = 0usize;
    while i + 3 < compacted.len() {
        // Preserve start/end approach geometry so marker orientation keeps a
        // clear supporting segment into/out of the endpoint.
        if i == 0 || i + 3 >= compacted.len() - 1 {
            i += 1;
            continue;
        }

        let p0 = compacted[i];
        let p1 = compacted[i + 1];
        let p2 = compacted[i + 2];
        let p3 = compacted[i + 3];

        let a1 = segment_axis(p0, p1);
        let a2 = segment_axis(p1, p2);
        let a3 = segment_axis(p2, p3);

        let Some(first_axis) = a1 else {
            i += 1;
            continue;
        };
        let Some(middle_axis) = a2 else {
            i += 1;
            continue;
        };
        let Some(last_axis) = a3 else {
            i += 1;
            continue;
        };

        if first_axis != last_axis || first_axis == middle_axis {
            i += 1;
            continue;
        }

        let l1 = segment_manhattan_len(p0, p1);
        let l2 = segment_manhattan_len(p1, p2);
        let l3 = segment_manhattan_len(p2, p3);
        if l1 > short_tol || l2 > short_tol || l3 > short_tol {
            i += 1;
            continue;
        }

        let replacement = match first_axis {
            SegmentAxis::Vertical => Point { x: p0.x, y: p3.y },
            SegmentAxis::Horizontal => Point { x: p3.x, y: p0.y },
        };
        compacted.splice(i + 1..=i + 2, [replacement]);
        i = i.saturating_sub(1);
    }

    compacted
}

fn segment_rect_intersection(start: Point, end: Point, rect: &Rect) -> Option<Point> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return None;
    }

    let mut candidates: Vec<(f64, Point)> = Vec::new();

    let x_min = rect.x;
    let x_max = rect.x + rect.width;
    let y_min = rect.y;
    let y_max = rect.y + rect.height;

    if dx.abs() > f64::EPSILON {
        let t_left = (x_min - start.x) / dx;
        if (0.0..=1.0).contains(&t_left) {
            let y = start.y + t_left * dy;
            if y >= y_min && y <= y_max {
                candidates.push((t_left, Point { x: x_min, y }));
            }
        }
        let t_right = (x_max - start.x) / dx;
        if (0.0..=1.0).contains(&t_right) {
            let y = start.y + t_right * dy;
            if y >= y_min && y <= y_max {
                candidates.push((t_right, Point { x: x_max, y }));
            }
        }
    }

    if dy.abs() > f64::EPSILON {
        let t_top = (y_min - start.y) / dy;
        if (0.0..=1.0).contains(&t_top) {
            let x = start.x + t_top * dx;
            if x >= x_min && x <= x_max {
                candidates.push((t_top, Point { x, y: y_min }));
            }
        }
        let t_bottom = (y_max - start.y) / dy;
        if (0.0..=1.0).contains(&t_bottom) {
            let x = start.x + t_bottom * dx;
            if x >= x_min && x <= x_max {
                candidates.push((t_bottom, Point { x, y: y_max }));
            }
        }
    }

    candidates
        .into_iter()
        .filter(|(t, _)| *t >= 0.0 && *t <= 1.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, point)| point)
}

fn clip_points_to_rect_start(points: &[Point], rect: &Rect) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    if !point_inside_rect(rect, points[0]) {
        return points.to_vec();
    }

    let mut idx = 0usize;
    while idx + 1 < points.len() && point_inside_rect(rect, points[idx]) {
        idx += 1;
    }
    if idx == 0 || idx >= points.len() {
        return points.to_vec();
    }

    let inside = points[idx - 1];
    let outside = points[idx];
    let intersection = segment_rect_intersection(inside, outside, rect).unwrap_or(inside);

    let mut out = Vec::new();
    out.push(intersection);
    out.extend_from_slice(&points[idx..]);
    out
}

fn clip_points_to_rect_end(points: &[Point], rect: &Rect) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let last_idx = points.len() - 1;
    if !point_inside_rect(rect, points[last_idx]) {
        return points.to_vec();
    }

    let mut idx = last_idx;
    while idx > 0 && point_inside_rect(rect, points[idx]) {
        idx -= 1;
    }
    if idx == last_idx || idx >= last_idx {
        return points.to_vec();
    }

    let outside = points[idx];
    let inside = points[idx + 1];
    let intersection = segment_rect_intersection(outside, inside, rect).unwrap_or(inside);

    let mut out = points[..=idx].to_vec();
    out.push(intersection);
    out
}

fn render_edge_labels(
    writer: &mut SvgWriter,
    diagram: &Diagram,
    geom: &GraphGeometry,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
    override_nodes: &HashMap<String, String>,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    // Pre-build label position lookup from GraphGeometry edges.
    let label_positions: HashMap<usize, Point> = geom
        .edges
        .iter()
        .filter_map(|e| e.label_position.map(|p| (e.index, p.into())))
        .collect();

    writer.start_group("edgeLabels");

    for edge in diagram.edges.iter() {
        if edge.stroke == Stroke::Invisible {
            continue;
        }
        let Some(label) = edge.label.as_ref() else {
            continue;
        };
        let edge_idx = edge.index;
        let cross_boundary = if edge.from_subgraph.is_none() && edge.to_subgraph.is_none() {
            let from_override = override_nodes.get(&edge.from);
            let to_override = override_nodes.get(&edge.to);
            matches!(
                (from_override, to_override),
                (Some(a), Some(b)) if a != b
            ) || matches!(
                (from_override, to_override),
                (Some(_), None) | (None, Some(_))
            )
        } else {
            false
        };
        let use_precomputed =
            edge.from_subgraph.is_none() && edge.to_subgraph.is_none() && !cross_boundary;
        let position = if use_precomputed {
            label_positions.get(&edge_idx).copied()
        } else {
            None
        }
        .or_else(|| fallback_label_position(geom, edge_idx, self_edge_paths));
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
    geom: &GraphGeometry,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    writer.start_group("nodes");

    let mut node_ids: Vec<&String> = diagram.nodes.keys().collect();
    node_ids.sort();

    for node_id in node_ids {
        let node = &diagram.nodes[node_id];
        let Some(pos_node) = geom.nodes.get(node_id) else {
            continue;
        };
        let rect: Rect = pos_node.rect.into();
        render_node_shape(writer, node, &rect, scale, diagram.direction);

        let center = rect.center();
        let mut text_x = center.x;
        let mut text_y = center.y;
        // Offset text downward for cylinders so it centers in the body below the top cap.
        if node.shape == Shape::Cylinder {
            let rx = rect.width / 2.0;
            let ry = rx / (2.5 + rect.width / 50.0);
            text_y += ry / 2.0;
        }
        // Offset text upward for document shapes to center in content area above wave.
        if node.shape == Shape::Document {
            let wave_amp = rect.height / 9.0;
            text_y -= wave_amp / 2.0;
        }
        if node.shape == Shape::TaggedDocument {
            let wave_amp = rect.height / 5.0;
            text_y -= wave_amp / 2.0;
        }
        if node.shape == Shape::Documents {
            let offset = 5.0;
            let front_h = rect.height - 2.0 * offset;
            let wave_amp = front_h / 5.0;
            text_y += offset - wave_amp / 2.0;
            text_x -= offset; // front doc is shifted left
        }
        render_node_label(
            writer,
            text_x * scale,
            text_y * scale,
            &node.label,
            &rect,
            metrics,
            scale,
        );
    }

    writer.end_group();
}

/// Render a node's label, converting `Node::SEPARATOR` lines into horizontal rules.
fn render_node_label(
    writer: &mut SvgWriter,
    x: f64,
    y: f64,
    text: &str,
    rect: &Rect,
    metrics: &SvgTextMetrics,
    scale: f64,
) {
    let lines: Vec<&str> = text.split('\n').collect();
    let has_separator = lines.contains(&Node::SEPARATOR);

    if !has_separator {
        render_text_centered(writer, x, y, text, metrics, scale);
        return;
    }

    let line_height = metrics.line_height * scale;
    let total_height = line_height * (lines.len().saturating_sub(1) as f64);
    let start_y = y - total_height / 2.0;
    let x1 = rect.x * scale;
    let x2 = (rect.x + rect.width) * scale;
    // Left-align x: node left edge + padding (matches text renderer's x+2 convention)
    let left_x = x1 + metrics.node_padding_x * scale;
    let mut past_separator = false;

    for (idx, line_text) in lines.iter().enumerate() {
        let line_y = start_y + line_height * idx as f64;
        if *line_text == Node::SEPARATOR {
            past_separator = true;
            let line = format!(
                "<line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke=\"{stroke}\" stroke-width=\"{sw}\" />",
                x1 = fmt_f64(x1),
                y = fmt_f64(line_y),
                x2 = fmt_f64(x2),
                stroke = STROKE_COLOR,
                sw = fmt_f64(1.0 * scale),
            );
            writer.push_line(&line);
        } else if past_separator {
            // Members: left-aligned
            let line = format!(
                "<text x=\"{x}\" y=\"{y}\" text-anchor=\"start\" dominant-baseline=\"middle\" fill=\"{color}\">{text}</text>",
                x = fmt_f64(left_x),
                y = fmt_f64(line_y),
                color = TEXT_COLOR,
                text = escape_text(line_text)
            );
            writer.push_line(&line);
        } else {
            // Class name: centered
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
}

fn render_node_shape(
    writer: &mut SvgWriter,
    node: &Node,
    rect: &Rect,
    scale: f64,
    direction: Direction,
) {
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
            let radius = 5.0 * scale;
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
        Shape::Document => {
            // Single closed path with sine wave bottom (matching Mermaid waveEdgedRectangle).
            // wave_amp = content_h/8; total_h = content_h + wave_amp = 9/8 * content_h
            let wave_amp = rect.height / 9.0;
            let d = document_svg_path(rect.x, rect.y, rect.width, rect.height, wave_amp);
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));
        }
        Shape::Documents => {
            // Three stacked document paths (back → middle → front), each filled white.
            // Front doc covers the others; back docs peek out at top-right.
            let offset = 5.0 * scale;
            let doc_w = rect.width - 2.0 * offset;
            let doc_h = rect.height - 2.0 * offset;
            // wave_amp = content_h/4; doc_h = content_h + wave_amp = 5/4 * content_h
            let wave_amp = doc_h / 5.0;
            // Back document (top-right)
            let d = document_svg_path(rect.x + 2.0 * offset, rect.y, doc_w, doc_h, wave_amp);
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));
            // Middle document
            let d = document_svg_path(rect.x + offset, rect.y + offset, doc_w, doc_h, wave_amp);
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));
            // Front document
            let d = document_svg_path(rect.x, rect.y + 2.0 * offset, doc_w, doc_h, wave_amp);
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));
        }
        Shape::TaggedDocument => {
            // Document with sine wave bottom + page fold at bottom-right.
            // wave_amp = content_h/4; total_h = content_h + wave_amp = 5/4 * content_h
            let wave_amp = rect.height / 5.0;
            let wave_y = rect.y + rect.height - wave_amp;
            let freq = std::f64::consts::TAU * 0.8 / rect.width;

            // Main document path with wave bottom.
            let d = document_svg_path(rect.x, rect.y, rect.width, rect.height, wave_amp);
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));

            // Fold at bottom-right corner: a white-filled shape that covers the
            // wave in that area and shows a diagonal fold line.
            let content_h = rect.height - wave_amp;
            let fold_w = 0.2 * rect.width;
            let fold_h = 0.25 * content_h;
            let right_x = rect.x + rect.width;
            let fold_left_x = right_x - fold_w;

            // Wave Y at the fold's left edge.
            let t_left = (fold_left_x - rect.x) / rect.width;
            let y_fold_left = wave_y + wave_amp * (freq * t_left * rect.width).sin();
            let fold_top_y = y_fold_left - fold_h;

            // Build fold shape: follow the wave from fold_left to right edge,
            // then up to fold_top; Z closes with the diagonal (the fold line).
            let steps = 50usize;
            let i_start = (t_left * steps as f64).ceil() as usize;
            let mut fold_d = format!("M{},{}", fmt_f64(fold_left_x), fmt_f64(y_fold_left));
            for i in i_start..=steps {
                let t = i as f64 / steps as f64;
                let x = rect.x + t * rect.width;
                let y = wave_y + wave_amp * (freq * t * rect.width).sin();
                let _ = write!(fold_d, " L{},{}", fmt_f64(x), fmt_f64(y));
            }
            let _ = write!(fold_d, " L{},{}", fmt_f64(right_x), fmt_f64(fold_top_y));
            fold_d.push_str(" Z");
            writer.push_line(&format!(
                "<path d=\"{fold_d}\" fill=\"{NODE_FILL}\" stroke=\"{STROKE_COLOR}\" stroke-width=\"{stroke_width}\" />"
            ));
        }
        Shape::Card => {
            // Polygon with cut corner at top-left (matching Mermaid card shape).
            let fold = 12.0 * scale;
            let x = rect.x;
            let y = rect.y;
            let w = rect.width;
            let h = rect.height;
            let d = format!(
                "M{},{} L{},{} L{},{} L{},{} L{},{} Z",
                fmt_f64(x + fold),
                fmt_f64(y),
                fmt_f64(x + w),
                fmt_f64(y),
                fmt_f64(x + w),
                fmt_f64(y + h),
                fmt_f64(x),
                fmt_f64(y + h),
                fmt_f64(x),
                fmt_f64(y + fold),
            );
            writer.push_line(&format!("<path d=\"{d}\"{style} />"));
        }
        Shape::TaggedRect => {
            // Rectangle with triangle tag at bottom-right (matching Mermaid taggedRect).
            let line = format!(
                "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"{style} />",
                x = fmt_f64(rect.x),
                y = fmt_f64(rect.y),
                w = fmt_f64(rect.width),
                h = fmt_f64(rect.height),
                style = style
            );
            writer.push_line(&line);
            // Triangle tag at bottom-right
            let tag = 0.2 * rect.height;
            let tag_d = format!(
                "M{},{} L{},{} L{},{} Z",
                fmt_f64(rect.x + rect.width - tag),
                fmt_f64(rect.y + rect.height),
                fmt_f64(rect.x + rect.width),
                fmt_f64(rect.y + rect.height),
                fmt_f64(rect.x + rect.width),
                fmt_f64(rect.y + rect.height - tag),
            );
            writer.push_line(&format!(
                "<path d=\"{tag_d}\" fill=\"{NODE_FILL}\" stroke=\"{STROKE_COLOR}\" stroke-width=\"{stroke_width}\" />"
            ));
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
        Shape::SmallCircle => {
            // UML initial node: small filled circle
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let radius = rect.width.min(rect.height) / 2.0;
            let circle = format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\" stroke-linejoin=\"round\" />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                r = fmt_f64(radius),
                fill = STROKE_COLOR,
                stroke = STROKE_COLOR,
                sw = stroke_width
            );
            writer.push_line(&circle);
        }
        Shape::FramedCircle => {
            // UML activity final node: outer circle with filled inner circle
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let outer_radius = rect.width.min(rect.height) / 2.0;
            let gap = 5.0 * scale;
            let inner_radius = outer_radius - gap;
            let outer = format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                r = fmt_f64(outer_radius),
                style = style
            );
            writer.push_line(&outer);
            let inner = format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"{sw}\" stroke-linejoin=\"round\" />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                r = fmt_f64(inner_radius),
                fill = STROKE_COLOR,
                stroke = STROKE_COLOR,
                sw = stroke_width
            );
            writer.push_line(&inner);
        }
        Shape::CrossedCircle => {
            // UML flow final node: circle with diagonal cross
            let cx = rect.x + rect.width / 2.0;
            let cy = rect.y + rect.height / 2.0;
            let radius = rect.width.min(rect.height) / 2.0;
            let circle = format!(
                "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\"{style} />",
                cx = fmt_f64(cx),
                cy = fmt_f64(cy),
                r = fmt_f64(radius),
                style = style
            );
            writer.push_line(&circle);
            let stroke_attr = format!(
                " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\"",
                stroke = STROKE_COLOR,
                stroke_width = stroke_width
            );
            // Cross lines span the full radius at 45 degrees
            let d = radius * std::f64::consts::FRAC_1_SQRT_2;
            let line1 = format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{stroke} />",
                x1 = fmt_f64(cx - d),
                y1 = fmt_f64(cy - d),
                x2 = fmt_f64(cx + d),
                y2 = fmt_f64(cy + d),
                stroke = stroke_attr
            );
            let line2 = format!(
                "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\"{stroke} />",
                x1 = fmt_f64(cx - d),
                y1 = fmt_f64(cy + d),
                x2 = fmt_f64(cx + d),
                y2 = fmt_f64(cy - d),
                stroke = stroke_attr
            );
            writer.push_line(&line1);
            writer.push_line(&line2);
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

            let inset = 8.0 * scale;
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
            // 3D cylinder: full ellipse at top, straight sides, half-ellipse at bottom.
            let rx = rect.width / 2.0;
            let ry = rx / (2.5 + rect.width / 50.0);
            let x0 = rect.x;
            let x1 = rect.x + rect.width;
            let top = rect.y + ry;
            let bot = rect.y + rect.height - ry;

            // Outer path: top ellipse (back + front arcs), sides, bottom arc
            let d = format!(
                "M{x0},{top} A{rx},{ry} 0 0,0 {x1},{top} A{rx},{ry} 0 0,0 {x0},{top} L{x0},{bot} A{rx},{ry} 0 0,0 {x1},{bot} L{x1},{top}",
                x0 = fmt_f64(x0),
                x1 = fmt_f64(x1),
                top = fmt_f64(top),
                bot = fmt_f64(bot),
                rx = fmt_f64(rx),
                ry = fmt_f64(ry),
            );
            let body = format!("<path d=\"{d}\"{style} />", d = d, style = style);
            writer.push_line(&body);

            // Inner line: front edge of top ellipse (creates the 3D rim)
            let inner_d = format!(
                "M{x0},{top} A{rx},{ry} 0 0,1 {x1},{top}",
                x0 = fmt_f64(x0),
                x1 = fmt_f64(x1),
                top = fmt_f64(top),
                rx = fmt_f64(rx),
                ry = fmt_f64(ry),
            );
            let inner_style = format!(
                " fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{sw}\"",
                stroke = STROKE_COLOR,
                sw = stroke_width,
            );
            let inner = format!("<path d=\"{inner_d}\"{inner_style} />");
            writer.push_line(&inner);
        }
        Shape::TextBlock => {
            // Borderless: only text will be drawn.
        }
        Shape::ForkJoin => {
            if matches!(direction, Direction::LeftRight | Direction::RightLeft) {
                // Vertical bar for horizontal flow
                let x = rect.x + rect.width / 2.0;
                let stroke = format!(
                    " stroke=\"{stroke}\" stroke-width=\"{stroke_width}\" stroke-linecap=\"square\"",
                    stroke = STROKE_COLOR,
                    stroke_width = fmt_f64((rect.width * 0.3).max(3.0 * scale))
                );
                let line = format!(
                    "<line x1=\"{x}\" y1=\"{y1}\" x2=\"{x}\" y2=\"{y2}\"{stroke} />",
                    x = fmt_f64(x),
                    y1 = fmt_f64(rect.y),
                    y2 = fmt_f64(rect.y + rect.height),
                    stroke = stroke
                );
                writer.push_line(&line);
            } else {
                // Horizontal bar for vertical flow
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
    geom: &GraphGeometry,
    metrics: &SvgTextMetrics,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
) -> SvgBounds {
    let mut bounds = SvgBounds::new();

    for pos_node in geom.nodes.values() {
        bounds.update_rect(&pos_node.rect.into());
    }

    for sg_geom in geom.subgraphs.values() {
        bounds.update_rect(&sg_geom.rect.into());
    }

    let is_invisible = |index: usize| -> bool {
        diagram
            .edges
            .get(index)
            .is_some_and(|e| e.stroke == Stroke::Invisible)
    };

    for layout_edge in &geom.edges {
        if layout_edge.index >= diagram.edges.len() || is_invisible(layout_edge.index) {
            continue;
        }
        if let Some(path) = &layout_edge.layout_path_hint {
            for point in path {
                bounds.update_point(point.x, point.y);
            }
        }
    }

    for se in &geom.self_edges {
        if is_invisible(se.edge_index) {
            continue;
        }
        if let Some(computed) = self_edge_paths.get(&se.edge_index) {
            for point in computed {
                bounds.update_point(point.x, point.y);
            }
        } else {
            for point in &se.points {
                bounds.update_point(point.x, point.y);
            }
        }
    }

    // Pre-build label position lookup from GraphGeometry edges.
    let label_positions: HashMap<usize, Point> = geom
        .edges
        .iter()
        .filter_map(|e| e.label_position.map(|p| (e.index, p.into())))
        .collect();

    for edge in diagram.edges.iter() {
        if edge.stroke == Stroke::Invisible {
            continue;
        }
        let Some(label) = edge.label.as_ref() else {
            continue;
        };
        let edge_idx = edge.index;
        let use_precomputed = edge.from_subgraph.is_none() && edge.to_subgraph.is_none();
        let position = if use_precomputed {
            label_positions.get(&edge_idx).copied()
        } else {
            None
        }
        .or_else(|| fallback_label_position(geom, edge_idx, self_edge_paths));
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
    match edge.arrow_start {
        Arrow::Normal => attrs.push_str(" marker-start=\"url(#arrowhead)\""),
        Arrow::Cross => attrs.push_str(" marker-start=\"url(#crosshead)\""),
        Arrow::Circle => attrs.push_str(" marker-start=\"url(#circlehead)\""),
        Arrow::OpenTriangle => attrs.push_str(" marker-start=\"url(#open-arrowhead)\""),
        Arrow::Diamond => attrs.push_str(" marker-start=\"url(#diamondhead)\""),
        Arrow::OpenDiamond => attrs.push_str(" marker-start=\"url(#open-diamondhead)\""),
        Arrow::None => {}
    }
    match edge.arrow_end {
        Arrow::Normal => attrs.push_str(" marker-end=\"url(#arrowhead)\""),
        Arrow::Cross => attrs.push_str(" marker-end=\"url(#crosshead)\""),
        Arrow::Circle => attrs.push_str(" marker-end=\"url(#circlehead)\""),
        Arrow::OpenTriangle => attrs.push_str(" marker-end=\"url(#open-arrowhead)\""),
        Arrow::Diamond => attrs.push_str(" marker-end=\"url(#diamondhead)\""),
        Arrow::OpenDiamond => attrs.push_str(" marker-end=\"url(#open-diamondhead)\""),
        Arrow::None => {}
    }
    attrs
}

fn path_from_points(
    points: &[Point],
    direction: Direction,
    scale: f64,
    curve: SvgEdgePathStyle,
    curve_radius: f64,
    path_detail: PathDetail,
) -> String {
    if points.is_empty() {
        return String::new();
    }
    let points: Vec<Point> =
        if matches!(curve, SvgEdgePathStyle::Orthogonal) && !points_are_axis_aligned(points) {
            let start: geometry::FPoint = points[0].into();
            let end: geometry::FPoint = points.last().copied().unwrap_or(points[0]).into();
            let waypoints: Vec<geometry::FPoint> = points
                .iter()
                .copied()
                .skip(1)
                .take(points.len().saturating_sub(2))
                .map(Into::into)
                .collect();
            build_orthogonal_path_float(start, end, direction, &waypoints)
                .into_iter()
                .map(Into::into)
                .collect()
        } else {
            points.to_vec()
        };
    let points = match path_detail {
        PathDetail::Full => points,
        PathDetail::Compact => {
            let compacted = compact_visual_staircases(&points, 12.0);
            PathDetail::Compact.simplify_with_coords(&compacted, |point| (point.x, point.y))
        }
        PathDetail::Simplified if matches!(curve, SvgEdgePathStyle::Orthogonal) => {
            simplify_orthogonal_points(&points, direction)
        }
        _ => path_detail.simplify_with_coords(&points, |point| (point.x, point.y)),
    };
    let scaled: Vec<(f64, f64)> = points
        .iter()
        .map(|point| (point.x * scale, point.y * scale))
        .collect();
    match curve {
        SvgEdgePathStyle::Basis => path_from_points_basis(&scaled),
        SvgEdgePathStyle::Rounded => path_from_points_rounded(&scaled, curve_radius * scale),
        SvgEdgePathStyle::Linear => path_from_points_linear(&scaled),
        SvgEdgePathStyle::Orthogonal => path_from_points_linear(&scaled),
    }
}

fn simplify_orthogonal_points(points: &[Point], direction: Direction) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let start = points[0];
    let end = points[points.len() - 1];
    if segment_axis(start, end).is_some() {
        return vec![start, end];
    }

    let elbow = match direction {
        Direction::TopDown | Direction::BottomTop => Point {
            x: start.x,
            y: end.y,
        },
        Direction::LeftRight | Direction::RightLeft => Point {
            x: end.x,
            y: start.y,
        },
    };
    vec![start, elbow, end]
}

type EndpointShapeRect = (Rect, Shape);

fn edge_endpoint_shape_rects(
    diagram: &Diagram,
    geom: &GraphGeometry,
    edge: &Edge,
) -> Option<(EndpointShapeRect, EndpointShapeRect)> {
    let from = if let Some(sg_id) = edge.from_subgraph.as_ref() {
        let sg_rect: Rect = geom.subgraphs.get(sg_id)?.rect.into();
        (sg_rect, Shape::Rectangle)
    } else {
        let node_rect: Rect = geom.nodes.get(&edge.from)?.rect.into();
        let node = diagram.nodes.get(&edge.from)?;
        (node_rect, node.shape)
    };

    let to = if let Some(sg_id) = edge.to_subgraph.as_ref() {
        let sg_rect: Rect = geom.subgraphs.get(sg_id)?.rect.into();
        (sg_rect, Shape::Rectangle)
    } else {
        let node_rect: Rect = geom.nodes.get(&edge.to)?.rect.into();
        let node = diagram.nodes.get(&edge.to)?;
        (node_rect, node.shape)
    };

    Some((from, to))
}

fn should_adjust_rerouted_edge_endpoints(
    diagram: &Diagram,
    geom: &GraphGeometry,
    edge: &Edge,
    points: &[Point],
    direction: Direction,
) -> bool {
    const EPS: f64 = 0.5;
    if points.len() < 2 {
        return false;
    }

    let Some(((from_rect, from_shape), (to_rect, to_shape))) =
        edge_endpoint_shape_rects(diagram, geom, edge)
    else {
        return false;
    };

    if matches!(from_shape, Shape::Diamond | Shape::Hexagon)
        || matches!(to_shape, Shape::Diamond | Shape::Hexagon)
    {
        return true;
    }

    let is_backward = geom.reversed_edges.contains(&edge.index);

    endpoint_attachment_is_invalid(points[0], from_rect, direction, true, is_backward, EPS)
        || endpoint_attachment_is_invalid(
            points[points.len() - 1],
            to_rect,
            direction,
            false,
            is_backward,
            EPS,
        )
}

fn endpoint_attachment_is_invalid(
    point: Point,
    rect: Rect,
    direction: Direction,
    is_source: bool,
    is_backward: bool,
    eps: f64,
) -> bool {
    const FACE_PROXIMITY: f64 = 6.0;
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;
    if point.x < left - eps
        || point.x > right + eps
        || point.y < top - eps
        || point.y > bottom + eps
    {
        return true;
    }

    let is_forward_source = is_source != is_backward;

    match direction {
        Direction::TopDown => {
            if is_forward_source {
                (bottom - point.y) > FACE_PROXIMITY
            } else {
                (point.y - top) > FACE_PROXIMITY
            }
        }
        Direction::BottomTop => {
            if is_forward_source {
                (point.y - top) > FACE_PROXIMITY
            } else {
                (bottom - point.y) > FACE_PROXIMITY
            }
        }
        Direction::LeftRight => {
            if is_forward_source {
                (right - point.x) > FACE_PROXIMITY
            } else {
                (point.x - left) > FACE_PROXIMITY
            }
        }
        Direction::RightLeft => {
            if is_forward_source {
                (point.x - left) > FACE_PROXIMITY
            } else {
                (right - point.x) > FACE_PROXIMITY
            }
        }
    }
}

fn adjust_edge_points_for_shapes(
    diagram: &Diagram,
    geom: &GraphGeometry,
    edge: &Edge,
    points: &[Point],
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let Some(((from_rect, from_shape), (to_rect, to_shape))) =
        edge_endpoint_shape_rects(diagram, geom, edge)
    else {
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

    adjusted[0] = intersect_svg_node(&from_rect, from_target, from_shape);
    let last = adjusted.len() - 1;
    adjusted[last] = intersect_svg_node(&to_rect, to_target, to_shape);

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

fn points_are_axis_aligned(points: &[Point]) -> bool {
    if points.len() < 2 {
        return true;
    }
    points
        .windows(2)
        .all(|seg| segment_axis(seg[0], seg[1]).is_some())
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

fn apply_marker_offsets(
    points: &[Point],
    edge: &Edge,
    direction: Direction,
    allow_interior_nudges: bool,
    enforce_primary_axis_no_backtrack: bool,
    preserve_orthogonal: bool,
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut start_offset: f64 = match edge.arrow_start {
        Arrow::Normal | Arrow::OpenTriangle => 4.0,
        Arrow::Diamond | Arrow::OpenDiamond => 5.0,
        // Cross and circle markers have refX past the visible shape,
        // so the marker already sits before the endpoint — no pullback needed.
        Arrow::Cross | Arrow::Circle | Arrow::None => 0.0,
    };
    let mut end_offset: f64 = match edge.arrow_end {
        Arrow::Normal | Arrow::OpenTriangle => 4.0,
        Arrow::Diamond | Arrow::OpenDiamond => 5.0,
        Arrow::Cross | Arrow::Circle | Arrow::None => 0.0,
    };

    let mut points = points.to_vec();
    if !preserve_orthogonal {
        // Non-orth styles (linear/rounded/basis) can look visually cramped when
        // an orthogonal route ends with a short final elbow immediately before
        // the marker. Collapse that elbow into a direct terminal approach.
        points = collapse_narrow_terminal_elbows_for_non_orth(&points, 14.0);
    }
    if preserve_orthogonal {
        // Keep endpoint support visibly longer than marker pullback so the
        // terminal stem remains readable in orthogonal mode.
        const MIN_ENDPOINT_SUPPORT: f64 = 12.0;
        points = enforce_min_orthogonal_endpoint_support(
            &points,
            start_offset + MIN_ENDPOINT_SUPPORT,
            end_offset + MIN_ENDPOINT_SUPPORT,
        );

        // Keep a visible endpoint stem in orthogonal mode so marker pullback
        // cannot invert the terminal segment direction.
        let start_support = segment_manhattan_len(points[0], points[1]);
        let end_support = segment_manhattan_len(points[points.len() - 2], points[points.len() - 1]);
        start_offset = start_offset.min((start_support - MIN_ENDPOINT_SUPPORT).max(0.0));
        end_offset = end_offset.min((end_support - MIN_ENDPOINT_SUPPORT).max(0.0));
    }

    let mut out = Vec::with_capacity(points.len());
    let start = points[0];
    let end = points[points.len() - 1];
    let direction_x = if start.x < end.x { "left" } else { "right" };
    let direction_y = if start.y < end.y { "down" } else { "up" };
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

        if allow_interior_nudges && !preserve_orthogonal {
            if end_offset > 0.0
                && diff_end < end_offset
                && diff_end > 0.0
                && diff_in_y_end < end_offset
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

        if allow_interior_nudges && !preserve_orthogonal {
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
        }

        out.push(Point {
            x: point.x + offset_x,
            y: point.y + offset_y,
        });
    }

    if enforce_primary_axis_no_backtrack && !preserve_orthogonal {
        enforce_primary_axis_tail_contracts(&mut out, direction, 8.0);
    }

    out
}

fn collapse_narrow_terminal_elbows_for_non_orth(
    points: &[Point],
    min_terminal_leg: f64,
) -> Vec<Point> {
    if points.len() < 4 || min_terminal_leg <= 0.0 {
        return points.to_vec();
    }

    let mut collapsed = points.to_vec();

    if collapsed.len() >= 4 {
        let n = collapsed.len();
        let pre = collapsed[n - 3];
        let elbow = collapsed[n - 2];
        let end = collapsed[n - 1];
        let pre_axis = segment_axis(pre, elbow);
        let end_axis = segment_axis(elbow, end);
        if let (Some(a), Some(b)) = (pre_axis, end_axis)
            && a != b
            && segment_manhattan_len(elbow, end) < min_terminal_leg
            && segment_manhattan_len(pre, end) > 0.001
        {
            collapsed.remove(n - 2);
        }
    }

    if collapsed.len() >= 4 {
        let start = collapsed[0];
        let elbow = collapsed[1];
        let post = collapsed[2];
        let start_axis = segment_axis(start, elbow);
        let post_axis = segment_axis(elbow, post);
        if let (Some(a), Some(b)) = (start_axis, post_axis)
            && a != b
            && segment_manhattan_len(start, elbow) < min_terminal_leg
            && segment_manhattan_len(start, post) > 0.001
        {
            collapsed.remove(1);
        }
    }

    collapsed
}

fn enforce_primary_axis_tail_contracts(
    points: &mut [Point],
    direction: Direction,
    min_terminal_support: f64,
) {
    if points.len() < 2 || min_terminal_support <= 0.0 {
        return;
    }

    let n = points.len();
    let end_idx = n - 1;
    let penult_idx = n - 2;

    match direction {
        Direction::TopDown => {
            let target_penult_y = points[end_idx].y - min_terminal_support;
            if points[penult_idx].y > target_penult_y {
                points[penult_idx].y = target_penult_y;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].y > points[penult_idx].y {
                    points[pre_idx].y = points[penult_idx].y;
                }
            }
        }
        Direction::BottomTop => {
            let target_penult_y = points[end_idx].y + min_terminal_support;
            if points[penult_idx].y < target_penult_y {
                points[penult_idx].y = target_penult_y;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].y < points[penult_idx].y {
                    points[pre_idx].y = points[penult_idx].y;
                }
            }
        }
        Direction::LeftRight => {
            let target_penult_x = points[end_idx].x - min_terminal_support;
            if points[penult_idx].x > target_penult_x {
                points[penult_idx].x = target_penult_x;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].x > points[penult_idx].x {
                    points[pre_idx].x = points[penult_idx].x;
                }
            }
        }
        Direction::RightLeft => {
            let target_penult_x = points[end_idx].x + min_terminal_support;
            if points[penult_idx].x < target_penult_x {
                points[penult_idx].x = target_penult_x;
            }
            if n >= 3 {
                let pre_idx = n - 3;
                if points[pre_idx].x < points[penult_idx].x {
                    points[pre_idx].x = points[penult_idx].x;
                }
            }
        }
    }
}

fn enforce_min_orthogonal_endpoint_support(
    points: &[Point],
    min_start_support: f64,
    min_end_support: f64,
) -> Vec<Point> {
    let mut adjusted = points.to_vec();
    extend_endpoint_support(&mut adjusted, true, min_start_support);
    extend_endpoint_support(&mut adjusted, false, min_end_support);
    adjusted
}

fn extend_endpoint_support(points: &mut Vec<Point>, at_start: bool, min_support: f64) {
    if points.len() < 2 || min_support <= 0.0 {
        return;
    }

    let (anchor_idx, adjacent_idx, before_adjacent_idx, before_before_adjacent_idx) = if at_start {
        (0usize, 1usize, Some(2usize), Some(3usize))
    } else {
        let n = points.len();
        (n - 1, n - 2, n.checked_sub(3), n.checked_sub(4))
    };

    let anchor = points[anchor_idx];
    let adjacent = points[adjacent_idx];
    let Some(axis) = segment_axis(adjacent, anchor) else {
        return;
    };
    let current_support = segment_manhattan_len(adjacent, anchor);
    if current_support >= min_support {
        return;
    }

    let new_adjacent = match axis {
        SegmentAxis::Vertical => {
            let sign = if anchor.y >= adjacent.y { 1.0 } else { -1.0 };
            Point {
                x: anchor.x,
                y: anchor.y - sign * min_support,
            }
        }
        SegmentAxis::Horizontal => {
            let sign = if anchor.x >= adjacent.x { 1.0 } else { -1.0 };
            Point {
                x: anchor.x - sign * min_support,
                y: anchor.y,
            }
        }
    };

    points[adjacent_idx] = new_adjacent;
    let Some(before_adjacent_idx) = before_adjacent_idx else {
        return;
    };

    let before_adjacent = points[before_adjacent_idx];
    if segment_axis(before_adjacent, new_adjacent).is_some() {
        return;
    }

    // Prefer shifting the elbow coordinate (when possible) over inserting a
    // new jog segment; this avoids tiny backtracks near turns.
    let mut shifted_before = before_adjacent;
    match axis {
        SegmentAxis::Vertical => shifted_before.y = new_adjacent.y,
        SegmentAxis::Horizontal => shifted_before.x = new_adjacent.x,
    }
    let keeps_adjacent_axis = segment_axis(shifted_before, new_adjacent).is_some();
    let keeps_prev_axis = before_before_adjacent_idx
        .and_then(|idx| points.get(idx).copied())
        .is_none_or(|prev| segment_axis(prev, shifted_before).is_some());
    if keeps_adjacent_axis && keeps_prev_axis {
        points[before_adjacent_idx] = shifted_before;
        return;
    }

    let elbow = match axis {
        SegmentAxis::Vertical => Point {
            x: before_adjacent.x,
            y: new_adjacent.y,
        },
        SegmentAxis::Horizontal => Point {
            x: new_adjacent.x,
            y: before_adjacent.y,
        },
    };

    if at_start {
        points.insert(2, elbow);
    } else {
        let insert_at = points.len() - 2;
        points.insert(insert_at, elbow);
    }
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

/// Generate sine wave as SVG path L-segments (matching Mermaid's generateFullSineWavePoints).
/// The wave starts at (x_start, y_center) with sin(0)=0 and traverses `width` pixels
/// using 0.8 cycles and 50 line segments.
fn sine_wave_segments(x_start: f64, y_center: f64, width: f64, amplitude: f64) -> String {
    let steps = 50usize;
    let freq = std::f64::consts::TAU * 0.8 / width;
    let mut d = String::new();
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = x_start + t * width;
        let y = y_center + amplitude * (freq * t * width).sin();
        let _ = write!(d, " L{},{}", fmt_f64(x), fmt_f64(y));
    }
    d
}

/// Build a closed SVG path for a document shape (straight top/sides, sine wave bottom).
fn document_svg_path(x: f64, y: f64, w: f64, h: f64, wave_amp: f64) -> String {
    let wave_y = y + h - wave_amp;
    let mut d = format!("M{},{}", fmt_f64(x), fmt_f64(wave_y));
    d.push_str(&sine_wave_segments(x, wave_y, w, wave_amp));
    let _ = write!(d, " L{},{}", fmt_f64(x + w), fmt_f64(y));
    let _ = write!(d, " L{},{}", fmt_f64(x), fmt_f64(y));
    d.push_str(" Z");
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
    geom: &GraphGeometry,
    metrics: &SvgTextMetrics,
) -> HashMap<usize, Vec<Point>> {
    let pad = metrics.node_padding_x.max(metrics.node_padding_y).max(4.0);
    let mut paths = HashMap::new();

    for se in &geom.self_edges {
        let Some(pos_node) = geom.nodes.get(&se.node_id) else {
            continue;
        };
        if se.points.is_empty() {
            continue;
        }
        let dagre_rect: Rect = pos_node.rect.into();
        let dagre_points: Vec<Point> = se.points.iter().map(|p| (*p).into()).collect();
        let adjusted = adjust_self_edge_points(&dagre_rect, &dagre_points, diagram.direction, pad);
        paths.insert(se.edge_index, adjusted);
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
    geom: &GraphGeometry,
    edge_index: usize,
    self_edge_paths: &HashMap<usize, Vec<Point>>,
) -> Option<Point> {
    if let Some(points) = self_edge_paths.get(&edge_index) {
        return points.get(points.len() / 2).copied();
    }

    // Try regular edges via layout_path_hint
    if let Some(layout_edge) = geom.edges.iter().find(|e| e.index == edge_index)
        && let Some(path) = &layout_edge.layout_path_hint
    {
        return path.get(path.len() / 2).map(|p| (*p).into());
    }

    // Try self-edges
    if let Some(se) = geom.self_edges.iter().find(|e| e.edge_index == edge_index) {
        return se.points.get(se.points.len() / 2).map(|p| (*p).into());
    }

    None
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
