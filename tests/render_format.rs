use mmdflux::render::OutputFormat;

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
fn output_format_display() {
    assert_eq!(format!("{:?}", OutputFormat::Text), "Text");
    assert_eq!(format!("{:?}", OutputFormat::Svg), "Svg");
}
