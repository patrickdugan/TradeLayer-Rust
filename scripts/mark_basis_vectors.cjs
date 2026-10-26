#!/usr/bin/env node
"use strict";
// Full unchanged source classes execute with genuine pinned BigNumber 9.3.1.
// Adapters only collect persistence/dependency calls; no replacement PnL math.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const CLEARING_SHA = "612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528";
const MARGIN_SHA = "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636";
const BN = loadBigNumber();
const trace = [];
const clone = value => JSON.parse(JSON.stringify(value));
let serial = 0;
const db = { async getDatabase(name) { return {
  async updateAsync(query, update) { trace.push(["update", name, clone(query), clone(update)]); },
  async findOneAsync() { return null; },
  async findAsync() { return []; },
  async insertAsync(row) { trace.push(["insert", name, clone(row)]); },
}; } };
const registry = {
  async getContractInfo() { return { notionalValue: 2.5 }; },
  setModFlag(...values) { trace.push(["setModFlag", ...values]); },
};
const consoleAdapter = { log() {}, warn() {}, error() {} };
const inert = name => new Proxy({}, { get(_target, key) { throw Error(`unexpected dependency ${name}.${String(key)}`); } });
function load(filename, sha, dependencies) {
  const module = { exports: {} };
  vm.runInNewContext(readPinnedSource(path.join(fixtures, filename), sha), {
    module, exports: module.exports, console: consoleAdapter,
    require(name) {
      if (name === "bignumber.js") return BN;
      if (name === "./db.js") return db;
      if (name === "./contractRegistry.js") return registry;
      if (Object.hasOwn(dependencies, name)) return dependencies[name];
      throw Error(`unexpected require ${name}`);
    },
  }, { filename: `pinned-${filename}`, timeout: 1000 });
  return module.exports;
}
const MarginMap = load("legacy_margin_map_source.txt", MARGIN_SHA, {
  uuid: { v4() { return `basis-source-${++serial}`; } },
  "./options.js": inert("options"), "./tally.js": inert("tally"),
});
const Clearing = load("legacy_clearing_source.txt", CLEARING_SHA, Object.fromEntries([
  "./tally.js", "./options.js", "./marginMap.js", "./orderbook.js", "./channels.js",
  "./property.js", "./volumeIndex.js", "./oracle.js", "./iou.js", "./tradeHistoryManager.js",
].map(name => [name, inert(name)])));
const rows = [], quirks = [];
const scalar = value => value instanceof BN ? value.toFixed() : String(value);
const add = (kind, input, output) => rows.push([kind, `case-${rows.length}`, input.join(","), scalar(output)].join("\t"));
const quirk = (name, input, output) => quirks.push([name, input, JSON.stringify(output), JSON.stringify(trace)].join("\t"));
function position(quantity = "0.5", average = "100", unrealized = 0) {
  return { address: "alice", contracts: quantity, avgPrice: average, margin: 20, unrealizedPNL: unrealized };
}
async function main() {
  const m = new MarginMap(1);
  for (const inverse of [false, true]) {
    for (const quantity of ["0.1", "-0.1", "0.33333333333333333333", "9007199254740993"]) {
      for (const [from, to, notional] of [["3", "7", "2.5"], ["100", "100.00000001", "1"],
        ["7", "3", "0.00000001"], ["1.00000000000000000001", "1.00000000000000000003", "1"]]) {
        const input = [quantity, from, to, +inverse, notional];
        add("raw", input, Clearing.calculateClearingPNL({ oldContracts: quantity,
          previousMarkPrice: from, currentMarkPrice: to, inverse, notional }));
        add("rounded", input, Clearing.calculateMarkToMarkPNL({ contracts: quantity,
          fromPrice: from, toPrice: to, inverse, notional }));
        add("new", input, Clearing.calculateNewContractPNL({ newContracts: quantity,
          avgEntryPrice: from, lastPrice: to, inverse, notional }));
        add("delta", input, m.calcPriceDelta(quantity, from, to, inverse, notional, true));
      }
    }
  }
  // Round20 reciprocal may legitimately become zero at large finite prices.
  for (const [from, to] of [["1000000000000000000000", "2000000000000000000000"],
    ["0.00000000000000000001", "0.00000000000000000002"]]) {
    add("raw", ["0.3", from, to, 1, "2.5"], Clearing.calculateClearingPNL({
      oldContracts: "0.3", previousMarkPrice: from, currentMarkPrice: to, inverse: true, notional: "2.5" }));
  }
  trace.length = 0;
  const small = { contracts: "0.1", fromPrice: "1", toPrice: "1.00000001", inverse: false, notional: 1 };
  const q8 = Clearing.calculateMarkToMarkPNL(small);
  const raw = Clearing.calculateClearingPNL({ oldContracts: small.contracts,
    previousMarkPrice: small.fromPrice, currentMarkPrice: small.toPrice, inverse: false, notional: 1 });
  assert.equal(raw.toFixed(), "0.000000001"); assert.equal(q8.toFixed(), "0");
  quirk("per-hop-q8-discards-subatom", "q0.1;1->1.00000001;N1", { raw: raw.toFixed(), q8: q8.toFixed() });
  for (const inverse of [false, true]) {
    trace.length = 0;
    const mm = new MarginMap(1), p = position("0.5", "100"); mm.margins.set("alice", p);
    const first = Clearing.calculateMarkToMarkPNL({ contracts: p.contracts,
      fromPrice: "100", toPrice: "110", inverse, notional: 2.5 });
    await mm.clear(p, "alice", first, 100, 1, 24, 110, 100);
    const marked = clone(p);
    await mm.realizePnl("alice", "0.5", "110", "100", inverse, 2.5, p, false, 1, 25);
    quirk(`entry-close-after-mark-${+inverse}`, "entry100;mark110;fullclose110;q0.5;N2.5", { marked, after: p });
  }
  trace.length = 0;
  const repeatMap = new MarginMap(1), repeated = position("0.5", "100");
  repeatMap.margins.set("alice", repeated);
  await repeatMap.clear(repeated, "alice", 5, 100, 1, 24, 110, 100);
  await repeatMap.clear(repeated, "alice", 5, 100, 1, 24, 110, 100);
  assert.equal(repeated.unrealizedPNL, 10);
  quirk("clear-has-no-epoch-idempotence", "same-clear-called-twice", repeated);
  trace.length = 0;
  const settleMap = new MarginMap(1), settled = position("0.5", "100", 7);
  settleMap.margins.set("alice", settled);
  const settledPnl = await settleMap.settlePNL("alice", "0.5", "110", "100", 1, 24, false, 2.5);
  assert.equal(settledPnl, 5);
  quirk("linear-settle-omits-notional", "q0.5;100->110;N2.5", { pnl: settledPnl, position: settled });
  trace.length = 0;
  const normalized = [position("0.5", "100")]; normalized[0].lastMark = 105;
  await Clearing.normalizePositionMarks(normalized, 100, {}, 1, 24);
  assert.equal(normalized[0].lastMark, 100); assert.equal(trace.length, 0);
  quirk("normalize-overwrites-mark-without-settlement", "lastMark105;canonical100", normalized);
  trace.length = 0;
  const zeroNotional = value => Clearing.calculateClearingPNL({ oldContracts: "0.5",
    previousMarkPrice: "100", currentMarkPrice: "110", inverse: false, notional: value }).toFixed();
  quirk("notional-truthy-fallback", "numeric0;string0", [zeroNotional(0), zeroNotional("0")]);
  const zeroInput = { oldContracts: 1, previousMarkPrice: 0, currentMarkPrice: 10, inverse: false, notional: 1 };
  quirk("zero-entry-helper-disagreement", "linear;0->10;q1", {
    raw: Clearing.calculateClearingPNL(zeroInput).toFixed(),
    rounded: Clearing.calculateMarkToMarkPNL({ contracts: 1, fromPrice: 0, toPrice: 10, inverse: false, notional: 1 }).toFixed(),
    new: Clearing.calculateNewContractPNL({ newContracts: 1, avgEntryPrice: 0, lastPrice: 10, inverse: false, notional: 1 }).toFixed(),
  });
  trace.length = 0;
  Clearing.blockTrades.clear();
  const tracked = Clearing.recordTrade(1, "alice", "9007199254740993", 0, 100, "a", true);
  quirk("record-trade-quantity-number-bridge", "open9007199254740993@100", { result: tracked, entry: Clearing.blockTrades.get("1:alice") });
  const header = `# Full unchanged clearing.js ${CLEARING_SHA}; marginMap.js ${MARGIN_SHA}; BigNumber9.3.1\n`;
  for (const [name, contents] of [["mark_basis_vectors.tsv", header + "# kind\tname\tquantity,from,to,inverse,notional\toutput\n" + rows.join("\n") + "\n"],
    ["mark_basis_quirks.tsv", header + "# name\tinput\toutput\tdependency_trace\n" + quirks.join("\n") + "\n"]]) {
    const target = path.join(fixtures, name);
    if (process.argv.includes("--check")) assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), contents);
    else fs.writeFileSync(target, contents);
  }
  console.log(`mark basis: ${rows.length} actual-source vectors; ${quirks.length} quirks`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
