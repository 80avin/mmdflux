#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

if (process.argv.length < 3) {
  console.error("Usage: dump-dagre-borders.js <input.json>");
  process.exit(1);
}

const inputPath = process.argv[2];
const data = JSON.parse(fs.readFileSync(inputPath, "utf8"));

const dagreRoot = "/Users/kevin/src/dagre";
const Graph = require(path.join(dagreRoot, "node_modules", "@dagrejs", "graphlib")).Graph;
const acyclic = require(path.join(dagreRoot, "lib", "acyclic"));
const normalize = require(path.join(dagreRoot, "lib", "normalize"));
const rank = require(path.join(dagreRoot, "lib", "rank"));
const util = require(path.join(dagreRoot, "lib", "util"));
const parentDummyChains = require(path.join(dagreRoot, "lib", "parent-dummy-chains"));
const nestingGraph = require(path.join(dagreRoot, "lib", "nesting-graph"));
const addBorderSegments = require(path.join(dagreRoot, "lib", "add-border-segments"));
const coordinateSystem = require(path.join(dagreRoot, "lib", "coordinate-system"));
const order = require(path.join(dagreRoot, "lib", "order"));
const position = require(path.join(dagreRoot, "lib", "position"));

const g = new Graph({ multigraph: true, compound: true });

const graphAttrs = {
  rankdir: data.graph.rankdir || "TB",
  nodesep: data.graph.nodesep ?? 50,
  edgesep: data.graph.edgesep ?? 20,
  ranksep: data.graph.ranksep ?? 50,
  ranker: data.graph.ranker || "network-simplex",
  rankalign: "center",
  marginx: data.graph.marginx ?? 10,
  marginy: data.graph.marginy ?? 10,
};

g.setGraph(graphAttrs);
g.setDefaultEdgeLabel(() => ({}));

for (const node of data.nodes) {
  g.setNode(node.id, {
    label: node.label,
    width: node.width,
    height: node.height,
  });
}

for (const node of data.nodes) {
  if (node.parent) {
    g.setParent(node.id, node.parent);
  }
}

for (const edge of data.edges) {
  const label = {
    weight: 1,
    minlen: 1,
    width: 0,
    height: 0,
    labeloffset: 10,
    labelpos: "r",
  };
  if (edge.label) {
    label.label = edge.label;
    label.width = edge.label_width;
    label.height = edge.label_height;
  }
  g.setEdge(edge.from, edge.to, label, String(edge.index));
}

function makeSpaceForEdgeLabels(graph) {
  const graphLabel = graph.graph();
  graphLabel.ranksep /= 2;
  graph.edges().forEach((e) => {
    const edge = graph.edge(e);
    edge.minlen *= 2;
    if (edge.labelpos.toLowerCase() !== "c") {
      if (graphLabel.rankdir === "TB" || graphLabel.rankdir === "BT") {
        edge.width += edge.labeloffset;
      } else {
        edge.height += edge.labeloffset;
      }
    }
  });
}

function injectEdgeLabelProxies(graph) {
  graph.edges().forEach((e) => {
    const edge = graph.edge(e);
    if (edge.width && edge.height) {
      const v = graph.node(e.v);
      const w = graph.node(e.w);
      const label = { rank: (w.rank - v.rank) / 2 + v.rank, e: e };
      util.addDummyNode(graph, "edge-proxy", label, "_ep");
    }
  });
}

function assignRankMinMax(graph) {
  let maxRank = 0;
  graph.nodes().forEach((v) => {
    const node = graph.node(v);
    if (node.borderTop) {
      node.minRank = graph.node(node.borderTop).rank;
      node.maxRank = graph.node(node.borderBottom).rank;
      maxRank = Math.max(maxRank, node.maxRank);
    }
  });
  graph.graph().maxRank = maxRank;
}

function removeEdgeLabelProxies(graph) {
  graph.nodes().forEach((v) => {
    const node = graph.node(v);
    if (node.dummy === "edge-proxy") {
      graph.edge(node.e).labelRank = node.rank;
      graph.removeNode(v);
    }
  });
}

function removeSelfEdges(graph) {
  graph.edges().forEach((e) => {
    if (e.v === e.w) {
      const node = graph.node(e.v);
      if (!node.selfEdges) {
        node.selfEdges = [];
      }
      node.selfEdges.push({ e: e, label: graph.edge(e) });
      graph.removeEdge(e);
    }
  });
}

function insertSelfEdges(graph) {
  const layers = util.buildLayerMatrix(graph);
  layers.forEach((layer) => {
    let orderShift = 0;
    layer.forEach((v, i) => {
      const node = graph.node(v);
      node.order = i + orderShift;
      (node.selfEdges || []).forEach((selfEdge) => {
        orderShift += 1;
        util.addDummyNode(graph, "selfedge", {
          width: selfEdge.label.width,
          height: selfEdge.label.height,
          rank: node.rank,
          order: i + orderShift,
          e: selfEdge.e,
          label: selfEdge.label,
        }, "_se");
      });
      delete node.selfEdges;
    });
  });
}

