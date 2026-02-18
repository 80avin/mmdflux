use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mmdflux::dagre::Ranker;
use mmdflux::diagram::{
    EdgeRouting, EdgeRoutingPolicyToggles, EdgeStyle, GeometryLevel, LayoutConfig, LayoutEngineId,
    OutputFormat, PathDetail, RenderConfig,
};
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

    /// Dagre nodesep (node spacing)
    #[arg(long)]
    node_spacing: Option<f64>,

    /// Dagre ranksep (rank spacing)
    #[arg(long)]
    rank_spacing: Option<f64>,

    /// Dagre edgesep (edge segment spacing)
    #[arg(long)]
    edge_spacing: Option<f64>,

    /// Dagre margin (translateGraph margin)
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

    /// SVG edge style (curved, straight, rounded, or orthogonal)
    #[arg(long, value_enum)]
    edge_style: Option<EdgeStyleArg>,

    /// SVG edge radius (px) for rounded corners.
    /// Clamped to half the shortest adjacent segment length.
    #[arg(long)]
    edge_radius: Option<f64>,

    /// SVG diagram padding (px)
    #[arg(long)]
    svg_diagram_padding: Option<f64>,

    /// Layout engine (dagre, elk)
    #[arg(long)]
    layout_engine: Option<String>,

    /// MMDS geometry level for JSON output (layout or routed)
    #[arg(long, value_enum)]
    geometry_level: Option<GeometryLevelArg>,

    /// Edge path detail level for MMDS and SVG output.
    /// Ignored for text/ASCII.
    #[arg(long, value_enum)]
    path_detail: Option<PathDetailArg>,

    /// Edge routing override for routed-geometry preview.
    #[arg(long, value_enum)]
    edge_routing: Option<EdgeRoutingArg>,
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
enum EdgeStyleArg {
    Curved,
    Straight,
    Rounded,
    Orthogonal,
}

impl From<EdgeStyleArg> for EdgeStyle {
    fn from(arg: EdgeStyleArg) -> Self {
        match arg {
            EdgeStyleArg::Curved => EdgeStyle::Curved,
            EdgeStyleArg::Straight => EdgeStyle::Straight,
            EdgeStyleArg::Rounded => EdgeStyle::Rounded,
            EdgeStyleArg::Orthogonal => EdgeStyle::Orthogonal,
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

#[derive(Clone, Copy, ValueEnum, Debug)]
enum EdgeRoutingArg {
    FullCompute,
    PassThroughClip,
    UnifiedPreview,
}

impl From<EdgeRoutingArg> for EdgeRouting {
    fn from(arg: EdgeRoutingArg) -> Self {
        match arg {
            EdgeRoutingArg::FullCompute => EdgeRouting::FullCompute,
            EdgeRoutingArg::PassThroughClip => EdgeRouting::PassThroughClip,
            EdgeRoutingArg::UnifiedPreview => EdgeRouting::UnifiedPreview,
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

    // Build render config from CLI options
    let layout_engine = match cli
        .layout_engine
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        Some(raw) => match LayoutEngineId::parse(raw) {
            Ok(id) => Some(id),
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        },
        None => None,
    };

    let default_edge_routing = if cli.edge_routing.is_none()
        && matches!(
            layout_engine.unwrap_or(LayoutEngineId::Dagre),
            LayoutEngineId::Dagre
        ) {
        Some(EdgeRouting::UnifiedPreview)
    } else {
        None
    };

    let config = RenderConfig {
        edge_routing_policies: EdgeRoutingPolicyToggles,
        layout: LayoutConfig {
            node_sep: cli.node_spacing.unwrap_or(50.0),
            edge_sep: cli.edge_spacing.unwrap_or(20.0),
            rank_sep: cli.rank_spacing.unwrap_or(50.0),
            margin: cli.margin.unwrap_or(8.0),
            ranker: cli.ranker.into(),
            ..LayoutConfig::default()
        },
        layout_engine,
        cluster_ranksep: cli.cluster_ranksep,
        padding: cli.padding,
        svg_scale: cli.svg_scale,
        svg_node_padding_x: cli.svg_node_padding_x,
        svg_node_padding_y: cli.svg_node_padding_y,
        edge_style: cli.edge_style.map(Into::into),
        edge_radius: cli.edge_radius,
        svg_diagram_padding: cli.svg_diagram_padding,
        show_ids: cli.show_ids,
        geometry_level: cli.geometry_level.map(Into::into).unwrap_or_default(),
        path_detail: cli.path_detail.map(Into::into).unwrap_or_default(),
        edge_routing: cli.edge_routing.map(Into::into).or(default_edge_routing),
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
