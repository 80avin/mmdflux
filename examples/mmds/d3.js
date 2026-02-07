// Convert MMDS layout-level JSON to D3 force-layout compatible format.
// Usage: mmdflux --format json diagram.mmd | node d3.js

const mmds = JSON.parse(require("fs").readFileSync("/dev/stdin", "utf8"));

const nodes = mmds.nodes.map((n) => ({
  id: n.id,
  label: n.label,
  x: n.position.x,
  y: n.position.y,
  width: n.size.width,
  height: n.size.height,
}));

const links = mmds.edges.map((e) => ({
  source: e.source,
  target: e.target,
  label: e.label || undefined,
}));

console.log(JSON.stringify({ nodes, links }, null, 2));
