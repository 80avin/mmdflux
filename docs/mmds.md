# MMDS — Machine-Mediated Diagram Specification

MMDS is the structured JSON output format for graph-family diagrams produced by mmdflux. It is designed for machine consumption in LLM pipelines, adapter libraries, and agentic workflows.

## Geometry Levels

MMDS supports two geometry levels that control how much spatial detail is included:

### Layout (default)

The default `--format mmds` output. (`--format json` is an alias.) Includes:

- **Node geometry**: position (center x, y) and size (width, height) in unitless layout space
- **Edge topology**: source, target, label, stroke style, arrow types
- **Diagram bounds**: overall width and height in the same layout coordinate space
- **Subgraph structure**: id, title, direct children, parent

Does **not** include edge paths, waypoints, ports, or routing metadata.

```bash
mmdflux --format mmds diagram.mmd
```

### Routed (opt-in)

Explicit opt-in via `--geometry-level routed`. Includes everything from layout plus:

- **Edge paths**: polyline coordinates as `[x, y]` pairs
- **Edge metadata**: `label_position`, `is_backward`
- **Subgraph bounds**: width and height of each subgraph

```bash
mmdflux --format mmds --geometry-level routed diagram.mmd
```

## Output Envelope

```json
{
  "version": 1,
  "defaults": {
    "node": { "shape": "rectangle" },
    "edge": { "stroke": "solid", "arrow_start": "none", "arrow_end": "normal" }
  },
  "geometry_level": "layout",
  "metadata": {
    "diagram_type": "flowchart",
    "direction": "TD",
    "bounds": { "width": 120.0, "height": 80.0 }
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
| `defaults` | object | Document-level defaults for omitted node/edge fields |
| `geometry_level` | `"layout"` or `"routed"` | Geometry detail level |
| `metadata.diagram_type` | string | `"flowchart"` or `"class"` |
| `metadata.direction` | string | `"TD"`, `"BT"`, `"LR"`, or `"RL"` |
| `metadata.bounds` | object | Overall diagram bounds (`width`, `height`) |
| `subgraphs` | array | Subgraph inventory (omitted when empty) |

### Node

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `id` | string | both | Node identifier |
| `label` | string | both | Display label |
| `shape` | string | both | Shape name (snake_case), omitted when equal to `defaults.node.shape` |
| `parent` | string? | both | Parent subgraph ID |
| `position` | `{x, y}` | both | Center position |
| `size` | `{width, height}` | both | Bounding box |

### Edge

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `source` | string | both | Source node ID |
| `target` | string | both | Target node ID |
| `id` | string | both | Deterministic edge ID (`e{declaration_index}`) |
| `label` | string? | both | Edge label |
| `stroke` | string | both | `"solid"`, `"dotted"`, `"thick"`, `"invisible"`; omitted when equal to `defaults.edge.stroke` |
| `arrow_start` | string | both | `"none"`, `"normal"`, `"cross"`, `"circle"`; omitted when equal to `defaults.edge.arrow_start` |
| `arrow_end` | string | both | `"none"`, `"normal"`, `"cross"`, `"circle"`; omitted when equal to `defaults.edge.arrow_end` |
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

## Coordinate System

MMDS coordinates are unitless layout-space values.

- `position.x` and `position.y` are node centers in layout space.
- `size.width` and `size.height` use the same layout-space units.
- `metadata.bounds.width` and `metadata.bounds.height` define the full diagram extents in the same space.
- Routed `path` points and `label_position` values also use this same coordinate space.

Consumers may scale these values to pixels, character cells, or any target render space.

## Defaults and Omission

MMDS has a single JSON shape. Fields that match document defaults may be omitted.

- `defaults.node.shape` defines the implicit node shape when `node.shape` is absent.
- `defaults.edge.stroke`, `defaults.edge.arrow_start`, and `defaults.edge.arrow_end` define implicit edge style when those fields are absent.
- `subgraphs` is omitted when there are no subgraphs.

Consumers should apply defaults before processing if they require explicit values.

## Supported Diagram Types

| Type | `diagram_type` | Status |
|------|---------------|--------|
| Flowchart | `"flowchart"` | Supported |
| Class | `"class"` | Supported |
| Sequence | — | Not supported (timeline family) |
