// Entry point: reads MMDS JSON from stdin, writes .excalidraw JSON to stdout.
//
// Usage:
//   mmdflux --format mmds diagram.mmd | node dist/index.js > out.excalidraw
//   mmdflux --format mmds --geometry-level routed diagram.mmd | node dist/index.js > out.excalidraw

import { convert } from "./convert.js";
import type { MmdsDocument, Bounds } from "./convert.js";

function computeAppState(bounds: Bounds) {
  const pad = 50;
  const contentW = bounds.maxX - bounds.minX + pad * 2;
  const contentH = bounds.maxY - bounds.minY + pad * 2;
  const cx = bounds.minX + (bounds.maxX - bounds.minX) / 2;
  const cy = bounds.minY + (bounds.maxY - bounds.minY) / 2;

  // Fit to a 1200x800 default viewport
  const viewW = 1200;
  const viewH = 800;
  const zoom = Math.min(viewW / contentW, viewH / contentH, 1);

  return {
    theme: "light" as const,
    viewBackgroundColor: "#ffffff",
    scrollX: viewW / 2 - cx * zoom,
    scrollY: viewH / 2 - cy * zoom,
    zoom: { value: zoom },
  };
}

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    let input = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk: string) => {
      input += chunk;
    });
    process.stdin.on("end", () => resolve(input));
    process.stdin.on("error", reject);
  });
}

async function main() {
  let mmds: MmdsDocument;
  try {
    const raw = await readStdin();
    mmds = JSON.parse(raw);
  } catch (err) {
    console.error(
      `Invalid MMDS JSON on stdin: ${err instanceof Error ? err.message : err}`,
    );
    process.exit(1);
  }

  const { elements, bounds } = convert(mmds);

  const output = {
    type: "excalidraw",
    version: 2,
    source: "mmdflux",
    elements,
    appState: computeAppState(bounds),
  };

  console.log(JSON.stringify(output, null, 2));
}

main();
