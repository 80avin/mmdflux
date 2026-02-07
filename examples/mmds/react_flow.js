// Convert MMDS layout-level JSON to React Flow nodes and edges.
// Usage: mmdflux --format mmds diagram.mmd | node react_flow.js

const mmds = JSON.parse(require("fs").readFileSync(process.stdin.fd, "utf8"));

const nodes = mmds.nodes.map((n) => ({
  id: n.id,
  data: { label: n.label },
  position: { x: n.position.x - n.size.width / 2, y: n.position.y - n.size.height / 2 },
  style: { width: n.size.width, height: n.size.height },
}));

const edges = mmds.edges.map((e) => ({
  id: e.id,
  source: e.source,
  target: e.target,
  label: e.label || undefined,
  animated: e.stroke === "dotted",
}));

console.log(JSON.stringify({ nodes, edges }, null, 2));
