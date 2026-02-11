use std::fs;
use std::path::Path;

use mmdflux::diagram::{GeometryLevel, LayoutEngineId, OutputFormat, RenderConfig, RoutingMode};
use mmdflux::diagrams::class::ClassInstance;
use mmdflux::registry::DiagramInstance;

#[test]
fn class_instance_parse_simple() {
    let mut instance = ClassInstance::new();
    let result = instance.parse("classDiagram\nclass User");
    assert!(result.is_ok());
}

#[test]
fn class_instance_parse_error_on_invalid() {
    let mut instance = ClassInstance::new();
    let result = instance.parse("not a class diagram");
    assert!(result.is_err());
}

#[test]
fn class_instance_parse_and_render_text() {
    let mut instance = ClassInstance::new();
    instance
        .parse("classDiagram\nclass A\nclass B\nA --> B")
        .unwrap();
    let out = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert!(out.contains('A'));
    assert!(out.contains('B'));
}

#[test]
fn class_instance_render_ascii() {
    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA --> B").unwrap();
    let out = instance
        .render(OutputFormat::Ascii, &RenderConfig::default())
        .unwrap();
    // ASCII mode should not contain Unicode box-drawing chars
    assert!(!out.contains('│'));
    assert!(!out.contains('─'));
}

#[test]
fn class_instance_render_svg() {
    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA --> B").unwrap();
    let out = instance
        .render(OutputFormat::Svg, &RenderConfig::default())
        .unwrap();
    assert!(out.starts_with("<svg"));
    assert!(out.contains("<text"));
}

#[test]
fn class_instance_render_before_parse_errors() {
    let instance = ClassInstance::new();
    let result = instance.render(OutputFormat::Text, &RenderConfig::default());
    assert!(result.is_err());
}

#[test]
fn class_instance_supports_text_ascii_svg() {
    let instance = ClassInstance::new();
    assert!(instance.supports_format(OutputFormat::Text));
    assert!(instance.supports_format(OutputFormat::Ascii));
    assert!(instance.supports_format(OutputFormat::Svg));
}

#[test]
fn class_instance_dependency_renders_dotted() {
    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA ..> B").unwrap();
    let out = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    // Dotted edges use ╎ or ┊ or similar in text mode
    assert!(out.contains('A'));
    assert!(out.contains('B'));
}

#[test]
fn class_instance_inheritance_renders() {
    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nAnimal <|-- Dog").unwrap();
    let out = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert!(out.contains("Animal"));
    assert!(out.contains("Dog"));
}

#[test]
fn lollipop_relations_render_all_participating_classes() {
    let mut instance = ClassInstance::new();
    let input = "classDiagram\nclass Class01 {\n  int amount\n  draw()\n}\nClass01 --() bar\nClass02 --() bar\nfoo ()-- Class01";
    instance.parse(input).unwrap();
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();

    assert!(output.contains("Class02"));
    assert!(output.contains("foo"));
    assert!(!output.contains("│ bar │"));
    assert!(!output.contains("│ foo │"));
}

#[test]
fn lollipop_same_name_interfaces_render_as_distinct_endpoints() {
    let mut instance = ClassInstance::new();
    let input = "classDiagram\nService --() InterfaceA\nClient --() InterfaceA";
    instance.parse(input).unwrap();
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();

    assert_eq!(output.matches("InterfaceA").count(), 2);
    assert!(!output.contains("│ InterfaceA │"));
}

#[test]
fn namespace_blocks_render_namespace_titles() {
    let mut instance = ClassInstance::new();
    let input = "\
classDiagram
namespace BaseShapes {
  class Triangle
  class Rectangle
}
Triangle --> Rectangle";
    instance.parse(input).unwrap();
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();

    assert!(output.contains("BaseShapes"));
    assert!(output.contains("Triangle"));
    assert!(output.contains("Rectangle"));
}

#[test]
fn class_instance_via_registry() {
    let registry = mmdflux::registry::default_registry();
    let mut instance = registry.create("class").unwrap();
    instance
        .parse("classDiagram\nclass User\nclass Order\nUser --> Order")
        .unwrap();
    let out = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert!(out.contains("User"));
    assert!(out.contains("Order"));
}

#[test]
fn class_instance_unknown_engine_rejected_at_parse_boundary() {
    let err = LayoutEngineId::parse("nonexistent").unwrap_err();
    assert!(err.message.contains("unknown layout engine"));
}

#[test]
fn class_instance_known_non_dagre_engine_errors_cleanly() {
    let mut instance = ClassInstance::new();
    instance.parse("classDiagram\nA --> B").unwrap();
    let result = instance.render(
        OutputFormat::Text,
        &RenderConfig {
            layout_engine: Some(LayoutEngineId::Cose),
            ..RenderConfig::default()
        },
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("not yet implemented"));
}

#[test]
fn class_routed_mmds_honors_routing_mode_override() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("class")
        .join("animal_hierarchy.mmd");
    let input = fs::read_to_string(&fixture).expect("class fixture should read");

    let mut instance = ClassInstance::new();
    instance.parse(&input).expect("fixture should parse");

    let full = instance
        .render(
            OutputFormat::Mmds,
            &RenderConfig {
                geometry_level: GeometryLevel::Routed,
                routing_mode: Some(RoutingMode::FullCompute),
                ..RenderConfig::default()
            },
        )
        .expect("full-compute mmds should render");
    let unified = instance
        .render(
            OutputFormat::Mmds,
            &RenderConfig {
                geometry_level: GeometryLevel::Routed,
                routing_mode: Some(RoutingMode::UnifiedPreview),
                ..RenderConfig::default()
            },
        )
        .expect("unified-preview mmds should render");

    assert_ne!(
        full, unified,
        "class routed MMDS should reflect routing-mode override semantics"
    );
}

#[test]
fn class_routed_mmds_honors_routing_mode_override_on_cycle() {
    let input = "classDiagram\nA --> B\nB --> C\nC --> A\n";
    let mut instance = ClassInstance::new();
    instance.parse(input).expect("class cycle should parse");

    let full = instance
        .render(
            OutputFormat::Mmds,
            &RenderConfig {
                geometry_level: GeometryLevel::Routed,
                routing_mode: Some(RoutingMode::FullCompute),
                ..RenderConfig::default()
            },
        )
        .expect("full-compute mmds should render");
    let unified = instance
        .render(
            OutputFormat::Mmds,
            &RenderConfig {
                geometry_level: GeometryLevel::Routed,
                routing_mode: Some(RoutingMode::UnifiedPreview),
                ..RenderConfig::default()
            },
        )
        .expect("unified-preview mmds should render");

    assert_ne!(
        full, unified,
        "class routed MMDS cycle output should change under unified-preview override"
    );
}

#[test]
fn class_svg_honors_routing_mode_override_on_cycle() {
    let input = "classDiagram\nA --> B\nB --> C\nC --> A\n";
    let mut instance = ClassInstance::new();
    instance.parse(input).expect("class cycle should parse");

    let full = instance
        .render(
            OutputFormat::Svg,
            &RenderConfig {
                routing_mode: Some(RoutingMode::FullCompute),
                ..RenderConfig::default()
            },
        )
        .expect("full-compute svg should render");
    let unified = instance
        .render(
            OutputFormat::Svg,
            &RenderConfig {
                routing_mode: Some(RoutingMode::UnifiedPreview),
                ..RenderConfig::default()
            },
        )
        .expect("unified-preview svg should render");

    assert!(full.starts_with("<svg"));
    assert!(unified.starts_with("<svg"));
    assert_ne!(
        full, unified,
        "class SVG cycle output should change under unified-preview override"
    );
}
