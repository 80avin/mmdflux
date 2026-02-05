//! Full integration tests for the multi-format architecture.
//!
//! These tests validate that registry detection, parsing, and rendering work
//! together across diagram types and output formats.

use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::registry::default_registry;

fn render_with_registry(input: &str, format: OutputFormat) -> String {
    let registry = default_registry();
    let diagram_id = registry.detect(input).expect("should detect diagram type");
    let mut instance = registry
        .create(diagram_id)
        .expect("should create diagram instance");
    instance.parse(input).expect("should parse");
    instance
        .render(format, &RenderConfig::default())
        .expect("should render")
}

fn render_flowchart_svg(input: &str) -> String {
    let registry = default_registry();
    let mut instance = registry
        .create("flowchart")
        .expect("should create flowchart instance");
    instance.parse(input).expect("should parse flowchart");
    instance
        .render(OutputFormat::Svg, &RenderConfig::default())
        .expect("should render svg")
}

#[test]
fn registry_detects_all_diagram_types() {
    let registry = default_registry();

    assert_eq!(registry.detect("graph TD\nA-->B"), Some("flowchart"));
    assert_eq!(registry.detect("flowchart LR\nA-->B"), Some("flowchart"));
    assert_eq!(registry.detect("pie\n\"A\": 50"), Some("pie"));
    assert_eq!(registry.detect("info"), Some("info"));
    assert_eq!(
        registry.detect("packet-beta\n0-15: \"Header\""),
        Some("packet")
    );
}

#[test]
fn all_diagram_types_render_text() {
    let cases = [
        ("flowchart", "graph TD\nA-->B", "A"),
        ("pie", "pie\n\"A\": 50", "[Pie Chart]"),
        ("info", "info", "mmdflux v"),
        (
            "packet",
            "packet-beta\n0-15: \"Header\"",
            "[Packet Diagram]",
        ),
    ];

    for (id, input, expected) in cases {
        let registry = default_registry();
        let mut instance = registry.create(id).expect("should create");
        instance.parse(input).expect("should parse");
        let output = instance
            .render(OutputFormat::Text, &RenderConfig::default())
            .expect("should render text");
        assert!(
            output.contains(expected),
            "{} output missing expected content",
            id
        );
    }
}

#[test]
fn flowchart_renders_all_formats() {
    let input = "graph TD\nA[Start]-->B[End]";
    let registry = default_registry();
    let mut instance = registry
        .create("flowchart")
        .expect("should create flowchart");
    instance.parse(input).expect("should parse flowchart");

    let text = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .expect("should render text");
    assert!(text.contains("Start"));
    assert!(text.contains("End"));
    assert!(text.contains('│'));

    let ascii = instance
        .render(OutputFormat::Ascii, &RenderConfig::default())
        .expect("should render ascii");
    assert!(ascii.contains("Start"));
    assert!(!ascii.contains('│'));

    let svg = instance
        .render(OutputFormat::Svg, &RenderConfig::default())
        .expect("should render svg");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("Start"));
    assert!(svg.contains("</svg>"));
}

#[test]
fn svg_shapes_render_expected_elements() {
    let input = r#"graph TD
    A[Rectangle]-->B(Rounded)
    B-->C{Diamond}
    C-->D((Circle))"#;
    let svg = render_flowchart_svg(input);

    assert!(svg.contains("<rect"));
    assert!(svg.contains("rx="));
    assert!(svg.contains("ry="));
    assert!(svg.contains("<polygon"));
    assert!(svg.contains("<ellipse"));
}

#[test]
fn svg_edges_and_subgraphs_render() {
    let input = r#"graph TD
    subgraph sg[Group]
        A-->A
    end
    B-.->C"#;
    let svg = render_flowchart_svg(input);

    assert!(svg.contains("class=\"subgraph\""));
    assert!(svg.contains("Group"));
    assert!(svg.matches("<path").count() >= 2);
    assert!(svg.contains("stroke-dasharray"));
}

#[test]
fn registry_render_smoke() {
    let text = render_with_registry("graph TD\nA-->B", OutputFormat::Text);
    assert!(text.contains("A"));

    let svg = render_with_registry("graph TD\nA-->B", OutputFormat::Svg);
    assert!(svg.starts_with("<svg"));
}
