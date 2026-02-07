# @mmdflux/excalidraw-adapter

Converts [MMDS](../../docs/mmds.md) JSON (mmdflux's intermediate format) into Excalidraw `.excalidraw` files. Nodes become rectangles, diamonds, or ellipses; edges become polyline arrows with labels. Subgraph membership is preserved as Excalidraw groups.

## Prerequisites

- [mmdflux](../../) built and on your PATH (or use `cargo run` from the repo root)
- Node.js >= 18

## Setup

```bash
npm install
npm run build
```

## Usage

Pipe MMDS JSON from mmdflux into the adapter:

```bash
# Layout-level (straight center-to-center arrows)
mmdflux --format mmds diagram.mmd | node dist/index.js > out.excalidraw

# Routed-level (polyline edge paths from mmdflux's router)
mmdflux --format mmds --geometry-level routed diagram.mmd | node dist/index.js > out.excalidraw
```

Open the resulting `.excalidraw` file in [excalidraw.com](https://excalidraw.com) or the Excalidraw VS Code extension.

### Geometry levels

- **layout** (default) — node positions and sizes only; edges are drawn as straight lines between node centers.
- **routed** — includes full edge paths with waypoints, producing right-angle polyline arrows that match mmdflux's text output.

### Scale

Node and edge coordinates are scaled from dagre layout units to pixel space. The default scale factor is 3. Override it with the `SCALE` environment variable:

```bash
mmdflux --format mmds diagram.mmd | SCALE=5 node dist/index.js > out.excalidraw
```

## How it works

1. Reads MMDS JSON from stdin
2. Maps MMDS node shapes to Excalidraw types (rectangle, diamond, ellipse) with text-aware sizing
3. Converts edges to Excalidraw arrows, snapping endpoints to node boundaries
4. Computes viewport zoom/scroll to fit the diagram
5. Writes a complete `.excalidraw` JSON document to stdout
