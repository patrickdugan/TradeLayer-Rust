#!/usr/bin/env node
"use strict";

// Execute complete, unchanged tally.js with asynchronous NeDB-shaped adapters.
// Adapters deliberately omit LevelDB get/put and do not repair missing awaits.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const SOURCE_SHA = "cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef";
const ORDER_SHA = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";
const source = readPinnedSource(path.join(fixtures, "legacy_tally_source.txt"), SOURCE_SHA);
const orderSource = readPinnedSource(path.join(fixtures, "legacy_orderbook_source.txt"), ORDER_SHA);
const BN = loadBigNumber();
const rows = [], quirks = [];
const out = (...fields) => rows.push(fields.join("\t"));
const quirk = (key, actual, expected) => {
  assert.equal(actual, expected, key); quirks.push(`${key}\t${actual}`);
};
const atoms = x => new BN(x || 0).times(1e8).toFixed();
const tokens = x => new BN(x).div(1e8).toFixed();
const csv = values => values.map(String).join(",");
const fields = ["amount", "available", "reserved", "margin", "vesting", "channelBalance"];
const tally = values => Object.fromEntries(fields.map((f, i) => [f, values[i]]));
const encode = obj => csv(fields.map(f => obj[f]));

function load(options = {}) {
  const stores = new Map(), deposits = [], journal = [];
  let uuid = 0, feeWrites = 0;
  const copy = x => x == null ? x : JSON.parse(JSON.stringify(x));
  const db = name => {
    if (!stores.has(name)) stores.set(name, new Map());
    const map = stores.get(name);
    return {
      findOneAsync: async q => {
        if (options.failRead === name) throw Error("injected read");
        return copy(map.get(q._id) ?? null);
      },
      updateAsync: async (q, update) => {
        if (options.failWrite === name || name === "feeCache" && ++feeWrites === options.failFeeWrite) throw Error("injected write");
        map.set(q._id, copy(update.$set ? { ...map.get(q._id), _id: q._id, ...update.$set } : update));
        journal.push(`save/${name}/${q._id}`);
      },
      insertAsync: async doc => { map.set(doc._id, copy(doc)); journal.push(`insert/${name}/${doc._id}`); },
      find: (q, cb) => cb(null, [...map.values()].filter(doc =>
        Object.entries(q).every(([key, value]) => key.split(".").reduce((o, k) => o?.[k], doc) === value))),
      findAsync: async () => [...map.values()].map(copy),
    };
  };
  const database = { getDatabase: async name => db(name) };
  const insurance = { getInstance: async (contract, oracle) => ({ deposit: async (property, amount, block) => {
    if (options.failInsurance) throw Error("injected insurance");
    deposits.push({ contract, oracle, property, amount, block });
  } }) };
  const sandbox = { module: { exports: {} }, console: { log() {}, error() {}, warn() {} }, require: name => {
    if (name === "bignumber.js") return loadBigNumber();
    if (name === "uuid") return { v4: () => `u${++uuid}` };
    if (name === "./db.js") return database;
    if (name === "./property.js") return { getPropertyData: async () => options.nullProperty ? null : ({ ticker: "TL", totalInCirculation: 100 }), getPropertyIndex: async () => new Map([[1, {}]]) };
    if (name === "./contractRegistry.js") return { isNativeContract: async id => id === "1" || id === "7" };
    if (name === "./insurance.js") return insurance;
    if (name === "./orderbook.js") return { getOrderbookInstance: async key => ({
      orderBooks: { [key]: { buy: [], sell: [{}] } },
      calculatePrice: () => 1,
      insertOrder: async order => { journal.push("order/insert"); return order; },
      matchTokenOrders: async () => {
        journal.push("order/match");
        if (options.failMatch) throw Error("injected match");
        return { matches: [] };
      },
      processTokenMatches: async () => journal.push("order/process"),
      saveOrderBook: async () => journal.push("order/save"),
    }) };
    if (name === "./txUtils.js") return {};
    throw Error(`unrecognized dependency ${name}`);
  } };
  vm.runInNewContext(source, sandbox, { filename: "pinned-tally.js", timeout: 1000 });
  const T = sandbox.module.exports;
  const seed = balances => {
    const data = balances.map((b, i) => [`a${i}`, { 1: b }]);
    db("tallyMap"); stores.get("tallyMap").set("tallyMap", { _id: "tallyMap", block: 10, data: JSON.stringify(data) });
  };
  const feeRow = key => stores.get("feeCache")?.get(key) ?? { value: 0, stash: 0 };
  const insuranceAtoms = () => deposits.reduce((a, p) => a.plus(atoms(p.amount)), new BN(0)).toFixed();
  return { T, seed, db, stores, journal, deposits, feeRow, insuranceAtoms };
}

