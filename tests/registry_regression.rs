//! Temporary parity check while the legacy path exists.
//! Remove or replace with golden-output tests once the legacy path is retired.

use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::registry::default_registry;
use mmdflux::render::{RenderOptions, render};
use mmdflux::{build_diagram, parse_flowchart};

/// Helper to compare old vs new rendering paths (temporary)
fn compare_outputs(input: &str, ascii: bool) {
    // Old path: direct parsing and rendering
    let flowchart = parse_flowchart(input).expect("Old path parse failed");
    let diagram = build_diagram(&flowchart);
    let output_format = if ascii {
        OutputFormat::Ascii
    } else {
        OutputFormat::Text
    };
    let old_options = RenderOptions {
        output_format,
        ..Default::default()
    };
    let old_output = render(&diagram, &old_options);

    // New path: registry-based
    let registry = default_registry();
    let diagram_id = registry.detect(input).expect("New path detect failed");
    assert_eq!(diagram_id, "flowchart");

    let mut instance = registry.create(diagram_id).expect("New path create failed");
    instance.parse(input).expect("New path parse failed");

    let new_output = instance
        .render(output_format, &RenderConfig::default())
        .expect("New path render failed");

    assert_eq!(
        old_output, new_output,
        "Output mismatch for input:\n{}\n\nOld:\n{}\n\nNew:\n{}",
        input, old_output, new_output
    );
}

#[test]
fn regression_simple_graph() {
    compare_outputs("graph TD\nA-->B", false);
    compare_outputs("graph TD\nA-->B", true);
}

#[test]
fn regression_graph_lr() {
    compare_outputs("graph LR\nA-->B-->C", false);
}

#[test]
fn regression_flowchart_keyword() {
    compare_outputs("flowchart TD\nA[Start]-->B[End]", false);
}

#[test]
fn regression_with_labels() {
    compare_outputs("graph TD\nA-->|label|B", false);
}

#[test]
fn regression_multiple_nodes() {
    compare_outputs("graph TD\nA-->B\nB-->C\nC-->D", false);
}

#[test]
fn regression_fan_out() {
    compare_outputs("graph TD\nA-->B\nA-->C\nA-->D", false);
}

#[test]
fn regression_fan_in() {
    compare_outputs("graph TD\nA-->D\nB-->D\nC-->D", false);
}

#[test]
fn regression_shapes() {
    compare_outputs("graph TD\nA[rect]\nB(round)\nC{diamond}", false);
}

#[test]
fn regression_edge_styles() {
    compare_outputs("graph TD\nA-->B\nA-.->C\nA==>D", false);
}

#[test]
fn regression_subgraph() {
    compare_outputs("graph TD\nsubgraph sg[Title]\nA-->B\nend\nC-->A", false);
}

#[test]
fn regression_backward_edge() {
    compare_outputs("graph TD\nA-->B\nB-->A", false);
}

#[test]
fn regression_self_edge() {
    compare_outputs("graph TD\nA-->A", false);
}

// Engine selection via registry path
#[test]
fn regression_engine_selection_via_registry() {
    let registry = default_registry();
    let input = "graph TD\nA-->B";

    let mut instance = registry.create("flowchart").unwrap();
    instance.parse(input).unwrap();

    // Default (None) and explicit "dagre" should produce identical output
    let default_out = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    let dagre_out = instance
        .render(
            OutputFormat::Text,
            &RenderConfig {
                layout_engine: Some("dagre".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(default_out, dagre_out);

    // Unknown engine should error
    let err = instance.render(
        OutputFormat::Text,
        &RenderConfig {
            layout_engine: Some("elk".to_string()),
            ..Default::default()
        },
    );
    assert!(err.is_err());
}

// Test all existing fixtures
#[test]
fn regression_all_fixtures() {
    use std::fs;

    let fixtures_dir = std::path::Path::new("tests/fixtures");
    for entry in fs::read_dir(fixtures_dir).expect("fixtures dir") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "mmd") {
            let input = fs::read_to_string(&path).expect("read fixture");

            // Skip non-flowchart fixtures
            if !input.trim_start().starts_with("graph")
                && !input.trim_start().starts_with("flowchart")
            {
                continue;
            }

            compare_outputs(&input, false);
        }
    }
}
