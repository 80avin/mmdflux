use mmdflux::diagram::EdgeStyle;

#[test]
fn edge_style_parse_accepts_canonical_values() {
    assert_eq!(EdgeStyle::parse("sharp").unwrap(), EdgeStyle::Sharp);
    assert_eq!(EdgeStyle::parse("smooth").unwrap(), EdgeStyle::Smooth);
    assert_eq!(EdgeStyle::parse("rounded").unwrap(), EdgeStyle::Rounded);
}

#[test]
fn edge_style_parse_rejects_legacy_curved_with_migration() {
    let err = EdgeStyle::parse("curved").unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("smooth"),
        "error should name the replacement: {message}"
    );
}

#[test]
fn edge_style_parse_rejects_legacy_straight_with_migration() {
    let err = EdgeStyle::parse("straight").unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("sharp"),
        "error should name the replacement: {message}"
    );
}

#[test]
fn edge_style_parse_rejects_legacy_orthogonal_with_explanation() {
    let err = EdgeStyle::parse("orthogonal").unwrap_err();
    let message = err.to_string();
    // Message should clarify that routing topology is engine-owned and
    // point to --edge-style rounded for orthogonal-looking paths.
    assert!(
        message.contains("engine-owned") || message.contains("engine"),
        "error should explain routing ownership: {message}"
    );
    assert!(
        message.contains("rounded"),
        "error should suggest --edge-style rounded for orthogonal paths: {message}"
    );
}

#[test]
fn edge_style_parse_rejects_unknown_values() {
    for unknown in ["basis", "linear", "bogus"] {
        let err = EdgeStyle::parse(unknown).expect_err("unknown value should fail");
        let message = err.to_string();
        assert!(
            message.contains("sharp") || message.contains("smooth") || message.contains("rounded"),
            "error should list canonical edge styles: {message}"
        );
    }
}

#[test]
fn edge_style_display_uses_canonical_values() {
    assert_eq!(EdgeStyle::Sharp.to_string(), "sharp");
    assert_eq!(EdgeStyle::Smooth.to_string(), "smooth");
    assert_eq!(EdgeStyle::Rounded.to_string(), "rounded");
}
