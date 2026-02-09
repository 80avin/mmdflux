# Library Usage

`mmdflux` can be used as a Rust library for parsing Mermaid and working with graph structures.

## Parse and Build

```rust
use mmdflux::{parse_flowchart, build_diagram};

fn main() {
    let input = r#"graph LR
A[Hello] --> B[World]
"#;

    // Parse Mermaid syntax into AST
    let flowchart = parse_flowchart(input).unwrap();

    // Build graph structure
    let diagram = build_diagram(&flowchart);

    println!("Direction: {:?}", diagram.direction);
    println!("Nodes: {}", diagram.nodes.len());
    println!("Edges: {}", diagram.edges.len());

    if let Some(node) = diagram.nodes.get("A") {
        println!("Node A: {} ({:?})", node.label, node.shape);
    }

    for edge in &diagram.edges {
        println!("{} -> {}", edge.from, edge.to);
    }
}
```

## MMDS to Mermaid

```rust
use mmdflux::generate_mermaid_from_mmds_str;

fn main() {
    let mmds_json = std::fs::read_to_string("diagram.mmds.json").unwrap();
    let mermaid = generate_mermaid_from_mmds_str(&mmds_json).unwrap();
    println!("{mermaid}");
}
```

## Common Types

```rust
use mmdflux::{Diagram, Direction, Node, Shape, Edge};
use mmdflux::graph::{Stroke, Arrow};

// Direction: TopDown, BottomTop, LeftRight, RightLeft
// Shape: Rectangle, Round, Stadium, Subroutine, Cylinder, Diamond, Hexagon, ...
// Stroke: Solid, Dotted, Thick, Invisible
// Arrow: Normal, None, Cross, Circle
```
