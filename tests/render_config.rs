use mmdflux::diagram::OutputFormat;
use mmdflux::render::{DiagramLayoutConfig, RenderConfig, RenderError};

#[test]
fn render_config_default() {
    let config = RenderConfig::default();
    assert!(config.padding.is_none());
    assert!(config.cluster_ranksep.is_none());
    assert!(config.svg_scale.is_none());
    assert!(config.svg_node_padding_x.is_none());
    assert!(config.svg_node_padding_y.is_none());
}

#[test]
fn render_error_from_string() {
    let err: RenderError = "test error".into();
    assert_eq!(err.message, "test error");
    assert_eq!(err.to_string(), "test error");
}

#[test]
fn render_config_to_render_options_conversion() {
    // Verify we can convert RenderConfig to existing RenderOptions
    let config = RenderConfig::default();
    let options: mmdflux::render::RenderOptions = (&config).into();
    assert!(matches!(options.output_format, OutputFormat::Text));
}

#[test]
fn diagram_layout_config_accessible_from_render() {
    // DiagramLayoutConfig (from diagram module) should be re-exported from render module
    let _ = DiagramLayoutConfig::default();
}
