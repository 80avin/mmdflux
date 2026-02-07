//! Engine registry tests: typed engine IDs, parsing, availability, and registry lookup.

use mmdflux::diagram::{
    EngineCapabilities, LayoutEngineId, OutputFormat, RenderConfig, RenderError, RoutingMode,
};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::engines::graph::GraphEngineRegistry;
use mmdflux::registry::DiagramInstance;

// =============================================================================
// LayoutEngineId parsing
// =============================================================================

#[test]
fn engine_id_parsing_accepts_dagre_and_elk() {
    assert_eq!(
        LayoutEngineId::parse("dagre").unwrap(),
        LayoutEngineId::Dagre
    );
    assert_eq!(LayoutEngineId::parse("elk").unwrap(), LayoutEngineId::Elk);
}

#[test]
fn engine_id_parsing_accepts_cose_variants() {
    assert_eq!(LayoutEngineId::parse("cose").unwrap(), LayoutEngineId::Cose);
    assert_eq!(
        LayoutEngineId::parse("cose-bilkent").unwrap(),
        LayoutEngineId::Cose
    );
}

#[test]
fn engine_id_parsing_is_case_insensitive() {
    assert_eq!(
        LayoutEngineId::parse("DAGRE").unwrap(),
        LayoutEngineId::Dagre
    );
    assert_eq!(
        LayoutEngineId::parse("Dagre").unwrap(),
        LayoutEngineId::Dagre
    );
    assert_eq!(LayoutEngineId::parse("ELK").unwrap(), LayoutEngineId::Elk);
    assert_eq!(LayoutEngineId::parse("Elk").unwrap(), LayoutEngineId::Elk);
    assert_eq!(LayoutEngineId::parse("COSE").unwrap(), LayoutEngineId::Cose);
}

#[test]
fn engine_id_parsing_rejects_unknown() {
    let err = LayoutEngineId::parse("unknown").unwrap_err();
    assert!(
        err.message.contains("unknown layout engine"),
        "error should mention unknown: {}",
        err.message
    );
}

#[test]
fn engine_id_parsing_trims_whitespace() {
    assert_eq!(
        LayoutEngineId::parse("  dagre  ").unwrap(),
        LayoutEngineId::Dagre
    );
}

#[test]
fn engine_id_display() {
    assert_eq!(LayoutEngineId::Dagre.to_string(), "dagre");
    assert_eq!(LayoutEngineId::Elk.to_string(), "elk");
    assert_eq!(LayoutEngineId::Cose.to_string(), "cose");
}

// =============================================================================
// GraphEngineRegistry
// =============================================================================

#[test]
fn graph_engine_registry_resolves_dagre() {
    let registry = GraphEngineRegistry::default();
    assert!(
        registry.get(LayoutEngineId::Dagre).is_some(),
        "dagre should be registered by default"
    );
}

#[test]
fn graph_engine_registry_dagre_has_correct_name() {
    let registry = GraphEngineRegistry::default();
    let engine = registry.get(LayoutEngineId::Dagre).unwrap();
    assert_eq!(engine.name(), "dagre");
}

#[test]
fn graph_engine_registry_elk_not_registered_without_feature() {
    #[cfg(not(feature = "engine-elk"))]
    {
        let registry = GraphEngineRegistry::default();
        assert!(
            registry.get(LayoutEngineId::Elk).is_none(),
            "ELK should not be registered without engine-elk feature"
        );
    }
}

#[test]
fn graph_engine_registry_cose_not_registered() {
    let registry = GraphEngineRegistry::default();
    assert!(
        registry.get(LayoutEngineId::Cose).is_none(),
        "COSE should not be registered (not yet implemented)"
    );
}

#[test]
fn graph_engine_registry_is_available() {
    let registry = GraphEngineRegistry::default();
    assert!(registry.is_available(LayoutEngineId::Dagre));
    #[cfg(not(feature = "engine-elk"))]
    assert!(!registry.is_available(LayoutEngineId::Elk));
    assert!(!registry.is_available(LayoutEngineId::Cose));
}

