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
    assert!(instance.supports_format(OutputFormat::Svg)); // Planned support
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
