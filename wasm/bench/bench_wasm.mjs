// WebAssembly codec benchmark under Node. Same corpus, block fixtures, iteration counts and run
// protocol as examples/bench_codec.rs and bench_legacy.cjs.
//
//   node wasm/bench/bench_wasm.mjs <tradelayer_wasm.wasm> [--iters N] [--block-iters M] [--runs R] [--warmup W]
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const { loadCorpus, ROOT } = require('./legacy_loader.cjs');
const { loadTradeLayer } = await import(pathToFileURL(path.join(here, '..', 'js', 'tradelayer.mjs')).href);

const wasmPath = process.argv[2];
if (!wasmPath) throw new Error('usage: bench_wasm.mjs <tradelayer_wasm.wasm> [options]');
const opt = { iters: 20, 'block-iters': 500, runs: 7, warmup: 3 };
const argv = process.argv.slice(3);
for (let i = 0; i < argv.length; i += 2) {
  const key = argv[i].replace(/^--/, '');
  if (!(key in opt)) throw new Error('bad argument ' + argv[i]);
  opt[key] = Number(argv[i + 1]);
}

const tl = await loadTradeLayer(fs.readFileSync(wasmPath));
const rows = loadCorpus();
const blocks = [
  ['synthetic-4tx', 'raw_block_ingest_synthetic.hex'],
  ['review-block', 'tx_parser_review_block.hex'],
].map(([name, file]) => [name, fs.readFileSync(path.join(ROOT, 'fixtures', file), 'utf8').trim()]);

function measure(once) {
  for (let i = 0; i < opt.warmup; i++) once();
  const times = [];
  for (let i = 0; i < opt.runs; i++) {
    const t = process.hrtime.bigint();
    once();
    times.push(Number(process.hrtime.bigint() - t) / 1e6);
  }
  return times;
}
function report(runtime, workload, ops, check, times) {
  const sorted = [...times].sort((a, b) => a - b);
  console.log(JSON.stringify({
    runtime, workload, ops, check,
    median_ms: Number(sorted[Math.floor(sorted.length / 2)].toFixed(3)),
    min_ms: Number(sorted[0].toFixed(3)),
    runs_ms: times.map((t) => Number(t.toFixed(3))),
  }));
}
console.log(JSON.stringify({ meta: { node: process.version, wasm_bytes: fs.statSync(wasmPath).size, rows: rows.length, ...opt } }));

// ---- decode_corpus ----
const requests = rows.map((r) => JSON.stringify({ op: 'decode_body', method: r.method, body: r.body }));
const objects = rows.map((r) => ({ op: 'decode_body', method: r.method, body: r.body }));
let ok = 0, thrown = 0, bytes = 0;
requests.forEach((req, i) => {
  const row = rows[i];
  const resp = tl.callRaw(req);
  if (row.ok) {
    assert.ok(resp.startsWith('{"ok":true'), row.method);
    const value = resp.slice(resp.indexOf('"value":') + 8, -1);
    assert.equal(value, row.expected, row.method);
    ok++;
    bytes += Buffer.byteLength(value);
  } else {
    const r = JSON.parse(resp);
    assert.equal(r.ok, false);
    assert.equal(r.code, 'SourceThrow');
    assert.equal(r.message, `${row.name}: ${row.message}`);
    thrown++;
  }
});
const check = `ok=${ok},throw=${thrown},bytes=${bytes}`;
const ops = rows.length * opt.iters;
let sink = 0;

report('wasm-raw', 'decode_corpus', ops, check, measure(() => {
  let acc = 0;
  for (let n = 0; n < opt.iters; n++) for (const req of requests) acc += tl.callRaw(req).length;
  sink += acc;
}));
report('wasm-call', 'decode_corpus', ops, check, measure(() => {
  let acc = 0;
  for (let n = 0; n < opt.iters; n++) for (const req of objects) acc += tl.call(req).ok ? 1 : 2;
  sink += acc;
}));

// ---- block_parse ----
const blockChecks = [];
const blockRequests = blocks.map(([name, hex]) => {
  const r = tl.call({ op: 'block_parse', hex });
  assert.equal(r.ok, true, name);
  assert.equal(r.merkleMatchesHeader, true, name);
  blockChecks.push(`${name}:${r.hash}:${r.txCount}`);
  return JSON.stringify({ op: 'block_parse', hex });
});
const blockOps = blocks.length * opt['block-iters'];
report('wasm-raw', 'block_parse', blockOps, blockChecks.join(';'), measure(() => {
  let acc = 0;
  for (let n = 0; n < opt['block-iters']; n++) for (const req of blockRequests) acc += tl.callRaw(req).length;
  sink += acc;
}));
if (sink === -1) console.log(sink);
