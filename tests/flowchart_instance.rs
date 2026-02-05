use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::registry::DiagramInstance;

#[test]
fn flowchart_instance_parse_simple() {
    let mut instance = FlowchartInstance::new();
    let result = instance.parse("graph TD\nA-->B");
    assert!(result.is_ok());
}

#[test]
fn flowchart_instance_parse_error_on_invalid() {
    let mut instance = FlowchartInstance::new();
    let result = instance.parse("not a valid diagram }{{}");
    assert!(result.is_err());
}

#[test]
fn flowchart_instance_render_text() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Text, &config).unwrap();

    // Output should contain the node labels
    assert!(output.contains('A'));
    assert!(output.contains('B'));
}

#[test]
fn flowchart_instance_render_ascii() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Ascii, &config).unwrap();

    // ASCII output should use ASCII characters only
    // No Unicode box drawing chars
    assert!(!output.contains('│'));
    assert!(!output.contains('─'));
}

#[test]
fn flowchart_instance_render_before_parse_errors() {
    let instance = FlowchartInstance::new();
    let config = RenderConfig::default();
    let result = instance.render(OutputFormat::Text, &config);
    assert!(result.is_err());
}

#[test]
fn flowchart_instance_supports_text_and_ascii() {
    let instance = FlowchartInstance::new();
    assert!(instance.supports_format(OutputFormat::Text));
    assert!(instance.supports_format(OutputFormat::Ascii));
    assert!(instance.supports_format(OutputFormat::Svg));
}

#[test]
fn flowchart_instance_render_svg() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Svg, &config).unwrap();

    assert!(output.starts_with("<svg"));
    assert!(output.contains("<text"));
}

#[test]
fn flowchart_instance_render_json() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Json, &config).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 1);

    // Should include positions since layout is computed
    let nodes = parsed["nodes"].as_array().unwrap();
    for node in nodes {
        assert!(
            node["position"].is_object(),
            "Node should have position: {}",
            node
        );
    }
}

#[test]
fn flowchart_instance_matches_existing_render_output() {
    // Verify that FlowchartInstance produces the same output as the
    // existing render() function
    use mmdflux::render::{RenderOptions, render};
    use mmdflux::{build_diagram, parse_flowchart};

    let input = "graph TD\nA-->B";

    // Old path
    let flowchart = parse_flowchart(input).unwrap();
    let diagram = build_diagram(&flowchart);
    let old_output = render(&diagram, &RenderOptions::default());

    // New path
    let mut instance = FlowchartInstance::new();
    instance.parse(input).unwrap();
    let new_output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();

    assert_eq!(old_output, new_output);
}

#[test]
fn test_show_ids_annotates_labels() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]\n").unwrap();

    let config = RenderConfig {
        show_ids: true,
        ..Default::default()
    };
    let output = instance.render(OutputFormat::Text, &config).unwrap();
    assert!(
        output.contains("A: Start"),
        "Should contain 'A: Start', got: {}",
        output
    );
    assert!(
        output.contains("B: End"),
        "Should contain 'B: End', got: {}",
        output
    );
}

#[test]
fn test_show_ids_bare_nodes_unchanged() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA --> B\n").unwrap();

    let config = RenderConfig {
        show_ids: true,
        ..Default::default()
    };
    let output = instance.render(OutputFormat::Text, &config).unwrap();
    assert!(
        !output.contains("A: A"),
        "Bare node should not be annotated: {}",
        output
    );
}

#[test]
fn test_show_ids_false_no_annotation() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]\n").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Text, &config).unwrap();
    assert!(
        !output.contains("A:"),
        "Default should not annotate: {}",
        output
    );
}
