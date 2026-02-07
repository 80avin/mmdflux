// Convert MMDS layout-level JSON to Excalidraw format (zero-dependency example).
// Usage: mmdflux --format mmds diagram.mmd | node excalidraw.js > diagram.excalidraw
//
// For the full-featured TypeScript version with polyline edge routing,
// see adapters/excalidraw/ in the project root.
//
// The output is a valid .excalidraw JSON file that can be opened directly
// in Excalidraw (https://excalidraw.com) or any compatible viewer.
//
// MMDS coordinates are in compact layout units. A scale factor (default 10)
// converts them to pixel-sized Excalidraw elements. Override with:
//   SCALE=15 node excalidraw.js

const SCALE = Number(process.env.SCALE) || 10;

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

function hashCode(s) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

function baseElement(id, type, x, y, width, height) {
  return {
    id,
    type,
    x,
    y,
    width,
    height,
    angle: 0,
    strokeColor: "#1e1e1e",
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 0,
    opacity: 100,
    groupIds: [],
    frameId: null,
    roundness: null,
    seed: hashCode(id),
    version: 1,
    versionNonce: 0,
    isDeleted: false,
    boundElements: null,
    updated: Date.now(),
    link: null,
    locked: false,
    index: null,
  };
}

function excalidrawShape(mmdsShape) {
  switch (mmdsShape) {
    case "diamond":
      return "diamond";
    case "circle":
    case "double-circle":
      return "ellipse";
    default:
      return "rectangle";
  }
}

function excalidrawRoundness(mmdsShape) {
  switch (mmdsShape) {
    case "round":
    case "stadium":
      return { type: 3 };
    default:
      return null;
  }
}

function excalidrawArrowhead(mmdsArrow) {
  switch (mmdsArrow) {
    case "normal":
      return "arrow";
    case "cross":
      return "bar";
    case "circle":
      return "circle";
    default:
      return null;
  }
}

function excalidrawStroke(mmdsStroke) {
  switch (mmdsStroke) {
    case "dotted":
      return { strokeStyle: "dotted", strokeWidth: 2 };
    case "thick":
      return { strokeStyle: "solid", strokeWidth: 4 };
    default:
      return { strokeStyle: "solid", strokeWidth: 2 };
  }
}

readStdin((mmds) => {
  const elements = [];
  const nodeDefaults = mmds.defaults?.node || {};
  const edgeDefaults = mmds.defaults?.edge || {};

  const nodeMap = new Map();
  for (const n of mmds.nodes) nodeMap.set(n.id, n);

  // Track which bound elements reference each node (arrows + text).
  const nodeBound = new Map();
  for (const n of mmds.nodes) nodeBound.set(n.id, []);

  // --- Subgraph frames (rendered behind everything) ---
  const sgMap = new Map();
  if (mmds.subgraphs) {
    for (const sg of mmds.subgraphs) {
      sgMap.set(sg.id, sg);
      const children = sg.children.map((id) => nodeMap.get(id)).filter(Boolean);
      if (children.length === 0) continue;

      const pad = 30;
      let minX = Infinity,
        minY = Infinity,
        maxX = -Infinity,
        maxY = -Infinity;
      for (const c of children) {
        minX = Math.min(minX, c.position.x * SCALE - (c.size.width * SCALE) / 2);
        maxX = Math.max(maxX, c.position.x * SCALE + (c.size.width * SCALE) / 2);
        minY = Math.min(minY, c.position.y * SCALE - (c.size.height * SCALE) / 2);
        maxY = Math.max(maxY, c.position.y * SCALE + (c.size.height * SCALE) / 2);
      }

      elements.push({
        ...baseElement(
          sg.id,
          "frame",
          minX - pad,
          minY - pad - 20,
          maxX - minX + pad * 2,
          maxY - minY + pad * 2 + 20,
        ),
        name: sg.title,
      });
    }
  }

  // --- Nodes ---
  for (const n of mmds.nodes) {
    const shape = n.shape || nodeDefaults.shape || "rectangle";
    const w = n.size.width * SCALE;
    const h = n.size.height * SCALE;
    const left = n.position.x * SCALE - w / 2;
    const top = n.position.y * SCALE - h / 2;
    const textId = `${n.id}_label`;

    nodeBound.get(n.id).push({ id: textId, type: "text" });

    const el = {
      ...baseElement(n.id, excalidrawShape(shape), left, top, w, h),
      roundness: excalidrawRoundness(shape),
    };
    if (n.parent && sgMap.has(n.parent)) el.frameId = n.parent;
    elements.push(el);

    const textEl = {
      ...baseElement(textId, "text", left, top, w, h),
      text: n.label,
      originalText: n.label,
      fontSize: 16,
      fontFamily: 2,
      textAlign: "center",
      verticalAlign: "middle",
      containerId: n.id,
      lineHeight: 1.25,
      autoResize: true,
    };
    if (n.parent && sgMap.has(n.parent)) textEl.frameId = n.parent;
    elements.push(textEl);
  }

  // --- Edges ---
  for (const e of mmds.edges) {
    const src = nodeMap.get(e.source);
    const tgt = nodeMap.get(e.target);
    if (!src || !tgt) continue;

    const dx = (tgt.position.x - src.position.x) * SCALE;
    const dy = (tgt.position.y - src.position.y) * SCALE;
    const stroke = e.stroke || edgeDefaults.stroke || "solid";
    const arrowStart = e.arrow_start || edgeDefaults.arrow_start || "none";
    const arrowEnd = e.arrow_end || edgeDefaults.arrow_end || "normal";
    const { strokeStyle, strokeWidth } = excalidrawStroke(stroke);

    const arrowId = e.id;
    const arrowEl = {
      ...baseElement(arrowId, "arrow", src.position.x * SCALE, src.position.y * SCALE, Math.abs(dx), Math.abs(dy)),
      strokeStyle,
      strokeWidth,
      points: [[0, 0], [dx, dy]],
      startBinding: { elementId: e.source, fixedPoint: [0.5, 0.5], focus: 0, gap: 1 },
      endBinding: { elementId: e.target, fixedPoint: [0.5, 0.5], focus: 0, gap: 1 },
      startArrowhead: excalidrawArrowhead(arrowStart),
      endArrowhead: excalidrawArrowhead(arrowEnd),
      elbowed: false,
    };

    if (nodeBound.has(e.source)) nodeBound.get(e.source).push({ id: arrowId, type: "arrow" });
    if (nodeBound.has(e.target)) nodeBound.get(e.target).push({ id: arrowId, type: "arrow" });

    if (e.label) {
      const labelId = `${arrowId}_label`;
      arrowEl.boundElements = [{ id: labelId, type: "text" }];
      elements.push(arrowEl);
      elements.push({
        ...baseElement(labelId, "text", src.position.x * SCALE + dx / 2, src.position.y * SCALE + dy / 2, 0, 0),
        text: e.label,
        originalText: e.label,
        fontSize: 14,
        fontFamily: 2,
        textAlign: "center",
        verticalAlign: "middle",
        containerId: arrowId,
        lineHeight: 1.25,
        autoResize: true,
      });
    } else {
      elements.push(arrowEl);
    }
  }

  // Patch boundElements onto node shapes (arrows + contained text).
  for (const el of elements) {
    if (nodeBound.has(el.id)) {
      el.boundElements = nodeBound.get(el.id);
    }
  }

  console.log(
    JSON.stringify(
      {
        type: "excalidraw",
        version: 2,
        source: "mmdflux",
        elements,
        appState: { theme: "light", viewBackgroundColor: "#ffffff" },
      },
      null,
      2,
    ),
  );
});
