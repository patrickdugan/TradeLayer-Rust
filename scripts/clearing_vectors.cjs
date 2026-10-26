#!/usr/bin/env node
"use strict";
// Complete unchanged clearing.js AND contractRegistry.js execute in the VM.
// External adapters select observations and record attempts, not funding math.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const SHA = "612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528";
const REGISTRY_SHA = "d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121";
const args = process.argv.slice(2), sourceAt = args.indexOf("--source");
const source = readPinnedSource(sourceAt < 0 ? path.join(fixtures, "legacy_clearing_source.txt") : args[sourceAt + 1], SHA);
const registrySource = readPinnedSource(path.join(fixtures, "legacy_contract_registry_source.txt"), REGISTRY_SHA);
const BN = loadBigNumber();
const scalar = x => new BN(x).isFinite() ? new BN(x).toFixed() : String(x);
const copy = x => JSON.parse(JSON.stringify(x));
let state, trace, transfers, writes;
function reset(change = {}) {
  state = { native: true, inverse: false, raw: "1", index: "101", average: "100", positions: [], rows: [], ...change };
  trace = []; transfers = []; writes = [];
  Clearing.latestOracleMarkById.clear(); Clearing.latestNativeMarkById.clear();
}
function meta() { return { id: 1, native: state.native, inverse: state.inverse, notionalValue: state.raw,
  underlyingOracleId: 5, notionalPropertyId: 7, collateralPropertyId: 8, expiryPeriod: 0, ticker: "FUND-PERP" }; }
function docs(name) {
  if (name === "contractList") return [{ id: 1, type: "contractSeries", data: meta() }];
  if (name === "volumeIndex") return [{ _id: "7-8", value: { price: state.index === "0" ? 0 : state.index, blockHeight: 24 } }];
  if (name === "oracleData") return state.rows;
  return [];
}
const matches = (row, query = {}) => Object.entries(query).every(([key, value]) => value && typeof value === "object"
  ? Object.entries(value).every(([op, limit]) => op === "$gte" ? row[key] >= limit : op === "$lte" ? row[key] <= limit : false)
  : row[key] === value);
const db = { getDatabase(name) { return {
  async findOneAsync(query) { trace.push(["findOne", name, query]); return copy(docs(name).find(row => matches(row, query)) || null); },
  async findAsync(query) { trace.push(["find", name, query || {}]); return copy(docs(name).filter(row => matches(row, query))); },
  async updateAsync(query, value) {
    trace.push(["write", name, query]);
    if (state.writeError) throw Error("injected event storage failure");
    writes.push([name, copy(value)]);
    if (name === "oracleData") {
      const index = state.rows.findIndex(row => matches(row, query));
      if (index < 0) state.rows.push(copy(value)); else state.rows[index] = copy(value);
    }
  },
  async insertAsync(value) { writes.push([name, copy(value)]); },
}; } };
const oracle = {
  async getTWAP(...values) { trace.push(["TWAP", ...values]); if (state.averageError) throw Error("average unavailable"); return state.average; },
  async getOraclePrice(...values) { trace.push(["oraclePrice", ...values]); return Object.hasOwn(state, "oracleValue") ? state.oracleValue : { price: state.index === "0" ? 0 : state.index, blockHeight: 24 }; },
};
const volume = { async getVWAP(...values) { trace.push(["VWAP", ...values]); if (state.averageError) throw Error("average unavailable"); return state.average; } };
const tally = { async updateBalance(owner, property, available, reserved, margin, vesting, reason, height) {
  trace.push(["balanceAttempt", owner, property, scalar(available), reason, height]);
  if (state.failTransfer === transfers.length + 1) throw Error("injected balance failure");
  transfers.push([owner, property, scalar(available), scalar(reserved), scalar(margin), scalar(vesting), reason, height]);
} };
const margins = { async getInstance() { trace.push(["positions"]); return { async getAllPositions() { return state.positions; } }; } };
const inert = name => new Proxy({}, { get(_target, key) { throw Error(`unexpected ${name}.${String(key)}`); } });
const consoleAdapter = { log() {}, warn() {}, error(...values) { trace.push(["error", ...values.map(x => x instanceof Error ? x.message : String(x))]); } };
const registryBox = { exports: {} };
vm.runInNewContext(registrySource, { module: registryBox, exports: registryBox.exports, console: consoleAdapter,
  require(name) {
    if (name === "bignumber.js") return BN;
    if (name === "./db" || name === "./db.js") return db;
    if (name === "path" || name === "util") return require(name);
    if (name === "./oracle.js") return oracle;
    if (name === "./volumeIndex.js") return volume;
    if (name === "./amm.js") return class AMM {};
    if (["./property.js", "./channels.js"].includes(name)) return inert(name);
    throw Error(`unexpected registry require ${name}`);
  } }, { filename: "pinned-contractRegistry.js", timeout: 1000 });
