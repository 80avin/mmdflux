use mmdflux::diagram::{DiagramFamily, OutputFormat, RenderConfig, RenderError};
use mmdflux::registry::{DiagramDefinition, DiagramInstance, DiagramRegistry};

struct MockDiagram {
    parsed: Option<String>,
}

impl MockDiagram {
    fn new() -> Self {
        Self { parsed: None }
    }
}

impl DiagramInstance for MockDiagram {
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.parsed = Some(input.to_string());
        Ok(())
    }

    fn render(&self, format: OutputFormat, _config: &RenderConfig) -> Result<String, RenderError> {
        let content = self.parsed.as_ref().ok_or("Not parsed")?;
        match format {
            OutputFormat::Text => Ok(format!("[TEXT] {}", content)),
            OutputFormat::Ascii => Ok(format!("[ASCII] {}", content)),
            OutputFormat::Svg | OutputFormat::Json => Err("Not supported".into()),
        }
    }

    fn supports_format(&self, format: OutputFormat) -> bool {
        matches!(format, OutputFormat::Text | OutputFormat::Ascii)
    }
}

#[test]
fn diagram_instance_parse_and_render() {
    let mut diagram = MockDiagram::new();
    diagram.parse("test input").unwrap();

    let output = diagram
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert_eq!(output, "[TEXT] test input");
}

#[test]
fn diagram_instance_supports_format() {
    let diagram = MockDiagram::new();
    assert!(diagram.supports_format(OutputFormat::Text));
    assert!(diagram.supports_format(OutputFormat::Ascii));
    assert!(!diagram.supports_format(OutputFormat::Svg));
}

#[test]
fn diagram_instance_unsupported_format_errors() {
    let mut diagram = MockDiagram::new();
    diagram.parse("test").unwrap();
    let result = diagram.render(OutputFormat::Svg, &RenderConfig::default());
    assert!(result.is_err());
}

#[test]
fn registry_create_returns_instance() {
    let mut registry = DiagramRegistry::new();

    registry.register(DiagramDefinition {
        id: "mock",
        family: DiagramFamily::Graph,
        detector: |_| true,
        factory: || Box::new(MockDiagram::new()),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii],
    });

    let mut instance = registry.create("mock").expect("should create instance");
    instance.parse("hello").unwrap();
    let output = instance
        .render(OutputFormat::Text, &RenderConfig::default())
        .unwrap();
    assert_eq!(output, "[TEXT] hello");
}

#[test]
fn registry_create_unknown_returns_none() {
    let registry = DiagramRegistry::new();
    assert!(registry.create("unknown").is_none());
}
