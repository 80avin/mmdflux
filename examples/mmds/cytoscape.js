// Convert MMDS layout-level JSON to Cytoscape.js elements.
// Usage: mmdflux --format mmds diagram.mmd | node cytoscape.js

const mmds = JSON.parse(require("fs").readFileSync(process.stdin.fd, "utf8"));

const elements = [];

for (const n of mmds.nodes) {
  elements.push({
    data: { id: n.id, label: n.label, parent: n.parent || undefined },
    position: { x: n.position.x, y: n.position.y },
  });
}

for (const sg of mmds.subgraphs) {
  elements.push({
    data: { id: sg.id, label: sg.title, parent: sg.parent || undefined },
  });
}

for (const e of mmds.edges) {
  elements.push({
    data: { id: e.id, source: e.source, target: e.target, label: e.label || undefined },
  });
}

console.log(JSON.stringify(elements, null, 2));