// The method is extracted verbatim, including all comments and statement order,
// from the complete SHA-pinned orderbook archive shared with orderbook vectors.
const feeStart = orderSource.indexOf("calculateFee({", orderSource.indexOf("* calculateFee"));
assert(feeStart > 0);
const bodyStart = orderSource.indexOf(") {", feeStart) + 2;
let depth = 1, end = bodyStart + 1;
for (; depth && end < orderSource.length; end++) {
  if (orderSource[end] === "{") depth++;
  if (orderSource[end] === "}") depth--;
}
const feeMethod = orderSource.slice(feeStart, end);
assert(feeMethod.endsWith("}"));
const feeContext = { BigNumber: loadBigNumber(), module: { exports: {} } };
vm.runInNewContext(`class FeeReference { ${feeMethod} }; module.exports = new FeeReference();`, feeContext, { timeout: 1000 });
const reference = feeContext.module.exports;

async function main() {
  const buckets = [
    [0, 0, 0, 0, 0], [1, 2, 3, 900, 4],
    ["0.000000005", "0.000000005", "0.0000000049", 1000, "0.000000005"],
    ["-0.000000005", "0.000000004", "1.000000005", 0, 0],
    ["9007199254740993", "0.123456789", 0, 99, 0],
    ["0.10000000000000001", "0.2", "0.3", 8, "0.4"],
  ];
  for (const b of buckets) {
    const { T } = load();
    const obj = tally([0, ...b]);
    out("TOTAL", csv(b), T.calculateTotal(obj));
  }
  for (const b of buckets) for (const c of buckets.slice(0, 4)) {
    const x = ["123.000000005", ...b], y = ["0.000000005", ...c];
    const { T, seed } = load(); seed([tally(x), tally(y)]);
    out("PROPERTY", csv(b), csv(c), (await T.getTotalForProperty(1)).toFixed());
    out("TALLY", csv(x), csv(y), encode(await T.getTotalTally(1)));
  }
  for (const b of [[0, 0], [0.1, 0.2], [1e16, 1], [0.00000001, 0.00000002]]) {
    const { T } = load(); const instance = new T();
    instance.addresses.set("a", { 1: { available: b[0], reserved: b[1] } });
    instance.addresses.set("b", { 1: { available: b[1], reserved: b[0] } });
    out("TOKENS", csv(b), instance.totalTokens(1));
  }
  for (const [before, change] of [
    [[10, 2, 3, 4, 99, 1], [1, -1, 2, -3]],
    [[10, 10, 0, 0, 0, 0], [1, -1, 0, 0]],
    [[10, 10, 2, 0, 0, 0], [1, 1, -1, 0]],
    [[10, 10, 2, 3, 0, 0], [1, 1, 1, -1]],
    [[10, 10, 2, 3, 4, 0], [-11, 0, 0, 0]],
    [[0, "9007199254740993", 0, 0, 0, 0], ["1.0000000000000001", 0, 0, 0]],
    [[0, 0, 0, 0, 0, 0], ["0.000000005", "0.000000005", 0, "0.123456789"]],
    [[0, "0.10000000000000001", 0, 0, 0, 0], ["0.2", 0, 0, 0]],
    [[10, 1, 2, 3, 99, 4], [0, 0, 0, 0]],
  ]) {
    const { T, seed, stores } = load(); seed([tally(before)]);
    let rejected = "none";
    try { await T.updateBalance("a0", 1, ...change, "test", 11); }
    catch (e) { rejected = /Available/.test(e.message) ? "available" : /Reserved/.test(e.message) ? "reserved" : /Margin/.test(e.message) ? "margin" : /Vesting/.test(e.message) ? "vesting" : e.name; }
    out("UPDATE", csv(before), csv([...change, 0]), encode(T.addresses.get("a0")[1]), rejected, stores.get("tallyMapDelta")?.size ?? 0);
  }
  for (const [before, change] of [
    [[10, 1, 2, 3, 100, 4], 1], [[10, 1, 2, 3, 100, 4], -5],
    [[0, 0, 0, 0, 0, 0], 0.000000005],
    [[0, 0, 0, 0, 0, "9007199254740993"], "1.0000000000000001"],
    [[0, 1, 0, 0, 0, 0], 0],
  ]) {
    const { T, seed, stores } = load(); seed([tally(before)]); let rejected = "none";
    try { await T.updateChannelBalance("a0", 1, change, "test", 11); } catch (e) { rejected = e.name; }
    out("CHANNEL", csv(before), change, encode(T.addresses.get("a0")[1]), rejected, stores.get("tallyMapDelta")?.size ?? 0);
  }

  // 5bps formula with 20dp division, integer floor, odd-up-even, and maker flags.
  for (const inverse of [false, true]) for (const channel of [false, true]) {
    for (const [buyMaker, sellMaker] of [[false, false], [false, true], [true, false], [true, true], [undefined, true], [undefined, undefined]]) {
      for (const [amountBuy, amountSell, lastMark, notionalValue] of [
        [1, 2, 100, 10], ["0.0000007", "0.0000003", 1, 1],
        [3, 7, "0.123456789123456789", "0.333333333333333333"],
        ["9007199254740993", "1.00000001", 7, 3],
        [-1, -3, "0.00000001", 9],
      ]) {
        const fee = reference.calculateFee({ amountBuy, amountSell, buyMaker, sellMaker, isInverse: inverse, lastMark, notionalValue, channel });
        const buyer = fee.buyerFee.times(1e8), seller = fee.sellerFee.times(1e8);
        out("MATCH", amountBuy, amountSell, String(buyMaker), String(sellMaker), inverse, lastMark, notionalValue, channel, buyer.toFixed(), seller.toFixed(), buyer.plus(seller).toFixed());
      }
    }
  }
  for (const amount of ["-1.5", "-0.000000009", "0", "0.0000000049", "0.000000009", "0.00000001", "0.000000015", "0.00000002", "0.00000003", "1.00000001"]) {
    for (const [property, contract, route] of [[1, null, "spot"], [2, null, "spot"], [1, 7, "native"], [2, 9, "oracle"]]) {
      const { T, feeRow, insuranceAtoms } = load(); let error = "none";
      try { await T.accrueFee(property, amount, contract, 11); } catch (e) { error = e.name; }
      const r = feeRow(`${property}-${contract ?? 1}`);
      out("ACCRUE", property, route, contract ?? 1, amount, atoms(r.value), atoms(r.stash), insuranceAtoms(), error);
    }
  }
  for (const property of [1, 2]) for (const amount of ["0.0000000049", "0.000000005", "0.000000015", "0.00000002", "0.00000003"]) {
    const { T, feeRow, insuranceAtoms } = load();
    await T.updateFeeCache(property, amount, null, 11); await T.flushQueuedFeeAccruals(11);
    const r = feeRow(`${property}-1`);
    out("QUEUE", property, amount, atoms(r.value), atoms(r.stash), insuranceAtoms());
  }
  for (const [value, stash, max, allow] of [[2, 7, 5, true], [0, 7, 5, true], [0, 7, 5, false], [2, 7, null, true], [0, 7, null, true], [2, 7, 0, true]]) {
    const { T, db, feeRow } = load();
    await T.saveFeeRow(db("feeCache"), "1-1", { value: tokens(value), stash: tokens(stash), contract: "1" });
    const result = await T.drawOnFeeCache(1, 1, { max: max == null ? null : tokens(max), allowStash: allow });
    const r = feeRow("1-1"); out("DRAW", value, stash, max ?? "none", allow, atoms(result.spent), atoms(r.value), atoms(r.stash));
  }

  // Concrete source defects and I/O behavior, asserted at generation time.
  {
    const { T, seed } = load(); seed([tally([10, 10, 0, 0, 0, 0])]);
    const instance = await T.getInstance();
    quirk("split_instance_maps", `${T.addresses.size},${instance.addresses.size},${instance.totalTokens(1)}`, "1,0,0");
    quirk("missing_tally", String(await T.getTally("absent", 1)), "0");
    quirk("missing_await_addresses", (await T.getAddressesWithBalanceForProperty(1)).length, 0);
    quirk("empty_load_deltas", String(await instance.loadDeltasForBlock(10)), "undefined");
    let error; try { await instance.applyDeltasSinceLastHeight(0); } catch (e) { error = e.name; }
    quirk("undefined_replay_height", error, "ReferenceError");
    instance.addresses.set("a", { 1: tally([1, 1, 0, 0, 0, 0]) });
    try { await instance.verifyPropertyIds(); } catch (e) { error = e.name; }
    quirk("undefined_property_index", error, "TypeError");
  }
  {
    const { T, seed, stores } = load(); seed([tally([10, 10, 0, 0, 0, 0])]);
    await T.updateBalance("a0", 1, 1, 0, 0, 0, "clearing", 11);
    const delta = [...stores.get("tallyMapDelta").values()][0].data;
    quirk("unawaited_delta_total", JSON.stringify(delta.total), "null");
    quirk("clearing_profit_type_only", await T.didReceiveClearingProfitThisBlock("a0", 11), true);
    const result = await T.updateBalance("a0", "1", 1, 0, 0, 0, "test", 11);
    quirk("numeric_string_property_returns_error", result.name, "Error");
  }
  {
    const { T, seed } = load({ nullProperty: true }); seed([tally([1, 1, 0, 0, 0, 0])]);
    let error; try { await T.getAddressBalances("a0"); } catch (e) { error = e.name; }
    quirk("unguarded_null_ticker", error, "TypeError");
  }
  {
    const { T, seed, stores } = load({ failWrite: "tallyMap" }); seed([tally([10, 10, 0, 0, 0, 0])]);
    await T.updateBalance("a0", 1, 1, 0, 0, 0, "test", 11);
    const disk = JSON.parse(stores.get("tallyMap").get("tallyMap").data)[0][1][1];
    quirk("swallowed_tally_write", `${T.addresses.get("a0")[1].available},${disk.available}`, "11,10");
  }
  {
    const { T, db } = load();
    await db("consensus").updateAsync({ _id: "TrackHeight" }, { _id: "TrackHeight", value: 0 });
    await db("consensus").updateAsync({ _id: "MaxProcessedHeight" }, { _id: "MaxProcessedHeight", value: 12 });
    quirk("zero_track_height_falls_through", await T.resolveBlock(), 12);
    quirk("explicit_zero_height", await T.resolveBlock(0), 0);
  }
  {
    const { T, feeRow } = load({ failFeeWrite: 2 });
    await T.queueFeeAccrual(1, "0.00000002", 7, 11); await T.queueFeeAccrual(2, "0.00000002", 7, 11);
    let error; try { await T.flushQueuedFeeAccruals(11); } catch (e) { error = e.message; }
    quirk("partial_flush_buffer_retained", `${atoms(feeRow("1-7").value)},${T.getFeeAccrualBuffer().get("11").size},${error}`, "2,2,injected write");
    await T.flushQueuedFeeAccruals(11);
    quirk("partial_flush_retry_duplicates", atoms(feeRow("1-7").value), "4");
  }
  {
    const spot = load({ failInsurance: true }); await spot.T.accrueFee(1, "0.00000002", null, 11);
    const oracle = load({ failInsurance: true }); await oracle.T.accrueFee(1, "0.00000002", 9, 11);
    quirk("spot_insurance_failure_drops_stash", atoms(spot.feeRow("1-1").stash), "0");
    quirk("oracle_insurance_failure_keeps_stash", atoms(oracle.feeRow("1-9").stash), "1");
  }
  {
    const { T, seed, stores } = load(); seed([tally([10, 10, 0, 0, 0, 0])]);
    quirk("initialization_flag_absent", await T.checkInitializationFlag(), false);
    await T.setInitializationFlag();
    quirk("initialization_flag_written", await T.checkInitializationFlag(), true);
    await T.loadFromDB(); stores.get("tallyMap").delete("tallyMap"); await T.loadFromDB();
    quirk("missing_checkpoint_clears_map", T.addresses.size, 0);
    quirk("missing_direct_blob", String(await T.loadTallyBlobDirect()), "null");
  }
  {
    const { T } = load({ failRead: "tallyMap" });
    T.addresses = new Map([["old", {}]]); await T.loadFromDB();
    quirk("read_failure_leaves_stale_map", T.addresses.size, 1);
  }
  {
    const { T, db, feeRow } = load();
    await T.saveFeeCacheToDB(1, 1, 23); await T.saveFeeCacheToDB(12, 2, 3);
    quirk("legacy_fee_key_collision", String(feeRow("123").value), "2");
    quirk("legacy_key_excluded_by_property_loader", (await T.loadFeeCacheForProperty(12)).toFixed(), "0");
    await T.adjustFeeCache(1, 1, { valueDelta: "-0.000000015", stashDelta: "0.000000005" });
    const r = await T.loadFeeRow(db("feeCache"), "1-1");
    quirk("negative_adjustment_and_halfup", `${atoms(r.value)},${atoms(r.stash)}`, "-2,1");
  }
  {
    const { T, db } = load();
    await T.saveFeeRow(db("feeCache"), "1-1", { value: "0.00000002", stash: 0, contract: 1 });
    let error; try { await T.drawOnFeeCache(1, 1, { max: "0.000000015" }); } catch (e) { error = e.message; }
    quirk("fractional_draw_missing_put", error, "db.put is not a function");
  }
  {
    const { T, feeRow, journal } = load();
    await T.accrueFee(2, "0.00000003", 7, 11); await T.feeCacheBuy(11);
    quirk("unmatched_buyback_reclassifies_value", `${atoms(feeRow("2-7").value)},${atoms(feeRow("2-7").stash)}`, "0,3");
    quirk("unmatched_buyback_call_order", journal.filter(x => x.startsWith("order/")).join(","), "order/insert,order/match,order/save");
  }
  {
    const { T, feeRow } = load({ failMatch: true });
    await T.accrueFee(2, "0.00000003", 7, 11);
    let error; try { await T.feeCacheBuy(11); } catch (e) { error = e.message; }
    quirk("buyback_match_failure_loses_metadata", `${atoms(feeRow("2-7").value)},${atoms(feeRow("2-7").stash)},${error}`, "0,0,injected match");
  }
  {
    const { T, seed, stores } = load(); seed([tally([1, 1, 0, 0, 100, 0])]);
    await T.applyVesting(2, 1, 11);
    quirk("vesting_missing_await_no_mutation", stores.get("tallyMapDelta")?.size ?? 0, 0);
  }
  const staticNames = Object.getOwnPropertyNames(load().T).filter(n => !["length", "name", "prototype", "instance"].includes(n));
  const instanceNames = Object.getOwnPropertyNames(load().T.prototype).filter(n => n !== "constructor");
  quirk("active_static_method_count", staticNames.length, 37);
  quirk("active_instance_method_count", instanceNames.length, 8);
  const header = `# Generated by scripts/tally_vectors.cjs; full tally SHA256 ${SOURCE_SHA}; orderbook SHA256 ${ORDER_SHA}\n`;
  const files = [["tally_vectors.tsv", header + rows.join("\n") + "\n"], ["tally_quirks.tsv", header + quirks.join("\n") + "\n"]];
  for (const [name, contents] of files) {
    const file = path.join(fixtures, name);
    if (process.argv.includes("--check")) assert.equal(fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n"), contents, name);
    else fs.writeFileSync(file, contents);
  }
  process.stdout.write(`tally: ${rows.length} numeric/state vectors, ${quirks.length} source behavior assertions; full pinned source executed\n`);
}
main().catch(e => { console.error(e); process.exitCode = 1; });
