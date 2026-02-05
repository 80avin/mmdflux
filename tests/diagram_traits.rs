use mmdflux::diagram::{DiagramFamily, DiagramModel, OutputFormat};

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

// Compile-time verification that traits exist with expected associated types
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
