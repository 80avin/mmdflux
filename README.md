# mmdflux

Parse and render Mermaid diagrams to text, SVG, and MMDS JSON for machine-mediated workflows.

## Installation

```bash
cargo install mmdflux
```

Or build from source:

```bash
git clone https://github.com/kevinswiber/mmdflux
cd mmdflux
cargo build --release
```

## CLI Usage

```bash
# Parse a Mermaid file
mmdflux diagram.mmd

# Read from stdin
echo -e 'graph LR\nA-->B' | mmdflux

# Multi-line input with heredoc
mmdflux <<EOF
graph TD
    A --> B
    B --> C
EOF

# Write to a file
mmdflux diagram.mmd -o output.txt

# Debug mode: show detected diagram type
mmdflux --debug diagram.mmd

# Lint mode: validate input and report diagnostics
mmdflux --lint diagram.mmd

# Show node IDs alongside labels
mmdflux --show-ids diagram.mmd

# ASCII output
mmdflux --format ascii diagram.mmd

# SVG output
mmdflux --format svg diagram.mmd -o diagram.svg

# SVG output with scale factor
mmdflux --format svg --svg-scale 1.5 diagram.mmd -o diagram.svg
```

Note: SVG output is currently supported only for flowcharts; other diagram types return an error.

## Examples

See the [gallery](docs/gallery.md) for rendered fixtures.

### Simple Flow (LR)

Input:
```
graph LR
    A[Start] --> B[Process] --> C[End]
```

Output:
```
┌───────┐     ┌─────────┐     ┌─────┐
│ Start │────►│ Process │────►│ End │
└───────┘     └─────────┘     └─────┘
```

### Decision Flow (TD)

Input:
```
graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Accept]
    B -->|No| D[Reject]
```

Output:
```
          ┌───────┐
          │ Start │
          └───────┘
              │
              │
              │
              │
              ▼
        ┌──────────┐
        < Decision >
        └──────────┘
     ┌───┘        └────┐
     │                 │
    Yes               No
     │                 │
     ▼                 ▼
┌────────┐        ┌────────┐
│ Accept │        │ Reject │
└────────┘        └────────┘
```

### Cycle Flow

Input:
```
graph TD
    A[Start] --> B[Process]
    B --> C[End]
    C --> A
```

Output:
```
      ┌───────┐
      │ Start │
      └───────┘
     ┌─┘     ▲
     │       └┐
     ▼        │
┌─────────┐   │
│ Process │   │
└─────────┘   │
     │        │
     └──┐     │
        ▼   ┌─┘
       ┌─────┐
       │ End │
       └─────┘
```

### Subgraph Flow

Input:
```
graph TD
    subgraph sg1[Process]
        A[Start] --> B[Middle]
    end
    B --> C[End]
```

Output:
```
┌── Process ───┐
│   ┌───────┐  │
│   │ Start │  │
│   └───────┘  │
│       │      │
│       │      │
│       ▼      │
│  ┌────────┐  │
│  │ Middle │  │
│  └────────┘  │
└───────┼──────┘
        │
        │
        │
        │
        ▼
     ┌─────┐
     │ End │
     └─────┘
```

### Nested Subgraphs (LR)

Input:
```
graph LR
    subgraph outer[Outer]
        subgraph left[Left]
            A --> B
        end
        subgraph right[Right]
            C --> D
        end
    end
    B --> C
```

Output:
```
┌───────────────────── Outer ──────────────────────┐
│                                                  │
│    ┌──── Left ────┐         ┌──── Right ─────┐   │
│    │              │         │                │   │
│    │┌───┐    ┌───┐│         │ ┌───┐    ┌───┐ │   │
│    ││ A │───►│ B │┼─────────┼►│ C │───►│ D │ │   │
│    │└───┘    └───┘│         │ └───┘    └───┘ │   │
│    │              │         │                │   │
│    └──────────────┘         └────────────────┘   │
│                                                  │
└──────────────────────────────────────────────────┘
```

