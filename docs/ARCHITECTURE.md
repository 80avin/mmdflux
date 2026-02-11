# Architecture

This document describes the internal architecture of mmdflux for developers who want to understand, modify, or extend the codebase.

## Overview

mmdflux converts Mermaid diagram syntax into text (Unicode/ASCII), SVG, or structured JSON (MMDS). It supports multiple diagram types through a registry-based architecture with pluggable layout engines.

```
Input Text → Registry (detect type) → Diagram Instance → Parse → Layout → Route → Render → Output
```

The codebase is organized around **diagram families** — groups of diagram types that share layout and rendering strategies:

| Family   | Diagram Types    | Pipeline                                       |
| -------- | ---------------- | ---------------------------------------------- |
| Graph    | flowchart, class | Parser → Graph → Engine → Geometry IR → Render |
| Timeline | sequence         | Parser → Model → Layout → Render               |
| Chart    | pie              | Parser → Render                                |
| Table    | packet           | Parser → Render                                |
| —        | info             | Parser → Render                                |

## Module Structure

```
src/
├── main.rs              CLI entry point
├── lib.rs               Library API
├── diagram.rs           Core traits (DiagramModel, DiagramParser, DiagramRenderer,
│                          GraphLayoutEngine), enums (OutputFormat, DiagramFamily,
│                          LayoutEngineId, GeometryLevel, PathDetail), RenderConfig
├── registry.rs          DiagramRegistry — type detection and dispatch
├── mmds.rs              MMDS JSON serialization/deserialization
├── lint.rs              Diagram linting
│
├── parser/              Mermaid parsing (PEG-based)
│   ├── grammar.pest     Flowchart PEG grammar
│   ├── ast.rs           Flowchart AST types
│   ├── flowchart.rs     Flowchart parser
│   ├── error.rs         Parser error types
│   ├── info.rs          Info diagram parser
│   ├── info_grammar.pest
│   ├── pie.rs           Pie chart parser
│   ├── pie_grammar.pest
│   ├── packet.rs        Packet diagram parser
│   └── packet_grammar.pest
│
├── graph/               Graph data structures (shared by Graph-family diagrams)
│   ├── diagram.rs       Diagram struct (nodes, edges, subgraphs, direction)
│   ├── node.rs          Node with Shape enum
│   ├── edge.rs          Edge with Stroke and Arrow
│   └── builder.rs       Flowchart AST → Diagram conversion
│
├── dagre/               Sugiyama hierarchical layout engine (~dagre v0.8.5 parity)
│   ├── mod.rs           layout() entry point, phase orchestration
│   ├── types.rs         LayoutConfig, LayoutResult, Rect, Point, Direction
│   ├── graph.rs         DiGraph input, LayoutGraph internal representation
│   ├── acyclic.rs       Cycle removal (DFS)
│   ├── rank.rs          Layer assignment (longest-path)
│   ├── network_simplex.rs  Network simplex ranking
│   ├── normalize.rs     Long edge normalization (dummy nodes)
│   ├── order.rs         Crossing reduction (barycenter heuristic)
│   ├── position.rs      Coordinate assignment orchestration
│   ├── bk.rs            Brandes-Köpf horizontal coordinate assignment
│   ├── nesting.rs       Compound graph nesting (subgraph hierarchy)
│   ├── border.rs        Border node handling for subgraphs
│   └── parent_dummy_chains.rs  Parent dummy chain handling
│
├── engines/             Layout engine abstraction
│   └── graph/
│       ├── registry.rs  GraphEngineRegistry (maps LayoutEngineId → engine)
│       ├── elk.rs       ELK subprocess adapter (feature-gated: engine-elk)
│       └── cose.rs      COSE stub (not yet implemented)
│
├── render/              Shared rendering primitives
│   ├── canvas.rs        2D character grid
│   ├── chars.rs         CharSet for box-drawing (Unicode/ASCII)
│   └── intersect.rs     Line intersection utilities
│
└── diagrams/            Diagram type implementations
    ├── flowchart/
    │   ├── mod.rs        Definition + detect function
    │   ├── instance.rs   FlowchartDiagram (DiagramInstance impl)
    │   ├── engine.rs     DagreLayoutEngine adapter (Diagram → GraphGeometry)
    │   ├── geometry.rs   GraphGeometry IR types (Layer 1 + Layer 2)
    │   ├── routing.rs    route_graph_geometry() — Layer 1 → Layer 2
    │   └── render/
    │       ├── layout.rs       Text layout computation
    │       ├── router.rs       Text edge routing
    │       ├── route_policy.rs Routing policies
    │       ├── edge.rs         Text edge rendering
    │       ├── shape.rs        Text node shape rendering
    │       ├── subgraph.rs     Subgraph border rendering
    │       ├── svg.rs          SVG output renderer
    │       ├── svg_router.rs   SVG edge routing
    │       └── svg_metrics.rs  SVG text metrics
    ├── class/
    │   ├── mod.rs        Definition + detect function
    │   ├── instance.rs   ClassDiagram implementation
    │   ├── compiler.rs   Class AST → graph::Diagram compiler
    │   └── parser/       Hand-written line parser + AST
    ├── sequence/
    │   ├── mod.rs        Definition + detect function
    │   ├── instance.rs   SequenceDiagram implementation
    │   ├── model.rs      Timeline model types
    │   ├── compiler.rs   Sequence AST → Model compiler
    │   ├── layout.rs     Timeline layout engine
    │   ├── parser/       Hand-written line parser + AST
    │   └── render/       Text renderer
    ├── mmds/
    │   ├── mod.rs        Definition
    │   ├── instance.rs   MMDS → render pipeline
    │   └── hydrate.rs    MMDS JSON → GraphGeometry hydration
    ├── info.rs           Info diagram (stub)
    ├── pie.rs            Pie chart (stub)
    └── packet.rs         Packet diagram (stub)
```

