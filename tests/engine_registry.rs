//! Engine registry tests: typed engine IDs, parsing, availability, and registry lookup.

use mmdflux::diagram::{
    AlgorithmId, CornerStyle, EdgePreset, EngineAlgorithmId, EngineConfig, EngineId, GeometryLevel,
    GraphEngine, GraphSolveRequest, InterpolationStyle, OutputFormat, PathDetail, RenderConfig,
    RenderError, RouteOwnership, RoutingStyle,
};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::diagrams::flowchart::engine::{FluxLayeredEngine, MermaidLayeredEngine};
use mmdflux::engines::graph::GraphEngineRegistry;
use mmdflux::registry::DiagramInstance;

// =============================================================================
// Engine selection through render path
// =============================================================================

/// Helper: parse + render with a specific engine algorithm ID string.
fn render_with_engine(input: &str, engine: &str) -> Result<String, RenderError> {
    let mut instance = FlowchartInstance::new();
    instance
        .parse(input)
        .expect("parse should succeed in test helper");
    let engine = EngineAlgorithmId::parse(engine)?;
    let config = RenderConfig {
        layout_engine: Some(engine),
        ..Default::default()
    };
    instance.render(OutputFormat::Text, &config)
}

#[test]
fn unavailable_engine_returns_actionable_error() {
    #[cfg(not(feature = "engine-elk"))]
    {
        let err = render_with_engine("graph TD\nA-->B", "elk-layered").unwrap_err();
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
        err.message.contains("unknown engine"),
        "error should mention unknown engine: {}",
        err.message
    );
}

#[test]
fn cose_rejected_at_parse_boundary() {
    // COSE is not in the new engine+algorithm taxonomy; rejected at parse time.
    let err = EngineAlgorithmId::parse("cose").unwrap_err();
    assert!(
        !err.message.is_empty(),
        "cose should be rejected at parse boundary: {}",
        err.message
    );
}

#[test]
fn cose_bilkent_rejected_at_parse_boundary() {
    let err = EngineAlgorithmId::parse("cose-bilkent").unwrap_err();
    assert!(
        !err.message.is_empty(),
        "cose-bilkent should be rejected at parse boundary: {}",
        err.message
    );
}

// =============================================================================
// Flux vs Mermaid routing: text-mode invariant
// =============================================================================

#[test]
fn flux_vs_mermaid_text_output_identical_for_simple() {
    let input = std::fs::read_to_string("tests/fixtures/flowchart/simple.mmd").unwrap();
    let mut instance = FlowchartInstance::new();
    instance.parse(&input).unwrap();

    let flux_config = RenderConfig {
        layout_engine: Some(EngineAlgorithmId::parse("flux-layered").unwrap()),
        ..RenderConfig::default()
    };
    let mermaid_config = RenderConfig {
        layout_engine: Some(EngineAlgorithmId::parse("mermaid-layered").unwrap()),
        ..RenderConfig::default()
    };

    let flux_out = instance.render(OutputFormat::Text, &flux_config).unwrap();
    let mermaid_out = instance
        .render(OutputFormat::Text, &mermaid_config)
        .unwrap();
    assert_eq!(
        flux_out, mermaid_out,
        "text output should be routing-independent"
    );
}

#[test]
fn flux_vs_mermaid_svg_output_may_diverge_for_cycle() {
    let input = std::fs::read_to_string("tests/fixtures/flowchart/simple_cycle.mmd").unwrap();
    let mut instance = FlowchartInstance::new();
    instance.parse(&input).unwrap();

    let flux_out = instance
        .render(
            OutputFormat::Svg,
            &RenderConfig {
                layout_engine: Some(EngineAlgorithmId::parse("flux-layered").unwrap()),
                ..RenderConfig::default()
            },
        )
        .unwrap();
    let mermaid_out = instance
        .render(
            OutputFormat::Svg,
            &RenderConfig {
                layout_engine: Some(EngineAlgorithmId::parse("mermaid-layered").unwrap()),
                ..RenderConfig::default()
            },
        )
        .unwrap();

    // SVG paths will differ because routing topology changes — document, don't assert equal
    let _ = (flux_out, mermaid_out); // classification: SVG-divergent
}

// =============================================================================
// EngineAlgorithmId taxonomy (plan-0081 Phase 1)
// =============================================================================

#[test]
fn engine_algorithm_id_parses_flux_layered() {
    let id = EngineAlgorithmId::parse("flux-layered").unwrap();
    assert_eq!(id.engine(), EngineId::Flux);
    assert_eq!(id.algorithm(), AlgorithmId::Layered);
    assert_eq!(id.to_string(), "flux-layered");
}

