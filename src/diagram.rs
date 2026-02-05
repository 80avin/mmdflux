//! Diagram abstraction traits for multi-diagram support.
//!
//! This module defines the core traits that allow mmdflux to support
//! multiple diagram types with different parsers, layout engines, and renderers.

use std::error::Error;

/// Diagram family classification.
///
/// Families group diagram types by their layout and rendering strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramFamily {
    /// Node-edge graphs (flowchart, state, class, ER).
    Graph,
    /// Timeline-based (sequence, gantt, gitgraph).
    Timeline,
    /// Chart/visualization (pie, radar, xy).
    Chart,
    /// Tabular layout (packet, kanban).
    Table,
}

/// Output format for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Unicode text output (default).
    #[default]
    Text,
    /// ASCII-only text output.
    Ascii,
    /// SVG vector graphics.
    Svg,
    /// JSON structured output.
    Json,
}

/// SVG edge curve style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgEdgeCurve {
    Basis,
    Linear,
    Rounded,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Ascii => write!(f, "ascii"),
            OutputFormat::Svg => write!(f, "svg"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

/// Metadata common to all diagram types.
///
/// Every diagram model must implement this trait to provide
/// basic metadata and lifecycle operations.
pub trait DiagramModel: Send + Sync {
    /// Clear/reset the model state.
    fn clear(&mut self);

    /// Get the diagram title, if set.
    fn title(&self) -> Option<&str>;

    /// Get the accessibility title, if set.
    fn acc_title(&self) -> Option<&str>;

    /// Get the accessibility description, if set.
    fn acc_description(&self) -> Option<&str>;
}

/// Parser that converts text input into a diagram model.
///
/// Each diagram type provides its own parser implementation.
pub trait DiagramParser: Send + Sync {
    /// The model type this parser produces.
    type Model: DiagramModel;
    /// Error type for parse failures.
    type Error: Error + Send + Sync + 'static;

    /// Parse input text into a diagram model.
    fn parse(&self, input: &str) -> Result<Self::Model, Self::Error>;
}

/// Renderer that produces output from a diagram model.
///
/// Renderers convert a parsed diagram model into a specific output format.
pub trait DiagramRenderer: Send + Sync {
    /// The model type this renderer consumes.
    type Model: DiagramModel;

    /// Render the model to a string in the specified format.
    fn render(
        &self,
        model: &Self::Model,
        format: OutputFormat,
        config: &RenderConfig,
    ) -> Result<String, RenderError>;

    /// Check if this renderer supports the given output format.
    fn supports_format(&self, format: OutputFormat) -> bool;
}

/// Layout engine abstraction for graph-based diagrams.
///
/// Different diagram types may use different layout algorithms
/// (e.g., Dagre, ELK, timeline-based).
pub trait LayoutEngine: Send + Sync {
    /// Input type for layout computation.
    type Input;
    /// Output type containing positioned elements.
    type Output;

    /// Compute layout positions for the input.
    fn compute(&self, input: &Self::Input, config: &LayoutConfig) -> Self::Output;
}

/// Configuration for layout computation.
///
/// This is a re-export of `dagre::types::LayoutConfig` to provide a single
/// canonical layout configuration type across the crate.
pub type LayoutConfig = crate::dagre::types::LayoutConfig;

/// Configuration for rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderConfig {
    /// Layout configuration.
    pub layout: LayoutConfig,
    /// Cluster (subgraph) rank separation override.
    pub cluster_ranksep: Option<f64>,
    /// Padding around content.
    pub padding: Option<usize>,
    /// SVG-specific: scale factor.
    pub svg_scale: Option<f64>,
    /// SVG-specific: edge curve style.
    pub svg_edge_curve: Option<SvgEdgeCurve>,
    /// SVG-specific: edge curve radius (px) for rounded corners.
    pub svg_edge_curve_radius: Option<f64>,
    /// SVG-specific: diagram padding (px).
    pub svg_diagram_padding: Option<f64>,
    /// SVG-specific: node padding on x-axis (px).
    pub svg_node_padding_x: Option<f64>,
    /// SVG-specific: node padding on y-axis (px).
    pub svg_node_padding_y: Option<f64>,
}

/// Error type for rendering failures.
#[derive(Debug, Clone)]
pub struct RenderError {
    pub message: String,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for RenderError {}

impl From<String> for RenderError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for RenderError {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}
