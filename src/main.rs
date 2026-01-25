use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use clap::Parser;
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

    let output = match render(&input, cli.debug) {
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

fn render(input: &str, debug: bool) -> Result<String, String> {
    // Parse the flowchart
    let flowchart = parse_flowchart(input).map_err(|e| e.to_string())?;

    // Build the graph
    let diagram = build_diagram(&flowchart);

    if debug {
        // Debug output: show parsed structure
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
                "  {} --{}--> {} [{:?}, {:?}]\n",
                edge.from, label, edge.to, edge.stroke, edge.arrow
            ));
        }
        Ok(output)
    } else {
        // TODO: Implement ASCII rendering
        // For now, output a summary
        Ok(format!(
            "Parsed flowchart: {} nodes, {} edges (direction: {:?})\n",
            diagram.nodes.len(),
            diagram.edges.len(),
            diagram.direction
        ))
    }
}
