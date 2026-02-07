//! ELK layout engine integration tests.
//!
//! These tests require the `engine-elk` feature flag and an available
//! ELK subprocess runtime (`mmdflux-elk` on PATH or `MMDFLUX_ELK_CMD`).

#![cfg(feature = "engine-elk")]

use mmdflux::diagram::{LayoutEngineId, OutputFormat, RenderConfig, RenderError};
use mmdflux::diagrams::flowchart::FlowchartInstance;
use mmdflux::engines::graph::GraphEngineRegistry;
use mmdflux::registry::DiagramInstance;

#[test]
fn elk_engine_registered_with_feature() {
    let registry = GraphEngineRegistry::default();
    assert!(
        registry.is_available(LayoutEngineId::Elk),
        "ELK should be registered when engine-elk feature is enabled"
    );
}

#[test]
fn elk_engine_has_correct_name() {
    let registry = GraphEngineRegistry::default();
    let engine = registry.get(LayoutEngineId::Elk).unwrap();
    assert_eq!(engine.name(), "elk");
}

#[test]
fn elk_engine_reports_edge_routing_capability() {
    let registry = GraphEngineRegistry::default();
    let engine = registry.get(LayoutEngineId::Elk).unwrap();
    let caps = engine.capabilities();
    assert!(caps.routes_edges, "ELK should report routes_edges=true");
}

/// Helper: render via the FlowchartInstance with a specific engine.
fn render_with_engine(input: &str, engine: &str) -> Result<String, RenderError> {
    let mut instance = FlowchartInstance::new();
    instance.parse(input).expect("parse should succeed");
    let config = RenderConfig {
        layout_engine: Some(engine.to_string()),
        ..Default::default()
    };
    instance.render(OutputFormat::Text, &config)
}

#[test]
fn elk_render_returns_error_when_subprocess_missing() {
    // SAFETY: test runs single-threaded; no other thread reads this env var
    unsafe {
        std::env::set_var("MMDFLUX_ELK_CMD", "nonexistent-elk-binary-99999");
    }
    let result = render_with_engine("graph TD\nA-->B", "elk");
    unsafe {
        std::env::remove_var("MMDFLUX_ELK_CMD");
    }

    assert!(
        result.is_err(),
        "should error when ELK subprocess is missing"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("not found") || err.message.contains("ELK"),
        "error should explain subprocess issue: {}",
        err.message
    );
}
