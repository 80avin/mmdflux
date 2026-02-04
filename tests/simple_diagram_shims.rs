use mmdflux::diagram::{OutputFormat, RenderConfig};
use mmdflux::diagrams::{info, packet, pie};
use mmdflux::registry::DiagramInstance;

// Pie tests
#[test]
fn pie_definition_exists() {
    let def = pie::definition();
    assert_eq!(def.id, "pie");
}

#[test]
fn pie_detector_works() {
    assert!(pie::detect("pie\n\"A\": 50"));
    assert!(pie::detect("pie title My Chart\n\"A\": 50"));
    assert!(!pie::detect("graph TD\nA-->B"));
}

#[test]
fn pie_instance_renders() {
    let mut instance = pie::PieInstance::new();
    instance.parse("pie\n\"A\": 50\n\"B\": 50").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Text, &config).unwrap();

    // Pie currently renders as simple text
    assert!(!output.is_empty());
}

// Info tests
#[test]
fn info_definition_exists() {
    let def = info::definition();
    assert_eq!(def.id, "info");
}

#[test]
fn info_detector_works() {
    assert!(info::detect("info"));
    assert!(!info::detect("pie"));
}

#[test]
fn info_instance_renders() {
    let mut instance = info::InfoInstance::new();
    instance.parse("info").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Text, &config).unwrap();

    // Info renders mmdflux version info
    assert!(output.contains("mmdflux"));
}

// Packet tests
#[test]
fn packet_definition_exists() {
    let def = packet::definition();
    assert_eq!(def.id, "packet");
}

#[test]
fn packet_detector_works() {
    assert!(packet::detect("packet-beta"));
    assert!(!packet::detect("graph TD"));
}

#[test]
fn packet_instance_renders() {
    let mut instance = packet::PacketInstance::new();
    instance.parse("packet-beta\n0-15: \"Header\"").unwrap();

    let config = RenderConfig::default();
    let output = instance.render(OutputFormat::Text, &config).unwrap();

    assert!(!output.is_empty());
}

// SVG not supported for simple diagrams
#[test]
fn simple_diagrams_dont_support_svg() {
    let pie_inst = pie::PieInstance::new();
    let info_inst = info::InfoInstance::new();
    let packet_inst = packet::PacketInstance::new();

    assert!(!pie_inst.supports_format(OutputFormat::Svg));
    assert!(!info_inst.supports_format(OutputFormat::Svg));
    assert!(!packet_inst.supports_format(OutputFormat::Svg));
}
