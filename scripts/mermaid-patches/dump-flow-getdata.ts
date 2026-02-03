import { readFileSync } from 'fs';
import { Diagram } from '../packages/mermaid/src/Diagram.ts';

async function main() {
  const file = process.argv[2];
  if (!file) {
    console.error('Usage: dump-flow-getdata.ts <input.mmd>');
    process.exit(1);
  }
  const text = readFileSync(file, 'utf8');
  const diagram = await Diagram.fromText(text);
  const db: any = diagram.db;
  const out = {
    type: diagram.type,
    subGraphs: db.getSubGraphs ? db.getSubGraphs() : null,
    data: db.getData ? db.getData() : null,
  };
  process.stdout.write(JSON.stringify(out, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
