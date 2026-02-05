use mmdflux::RenderConfig;
use mmdflux::render::OutputFormat;

#[test]
fn test_render_config_has_show_ids() {
    let config = RenderConfig::default();
    assert!(!config.show_ids);
}

#[test]
fn test_render_config_show_ids_set() {
    let config = RenderConfig {
        show_ids: true,
        ..Default::default()
    };
    assert!(config.show_ids);
}

#[test]
fn output_format_from_render_module() {
    // Verify OutputFormat is accessible from render module
    let text = OutputFormat::Text;
    let ascii = OutputFormat::Ascii;
    let svg = OutputFormat::Svg;

    assert_eq!(text, OutputFormat::default());
    assert_ne!(ascii, svg);
}

#[test]
fn output_format_debug() {
    assert_eq!(format!("{:?}", OutputFormat::Text), "Text");
    assert_eq!(format!("{:?}", OutputFormat::Svg), "Svg");
}

#[test]
fn output_format_json_display() {
    assert_eq!(format!("{}", OutputFormat::Json), "json");
}
