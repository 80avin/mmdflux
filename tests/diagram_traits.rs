use mmdflux::diagram::{
    DiagramFamily, DiagramModel, EngineCapabilities, EngineConfig, GraphLayoutEngine, OutputFormat,
    RenderConfig, RenderError,
};

#[test]
fn diagram_family_variants_exist() {
    let _graph = DiagramFamily::Graph;
    let _timeline = DiagramFamily::Timeline;
    let _chart = DiagramFamily::Chart;
    let _table = DiagramFamily::Table;
}

#[test]
fn output_format_default_is_text() {
    assert_eq!(OutputFormat::default(), OutputFormat::Text);
}

struct DummyModel;
impl DiagramModel for DummyModel {
    fn clear(&mut self) {}
    fn title(&self) -> Option<&str> {
        None
    }
    fn acc_title(&self) -> Option<&str> {
        None
    }
    fn acc_description(&self) -> Option<&str> {
        None
    }
}

#[test]
fn dummy_model_compiles() {
    let mut model = DummyModel;
    model.clear();
    assert!(model.title().is_none());
    assert!(model.acc_title().is_none());
    assert!(model.acc_description().is_none());
}

// --- EngineConfig tests (Task 1.2) ---

#[test]
fn engine_config_dagre_variant_exists() {
    let dagre_cfg = mmdflux::dagre::LayoutConfig::default();
    let ec = EngineConfig::Dagre(dagre_cfg);
    assert!(matches!(ec, EngineConfig::Dagre(_)));
}

#[test]
fn render_config_layout_converts_to_engine_config_dagre() {
    let cfg: EngineConfig = RenderConfig::default().layout.into();
    assert!(matches!(cfg, EngineConfig::Dagre(_)));
}

#[test]
fn render_config_default_layout_engine_is_none() {
    let cfg = RenderConfig::default();
    assert!(cfg.layout_engine.is_none());
}

// --- EngineCapabilities tests (Task 1.3) ---

#[test]
fn engine_capabilities_default_all_false() {
    let caps = EngineCapabilities::default();
    assert!(!caps.routes_edges);
    assert!(!caps.supports_subgraphs);
    assert!(!caps.supports_direction_overrides);
}

// --- GraphLayoutEngine trait compile test (Task 1.3) ---

struct StubEngine;

impl GraphLayoutEngine for StubEngine {
    type Input = String;
    type Output = String;

    fn name(&self) -> &str {
        "stub"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::default()
    }

    fn layout(
        &self,
        input: &Self::Input,
        _config: &EngineConfig,
    ) -> Result<Self::Output, RenderError> {
        Ok(format!("laid out: {input}"))
    }
}

#[test]
fn graph_layout_engine_trait_is_implementable() {
    let engine = StubEngine;
    assert_eq!(engine.name(), "stub");
    assert!(!engine.capabilities().routes_edges);

    let cfg = EngineConfig::Dagre(mmdflux::dagre::LayoutConfig::default());
    let result = engine.layout(&"test".to_string(), &cfg).unwrap();
    assert_eq!(result, "laid out: test");
}

#[test]
fn graph_layout_engine_trait_is_object_safe() {
    let engine: Box<dyn GraphLayoutEngine<Input = String, Output = String>> = Box::new(StubEngine);
    assert_eq!(engine.name(), "stub");
}
