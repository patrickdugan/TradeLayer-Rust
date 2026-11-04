// Runs the benchmark schedule of examples/bench_playground.rs through the wasm and prints the
// same summary JSON, so the two checksums can be compared.
//
//   node bench.mjs <path/to/tradelayer_wasm.wasm> [blocks=1000] [seed=1] [marks]
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { ATOMS, centsText, loadEngine, midCents, mulberry32, quoteOps, toPrice1e4, walkStep } from './engine.mjs';

const [wasmPath, blocksArg, seedArg, marksArg] = process.argv.slice(2);
if (!wasmPath) {
  console.error('usage: node bench.mjs <tradelayer_wasm.wasm> [blocks] [seed] [marks]');
  process.exit(2);
}
const blocks = Number(blocksArg ?? 1000);
const seed = Number(seedArg ?? 1);
const marks = marksArg === 'marks';
const HEIGHT0 = 1_000_000;
const TRADERS = 4;
const MM = { levels: 3, size: 0.5, spreadBps: 5, stepBps: 5 };

const engine = await loadEngine(await readFile(wasmPath));
function call(request) {
  const out = engine.call(request);
  if (!out.ok) throw new Error(`${request.op} failed: ${JSON.stringify(out)}`);
  return out;
}

call({
  op: 'pg_new',
  id: 'bench',
  anchor_height: HEIGHT0,
  price_1e4: 1_000_000_000,
  marks,
  accounts: [
    { name: 'mm', tusd_atoms: 5_000_000 * ATOMS },
    ...Array.from({ length: TRADERS }, (_, t) => ({ name: `t${t}`, tusd_atoms: 500_000 * ATOMS })),
  ],
});

const rng = mulberry32(seed);
let price = 100_000;
const times = [];
let fills = 0;
for (let k = 0; k < blocks; k += 1) {
  price = walkStep(price, rng(), 0.001);
  const price1e4 = toPrice1e4(price);
  const mid = midCents(price1e4);
  const ops = [{ t: 'oracle', price_1e4: price1e4 }, ...quoteOps('mm', price1e4, MM)];
  if (rng() < 0.6) {
    const trader = Math.floor(rng() * TRADERS);
    const buy = rng() < 0.5;
    const qty = `0.${1 + Math.floor(rng() * 3)}`;
    const side = buy ? 'buy' : 'sell';
    if (rng() < 0.5) {
      ops.push({ t: 'order', account: `t${trader}`, side, qty, market: true, price: centsText(mid) });
    } else {
      const px = buy ? Math.floor((mid * 10020) / 10000) : Math.floor((mid * 9980) / 10000);
      ops.push({ t: 'order', account: `t${trader}`, side, qty, price: centsText(px) });
    }
  }
  const start = performance.now();
  const out = call({ op: 'pg_block', session: 'bench', height: HEIGHT0 + k, ops });
  times.push(performance.now() - start);
  fills += out.fill_count;
}

const state = call({ op: 'pg_state', session: 'bench', fills: 0 });
const mm = state.positions.find((p) => p.account === 'mm');
const total = times.reduce((a, b) => a + b, 0);
const sorted = [...times].sort((a, b) => a - b);
const median = sorted[Math.floor(sorted.length / 2)];
const tenth = Math.max(1, Math.floor(times.length / 10));
const avg = (xs) => xs.reduce((a, b) => a + b, 0) / xs.length;
console.log(
  JSON.stringify({
    runtime: `node ${process.version} wasm`,
    blocks,
    seed,
    marks,
    median_ms: Number(median.toFixed(3)),
    mean_ms: Number((total / blocks).toFixed(3)),
    first_tenth_ms: Number(avg(times.slice(0, tenth)).toFixed(3)),
    last_tenth_ms: Number(avg(times.slice(-tenth)).toFixed(3)),
    blocks_per_s_median: Number((1000 / median).toFixed(1)),
    blocks_per_s_total: Number((blocks / (total / 1000)).toFixed(1)),
    fills,
    checksum: {
      fill_total: state.fill_total,
      mm_contracts: mm ? mm.contracts : '0',
      net: state.net_contracts,
      price_1e4: state.oracle.price_1e4,
    },
  }),
);