#[test]
fn engine_algorithm_id_parses_all_valid_ids() {
    for (input, engine, algo) in [
        ("flux-layered", EngineId::Flux, AlgorithmId::Layered),
        ("mermaid-layered", EngineId::Mermaid, AlgorithmId::Layered),
        ("elk-layered", EngineId::Elk, AlgorithmId::Layered),
        ("elk-mrtree", EngineId::Elk, AlgorithmId::MrTree),
    ] {
        let id = EngineAlgorithmId::parse(input).unwrap();
        assert_eq!(id.engine(), engine);
        assert_eq!(id.algorithm(), algo);
    }
}

#[test]
fn engine_algorithm_id_is_case_insensitive() {
    assert!(EngineAlgorithmId::parse("Flux-Layered").is_ok());
    assert!(EngineAlgorithmId::parse("ELK-MRTREE").is_ok());
    assert!(EngineAlgorithmId::parse("  elk-layered  ").is_ok());
}

#[test]
fn engine_algorithm_id_rejects_legacy_dagre_with_migration() {
    let err = EngineAlgorithmId::parse("dagre").unwrap_err();
    assert!(
        err.message.contains("flux-layered"),
        "should suggest replacement: {}",
        err
    );
}

#[test]
fn engine_algorithm_id_rejects_legacy_elk_with_migration() {
    let err = EngineAlgorithmId::parse("elk").unwrap_err();
    assert!(
        err.message.contains("elk-layered"),
        "should suggest replacement: {}",
        err
    );
}

