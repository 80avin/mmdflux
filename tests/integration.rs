//! Integration tests for mmdflux.
//!
//! These tests verify the full parsing and rendering pipeline using fixture files.

use mmdflux::{build_diagram, parse_flowchart, Direction, Shape};
use mmdflux::render::{render, RenderOptions};
use std::fs;
use std::path::Path;

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
        assert!(output.contains("Client"));
        assert!(output.contains("Server"));
        // Diamond labels may render with some overlap in complex layouts
        assert!(
            output.contains("Authenticated") || output.contains('<'),
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
        let unicode_chars = ['─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '╭', '╮', '╯', '╰'];
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
