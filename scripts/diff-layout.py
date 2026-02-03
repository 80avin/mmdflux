#!/usr/bin/env python3
import json
import math
import sys

TOL = 1e-6


def load(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def fmt(num):
    return f"{num:.6f}"


def delta(a, b):
    return abs(a - b)


def compare_nodes(mmd_nodes, dagre_nodes):
    mmd_ids = set(mmd_nodes)
    dagre_ids = set(dagre_nodes)
    missing_in_dagre = sorted(mmd_ids - dagre_ids)
    missing_in_mmd = sorted(dagre_ids - mmd_ids)

    diffs = []
    for node_id in sorted(mmd_ids & dagre_ids):
        m = mmd_nodes[node_id]
        d = dagre_nodes[node_id]
        dx = delta(m["x"], d["x"])
        dy = delta(m["y"], d["y"])
        dw = delta(m["width"], d["width"])
        dh = delta(m["height"], d["height"])
        if dx > TOL or dy > TOL or dw > TOL or dh > TOL:
            diffs.append(
                {
                    "id": node_id,
                    "dx": dx,
                    "dy": dy,
                    "dw": dw,
                    "dh": dh,
                    "m": m,
                    "d": d,
                }
            )

    return missing_in_dagre, missing_in_mmd, diffs


def compare_edges(mmd_edges, dagre_edges):
    mmd_idx = set(mmd_edges)
    dagre_idx = set(dagre_edges)
    missing_in_dagre = sorted(mmd_idx - dagre_idx)
    missing_in_mmd = sorted(dagre_idx - mmd_idx)

    diffs = []
    for idx in sorted(mmd_idx & dagre_idx):
        m = mmd_edges[idx]
        d = dagre_edges[idx]
        if len(m["points"]) != len(d["points"]):
            diffs.append(
                {
                    "index": idx,
                    "reason": "point-count",
                    "m_points": len(m["points"]),
                    "d_points": len(d["points"]),
                }
            )
            continue
        max_delta = 0.0
        for (mx, my), (dx, dy) in zip(m["points"], d["points"]):
            max_delta = max(max_delta, delta(mx, dx), delta(my, dy))
        if max_delta > TOL:
            diffs.append(
                {
                    "index": idx,
                    "reason": "point-delta",
                    "max_delta": max_delta,
                    "m": m,
                    "d": d,
                }
            )

    return missing_in_dagre, missing_in_mmd, diffs


def to_node_map(nodes):
    out = {}
    for node in nodes:
        out[node["id"]] = node
    return out


def to_edge_map(edges):
    out = {}
    for edge in edges:
        idx = edge.get("index")
        if idx is None:
            continue
        out[idx] = edge
    return out


def to_subgraph_map(bounds):
    out = {}
    for sg in bounds:
        out[sg["id"]] = sg
    return out


def main():
    if len(sys.argv) != 3:
        print("Usage: diff-layout.py <mmdflux-layout.json> <dagre-layout.json>")
        return 2

    mmd = load(sys.argv[1])
    dagre = load(sys.argv[2])

    mmd_nodes = to_node_map(mmd.get("nodes", []))
    dagre_nodes = to_node_map(dagre.get("nodes", []))
    mmd_edges = to_edge_map(mmd.get("edges", []))
    dagre_edges = to_edge_map(dagre.get("edges", []))
    mmd_subgraphs = to_subgraph_map(mmd.get("subgraph_bounds", []))
    dagre_subgraphs = to_subgraph_map(dagre.get("subgraph_bounds", []))

    n_missing_dagre, n_missing_mmd, node_diffs = compare_nodes(mmd_nodes, dagre_nodes)
    sg_missing_dagre, sg_missing_mmd, sg_diffs = compare_nodes(mmd_subgraphs, dagre_subgraphs)
    e_missing_dagre, e_missing_mmd, edge_diffs = compare_edges(mmd_edges, dagre_edges)

    print("## Layout Output Diffs (mmdflux vs dagre.js)")
    print(f"- Tolerance: {TOL}")
    print(f"- Nodes (mmdflux): {len(mmd_nodes)}, dagre: {len(dagre_nodes)}")
    print(f"- Node ids only in mmdflux: {len(n_missing_dagre)}")
    print(f"- Node ids only in dagre: {len(n_missing_mmd)}")
    print(f"- Node position/size diffs: {len(node_diffs)}")
    print(f"- Subgraph bounds (mmdflux): {len(mmd_subgraphs)}, dagre: {len(dagre_subgraphs)}")
    print(f"- Subgraph ids only in mmdflux: {len(sg_missing_dagre)}")
    print(f"- Subgraph ids only in dagre: {len(sg_missing_mmd)}")
    print(f"- Subgraph bounds diffs: {len(sg_diffs)}")
    print(f"- Edges (mmdflux): {len(mmd_edges)}, dagre: {len(dagre_edges)}")
    print(f"- Edge ids only in mmdflux: {len(e_missing_dagre)}")
    print(f"- Edge ids only in dagre: {len(e_missing_mmd)}")
    print(f"- Edge point diffs: {len(edge_diffs)}")

    def emit_missing(title, items):
        if not items:
            return
        print(f"\n### {title}")
        for item in items:
            print(f"- `{item}`")

    emit_missing("Node ids only in mmdflux", n_missing_dagre)
    emit_missing("Node ids only in dagre", n_missing_mmd)

    if node_diffs:
        print("\n### Node diffs (showing up to 10)")
        for diff in node_diffs[:10]:
            print(
                "- `{}` dx={} dy={} dw={} dh={}"
                .format(
                    diff["id"],
                    fmt(diff["dx"]),
                    fmt(diff["dy"]),
                    fmt(diff["dw"]),
                    fmt(diff["dh"]),
                )
            )

    emit_missing("Subgraph ids only in mmdflux", sg_missing_dagre)
    emit_missing("Subgraph ids only in dagre", sg_missing_mmd)

    if sg_diffs:
        print("\n### Subgraph bounds diffs (showing up to 10)")
        for diff in sg_diffs[:10]:
            print(
                "- `{}` dx={} dy={} dw={} dh={}"
                .format(
                    diff["id"],
                    fmt(diff["dx"]),
                    fmt(diff["dy"]),
                    fmt(diff["dw"]),
                    fmt(diff["dh"]),
                )
            )

    emit_missing("Edge ids only in mmdflux", e_missing_dagre)
    emit_missing("Edge ids only in dagre", e_missing_mmd)

    if edge_diffs:
        print("\n### Edge point diffs (showing up to 10)")
        for diff in edge_diffs[:10]:
            if diff["reason"] == "point-count":
                print(
                    f"- `edge {diff['index']}` point-count mmdflux={diff['m_points']} dagre={diff['d_points']}"
                )
            else:
                print(
                    f"- `edge {diff['index']}` max_delta={fmt(diff['max_delta'])}"
                )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
