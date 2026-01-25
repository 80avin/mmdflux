# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

mmdflux is a Rust CLI tool and library that parses Mermaid flowchart diagrams and renders them as ASCII art. It converts Mermaid syntax into terminal-friendly visualizations with support for multiple layout directions (TD, BT, LR, RL), node shapes (rectangle, rounded, diamond), and edge styles (solid, dotted, thick).

## Common Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test --test integration  # Integration tests only
cargo test test_name           # Run specific test

# Run CLI directly
cargo run -- diagram.mmd
cargo run -- --debug diagram.mmd
cargo run -- --ascii diagram.mmd
echo 'graph LR\nA-->B' | cargo run

# Linting
cargo clippy
cargo fmt
```

## Architecture

Three-stage pipeline: **Parser → Graph → Render**

```
Mermaid Text → Parser (pest PEG) → AST → Graph Builder → Diagram → Layout → Router → Canvas → ASCII
```

### Module Structure

**`src/parser/`** - Mermaid parsing
- `grammar.pest` - PEG grammar definition (header, nodes, edges, connectors)
- `ast.rs` - AST types: `ShapeSpec`, `Vertex`, `ConnectorSpec`, `EdgeSpec`, `Statement`
- `flowchart.rs` - `parse_flowchart()` entry point, converts pest tree to AST

**`src/graph/`** - Graph data structures
- `diagram.rs` - `Diagram` struct (nodes HashMap, edges Vec, direction)
- `node.rs` - `Node` with `Shape` enum (Rectangle, Round, Diamond)
- `edge.rs` - `Edge` with `Stroke` (Solid, Dotted, Thick) and `Arrow` (Normal, None)
- `builder.rs` - `build_diagram()` converts AST to Diagram

**`src/render/`** - ASCII rendering
- `layout.rs` - `compute_layout()` does topological sort, grid positioning, backward edge corridors
- `router.rs` - `route_edge()` and `route_backward_edge()` compute paths avoiding nodes
- `edge.rs` - `render_edge()` draws edges with arrows and labels
- `shape.rs` - `render_node()` draws node shapes
- `canvas.rs` - 2D character grid
- `chars.rs` - `CharSet` for Unicode/ASCII box-drawing characters

### Key Data Flow

1. `parse_flowchart(input)` → `Flowchart` AST
2. `build_diagram(&flowchart)` → `Diagram` with nodes/edges
3. `compute_layout(&diagram, &config)` → `Layout` with positions
4. `route_edges()` → paths for each edge
5. `render()` → `Canvas` → String output

## Testing

Test fixtures live in `tests/fixtures/` with `.mmd` files covering various patterns:
- `simple.mmd`, `chain.mmd` - Basic flows
- `decision.mmd`, `shapes.mmd` - Node shapes
- `simple_cycle.mmd`, `multiple_cycles.mmd` - Backward edges
- `labeled_edges.mmd`, `edge_styles.mmd` - Edge variations
- `left_right.mmd`, `right_left.mmd`, `bottom_top.mmd` - Directions

Integration tests in `tests/integration.rs` verify parsing, building, and rendering.

## Planning and Task Tracking

Use `/plan` to create implementation plans and `/resume` to continue in-progress work.
See `plans/CLAUDE.md` for workflow details and conventions.
