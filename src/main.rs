use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mmdflux::diagram::{
    CornerStyle, EdgePreset, EngineAlgorithmId, GeometryLevel, InterpolationStyle, LayoutConfig,
    OutputFormat, PathDetail, RenderConfig, RoutingStyle,
};
use mmdflux::layered::Ranker;
use mmdflux::registry::default_registry;

#[derive(Parser)]
#[command(name = "mmdflux")]
#[command(about = "Convert Mermaid diagrams to text, SVG, or MMDS JSON")]
struct Cli {
    /// Input file (reads from stdin if not provided)
    input: Option<PathBuf>,

    /// Output file (prints to stdout if not provided)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Show diagnostic info (detected diagram type)
    #[arg(long)]
    debug: bool,

    /// Output format (text, ascii, svg, or mmds; json is an alias)
    #[arg(short = 'f', long, value_enum, default_value_t = FormatArg::Text)]
    format: FormatArg,

    /// Ranking algorithm
    #[arg(long, value_enum, default_value_t = RankerArg::NetworkSimplex)]
    ranker: RankerArg,

    /// Layout nodesep (node spacing)
    #[arg(long)]
    node_spacing: Option<f64>,

    /// Layout ranksep (rank spacing)
    #[arg(long)]
    rank_spacing: Option<f64>,

    /// Layout edgesep (edge segment spacing)
    #[arg(long)]
    edge_spacing: Option<f64>,

    /// Layout margin (translateGraph margin)
    #[arg(long)]
    margin: Option<f64>,

    /// Extra ranksep applied when subgraphs are present (Mermaid clusters)
    #[arg(long)]
    cluster_ranksep: Option<f64>,

    /// Validate input and report diagnostics (no rendering)
    #[arg(long)]
    lint: bool,

    /// Show node IDs alongside labels (e.g., "A: Start")
    #[arg(long)]
    show_ids: bool,

    /// ASCII padding around the diagram
    #[arg(long)]
    padding: Option<usize>,

    /// SVG scale factor
    #[arg(long)]
    svg_scale: Option<f64>,

    /// SVG node padding on x-axis (px)
    #[arg(long)]
    svg_node_padding_x: Option<f64>,

    /// SVG node padding on y-axis (px)
    #[arg(long)]
    svg_node_padding_y: Option<f64>,

    /// [REMOVED] Use --edge-preset (straight, step, smoothstep, bezier) or
    /// --interpolation-style + --corner-style for low-level control.
    /// Migration: sharp→straight, smooth→bezier, rounded→smoothstep.
    #[arg(long, hide = true)]
    edge_style: Option<String>,

    /// Edge style preset (straight, step, smoothstep, or bezier).
    /// Expands to routing + interpolation + corner defaults.
    /// Explicit --routing-style / --interpolation-style / --corner-style take precedence.
    #[arg(long)]
    edge_preset: Option<String>,

    /// SVG routing style (polyline or orthogonal).
    /// Overrides the routing component of --edge-preset when both are set.
    #[arg(long)]
    routing_style: Option<String>,

    /// SVG interpolation style (linear or bezier).
    /// Overrides the interpolation component of --edge-preset when both are set.
    #[arg(long)]
    interpolation_style: Option<String>,

    /// SVG corner style (sharp or rounded).
    /// Overrides the corner component of --edge-preset when both are set.
    #[arg(long)]
    corner_style: Option<String>,

    /// SVG corner arc radius (px) for rounded corners.
    /// Clamped to half the shortest adjacent segment length.
    #[arg(long)]
    edge_radius: Option<f64>,

    /// SVG diagram padding (px)
    #[arg(long)]
    svg_diagram_padding: Option<f64>,

    /// Layout engine (flux-layered, mermaid-layered, elk-layered, elk-mrtree)
    #[arg(long)]
    layout_engine: Option<String>,

    /// MMDS geometry level for JSON output (layout or routed)
    #[arg(long, value_enum)]
    geometry_level: Option<GeometryLevelArg>,

    /// Edge path detail level for MMDS and SVG output.
    /// Ignored for text/ASCII.
    #[arg(long, value_enum)]
    path_detail: Option<PathDetailArg>,

    /// [REMOVED] Edge routing is now engine-owned. Use --layout-engine instead.
    #[arg(long, hide = true)]
    edge_routing: Option<String>,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum FormatArg {
    /// Unicode text output (default)
    Text,
    /// ASCII-only text output
    Ascii,
    /// SVG vector graphics
    Svg,
    /// MMDS structured output (`json` is an alias)
    #[value(name = "mmds", alias = "json")]
    Mmds,
    /// Mermaid syntax output (from MMDS input)
    Mermaid,
}

impl From<FormatArg> for OutputFormat {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Text => OutputFormat::Text,
            FormatArg::Ascii => OutputFormat::Ascii,
            FormatArg::Svg => OutputFormat::Svg,
            FormatArg::Mmds => OutputFormat::Mmds,
            FormatArg::Mermaid => OutputFormat::Mermaid,
        }
    }
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum RankerArg {
    NetworkSimplex,
    LongestPath,
}

