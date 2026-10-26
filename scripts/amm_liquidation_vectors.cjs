#!/usr/bin/env node
"use strict";
// Execute COMPLETE unchanged SHA-pinned AMM and clearing classes. Dependencies
// reject unexpected calls; no replacement quote/sweep/funding formulas execute.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const AMM_SHA = "bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1";
const CLEARING_SHA = "612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528";
const BN = loadBigNumber();
const inert = name => new Proxy({}, { get(_target, key) { throw Error(`unexpected dependency ${name}.${String(key)}`); } });
const quiet = { log() {}, warn() {}, error() {} };
function load(filename, sha, dependencies) {
  const module = { exports: {} };
  vm.runInNewContext(readPinnedSource(path.join(fixtures, filename), sha), {
    module, exports: module.exports, console: quiet,
    require(name) {
      if (name === "bignumber.js") return BN;
      assert.ok(Object.hasOwn(dependencies, name), `unexpected require ${name}`);
      return dependencies[name];
    },
  }, { filename: `full-pinned-${filename}`, timeout: 1000 });
  return module.exports;
}
const Clearing = load("legacy_clearing_source.txt", CLEARING_SHA, Object.fromEntries([
  "./tally.js", "./contractRegistry.js", "./db.js", "./options.js", "./marginMap.js",
  "./orderbook.js", "./channels.js", "./property.js", "./volumeIndex.js", "./oracle.js",
  "./iou.js", "./tradeHistoryManager.js",
].map(name => [name, inert(name)])));
const AMM = load("legacy_amm_source.txt", AMM_SHA, Object.fromEntries([
  "./contractRegistry.js", "./orderbook.js", "./clearing.js", "./db.js",
].map(name => [name, inert(name)])));
const num = value => Object.is(value, -0) ? "-0" : String(value);
const level = (txid, price, amount = "0.5", sender = "amm", blockTime = 1) => ({ txid, price, amount, sender, blockTime });
const encodeLevels = levels => levels.map(l => [l.txid, l.sender, l.price, l.amount, l.blockTime].map(num).join("|")).join(";") || "-";
const encodeMatches = matches => matches.map(m => [m.buyOrder.amount, m.tradePrice,
  m.buyOrder.sender, m.sellOrder.sender, m.txid].map(num).join("|")).join(";") || "-";