## Diagram Registry

The `DiagramRegistry` (`src/registry.rs`) is the central dispatch mechanism. Each diagram type registers a `DiagramDefinition` containing:

- **id** — unique name (e.g., `"flowchart"`, `"class"`)
- **family** — `DiagramFamily` classification
- **detector** — function that checks if input text matches this type
- **factory** — creates a `Box<dyn DiagramInstance>`
- **supported_formats** — which output formats this type can produce

Detection runs in registration order; first match wins. The registry is populated at startup in `lib.rs`.

## Graph-Family Pipeline (Flowchart, Class)

Graph-family diagrams share the most complex pipeline, built around two geometry IR layers:

```
           ┌──────────────┐
           │ Mermaid Text │
           └──────────────┘
                   │
                   ▼
              ┌────────┐
              │ Parser │
              └────────┘
                   │
                   ▼
              ┌─────────┐
              │ Diagram │
              └─────────┘
                   │
                   ▼
           ┌───────────────┐
           │ Layout Engine │
           └───────────────┘
                   │
                   ▼
      ┌─────────────────────────┐
      │ GraphGeometry (Layer 1) │
      └─────────────────────────┘
                   │
                   ▼
   ┌───────────────────────────────┐
   │ RoutedGraphGeometry (Layer 2) │
   └───────────────────────────────┘
    └───┐                     ┌───┘
        │                     │
        ▼                     ▼
┌───────────────┐     ┌──────────────┐
│ Text Renderer │     │ SVG Renderer │
└───────────────┘     └──────────────┘
```

### Parsing

Flowcharts use a **pest PEG grammar** (`src/parser/grammar.pest`). Class and sequence diagrams use **hand-written line parsers** in their respective `diagrams/*/parser/` directories.

The flowchart grammar supports: header declarations, 20+ node shapes, edge types (`-->`, `-.->`, `==>`, `---`), edge labels, chains, ampersand groups, subgraphs, comments, and various directives (style, classDef, click, linkStyle, direction) which are parsed and discarded.

### Graph Building

The graph builder (`src/graph/builder.rs`) converts the flowchart AST into a `Diagram`:
- Deduplicates nodes (same ID referenced multiple times)
- Merges node attributes from different statements
- Expands chains (`A --> B --> C` → two edges)
- Expands ampersand groups (`A & B --> C` → two edges)

