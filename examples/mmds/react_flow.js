// Convert MMDS layout-level JSON to React Flow nodes and edges.
// Usage: mmdflux --format mmds diagram.mmd | node react_flow.js

function readStdin(onSuccess) {
  let input = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => {
    input += chunk;
  });
  process.stdin.on("end", () => {
    try {
      onSuccess(JSON.parse(input));
    } catch (err) {
      console.error(`Invalid MMDS JSON on stdin: ${err.message}`);
      process.exit(1);
    }
  });
  process.stdin.on("error", (err) => {
    console.error(`Failed reading stdin: ${err.message}`);
    process.exit(1);
  });
}

readStdin((mmds) => {
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
});
