use std::collections::HashMap;

use mmdflux::render::{RenderOptions, render_svg};
use mmdflux::{build_diagram, parse_flowchart};

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
            rest.find('"').and_then(|end| rest[..end].parse::<f64>().ok())
        });
        // Extract text content between >...</text>
        let label = line.find("</text>").and_then(|end| {
            let before = &line[..end];
            before.rfind('>').map(|start| before[start + 1..].to_string())
        });
        if let (Some(x), Some(label)) = (x_val, label) {
            if !label.is_empty() {
                positions.insert(label, x);
            }
        }
    }
    positions
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
    let input = std::fs::read_to_string("tests/fixtures/subgraph_direction_lr.mmd").unwrap();
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
        std::fs::read_to_string("tests/fixtures/subgraph_direction_cross_boundary.mmd").unwrap();
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
    assert!(
        !svg.contains("NaN"),
        "SVG should not contain NaN values"
    );
}
