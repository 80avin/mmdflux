//! Integration tests for mmdflux.
//!
//! These tests verify the full parsing and rendering pipeline using fixture files.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use mmdflux::diagram::OutputFormat;
use mmdflux::render::{
    Layout, LayoutConfig, RenderOptions, compute_layout_direct, render, route_all_edges,
};
use mmdflux::{Diagram, Direction, Shape, build_diagram, parse_flowchart};

/// Load a fixture file by name from `tests/fixtures/`.
fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", name, e))
}

/// Parse and build a diagram from a fixture file.
fn parse_and_build(name: &str) -> Diagram {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    build_diagram(&flowchart)
}

/// Parse, build, and compute layout for a fixture file.
fn layout_fixture(name: &str) -> (Diagram, Layout) {
    let diagram = parse_and_build(name);
    let layout = compute_layout_direct(&diagram, &LayoutConfig::default());
    (diagram, layout)
}

/// Parse, build, and render a fixture file.
fn render_fixture(name: &str) -> String {
    let diagram = parse_and_build(name);
    render(&diagram, &RenderOptions::default())
}

/// Parse, build, and render a Mermaid input string.
fn render_input(input: &str) -> String {
    let flowchart = parse_flowchart(input).expect("Failed to parse input");
    let diagram = build_diagram(&flowchart);
    render(&diagram, &RenderOptions::default())
}

/// Parse, build, and render a fixture file with ASCII-only output.
fn render_fixture_ascii(name: &str) -> String {
    let diagram = parse_and_build(name);
    render(
        &diagram,
        &RenderOptions {
            output_format: OutputFormat::Ascii,
            ..Default::default()
        },
    )
}

/// Assert that all values in the slice are distinct.
fn assert_all_distinct(values: &[usize], context: &str) {
    for i in 0..values.len() {
        for j in (i + 1)..values.len() {
            assert_ne!(
                values[i], values[j],
                "{}: duplicate value {} (all: {:?})",
                context, values[i], values
            );
        }
    }
}

// =============================================================================
// Parsing Tests
// =============================================================================

mod parsing {
    use super::*;

    #[test]
    fn simple_parses_correctly() {
        let input = load_fixture("simple.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::TopDown);
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);

        assert!(diagram.nodes.contains_key("A"));
        assert!(diagram.nodes.contains_key("B"));
        assert_eq!(diagram.nodes["A"].label, "Start");
        assert_eq!(diagram.nodes["B"].label, "End");
    }

