#!/usr/bin/env node
"use strict";

// Execute complete unmodified, SHA-pinned amm.js with controlled dependencies.
// No database, network, wallet or chain process runs. The global-txid token
// experiment is separate from the ordinary entry point, which places nothing.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SOURCE_SHA256 = "bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1";
const args = process.argv.slice(2);
const sourceIndex = args.indexOf("--source");
const sourcePath = sourceIndex < 0 ? path.join(fixtures, "legacy_amm_source.txt") : args[sourceIndex + 1];
if (!sourcePath) throw new Error("--source requires a path");
const code = readPinnedSource(sourcePath, SOURCE_SHA256);
const n = value => Object.is(value, -0) ? "-0" : String(value);
const rows = [`# Unmodified amm.js ${SOURCE_SHA256}; source dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, "# P pools; A/R capital journal; Q quotes; S size; V redemption; G contract curve; T strategy; F Number.toFixed(8); X token curve with supplied global txid; K source observations"];
function add(...fields) { rows.push(fields.map(value => typeof value === "number" ? n(value) : value).join("\t")); }

function load({ globalTxid, registry = {}, db = {}, failTallyAt = 0 } = {}) {
  const movements = [], tokenOrders = [], errors = [], events = [];
  const book = {
    async addTokenOrder(order, block, txid) { tokenOrders.push({ ...order, block, txid }); },
    async cancelOrdersByCriteria(...params) { events.push(["cancel", ...params]); },
    async loadOrderBook(key, isToken) { events.push(["load", key, isToken]); return { inserted: [] }; },
    async insertOrder(order, data, sell, isToken) { events.push(["insert", order.txid, sell, isToken]); data.inserted.push(order); return data; },
    async saveOrderBook(data, key) { events.push(["save", key, data.inserted.length]); },
  };
  const dependencies = {
    "./contractRegistry.js": registry,
    "./orderbook.js": { async getOrderbookInstance(key) { events.push(["book", key]); return book; } },
    "./clearing.js": { async getIndexPrice(id) { return id === 5 ? 0 : 100; } },
    "./db.js": db,
    "./tally.js": { async updateBalance(...params) { if (movements.length + 1 === failTallyAt) throw new Error("controlled tally failure"); movements.push(params); } },
  };
  const module = { exports: {} };
  const context = { module, exports: module.exports, Date: { now: () => 1000 }, console: { log() {}, error(...params) { errors.push(params.map(String).join(" ")); } }, require(name) { assert.ok(Object.hasOwn(dependencies, name), `unexpected dependency ${name}`); return dependencies[name]; } };
  if (globalTxid !== undefined) context.txid = globalTxid;
  vm.runInNewContext(code, context, { filename: "pinned-amm.js", timeout: 1000 });
  return { AMM: module.exports, movements, tokenOrders, errors, events };
}
function journal(movements) {
  return movements.map(([owner, property, amount, a, b, c, kind, block]) => {
    assert.equal(a, 0); assert.equal(b, 0); assert.equal(c, 0);
    assert.equal(typeof owner, "string"); assert.equal(typeof block, "number");
    return `${property}:${n(amount)}:${kind}`;
  }).join(";");
}
async function errorOf(action) {
  try { await action(); return "success"; } catch (error) { return `${error.name}:${error.message}`; }
}
async function main() {
  const { AMM, movements } = load();
  const pools = new Map();
  for (const [label, initial, max, quote, cid] of [["c", 0, 100, 10, 7], ["t", 0, 100, 5, 8], ["fraction", 1, 10, 2, 9], ["keys", 0, 100, 5, 10]]) {
    pools.set(label, new AMM(initial, max, quote, cid)); add("P", label, initial, max, quote, cid);
  }
  for (const [op, pool, owner, id, amount, contract, id2, amount2, block] of [
    ["A", "c", "alice", 1, 10, true, 0, 0, 1], ["A", "c", "bob", 1, 20, true, 0, 0, 2],
    ["A", "c", "alice", 1, 5, true, 0, NaN, 3], ["R", "c", "alice", 1, 3, true, 0, 0, 4],
    ["R", "c", "alice", 1, 12, true, 0, 0, 5], ["R", "c", "bob", 1, 20, true, 0, 0, 6],
    ["A", "t", "alice", 1, 5, false, 2, 7, 7], ["A", "t", "bob", 1, 3, false, 2, 11, 8],
    ["R", "t", "alice", 1, 2, false, 2, 1, 9], ["R", "t", "alice", 1, 3, false, 2, 6, 10],
    ["A", "fraction", "alice", 2, 0.1, true, 0, 0, 11], ["A", "fraction", "alice", 2, 0.2, true, 0, 0, 12],
    ["R", "fraction", "alice", 2, 0.1, true, 0, 0, 13],
    ["A", "keys", "z", 1, 1, true, 0, 0, 14], ["A", "keys", "10", 1, 2, true, 0, 0, 15],
    ["A", "keys", "2", 1, 3, true, 0, 0, 16], ["A", "keys", "01", 1, 4, true, 0, 0, 17],
  ]) {
    const p = pools.get(pool), before = movements.length;
    await p[op === "A" ? "addCapital" : "redeemCapital"](owner, id, amount, contract, id2, amount2, block);
    add(op, pool, owner, id, amount, Number(contract), id2, amount2, block, p.position, p.lpAddresses[owner] || 0, journal(movements.slice(before)), Object.keys(p.lpAddresses).join(","));
  }
  const queryPool = pools.get("keys");
  for (const owner of [null, "2", "missing"]) add("H", "keys", owner === null ? "*" : owner, AMM.calculateTotalPosition.call({ lpShares: queryPool.lpAddresses, maxPosition: queryPool.maxPosition }, owner));
  for (const owner of [null, "missing"]) add("H", "emptyzero", owner === null ? "*" : owner, AMM.calculateTotalPosition.call({ lpShares: {}, maxPosition: 0 }, owner));
  for (const owner of [null, "missing"]) add("H", "emptypositive", owner === null ? "*" : owner, AMM.calculateTotalPosition.call({ lpShares: {}, maxPosition: 100 }, owner));
  assert.equal(AMM.getLPAddresses.call(queryPool).join(","), "2,10,z,01");
  const snapshot = AMM.getAMMOrdersAndPositions.call(queryPool);
  assert.equal(snapshot.position, queryPool.position); assert.equal(snapshot.lpAddresses, queryPool.lpAddresses); assert.equal(snapshot.orders, queryPool.ammOrders);
  add("K", "bound_address_and_position_queries", "same pool fields and references;numeric keys precede string keys");
  let countQ = 0;
  for (const [position, maxQuote, mid, spread, block] of [
    [10, 4, 100, 40, 1], [2, 4, 0.00000001, 40, 2], [10, 4, 100, 0, 3], [10, 4, 100, -5, 4],
    [10, 4, 100, 10000, 5], [10, 4, 100, 15000, 6], [0, 4, 100, 40, 7], [-2, 4, 100, 40, 8],
    [10, 0, 100, 40, 9], [10, 4, 0, 40, 10], [10, 4, -100, 40, 11], [10, 4, Infinity, 40, 12],
    [10, 4, NaN, 40, 13], [10, 4, 123.123456789, 17, 14], [10, 4, 1e22, 40, 15],
    [10, 4, 100, NaN, 16], [10, 4, 100, Infinity, 17],
  ]) {
    const p = new AMM(position, 100, maxQuote, 7), quotes = p.quoteContractOrders(mid, block, spread);
    for (const quote of quotes) {
      assert.equal(quote.sender, "amm"); assert.equal(quote.initMargin, 0);
      for (const flag of ["isLiq", "reduce", "post", "stop", "initialReduce"]) assert.equal(quote[flag], false);
    }
    add("Q", position, maxQuote, mid, spread, block, quotes.length, ...quotes.flatMap(q => [q.amount, q.price, q.sell ? "sell" : "buy", q.txid])); countQ++;
  }
  for (const [distance, priceDistance, total] of [[0.001, 1, 100], [0.01, 1, 100], [0.02, 1, 100], [0.03, 1, 100], [0.031, 1, 100], [0, 0, 100], [-1, 1, 100], [1, -1, 100], [1, 10, -100]]) add("S", distance, priceDistance, total, AMM.calculateOrderSize(distance, priceDistance, total));
  for (const [amount, contract, tokens, collateral, price, last] of [[2, true, 10, 3, 4, 999], [2, false, 10, 3, 4, 11], [2, true, 0, 3, 4, 5], [0, true, 0, 3, 4, 5], [-2, true, 10, 3, 4, 5], [0.1, true, 0.3, 0.2, 0.1, 1]]) add("V", amount, Number(contract), tokens, collateral, price, last, AMM.calculateRedemptionValue(amount, contract, { tokens, collateralId: collateral, price }, last));
  for (const type of ["linear", "inverse"]) for (const position of [10, 0, -3]) for (const priceDistance of [1, 0, -1]) {
    const p = new AMM(position, 100, 4, type);
    p.calculateOrderSize = AMM.calculateOrderSize;
    const ord = AMM[type === "linear" ? "generateOrdersForLinear" : "generateOrdersForInverse"].call(p, 100, priceDistance, 100);
    add("G", type, position, 4, 100, priceDistance, 100, p.position, ord.price, ord.size, ord.side);
  }
  for (const type of ["straddle", "ironFly", "calendar", "unknown"]) for (const price of [100, -10, 0.00000001]) {
    const receiver = { maxQuoteSize: 3, strategyBlob: JSON.stringify({ type }), buildStraddle: AMM.buildStraddle, buildIronFly: AMM.buildIronFly, buildCalendar: AMM.buildCalendar };
    const orders = await AMM.runOptionStrategy.call(receiver, price, 0.4, 100);
    add("T", type, price, 100, 3, orders.map(o => [o.side, o.type, n(o.strike), n(o.expiry), n(o.size)].join(",")).join(";"));
  }
  for (const value of [0, -0, 0.001953125, -0.001953125, 0.005859375, -0.005859375, 1.000000005, -1.000000005, 1.234567895, 2.675, Number.MIN_VALUE, -Number.MIN_VALUE, 1e21, 1e22, Number.MAX_VALUE, Infinity, -Infinity, NaN]) add("F", value, Number(value.toFixed(8)));
  let seed = 0x6d2b79f5;
  function random32() { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return seed >>> 0; }
  for (let index = 0; index < 128; index++) {
    const bytes = Buffer.alloc(8); bytes.writeUInt32LE(random32(), 0); bytes.writeUInt32LE(random32(), 4);
    const value = bytes.readDoubleLE();
    add("F", value, Number(value.toFixed(8)));
  }
  // Source curve arithmetic becomes observable only if the missing global is supplied.
  for (const [liquidity, total, last] of [[100, 1, 10], [100, 4, 10], [0.1, 3, 123.456], [100, 0, 10]]) {
    const test = load({ globalTxid: "caller-supplied-txid" });
    await test.AMM.generateTokenOrders(1, 2, liquidity, total, last, 50);
    assert.equal(test.errors.length, 0);
    add("X", liquidity, total, last, test.tokenOrders.map(o => [o.offeredPropertyId, o.desiredPropertyId, n(o.amountOffered), n(o.amountExpected), n(o.price)].join(",")).join(";"));
  }
  // Ordinary broken source paths are evidence, not silently repaired execution.
  const broken = load();
  await broken.AMM.generateTokenOrders(1, 2, 100, 3, 10, 50);
  assert.equal(broken.tokenOrders.length, 0); assert.equal(broken.errors.length, 3);
  assert.ok(broken.errors.every(e => e.includes("txid is not defined")));
  add("K", "token_orders_without_global_txid", "0 placements;3 caught ReferenceErrors");
  add("K", "static_insert_on_class", await errorOf(() => broken.AMM.insertCapital("a", 1, 1, true, 2, 2, 1)));
  add("K", "static_redeem_on_class", await errorOf(() => broken.AMM.redeemCapital("a", 1, 1, true, 2, 2, 1)));
  add("K", "static_request_on_class", await errorOf(() => broken.AMM.requestLiquidity({ size: 1, side: "buy" })));
  add("K", "static_total_on_class", await errorOf(() => broken.AMM.calculateTotalPosition()));
  add("K", "static_addresses_on_class", await errorOf(() => broken.AMM.getLPAddresses()));
  const failedAdd = new AMM(0, 100, 10, 7);
  add("K", "invalid_second_add_error", await errorOf(() => failedAdd.addCapital("a", 1, 5, false, 2, 0, 1)));
  assert.equal(failedAdd.position, 5); assert.equal(failedAdd.lpAddresses.a, 5); add("K", "invalid_second_add_mutates", "position=5;shares=5");
  const failedRedeem = new AMM(0, 100, 10, 7);
  await failedRedeem.addCapital("a", 1, 5, false, 2, 7, 1);
  add("K", "invalid_second_redeem_error", await errorOf(() => failedRedeem.redeemCapital("a", 1, 2, false, 2, 0, 2)));
  assert.equal(failedRedeem.position, 3); assert.equal(failedRedeem.lpAddresses.a, 3); add("K", "invalid_second_redeem_mutates", "position=3;shares=3");
  add("K", "static_insert_bound_to_instance", await errorOf(() => AMM.insertCapital.call(new AMM(0, 100, 10, 7), "a", 1, 1, true, 2, 2, 1)));
  const staticCapital = { position: 0, maxPosition: 100, lpShares: {} };
  add("K", "static_insert_bound_with_shares", await errorOf(() => AMM.insertCapital.call(staticCapital, "a", 1, 5, true, 2, 2, 1)));
  assert.equal(staticCapital.position, 5); assert.equal(staticCapital.lpShares.a, 5);
  add("K", "static_redeem_bound_with_shares", await errorOf(() => AMM.redeemCapital.call(staticCapital, "a", 1, 2, true, 2, 2, 1)));
  assert.equal(staticCapital.position, 3); assert.equal(staticCapital.lpShares.a, 3);
  const failTally = load({ failTallyAt: 2 }), partial = new failTally.AMM(0, 100, 10, 7);
  add("K", "tally_failure_error", await errorOf(() => partial.addCapital("a", 1, 5, true, 2, 2, 1)));
  assert.equal(partial.position, 5); assert.equal(partial.lpAddresses.a, 5); assert.equal(failTally.movements.length, 1);
  add("K", "tally_failure_leaves_prefix", "position=5;shares=5;one balance movement");
  add("K", "generate_numeric_contract_type", await errorOf(() => AMM.generateOrders.call(new AMM(0, 100, 10, 7), 100, 1, 10, 1, 2, false, false)));
  const dispatched = new AMM(10, 100, 4, "linear");
  dispatched.calculateOrderSize = AMM.calculateOrderSize;
  dispatched.generateOrdersForLinear = AMM.generateOrdersForLinear;
  const dispatchOrder = AMM.generateOrders.call(dispatched, 100, 1, 100, 1, 2, true, false);
  assert.equal(dispatchOrder.price, 100.01); assert.equal(dispatchOrder.side, "buy");
  add("K", "string_dispatch_ignores_inverse_argument", "linear receiver returns buy despite inverse=true");
  add("K", "generate_tokens_missing_method", await errorOf(() => AMM.generateOrders.call(new AMM(0, 100, 10, 7), 100, 1, 10, 1, 2, false, true)));
  assert.equal((await AMM.runOptionStrategy(100, 0.4, 100)).length, 0);
  assert.equal((await AMM.runOptionStrategy.call({ strategyBlob: "{" }, 100, 0.4, 100)).length, 0);
  add("K", "absent_or_invalid_strategy", "empty orders;invalid JSON is caught");
  const bound = { position: 10, maxPosition: 100, ammOrders: [] };
  await AMM.requestLiquidity.call(bound, { size: 5, side: "sell" });
  assert.equal(bound.position, 5); assert.equal(bound.ammOrders.length, 1);
  add("L", 10, 100, 5, "sell", 1, 5, 1);
  const rejectedSell = { position: 95, maxPosition: 100, ammOrders: [] };
  const result = await AMM.requestLiquidity.call(rejectedSell, { size: 10, side: "sell" });
  assert.equal(result, null); add("L", 95, 100, 10, "sell", 0, 95, 0);
  for (const side of ["buy", "sell"]) for (const size of [-2, 0, 1, 20]) {
    const p = { position: 10, maxPosition: 20, ammOrders: [] };
    const result = await AMM.requestLiquidity.call(p, { size, side });
    add("L", 10, 20, size, side, Number(result !== null), p.position, p.ammOrders.length);
  }
  // DB create/save/load source calls, with deterministic Date and in-memory mocks.
  const collections = { ammRegistry: [], ammState: [] };
  const database = { getDatabase(name) {
    const docs = collections[name]; assert.ok(docs);
    return {
      findAsync() { return { sort() { return { async limit() { return docs.slice().sort((a,b) => b.ammId-a.ammId).slice(0,1); } }; } }; },
      async insertAsync(doc) { docs.push(structuredClone(doc)); return doc; },
      async findOneAsync(query) { return docs.find(doc => doc.ammId === query.ammId) || null; },
      async updateAsync(query, update) { let doc = docs.find(doc => doc.ammId === query.ammId); if (!doc) { doc = { ...query }; docs.push(doc); } Object.assign(doc, structuredClone(update.$set)); },
    };
  } };
  const stored = load({ db: database }); const instance = new stored.AMM(0, 100, 10, 7);
  for (const [cid, pid] of [[7, 1], [8, 2]]) {
    const definition = await instance.createAMM({ contractId: cid, propertyId: pid });
    const state = await instance.loadAMMState(definition.ammId);
    const registry = await instance.loadAMMRegistry(definition.ammId);
    assert.equal(JSON.stringify(registry), JSON.stringify(definition)); assert.equal(state.position, 0); assert.equal(state.pnl, 0); assert.equal(state.updatedAt, 1000);
    add("D", definition.ammId, cid, pid, definition.createdAt, state.position, state.pnl, state.updatedAt);
  }
  const update = { position: 3, pnl: -2 }; await instance.saveAMMState(1, update);
  assert.equal(update.updatedAt, 1000); assert.equal((await instance.loadAMMState(1)).position, 3);
  add("K", "save_mutates_input_timestamp", "1000");
  await instance.saveAMMState(99, { position: 4 }); assert.equal((await instance.loadAMMState(99)).position, 4);
  add("K", "save_upserts_without_registry", "true");
  // Complete updateOrders orchestration under mocks: failing getAMM, no pool,
  // empty pool and invalid mark are skipped before valid pool cancel/insert/save.
  const validPool = new AMM(10, 100, 4, 1);
  const registry = { async loadContractSeries() { return new Map([1,2,3,4,5].map(id => [id, {}])); }, async getAMM(id) { if (id === 2) throw new Error("missing"); if (id === 3) return null; if (id === 4) return new AMM(0,100,4,id); return validPool; } };
  const refresh = load({ registry }); await refresh.AMM.updateOrdersForAllContractAMMs(50);
  assert.equal(refresh.events.map(e => e[0]).join(","), "book,cancel,load,insert,insert,save");
  assert.equal(refresh.events[1][1], "amm"); assert.equal(refresh.events.at(-1)[2], 2);
  add("K", "refresh_contract_orchestration", "cancel amm;insert bid;insert ask;save;skip four ineligible series");
  const fixture = rows.join("\n") + "\n", filename = path.join(fixtures, "amm_vectors.tsv");
  if (args.includes("--write")) fs.writeFileSync(filename, fixture); else assert.equal(fs.readFileSync(filename, "utf8"), fixture, "AMM vector drift");
  const counts = Object.fromEntries([...new Set(rows.filter(r => !r.startsWith("#")).map(r => r.split("\t")[0]))].map(kind => [kind, rows.filter(r => r.startsWith(`${kind}\t`)).length]));
  console.log(`PASS: AMM unmodified source vectors ${JSON.stringify(counts)} (${countQ} quote scenarios)`);
  console.log("BOUNDARY: local numeric accounting/intents; no authenticated ledger debit, book execution or durable DB claims");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