### HTTP Request Flow

Input:
```
graph TD
    Client[Client] -->|HTTP Request| Server[Server]
    Server --> Auth{Authenticated?}
    Auth -->|Yes| Process[Process Request]
    Auth -->|No| Reject[401 Unauthorized]
    Process --> Response[Send Response]
    Reject --> Response
    Response -->|HTTP Response| Client
```

Output:
```
                         ┌────────┐
                         │ Client │◄────────┐
                         └────────┘         │
                     ┌────┘                 │
                     │                      │
               HTTP Request                 │
                     │                      │
                     ▼                      │
                ┌────────┐                  │
                │ Server │                  │
                └────────┘                  │
                     │                      │
                     │                      │
                     │                      │
                     │                      │
                     ▼                HTTP Response
            ┌────────────────┐              │
            < Authenticated? >              │
            └────────────────┘              │
         ┌───┘              └────┐          │
         │                       │          │
         │                       │          │
        Yes                     No          │
         │                       │          │
         ▼                       ▼          │
┌─────────────────┐       ┌──────────────────┐
│ Process Request │       │ 401 Unauthorized │
└─────────────────┘       └──────────────────┘
         │                       │          │
         │                       │          │
         │                       │          │
         └────────────────┐      └──────┐   │
                          ▼             ▼   │
                         ┌───────────────┐  │
                         │ Send Response │──┘
                         └───────────────┘
```

### ASCII Mode

Use `--format ascii` for ASCII-only output (no Unicode box-drawing):

```bash
echo 'graph LR\nA-->B-->C' | mmdflux --format ascii
```

```
+---+    +---+    +---+
| A |--->| B |--->| C |
+---+    +---+    +---+
```

## Supported Syntax

### Directions

- `TD` / `TB` - Top to Bottom
- `BT` - Bottom to Top
- `LR` - Left to Right
- `RL` - Right to Left

### Node Shapes

| Syntax          | Shape                   |
| --------------- | ----------------------- |
| `A`             | Rectangle (default)     |
| `A[text]`       | Rectangle with label    |
| `A(text)`       | Rounded rectangle       |
| `A([text])`     | Stadium                 |
| `A[[text]]`     | Subroutine              |
| `A[(text)]`     | Cylinder                |
| `A{text}`       | Diamond                 |
| `A{{text}}`     | Hexagon                 |
| `A((text))`     | Circle                  |
| `A(((text)))`   | Double circle            |
| `A>text]`       | Asymmetric (flag)       |
| `A[/text\]`     | Trapezoid               |
| `A[\text/]`     | Inverse trapezoid       |
| `@{shape: ...}` | Extended shape notation |

### Edge Types

| Syntax         | Description              |
| -------------- | ------------------------ |
| `-->`          | Solid arrow              |
| `-->\|label\|` | Solid arrow with label   |
| `---`          | Open line (no arrow)     |
| `-.->`         | Dotted arrow             |
| `==>`          | Thick arrow              |
| `~~~`          | Invisible (layout only)  |
| `--x`          | Cross arrow              |
| `--o`          | Circle arrow             |
| `<-->`         | Bidirectional arrow      |

### Chains and Groups

```
graph LR
    %% Chain: connects A to B to C
    A --> B --> C

    %% Ampersand: connects X and Y to Z
    X & Y --> Z
```

### Subgraphs

```
graph TD
    subgraph id[Title]
        direction LR
        A --> B
    end
```

Subgraphs group nodes inside a bordered box with an optional title.
Nested subgraphs, cross-boundary edges, direction overrides, and
edges to/from subgraph IDs are supported.

### Comments

Lines starting with `%%` are treated as comments.

## JSON Output (MMDS)

mmdflux produces structured JSON using the MMDS (Machine-Mediated Diagram Specification) format, designed for machine consumption in LLM pipelines, adapter libraries, and agentic workflows.