const Registry = registryBox.exports;
class FixedDate extends Date { constructor(...values) { super(...(values.length ? values : ["2026-10-26T00:00:00.000Z"])); } }
const box = { exports: {} };
vm.runInNewContext(source, { module: box, exports: box.exports, console: consoleAdapter, Date: FixedDate,
  require(name) {
    if (name === "bignumber.js") return BN;
    if (name === "./tally.js") return tally;
    if (name === "./contractRegistry.js") return Registry;
    if (name === "./db.js") return db;
    if (name === "./marginMap.js") return margins;
    if (name === "./oracle.js") return oracle;
    if (name === "./volumeIndex.js") return volume;
    if (["./options.js", "./orderbook.js", "./channels.js", "./property.js", "./iou.js", "./tradeHistoryManager.js"].includes(name)) return inert(name);
    throw Error(`unexpected clearing require ${name}`);
  } }, { filename: "pinned-clearing.js", timeout: 1000 });
const Clearing = box.exports;
const oracleBox = { exports: {} };
vm.runInNewContext(readPinnedSource(path.join(fixtures, "legacy_oracle_source.txt"), "70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d"), {
  module: oracleBox, exports: oracleBox.exports, console: consoleAdapter,
  require(name) {
    if (name === "./db") return db;
    if (name === "bignumber.js") return BN;
    if (name === "crypto") return require("node:crypto");
    if (name === "./dlcOracleBridge.js") return {};
    throw Error(`unexpected oracle average dependency ${name}`);
  },
}, { filename: "pinned-oracle.js", timeout: 1000 });
const staticMethods = Object.getOwnPropertyNames(Clearing).filter(n => typeof Clearing[n] === "function" && !["name", "length", "prototype"].includes(n));
const instanceMethods = Object.getOwnPropertyNames(Clearing.prototype).filter(n => n !== "constructor");
const rows = [], quirks = [];
const add = (kind, name, input, output) => rows.push([kind, name, input, output, JSON.stringify(trace)].join("\t"));
const quirk = (name, input, output) => quirks.push([name, input, output, JSON.stringify(trace)].join("\t"));
async function main() {
  for (const prices of [[], ["99", "101"], ["100", "100", "100.0001"], ["0.00000001", "0", "0"],
    ["0.00000001", "0.00000002"], ["9999999999999999", "10000000000000000"],
    ["1.000000005", "1.000000005"], ["99.9999", "100", "100"], ["1", "2", "2"]]) {
    reset({ rows: prices.map((price, i) => ({ oracleId: 5, blockHeight: i + 1, data: { price } })) });
    const result = await oracleBox.exports.getTWAP(5, 24, 192);
    add("average", `average-${rows.length}`, prices.join(",") || "-", result === null ? "null" : scalar(result));
  }
  const indices = ["98", "99", "99.94995", "99.95", "99.95005", "100", "100.049", "100.04995", "100.05", "100.05005", "100.05015", "100.051", "101", "102", "0"];
  for (const native of [false, true]) for (const index of indices) {
    reset({ native, index });
    add("rate", `rate-${+native}-${index}`, `${index},100`, scalar(await Clearing.calculateFundingRate(1, 24)));
  }
  for (const average of ["3", "0.00000001", "9999999999999999", "0"]) for (const index of ["1", "3.0015005", "10000000000000000"]) {
    reset({ average, index });
    add("rate", `scale-${average}-${index}`, `${index},${average}`, scalar(await Clearing.calculateFundingRate(1, 24)));
  }
  for (const native of [false, true]) for (const inverse of [false, true]) for (const raw of ["1", "2.5", "0.00000001", "9007199254740993"]) for (const index of ["3", "100", "123.456789"]) {
    reset({ native, inverse, raw, index });
    const out = await Registry.getNotionalValue(1, index);
    add("notional", `notional-${+native}-${+inverse}-${raw}-${index}`, [raw, index, +inverse, +!native].join(","), scalar(out.notionalValue));
  }
  for (const quantity of ["0.000001", "0.00001", "0.125", "0.3", "1.25", "100.00000001"]) for (const rate of ["0.0001", "1.2345", "12.5"]) {
    reset({ positions: [{ address: "payer", contracts: quantity }, { address: "receiver", contracts: `-${quantity}` }] });
    await Clearing.applyFundingToPositions(1, rate, 24);
    const debit = transfers.find(row => row[0] === "payer");
    add("payer", `payer-${quantity}-${rate}`, `${quantity},1,${rate}`, debit ? scalar(new BN(debit[2]).negated()) : "0");
  }
  reset({ positions: [{ address: "long", contracts: "0.3" }, { address: "short", contracts: "-0.3" }] });
  await Clearing.applyFundingToPositions(1, -10, 24);
  quirk("negative-sign-reversed", "rate=-10;long0.3;short-0.3", JSON.stringify(transfers));
  reset();
  await Clearing.processFundingPayments([{ address: "payer", contracts: 1 }], [{ address: "a", contracts: -0.5 }, { address: "b", contracts: -0.5 }], "0.00000001", 1, 24);
  quirk("independent-rounding-mints-atom", "one-atom;two-equal-receivers", JSON.stringify(transfers));
  reset();
  await Clearing.processFundingPayments([{ address: "payer", contracts: 1 }], [], "0.00000001", 1, 24);
  quirk("missing-receiver-debit-prefix", "one-atom;no-receivers", JSON.stringify(transfers));
  reset({ failTransfer: 2 });
  try { await Clearing.processFundingPayments([{ address: "a", contracts: 1 }, { address: "b", contracts: 1 }], [{ address: "r", contracts: -2 }], "1", 1, 24); }
  catch (error) { quirk("dependency-failure-after-prefix", error.message, JSON.stringify(transfers)); }
  for (const height of [23, 24, 25, 48]) {
    reset({ positions: height === 48 ? [] : [{ address: "long", contracts: 1 }, { address: "short", contracts: -1 }] });
    await Clearing.applyFundingRates(height);
    quirk(`cadence-${height}`, `height=${height}`, JSON.stringify({ transfers, writes }));
  }
  reset({ index: "100", positions: [] }); await Clearing.applyFundingRates(24);
  quirk("zero-rate-no-event", "index=average=100", JSON.stringify(writes));
  reset({ writeError: true, positions: [] }); await Clearing.applyFundingRates(24);
  quirk("event-write-error-swallowed", "nonzero-rate;empty-positions", JSON.stringify(writes));
  reset({ averageError: true });
  quirk("average-error-rate-zero", "VWAP-failure", scalar(await Clearing.calculateFundingRate(1, 24)));
  reset({ native: false, rows: [{ oracleId: 5, blockHeight: 50, data: { price: 150 } }] });
  quirk("cold-cache-primes-future-row", "request24;row50", JSON.stringify(await Clearing.isPriceUpdatedForBlockHeight(1, 24)));
  for (const value of [101, { price: 101, blockHeight: 50 }]) {
    reset({ native: false, oracleValue: value });
    const out = await Clearing.getIndexPrice(1, 24);
    quirk(typeof value === "number" ? "numeric-oracle-without-height" : "future-object-rejected", JSON.stringify(value), String(out));
  }
  reset({ native: false, oracleValue: { price: "0", blockHeight: 24 } });
  quirk("truthy-string-zero-index", "object.price=string0", scalar(await Clearing.calculateFundingRate(1, 24)));
  reset();
  const raw = Clearing.calculateClearingPNL({ oldContracts: "0.1", previousMarkPrice: "1", currentMarkPrice: "1.00000001", inverse: false, notional: 1 });
  const rounded = Clearing.calculateMarkToMarkPNL({ contracts: "0.1", fromPrice: "1", toPrice: "1.00000001", inverse: false, notional: 1 });
  quirk("raw-mark-pnl-differs-from-q8-helper", "q0.1;1->1.00000001;N1", `${scalar(raw)},${scalar(rounded)}`);
  reset({ native: false });
  oracleBox.exports.instance = null; oracleBox.exports.lastOracleUpdateBlock.clear();
  const directOracle = oracleBox.exports.getInstance();
  await directOracle.saveOracleData(5, { price: 100 }, 2);
  await directOracle.saveOracleData(5, { price: 200 }, 2);
  const average = await oracleBox.exports.getTWAP(5, 24, 192);
  state.average = average;
  state.oracleValue = await oracleBox.exports.getOraclePrice(5);
  const sameHeightRate = await Clearing.calculateFundingRate(1, 24);
  assert.equal(state.rows.length, 1); assert.equal(average, 200); assert.equal(sameHeightRate, 0);
  quirk("direct-save-same-height-upserts-final-row", "save100@2;save200@2;TWAP@24", JSON.stringify({ rows: state.rows, average, rate: sameHeightRate, writes }));
  reset(); oracleBox.exports.instance = null; oracleBox.exports.lastOracleUpdateBlock.clear();
  await oracleBox.exports.publishData(5, 100, 100, 100, 100, 2);
  await oracleBox.exports.publishData(5, 200, 200, 200, 200, 2);
  const firstAverage = await oracleBox.exports.getTWAP(5, 24, 192);
  assert.equal(state.rows.length, 1); assert.equal(firstAverage, 100);
  quirk("high-level-publish-same-height-first-update-wins", "publish100@2;publish200@2;TWAP@24", JSON.stringify({ rows: state.rows, average: firstAverage, writes }));
  reset({ writeError: true }); oracleBox.exports.instance = null; oracleBox.exports.lastOracleUpdateBlock.clear();
  try { await oracleBox.exports.publishData(5, 100, 100, 100, 100, 2); } catch (error) { assert.equal(error.message, "injected event storage failure"); }
  state.writeError = false;
  await oracleBox.exports.publishData(5, 200, 200, 200, 200, 2);
  assert.equal(state.rows.length, 0);
  quirk("high-level-publish-first-attempt-guard-survives-write-failure", "failed100@2;retry200@2", JSON.stringify({ rows: state.rows, guardedHeight: oracleBox.exports.lastOracleUpdateBlock.get(5), writes }));
  const header = `# Full unchanged clearing.js ${SHA}; registry ${REGISTRY_SHA}; oracle average 70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d\n# active static methods(${staticMethods.length}): ${staticMethods.join(",")}\n# active instance methods(${instanceMethods.length}): ${instanceMethods.join(",")}\n`;
  const outputs = [["clearing_vectors.tsv", header + "# kind\tname\tinput\toutput\tdependency_trace\n" + rows.join("\n") + "\n"],
    ["clearing_quirks.tsv", header + "# name\tinput\toutput\tdependency_trace\n" + quirks.join("\n") + "\n"]];
  for (const [name, text] of outputs) {
    const target = path.join(fixtures, name);
    if (args.includes("--check")) assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), text);
    else fs.writeFileSync(target, text);
  }
  console.log(`clearing: ${rows.length} actual-source vectors; ${quirks.length} quirks; active methods ${staticMethods.length}+${instanceMethods.length}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
