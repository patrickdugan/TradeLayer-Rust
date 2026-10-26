#!/usr/bin/env node
"use strict";

// The complete hash-pinned class runs unchanged. Only external dependencies
// are adapters. No arithmetic or matching implementation substitutes for it.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const HASH = "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98";
const args = process.argv.slice(2);
const sourceIndex = args.indexOf("--source");
const source = readPinnedSource(sourceIndex < 0 ? path.join(fixtures, "legacy_orderbook_source.txt") : args[sourceIndex + 1], HASH);
const BigNumber = loadBigNumber();
const clone = value => JSON.parse(JSON.stringify(value));
const data = new Map();
const trace = [];
let position = 0;
let serial = 0;
const db = { async getDatabase(name) {
  if (!data.has(name)) data.set(name, new Map());
  const rows = data.get(name);
  return {
    async findOneAsync(q) { return rows.has(q._id) ? clone(rows.get(q._id)) : null; },
    async findAsync() { return [...rows.values()].map(clone); },
    async updateAsync(q, update) {
      rows.set(q._id, clone(update.$set ? { ...(rows.get(q._id) || {}), ...update.$set, _id: q._id } : update));
      trace.push(["write", name, q._id]);
    },
    async insertAsync(doc) { rows.set(doc._id, clone(doc)); },
    async loadDatabase() {},
  };
}};
const registry = {
  async getInitialMargin(contract, price) { trace.push(["initialMargin", contract, String(price)]); return 2; },
  async moveCollateralToReserve(owner, contract, quantity, price, height, txid) {
    trace.push(["reserve", owner, contract, String(quantity), String(price), height, txid]);
    return Number(quantity) * 2;
  },
  async getCollateralId() { return 2; },
};
const margin = { async loadMarginMap() { return { async getPositionForAddress() { return { contracts: position }; } }; } };
const tally = { async updateBalance(...values) { trace.push(["balance", ...values]); } };
const inert = name => new Proxy({}, { get(_object, key) { throw Error(`unexpected ${name}.${String(key)}`); } });
const moduleBox = { exports: {} };
const context = vm.createContext({
  module: moduleBox, exports: moduleBox.exports,
  console: { log() {}, error() {}, warn() {} },
  require(name) {
    if (name === "bignumber.js") return BigNumber;
    if (name === "./db.js") return db;
    if (name === "uuid") return { v4: () => `oracle-${++serial}` };
    if (name === "./contractRegistry.js") return registry;
    if (name === "./marginMap.js") return margin;
    if (name === "./tally.js") return tally;
    if (name === "./tradeHistoryManager.js") return class TradeHistory {};
    if (["./volumeIndex.js", "./channels.js", "./clearlist.js", "./consensus.js", "./iou.js", "./clearing.js"].includes(name)) return inert(name);
    throw Error(`unexpected require ${name}`);
  },
});
vm.runInContext(source, context, { filename: "pinned-orderbook.js", timeout: 1000 });
const Orderbook = moduleBox.exports;
const methods = Object.getOwnPropertyNames(Orderbook.prototype).filter(n => n !== "constructor");
const statics = Object.getOwnPropertyNames(Orderbook).filter(n => typeof Orderbook[n] === "function" && !["length", "name", "prototype"].includes(n));
assert.equal(methods.length, 54);
assert.equal(statics.length, 15);
const id = number => number.toString(16).padStart(2, "0").repeat(32);
const idNumber = value => parseInt(value.slice(0, 2), 16);
const num = value => new BigNumber(value).toFixed();
// id,owner,height,side,quantity,price,post,market
const encode = o => [o.id, o.sender, o.blockTime, o.sell ? "S" : "B", o.amount, o.price, +o.post, +o.isMarket].join(",");
function order(number, sell = false, amount = "1", price = "100", height = 10, owner = sell ? "seller" : "buyer", post = false, market = false) {
  return { id: number, contractId: 1, txid: id(number), sender: owner, blockTime: height,
    sell, amount, price, post, isMarket: market, initMargin: Number(amount) * 2, initialReduce: false };
}
const shapeBook = book => [book.buy, book.sell].map(side => side.map(o => [idNumber(o.txid), num(o.amount)].join(",")).join(";")).join("|");
const shapeMatches = result => result.matches.map(m => [idNumber(m.buyOrder.txid), idNumber(m.sellOrder.txid), num(m.buyOrder.amount), num(m.tradePrice), +m.buyOrder.maker, +m.sellOrder.maker, idNumber(m.txid)].join(",")).join(";") + "|" + shapeBook(result.orderBook);
const rows = [], quirks = [];
const add = (kind, name, input, output) => rows.push([kind, name, input, output, JSON.stringify(trace)].join("\t"));
async function matched(name, orders) {
  trace.length = 0;
  const book = { buy: orders.filter(o => !o.sell).map(clone), sell: orders.filter(o => o.sell).map(clone) };
  const result = await new Orderbook("1").matchContractOrders(book);
  add("match", name, orders.map(encode).join(";"), shapeMatches(result));
}
async function reset() { data.clear(); trace.length = 0; position = 0; }
async function main() {
  for (const bh of [9, 10, 11]) for (const sh of [9, 10, 11]) for (const bp of [false, true]) for (const sp of [false, true]) {
    await matched(`limit-${bh}-${sh}-${+bp}-${+sp}`, [order(1, false, "1.25", "103", bh, "buyer", bp), order(2, true, "0.75", "99", sh, "seller", sp)]);
  }
  for (const bm of [false, true]) for (const sm of [false, true]) for (const olderSell of [false, true]) {
    await matched(`market-${+bm}-${+sm}-${+olderSell}`, [order(1, false, "1.5", "90", olderSell ? 11 : 9, "buyer", true, bm), order(2, true, "2.25", "100", 10, "seller", true, sm)]);
  }
  for (const bh of [9, 10, 11]) for (const bp of [false, true]) for (const sp of [false, true]) {
    await matched(`self-${bh}-${+bp}-${+sp}`, [order(1, false, "2.5", "103", bh, "same", bp), order(2, true, "1", "99", 10, "same", sp)]);
  }
  await matched("partial-many", [order(1, false, "2.75", "103", 11), order(2, true, "0.25", "99", 9, "sella"), order(3, true, "1.5", "100", 9, "sellb"), order(4, true, "2", "101", 10, "sellc")]);
  await matched("stable-price-time", [order(3, false, "0.25", "103", 9, "third"), order(1, false, "0.25", "103", 9, "first"), order(2, true, "0.4", "100", 10)]);
  await matched("market-no-sort-bump", [order(1, false, "1", "10", 9, "market", false, true), order(2, false, "1", "20", 9, "limit"), order(3, true, "1", "30", 10)]);
  for (const [a, b] of [["0.3", "0.1"], ["9007199254740993", "1"], ["1.99999999999999999", "0.00000000000000001"], ["0.00000001", "0.000000001"]]) {
    await matched(`number-${a}-${b}`, [order(1, false, a, "103", 9), order(2, true, b, "100", 10)]);
  }
  for (const before of ["-3", "0", "3", "-0.3", "0.3", "9007199254740993"]) for (const sell of [false, true]) for (const amount of ["0.1", "4.25"]) {
    await reset(); position = before;
    const o = order(1, sell, amount, "100", 10, "alice");
    const result = await new Orderbook("1").addContractOrder(1, o.price, o.amount, sell, false, 10, o.txid, "alice", false, false, false, false);
    const placed = result.orderBook[sell ? "sell" : "buy"][0];
    const reserve = trace.find(row => row[0] === "reserve");
    add("reserve", `reserve-${before}-${+sell}-${amount}`, [before, sell ? "S" : "B", amount].join(","), [num(reserve ? reserve[3] : 0), +placed.initialReduce].join(","));
  }
  const names = ["alice", "bob", "ann9", "anna", "annA", "z", "zz", "zz1", "ABC", "abc", "AbC", "abC", "xxabc", "yyabc"];
  for (let i = 0; i < names.length; i++) for (let j = 0; j < names.length; j++) {
    const result = context.compareSenderAddresses(names[i], names[j], id(1), id(2));
    trace.length = 0;
    add("compare", `compare-${i}-${j}`, `${names[i]},${names[j]},1,2`, String(Math.sign(result)));
  }
  for (const owners of [["ann9", "anna", "bob", "alice"], ["abc", "AbC", "ABC", "abC"], ["zz1", "zz", "z"], ["same", "same", "same"]]) {
    await reset();
    const orders = owners.map((owner, i) => order(i + 1, false, "1.25", "100", i % 2 === 0 ? 9 : 10, owner));
    for (const o of orders) await Orderbook.queueOnChainContractOrder("1", o.sender, { contractId: 1, price: o.price, amount: o.amount, sell: false, blockTime: o.blockTime }, o.blockTime, o.txid);
    await Orderbook.processQueuedOnChainOrdersForBlock(10);
    const result = await new Orderbook("1").loadOrderBook("1");
    add("queue", `queue-${rows.length}`, orders.map(encode).join(";"), result.buy.map(o => idNumber(o.txid)).join(","));
  }
  // Actual cancellation traces, including broken behavior. These are
  // characterizations, not expectations for the owner-checked typed repair.
  for (const [name, criteria] of [["all-owner-noop", { address: "alice" }], ["buy-txid-foreign", { txid: id(1), buy: true }], ["sell-txid-wrong-side", { txid: id(2), buy: false }], ["price-without-txid-noop", { price: "99", buy: false }], ["price-with-txid-foreign", { txid: "missing", price: "999", buy: true }]]) {
    await reset();
    const instance = new Orderbook("1");
    await instance.saveOrderBook({ buy: [order(1, false, "1", "103", 9, "other")], sell: [order(2, true, "2", "99", 9, "alice")] }, "1");
    trace.length = 0;
    const removed = await instance.cancelOrdersByCriteria("alice", "1", criteria, false, false);
    const book = await instance.loadOrderBook("1");
    quirks.push(["cancel", name, JSON.stringify(criteria), `${removed.map(o => idNumber(o.txid)).join(",")}|${shapeBook(book)}|${String(removed.returnFromReserve)}`, JSON.stringify(trace)].join("\t"));
  }
  await reset();
  const bad = await db.getDatabase("orderBooks");
  await bad.updateAsync({ _id: "1" }, { _id: "1", value: "{bad-json" });
  const empty = await new Orderbook("1").loadOrderBook("1");
  quirks.push(["load", "corrupt-json-swallowed", "{bad-json", shapeBook(empty), "[]"].join("\t"));
  for (const [name, run] of [
    ["short-settlement-unqualified", () => new Orderbook("1").processContractMatchesShort([{ buyOrder: { buyerAddress: "buyer" }, sellOrder: { sellerAddress: "seller" } }], 10, false)],
    ["contract-txid-missing-buy", () => new Orderbook("1").cancelContractOrderByTxid("alice", 1, id(1), 10)],
    ["token-txid-undefined-order", () => new Orderbook("1").cancelTokenOrderByTxid("alice", 1, 2, id(1), 10)],
  ]) {
    await reset();
    try { await run(); quirks.push(["failure", name, "", "returned", JSON.stringify(trace)].join("\t")); }
    catch (error) { quirks.push(["failure", name, "", `${error.name}:${error.message}`, JSON.stringify(trace)].join("\t")); }
  }
  const header = `# Full unchanged orderbook.js ${HASH}; 15 static +54 instance methods; external adapters explicit\n# kind\tname\tinput\toutput\tdependency_trace\n`;
  const output = header + rows.join("\n") + "\n";
  const failures = header + quirks.join("\n") + "\n";
  if (args.includes("--check")) {
    assert.equal(fs.readFileSync(path.join(fixtures, "orderbook_vectors.tsv"), "utf8").replace(/\r\n/g, "\n"), output);
    assert.equal(fs.readFileSync(path.join(fixtures, "orderbook_quirks.tsv"), "utf8").replace(/\r\n/g, "\n"), failures);
  } else {
    fs.writeFileSync(path.join(fixtures, "orderbook_vectors.tsv"), output);
    fs.writeFileSync(path.join(fixtures, "orderbook_quirks.tsv"), failures);
  }
  console.log(`orderbook: ${rows.length} actual-source vectors; ${quirks.length} broken-path characterizations; methods=${statics.length}+${methods.length}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