impl From<RankerArg> for Ranker {
    fn from(arg: RankerArg) -> Self {
        match arg {
            RankerArg::NetworkSimplex => Ranker::NetworkSimplex,
            RankerArg::LongestPath => Ranker::LongestPath,
        }
    }
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum GeometryLevelArg {
    /// Node geometry + edge topology only (default)
    Layout,
    /// Full geometry including routed edge paths
    Routed,
}

impl From<GeometryLevelArg> for GeometryLevel {
    fn from(arg: GeometryLevelArg) -> Self {
        match arg {
            GeometryLevelArg::Layout => GeometryLevel::Layout,
            GeometryLevelArg::Routed => GeometryLevel::Routed,
        }
    }
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum PathDetailArg {
    /// All routed waypoints (default)
    Full,
    /// Remove redundant interior points while preserving path shape
    Compact,
    /// Start, midpoint, and end only
    Simplified,
    /// Start and end only
    Endpoints,
}

impl From<PathDetailArg> for PathDetail {
    fn from(arg: PathDetailArg) -> Self {
        match arg {
            PathDetailArg::Full => PathDetail::Full,
            PathDetailArg::Compact => PathDetail::Compact,
            PathDetailArg::Simplified => PathDetail::Simplified,
            PathDetailArg::Endpoints => PathDetail::Endpoints,
        }
    }
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let input = match &cli.input {
        Some(path) => fs::read_to_string(path)?,
        None => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            buffer
        }
    };

    let format: OutputFormat = cli.format.into();

    // Lint mode: validate and exit
    if cli.lint {
        let result = mmdflux::lint::lint(&input);

        if matches!(format, OutputFormat::Mmds) {
            println!("{}", result.to_json());
        } else {
            for diag in &result.errors {
                eprintln!("{}", diag);
            }
            for diag in &result.warnings {
                eprintln!("{}", diag);
            }
        }

        std::process::exit(result.exit_code());
    }

    // --edge-routing has been removed; reject it with a helpful message.
    if cli.edge_routing.is_some() {
        eprintln!(
            "Error: --edge-routing has been removed. \
             Edge routing is now determined by the layout engine. \
             Use --layout-engine flux-layered for unified routing \
             or --layout-engine mermaid-layered for legacy compute."
        );
        std::process::exit(1);
    }

    // --edge-style has been removed; reject it with a migration guide.
    if cli.edge_style.is_some() {
        eprintln!(
            "Error: --edge-style has been removed. \
             Use --edge-preset for presets or explicit style flags for low-level control.\n\
             Migration guide:\n\
               --edge-style sharp   → --edge-preset straight\n\
               --edge-style smooth  → --edge-preset bezier\n\
               --edge-style rounded → --edge-preset smoothstep\n\
             Or use --interpolation-style and --corner-style directly."
        );
        std::process::exit(1);
    }

    // Parse new style flags.
    let edge_preset: Option<EdgePreset> = match cli.edge_preset.as_deref() {
        Some(s) => match EdgePreset::parse(s) {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let routing_style: Option<RoutingStyle> = match cli.routing_style.as_deref() {
        Some(s) => match RoutingStyle::parse(s) {
            Ok(rs) => Some(rs),
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let interpolation_style: Option<InterpolationStyle> = match cli.interpolation_style.as_deref() {
        Some(s) => match InterpolationStyle::parse(s) {
            Ok(is) => Some(is),
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let corner_style: Option<CornerStyle> = match cli.corner_style.as_deref() {
        Some(s) => match CornerStyle::parse(s) {
            Ok(cs) => Some(cs),
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    // Build render config from CLI options
    let engine_algo: Option<EngineAlgorithmId> = match cli
        .layout_engine
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        Some(raw) => match EngineAlgorithmId::parse(raw) {
            Ok(id) => {
                if let Err(err) = id.check_available() {
                    eprintln!("Error: {}", err);
                    std::process::exit(1);
                }
                Some(id)
            }
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let config = RenderConfig {
        layout: LayoutConfig {
            node_sep: cli.node_spacing.unwrap_or(50.0),
            edge_sep: cli.edge_spacing.unwrap_or(20.0),
            rank_sep: cli.rank_spacing.unwrap_or(50.0),
            margin: cli.margin.unwrap_or(8.0),
            ranker: cli.ranker.into(),
            ..LayoutConfig::default()
        },
        layout_engine: engine_algo,
        cluster_ranksep: cli.cluster_ranksep,
        padding: cli.padding,
        svg_scale: cli.svg_scale,
        svg_node_padding_x: cli.svg_node_padding_x,
        svg_node_padding_y: cli.svg_node_padding_y,
        edge_preset,
        routing_style,
        interpolation_style,
        corner_style,
        edge_radius: cli.edge_radius,
        svg_diagram_padding: cli.svg_diagram_padding,
        show_ids: cli.show_ids,
        geometry_level: cli.geometry_level.map(Into::into).unwrap_or_default(),
        path_detail: cli.path_detail.map(Into::into).unwrap_or_default(),
    };

    // Use registry for detection and rendering
    let registry = default_registry();

    let diagram_id = match registry.detect(&input) {
        Some(id) => id,
        None => {
            eprintln!("Error: Unknown diagram type");
            std::process::exit(1);
        }
    };

    if cli.debug {
        eprintln!("Detected diagram type: {}", diagram_id);
    }

    let mut instance = registry.create(diagram_id).unwrap_or_else(|| {
        eprintln!("Error: No implementation for diagram type: {}", diagram_id);
        std::process::exit(1);
    });

    if let Err(e) = instance.parse(&input) {
        eprintln!("Parse error: {}", e);
        std::process::exit(1);
    }

    if !instance.supports_format(format) {
        eprintln!(
            "Error: {} diagrams do not support {} output",
            diagram_id, format
        );
        std::process::exit(1);
    }

    match instance.render(format, &config) {
        Ok(output) => match &cli.output {
            Some(path) => fs::write(path, &output)?,
            None => print!("{}", output),
        },
        Err(e) => {
            eprintln!("Render error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
