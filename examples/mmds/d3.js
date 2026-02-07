// Convert MMDS layout-level JSON to D3 force-layout compatible format.
// Usage: mmdflux --format mmds diagram.mmd | node d3.js

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
    label: n.label,
    x: n.position.x,
    y: n.position.y,
    width: n.size.width,
    height: n.size.height,
  }));

  const links = mmds.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    label: e.label || undefined,
  }));

  console.log(JSON.stringify({ nodes, links }, null, 2));
});