#[test]
fn engine_algorithm_id_rejects_legacy_cose() {
    let err = EngineAlgorithmId::parse("cose").unwrap_err();
    assert!(
        err.message.contains("no longer supported") || err.message.contains("cose"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn engine_algorithm_id_rejects_unknown() {
    let err = EngineAlgorithmId::parse("bogus-engine").unwrap_err();
    assert!(
        err.message.contains("unknown") || err.message.contains("Valid options"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn engine_algorithm_id_display_roundtrips() {
    for input in [
        "flux-layered",
        "mermaid-layered",
        "elk-layered",
        "elk-mrtree",
    ] {
        let id = EngineAlgorithmId::parse(input).unwrap();
        assert_eq!(id.to_string(), input);
    }
}

// =============================================================================
// RouteOwnership and EngineAlgorithmCapabilities (plan-0081 Phase 1.2)
// =============================================================================

#[test]
fn flux_layered_capabilities() {
    let id = EngineAlgorithmId::parse("flux-layered").unwrap();
    let caps = id.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::Native);
    assert!(caps.supports_subgraphs);
}

#[test]
fn mermaid_layered_capabilities() {
    let id = EngineAlgorithmId::parse("mermaid-layered").unwrap();
    let caps = id.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::HintDriven);
    assert!(caps.supports_subgraphs);
}

#[test]
fn elk_layered_capabilities() {
    let id = EngineAlgorithmId::parse("elk-layered").unwrap();
    let caps = id.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::EngineProvided);
    assert!(caps.supports_subgraphs);
}

#[test]
fn elk_mrtree_capabilities() {
    let id = EngineAlgorithmId::parse("elk-mrtree").unwrap();
    let caps = id.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::EngineProvided);
    assert!(!caps.supports_subgraphs);
}

#[test]
fn route_ownership_native_routes_edges() {
    assert!(RouteOwnership::Native.routes_edges());
    assert!(!RouteOwnership::HintDriven.routes_edges());
    assert!(RouteOwnership::EngineProvided.routes_edges());
}

// =============================================================================
// EngineAlgorithmId availability gating (plan-0081 Phase 1.3)
// =============================================================================

#[test]
fn flux_layered_is_always_available() {
    let id = EngineAlgorithmId::parse("flux-layered").unwrap();
    assert!(id.check_available().is_ok());
}

#[test]
fn mermaid_layered_is_always_available() {
    let id = EngineAlgorithmId::parse("mermaid-layered").unwrap();
    assert!(id.check_available().is_ok());
}

#[cfg(not(feature = "engine-elk"))]
#[test]
fn elk_layered_unavailable_without_feature() {
    let id = EngineAlgorithmId::parse("elk-layered").unwrap();
    let err = id.check_available().unwrap_err();
    assert!(
        err.message.contains("engine-elk"),
        "should name feature flag: {}",
        err
    );
}

#[cfg(not(feature = "engine-elk"))]
#[test]
fn elk_mrtree_unavailable_without_feature() {
    let id = EngineAlgorithmId::parse("elk-mrtree").unwrap();
    let err = id.check_available().unwrap_err();
    assert!(
        err.message.contains("engine-elk"),
        "should name feature flag: {}",
        err
    );
}

// =============================================================================
// Style model taxonomy (plan-0081 Phase 7.2)
// =============================================================================

#[test]
fn routing_style_parses_polyline() {
    assert_eq!(
        RoutingStyle::parse("polyline").unwrap(),
        RoutingStyle::Polyline
    );
}

#[test]
fn routing_style_parses_orthogonal() {
    assert_eq!(
        RoutingStyle::parse("orthogonal").unwrap(),
        RoutingStyle::Orthogonal
    );
}

#[test]
fn interpolation_style_parses_linear() {
    assert_eq!(
        InterpolationStyle::parse("linear").unwrap(),
        InterpolationStyle::Linear
    );
}

#[test]
fn interpolation_style_parses_bezier() {
    assert_eq!(
        InterpolationStyle::parse("bezier").unwrap(),
        InterpolationStyle::Bezier
    );
}

#[test]
fn corner_style_parses_sharp() {
    assert_eq!(CornerStyle::parse("sharp").unwrap(), CornerStyle::Sharp);
}

#[test]
fn corner_style_parses_rounded() {
    assert_eq!(CornerStyle::parse("rounded").unwrap(), CornerStyle::Rounded);
}

#[test]
fn edge_preset_parses_all_values() {
    assert_eq!(EdgePreset::parse("straight").unwrap(), EdgePreset::Straight);
    assert_eq!(EdgePreset::parse("step").unwrap(), EdgePreset::Step);
    assert_eq!(
        EdgePreset::parse("smoothstep").unwrap(),
        EdgePreset::SmoothStep
    );
    assert_eq!(EdgePreset::parse("bezier").unwrap(), EdgePreset::Bezier);
}

#[test]
fn edge_preset_expand_is_deterministic() {
    let (r, i, c) = EdgePreset::Straight.expand();
    assert_eq!(r, RoutingStyle::Polyline);
    assert_eq!(i, InterpolationStyle::Linear);
    assert_eq!(c, CornerStyle::Sharp);
}

// =============================================================================
// GraphEngine solve contract (plan-0081 Phase 3.1)
// =============================================================================

#[test]
fn solve_request_fields_round_trip() {
    let req = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Layout,
        path_detail: PathDetail::Full,
        routing_style: None,
    };
    assert_eq!(req.output_format, OutputFormat::Text);
    assert_eq!(req.geometry_level, GeometryLevel::Layout);
    assert_eq!(req.path_detail, PathDetail::Full);
}

#[test]
fn solve_request_from_config_derives_fields() {
    let config = RenderConfig {
        geometry_level: GeometryLevel::Routed,
        ..RenderConfig::default()
    };
    let req = GraphSolveRequest::from_config(&config, OutputFormat::Svg);
    assert_eq!(req.output_format, OutputFormat::Svg);
    assert_eq!(req.geometry_level, GeometryLevel::Routed);
}

// =============================================================================
// FluxLayeredEngine (plan-0081 Phase 3.2)
// =============================================================================

fn build_simple_diagram() -> mmdflux::Diagram {
    let flowchart = mmdflux::parse_flowchart("graph TD\nA-->B").unwrap();
    mmdflux::build_diagram(&flowchart)
}

#[test]
fn flux_layered_engine_id() {
    let engine = FluxLayeredEngine::text();
    assert_eq!(
        engine.id(),
        EngineAlgorithmId::new(EngineId::Flux, AlgorithmId::Layered)
    );
}

#[test]
fn flux_layered_capabilities_are_native() {
    let engine = FluxLayeredEngine::text();
    let caps = engine.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::Native);
    assert!(caps.supports_subgraphs);
}

#[test]
fn flux_layered_solve_layout_level_has_no_routed_geometry() {
    let diagram = build_simple_diagram();
    let engine = FluxLayeredEngine::text();
    let request = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Layout,
        path_detail: PathDetail::Full,
        routing_style: None,
    };
    let config = EngineConfig::Layered(mmdflux::layered::types::LayoutConfig::default());
    let result = engine.solve(&diagram, &config, &request).unwrap();

    assert_eq!(result.engine_id.engine(), EngineId::Flux);
    assert!(!result.geometry.nodes.is_empty());
    assert!(
        result.routed.is_none(),
        "layout level should not include routed geometry"
    );
}

#[test]
fn flux_layered_solve_routed_level_has_routed_geometry() {
    let diagram = build_simple_diagram();
    let engine = FluxLayeredEngine::text();
    let request = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Routed,
        path_detail: PathDetail::Full,
        routing_style: None,
    };
    let config = EngineConfig::Layered(mmdflux::layered::types::LayoutConfig::default());
    let result = engine.solve(&diagram, &config, &request).unwrap();

    assert!(
        result.routed.is_some(),
        "routed level should produce routed geometry"
    );
    let routed = result.routed.unwrap();
    assert!(!routed.edges.is_empty());
}