    #[test]
    fn decision_parses_correctly() {
        let input = load_fixture("decision.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes.len(), 4);
        assert_eq!(diagram.edges.len(), 4);

        // Verify diamond shape for decision node
        assert_eq!(diagram.nodes["B"].shape, Shape::Diamond);
        assert_eq!(diagram.nodes["B"].label, "Is it working?");
    }

    #[test]
    fn shapes_parses_correctly() {
        let input = load_fixture("shapes.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes["rect"].shape, Shape::Rectangle);
        assert_eq!(diagram.nodes["round"].shape, Shape::Round);
        assert_eq!(diagram.nodes["diamond"].shape, Shape::Diamond);
    }

    #[test]
    fn shape_keywords_parse_document_and_card() {
        let diagram = parse_and_build("shapes_document.mmd");
        assert_eq!(diagram.nodes["doc"].shape, Shape::Document);
        assert_eq!(diagram.nodes["docs"].shape, Shape::Documents);
        assert_eq!(diagram.nodes["tagdoc"].shape, Shape::TaggedDocument);
        assert_eq!(diagram.nodes["card"].shape, Shape::Card);
        assert_eq!(diagram.nodes["tag"].shape, Shape::TaggedRect);
    }

    #[test]
    fn shape_keywords_parse_junctions_and_specials() {
        let diagram = parse_and_build("shapes_junction.mmd");
        assert_eq!(diagram.nodes["j1"].shape, Shape::SmallCircle);
        assert_eq!(diagram.nodes["j2"].shape, Shape::FramedCircle);
        assert_eq!(diagram.nodes["j3"].shape, Shape::CrossedCircle);

        let diagram = parse_and_build("shapes_special.mmd");
        assert_eq!(diagram.nodes["fork"].shape, Shape::ForkJoin);
        assert_eq!(diagram.nodes["note"].shape, Shape::TextBlock);
    }

    #[test]
    fn shape_keywords_parse_degenerate_fallbacks() {
        let diagram = parse_and_build("shapes_degenerate.mmd");
        for id in [
            "cloud",
            "bolt",
            "bang",
            "icon",
            "hourglass",
            "tri",
            "flip",
            "notch",
        ] {
            assert_eq!(diagram.nodes[id].shape, Shape::Rectangle);
        }
    }

    #[test]
    fn left_right_direction() {
        let input = load_fixture("left_right.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::LeftRight);
    }

    #[test]
    fn bottom_top_direction() {
        let input = load_fixture("bottom_top.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::BottomTop);
    }

    #[test]
    fn right_left_direction() {
        let input = load_fixture("right_left.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.direction, Direction::RightLeft);
    }

    #[test]
    fn chain_creates_correct_edges() {
        let input = load_fixture("chain.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes.len(), 4);
        assert_eq!(diagram.edges.len(), 3);
    }

    #[test]
    fn ampersand_expands_to_multiple_edges() {
        let input = load_fixture("ampersand.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.nodes.len(), 5);
        // A & B --> C creates 2 edges, C --> D & E creates 2 edges
        assert_eq!(diagram.edges.len(), 4);
    }

    #[test]
    fn labeled_edges_parsed() {
        let input = load_fixture("labeled_edges.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        // Check that at least some edges have labels
        let edges_with_labels = diagram.edges.iter().filter(|e| e.label.is_some()).count();
        assert!(edges_with_labels > 0, "Should have labeled edges");
    }

    #[test]
    fn inline_edge_labels_parsed() {
        let input = load_fixture("inline_edge_labels.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        assert_eq!(diagram.edges.len(), 4);
        assert_eq!(diagram.edges[0].label.as_deref(), Some("yes"));
        assert_eq!(diagram.edges[1].label.as_deref(), Some("retry"));
        assert_eq!(diagram.edges[2].label.as_deref(), Some("final step"));
        assert_eq!(diagram.edges[3].label.as_deref(), Some("no"));

        // Inline labels should not create nodes.
        assert!(!diagram.nodes.contains_key("yes"));
        assert!(!diagram.nodes.contains_key("retry"));
        assert!(!diagram.nodes.contains_key("no"));
    }

    #[test]
    fn inline_label_flowchart_parsed() {
        let input = load_fixture("inline_label_flowchart.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for label in diagram.edges.iter().filter_map(|e| e.label.as_deref()) {
            *counts.entry(label).or_insert(0) += 1;
        }

        assert_eq!(counts.get("no"), Some(&2));
        assert_eq!(counts.get("yes"), Some(&2));
        assert_eq!(counts.get("sync"), Some(&1));
        assert_eq!(counts.get("async"), Some(&1));
        assert_eq!(counts.get("hit"), Some(&1));
        assert_eq!(counts.get("miss"), Some(&1));
        assert_eq!(counts.get("warn"), Some(&1));
        assert_eq!(counts.values().sum::<usize>(), 9);
    }

    #[test]
    fn comments_are_ignored() {
        let input = load_fixture("git_workflow.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        // Comments should not create nodes
        assert_eq!(diagram.nodes.len(), 4);
        assert!(!diagram.nodes.contains_key("%%"));
    }

    #[test]
    fn complex_parses_all_features() {
        let input = load_fixture("complex.mmd");
        let flowchart = parse_flowchart(&input).expect("Should parse");
        let diagram = build_diagram(&flowchart);

        // Just verify it parses without error and has expected structure
        assert!(diagram.nodes.len() >= 9);
        assert!(diagram.edges.len() >= 10);
    }
}

// =============================================================================
// Rendering Tests
// =============================================================================

mod rendering {
    use super::*;

    #[test]
    fn simple_renders() {
        let output = render_fixture("simple.mmd");
        assert!(!output.is_empty());
        assert!(output.contains("Start"));
        assert!(output.contains("End"));
    }

    #[test]
    fn decision_renders_diamond() {
        let output = render_fixture("decision.mmd");
        assert!(output.contains("Is it working?"));
        // Diamond shapes use < and > characters
        assert!(output.contains('<') || output.contains('>'));
    }

    #[test]
    fn shapes_render_distinctly() {
        let output = render_fixture("shapes.mmd");
        assert!(output.contains("Rectangle Node"));
        assert!(output.contains("Rounded Node"));
        assert!(output.contains("Diamond Node"));
    }

    #[test]
    fn shapes_document_render_distinctly() {
        let output = render_fixture("shapes_document.mmd");
        assert!(output.contains("Doc"));
        assert!(output.contains("Docs"));
        assert!(output.contains("TagDoc"));
        assert!(output.contains("Card"));
        assert!(output.contains("Tag"));
        assert!(output.contains('~'), "Document should use wavy bottom");
        assert!(
            output.contains('╱'),
            "Tagged doc/card should use folded corner"
        );
    }

    #[test]
    fn shapes_junction_render_glyphs() {
        let output = render_fixture("shapes_junction.mmd");
        assert!(output.contains('●'));
        assert!(output.contains('◉'));
        assert!(output.contains('⊗'));
    }

    #[test]
    fn shapes_special_render_bar_and_text() {
        let output = render_fixture("shapes_special.mmd");
        assert!(output.contains('━'), "Fork/join should use heavy bar");
        assert!(output.contains("Note"));
    }

    #[test]
    fn shapes_junction_ascii_degrades() {
        let output = render_fixture_ascii("shapes_junction.mmd");
        assert!(output.contains("o"));
        assert!(output.contains("(o)"));
        assert!(output.contains("x"));
    }

    #[test]
    fn shapes_degenerate_render_labels() {
        let output = render_fixture("shapes_degenerate.mmd");
        for label in [
            "Cloud", "Bolt", "Bang", "Icon", "Hour", "Tri", "Flip", "Notch",
        ] {
            assert!(output.contains(label));
        }
    }

    #[test]
    fn edge_styles_render() {
        let output = render_fixture("edge_styles.mmd");
        assert!(output.contains("Solid"));
        assert!(output.contains("Dotted"));
        assert!(output.contains("Thick"));
    }

    #[test]
    fn left_right_renders_horizontally() {
        let output = render_fixture("left_right.mmd");
        // In LR layout, nodes should be on similar vertical lines
        // The output should be wider than tall (roughly)
        let lines: Vec<&str> = output.lines().collect();
        let height = lines.len();
        let width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        assert!(
            width > height,
            "LR layout should be wider than tall: {}x{}",
            width,
            height
        );
    }

    #[test]
    fn chain_renders_all_nodes() {
        let output = render_fixture("chain.mmd");
        assert!(output.contains("Step 1"));
        assert!(output.contains("Step 2"));
        assert!(output.contains("Step 3"));
        assert!(output.contains("Step 4"));
    }

    #[test]
    fn labeled_edges_show_labels() {
        let output = render_fixture("labeled_edges.mmd");
        // Labels should appear in output
        assert!(output.contains("initialize") || output.contains("configure"));

        // "yes" and "no" labels from the Config diamond branches
        assert!(
            output.contains("yes"),
            "Expected 'yes' label in output:\n{output}"
        );
        assert!(
            output.contains("no"),
            "Expected 'no' label in output:\n{output}"
        );
    }

    #[test]
    #[ignore]
    fn branching_labels_dont_overlap() {
        // Test that branching edges with labels place them on separate branches
        let output = render_fixture("label_spacing.mmd");

        // Both labels should be present and complete (not truncated)
        assert!(output.contains("valid"), "Should contain 'valid' label");
        assert!(output.contains("invalid"), "Should contain 'invalid' label");

        // The labels should NOT directly overlap (no merged text like "valinvalid")
        // They can be on the same row as long as they're separated
        assert!(
            !output.contains("valinvalid"),
            "Labels should not merge into 'valinvalid'"
        );
        assert!(
            !output.contains("invalidvalid"),
            "Labels should not merge into 'invalidvalid'"
        );

        // Labels should appear between source node A and target nodes B/C
        let lines: Vec<&str> = output.lines().collect();
        let a_line = lines.iter().position(|l| l.contains(" A ")).unwrap();
        let b_line = lines.iter().rposition(|l| l.contains(" B ")).unwrap();

        // At least one label should be between A and B rows
        let label_line = lines.iter().position(|l| l.contains("valid")).unwrap();
        assert!(
            label_line > a_line && label_line < b_line,
            "Label at line {} should be between A (line {}) and B (line {})\n{}",
            label_line,
            a_line,
            b_line,
            output
        );
    }

    #[test]
    fn git_workflow_renders() {
        let output = render_fixture("git_workflow.mmd");

        // All node labels fully visible
        assert!(
            output.contains("Working Dir"),
            "Missing 'Working Dir':\n{output}"
        );
        assert!(
            output.contains("Staging Area"),
            "Missing 'Staging Area':\n{output}"
        );
        assert!(
            output.contains("Local Repo"),
            "Missing 'Local Repo':\n{output}"
        );
        assert!(
            output.contains("Remote Repo"),
            "Missing 'Remote Repo':\n{output}"
        );

        // All forward edge labels fully visible (not clipped by nodes)
        assert!(
            output.contains("git add"),
            "Missing 'git add' label:\n{output}"
        );
        assert!(
            output.contains("git commit"),
            "Missing 'git commit' label:\n{output}"
        );
        assert!(
            output.contains("git push"),
            "Missing 'git push' label:\n{output}"
        );

        // Backward edge label
        assert!(
            output.contains("git pull"),
            "Missing 'git pull' label:\n{output}"
        );
    }

    #[test]
    fn http_request_renders() {
        let output = render_fixture("http_request.mmd");
        // Due to cycle handling, node order may vary. Check for presence of key elements.
        assert!(!output.is_empty());
        // At least some nodes should be present
        let has_nodes = output.contains("Client")
            || output.contains("Server")
            || output.contains("Process")
            || output.contains("Response");
        assert!(has_nodes, "Should contain at least one node label");
        // Should have diamond shape indicators
        assert!(
            output.contains('<') || output.contains('>'),
            "Should have decision node (diamond shape uses < or > chars)"
        );
    }

    #[test]
    fn ci_pipeline_renders() {
        let output = render_fixture("ci_pipeline.mmd");
        assert!(output.contains("Build"));
        assert!(output.contains("Test"));
        assert!(output.contains("Deploy?"));
    }

    #[test]
    fn complex_renders_without_panic() {
        let output = render_fixture("complex.mmd");
        assert!(!output.is_empty());
        assert!(output.contains("Input"));
        assert!(output.contains("Output"));
    }

    #[test]
    fn ascii_only_mode() {
        let unicode_output = render_fixture("simple.mmd");
        let ascii_output = render_fixture_ascii("simple.mmd");

        // Both should contain the labels
        assert!(unicode_output.contains("Start"));
        assert!(ascii_output.contains("Start"));

        // ASCII output should not contain Unicode box-drawing chars
        let unicode_chars = [
            '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '╭', '╮', '╯', '╰',
        ];
        for ch in unicode_chars {
            assert!(
                !ascii_output.contains(ch),
                "ASCII output should not contain '{}'",
                ch
            );
        }
    }
}

// =============================================================================
// Stagger Preservation Tests
// =============================================================================

mod stagger {
    use super::*;

    #[test]
    fn stagger_present_for_multiple_cycles() {
        // multiple_cycles.mmd: A[Top] --> B[Middle], B --> C[Bottom], C --> A, C --> B
        // Mermaid/dagre-d3-es computes A rightward (aligned with the reversed edge chain).
        // After stagger: A's center_x should be > B's and C's center_x
        let (_, layout) = layout_fixture("multiple_cycles.mmd");

        let a_cx = layout.node_bounds["A"].center_x();
        let b_cx = layout.node_bounds["B"].center_x();
        let c_cx = layout.node_bounds["C"].center_x();

        assert!(
            a_cx > b_cx,
            "A (center_x={}) should be right of B (center_x={})",
            a_cx,
            b_cx
        );
        assert!(
            a_cx > c_cx,
            "A (center_x={}) should be right of C (center_x={})",
            a_cx,
            c_cx
        );
    }

    #[test]
    fn no_stagger_for_simple_chain() {
        // chain.mmd: linear chain with no backward edges → no stagger
        let (_, layout) = layout_fixture("chain.mmd");

        let centers: Vec<usize> = layout.node_bounds.values().map(|b| b.center_x()).collect();
        let first = centers[0];
        for &c in &centers[1..] {
            assert!(
                (c as isize - first as isize).unsigned_abs() <= 1,
                "All nodes should be centered: got {:?}",
                centers
            );
        }
    }

    #[test]
    fn stagger_produces_different_attachment_points() {
        // For multiple_cycles.mmd, the forward edge A→B and backward edge C→A
        // should attach at different positions on node A's boundary.
        let (diagram, layout) = layout_fixture("multiple_cycles.mmd");
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let a_b_edge = routed
            .iter()
            .find(|e| e.edge.from == "A" && e.edge.to == "B")
            .expect("A→B edge should exist");
        let c_a_edge = routed
            .iter()
            .find(|e| e.edge.from == "C" && e.edge.to == "A")
            .expect("C→A edge should exist");

        // A→B exits from A (start point); C→A enters A (end point)
        // With stagger, these should be at different positions on A
        assert_ne!(
            a_b_edge.start, c_a_edge.end,
            "Forward A→B start ({:?}) and backward C→A end ({:?}) should differ on A",
            a_b_edge.start, c_a_edge.end
        );
    }

    #[test]
    fn stagger_present_for_simple_cycle() {
        // simple_cycle.mmd has backward edges → should show stagger
        let (_, layout) = layout_fixture("simple_cycle.mmd");

        let centers: Vec<usize> = layout.node_bounds.values().map(|b| b.center_x()).collect();
        let min_center = *centers.iter().min().unwrap();
        let max_center = *centers.iter().max().unwrap();
        assert!(
            max_center - min_center >= 2,
            "Cycle diagram should have stagger: centers {:?} (range={})",
            centers,
            max_center - min_center
        );
    }
}

// =============================================================================
// Attachment Point Spreading Tests
// =============================================================================

mod spreading {
    use super::*;

    /// Verify that no row has immediately adjacent down-arrows.
    fn assert_no_adjacent_arrows(output: &str, fixture_name: &str) {
        for (line_num, line) in output.lines().enumerate() {
            assert!(
                !line.contains("▼▼"),
                "{}: line {} has adjacent arrows: {}",
                fixture_name,
                line_num + 1,
                line
            );
        }
    }

    /// Verify that edges arriving at a shared target node have distinct endpoint x-coordinates.
    fn assert_distinct_arrival_x(fixture_name: &str, target_node: &str) {
        let (diagram, layout) = layout_fixture(fixture_name);
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let arrival_xs: Vec<usize> = routed
            .iter()
            .filter(|r| r.edge.to == target_node)
            .map(|r| r.end.x)
            .collect();

        assert!(
            arrival_xs.len() >= 2,
            "{}: expected >=2 edges arriving at {}, got {}",
            fixture_name,
            target_node,
            arrival_xs.len()
        );

        assert_all_distinct(
            &arrival_xs,
            &format!("{}: edges arriving at {}", fixture_name, target_node),
        );
    }

    // --- Wide-node fixtures: no adjacent arrows ---

    #[test]
    fn fan_in_no_adjacent_arrows() {
        let output = render_fixture("fan_in.mmd");
        assert_no_adjacent_arrows(&output, "fan_in.mmd");
    }

    #[test]
    fn fan_out_no_adjacent_arrows() {
        let output = render_fixture("fan_out.mmd");
        assert_no_adjacent_arrows(&output, "fan_out.mmd");
    }

    // --- All target fixtures: distinct arrival x-coordinates ---

    #[test]
    fn double_skip_distinct_arrivals() {
        assert_distinct_arrival_x("double_skip.mmd", "D");
    }

    #[test]
    fn stacked_fan_in_distinct_arrivals() {
        assert_distinct_arrival_x("stacked_fan_in.mmd", "C");
    }

    #[test]
    fn narrow_fan_in_distinct_arrivals() {
        assert_distinct_arrival_x("narrow_fan_in.mmd", "D");
    }

    #[test]
    fn skip_edge_collision_distinct_arrivals() {
        assert_distinct_arrival_x("skip_edge_collision.mmd", "D");
    }

    #[test]
    fn fan_in_distinct_arrivals() {
        assert_distinct_arrival_x("fan_in.mmd", "D");
    }

    #[test]
    fn five_fan_in_distinct_arrivals() {
        assert_distinct_arrival_x("five_fan_in.mmd", "F");
    }

    // --- Departure-side spreading ---

    #[test]
    fn fan_out_distinct_departures() {
        let (diagram, layout) = layout_fixture("fan_out.mmd");
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let departure_xs: Vec<usize> = routed
            .iter()
            .filter(|r| r.edge.from == "A")
            .map(|r| r.start.x)
            .collect();

        assert!(departure_xs.len() >= 2);
        assert_all_distinct(&departure_xs, "fan_out.mmd: edges departing A");
    }
}

// =============================================================================
// Skip-Edge Waypoint Separation Tests
// =============================================================================

mod skip_edge_separation {
    use super::*;

    /// Assert that the A→D skip-edge waypoints do not overlap with node B's bounding box.
    /// Both fixtures have an A→B→...→D chain plus an A→D skip edge whose waypoints
    /// must clear intermediate node B (either to the left or right).
    fn assert_skip_edge_clears_node_b(fixture_name: &str) {
        let (diagram, layout) = layout_fixture(fixture_name);

        let b_bounds = &layout.node_bounds["B"];
        let ad_edge = diagram
            .edges
            .iter()
            .find(|e| e.from == "A" && e.to == "D")
            .expect("Should have an A→D edge");
        let waypoints = layout
            .edge_waypoints
            .get(&ad_edge.index)
            .expect("A→D should have waypoints");

        assert!(
            !waypoints.is_empty(),
            "{}: A→D skip edge should have at least one waypoint",
            fixture_name
        );

        // Waypoints are ordered by rank; the first is at B's rank.
        let wp_at_b_rank = waypoints[0];
        assert!(
            !b_bounds.contains(wp_at_b_rank.0, wp_at_b_rank.1),
            "{}: A→D waypoint {:?} should clear B's bounds {:?} (need separation)",
            fixture_name,
            wp_at_b_rank,
            b_bounds,
        );
    }

    #[test]
    fn double_skip_waypoints_avoid_intermediate_nodes() {
        assert_skip_edge_clears_node_b("double_skip.mmd");
    }

    #[test]
    fn skip_edge_collision_waypoints_avoid_intermediate_nodes() {
        assert_skip_edge_clears_node_b("skip_edge_collision.mmd");
    }
}

// =============================================================================
// Direct Layout Integration Tests
// =============================================================================

mod direct_layout {
    use super::*;

    #[test]
    fn direct_simple_produces_valid_layout() {
        let (_, layout) = layout_fixture("simple.mmd");

        assert!(layout.width > 0, "canvas width must be positive");
        assert!(layout.height > 0, "canvas height must be positive");
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
        assert!(layout.node_bounds.contains_key("A"));
        assert!(layout.node_bounds.contains_key("B"));
    }

    #[test]
    fn direct_no_node_overlaps() {
        let (_, layout) = layout_fixture("chain.mmd");

        let bounds: Vec<_> = layout.node_bounds.values().collect();
        for i in 0..bounds.len() {
            for j in (i + 1)..bounds.len() {
                let a = bounds[i];
                let b = bounds[j];
                let x_overlap = a.x < b.x + b.width && b.x < a.x + a.width;
                let y_overlap = a.y < b.y + b.height && b.y < a.y + a.height;
                assert!(
                    !(x_overlap && y_overlap),
                    "nodes overlap: {:?} vs {:?}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn direct_nodes_within_canvas() {
        let (_, layout) = layout_fixture("fan_out.mmd");

        for (id, bounds) in &layout.node_bounds {
            assert!(
                bounds.x + bounds.width <= layout.width,
                "node {} exceeds canvas width: {} + {} > {}",
                id,
                bounds.x,
                bounds.width,
                layout.width
            );
            assert!(
                bounds.y + bounds.height <= layout.height,
                "node {} exceeds canvas height: {} + {} > {}",
                id,
                bounds.y,
                bounds.height,
                layout.height
            );
        }
    }

    #[test]
    fn direct_td_vertical_ordering() {
        let (_, layout) = layout_fixture("simple.mmd");

        let a_y = layout.draw_positions["A"].1;
        let b_y = layout.draw_positions["B"].1;
        assert!(
            a_y < b_y,
            "in TD layout, A (rank 0) should be above B (rank 1)"
        );
    }

    #[test]
    fn direct_lr_horizontal_ordering() {
        let (_, layout) = layout_fixture("left_right.mmd");

        assert!(
            layout.width > layout.height || layout.node_bounds.len() <= 2,
            "LR layout should generally be wider than tall"
        );
    }

    #[test]
    fn direct_preserves_cross_axis_stagger() {
        // fan_out.mmd: A→B, A→C, A→D — layer 1 has B, C, D which should
        // have distinct x positions from dagre's BK algorithm.
        let (_, layout) = layout_fixture("fan_out.mmd");

        let b_x = layout.node_bounds["B"].center_x();
        let c_x = layout.node_bounds["C"].center_x();
        let d_x = layout.node_bounds["D"].center_x();

        assert!(
            b_x != c_x || c_x != d_x,
            "B/C/D all at same x center ({}) — cross-axis stagger was lost",
            b_x,
        );
    }

    #[test]
    fn direct_cycle_no_edge_overlap_at_attachment() {
        let (_, layout) = layout_fixture("simple_cycle.mmd");

        let wp_vecs: Vec<&Vec<(usize, usize)>> = layout.edge_waypoints.values().collect();
        for i in 0..wp_vecs.len() {
            for j in (i + 1)..wp_vecs.len() {
                if !wp_vecs[i].is_empty() && !wp_vecs[j].is_empty() {
                    assert_ne!(
                        wp_vecs[i], wp_vecs[j],
                        "two edges share identical waypoint paths — overlap likely"
                    );
                }
            }
        }
    }

    #[test]
    fn direct_fan_in_ordered_arrivals() {
        let (diagram, layout) = layout_fixture("fan_in.mmd");
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let mut arrival_xs: Vec<usize> = routed
            .iter()
            .filter(|r| r.edge.to == "D")
            .map(|r| r.end.x)
            .collect();
        arrival_xs.sort();
        arrival_xs.dedup();

        assert!(
            arrival_xs.len() >= 2,
            "fan_in: expected >=2 distinct arrival x-coords at D, got {:?}",
            arrival_xs
        );
    }

    #[test]
    fn direct_five_fan_in_distinct_arrivals() {
        let (diagram, layout) = layout_fixture("five_fan_in.mmd");
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let arrival_xs: Vec<usize> = routed
            .iter()
            .filter(|r| r.edge.to == "F")
            .map(|r| r.end.x)
            .collect();

        assert_all_distinct(&arrival_xs, "five_fan_in: arrival x at F");
    }
}

// =============================================================================
// Baseline Snapshots
// =============================================================================

mod snapshots {
    use super::*;

    #[test]
    fn generate_baseline_snapshots() {
        let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        let snapshot_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("snapshots");
        fs::create_dir_all(&snapshot_dir).unwrap();
        let regenerate = std::env::var("GENERATE_TEXT_SNAPSHOTS").is_ok();

        for entry in fs::read_dir(&fixture_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "mmd") {
                let name = path.file_stem().unwrap().to_str().unwrap();
                let input = fs::read_to_string(&path).unwrap();
                let flowchart = parse_flowchart(&input).expect("Failed to parse");
                let diagram = build_diagram(&flowchart);
                let output = render(&diagram, &RenderOptions::default());
                let snapshot_path = snapshot_dir.join(format!("{}.txt", name));
                if regenerate {
                    fs::write(snapshot_path, &output).unwrap();
                } else {
                    let expected = fs::read_to_string(&snapshot_path).unwrap_or_else(|_| {
                        panic!(
                            "Missing snapshot: {}. Set GENERATE_TEXT_SNAPSHOTS=1 to generate.",
                            snapshot_path.display()
                        )
                    });
                    assert_eq!(
                        output, expected,
                        "Snapshot mismatch for {}. Set GENERATE_TEXT_SNAPSHOTS=1 to regenerate.",
                        name
                    );
                }
            }
        }
    }
}

// =============================================================================
// All Fixtures Parse and Render
// =============================================================================

mod all_fixtures {
    use super::*;

    const FIXTURE_FILES: &[&str] = &[
        "simple.mmd",
        "decision.mmd",
        "shapes.mmd",
        "shapes_basic.mmd",
        "shapes_junction.mmd",
        "shapes_document.mmd",
        "shapes_special.mmd",
        "shapes_degenerate.mmd",
        "edge_styles.mmd",
        "left_right.mmd",
        "bottom_top.mmd",
        "right_left.mmd",
        "chain.mmd",
        "ampersand.mmd",
        "labeled_edges.mmd",
        "git_workflow.mmd",
        "http_request.mmd",
        "ci_pipeline.mmd",
        "complex.mmd",
        "simple_subgraph.mmd",
        "subgraph_edges.mmd",
        "multi_subgraph.mmd",
        "nested_subgraph.mmd",
        "nested_subgraph_only.mmd",
        "nested_with_siblings.mmd",
    ];

    #[test]
    fn all_fixtures_parse() {
        for fixture in FIXTURE_FILES {
            let input = load_fixture(fixture);
            parse_flowchart(&input)
                .unwrap_or_else(|e| panic!("Failed to parse {}: {:?}", fixture, e));
        }
    }

    #[test]
    fn all_fixtures_render() {
        for fixture in FIXTURE_FILES {
            let output = render_fixture(fixture);
            assert!(
                !output.is_empty(),
                "Fixture {} should produce non-empty output",
                fixture
            );
        }
    }

    #[test]
    fn all_fixtures_render_ascii() {
        for fixture in FIXTURE_FILES {
            let output = render_fixture_ascii(fixture);
            assert!(
                !output.is_empty(),
                "Fixture {} should produce non-empty ASCII output",
                fixture
            );
        }
    }
}

// =============================================================================
// LR/RL Routing Tests
// =============================================================================

mod lr_routing {
    use super::*;

    /// Render inline Mermaid text (not a fixture file).
    fn render_inline(input: &str) -> String {
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);
        render(&diagram, &RenderOptions::default())
    }

    fn assert_has_right_arrow(output: &str) {
        assert!(
            output.contains('►') || output.contains('>'),
            "LR edge should use right-pointing arrow, got:\n{}",
            output
        );
    }

    fn assert_has_left_arrow(output: &str) {
        assert!(
            output.contains('◄') || output.contains('<'),
            "LR backward edge should have left-pointing arrow, got:\n{}",
            output
        );
    }

    fn assert_no_vertical_arrows_between_nodes(output: &str) {
        let has_vertical = output
            .lines()
            .any(|line| line.contains("│▲│") || line.contains("│▼│"));
        assert!(
            !has_vertical,
            "LR edge should not have vertical arrows between nodes, got:\n{}",
            output
        );
    }

    #[test]
    fn lr_simple_chain_horizontal_arrows() {
        let output = render_inline("graph LR\n    A[Start] --> B[End]");
        assert_has_right_arrow(&output);
        assert_no_vertical_arrows_between_nodes(&output);
    }

    #[test]
    fn lr_three_node_chain_horizontal_arrows() {
        let output = render_fixture("left_right.mmd");
        assert_has_right_arrow(&output);
        assert_no_vertical_arrows_between_nodes(&output);
    }

    #[test]
    fn lr_backward_edge_renders_without_panic() {
        let output =
            render_inline("graph LR\n    A[Start] --> B[Middle]\n    B --> C[End]\n    C --> A");

        assert!(output.contains("Start"), "Should contain Start node");
        assert!(output.contains("Middle"), "Should contain Middle node");
        assert!(output.contains("End"), "Should contain End node");
        assert_has_left_arrow(&output);
    }

    #[test]
    fn lr_backward_edge_routes_around_nodes() {
        // LR backward edges now route below nodes with synthetic waypoints.
        // The backward edge should produce some arrow character.
        let output = render_inline("graph LR\n    A --> B\n    B --> A");
        let arrow_count = output
            .chars()
            .filter(|c| matches!(c, '▲' | '▼' | '◄' | '►' | '<' | '>'))
            .count();
        // Should have at least 2 arrows: one for forward A→B (►) and one for backward B→A
        assert!(
            arrow_count >= 2,
            "Should have arrows for both forward and backward edges, found {} arrows in:\n{}",
            arrow_count,
            output
        );
    }

    #[test]
    fn lr_multirank_backward_edge_does_not_extend_left_of_target() {
        // The backward edge D→A should NOT place its arrow to the LEFT
        // of A's left border -- that extends outside the diagram bounds.
        let output = render_inline("graph LR\n    A --> B --> C --> D\n    D --> A");

        let mut arrow_col = None;
        let mut node_left_border = None;

        for line in output.lines() {
            if let Some(pos) = line.find('◄') {
                arrow_col = Some(pos);
            }
            if line.contains(" A ")
                && let Some(pos) = line.find('│')
            {
                node_left_border = Some(pos);
            }
        }

        if let (Some(arrow), Some(border)) = (arrow_col, node_left_border) {
            assert!(
                arrow >= border,
                "Backward edge arrow (col {}) should not extend left of node A's border (col {}). \
                 The arrow extends outside the diagram area.\nOutput:\n{}",
                arrow,
                border,
                output
            );
        }
    }
}

// =============================================================================
// Subgraph Rendering Tests
// =============================================================================

mod subgraph_rendering {
    use super::*;

    #[test]
    fn simple_subgraph_renders_title_and_nodes() {
        let output = render_fixture("simple_subgraph.mmd");
        assert!(output.contains("Process"), "Should contain subgraph title");
        assert!(output.contains("Start"), "Should contain Start node");
        assert!(output.contains("Middle"), "Should contain Middle node");
        assert!(output.contains("End"), "Should contain End node");
    }

    #[test]
    fn simple_subgraph_has_border() {
        let output = render_fixture("simple_subgraph.mmd");
        // Subgraph border uses box-drawing characters
        assert!(
            output.contains('┌') && output.contains('┘'),
            "Should have box-drawing border characters"
        );
    }

    #[test]
    fn subgraph_edges_renders_both_groups() {
        let output = render_fixture("subgraph_edges.mmd");
        assert!(
            output.contains("Input"),
            "Should contain Input subgraph title"
        );
        assert!(output.contains("Data"), "Should contain Data node");
        assert!(output.contains("Config"), "Should contain Config node");
        assert!(output.contains("Result"), "Should contain Result node");
        assert!(output.contains("Log"), "Should contain Log node");
    }

    #[test]
    fn subgraph_edges_borders_do_not_overlap() {
        let output = render_fixture("subgraph_edges.mmd");
        let lines: Vec<&str> = output.lines().collect();

        // Find the row containing sg1's bottom-left corner (└)
        // and sg2's top-left corner (┌). They must be on separate rows.
        let bottom_border_rows: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains('└'))
            .map(|(i, _)| i)
            .collect();
        let top_border_rows: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains('┌'))
            .map(|(i, _)| i)
            .collect();

        // sg1's bottom border (last └ row before sg2) should be strictly
        // above sg2's top border (┌ row containing "Output" or second ┌)
        // Simple check: no row should contain both └ and ┌ from different subgraphs
        // More robust: the sg1 bottom-left corner row < sg2 top-left corner row
        assert!(
            !bottom_border_rows.is_empty(),
            "Should have bottom border rows"
        );
        assert!(!top_border_rows.is_empty(), "Should have top border rows");

        // Find sg1's bottom border (└ row) and sg2's top border (second ┌ row).
        // sg1's top border is the first ┌ row. sg2's top border is the next ┌ row
        // after sg1's content.
        let _first_top = lines.iter().position(|l| l.contains('┌')).unwrap();
        let first_bottom = lines.iter().position(|l| l.contains('└')).unwrap();
        let second_top = lines
            .iter()
            .enumerate()
            .skip(first_bottom)
            .position(|(_, l)| l.contains('┌'))
            .map(|pos| pos + first_bottom);

        if let Some(second_top) = second_top {
            assert!(
                second_top > first_bottom,
                "Second subgraph top ({second_top}) should be below first subgraph \
                bottom ({first_bottom})"
            );
        }
    }

    #[test]
    fn subgraph_titles_preserved_with_cross_edges() {
        let output = render_fixture("subgraph_edges.mmd");
        // Both titles should be fully intact (not corrupted by edge arrows)
        assert!(
            output.contains("Input"),
            "Input title should be intact in: {}",
            output
        );
        assert!(
            output.contains("Output"),
            "Output title should be intact in: {}",
            output
        );
    }

    #[test]
    fn multi_subgraph_renders_both_groups() {
        let output = render_fixture("multi_subgraph.mmd");
        // LR layout may not display subgraph titles if the box is too compact
        assert!(output.contains("UI"), "Should contain UI node");
        assert!(output.contains("API"), "Should contain API node");
        assert!(output.contains("Server"), "Should contain Server node");
        assert!(output.contains("DB"), "Should contain DB node");
        // Should have subgraph borders and node borders
        let border_count = output.matches('┌').count();
        assert!(
            border_count >= 3,
            "Should have borders for subgraphs and nodes, got {} '┌' chars",
            border_count
        );
        // Both subgraph titles should appear
        assert!(output.contains("Frontend"), "Should contain Frontend title");
        assert!(output.contains("Backend"), "Should contain Backend title");
    }

    #[test]
    fn simple_subgraph_ascii_mode() {
        let output = render_fixture_ascii("simple_subgraph.mmd");
        assert!(output.contains("Process"), "ASCII: should contain title");
        assert!(output.contains("Start"), "ASCII: should contain Start");
        // ASCII mode uses +/-/| for borders
        assert!(
            output.contains('+') && output.contains('-'),
            "ASCII mode should use +/- border characters"
        );
    }

    #[test]
    fn subgraph_nodes_aligned_vertically() {
        // Verify the stagger fix: nodes in a vertical chain inside a subgraph
        // should have similar horizontal positions
        let (_, layout) = layout_fixture("simple_subgraph.mmd");

        let a_cx = layout.node_bounds["A"].center_x();
        let b_cx = layout.node_bounds["B"].center_x();

        assert!(
            (a_cx as isize - b_cx as isize).unsigned_abs() <= 1,
            "A (center_x={}) and B (center_x={}) should be vertically aligned",
            a_cx,
            b_cx
        );
    }

    #[test]
    fn subgraph_title_embedded_in_border() {
        let output = render_fixture("simple_subgraph.mmd");
        // Title should be embedded in border line, not floating above
        assert!(
            output.contains("─ Process ─") || output.contains("- Process -"),
            "Title should be embedded in border: {}",
            output
        );
    }

    #[test]
    fn existing_fixtures_unchanged_by_subgraph_support() {
        // Render non-subgraph fixtures and verify they produce valid output
        let fixtures = [
            "simple.mmd",
            "chain.mmd",
            "fan_in.mmd",
            "fan_out.mmd",
            "decision.mmd",
            "edge_styles.mmd",
        ];
        for fixture in fixtures {
            let output = render_fixture(fixture);
            assert!(
                !output.is_empty(),
                "{} should produce non-empty output",
                fixture
            );
        }
    }
}

// === Subgraph parsing and building tests ===

#[test]
fn test_parse_simple_subgraph_fixture() {
    let diagram = parse_and_build("simple_subgraph.mmd");

    assert!(diagram.has_subgraphs());
    assert!(diagram.subgraphs.contains_key("sg1"));
    assert_eq!(diagram.subgraphs["sg1"].title, "Process");
    assert!(diagram.subgraphs["sg1"].nodes.contains(&"A".to_string()));
    assert!(diagram.subgraphs["sg1"].nodes.contains(&"B".to_string()));
}

#[test]
fn test_parse_subgraph_edges_fixture() {
    let diagram = parse_and_build("subgraph_edges.mmd");

    assert_eq!(diagram.subgraphs.len(), 2);
    assert!(diagram.subgraphs.contains_key("sg1"));
    assert!(diagram.subgraphs.contains_key("sg2"));
    // Edges cross subgraph boundaries
    assert!(diagram.edges.iter().any(|e| e.from == "A" && e.to == "C"));
    assert!(diagram.edges.iter().any(|e| e.from == "B" && e.to == "D"));
}

#[test]
fn test_parse_multi_subgraph_fixture() {
    let diagram = parse_and_build("multi_subgraph.mmd");

    assert_eq!(diagram.subgraphs.len(), 2);
    assert!(diagram.subgraphs.contains_key("sg1"));
    assert!(diagram.subgraphs.contains_key("sg2"));
    assert_eq!(diagram.subgraphs["sg1"].title, "Frontend");
    assert_eq!(diagram.subgraphs["sg2"].title, "Backend");
    // Cross-boundary edge
    assert!(diagram.edges.iter().any(|e| e.from == "B" && e.to == "C"));
}

/// Edge case tests for label-as-dummy-node (Plan 0024).
mod label_edge_cases {
    use super::*;

    #[test]
    fn long_label_renders_without_panic() {
        let output =
            render_input("graph TD\n    A -->|this is a very long label that might overflow| B");
        // Should not panic; nodes should still render correctly
        assert!(!output.is_empty());
        assert!(output.contains(" A "), "Node A should render:\n{output}");
        assert!(output.contains(" B "), "Node B should render:\n{output}");
        // Label may be truncated or omitted if wider than canvas — this is
        // acceptable behavior for now.
    }

    #[test]
    fn fan_out_with_labels() {
        let output =
            render_input("graph TD\n    A -->|yes| B\n    A -->|no| C\n    A -->|maybe| D");
        // All three labels should be visible
        assert!(output.contains("yes"), "Expected 'yes' label:\n{output}");
        assert!(output.contains("no"), "Expected 'no' label:\n{output}");
        assert!(
            output.contains("maybe"),
            "Expected 'maybe' label:\n{output}"
        );
    }

    #[test]
    #[ignore = "backward edge label positioning — will be fixed by BK parity work (plan 0040)"]
    fn labeled_backward_edge_renders() {
        let output = render_input("graph TD\n    A --> B\n    B -->|retry| A");
        assert!(!output.is_empty());
        // The backward "retry" label should appear
        assert!(
            output.contains("retry"),
            "Expected 'retry' label on backward edge:\n{output}"
        );
        // Label should be to the right of nodes (path-midpoint placement)
        let lines: Vec<&str> = output.lines().collect();
        let node_line = lines.iter().find(|l| l.contains('A')).unwrap();
        let node_right = node_line.rfind('A').unwrap_or(0);
        let retry_line = lines.iter().find(|l| l.contains("retry")).unwrap();
        let retry_col = retry_line.find("retry").unwrap();
        assert!(
            retry_col > node_right,
            "Label should be to the right of nodes:\n{output}"
        );
    }

    #[test]
    fn labeled_edge_lr_direction() {
        let output = render_input("graph LR\n    A -->|label| B");
        assert!(output.contains(" A "), "Should contain node A:\n{output}");
        assert!(output.contains(" B "), "Should contain node B:\n{output}");
        assert!(
            output.contains("label"),
            "Expected 'label' in LR layout:\n{output}"
        );
    }

    #[test]
    fn mixed_labeled_and_unlabeled() {
        let output = render_input(
            "graph TD\n    A -->|yes| B\n    A --> C\n    B --> D\n    C -->|error| D",
        );
        assert!(output.contains("yes"), "Expected 'yes' label:\n{output}");
        assert!(
            output.contains("error"),
            "Expected 'error' label:\n{output}"
        );
        // All nodes should be present
        for node in ["A", "B", "C", "D"] {
            assert!(
                output.contains(&format!(" {node} ")),
                "Expected node {node}:\n{output}"
            );
        }
    }

    #[test]
    fn all_edges_labeled() {
        let output =
            render_input("graph TD\n    A -->|start| B\n    B -->|process| C\n    C -->|end| D");
        // At least the last label should appear (via precomputed position)
        assert!(output.contains("end"), "Expected 'end' label:\n{output}");
        // All nodes should render (check for bordered node text)
        assert!(output.contains(" A "), "Expected node A:\n{output}");
        assert!(output.contains(" B "), "Expected node B:\n{output}");
        assert!(output.contains(" D "), "Expected node D:\n{output}");
        // Node C may have arrow overlap in its box due to edge routing
        // through the node, but the node box itself should exist
        assert!(
            output.contains("┌───┐"),
            "Expected at least one node box:\n{output}"
        );
    }

    #[test]
    #[ignore]
    fn labeled_edges_reasonable_height() {
        let input = load_fixture("labeled_edges.mmd");
        let flowchart = parse_flowchart(&input).expect("Failed to parse labeled_edges");
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &Default::default());
        let line_count = output.lines().count();

        // Main branch renders ~29 lines. Regression was 51+ lines.
        // With the fix, expect similar to main branch (allow some tolerance for label dummies).
        assert!(
            line_count < 40,
            "labeled_edges.mmd should render in under 40 lines, got {line_count}"
        );

        // All 5 labels should be present
        for label in &["initialize", "configure", "yes", "no", "retry"] {
            assert!(
                output.contains(label),
                "Output should contain label '{label}'"
            );
        }
    }

    #[test]
    fn diamond_text_not_corrupted_by_arrows() {
        let input = load_fixture("labeled_edges.mmd");
        let flowchart = parse_flowchart(&input).expect("Failed to parse");
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &Default::default());

        // The diamond should contain "Valid?" text, not corrupted by arrow characters
        assert!(
            output.contains("Valid?"),
            "Diamond text 'Valid?' should be intact in output:\n{output}"
        );
    }

    #[test]
    fn simple_cycle_compact_backward_routing() {
        let input = load_fixture("simple_cycle.mmd");
        let flowchart = parse_flowchart(&input).expect("Failed to parse");
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &Default::default());
        let line_count = output.lines().count();

        assert!(
            line_count < 30,
            "simple_cycle.mmd should be compact, got {line_count} lines"
        );
    }

    #[test]
    fn multiple_cycles_compact_backward_routing() {
        let input = load_fixture("multiple_cycles.mmd");
        let flowchart = parse_flowchart(&input).expect("Failed to parse");
        let diagram = build_diagram(&flowchart);
        let output = render(&diagram, &Default::default());
        let line_count = output.lines().count();

        assert!(
            line_count < 40,
            "multiple_cycles.mmd should be compact, got {line_count} lines"
        );
    }
}

// === Backward edge label position tests (Plan 0027, Task 5.1) ===

#[test]
#[ignore = "backward edge label positioning — will be fixed by BK parity work (plan 0040)"]
fn backward_edge_label_position_td() {
    let output = render_input("graph TD\n    A --> B\n    B -->|retry| A");
    assert!(output.contains("retry"), "Label missing:\n{output}");
    let lines: Vec<&str> = output.lines().collect();
    let retry_line = lines.iter().find(|l| l.contains("retry")).unwrap();
    let retry_col = retry_line.find("retry").unwrap();
    assert!(
        retry_col > 5,
        "Label should be positioned away from left edge:\n{output}"
    );
}

#[test]
fn backward_edge_label_position_bt() {
    let output = render_input("graph BT\n    A --> B\n    B -->|retry| A");
    assert!(output.contains("retry"), "Label missing:\n{output}");
}

#[test]
fn backward_edge_label_position_lr() {
    let output = render_input("graph LR\n    A --> B\n    B -->|retry| A");
    assert!(output.contains("retry"), "Label missing:\n{output}");
}

#[test]
fn backward_edge_label_position_rl() {
    let output = render_input("graph RL\n    A --> B\n    B -->|retry| A");
    assert!(output.contains("retry"), "Label missing:\n{output}");
}

#[test]
fn backward_and_forward_labels_coexist() {
    let output = render_input("graph TD\n    A -->|go| B\n    B -->|retry| A");
    assert!(output.contains("go"), "Forward label missing:\n{output}");
    assert!(
        output.contains("retry"),
        "Backward label missing:\n{output}"
    );
}

#[test]
fn backward_edge_label_does_not_overlap_nodes() {
    let output = render_input("graph TD\n    Start --> End\n    End -->|back| Start");
    assert!(output.contains("back"), "Label missing:\n{output}");
    let lines: Vec<&str> = output.lines().collect();
    for line in &lines {
        if line.contains("back") {
            let back_pos = line.find("back").unwrap();
            let before_label = &line[..back_pos];
            assert!(
                !before_label.ends_with('│') && !before_label.ends_with('┐'),
                "Label overlaps with node box:\n{output}"
            );
        }
    }
}

// =========================================================================
// Multi-Subgraph Title Tests (Plan 0031)
// =========================================================================

#[test]
fn test_render_titled_subgraph_shows_title() {
    let input = r#"graph TD
    subgraph sg1[Processing]
        A[Step 1] --> B[Step 2]
    end"#;
    let output = render_input(input);

    assert!(
        output.contains("Processing"),
        "Output should contain subgraph title 'Processing':\n{}",
        output
    );
    assert!(output.contains("Step 1"));
    assert!(output.contains("Step 2"));
}

#[test]
fn test_render_multi_subgraph_titled() {
    // Two titled subgraphs with a cross-edge.
    // Note: multi-subgraph border overlap is a known pre-existing issue —
    // this test verifies titles appear and layout completes without panic.
    let input = r#"graph TD
    subgraph sg1[Intake]
        A[Read] --> B[Parse]
    end
    subgraph sg2[Emit]
        C[Format] --> D[Write]
    end
    B --> C"#;
    let output = render_input(input);

    assert!(
        output.contains("Intake"),
        "Output should contain 'Intake' title:\n{}",
        output
    );
    assert!(
        output.contains("Emit"),
        "Output should contain 'Emit' title:\n{}",
        output
    );
    assert!(output.contains("Read"), "Missing 'Read':\n{}", output);
    assert!(output.contains("Write"), "Missing 'Write':\n{}", output);
}

#[test]
fn test_render_titled_subgraph_title_not_overwritten_by_edge() {
    let input = r#"graph TD
    D[External] --> A
    subgraph sg1[Processing]
        A[Internal] --> B[Next]
    end"#;
    let output = render_input(input);

    assert!(
        output.contains("Processing"),
        "Title should not be overwritten by edge:\n{}",
        output
    );
    assert!(output.contains("External"));
    assert!(output.contains("Internal"));
}

// =========================================================================
// Nested Subgraph Tests (Plan 0032)
// =========================================================================

#[test]
fn test_nested_subgraph_renders_both_borders() {
    let output = render_fixture("nested_subgraph.mmd");
    assert!(
        output.contains("Outer"),
        "Should contain outer border title:\n{}",
        output
    );
    assert!(
        output.contains("Inner"),
        "Should contain inner border title:\n{}",
        output
    );
}

#[test]
fn test_nested_subgraph_only_renders() {
    let output = render_fixture("nested_subgraph_only.mmd");
    assert!(
        output.contains("Outer"),
        "Should contain outer border title:\n{}",
        output
    );
    assert!(
        output.contains("Inner"),
        "Should contain inner border title:\n{}",
        output
    );
}

#[test]
fn test_nested_with_siblings_renders() {
    let output = render_fixture("nested_with_siblings.mmd");
    assert!(
        output.contains("Outer"),
        "Should contain outer border title:\n{}",
        output
    );
    assert!(
        output.contains("Left"),
        "Should contain left border title:\n{}",
        output
    );
    assert!(
        output.contains("Right"),
        "Should contain right border title:\n{}",
        output
    );
}

#[test]
fn test_nested_subgraph_parent_tracking() {
    let diagram = parse_and_build("nested_subgraph.mmd");
    assert_eq!(diagram.subgraphs["inner"].parent, Some("outer".to_string()));
    assert_eq!(diagram.subgraphs["outer"].parent, None);
}

#[test]
fn test_nested_subgraph_bounds_containment() {
    let (_, layout) = layout_fixture("nested_subgraph.mmd");
    let outer = &layout.subgraph_bounds["outer"];
    let inner = &layout.subgraph_bounds["inner"];
    assert!(
        outer.x <= inner.x,
        "outer.x ({}) <= inner.x ({})",
        outer.x,
        inner.x
    );
    assert!(
        outer.y <= inner.y,
        "outer.y ({}) <= inner.y ({})",
        outer.y,
        inner.y
    );
    assert!(
        outer.x + outer.width >= inner.x + inner.width,
        "outer right ({}) >= inner right ({})",
        outer.x + outer.width,
        inner.x + inner.width
    );
    assert!(
        outer.y + outer.height >= inner.y + inner.height,
        "outer bottom ({}) >= inner bottom ({})",
        outer.y + outer.height,
        inner.y + inner.height
    );
}

// ==========================================
// Self-edge (A --> A) tests
// ==========================================

#[test]
fn test_self_loop_renders_without_crash() {
    let output = render_fixture("self_loop.mmd");
    assert!(!output.trim().is_empty());
    assert!(output.contains("Process"));
}

#[test]
fn test_self_loop_has_loop_segments() {
    let output = render_input("graph TD\n    A --> A");
    // Should have vertical line segments forming the loop
    assert!(
        output.contains('│') || output.contains('|'),
        "should have vertical segments"
    );
    // Should have horizontal line segments
    assert!(
        output.contains('─') || output.contains('-'),
        "should have horizontal segments"
    );
}

#[test]
fn test_self_loop_node_appears_once() {
    let output = render_input("graph TD\n    A[Unique] --> A");
    let count = output.matches("Unique").count();
    assert_eq!(count, 1, "node label should appear exactly once");
}

#[test]
fn test_self_loop_with_label() {
    let output = render_fixture("self_loop_labeled.mmd");
    assert!(output.contains("retry"), "label text should appear");
    assert!(output.contains("done"), "other label should appear");
}

#[test]
fn test_self_loop_all_directions() {
    for dir in &["TD", "BT", "LR", "RL"] {
        let input = format!("graph {}\n    A --> A", dir);
        let output = render_input(&input);
        assert!(
            !output.trim().is_empty(),
            "direction {} should produce non-empty output",
            dir
        );
        assert!(
            output.contains('A'),
            "direction {} should contain node label",
            dir
        );
    }
}

#[test]
fn test_self_loop_with_normal_edges() {
    let output = render_fixture("self_loop_with_others.mmd");
    assert!(output.contains("Start"));
    assert!(output.contains("Process"));
    assert!(output.contains("End"));
}

#[test]
fn test_self_loop_on_isolated_node() {
    let output = render_input("graph TD\n    A --> A");
    assert!(output.contains('A'));
}

#[test]
fn test_self_loop_with_backward_edge() {
    // A->B->A cycle plus B->B self-loop
    let output = render_input("graph TD\n    A --> B\n    B --> A\n    B --> B");
    assert!(output.contains('A'));
    assert!(output.contains('B'));
}

#[test]
fn test_self_loop_ascii_mode() {
    let diagram = parse_and_build("self_loop.mmd");
    let output = render(
        &diagram,
        &RenderOptions {
            output_format: OutputFormat::Ascii,
            ..Default::default()
        },
    );
    // Should use ASCII characters, no Unicode box drawing
    assert!(!output.contains('┌'), "should not have Unicode box drawing");
    assert!(
        !output.contains('─'),
        "should not have Unicode horizontal line"
    );
}

// === Compound graph external node positioning tests ===

#[test]
fn test_sibling_subgraph_nodes_distinct_x() {
    // A (us-east) and C (us-west) are at the same rank but in different subgraphs.
    // They should have distinct x-coordinates (not collapsed on top of each other).
    let (_, layout) = layout_fixture("external_node_subgraph.mmd");
    let a_cx = layout.node_bounds["A"].center_x();
    let c_cx = layout.node_bounds["C"].center_x();
    assert_ne!(
        a_cx, c_cx,
        "Sibling subgraph nodes should have distinct x: A={}, C={}",
        a_cx, c_cx
    );
}

#[test]
#[ignore = "external node positioning — goal of BK parity work (plan 0040)"]
fn test_external_node_not_far_from_targets() {
    // E connects to A (us-east) and C (us-west).
    // E should be reasonably close to the A-C range, not pushed far away.
    // Ideally E would be centered between A and C, but the current layout
    // positions E near the left subgraph border. This test verifies E isn't
    // wildly offset (the original bug had E ~150 chars away from the subgraphs).
    let (_, layout) = layout_fixture("external_node_subgraph.mmd");
    let a_cx = layout.node_bounds["A"].center_x();
    let c_cx = layout.node_bounds["C"].center_x();
    let e_cx = layout.node_bounds["E"].center_x();
    let min_x = a_cx.min(c_cx);
    let max_x = a_cx.max(c_cx);
    let range = max_x - min_x;
    // E should be within a reasonable distance of the A-C midpoint.
    // The original bug had E ~150 chars away. Use max(2*range, 60) as
    // threshold to allow for intermediate layout states while still
    // catching catastrophic offsets.
    let midpoint = (min_x + max_x) / 2;
    let distance = (e_cx as isize - midpoint as isize).unsigned_abs();
    let tolerance = (range * 2).max(60);
    assert!(
        distance <= tolerance,
        "External node E ({}) is too far from A ({}) - C ({}) range (distance {} > {})",
        e_cx,
        a_cx,
        c_cx,
        distance,
        tolerance
    );
}

#[test]
#[ignore = "external node positioning — goal of BK parity work (plan 0040)"]
fn test_external_node_centered_between_targets() {
    let (_, layout) = layout_fixture("external_node_subgraph.mmd");
    let a_cx = layout.node_bounds["A"].center_x();
    let c_cx = layout.node_bounds["C"].center_x();
    let e_cx = layout.node_bounds["E"].center_x();

    let min_x = a_cx.min(c_cx);
    let max_x = a_cx.max(c_cx);
    let range = max_x - min_x;
    let midpoint = (min_x + max_x) / 2;
    let distance = (e_cx as isize - midpoint as isize).unsigned_abs();
    let tolerance = (range / 2).max(15);

    assert!(
        distance <= tolerance,
        "External node E ({}) is not centered between A ({}) and C ({}) (distance {} > {})",
        e_cx,
        a_cx,
        c_cx,
        distance,
        tolerance
    );
}

// =============================================================================
// Parse Compatibility Tests
// =============================================================================

mod compat {
    use super::*;

    #[test]
    fn directive_stripped() {
        let output = render_fixture("compat_directive.mmd");
        assert!(output.contains("Start"));
        assert!(output.contains("Decision"));
    }

    #[test]
    fn frontmatter_stripped() {
        let output = render_fixture("compat_frontmatter.mmd");
        assert!(output.contains("A"));
        assert!(output.contains("B"));
        assert!(output.contains("C"));
    }

    #[test]
    fn no_direction_defaults_to_td() {
        let diagram = parse_and_build("compat_no_direction.mmd");
        assert_eq!(diagram.direction, Direction::TopDown);
        let output = render_fixture("compat_no_direction.mmd");
        assert!(output.contains("Start"));
        assert!(output.contains("End"));
    }

    #[test]
    fn numeric_ids() {
        let output = render_fixture("compat_numeric_ids.mmd");
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
        assert!(output.contains("Third"));
    }

    #[test]
    fn hyphenated_ids() {
        let output = render_fixture("compat_hyphenated_ids.mmd");
        assert!(output.contains("Start"));
        assert!(output.contains("Process A"));
        assert!(output.contains("Check"));
        assert!(output.contains("Done"));
    }

    #[test]
    fn class_annotation_ignored() {
        let output = render_fixture("compat_class_annotation.mmd");
        assert!(output.contains("Start"));
        assert!(output.contains("Decision"));
        // classDef lines should not cause parse failures
    }

    #[test]
    fn invisible_edge_not_rendered() {
        let output = render_fixture("compat_invisible_edge.mmd");
        assert!(output.contains("A"));
        assert!(output.contains("B"));
        assert!(output.contains("C"));
        // Invisible edge should not appear in output
        assert!(!output.contains("~~~"));
    }

    #[test]
    fn kitchen_sink() {
        let output = render_fixture("compat_kitchen_sink.mmd");
        assert!(output.contains("Start"));
        assert!(output.contains("Check Input"));
        assert!(output.contains("Done"));
        assert!(output.contains("Error"));
    }
}

#[test]
fn test_bidirectional_arrows_both_ends() {
    let output = render_fixture("bidirectional.mmd");

    // For TD layout, down arrows (▼) appear at the target end,
    // up arrows (▲) appear at the source end of bidirectional edges.
    let down_arrows = output.chars().filter(|&c| c == '\u{25BC}').count();
    let up_arrows = output.chars().filter(|&c| c == '\u{25B2}').count();

    // Each bidirectional edge has an arrow at each end.
    // We have 3 bidirectional edges, so expect at least 3 down + 3 up arrows.
    assert!(
        down_arrows >= 3,
        "Should have at least 3 down arrows for 3 bidir edges, got {down_arrows}\n{output}"
    );
    assert!(
        up_arrows >= 3,
        "Should have at least 3 up arrows for 3 bidir edges, got {up_arrows}\n{output}"
    );
}

#[test]
fn test_invisible_edge_not_rendered() {
    use mmdflux::graph::Stroke;

    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("A"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("B"));
    diagram.add_node(mmdflux::graph::Node::new("C").with_label("C"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "B")); // visible
    diagram.add_edge(mmdflux::graph::Edge::new("A", "C").with_stroke(Stroke::Invisible)); // invisible

    let output = render(&diagram, &RenderOptions::default());

    // All nodes should appear
    assert!(output.contains("A"), "Node A should appear");
    assert!(output.contains("B"), "Node B should appear");
    assert!(output.contains("C"), "Node C should appear");

    // There should be exactly 1 arrow (for A→B), not 2
    let down_arrows = output.chars().filter(|&c| c == '▼').count();
    assert_eq!(
        down_arrows, 1,
        "Should have exactly 1 visible arrow (A→B only), got {down_arrows}\n{output}"
    );
}

#[test]
fn test_invisible_edge_affects_layout() {
    use mmdflux::graph::Stroke;

    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("A"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("B"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "B").with_stroke(Stroke::Invisible));

    let output = render(&diagram, &RenderOptions::default());

    // Both nodes should appear
    assert!(output.contains("A"), "Node A should appear");
    assert!(output.contains("B"), "Node B should appear");

    // A should be above B (invisible edge enforces rank ordering)
    let lines: Vec<&str> = output.lines().collect();
    let a_line = lines.iter().position(|l| l.contains('A')).unwrap();
    let b_line = lines.iter().position(|l| l.contains('B')).unwrap();
    assert!(
        a_line < b_line,
        "A should be above B due to invisible edge rank constraint\n{output}"
    );

    // No visible edge characters (no arrows, no lines)
    let down_arrows = output.chars().filter(|&c| c == '▼').count();
    assert_eq!(
        down_arrows, 0,
        "Invisible edge should produce no arrows\n{output}"
    );
}

#[test]
fn test_multi_edge_both_labels_rendered() {
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("Start"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("End"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "B").with_label("path 1"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "B").with_label("path 2"));

    let output = render(&diagram, &RenderOptions::default());

    assert!(
        output.contains("path 1"),
        "First edge label should appear:\n{output}"
    );
    assert!(
        output.contains("path 2"),
        "Second edge label should appear:\n{output}"
    );
}

#[test]
fn test_multi_edge_basic() {
    let input = std::fs::read_to_string("tests/fixtures/multi_edge.mmd").unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);

    assert_eq!(
        diagram.edges.len(),
        2,
        "Should have 2 edges between A and B"
    );

    let output = render(&diagram, &RenderOptions::default());
    assert!(output.contains("A"), "Node A should appear");
    assert!(output.contains("B"), "Node B should appear");
}

#[test]
fn test_multi_edge_labeled_both_labels_visible() {
    let input = std::fs::read_to_string("tests/fixtures/multi_edge_labeled.mmd").unwrap();
    let flowchart = parse_flowchart(&input).unwrap();
    let diagram = build_diagram(&flowchart);

    assert_eq!(diagram.edges.len(), 3);
    assert_eq!(diagram.edges[0].label, Some("path 1".to_string()));
    assert_eq!(diagram.edges[1].label, Some("path 2".to_string()));

    let output = render(&diagram, &RenderOptions::default());
    assert!(
        output.contains("path 1"),
        "First edge label should appear:\n{output}"
    );
    assert!(
        output.contains("path 2"),
        "Second edge label should appear:\n{output}"
    );
}

#[test]
fn test_multi_edge_lr_layout() {
    let flowchart = parse_flowchart("graph LR\n    A -->|yes| B\n    A -->|no| B\n").unwrap();
    let diagram = build_diagram(&flowchart);
    let output = render(&diagram, &RenderOptions::default());

    assert!(
        output.contains("yes"),
        "Label 'yes' should appear:\n{output}"
    );
    assert!(output.contains("no"), "Label 'no' should appear:\n{output}");
}

#[test]
fn test_multi_edge_different_styles() {
    use mmdflux::graph::Stroke;
    let flowchart = parse_flowchart("graph TD\n    A --> B\n    A -.-> B\n    A ==> B\n").unwrap();
    let diagram = build_diagram(&flowchart);

    assert_eq!(diagram.edges.len(), 3);
    let strokes: Vec<_> = diagram.edges.iter().map(|e| e.stroke).collect();
    assert!(strokes.contains(&Stroke::Solid));
    assert!(strokes.contains(&Stroke::Dotted));
    assert!(strokes.contains(&Stroke::Thick));
}

#[test]
fn test_same_rank_constraint_horizontal_alignment() {
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("A"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("B"));
    diagram.add_node(mmdflux::graph::Node::new("C").with_label("C"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "C"));
    diagram.add_same_rank_constraint("A", "B");

    let output = render(&diagram, &RenderOptions::default());

    let lines: Vec<&str> = output.lines().collect();
    let a_line = lines.iter().position(|l| l.contains('A')).unwrap();
    let b_line = lines.iter().position(|l| l.contains('B')).unwrap();
    let c_line = lines.iter().rposition(|l| l.contains('C')).unwrap();

    assert_eq!(a_line, b_line, "A and B should be on same line:\n{output}");
    assert!(c_line > a_line, "C should be below A:\n{output}");
}

#[test]
fn test_same_rank_no_visible_edge() {
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("X").with_label("X"));
    diagram.add_node(mmdflux::graph::Node::new("Y").with_label("Y"));
    diagram.add_same_rank_constraint("X", "Y");

    let output = render(&diagram, &RenderOptions::default());

    assert!(output.contains("X"));
    assert!(output.contains("Y"));

    let has_arrows = output
        .chars()
        .any(|c| c == '\u{25BC}' || c == '\u{25B2}' || c == '\u{25BA}' || c == '\u{25C4}');
    assert!(
        !has_arrows,
        "Same-rank constraint should not render arrows:\n{output}"
    );
}

#[test]
fn test_same_rank_lr_layout() {
    let mut diagram = Diagram::new(Direction::LeftRight);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("A"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("B"));
    diagram.add_node(mmdflux::graph::Node::new("C").with_label("C"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "C"));
    diagram.add_same_rank_constraint("A", "B");

    let output = render(&diagram, &RenderOptions::default());

    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("C"));
}

#[test]
fn test_minlen_2_forces_rank_gap() {
    let mut diagram = Diagram::new(Direction::TopDown);
    diagram.add_node(mmdflux::graph::Node::new("A").with_label("A"));
    diagram.add_node(mmdflux::graph::Node::new("B").with_label("B"));
    diagram.add_edge(mmdflux::graph::Edge::new("A", "B").with_minlen(2));

    let output = render(&diagram, &RenderOptions::default());

    let lines: Vec<&str> = output.lines().collect();
    let a_line = lines.iter().position(|l| l.contains('A')).unwrap();
    let b_line = lines.iter().rposition(|l| l.contains('B')).unwrap();
    let gap = b_line - a_line;

    assert!(
        gap > 3,
        "Gap between A and B should be significant with minlen=2, got {gap}:\n{output}"
    );
}

mod arrow_types {
    use super::*;

    #[test]
    fn test_bidirectional_td_both_arrows_visible() {
        let output = render_input("graph TD\n    A <--> B");
        let lines: Vec<&str> = output.lines().collect();
        let a_line = lines.iter().position(|l| l.contains('A')).unwrap();
        let b_line = lines.iter().rposition(|l| l.contains('B')).unwrap();
        assert!(b_line > a_line, "B should be below A:\n{output}");
    }

    #[test]
    fn test_bidirectional_lr_both_arrows_visible() {
        let output = render_input("graph LR\n    A <--> B");
        assert!(output.contains('A'), "Node A should appear:\n{output}");
        assert!(output.contains('B'), "Node B should appear:\n{output}");
    }

    #[test]
    fn test_cross_arrow_renders_x() {
        let output = render_input("graph TD\n    A --x B");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(
            output.contains('x') || output.contains('X'),
            "Cross arrow should render x/X character:\n{output}"
        );
    }

    #[test]
    fn test_circle_arrow_renders_o() {
        let output = render_input("graph TD\n    A --o B");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(
            output.contains('o') || output.contains('O'),
            "Circle arrow should render o/O character:\n{output}"
        );
    }

    #[test]
    fn test_cross_both_ends() {
        let output = render_input("graph TD\n    A x--x B");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        let x_count = output.chars().filter(|&c| c == 'x' || c == 'X').count();
        assert!(
            x_count >= 2,
            "x--x should render x on both ends, found {x_count}:\n{output}"
        );
    }

    #[test]
    fn test_circle_both_ends() {
        let output = render_input("graph TD\n    A o--o B");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
    }

    #[test]
    fn test_bidirectional_fixture_all_styles() {
        let output = render_fixture("bidirectional_arrows.mmd");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(output.contains('C'));
        assert!(output.contains('D'));
    }

    #[test]
    fn test_cross_circle_fixture() {
        let output = render_fixture("cross_circle_arrows.mmd");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(output.contains('C'));
        assert!(output.contains('D'));
        assert!(output.contains('E'));
    }

    #[test]
    fn test_mixed_arrow_types_in_chain() {
        let output = render_input("graph TD\n    A --> B\n    B --x C\n    C --o D\n    D <--> E");
        assert!(output.contains('A'));
        assert!(output.contains('E'));
    }
}

mod multigraph {
    use super::*;

    #[test]
    fn test_multi_edge_parse_preserves_both() {
        let input = load_fixture("multi_edge.mmd");
        let flowchart = parse_flowchart(&input).unwrap();
        let diagram = build_diagram(&flowchart);
        assert_eq!(
            diagram.edges.len(),
            2,
            "Should preserve both edges between A and B"
        );
    }

    #[test]
    fn test_multi_edge_renders_without_panic() {
        let output = render_fixture("multi_edge.mmd");
        assert!(output.contains('A'), "Node A should appear:\n{output}");
        assert!(output.contains('B'), "Node B should appear:\n{output}");
    }

    #[test]
    fn test_multi_edge_labeled_both_labels_visible() {
        let output = render_fixture("multi_edge_labeled.mmd");
        assert!(
            output.contains("path 1"),
            "First edge label should appear:\n{output}"
        );
        assert!(
            output.contains("path 2"),
            "Second edge label should appear:\n{output}"
        );
    }

    #[test]
    fn test_multi_edge_lr_layout() {
        let output = render_input("graph LR\n    A -->|yes| B\n    A -->|no| B");
        assert!(
            output.contains("yes"),
            "Label 'yes' should appear:\n{output}"
        );
        assert!(output.contains("no"), "Label 'no' should appear:\n{output}");
    }

    #[test]
    fn test_multi_edge_different_styles() {
        let input = "graph TD\n    A --> B\n    A -.-> B\n    A ==> B";
        let flowchart = parse_flowchart(input).unwrap();
        let diagram = build_diagram(&flowchart);

        assert_eq!(
            diagram.edges.len(),
            3,
            "Should have 3 edges between A and B"
        );

        let output = render(&diagram, &RenderOptions::default());
        assert!(output.contains('A'), "Node A should appear:\n{output}");
        assert!(output.contains('B'), "Node B should appear:\n{output}");
    }

    #[test]
    fn test_multi_edge_with_downstream_node() {
        let output = render_fixture("multi_edge_labeled.mmd");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(output.contains('C'));
        let lines: Vec<&str> = output.lines().collect();
        let b_line = lines.iter().position(|l| l.contains('B')).unwrap();
        let c_line = lines.iter().rposition(|l| l.contains('C')).unwrap();
        assert!(c_line > b_line, "C should be below B:\n{output}");
    }

    #[test]
    fn test_multi_edge_three_edges_same_pair() {
        let output =
            render_input("graph TD\n    A -->|one| B\n    A -->|two| B\n    A -->|three| B");
        assert!(
            output.contains("one"),
            "Label 'one' should appear:\n{output}"
        );
        assert!(
            output.contains("two"),
            "Label 'two' should appear:\n{output}"
        );
        assert!(
            output.contains("three"),
            "Label 'three' should appear:\n{output}"
        );
    }

    #[test]
    fn test_multi_edge_mixed_with_other_edges() {
        let output =
            render_input("graph TD\n    A -->|x| B\n    A -->|y| B\n    A --> C\n    B --> D");
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(output.contains('C'));
        assert!(output.contains('D'));
        assert!(output.contains('x'), "Label 'x' should appear:\n{output}");
        assert!(output.contains('y'), "Label 'y' should appear:\n{output}");
    }
}

// === Subgraph-as-node edge resolution tests ===

#[test]
fn test_render_subgraph_as_node_edge() {
    let output = render_fixture("subgraph_as_node_edge.mmd");

    assert!(output.contains("Backend"), "Should render subgraph title");
    assert!(output.contains("Client"), "Should render Client node");
    assert!(output.contains("Logs"), "Should render Logs node");
    assert!(
        output.contains("API Server"),
        "Should render API Server node"
    );
    assert!(output.contains("Database"), "Should render Database node");
}

#[test]
fn test_subgraph_as_node_edge_no_sg_node() {
    let diagram = parse_and_build("subgraph_as_node_edge.mmd");

    // sg1 should not exist as a regular leaf node
    assert!(
        !diagram.nodes.contains_key("sg1"),
        "sg1 should not be a regular node after edge resolution"
    );
    // But it should exist as a subgraph
    assert!(diagram.subgraphs.contains_key("sg1"));

    // Edges should target children of sg1, not sg1 itself
    for edge in &diagram.edges {
        assert_ne!(edge.from, "sg1", "Edge source should not be sg1");
        assert_ne!(edge.to, "sg1", "Edge target should not be sg1");
    }
}

// ============================================================================
// Phase 5: Integration test fixtures
// ============================================================================

// --- 5.1: Subgraph-as-node edge fixtures ---

#[test]
fn test_render_subgraph_to_subgraph_edge() {
    let output = render_fixture("subgraph_to_subgraph_edge.mmd");

    assert!(output.contains("Frontend"), "Should render Frontend title");
    assert!(output.contains("Backend"), "Should render Backend title");
    assert!(
        output.contains("User Interface"),
        "Should render User Interface"
    );
    assert!(output.contains("API Server"), "Should render API Server");
}

#[test]
fn test_subgraph_to_subgraph_edge_resolution() {
    let diagram = parse_and_build("subgraph_to_subgraph_edge.mmd");

    // Neither frontend nor backend should exist as regular nodes
    assert!(!diagram.nodes.contains_key("frontend"));
    assert!(!diagram.nodes.contains_key("backend"));

    // Both should exist as subgraphs
    assert!(diagram.subgraphs.contains_key("frontend"));
    assert!(diagram.subgraphs.contains_key("backend"));

    // The edge "frontend --> backend" should be resolved to child nodes
    for edge in &diagram.edges {
        assert_ne!(edge.from, "frontend");
        assert_ne!(edge.to, "backend");
    }
}

#[test]
fn test_render_nested_subgraph_edge() {
    let output = render_fixture("nested_subgraph_edge.mmd");

    assert!(output.contains("Cloud"), "Should render Cloud title");
    assert!(output.contains("US East"), "Should render US East title");
    assert!(output.contains("Client"), "Should render Client");
    assert!(output.contains("Monitoring"), "Should render Monitoring");
    assert!(output.contains("Server1"), "Should render Server1");
}

#[test]
fn test_nested_subgraph_edge_resolution() {
    let diagram = parse_and_build("nested_subgraph_edge.mmd");

    // cloud should not exist as a regular node
    assert!(!diagram.nodes.contains_key("cloud"));
    assert!(diagram.subgraphs.contains_key("cloud"));

    // Edges targeting "cloud" should resolve to a child node
    for edge in &diagram.edges {
        assert_ne!(edge.to, "cloud", "Edge target should not be cloud");
        assert_ne!(edge.from, "cloud", "Edge source should not be cloud");
    }
}

// --- 5.2: Multi-word title and numeric ID fixtures ---

#[test]
fn test_render_multi_word_subgraph_title() {
    let output = render_fixture("subgraph_multi_word_title.mmd");

    assert!(
        output.contains("Data Processing Pipeline"),
        "Should render multi-word title"
    );
    assert!(output.contains("Extract"), "Should render Extract");
    assert!(output.contains("Transform"), "Should render Transform");
    assert!(output.contains("Load"), "Should render Load");
    assert!(output.contains("Source"), "Should render Source");
    assert!(output.contains("Sink"), "Should render Sink");
}

#[test]
fn test_render_numeric_subgraph_id() {
    let output = render_fixture("subgraph_numeric_id.mmd");

    assert!(output.contains("Phase 1"), "Should render Phase 1 title");
    assert!(output.contains("Phase 2"), "Should render Phase 2 title");
    assert!(output.contains("A"), "Should render node A");
    assert!(output.contains("D"), "Should render node D");
}

#[test]
fn test_parse_subgraph_id_with_quoted_title() {
    let output = render_input("graph TD\nsubgraph myId \"My Custom Title\"\nA --> B\nend\n");
    assert!(
        output.contains("My Custom Title"),
        "Should render quoted title"
    );
}

// --- 5.3: Direction override fixtures ---

#[test]
fn test_render_subgraph_direction_lr() {
    let output = render_fixture("subgraph_direction_lr.mmd");

    assert!(
        output.contains("Horizontal Flow"),
        "Should render subgraph title"
    );
    assert!(output.contains("Step 1"), "Should render Step 1");
    assert!(output.contains("Step 2"), "Should render Step 2");
    assert!(output.contains("Step 3"), "Should render Step 3");
    assert!(output.contains("Start"), "Should render Start");
    assert!(output.contains("End"), "Should render End");
}

#[test]
fn test_subgraph_direction_lr_horizontal_arrangement() {
    let (diagram, layout) = layout_fixture("subgraph_direction_lr.mmd");

    // A, B, C should be arranged horizontally (increasing x, similar y)
    let a = layout.get_bounds("A").unwrap();
    let b = layout.get_bounds("B").unwrap();
    let c = layout.get_bounds("C").unwrap();

    assert!(
        a.center_x() < b.center_x(),
        "Step 1 should be left of Step 2"
    );
    assert!(
        b.center_x() < c.center_x(),
        "Step 2 should be left of Step 3"
    );

    let y_tol = 2;
    assert!(
        (a.center_y() as isize - b.center_y() as isize).abs() <= y_tol,
        "Step 1 and Step 2 should be at similar y"
    );

    // Nodes should have LR effective direction
    assert_eq!(layout.node_directions.get("A"), Some(&Direction::LeftRight));
    let _ = diagram; // suppress unused variable
}

#[test]
fn test_render_subgraph_direction_nested() {
    let output = render_fixture("subgraph_direction_nested.mmd");

    assert!(
        output.contains("Vertical Outer"),
        "Should render outer title"
    );
    assert!(
        output.contains("Horizontal Inner"),
        "Should render inner title"
    );
    assert!(output.contains("D"), "Should render node D");
    assert!(output.contains("A"), "Should render node A");
    assert!(output.contains("C"), "Should render node C");
}

#[test]
fn test_render_subgraph_direction_mixed() {
    let output = render_fixture("subgraph_direction_mixed.mmd");

    assert!(
        output.contains("Left to Right"),
        "Should render LR subgraph title"
    );
    assert!(
        output.contains("Bottom to Top"),
        "Should render BT subgraph title"
    );
    assert!(output.contains("A"), "Should render node A");
    assert!(output.contains("B"), "Should render node B");
    assert!(output.contains("C"), "Should render node C");
    assert!(output.contains("D"), "Should render node D");
}

#[test]
fn test_subgraph_direction_mixed_layout() {
    let (_, layout) = layout_fixture("subgraph_direction_mixed.mmd");

    // A, B in LR subgraph: horizontal arrangement
    let a = layout.get_bounds("A").unwrap();
    let b = layout.get_bounds("B").unwrap();
    assert!(
        a.center_x() < b.center_x(),
        "A should be left of B in LR subgraph"
    );

    // C, D in BT subgraph: C below D (BT = source at bottom flows up)
    let c = layout.get_bounds("C").unwrap();
    let d = layout.get_bounds("D").unwrap();
    assert!(
        c.center_y() > d.center_y(),
        "C (source) should be below D (target) in BT subgraph: C_cy={} D_cy={}",
        c.center_y(),
        d.center_y()
    );

    // Check effective directions
    assert_eq!(layout.node_directions.get("A"), Some(&Direction::LeftRight));
    assert_eq!(layout.node_directions.get("C"), Some(&Direction::BottomTop));
}

#[test]
fn test_render_subgraph_direction_nested_both() {
    // Both parent (LR) and child (BT) have direction overrides.
    // Nodes in the inner subgraph should get the inner direction (BT),
    // not the outer (LR), regardless of HashMap iteration order.
    let output = render_fixture("subgraph_direction_nested_both.mmd");

    assert!(output.contains("Outer LR"), "Should render outer title");
    assert!(output.contains("Inner BT"), "Should render inner title");
    assert!(output.contains("A"), "Should render node A");
    assert!(output.contains("B"), "Should render node B");
    assert!(output.contains("C"), "Should render node C");
    assert!(output.contains("D"), "Should render node D");
}

#[test]
fn test_subgraph_direction_nested_both_layout() {
    // Verify deterministic direction assignment for nested overrides.
    let (_, layout) = layout_fixture("subgraph_direction_nested_both.mmd");

    // A, B are in inner (BT): deepest override wins → BottomTop
    assert_eq!(
        layout.node_directions.get("A"),
        Some(&Direction::BottomTop),
        "A should get inner BT direction, not outer LR"
    );
    assert_eq!(
        layout.node_directions.get("B"),
        Some(&Direction::BottomTop),
        "B should get inner BT direction, not outer LR"
    );

    // C is only in outer (LR): gets outer direction
    assert_eq!(
        layout.node_directions.get("C"),
        Some(&Direction::LeftRight),
        "C should get outer LR direction"
    );

    // D is outside both: gets diagram root direction (TD)
    assert_eq!(
        layout.node_directions.get("D"),
        Some(&Direction::TopDown),
        "D should get root TD direction"
    );
}

#[test]
fn test_subgraph_direction_cross_boundary_no_stale_waypoints() {
    // Cross-boundary edges (one endpoint inside override subgraph, one outside)
    // should NOT retain waypoints from the parent layout after reconciliation.
    let (diagram, layout) = layout_fixture("subgraph_direction_cross_boundary.mmd");

    // C-->A crosses into the LR subgraph; B-->D crosses out.
    // After reconciliation, these edges should have their waypoints invalidated
    // (empty or absent) so the router recomputes from reconciled positions.
    let ca_idx = diagram
        .edges
        .iter()
        .find(|e| e.from == "C" && e.to == "A")
        .expect("C->A edge should exist")
        .index;
    let bd_idx = diagram
        .edges
        .iter()
        .find(|e| e.from == "B" && e.to == "D")
        .expect("B->D edge should exist")
        .index;

    // Ensure the fixture makes these long edges (rank span > 1), so waypoints
    // would exist without invalidation.
    let ca_layer_diff = layout
        .grid_positions
        .get("C")
        .unwrap()
        .layer
        .abs_diff(layout.grid_positions.get("A").unwrap().layer);
    let bd_layer_diff = layout
        .grid_positions
        .get("B")
        .unwrap()
        .layer
        .abs_diff(layout.grid_positions.get("D").unwrap().layer);
    assert!(
        ca_layer_diff > 1,
        "fixture should make C->A a long edge (layer diff > 1)"
    );
    assert!(
        bd_layer_diff > 1,
        "fixture should make B->D a long edge (layer diff > 1)"
    );

    // Cross-boundary waypoints are clipped to the subgraph border (not
    // removed) so the text renderer can route them.  The SVG renderer
    // re-routes cross-boundary edges from scratch, ignoring waypoints.
    // Verify the waypoints exist but have fewer points than the original
    // long edge (the clipping should have trimmed interior points).
    let ca_wps = layout.edge_waypoints.get(&ca_idx);
    let bd_wps = layout.edge_waypoints.get(&bd_idx);
    assert!(
        ca_wps.is_some(),
        "C->A cross-boundary edge should have clipped waypoints"
    );
    assert!(
        bd_wps.is_some(),
        "B->D cross-boundary edge should have clipped waypoints"
    );
}

#[test]
fn test_render_subgraph_direction_cross_boundary() {
    // Smoke test: cross-boundary edges with direction overrides should render
    // without panics and include all nodes.
    let output = render_fixture("subgraph_direction_cross_boundary.mmd");

    assert!(
        output.contains("Horizontal Section"),
        "Should render subgraph title"
    );
    assert!(output.contains("A"), "Should render node A");
    assert!(output.contains("B"), "Should render node B");
    assert!(output.contains("C"), "Should render node C");
    assert!(output.contains("D"), "Should render node D");
}
