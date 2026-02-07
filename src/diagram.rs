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

/// Configuration for layout computation.
///
/// This is a re-export of `dagre::types::LayoutConfig` to provide a single
/// canonical layout configuration type across the crate.
pub type LayoutConfig = crate::dagre::types::LayoutConfig;

/// Typed layout engine identifier.
///
/// Strongly typed engine IDs replace raw strings for engine selection.
/// Parsing is case-insensitive: "dagre", "DAGRE", "Dagre" all resolve to `Dagre`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutEngineId {
    /// Dagre (Sugiyama) hierarchical layout.
    Dagre,
    /// ELK (Eclipse Layout Kernel) — requires `engine-elk` feature.
    Elk,
    /// COSE (Compound Spring Embedder) — not yet available.
    Cose,
}

impl LayoutEngineId {
    /// Parse a string into a typed engine ID (case-insensitive).
    ///
    /// Returns an error for unrecognized engine names.
    pub fn parse(s: &str) -> Result<Self, RenderError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dagre" => Ok(LayoutEngineId::Dagre),
            "elk" => Ok(LayoutEngineId::Elk),
            "cose" | "cose-bilkent" => Ok(LayoutEngineId::Cose),
            _ => Err(RenderError {
                message: format!("unknown layout engine: {s:?}"),
            }),
        }
    }

    /// Check whether this engine is available at runtime.
    ///
    /// Returns `Ok(())` if available, or an actionable error explaining
    /// how to enable the engine (e.g., feature flag).
    pub fn check_available(&self) -> Result<(), RenderError> {
        match self {
            LayoutEngineId::Dagre => Ok(()),
            LayoutEngineId::Elk => {
                #[cfg(feature = "engine-elk")]
                {
                    Ok(())
                }
                #[cfg(not(feature = "engine-elk"))]
                {
                    Err(RenderError {
                        message: "ELK engine is not available; rebuild with the `engine-elk` feature flag enabled".to_string(),
                    })
                }
            }
            LayoutEngineId::Cose => Err(RenderError {
                message: "COSE engine is not yet implemented".to_string(),
            }),
        }
    }
}

impl std::fmt::Display for LayoutEngineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutEngineId::Dagre => write!(f, "dagre"),
            LayoutEngineId::Elk => write!(f, "elk"),
            LayoutEngineId::Cose => write!(f, "cose"),
        }
    }
}

/// Engine-specific configuration envelope.
///
/// Wraps engine-specific layout parameters. Phase 2 supports Dagre only;
/// future engines (ELK, COSE) will add variants here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum EngineConfig {
    /// Dagre (Sugiyama) layout engine configuration.
    Dagre(crate::dagre::types::LayoutConfig),
}

impl From<LayoutConfig> for EngineConfig {
    fn from(config: LayoutConfig) -> Self {
        EngineConfig::Dagre(config)
    }
}

/// Capabilities advertised by a layout engine.
///
/// The runtime uses these to decide what post-processing is needed
/// after layout (e.g., whether edge routing is already done).
#[derive(Debug, Clone, Default)]
pub struct EngineCapabilities {
    /// Whether the engine handles edge routing (vs. leaving it to the renderer).
    pub routes_edges: bool,
    /// Whether the engine supports subgraph (cluster) layout.
    pub supports_subgraphs: bool,
    /// Whether the engine supports direction overrides per subgraph.
    pub supports_direction_overrides: bool,
}

/// Routing mode determined by engine capabilities.
///
/// Controls how the rendering pipeline processes edge paths after layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingMode {
    /// Engine provides only node positions; run full edge routing.
    FullCompute,
    /// Engine provides routed edge paths; apply clipping and spacing only.
    PassThroughClip,
}

impl RoutingMode {
    /// Determine routing mode from engine capabilities.
    pub fn for_capabilities(caps: &EngineCapabilities) -> Self {
        if caps.routes_edges {
            RoutingMode::PassThroughClip
        } else {
            RoutingMode::FullCompute
        }
    }
}

/// Synchronous graph layout engine trait.
///
/// Layout engines position nodes and edges in a graph. Each engine
/// can advertise its capabilities so the runtime knows what
/// post-processing is needed.
pub trait GraphLayoutEngine: Send + Sync {
    /// Input graph type for this engine.
    type Input;
    /// Positioned output type.
    type Output;

    /// Engine name (e.g., "dagre", "elk").
    fn name(&self) -> &str;

    /// Capabilities this engine provides.
    fn capabilities(&self) -> EngineCapabilities;

    /// Compute layout positions for the input graph.
    fn layout(
        &self,
        input: &Self::Input,
        config: &EngineConfig,
    ) -> Result<Self::Output, RenderError>;
}

/// MMDS geometry level for JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeometryLevel {
    /// Node geometry + edge topology only (no edge paths).
    #[default]
    Layout,
    /// Full geometry including routed edge paths.
    Routed,
}

/// Configuration for rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderConfig {
    /// Layout configuration.
    pub layout: LayoutConfig,
    /// Layout engine selection.
    ///
    /// - `None` => default (dagre)
    /// - `Some("dagre")` => explicit dagre
    /// - Any other value => unsupported engine error
    pub layout_engine: Option<String>,
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
    /// Show node IDs alongside labels (e.g., "A: Start").
    pub show_ids: bool,
    /// MMDS geometry level for JSON output.
    pub geometry_level: GeometryLevel,
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
