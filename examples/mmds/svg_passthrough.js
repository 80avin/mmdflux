// Convert MMDS routed-level JSON to a simple SVG using edge paths directly.
// Usage: mmdflux --format mmds --geometry-level routed diagram.mmd | node svg_passthrough.js

const mmds = JSON.parse(require("fs").readFileSync(process.stdin.fd, "utf8"));

if (mmds.geometry_level !== "routed") {
  console.error("This adapter requires routed-level MMDS. Use --geometry-level routed");
  process.exit(1);
}

const b = mmds.metadata.bounds;
const pad = 20;
const lines = [`<svg xmlns="http://www.w3.org/2000/svg" width="${b.width + pad * 2}" height="${b.height + pad * 2}">`];

for (const n of mmds.nodes) {
  const x = n.position.x - n.size.width / 2 + pad;
  const y = n.position.y - n.size.height / 2 + pad;
  lines.push(`  <rect x="${x}" y="${y}" width="${n.size.width}" height="${n.size.height}" fill="none" stroke="black"/>`);
  lines.push(`  <text x="${n.position.x + pad}" y="${n.position.y + pad}" text-anchor="middle" dominant-baseline="central">${n.label}</text>`);
}

for (const e of mmds.edges) {
  if (e.path && e.path.length >= 2) {
    const d = e.path.map((p, i) => `${i === 0 ? "M" : "L"}${p[0] + pad},${p[1] + pad}`).join(" ");
    lines.push(`  <path d="${d}" fill="none" stroke="black"/>`);
  }
}

lines.push("</svg>");
console.log(lines.join("\n"));