Class diagrams have their own compiler (`diagrams/class/compiler.rs`) that produces the same `Diagram` type.

### Layout Engine

The `GraphLayoutEngine` trait (`src/diagram.rs`) defines the engine interface:

```rust
trait GraphLayoutEngine {
    type Input;   // Diagram
    type Output;  // GraphGeometry
    fn layout(&self, input: &Self::Input, config: &EngineConfig) -> Result<Self::Output, RenderError>;
    fn capabilities(&self) -> EngineCapabilities;
}
```

The `GraphEngineRegistry` maps `LayoutEngineId` (Dagre, Elk, Cose) to concrete engine adapters. The default engine is **Dagre**.

**Dagre** (`src/dagre/`) implements the Sugiyama framework targeting parity with dagre.js v0.8.5:

1. **Nesting** — compound graph hierarchy for subgraphs
2. **Acyclic** — cycle removal via DFS, tracks reversed edges
3. **Rank** — layer assignment (longest-path or network simplex)
4. **Normalize** — long edge splitting with dummy nodes
5. **Border** — border node creation for subgraph boundaries
6. **Parent dummy chains** — maintain hierarchy through dummy nodes
7. **Order** — crossing reduction via barycenter heuristic
8. **Position** — Brandes-Köpf coordinate assignment
9. **Denormalize** — remove dummy nodes, reconstruct edge paths

The dagre adapter (`diagrams/flowchart/engine.rs`) wraps this as a `DagreLayoutEngine` that maps `Diagram` → `GraphGeometry`.

**ELK** (behind `engine-elk` feature flag) runs the Eclipse Layout Kernel as a subprocess.

### Geometry IR

**Layer 1: GraphGeometry** (`diagrams/flowchart/geometry.rs`) — engine-agnostic layout output:
- Node positions and dimensions (`FRect`)
- Edge topology with layout hints (waypoints from engine)
- Subgraph boundaries
- Reversed edge tracking
- Metadata (graph bounds, direction)

**Layer 2: RoutedGraphGeometry** — produced by `routing::route_graph_geometry()`:
- Resolved edge polyline paths (`Vec<FPoint>`)
- Label positions
- Backward-edge markers
- Subgraph bounds with titles

The routing stage (`diagrams/flowchart/routing.rs`) supports two modes based on engine capabilities:
- **FullCompute** — build paths from layout hints and node positions (used by Dagre)
- **PassThroughClip** — use engine-provided paths directly (used by ELK)

### Rendering

Text rendering converts RoutedGraphGeometry to a character grid:
1. Map float coordinates to canvas grid positions
2. Draw node shapes with labels (Unicode box-drawing or ASCII)
3. Draw routed edge paths with arrows
4. Draw edge labels and subgraph borders

SVG rendering (`diagrams/flowchart/render/svg.rs`) consumes the same geometry IR but emits SVG elements with presentation attributes. It has its own edge routing (`svg_router.rs`) and text metrics (`svg_metrics.rs`).

## Timeline-Family Pipeline (Sequence)

Sequence diagrams use an independent pipeline:

```
┌──────────────┐
│ Mermaid Text │
└──────────────┘
        │
        ▼
   ┌────────┐
   │ Parser │
   └────────┘
        │
        ▼
     ┌─────┐
     │ AST │
     └─────┘
        │
        ▼
  ┌──────────┐
  │ Compiler │
  └──────────┘
        │
        ▼
┌───────────────┐
│ SequenceModel │
└───────────────┘
        │
        ▼
   ┌────────┐
   │ Layout │
   └────────┘
        │
        ▼
┌───────────────┐
│ Text Renderer │
└───────────────┘
```

- **Parser** — hand-written line parser (`diagrams/sequence/parser/`)
- **Model** — participants, messages, activations, notes
- **Layout** — column assignment, row spacing, lifeline positioning
- **Renderer** — direct text output (no geometry IR)

## MMDS JSON Format