// =============================================================================
// Engine availability
// =============================================================================

#[test]
fn dagre_engine_is_always_available() {
    assert!(LayoutEngineId::Dagre.check_available().is_ok());
}

#[test]
fn elk_engine_unavailable_without_feature() {
    // Without engine-elk feature, ELK should be unavailable with actionable error
    #[cfg(not(feature = "engine-elk"))]
    {
        let err = LayoutEngineId::Elk.check_available().unwrap_err();
        assert!(
            err.message.contains("engine-elk"),
            "error should mention feature flag: {}",
            err.message
        );
    }
}

#[test]
fn cose_engine_not_yet_implemented() {
    let err = LayoutEngineId::Cose.check_available().unwrap_err();
    assert!(
        err.message.contains("not yet implemented"),
        "error should explain COSE is not implemented: {}",
        err.message
    );
}

// =============================================================================
// Engine selection through render path
// =============================================================================

/// Helper: parse + render with a specific engine name.
fn render_with_engine(input: &str, engine: &str) -> Result<String, RenderError> {
    let mut instance = FlowchartInstance::new();
    instance
        .parse(input)
        .expect("parse should succeed in test helper");
    let config = RenderConfig {
        layout_engine: Some(engine.to_string()),
        ..Default::default()
    };
    instance.render(OutputFormat::Text, &config)
}

#[test]
fn unavailable_engine_returns_actionable_error() {
    #[cfg(not(feature = "engine-elk"))]
    {
        let err = render_with_engine("graph TD\nA-->B", "elk").unwrap_err();
        assert!(
            err.message.contains("engine-elk"),
            "error should reference feature flag: {}",
            err.message
        );
    }
}

#[test]
fn unknown_engine_returns_error() {
    let err = render_with_engine("graph TD\nA-->B", "nonexistent").unwrap_err();
    assert!(
        err.message.contains("unknown layout engine"),
        "error should mention unknown engine: {}",
        err.message
    );
}

// =============================================================================
// Routing mode from capabilities
// =============================================================================

#[test]
fn position_only_engine_uses_full_compute() {
    let caps = EngineCapabilities {
        routes_edges: false,
        ..Default::default()
    };
    assert_eq!(
        RoutingMode::for_capabilities(&caps),
        RoutingMode::FullCompute
    );
}

#[test]
fn routed_engine_uses_pass_through_clip() {
    let caps = EngineCapabilities {
        routes_edges: true,
        ..Default::default()
    };
    assert_eq!(
        RoutingMode::for_capabilities(&caps),
        RoutingMode::PassThroughClip
    );
}

#[test]
fn dagre_routing_mode_is_full_compute() {
    let registry = GraphEngineRegistry::default();
    let engine = registry.get(LayoutEngineId::Dagre).unwrap();
    assert_eq!(
        RoutingMode::for_capabilities(&engine.capabilities()),
        RoutingMode::FullCompute
    );
}

#[test]
fn cose_render_path_returns_not_implemented() {
    let err = render_with_engine("graph TD\nA-->B", "cose").unwrap_err();
    assert!(
        err.message.contains("not yet implemented"),
        "COSE should return not-implemented error: {}",
        err.message
    );
}

#[test]
fn cose_bilkent_alias_also_returns_not_implemented() {
    let err = render_with_engine("graph TD\nA-->B", "cose-bilkent").unwrap_err();
    assert!(
        err.message.contains("not yet implemented"),
        "COSE-Bilkent should return not-implemented error: {}",
        err.message
    );
}

#[cfg(feature = "engine-elk")]
#[test]
fn elk_routing_mode_is_pass_through_clip() {
    let registry = GraphEngineRegistry::default();
    let engine = registry.get(LayoutEngineId::Elk).unwrap();
    assert_eq!(
        RoutingMode::for_capabilities(&engine.capabilities()),
        RoutingMode::PassThroughClip
    );
}
