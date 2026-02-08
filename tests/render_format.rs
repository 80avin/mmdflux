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
fn mmds_positioned_payload_rejects_text_formats_with_actionable_error() {
    let input =
        std::fs::read_to_string("tests/fixtures/mmds/positioned/routed-basic.json").unwrap();
    let mut instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    instance.parse(&input).unwrap();

    for format in [OutputFormat::Text, OutputFormat::Ascii] {
        let err = instance
            .render(format, &mmdflux::diagram::RenderConfig::default())
            .unwrap_err();
        assert!(err.to_string().contains("positioned MMDS"));
        assert!(err.to_string().contains("use --format svg"));
    }
}

fn mmds_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/mmds/{name}")).unwrap()
}

#[test]
fn mmds_capability_matrix_matches_geometry_level_contract() {
    let mut layout_instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    layout_instance
        .parse(&mmds_fixture("positioned/layout-basic.json"))
        .unwrap();
    for format in [
        OutputFormat::Text,
        OutputFormat::Ascii,
        OutputFormat::Svg,
        OutputFormat::Json,
    ] {
        assert!(
            layout_instance
                .render(format, &mmdflux::diagram::RenderConfig::default())
                .is_ok(),
            "layout payload should support {format}"
        );
    }

    let mut routed_instance = mmdflux::diagrams::mmds::MmdsInstance::default();
    routed_instance
        .parse(&mmds_fixture("positioned/routed-basic.json"))
        .unwrap();
    for format in [OutputFormat::Text, OutputFormat::Ascii] {
        let err = routed_instance
            .render(format, &mmdflux::diagram::RenderConfig::default())
            .expect_err("routed payload should reject text/ascii");
        assert!(err.to_string().contains("positioned MMDS"));
    }
    assert!(
        routed_instance
            .render(
                OutputFormat::Svg,
                &mmdflux::diagram::RenderConfig::default()
            )
            .is_ok()
    );
    assert!(
        routed_instance
            .render(
                OutputFormat::Json,
                &mmdflux::diagram::RenderConfig::default()
            )
            .is_ok()
    );
}