```bash
# Layout level (default): node geometry + edge topology, no edge paths
mmdflux --format mmds diagram.mmd

# Routed level: includes edge paths, bounds, and routing metadata
mmdflux --format mmds --geometry-level routed diagram.mmd
```

MMDS output is supported for flowchart and class diagrams (`--format json` remains an alias). The output includes a top-level `defaults` block and omits per-node/per-edge fields when they match those defaults. See [`docs/mmds.md`](docs/mmds.md) for the full specification and [`docs/mmds.schema.json`](docs/mmds.schema.json) for the JSON Schema. Adapter examples for React Flow, Cytoscape.js, and D3 are in [`examples/mmds/`](examples/mmds/).

For subgraph-as-endpoint parity, MMDS edges may include optional `from_subgraph` / `to_subgraph` intent metadata. Producers should emit these when available; consumers should tolerate payloads where they are absent and fall back to `source`/`target` node semantics.

MMDS input detection is wired in the registry path, and hydration enforces a strict-core/permissive-extensions validation contract (see [MMDS input validation contract](docs/mmds.md#mmds-input-validation-contract)).

Render support for MMDS input is geometry-level aware:
- `layout` payloads support `text`, `ascii`, `svg`, and `mmds/json`.
- `routed` (positioned) payloads support `svg` and `mmds/json`; `text`/`ascii` are intentionally rejected with guidance to use SVG.

MMDS governance fields are `profiles` and namespaced `extensions`. The initial profile vocabulary is `mmds-core-v1`, `mmdflux-svg-v1`, and `mmdflux-text-v1`. Canonical profile payload examples are:
- [`examples/mmds/profile-mmdflux-svg-v1.json`](examples/mmds/profile-mmdflux-svg-v1.json)
- [`examples/mmds/profile-mmdflux-text-v1.json`](examples/mmds/profile-mmdflux-text-v1.json)

See [`docs/mmds.md`](docs/mmds.md) for the full capability matrix and detailed accepted/rejected/tolerated MMDS input behavior.

### MMDS -> Mermaid Generation (Library)

For graph-family payloads, mmdflux can generate canonical Mermaid from MMDS:

```rust
use mmdflux::generate_mermaid_from_mmds_str;

let mmds_json = std::fs::read_to_string("diagram.mmds.json").unwrap();
let mermaid = generate_mermaid_from_mmds_str(&mmds_json).unwrap();
println!("{mermaid}");
```

Generation is deterministic (stable ordering, escaping policy, trailing newline) and intended for semantic roundtrip workflows. See [MMDS -> Mermaid generation contract](docs/mmds.md#mmds---mermaid-generation-contract) for the full policy and caveats.

## Library Usage

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

    // Access nodes by ID
    if let Some(node) = diagram.nodes.get("A") {
        println!("Node A: {} ({:?})", node.label, node.shape);
    }

    // Iterate edges
    for edge in &diagram.edges {
        println!("{} -> {}", edge.from, edge.to);
    }
}
```

### Types

```rust
use mmdflux::{Diagram, Direction, Node, Shape, Edge};
use mmdflux::graph::{Stroke, Arrow};

// Direction: TopDown, BottomTop, LeftRight, RightLeft
// Shape: Rectangle, Round, Stadium, Subroutine, Cylinder, Diamond, Hexagon, ...
// Stroke: Solid, Dotted, Thick, Invisible
// Arrow: Normal, None, Cross, Circle
```

## Roadmap

- [x] Flowchart parsing (`graph` / `flowchart`)
- [x] Text rendering (Unicode and ASCII, TD/BT/LR/RL layouts)
- [x] SVG rendering
- [x] Subgraph support (nesting, direction overrides, edges to subgraph IDs)
- [ ] Sequence diagrams
- [ ] Class diagrams
- [ ] State diagrams
- [ ] Entity Relationship diagrams

## License

MIT
