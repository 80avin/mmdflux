use mmdflux::diagram::EdgeStyle;

#[test]
fn edge_style_parse_accepts_canonical_values() {
    assert_eq!(EdgeStyle::parse("curved").unwrap(), EdgeStyle::Curved);
    assert_eq!(EdgeStyle::parse("straight").unwrap(), EdgeStyle::Straight);
    assert_eq!(EdgeStyle::parse("rounded").unwrap(), EdgeStyle::Rounded);
    assert_eq!(
        EdgeStyle::parse("orthogonal").unwrap(),
        EdgeStyle::Orthogonal
    );
}

#[test]
fn edge_style_parse_rejects_legacy_values() {
    for legacy in ["basis", "linear"] {
        let err = EdgeStyle::parse(legacy).expect_err("legacy value should fail");
        let message = err.to_string();
        assert!(
            message.contains("expected one of: curved, straight, rounded, orthogonal"),
            "error should list canonical edge styles: {message}"
        );
    }
}

#[test]
fn edge_style_display_uses_canonical_values() {
    assert_eq!(EdgeStyle::Curved.to_string(), "curved");
    assert_eq!(EdgeStyle::Straight.to_string(), "straight");
    assert_eq!(EdgeStyle::Rounded.to_string(), "rounded");
    assert_eq!(EdgeStyle::Orthogonal.to_string(), "orthogonal");
}
