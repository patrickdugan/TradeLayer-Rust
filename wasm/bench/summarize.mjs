// Merges the JSON lines printed by the three benchmarks into one markdown table.
//
//   node wasm/bench/summarize.mjs native.jsonl wasm.jsonl legacy.jsonl [more.jsonl ...]
//
// Rows of the same workload must carry the same `check` string (same outputs); a mismatch aborts.
import fs from 'node:fs';
import path from 'node:path';

const order = ['native-core', 'native-abi', 'wasm-raw', 'wasm-call', 'legacy-js', 'legacy-js-decode-only'];
const rows = [];
for (const file of process.argv.slice(2)) {
  for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/).filter(Boolean)) {
    const row = JSON.parse(line);
    if (!row.runtime) continue;
    const stem = path.basename(file, '.jsonl').replace(/^native-/, '');
    rows.push({ ...row, base: row.runtime, runtime: row.runtime.startsWith('native') ? `${row.runtime} (${stem})` : row.runtime });
  }
}
const checks = new Map();
for (const row of rows) {
  if (row.base === 'legacy-js-decode-only') continue;
  const seen = checks.get(row.workload);
  if (seen === undefined) checks.set(row.workload, row.check);
  else if (seen !== row.check) throw new Error(`output mismatch in ${row.workload}: ${seen} vs ${row.check} (${row.runtime})`);
}
for (const workload of [...new Set(rows.map((r) => r.workload))]) {
  const set = rows
    .filter((r) => r.workload === workload)
    .sort((a, b) => order.indexOf(a.base) - order.indexOf(b.base) || a.runtime.localeCompare(b.runtime));
  const base = set.find((r) => r.runtime === 'native-core (windows)') ?? set[0];
  console.log(`\n### ${workload} (${set[0].ops} operations per run)\n`);
  console.log('| runtime | median ms | min ms | ops/s | x native-core (windows) |');
  console.log('|---|---:|---:|---:|---:|');
  for (const r of set) {
    const ops = Math.round(r.ops / (r.median_ms / 1000));
    console.log(`| ${r.runtime} | ${r.median_ms.toFixed(2)} | ${r.min_ms.toFixed(2)} | ${ops.toLocaleString('en-US')} | ${(r.median_ms / base.median_ms).toFixed(2)} |`);
  }
}
