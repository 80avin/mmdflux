# MMDS — Machine-Mediated Diagram Specification

MMDS is the structured JSON output format for graph-family diagrams produced by mmdflux. It is designed for machine consumption in LLM pipelines, adapter libraries, and agentic workflows.

## Geometry Levels

MMDS supports two geometry levels that control how much spatial detail is included:

### Layout (default)

The default `--format mmds` output. (`--format json` is an alias.) Includes:

- **Node geometry**: position (center x, y) and size (width, height) in layout float space
- **Edge topology**: source, target, label, stroke style, arrow types
- **Subgraph structure**: id, title, direct children, parent

Does **not** include edge paths, waypoints, ports, or routing metadata.

```bash
mmdflux --format mmds diagram.mmd
```

### Routed (opt-in)

Explicit opt-in via `--geometry-level routed`. Includes everything from layout plus:

- **Edge paths**: polyline coordinates as `[x, y]` pairs
- **Edge metadata**: `label_position`, `is_backward`
- **Metadata bounds**: overall layout bounding box
- **Subgraph bounds**: width and height of each subgraph

```bash
mmdflux --format mmds --geometry-level routed diagram.mmd
```

## Output Envelope

```json
{
  "version": 1,
  "geometry_level": "layout",
  "metadata": {
    "diagram_type": "flowchart",
    "direction": "TD"
  },
  "nodes": [...],
  "edges": [...],
  "subgraphs": [...]
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | `1` | Schema version |
| `geometry_level` | `"layout"` or `"routed"` | Geometry detail level |
| `metadata.diagram_type` | string | `"flowchart"` or `"class"` |
| `metadata.direction` | string | `"TD"`, `"BT"`, `"LR"`, or `"RL"` |
| `metadata.bounds` | object | Overall bounds (routed only) |

### Node

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `id` | string | both | Node identifier |
| `label` | string | both | Display label |
| `shape` | string | both | Shape name (snake_case) |
| `parent` | string? | both | Parent subgraph ID |
| `position` | `{x, y}` | both | Center position |
| `size` | `{width, height}` | both | Bounding box |

### Edge

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `source` | string | both | Source node ID |
| `target` | string | both | Target node ID |
| `label` | string? | both | Edge label |
| `stroke` | string | both | `"solid"`, `"dotted"`, `"thick"`, `"invisible"` |
| `arrow_start` | string | both | `"none"`, `"normal"`, `"cross"`, `"circle"` |
| `arrow_end` | string | both | `"none"`, `"normal"`, `"cross"`, `"circle"` |
| `path` | `[[x,y],...]` | routed | Polyline path coordinates |
| `label_position` | `{x, y}` | routed | Label center |
| `is_backward` | boolean | routed | Flows backward in layout |

### Subgraph

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `id` | string | both | Subgraph identifier |
| `title` | string | both | Display title |
| `children` | string[] | both | Direct child node IDs |
| `parent` | string? | both | Parent subgraph ID |
| `bounds` | `{width, height}` | routed | Bounding box dimensions |

## Schema

The formal JSON Schema is available at [`docs/mmds.schema.json`](./mmds.schema.json).

## Supported Diagram Types

| Type | `diagram_type` | Status |
|------|---------------|--------|
| Flowchart | `"flowchart"` | Supported |
| Class | `"class"` | Supported |
| Sequence | — | Not supported (timeline family) |
