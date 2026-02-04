use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use mmdflux::dagre::Ranker;
use mmdflux::parser::{DiagramType, detect_diagram_type, parse_info, parse_packet, parse_pie};
use mmdflux::render::{RenderOptions, render};
use mmdflux::{build_diagram, parse_flowchart};

#[derive(Parser)]
#[command(name = "mmdflux")]
#[command(about = "Convert Mermaid diagrams to ASCII art")]
struct Cli {
    /// Input file (reads from stdin if not provided)
    input: Option<PathBuf>,

    /// Output file (prints to stdout if not provided)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Show debug information (AST and graph dump)
    #[arg(long)]
    debug: bool,

    /// Use ASCII-only characters instead of Unicode box-drawing
    #[arg(long)]
    ascii: bool,

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
}

#[derive(Clone, Copy, ValueEnum)]
enum RankerArg {
    NetworkSimplex,
    LongestPath,
}

impl RankerArg {
    fn to_ranker(self) -> Ranker {
        match self {
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

    let options = RenderOptions {
        ascii_only: cli.ascii,
        ranker: Some(cli.ranker.to_ranker()),
        node_spacing: cli.node_spacing,
        rank_spacing: cli.rank_spacing,
        edge_spacing: cli.edge_spacing,
        margin: cli.margin,
        cluster_ranksep: cli.cluster_ranksep,
        padding: cli.padding,
    };

    let output = match render_input(&input, cli.debug, &options) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    match &cli.output {
        Some(path) => fs::write(path, &output)?,
        None => print!("{}", output),
    }

    Ok(())
}

fn render_input(input: &str, debug: bool, options: &RenderOptions) -> Result<String, String> {
    // Detect diagram type and dispatch
    match detect_diagram_type(input) {
        Some(DiagramType::Flowchart) => render_flowchart_diagram(input, debug, options),
        Some(DiagramType::Info) => render_info_diagram(input),
        Some(DiagramType::Pie) => render_pie_diagram(input),
        Some(DiagramType::Packet) => render_packet_diagram(input),
        None => Err("unknown diagram type".to_string()),
    }
}

fn render_flowchart_diagram(
    input: &str,
    debug: bool,
    options: &RenderOptions,
) -> Result<String, String> {
    let flowchart = parse_flowchart(input).map_err(|e| e.to_string())?;
    let diagram = build_diagram(&flowchart);

    if debug {
        let mut output = String::new();
        output.push_str(&format!("Direction: {:?}\n", diagram.direction));
        output.push_str(&format!("Nodes ({}):\n", diagram.nodes.len()));
        for (id, node) in &diagram.nodes {
            output.push_str(&format!(
                "  {} [label=\"{}\", shape={:?}]\n",
                id, node.label, node.shape
            ));
        }
        output.push_str(&format!("Edges ({}):\n", diagram.edges.len()));
        for edge in &diagram.edges {
            let label = edge
                .label
                .as_ref()
                .map(|l| format!("|{}|", l))
                .unwrap_or_default();
            output.push_str(&format!(
                "  {} --{}--> {} [{:?}, start={:?}, end={:?}]\n",
                edge.from, label, edge.to, edge.stroke, edge.arrow_start, edge.arrow_end
            ));
        }
        Ok(output)
    } else {
        Ok(render(&diagram, options))
    }
}

fn render_info_diagram(input: &str) -> Result<String, String> {
    let info = parse_info(input).map_err(|e| e.to_string())?;
    let mut output = String::new();
    if let Some(title) = &info.title {
        output.push_str(&format!("title: {}\n", title));
    }
    if info.show_info {
        output.push_str("mmdflux v0.1.0\n");
    }
    Ok(output)
}

fn render_pie_diagram(input: &str) -> Result<String, String> {
    let pie = parse_pie(input).map_err(|e| e.to_string())?;
    let mut output = String::new();
    if let Some(title) = &pie.title {
        output.push_str(&format!("title: {}\n", title));
    }
    let total: f64 = pie.sections.iter().map(|s| s.value).sum();
    for section in &pie.sections {
        let pct = if total > 0.0 {
            section.value / total * 100.0
        } else {
            0.0
        };
        output.push_str(&format!("  {}: {:.1}%\n", section.label, pct));
    }
    Ok(output)
}

fn render_packet_diagram(input: &str) -> Result<String, String> {
    let packet = parse_packet(input).map_err(|e| e.to_string())?;
    let mut output = String::new();
    if let Some(title) = &packet.title {
        output.push_str(&format!("title: {}\n", title));
    }
    for block in &packet.blocks {
        match block {
            mmdflux::parser::packet::PacketBlock::Range { start, end, label } => {
                if let Some(e) = end {
                    output.push_str(&format!("  {}-{}: {}\n", start, e, label));
                } else {
                    output.push_str(&format!("  {}: {}\n", start, label));
                }
            }
            mmdflux::parser::packet::PacketBlock::Relative { bits, label } => {
                output.push_str(&format!("  +{}: {}\n", bits, label));
            }
        }
    }
    Ok(output)
}