MMDS (Mermaid Diagram Specification) is a structured JSON interchange format (`src/mmds.rs`):

- **Version 2** of the spec
- Two geometry levels controlled by `--geometry-level`:
  - **layout** (default) — node positions + edge topology only
  - **routed** — adds edge paths, label positions, backward markers, subgraph bounds
- Path detail controlled by `--path-detail`:
  - **full** — all routed waypoints
  - **compact** — remove redundant collinear waypoints
  - **simplified** — start, midpoint, end
  - **endpoints** — start and end only
- MMDS can also be used as input — the `diagrams/mmds/` module hydrates JSON back into `GraphGeometry` for rendering

Schema: `docs/mmds.schema.json`. Spec: `docs/mmds.md`.

## Output Formats

| Format  | Flag        | Description                      |
| ------- | ----------- | -------------------------------- |
| Text    | (default)   | Unicode box-drawing characters   |
| ASCII   | `--ascii`   | ASCII-only characters            |
| SVG     | `--svg`     | Scalable Vector Graphics         |
| MMDS    | `--json`    | Structured JSON interchange      |
| Mermaid | `--mermaid` | Mermaid syntax (from MMDS input) |

## Key Design Decisions

### PEG Parser (pest)

Flowcharts use pest for parsing: declarative grammar, fast zero-copy parsing, good error messages, and Rust-native derive macros. Class and sequence diagrams use hand-written parsers because their syntax is more line-oriented.

### Dagre Parity

The dagre module targets v0.8.5 parity (the same version Mermaid.js uses). This means layout output should match dagre.js for identical input graphs. Parity tests (`tests/dagre_parity.rs`) compare against fixtures generated by running dagre.js directly.

### Compound Graph Support

mmdflux uses dagre's native compound graph pipeline for subgraphs (nesting, border nodes, parent dummy chains). This differs from Mermaid.js, which uses recursive rendering for subgraphs.

### Two-Layer Geometry IR

The GraphGeometry/RoutedGraphGeometry split allows:
- Layout engines to produce geometry without knowing about routing
- Routing strategies to vary independently of engine choice
- Both text and SVG renderers to consume the same routed geometry
- MMDS JSON to serialize either layer depending on consumer needs

### Engine Abstraction

The `GraphLayoutEngine` trait with associated types enables multiple layout backends without changing diagram code. Each engine advertises capabilities (edge routing, subgraph support) so the pipeline adapts automatically.

## Extending mmdflux

### Adding a New Node Shape

1. Add variant to `ShapeSpec` in `parser/ast.rs`
2. Add grammar rule in `grammar.pest`
3. Add variant to `Shape` in `graph/node.rs`
4. Map AST shape to graph shape in `graph/builder.rs`
5. Implement text drawing in `diagrams/flowchart/render/shape.rs`
6. Implement SVG drawing in `diagrams/flowchart/render/svg.rs`

### Adding a New Edge Style

1. Add grammar rules in `grammar.pest`
2. Add variant to `ConnectorSpec` in `parser/ast.rs`
3. Map to `Stroke`/`Arrow` in `graph/builder.rs`
4. Add drawing characters in `render/chars.rs` if needed

### Adding a New Diagram Type

1. Create `src/diagrams/<type>/` with:
   - `mod.rs` — `definition()` returning `DiagramDefinition` with detector + factory
   - `instance.rs` — implement `DiagramInstance` trait
   - `parser/` — parser for the diagram syntax
2. Register in `src/diagrams/mod.rs` and wire into `default_registry()` in `lib.rs`
3. For Graph-family diagrams: write a compiler to `graph::Diagram` and reuse the shared engine/geometry/routing pipeline
4. For other families: implement the full pipeline within the diagram module

### Adding a New Layout Engine

1. Implement `GraphLayoutEngine` with `Input = Diagram, Output = GraphGeometry`
2. Add variant to `LayoutEngineId` in `diagram.rs`
3. Register in `GraphEngineRegistry::default()`
4. Set `EngineCapabilities` appropriately (the pipeline adapts routing mode automatically)
