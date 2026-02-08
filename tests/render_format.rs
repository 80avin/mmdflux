use mmdflux::diagram::OutputFormat;
use mmdflux::registry::DiagramInstance;

#[test]
fn output_format_from_render_module() {
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
    assert_eq!(format!("{}", OutputFormat::Json), "mmds");
}

#[test]
fn mmds_scaffold_render_error_message_is_stable_across_formats() {
    let input = std::fs::read_to_string("tests/fixtures/mmds/minimal-layout.json").unwrap();
    let mut instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    instance.parse(&input).unwrap();

    for format in [
        OutputFormat::Text,
        OutputFormat::Ascii,
        OutputFormat::Svg,
        OutputFormat::Json,
    ] {
        let err = instance
            .render(format, &mmdflux::diagram::RenderConfig::default())
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "MMDS input scaffold: hydration/render pipeline is not implemented yet"
        );
    }
}
