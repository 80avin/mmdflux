//! Diagram abstraction traits for multi-diagram support.
//!
//! This module defines the core traits that allow mmdflux to support
//! multiple diagram types with different parsers, layout engines, and renderers.

use std::error::Error;
use std::str::FromStr;

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
    /// MMDS structured JSON output.
    Mmds,
    /// Mermaid syntax output (from MMDS input).
    Mermaid,
}

/// SVG edge path style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgEdgePathStyle {
    Basis,
    Linear,
    Rounded,
    Orthogonal,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Ascii => write!(f, "ascii"),
            OutputFormat::Svg => write!(f, "svg"),
            OutputFormat::Mmds => write!(f, "mmds"),
            OutputFormat::Mermaid => write!(f, "mermaid"),
        }
    }
}

impl OutputFormat {
    /// Parse output format from user-provided text.
    ///
    /// Accepts `json` as an alias for `mmds`.
    pub fn parse(s: &str) -> Result<Self, RenderError> {
        match normalize_enum_token(s).as_str() {
            "text" => Ok(OutputFormat::Text),
            "ascii" => Ok(OutputFormat::Ascii),
            "svg" => Ok(OutputFormat::Svg),
            "mmds" | "json" => Ok(OutputFormat::Mmds),
            "mermaid" => Ok(OutputFormat::Mermaid),
            _ => Err(RenderError {
                message: format!("unknown output format: {s:?}"),
            }),
        }
    }
}

impl FromStr for OutputFormat {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        OutputFormat::parse(s)
    }
}

impl std::fmt::Display for SvgEdgePathStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SvgEdgePathStyle::Basis => write!(f, "basis"),
            SvgEdgePathStyle::Linear => write!(f, "linear"),
            SvgEdgePathStyle::Rounded => write!(f, "rounded"),
            SvgEdgePathStyle::Orthogonal => write!(f, "orthogonal"),
        }
    }
}

impl SvgEdgePathStyle {
    /// Parse SVG edge path style from user-provided text.
    pub fn parse(s: &str) -> Result<Self, RenderError> {
        match normalize_enum_token(s).as_str() {
            "basis" => Ok(SvgEdgePathStyle::Basis),
            "linear" => Ok(SvgEdgePathStyle::Linear),
            "rounded" => Ok(SvgEdgePathStyle::Rounded),
            "orthogonal" => Ok(SvgEdgePathStyle::Orthogonal),
            _ => Err(RenderError {
                message: format!("unknown svg edge path style: {s:?}"),
            }),
        }
    }
}

impl FromStr for SvgEdgePathStyle {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SvgEdgePathStyle::parse(s)
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

impl FromStr for LayoutEngineId {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LayoutEngineId::parse(s)
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
    /// Preview float-first unified routing with guarded fallback behavior.
    UnifiedPreview,
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

/// Per-policy routing toggles for staged unified-routing rollout.
///
/// Defaults are conservative: keep fan-in overflow and label revalidation
/// enabled, while long-skip periphery detours start disabled until explicitly
/// promoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutingPolicyToggles {
    pub fan_in_face_overflow: bool,
    pub label_anchor_revalidation: bool,
    pub long_skip_periphery_detour: bool,
}

impl RoutingPolicyToggles {
    pub const fn all_enabled() -> Self {
        Self {
            fan_in_face_overflow: true,
            label_anchor_revalidation: true,
            long_skip_periphery_detour: true,
        }
    }
}

impl Default for RoutingPolicyToggles {
    fn default() -> Self {
        Self {
            fan_in_face_overflow: true,
            label_anchor_revalidation: true,
            long_skip_periphery_detour: false,
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

/// Path detail level for edge waypoints in MMDS and SVG output.
///
/// Controls how many anchor points are included in edge paths.
/// Ignored for text/ASCII output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathDetail {
    /// All routed waypoints (default).
    #[default]
    Full,
    /// Remove redundant points while preserving path shape.
    Compact,
    /// Start, midpoint, and end only (3 points).
    Simplified,
    /// Start and end only (2 points).
    Endpoints,
}

impl std::fmt::Display for PathDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathDetail::Full => write!(f, "full"),
            PathDetail::Compact => write!(f, "compact"),
            PathDetail::Simplified => write!(f, "simplified"),
            PathDetail::Endpoints => write!(f, "endpoints"),
        }
    }
}

impl PathDetail {
    /// Parse path detail level from user-provided text.
    pub fn parse(s: &str) -> Result<Self, RenderError> {
        match normalize_enum_token(s).as_str() {
            "full" => Ok(PathDetail::Full),
            "compact" => Ok(PathDetail::Compact),
            "simplified" => Ok(PathDetail::Simplified),
            "endpoints" => Ok(PathDetail::Endpoints),
            _ => Err(RenderError {
                message: format!("unknown path detail: {s:?}"),
            }),
        }
    }
}

