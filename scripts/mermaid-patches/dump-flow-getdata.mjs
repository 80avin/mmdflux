import { readFileSync } from 'fs';
import mermaid from '../packages/mermaid/dist/mermaid.core.mjs';

const file = process.argv[2];
if (!file) {
  console.error('Usage: dump-flow-getdata.mjs <input.mmd>');
  process.exit(1);
}

mermaid.mermaidAPI.initialize({ startOnLoad: false });

const text = readFileSync(file, 'utf8');
const diag = await mermaid.mermaidAPI.getDiagramFromText(text);
const db = diag.db;
const out = {
  type: diag.type,
  subGraphs: db.getSubGraphs ? db.getSubGraphs() : null,
  data: db.getData ? db.getData() : null,
};

process.stdout.write(JSON.stringify(out, null, 2));