function translateGraph(graph) {
  let minX = Number.POSITIVE_INFINITY;
  let maxX = 0;
  let minY = Number.POSITIVE_INFINITY;
  let maxY = 0;
  const graphLabel = graph.graph();
  const marginX = graphLabel.marginx || 0;
  const marginY = graphLabel.marginy || 0;

  function isFiniteNum(value) {
    return typeof value === "number" && Number.isFinite(value);
  }

  function getExtremes(attrs) {
    if (!isFiniteNum(attrs.x) || !isFiniteNum(attrs.y)) {
      return;
    }
    const x = attrs.x;
    const y = attrs.y;
    const w = attrs.width;
    const h = attrs.height;
    minX = Math.min(minX, x - w / 2);
    maxX = Math.max(maxX, x + w / 2);
    minY = Math.min(minY, y - h / 2);
    maxY = Math.max(maxY, y + h / 2);
  }

  graph.nodes().forEach((v) => getExtremes(graph.node(v)));
  graph.edges().forEach((e) => {
    const edge = graph.edge(e);
    if (Object.hasOwn(edge, "x")) {
      getExtremes(edge);
    }
  });

  if (!Number.isFinite(minX) || !Number.isFinite(minY)) {
    return;
  }

  minX -= marginX;
  minY -= marginY;
  maxX += marginX;
  maxY += marginY;

  graph.nodes().forEach((v) => {
    const node = graph.node(v);
    if (!isFiniteNum(node.x) || !isFiniteNum(node.y)) {
      return;
    }
    node.x -= minX;
    node.y -= minY;
  });

  graph.edges().forEach((e) => {
    const edge = graph.edge(e);
    if (Object.hasOwn(edge, "x")) {
      edge.x -= minX;
      edge.y -= minY;
    }
  });

  graph.graph().width = maxX - minX;
  graph.graph().height = maxY - minY;
}

// --- Pipeline (mirrors dagre runLayout up through position, without removing borders) ---
makeSpaceForEdgeLabels(g);
removeSelfEdges(g);
acyclic.run(g);
nestingGraph.run(g);
rank(util.asNonCompoundGraph(g));
injectEdgeLabelProxies(g);
util.removeEmptyRanks(g);
nestingGraph.cleanup(g);
util.normalizeRanks(g);
assignRankMinMax(g);
removeEdgeLabelProxies(g);
normalize.run(g);
parentDummyChains(g);
addBorderSegments(g);
order(g, {});
insertSelfEdges(g);
coordinateSystem.adjust(g);
position(g);
g.edges().forEach((e) => {
  const edge = g.edge(e);
  if (!edge.points) {
    edge.points = [];
  }
});
coordinateSystem.undo(g);
translateGraph(g);

function fmtNum(value) {
  if (value === undefined || Number.isNaN(value)) {
    return "NaN";
  }
  return value.toFixed(2);
}

function fmtInt(value) {
  return value === undefined ? "?" : value;
}

const compounds = g.nodes().filter((v) => g.children(v).length);
compounds.sort();
console.log("[border_nodes] layout positions");
for (const v of compounds) {
  const node = g.node(v);
  if (!node.borderLeft || !node.borderRight) {
    continue;
  }

  const minRank = node.minRank;
  const maxRank = node.maxRank;
  console.log(`[border_nodes] ${v} min_rank=${minRank} max_rank=${maxRank}`);

  if (node.borderTop) {
    const top = g.node(node.borderTop);
    console.log(
      `[border_nodes]   top ${node.borderTop} rank=${fmtInt(top.rank)} order=${fmtInt(top.order)} x=${fmtNum(top.x)} y=${fmtNum(top.y)}`
    );
  }

  if (node.borderBottom) {
    const bottom = g.node(node.borderBottom);
    console.log(
      `[border_nodes]   bottom ${node.borderBottom} rank=${fmtInt(bottom.rank)} order=${fmtInt(bottom.order)} x=${fmtNum(bottom.x)} y=${fmtNum(bottom.y)}`
    );
  }

  for (let rankVal = minRank; rankVal <= maxRank; rankVal += 1) {
    const leftId = node.borderLeft[rankVal];
    const rightId = node.borderRight[rankVal];
    if (!leftId || !rightId) {
      continue;
    }

    const left = g.node(leftId);
    const right = g.node(rightId);
    console.log(
      `[border_nodes]   rank ${rankVal}: left ${leftId} order=${fmtInt(left.order)} x=${fmtNum(left.x)} y=${fmtNum(left.y)} right ${rightId} order=${fmtInt(right.order)} x=${fmtNum(right.x)} y=${fmtNum(right.y)}`
    );
  }
}