impl FromStr for PathDetail {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PathDetail::parse(s)
    }
}

impl PathDetail {
    /// Simplify a path according to the detail level.
    ///
    /// Returns a new vec with the appropriate number of points:
    /// - `Full` — all points unchanged
    /// - `Simplified` — first, middle, last (3 points max)
    /// - `Endpoints` — first and last only (2 points max)
    pub fn simplify<T: Clone>(&self, points: &[T]) -> Vec<T> {
        match self {
            PathDetail::Full => points.to_vec(),
            PathDetail::Compact => points.to_vec(),
            PathDetail::Simplified if points.len() > 3 => {
                let mid = points.len() / 2;
                vec![
                    points[0].clone(),
                    points[mid].clone(),
                    points[points.len() - 1].clone(),
                ]
            }
            PathDetail::Endpoints if points.len() > 2 => {
                vec![points[0].clone(), points[points.len() - 1].clone()]
            }
            _ => points.to_vec(),
        }
    }

    /// Simplify path points with coordinate-aware compacting.
    ///
    /// - `Compact` removes consecutive duplicates and strictly collinear
    ///   interior points while preserving overall shape.
    /// - Other variants behave the same as `simplify`.
    pub fn simplify_with_coords<T: Clone>(
        &self,
        points: &[T],
        coords: impl Fn(&T) -> (f64, f64),
    ) -> Vec<T> {
        match self {
            PathDetail::Compact => compact_points(points, coords),
            _ => self.simplify(points),
        }
    }
}

fn compact_points<T: Clone>(points: &[T], coords: impl Fn(&T) -> (f64, f64)) -> Vec<T> {
    const EPS: f64 = 1e-6;

    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut deduped = Vec::with_capacity(points.len());
    for point in points {
        let keep = deduped.last().is_none_or(|prev: &T| {
            let (px, py) = coords(prev);
            let (x, y) = coords(point);
            (px - x).abs() > EPS || (py - y).abs() > EPS
        });
        if keep {
            deduped.push(point.clone());
        }
    }

    if deduped.len() <= 2 {
        return deduped;
    }

    let mut result = Vec::with_capacity(deduped.len());
    result.push(deduped[0].clone());
    for idx in 1..(deduped.len() - 1) {
        let prev = result.last().expect("result has first element");
        let curr = &deduped[idx];
        let next = &deduped[idx + 1];

        let (px, py) = coords(prev);
        let (cx, cy) = coords(curr);
        let (nx, ny) = coords(next);

        let dx1 = cx - px;
        let dy1 = cy - py;
        let dx2 = nx - cx;
        let dy2 = ny - cy;
        let cross = dx1 * dy2 - dy1 * dx2;
        let dot = dx1 * dx2 + dy1 * dy2;
        let collinear_same_direction = cross.abs() <= EPS && dot >= -EPS;

        if !collinear_same_direction {
            result.push(curr.clone());
        }
    }
    result.push(deduped[deduped.len() - 1].clone());
    result
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

impl std::fmt::Display for GeometryLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryLevel::Layout => write!(f, "layout"),
            GeometryLevel::Routed => write!(f, "routed"),
        }
    }
}

impl GeometryLevel {
    /// Parse MMDS geometry level from user-provided text.
    pub fn parse(s: &str) -> Result<Self, RenderError> {
        match normalize_enum_token(s).as_str() {
            "layout" => Ok(GeometryLevel::Layout),
            "routed" => Ok(GeometryLevel::Routed),
            _ => Err(RenderError {
                message: format!("unknown geometry level: {s:?}"),
            }),
        }
    }
}

impl FromStr for GeometryLevel {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        GeometryLevel::parse(s)
    }
}

/// Configuration for rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderConfig {
    /// Layout configuration.
    pub layout: LayoutConfig,
    /// Layout engine selection.
    ///
    /// - `None` => default (dagre)
    /// - `Some(LayoutEngineId::Dagre)` => explicit dagre
    pub layout_engine: Option<LayoutEngineId>,
    /// Cluster (subgraph) rank separation override.
    pub cluster_ranksep: Option<f64>,
    /// Padding around content.
    pub padding: Option<usize>,
    /// SVG-specific: scale factor.
    pub svg_scale: Option<f64>,
    /// SVG-specific: edge path style.
    pub svg_edge_path_style: Option<SvgEdgePathStyle>,
    /// SVG-specific: edge path radius (px) for rounded corners.
    pub svg_edge_path_radius: Option<f64>,
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
    /// Path detail level for edge waypoints (MMDS and SVG).
    pub path_detail: PathDetail,
    /// Optional routing mode override for routed-geometry preview/testing.
    pub routing_mode: Option<RoutingMode>,
    /// Policy toggles for staged unified-routing rollout.
    pub routing_policies: RoutingPolicyToggles,
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

fn normalize_enum_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}
