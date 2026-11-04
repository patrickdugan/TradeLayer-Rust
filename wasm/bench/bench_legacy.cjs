"use strict";
// Legacy tradelayer.js txDecoder benchmark. Same corpus, iteration count and run protocol as
// examples/bench_codec.rs and bench_wasm.mjs.
//
//   node wasm/bench/bench_legacy.cjs [--iters N] [--runs R] [--warmup W]
//
// There is no block workload: the pinned legacy txIndex.js fetches blocks through the Core RPC
// client (client.getblock) and contains no raw-block, txid or merkle code to run offline.
const assert = require("node:assert/strict");
const { loadLegacyDecoder, loadCorpus, tagged } = require("./legacy_loader.cjs");

const opt = { iters: 20, runs: 7, warmup: 3 };
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i += 2) {
  const key = argv[i].replace(/^--/, "");
  if (!(key in opt)) throw new Error("bad argument " + argv[i]);
  opt[key] = Number(argv[i + 1]);
}

const decoder = loadLegacyDecoder();
const rows = loadCorpus();

// Verification (untimed): decode + tagged JSON equals the fixture, throws match name and message.
let ok = 0, thrown = 0, bytes = 0;
for (const row of rows) {
  let out;
  try {
    out = JSON.stringify(tagged(decoder[row.method](row.body)));
  } catch (error) {
    assert.equal(row.ok, false, row.method);
    assert.equal(error.name, row.name);
    // The fixture stores messages as UTF-8, which turns a lone surrogate into U+FFFD.
    assert.equal(Buffer.from(error.message).toString("utf8"), row.message);
    thrown++;
    continue;
  }
  assert.equal(row.ok, true, row.method);
  assert.equal(out, row.expected, row.method);
  ok++;
  bytes += Buffer.byteLength(out);
}
const check = `ok=${ok},throw=${thrown},bytes=${bytes}`;
const ops = rows.length * opt.iters;

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

function report(runtime, workload, times) {
  const sorted = [...times].sort((a, b) => a - b);
  console.log(
    JSON.stringify({
      runtime,
      workload,
      ops,
      check,
      median_ms: Number(sorted[Math.floor(sorted.length / 2)].toFixed(3)),
      min_ms: Number(sorted[0].toFixed(3)),
      runs_ms: times.map((t) => Number(t.toFixed(3))),
    }),
  );
}

console.log(JSON.stringify({ meta: { node: process.version, rows: rows.length, ...opt } }));

let sink = 0;

// decode + tagged JSON.stringify: the same output the Rust side produces.
report("legacy-js", "decode_corpus", measure(() => {
  let acc = 0;
  for (let n = 0; n < opt.iters; n++) {
    for (const row of rows) {
      try {
        acc += JSON.stringify(tagged(decoder[row.method](row.body))).length;
      } catch {
        acc += 1;
      }
    }
  }
  sink += acc;
}));

// decode only, no serialization (lower bound for the JS side).
report("legacy-js-decode-only", "decode_corpus", measure(() => {
  let acc = 0;
  for (let n = 0; n < opt.iters; n++) {
    for (const row of rows) {
      try {
        decoder[row.method](row.body);
        acc++;
      } catch {
        acc += 2;
      }
    }
  }
  sink += acc;
}));
if (sink === -1) console.log(sink);
