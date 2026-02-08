# MMDS — Machine-Mediated Diagram Specification

MMDS is the structured JSON output format for graph-family diagrams produced by mmdflux. It is designed for machine consumption in LLM pipelines, adapter libraries, and agentic workflows.

## Input Status

MMDS input support is in active ingestion rollout:

- The registry detects MMDS JSON input and dispatches to the `mmds` diagram type.
- Parse-time envelope validation is active (`MMDS parse error: ...` on invalid JSON/envelope).
- MMDS core hydration/validation contract is implemented (`MMDS validation error: ...` on invalid core payloads).
- Render runtime is still scaffolded and currently returns:
  - `MMDS input scaffold: hydration/render pipeline is not implemented yet`

This means MMDS is stable as an output contract and hydration contract today, while direct MMDS-to-render behavior is finishing in follow-on phases.

## MMDS Input Validation Contract

Hydration follows a **strict-core / permissive-extensions** policy:

- **Strict core** (rejected):
  - unsupported `version`
  - invalid core enum values (`geometry_level`, directions, shapes, strokes, arrows)
  - missing required identifiers (`node.id`, `edge.id`, `subgraph.id`, edge endpoints)
  - dangling references (edge source/target, node parent, subgraph parent/children)
  - cyclic subgraph parent chains
- **Permissive extensions** (tolerated):
  - unknown `profiles` values
  - unknown namespaces under `extensions`

Hydration also expands omitted node/edge fields from the document `defaults` block before mapping to internal graph types.

### Deterministic Ordering

Hydrated edge insertion order is deterministic:

1. sort by explicit edge ID when it matches `e{number}`
2. fallback to declaration order for ties/non-numeric IDs

### Canonical Error Example

`MMDS validation error: edge e0 target 'X' not found`

## Geometry Levels

MMDS supports two geometry levels that control how much spatial detail is included:

### Layout (default)

The default `--format mmds` output. (`--format json` is an alias.) Includes:

- **Node geometry**: position (center x, y) and size (width, height) in unitless layout space
- **Edge topology**: source, target, label, stroke style, arrow types
- **Diagram bounds**: overall width and height in the same layout coordinate space
- **Subgraph structure**: id, title, direct children, parent, direction override

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
    "edge": { "stroke": "solid", "arrow_start": "none", "arrow_end": "normal", "minlen": 1 }
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
| `version` | `1` | Integer schema version. Increment only for breaking MMDS changes. |
| `defaults` | object | Document-level defaults for omitted node/edge fields |
| `geometry_level` | `"layout"` or `"routed"` | Geometry detail level |
| `metadata.diagram_type` | string | `"flowchart"` or `"class"` |
| `metadata.direction` | string | `"TD"`, `"BT"`, `"LR"`, or `"RL"` |
| `metadata.bounds` | object | Overall diagram canvas extents (`width`, `height`) in MMDS layout space |
| `subgraphs` | array | Subgraph inventory (omitted when empty) |

### Node

| Field | Type | Level | Description |
|-------|------|-------|-------------|
| `id` | string | both | Node identifier |
| `label` | string | both | Display label |
| `shape` | string | both | Shape name (snake_case), omitted when equal to `defaults.node.shape` |
| `parent` | string? | both | Parent subgraph ID |
| `position` | `{x, y}` | both | Center position (not top-left) |
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
| `minlen` | integer | both | Minimum rank separation; omitted when equal to `defaults.edge.minlen` |
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
| `direction` | string? | both | Direction override: `"TD"`, `"BT"`, `"LR"`, or `"RL"` |
| `bounds` | `{width, height}` | routed | Bounding box dimensions |

## Schema

The formal JSON Schema is available at [`docs/mmds.schema.json`](./mmds.schema.json).

## Coordinate System

MMDS coordinates are unitless layout-space values.

- `position.x` and `position.y` are node centers in layout space (not top-left anchors).
- `size.width` and `size.height` use the same layout-space units.
- `metadata.bounds.width` and `metadata.bounds.height` define full document extents in the same space.
- `metadata.bounds` is a canvas extent, not guaranteed to be a tight content bounding box.
- Current graph-family engines may include outer margin in `metadata.bounds`.
- Routed `path` points and `label_position` values also use this same coordinate space.

Consumers may scale these values to pixels, character cells, or any target render space.
Consumers SHOULD scale uniformly (same factor on both axes) to preserve the aspect ratio implied by `metadata.bounds`.
Consumers rendering top-left-anchored primitives should convert node placement as:
- `left = position.x - size.width / 2`
- `top = position.y - size.height / 2`

## Defaults and Omission

MMDS has a single JSON shape. Fields that match document defaults may be omitted.

- `defaults.node.shape` defines the implicit node shape when `node.shape` is absent.
- `defaults.edge.stroke`, `defaults.edge.arrow_start`, `defaults.edge.arrow_end`, and `defaults.edge.minlen` define implicit edge semantics when those fields are absent.
- `subgraphs` is omitted when there are no subgraphs.

Consumers should apply defaults before processing if they require explicit values.

## Supported Diagram Types

| Type | `diagram_type` | Status |
|------|---------------|--------|
| Flowchart | `"flowchart"` | Supported |
| Class | `"class"` | Supported |
| Sequence | — | Not supported (timeline family) |