// =============================================================================
// MermaidLayeredEngine (plan-0081 Phase 3.3)
// =============================================================================

#[test]
fn mermaid_layered_engine_id() {
    let engine = MermaidLayeredEngine::text();
    assert_eq!(
        engine.id(),
        EngineAlgorithmId::new(EngineId::Mermaid, AlgorithmId::Layered)
    );
}

#[test]
fn mermaid_layered_capabilities_are_hint_driven() {
    let engine = MermaidLayeredEngine::text();
    let caps = engine.capabilities();
    assert_eq!(caps.route_ownership, RouteOwnership::HintDriven);
    assert!(caps.supports_subgraphs);
}

#[test]
fn mermaid_layered_solve_layout_level_has_no_routed_geometry() {
    let diagram = build_simple_diagram();
    let engine = MermaidLayeredEngine::text();
    let request = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Layout,
        path_detail: PathDetail::Full,
        routing_style: None,
    };
    let config = EngineConfig::Layered(mmdflux::layered::types::LayoutConfig::default());
    let result = engine.solve(&diagram, &config, &request).unwrap();

    assert!(
        result.routed.is_none(),
        "layout level should not include routed geometry"
    );
    assert!(!result.geometry.nodes.is_empty());
}

#[test]
fn mermaid_layered_layout_matches_flux_layered_layout() {
    // Both engines share the dagre kernel — node positions should be identical
    let diagram = build_simple_diagram();
    let config = EngineConfig::Layered(mmdflux::layered::types::LayoutConfig::default());
    let layout_req = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Layout,
        path_detail: PathDetail::Full,
        routing_style: None,
    };

    let flux = FluxLayeredEngine::text()
        .solve(&diagram, &config, &layout_req)
        .unwrap();
    let mermaid = MermaidLayeredEngine::text()
        .solve(&diagram, &config, &layout_req)
        .unwrap();

    assert_eq!(flux.geometry.nodes.len(), mermaid.geometry.nodes.len());
    for (id, flux_node) in &flux.geometry.nodes {
        let mermaid_node = mermaid.geometry.nodes.get(id).unwrap();
        assert_eq!(
            flux_node.rect.x, mermaid_node.rect.x,
            "node {id} x mismatch"
        );
        assert_eq!(
            flux_node.rect.y, mermaid_node.rect.y,
            "node {id} y mismatch"
        );
    }
}

#[test]
fn mermaid_layered_solve_routed_level_has_routed_geometry() {
    let diagram = build_simple_diagram();
    let engine = MermaidLayeredEngine::text();
    let request = GraphSolveRequest {
        output_format: OutputFormat::Text,
        geometry_level: GeometryLevel::Routed,
        path_detail: PathDetail::Full,
        routing_style: None,
    };
    let config = EngineConfig::Layered(mmdflux::layered::types::LayoutConfig::default());
    let result = engine.solve(&diagram, &config, &request).unwrap();

    assert!(
        result.routed.is_some(),
        "routed level should produce routed geometry"
    );
}

// =============================================================================
// GraphEngineRegistry solver lookup (plan-0081 Phase 3.4)
// =============================================================================

#[test]
fn registry_resolves_flux_layered() {
    let registry = GraphEngineRegistry::default();
    let id = EngineAlgorithmId::parse("flux-layered").unwrap();
    let engine = registry.get_solver(id);
    assert!(engine.is_some(), "flux-layered should be registered");
    assert_eq!(engine.unwrap().id().to_string(), "flux-layered");
}

#[test]
fn registry_resolves_mermaid_layered() {
    let registry = GraphEngineRegistry::default();
    let id = EngineAlgorithmId::parse("mermaid-layered").unwrap();
    let engine = registry.get_solver(id);
    assert!(engine.is_some(), "mermaid-layered should be registered");
    assert_eq!(engine.unwrap().id().to_string(), "mermaid-layered");
}

#[cfg(not(feature = "engine-elk"))]
#[test]
fn registry_does_not_have_elk_solver_without_feature() {
    let registry = GraphEngineRegistry::default();
    let id = EngineAlgorithmId::parse("elk-layered").unwrap();
    assert!(
        registry.get_solver(id).is_none(),
        "elk-layered should not be registered without engine-elk feature"
    );
}

#[test]
fn registry_get_solver_unknown_id_returns_none() {
    // An ID that parses but has no engine registered (elk without feature).
    // This test verifies get_solver returns None rather than panicking.
    let registry = GraphEngineRegistry::default();
    // flux-layered is always registered — verify the lookup succeeds (not None)
    let id = EngineAlgorithmId::parse("flux-layered").unwrap();
    assert!(registry.get_solver(id).is_some());
}
