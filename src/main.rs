use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mmdflux")]
#[command(about = "Convert Mermaid diagrams to ASCII art")]
struct Cli {
    /// Input file (reads from stdin if not provided)
    input: Option<PathBuf>,

    /// Output file (prints to stdout if not provided)
    #[arg(short, long)]
    output: Option<PathBuf>,
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

    // TODO: Parse and render the diagram
    let output = render(&input);

    match &cli.output {
        Some(path) => fs::write(path, &output)?,
        None => print!("{}", output),
    }

    Ok(())
}

fn render(input: &str) -> String {
    // Placeholder - just echo input for now
    format!("Input received:\n{}", input)
}
