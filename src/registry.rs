//! Diagram registry for type detection and dispatch.
//!
//! The registry holds diagram definitions and provides:
//! - Type detection from input text
//! - Factory creation of diagram instances
//! - Format support queries

use std::collections::HashMap;

use crate::diagram::{DiagramFamily, OutputFormat, RenderConfig, RenderError};

/// Detector function type.
///
/// Returns `true` if the input text matches this diagram type.
pub type DiagramDetector = fn(&str) -> bool;

/// Factory for creating diagram instances.
pub type DiagramFactory = fn() -> Box<dyn DiagramInstance>;

/// Diagram definition for registration.
///
/// Each diagram type provides a definition that describes how to
/// detect, create, and render that diagram type.
pub struct DiagramDefinition {
    /// Unique identifier (e.g., "flowchart", "pie").
    pub id: &'static str,
    /// Diagram family classification.
    pub family: DiagramFamily,
    /// Detection function that checks if input matches this type.
    pub detector: DiagramDetector,
    /// Factory for creating instances.
    pub factory: DiagramFactory,
    /// Supported output formats.
    pub supported_formats: &'static [OutputFormat],
}

/// Global diagram registry.
///
/// Holds all registered diagram types and provides detection/dispatch.
pub struct DiagramRegistry {
    diagrams: HashMap<&'static str, DiagramDefinition>,
    /// Detection order (priority) - first match wins.
    detection_order: Vec<&'static str>,
}

impl DiagramRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            diagrams: HashMap::new(),
            detection_order: Vec::new(),
        }
    }

    /// Register a diagram type.
    ///
    /// Diagrams are detected in registration order (first match wins).
    pub fn register(&mut self, definition: DiagramDefinition) {
        let id = definition.id;
        self.diagrams.insert(id, definition);
        self.detection_order.push(id);
    }

    /// Detect diagram type from input text.
    ///
    /// Returns the ID of the first registered diagram whose detector
    /// returns `true` for the input.
    #[must_use]
    pub fn detect(&self, input: &str) -> Option<&'static str> {
        for id in &self.detection_order {
            if let Some(def) = self.diagrams.get(id) {
                if (def.detector)(input) {
                    return Some(def.id);
                }
            }
        }
        None
    }

    /// Get a diagram definition by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DiagramDefinition> {
        self.diagrams.get(id)
    }

    /// List all registered diagram IDs.
    pub fn diagram_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.detection_order.iter().copied()
    }

    /// Create a new diagram instance by ID.
    ///
    /// Returns `None` if no diagram with the given ID is registered.
    #[must_use]
    pub fn create(&self, id: &str) -> Option<Box<dyn DiagramInstance>> {
        self.diagrams.get(id).map(|def| (def.factory)())
    }
}

impl Default for DiagramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Instance of a parsed diagram.
///
/// Each diagram type implements this trait to provide parsing and rendering.
pub trait DiagramInstance: Send + Sync {
    /// Parse input text into the diagram model.
    fn parse(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Render the diagram to the specified format.
    fn render(&self, format: OutputFormat, config: &RenderConfig) -> Result<String, RenderError>;

    /// Check if this instance supports the given output format.
    fn supports_format(&self, format: OutputFormat) -> bool;
}

/// Create the default registry with all built-in diagram types.
///
/// Registration order determines detection priority. Flowchart is
/// registered first as the most common diagram type.
pub fn default_registry() -> DiagramRegistry {
    let mut registry = DiagramRegistry::new();

    // Flowchart - most common, register first
    registry.register(flowchart_definition());

    // Simple diagrams (shims)
    registry.register(pie_definition());
    registry.register(info_definition());
    registry.register(packet_definition());

    registry
}

// Stub implementations until the diagrams module is complete (Phase A3)

struct StubInstance;

impl DiagramInstance for StubInstance {
    fn parse(&mut self, _: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("Stub diagram - not yet implemented".into())
    }

    fn render(&self, _: OutputFormat, _: &RenderConfig) -> Result<String, RenderError> {
        Err("Stub diagram - not yet implemented".into())
    }

    fn supports_format(&self, _: OutputFormat) -> bool {
        false
    }
}

fn flowchart_definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "flowchart",
        family: DiagramFamily::Graph,
        detector: |input| {
            let trimmed = input.trim_start();
            trimmed.starts_with("graph ")
                || trimmed.starts_with("graph\n")
                || trimmed.starts_with("flowchart ")
                || trimmed.starts_with("flowchart\n")
        },
        factory: || Box::new(StubInstance),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii, OutputFormat::Svg],
    }
}

fn pie_definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "pie",
        family: DiagramFamily::Chart,
        detector: |input| input.trim_start().starts_with("pie"),
        factory: || Box::new(StubInstance),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii],
    }
}

fn info_definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "info",
        family: DiagramFamily::Chart,
        detector: |input| input.trim_start().starts_with("info"),
        factory: || Box::new(StubInstance),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii],
    }
}

fn packet_definition() -> DiagramDefinition {
    DiagramDefinition {
        id: "packet",
        family: DiagramFamily::Table,
        detector: |input| input.trim_start().starts_with("packet-beta"),
        factory: || Box::new(StubInstance),
        supported_formats: &[OutputFormat::Text, OutputFormat::Ascii],
    }
}
