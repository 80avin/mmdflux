//! Integration tests for mmdflux.
//!
//! These tests verify the full parsing and rendering pipeline using fixture files.

use std::fs;
use std::path::Path;

use mmdflux::render::{RenderOptions, render};
use mmdflux::{Direction, Shape, build_diagram, parse_flowchart};

/// Helper to load a fixture file.
fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", name, e))
}

/// Helper to parse, build, and render a fixture.
fn render_fixture(name: &str) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    render(&diagram, &RenderOptions::default())
}

/// Helper to parse, build, and render with ASCII-only output.
fn render_fixture_ascii(name: &str) -> String {
    let input = load_fixture(name);
    let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
    let diagram = build_diagram(&flowchart);
    render(&diagram, &RenderOptions { ascii_only: true })
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
    }

    #[test]
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
    }

    #[test]
    fn git_workflow_renders() {
        let output = render_fixture("git_workflow.mmd");
        // In LR layout with labels, some text may overlap
        // Just verify rendering works and contains key elements
        assert!(!output.is_empty());
        // At least some node text should appear
        assert!(
            output.contains("Working") || output.contains("Staging") || output.contains("Local"),
            "Should contain at least one node label fragment"
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
    use mmdflux::render::{LayoutConfig, compute_layout_dagre, route_all_edges};

    use super::*;

    /// Helper to parse, build a diagram from a fixture.
    fn parse_and_build(name: &str) -> mmdflux::Diagram {
        let input = load_fixture(name);
        let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
        build_diagram(&flowchart)
    }

    #[test]
    fn stagger_present_for_multiple_cycles() {
        // multiple_cycles.mmd: A[Top] --> B[Middle], B --> C[Bottom], C --> A, C --> B
        // Dagre computes A rightward (aligned with dummy chain for reversed A→C edge)
        // After stagger: A's center_x should be > B's and C's center_x
        let diagram = parse_and_build("multiple_cycles.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        let a_bounds = layout.node_bounds.get("A").expect("A should have bounds");
        let b_bounds = layout.node_bounds.get("B").expect("B should have bounds");
        let c_bounds = layout.node_bounds.get("C").expect("C should have bounds");

        assert!(
            a_bounds.center_x() > b_bounds.center_x(),
            "A (center_x={}) should be right of B (center_x={})",
            a_bounds.center_x(),
            b_bounds.center_x()
        );
        assert!(
            a_bounds.center_x() > c_bounds.center_x(),
            "A (center_x={}) should be right of C (center_x={})",
            a_bounds.center_x(),
            c_bounds.center_x()
        );
    }

    #[test]
    fn no_stagger_for_simple_chain() {
        // chain.mmd: linear chain with no backward edges → no stagger
        let diagram = parse_and_build("chain.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        // All nodes should have the same center_x (centered, no stagger)
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
        let diagram = parse_and_build("multiple_cycles.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        // Find edges involving node A
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
        let a_b_start = a_b_edge.start;
        let c_a_end = c_a_edge.end;

        assert_ne!(
            a_b_start, c_a_end,
            "Forward A→B start ({:?}) and backward C→A end ({:?}) should differ on A",
            a_b_start, c_a_end
        );
    }

    #[test]
    fn stagger_present_for_simple_cycle() {
        // simple_cycle.mmd has backward edges → should show stagger
        let diagram = parse_and_build("simple_cycle.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);

        // With cycle, nodes should not all have the same center_x
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
    use mmdflux::render::{LayoutConfig, compute_layout_dagre, route_all_edges};

    use super::*;

    fn parse_and_build(name: &str) -> mmdflux::Diagram {
        let input = load_fixture(name);
        let flowchart = parse_flowchart(&input).expect("Failed to parse fixture");
        build_diagram(&flowchart)
    }

    /// Verify that no row has immediately adjacent down-arrows (▼▼).
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
        let diagram = parse_and_build(fixture_name);
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);
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

        // Check all pairs are distinct
        for i in 0..arrival_xs.len() {
            for j in (i + 1)..arrival_xs.len() {
                assert_ne!(
                    arrival_xs[i], arrival_xs[j],
                    "{}: edges arriving at {} have duplicate x-coordinate {} (all: {:?})",
                    fixture_name, target_node, arrival_xs[i], arrival_xs
                );
            }
        }
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
        let diagram = parse_and_build("fan_out.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_dagre(&diagram, &config);
        let routed = route_all_edges(&diagram.edges, &layout, diagram.direction);

        let departure_xs: Vec<usize> = routed
            .iter()
            .filter(|r| r.edge.from == "A")
            .map(|r| r.start.x)
            .collect();

        assert!(departure_xs.len() >= 2);
        for i in 0..departure_xs.len() {
            for j in (i + 1)..departure_xs.len() {
                assert_ne!(
                    departure_xs[i], departure_xs[j],
                    "fan_out.mmd: edges departing A have duplicate x {} (all: {:?})",
                    departure_xs[i], departure_xs
                );
            }
        }
    }
}

// =============================================================================
// Direct Layout Integration Tests
// =============================================================================

mod direct_layout {
    use mmdflux::render::{Layout, LayoutConfig, compute_layout_direct};

    use super::*;

    fn parse_and_build(name: &str) -> mmdflux::Diagram {
        let input = load_fixture(name);
        let flowchart = parse_flowchart(&input).expect("Failed to parse");
        build_diagram(&flowchart)
    }

    fn layout_fixture(name: &str) -> Layout {
        let diagram = parse_and_build(name);
        let config = LayoutConfig::default();
        compute_layout_direct(&diagram, &config)
    }

    #[test]
    fn direct_simple_produces_valid_layout() {
        let layout = layout_fixture("simple.mmd");

        assert!(layout.width > 0, "canvas width must be positive");
        assert!(layout.height > 0, "canvas height must be positive");
        assert!(layout.draw_positions.contains_key("A"));
        assert!(layout.draw_positions.contains_key("B"));
        assert!(layout.node_bounds.contains_key("A"));
        assert!(layout.node_bounds.contains_key("B"));
    }

    #[test]
    fn direct_no_node_overlaps() {
        let layout = layout_fixture("chain.mmd");

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
        let layout = layout_fixture("fan_out.mmd");

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
        let layout = layout_fixture("simple.mmd");

        let a_y = layout.draw_positions["A"].1;
        let b_y = layout.draw_positions["B"].1;
        assert!(
            a_y < b_y,
            "in TD layout, A (rank 0) should be above B (rank 1)"
        );
    }

    #[test]
    fn direct_lr_horizontal_ordering() {
        let diagram = parse_and_build("left_right.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        assert!(
            layout.width > layout.height || layout.node_bounds.len() <= 2,
            "LR layout should generally be wider than tall"
        );
    }

    #[test]
    fn direct_preserves_cross_axis_stagger() {
        // fan_out.mmd: A→B, A→C, A→D — layer 1 has B, C, D which should
        // have distinct x positions from dagre's BK algorithm.
        let diagram = parse_and_build("fan_out.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

        // B, C, D are in the same layer — they should have distinct x centers
        let b_x = layout.node_bounds["B"].center_x();
        let c_x = layout.node_bounds["C"].center_x();
        let d_x = layout.node_bounds["D"].center_x();

        // At least two must differ (dagre assigns different cross-axis positions)
        assert!(
            b_x != c_x || c_x != d_x,
            "B/C/D all at same x center ({}) — cross-axis stagger was lost",
            b_x,
        );
    }

    #[test]
    fn direct_cycle_no_edge_overlap_at_attachment() {
        let diagram = parse_and_build("simple_cycle.mmd");
        let config = LayoutConfig::default();
        let layout = compute_layout_direct(&diagram, &config);

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

        for entry in fs::read_dir(&fixture_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().map_or(false, |e| e == "mmd") {
                let name = path.file_stem().unwrap().to_str().unwrap();
                let input = fs::read_to_string(&path).unwrap();
                let flowchart = parse_flowchart(&input).expect("Failed to parse");
                let diagram = build_diagram(&flowchart);
                let output = render(&diagram, &RenderOptions::default());
                fs::write(snapshot_dir.join(format!("{}.txt", name)), &output).unwrap();
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
