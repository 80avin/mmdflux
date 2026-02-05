use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mmdflux::dagre::Ranker;
use mmdflux::diagram::{LayoutConfig, OutputFormat, RenderConfig};
use mmdflux::registry::default_registry;

#[derive(Parser)]
#[command(name = "mmdflux")]
#[command(about = "Convert Mermaid diagrams to text or SVG")]
struct Cli {
    /// Input file (reads from stdin if not provided)
    input: Option<PathBuf>,

    /// Output file (prints to stdout if not provided)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Show diagnostic info (detected diagram type)
    #[arg(long)]
    debug: bool,

    /// Output format (text, ascii, or svg)
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
}

#[derive(Clone, Copy, ValueEnum, Debug)]
enum FormatArg {
    /// Unicode text output (default)
    Text,
    /// ASCII-only text output
    Ascii,
    /// SVG vector graphics
    Svg,
}

impl From<FormatArg> for OutputFormat {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Text => OutputFormat::Text,
            FormatArg::Ascii => OutputFormat::Ascii,
            FormatArg::Svg => OutputFormat::Svg,
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

    // Build render config from CLI options
    let config = RenderConfig {
        layout: LayoutConfig {
            direction: Default::default(),
            node_sep: cli.node_spacing.unwrap_or(50.0),
            edge_sep: cli.edge_spacing.unwrap_or(20.0),
            rank_sep: cli.rank_spacing.unwrap_or(50.0),
            margin: cli.margin.unwrap_or(8.0),
            acyclic: true,
            ranker: cli.ranker.into(),
        },
        cluster_ranksep: cli.cluster_ranksep,
        padding: cli.padding,
        svg_scale: cli.svg_scale,
        svg_node_padding_x: cli.svg_node_padding_x,
        svg_node_padding_y: cli.svg_node_padding_y,
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