const rows = [
  `# complete unchanged amm.js ${AMM_SHA}; clearing.js ${CLEARING_SHA}; BigNumber9.3.1; JS dca0888b827c5bc28fa9189bc9e1d507f5c5670c`,
  "# kind\tcase\tinverse\tliquidation_sell\tboundary\tcap\tinput_levels\tfilled\tmatches\tremaining_levels",
];
const results = new Map();
function sweep(name, inverse, sell, boundary, cap, levels, kind = "S") {
  const book = { buy: [], sell: [] };
  const side = sell ? "buy" : "sell";
  book[side] = levels.map(l => ({ ...l }));
  const input = encodeLevels(book[side]);
  const result = Clearing._sweepAmmLiquidityFirst(book,
    { sell, address: "distressed", contractId: 7, txid: "liq-original", isLiq: true },
    cap, boundary, inverse);
  for (const match of result.matches) {
    assert.equal(match.buyOrder.contractId, 7);
    assert.equal(match.sellOrder.contractId, 7);
    assert.equal(match.buyOrder.maker, sell);
    assert.equal(match.sellOrder.maker, !sell);
    assert.equal(match.buyOrder.amount, match.sellOrder.amount);
  }
  rows.push([kind, name, +inverse, +sell, boundary, cap, input, num(result.filledSize),
    encodeMatches(result.matches), encodeLevels(book[side])].join("\t"));
  results.set(name, { result, book, side });
  return result;
}
sweep("linear-buy-prefers-amm-over-better-real", false, false, 101, 1, [level("real-better", 99, 1, "carol"), level("amm-ask", 100.4)]);
sweep("linear-sell-prefers-amm-over-better-real", false, true, 99, 1, [level("real-better", 101, 1, "carol"), level("amm-bid", 99.6)]);
sweep("inverse-buy-reversal", true, false, 101, 1, [level("below", 100), level("equal", 101), level("above", 102)]);
sweep("inverse-sell-reversal", true, true, 99, 1, [level("above", 100), level("equal", 99), level("below", 98)]);
sweep("linear-buy-equal-inclusive", false, false, 101, 1, [level("equal", 101, 1), level("outside", "101.00000001", 1)]);
sweep("linear-sell-equal-inclusive", false, true, 99, 1, [level("equal", 99, 1), level("outside", "98.99999999", 1)]);
sweep("price-before-time", false, false, 103, 1, [level("old-worse", 102, 1, "amm", 1), level("new-better", 100, 1, "amm", 2)]);
sweep("time-tiebreak", false, false, 101, 1, [level("new", 100, 1, "amm", 5), level("old", 100, 1, "amm", 3)]);
sweep("stable-equal-price-time", false, false, 101, 1, [level("first", 100, 1), level("second", 100, 1)]);
sweep("partial-remainder", false, false, 101, "0.3", [level("partial", 100, "0.7")]);
sweep("zero-cap-no-mutation", false, false, 101, 0, [level("zero", 100, 0), level("positive", 100, 1)]);
sweep("negative-cap-no-mutation", false, true, 99, -1, [level("positive", 100, 1)]);
sweep("invalid-levels-skip", false, false, 101, 1, [level("nan-price", NaN, 1), level("inf-amount", 100, Infinity), level("negative", 100, -1), level("valid", 100, 1)]);
sweep("negative-price-source-acceptance", false, false, 101, 1, [level("negative-price", -1, 1)]);
sweep("zero-price-source-acceptance", false, false, 101, 1, [level("zero-price", 0, 1)]);
sweep("exact-take-number-bridge", false, false, 101, "0.33333333333333333333", [level("fraction", 100, "0.11111111111111111111"), level("remainder", 100, 1)]);
sweep("large-number-price-bridge", false, false, "9007199254740994", 1, [level("rounded-price", "9007199254740993", 1)]);
sweep("amm-colon-not-selected", false, false, 101, 1, [level("named-pool", 100, 1, "amm:7"), level("global-pool", 100, "0.5")]);
for (const inverse of [false, true]) for (const sell of [false, true]) {
  const pool = new AMM(100, 1000, 0.5, 7);
  const quotes = pool.quoteContractOrders(120, 24, 40);
  const quote = quotes.find(q => q.sell !== sell);
  sweep(`quote-to-sweep-${+inverse}-${+sell}`, inverse, sell, sell ? 118.8 : 121.2, 1,
    [{ ...quote, blockTime: quote.blockTime }], "Q");
  assert.equal(pool.position, 100, "quote/sweep do not update AMMPool capital/inventory");
}
assert.equal(results.get("linear-buy-prefers-amm-over-better-real").result.matches[0].tradePrice, 100.4);
assert.equal(results.get("linear-buy-prefers-amm-over-better-real").book.sell[0].sender, "carol");
assert.equal(results.get("inverse-buy-reversal").result.matches[0].tradePrice, 101);
assert.equal(results.get("inverse-buy-reversal").result.matches[1].tradePrice, 102);
assert.equal(results.get("inverse-sell-reversal").result.matches[1].tradePrice, 98);
assert.equal(results.get("negative-price-source-acceptance").result.matches[0].tradePrice, -1);
assert.equal(results.get("exact-take-number-bridge").result.filledSize, 0.3333333333333333);
assert.equal(results.get("large-number-price-bridge").result.matches[0].tradePrice, 9007199254740992);
assert.equal(results.get("amm-colon-not-selected").result.filledSize, 0.5);
const output = rows.join("\n") + "\n";
const target = path.join(fixtures, "amm_liquidation_vectors.tsv");
if (process.argv.includes("--write")) fs.writeFileSync(target, output);
else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), output, "source sweep fixture changed");
console.log(`PASS: ${rows.length - 2} complete-source AMM liquidation sweep cases`);
