use mmdflux::diagram::{OutputFormat, RenderConfig, RenderError};

#[test]
fn render_config_default() {
    let config = RenderConfig::default();
    assert!(config.padding.is_none());
    assert!(config.cluster_ranksep.is_none());
    assert!(config.svg_scale.is_none());
    assert!(config.svg_node_padding_x.is_none());
    assert!(config.svg_node_padding_y.is_none());
    assert!(config.edge_style.is_none());
    assert!(config.edge_radius.is_none());
    assert!(config.svg_diagram_padding.is_none());
    assert!(!config.show_ids);
}

#[test]
fn render_error_from_string() {
    let err: RenderError = "test error".into();
    assert_eq!(err.message, "test error");
    assert_eq!(err.to_string(), "test error");
}

#[test]
fn render_config_to_render_options_conversion() {
    let config = RenderConfig::default();
    let options: mmdflux::render::RenderOptions = (&config).into();
    assert!(matches!(options.output_format, OutputFormat::Text));
}

#[test]
fn diagram_layout_config_accessible_from_diagram_module() {
    let _ = mmdflux::diagram::LayoutConfig::default();
}
