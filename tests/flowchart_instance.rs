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
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert!(output.contains('A'));
    assert!(output.contains('B'));
}

#[test]
fn flowchart_instance_render_ascii() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();
    let output = instance
        .render(OutputFormat::Ascii, &RenderConfig::default())
        .unwrap();
    assert!(!output.contains('│'));
    assert!(!output.contains('─'));
}

#[test]
fn flowchart_instance_render_before_parse_errors() {
    let instance = FlowchartInstance::new();
    let result = instance.render(OutputFormat::Text, &RenderConfig::default());
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
    let output = instance
        .render(OutputFormat::Svg, &RenderConfig::default())
        .unwrap();
    assert!(output.starts_with("<svg"));
    assert!(output.contains("<text"));
}

#[test]
fn flowchart_instance_render_json() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]").unwrap();
    let output = instance
        .render(OutputFormat::Json, &RenderConfig::default())
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["geometry_level"], "layout");
    assert!(parsed["metadata"]["bounds"].is_object());
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["edges"][0]["id"], "e0");

    let nodes = parsed["nodes"].as_array().unwrap();
    for node in nodes {
        assert!(
            node["position"].is_object(),
            "Node should have position: {}",
            node
        );
        assert!(node["size"].is_object(), "Node should have size: {}", node);
    }

    // Layout level: no edge geometry
    assert!(!output.contains("\"path\""));
    assert!(!output.contains("\"is_backward\""));
}

#[test]
fn flowchart_instance_render_json_uses_defaults_omission() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();
    let output = instance
        .render(OutputFormat::Json, &RenderConfig::default())
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["defaults"]["node"]["shape"], "rectangle");
    assert_eq!(parsed["defaults"]["edge"]["stroke"], "solid");
    assert_eq!(parsed["defaults"]["edge"]["minlen"], 1);
    assert_eq!(parsed["edges"][0]["id"], "e0");
    assert!(parsed["edges"][0]["stroke"].is_null());
    assert!(parsed["edges"][0]["arrow_start"].is_null());
    assert!(parsed["edges"][0]["arrow_end"].is_null());
    assert!(parsed["edges"][0]["minlen"].is_null());
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
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert!(
        !output.contains("A:"),
        "Default should not annotate: {}",
        output
    );
}

#[test]
fn test_json_with_show_ids() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]\n").unwrap();

    let config = RenderConfig {
        show_ids: true,
        ..Default::default()
    };
    let output = instance.render(OutputFormat::Json, &config).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let node_a = nodes.iter().find(|n| n["id"] == "A").unwrap();
    assert_eq!(node_a["label"], "A: Start");
}

#[test]
fn test_json_without_show_ids() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA[Start] --> B[End]\n").unwrap();
    let output = instance
        .render(OutputFormat::Json, &RenderConfig::default())
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    let nodes = parsed["nodes"].as_array().unwrap();
    let node_a = nodes.iter().find(|n| n["id"] == "A").unwrap();
    assert_eq!(node_a["label"], "Start");
}

// --- Engine selection tests (Task 2.2) ---

#[test]
fn engine_selection_none_uses_default_dagre() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig::default(); // layout_engine: None
    let output = instance.render(OutputFormat::Text, &config).unwrap();
    assert!(output.contains('A'));
    assert!(output.contains('B'));
}

#[test]
fn engine_selection_explicit_dagre_matches_default() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let default_output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();

    let dagre_config = RenderConfig {
        layout_engine: Some("dagre".to_string()),
        ..Default::default()
    };
    let dagre_output = instance.render(OutputFormat::Text, &dagre_config).unwrap();

    assert_eq!(default_output, dagre_output);
}

#[cfg(not(feature = "engine-elk"))]
#[test]
fn engine_selection_unavailable_engine_errors() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig {
        layout_engine: Some("elk".to_string()),
        ..Default::default()
    };
    let result = instance.render(OutputFormat::Text, &config);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("engine-elk") || err.message.contains("not available"),
        "error should be actionable: {}",
        err.message
    );
}

#[test]
fn engine_selection_unknown_engine_errors() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig {
        layout_engine: Some("nonexistent".to_string()),
        ..Default::default()
    };
    let result = instance.render(OutputFormat::Text, &config);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("unknown layout engine"),
        "error should mention unknown engine: {}",
        err.message
    );
}

#[test]
fn engine_selection_unknown_engine_svg_errors() {
    let mut instance = FlowchartInstance::new();
    instance.parse("graph TD\nA-->B").unwrap();

    let config = RenderConfig {
        layout_engine: Some("does-not-exist".to_string()),
        ..Default::default()
    };
    let result = instance.render(OutputFormat::Svg, &config);
    assert!(result.is_err());
}
