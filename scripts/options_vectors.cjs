#!/usr/bin/env node
"use strict";

// Execute all nine methods of the pinned options.js, using the archived real
// BigNumber dependency. This pure source module has no DB, wallet or RPC imports.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, loadBigNumber, BIG_NUMBER_VERSION } = require("./legacy_numeric_oracle.cjs");
const args = process.argv.slice(2);
const sourceFlag = args.indexOf("--source-root");
if (sourceFlag >= 0 && !args[sourceFlag + 1]) {
  throw Error("usage: node scripts/options_vectors.cjs [--source-root PATH] [--write]");
}
const sourcePath = sourceFlag >= 0 ? path.join(path.resolve(args[sourceFlag + 1]), "src", "options.js")
  : path.join(fixtures, "legacy_options_source.txt");
const source = fs.readFileSync(sourcePath, "utf8").replace(/\r\n/g, "\n");
const digest = crypto.createHash("sha256").update(source).digest("hex");
assert.equal(digest, "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703", "pinned options.js LF SHA256");
const BigNumber = loadBigNumber();
const version = BIG_NUMBER_VERSION;
assert.equal(version, "9.3.1", "oracle dependency version");
assert.equal(BigNumber.config().DECIMAL_PLACES, 20);
assert.equal(BigNumber.config().ROUNDING_MODE, 4);
const context = {
  module: { exports: {} },
  require(name) { assert.equal(name, "bignumber.js"); return BigNumber; },
};
vm.runInNewContext(source, context, { filename: "pinned-options.js", timeout: 1000 });
const options = context.module.exports;
const header = `# options.js LF SHA256=${digest}; source=dca0888b827c5bc28fa9189bc9e1d507f5c5670c; bignumber.js=${version}\n`;
const hex = value => Buffer.from(value, "utf8").toString("hex");
const scalar = value => value === null ? "null" : String(value);
const tickers = [
  "", "1-2-C", "series-123-P-100.5", "a-b-2-C", "x-1-C-junk", "x-1-C-", "x-1-C-0",
  "x-1-C-1e2", "x-1-C-1e-2", "x-1-C-1.", "x-1-C-.5", "x-1-C-+1.5tail",
  "x-1-C-Infinity", "x-1-C-NaN", "x-1-C-0x20", "x-1-C-9.5-extra", "x-  +12tail-C-7",
  "x-0x20-P-0.25", "x-1.5-C-2", "x--2-C", "-1-C", "x-1-X", "x-1-c", "x-1-C-extra",
  "x-1-C\n", "x-1-C\r\n", "a\u2028-2-C", "a\u2029-2-C", "\ufeffx-\ufeff+12-P-\ufeff.25tail",
  "x-\u008512-C-2", "x-\u200312-C-2", "x-+00012-P-3", "x-999999999999999999999-C-1",
  `x-${"9".repeat(400)}-C-${"9".repeat(400)}`, "x-1-C-0.000000000000000000001", "x-1-P-2.5junk",
];
const tickerRows = tickers.map((ticker, index) => {
  const parsed = options.parseTicker(ticker);
  return [index, hex(ticker), options.isOptionTicker(ticker) ? "1" : "0", parsed ? hex(parsed.seriesId) : "null",
    parsed ? scalar(parsed.expiryBlock) : "null", parsed ? parsed.type : "null", parsed ? scalar(parsed.strike) : "null"].join("\t");
});
const rows = [];
function add(name, method, input, output) {
  rows.push([name, method, ...input, ...output.map(scalar)].join("\t"));
}
for (const [index, [existing, delta]] of [
  [0, 5], [5, 2], [5, -2], [5, -5], [5, -7], [-5, 2], [-5, 5], [-5, 7], [-5, -2],
  [0.3, -0.1], [NaN, 3], [2, NaN], [Infinity, -Infinity], [-Infinity, 2], [0, -0],
].entries()) {
  const result = options.computeReduceFlip(existing, delta);
  add(`reduce-${index}`, "reduce", [scalar(existing), scalar(delta)],
    [result.existing, result.delta, result.after, result.exSide, result.dSide, result.afterSide, result.closedQty, result.flipQty]);
}
for (const [index, [side, qty, trade, average]] of [
  ["LONG", "2.5", "125.5", "100.25"], ["SHORT", "2.5", "125.5", "100.25"],
  ["unexpected", "2.5", "125.5", "100.25"], ["", "3", "1", "2"], [null, "3", "1", "2"],
  ["LONG", "0", "1", "2"], ["LONG", "-2.5", "125.5", "100.25"],
  ["LONG", "0.0000000000000000000001", "123456789012345678901234567890.5", "0.1"],
].entries()) add(`rpnl-${index}`, "rpnl", [side === null ? "null" : side, qty, trade, average], [options.rpnlForClose(side, qty, trade, average)]);
for (const [index, [type, strike, spot]] of [
  ["Call", "100", "125"], ["Call", "125", "100"], ["Put", "125", "100"], ["Put", "100", "125"],
  ["unknown", "125", "100"], ["Call", "-2.5", "-1"], ["Put", "-2.5", "-1"],
  ["Call", "0.0000000000000000000001", "0.0000000000000000000002"],
].entries()) {
  add(`intrinsic-${index}`, "intrinsic", [type, strike, spot], [options.intrinsic(type, strike, spot)]);
  add(`naked-${index}`, "naked", [type, strike, spot], [options.nakedMaintenance(type, strike, spot)]);
}
for (const [index, [type, spot, strike, volatility, days]] of [
  ["Call", "100", "110", 0.2, 365], ["Call", "100", "90", 0.2, 365],
  ["Put", "100", "110", 0.2, 30], ["Call", "100", "110", 0, 30],
  ["Call", "100", "90", -0.2, 30], ["Put", "100", "110", 0.5, -5],
  ["Call", "100", "110", 0.2, 0], ["Put", "100", "110", 0.2, NaN],
  ["Call", "123.456789012345678901", "100", 0.3333333333333333, 0.125],
  ["unexpected", "100", "110", 0.7, 13.25],
  ["Call", "-100", "-110", 0.2, 30],
  ["Call", "0.000000000000000000001", "0", 0.123456789, 1],
  ["Put", "123456789012345678901234567890.5", "1", 0.2, 1],
].entries()) add(`price-${index}`, "price", [type, spot, strike, scalar(volatility), scalar(days)], [options.priceEUApprox(type, spot, strike, volatility, days)]);
const mtmCases = [
  ["empty", "100", 0.2, 1440, 144, []],
  ["mixed", "100", 0.2, 4320, 144, [["Call", "90", "2.5"], ["Put", "110", "-1.25"]]],
  ["default-bpd", "100", 0.2, 144, 0, [["Call", "90", "1"]]],
  ["negative-bpd", "100", 0.2, 30, -5, [["Put", "110", "2"]]],
  ["expired", "100", 0.2, -1, 144, [["Call", "90", "2"], ["Put", "110", "-1"]]],
  ["nan-bpd", "100", 0.2, 144, NaN, [["Call", "90", "1"]]],
  ["nan-blocks", "100", 0.2, NaN, 144, [["Call", "90", "1"]]],
  ["long-decimal", "123.456789012345678901", 0.3333333333333333, 18, 144, [["Call", "100", "0.000000000000000000001"], ["Put", "150", "-2.5"]]],
];
for (const [name, spot, volatility, blocks, bpd, legs] of mtmCases) {
  const positions = legs.map(([type, strike, qty]) => ({ type, strike, qty }));
  const result = options.mtmExposure(positions, spot, volatility, blocks, bpd);
  add(`mtm-${name}`, "mtm", [spot, scalar(volatility), scalar(blocks), scalar(bpd), legs.map(leg => leg.join(":")).join(";")], [result.premium, result.intrinsic]);
}
const portfolios = [
  ["empty", "100", []],
  ["naked-call", "100", [["Call", 10, "90", "-2.5"]]],
  ["naked-put", "100", [["Put", 10, "90", "-2.5"]]],
  ["long-only", "100", [["Call", 10, "90", "2.5"]]],
  ["call-protection", "100", [["Call", 10, "90", "-2"], ["Call", 10, "110", "1"]]],
  ["put-protection", "100", [["Put", 10, "110", "-2"], ["Put", 10, "90", "1"]]],
  ["equal-strike-unprotected", "100", [["Call", 10, "90", "-2"], ["Call", 10, "90", "2"]]],
  ["wrong-side-unprotected", "100", [["Call", 10, "90", "-2"], ["Call", 10, "80", "2"]]],
  ["different-expiry", "100", [["Call", 10, "90", "-2"], ["Call", 11, "110", "2"]]],
  ["different-type", "100", [["Call", 10, "90", "-2"], ["Put", 10, "80", "2"]]],
  ["capped-offset", "100", [["Call", 10, "90", "-2"], ["Call", 10, "300", "2"]]],
  ["long-order-a", "100", [["Call", 10, "90", "-1"], ["Call", 10, "100", "1"], ["Call", 10, "200", "1"]]],
  ["long-order-b", "100", [["Call", 10, "90", "-1"], ["Call", 10, "200", "1"], ["Call", 10, "100", "1"]]],
  ["short-order-a", "100", [["Call", 10, "90", "-1"], ["Call", 10, "110", "-1"], ["Call", 10, "120", "1"]]],
  ["short-order-b", "100", [["Call", 10, "110", "-1"], ["Call", 10, "90", "-1"], ["Call", 10, "120", "1"]]],
  ["fractional-cover", "100", [["Put", 10, "110.123456789012345678901", "-2.5"], ["Put", 10, "90.01234567890123456789", "1.25"]]],
  ["nan-expiry-group", "100", [["Call", NaN, "90", "-1"], ["Call", NaN, "110", "1"]]],
  ["zero-expiry-group", "100", [["Call", -0, "90", "-1"], ["Call", 0, "110", "1"]]],
  ["infinite-expiry-group", "100", [["Call", Infinity, "90", "-1"], ["Call", Infinity, "110", "1"]]],
  ["unknown-type", "100", [["unknown", 10, "90", "-1"], ["unknown", 10, "110", "1"]]],
  ["empty-type-skipped", "100", [["", 10, "90", "-1"]]],
  ["negative-total-floor", "-100", [["Call", 10, "90", "-1"]]],
];
const portfolioRows = portfolios.map(([name, spot, legs]) => {
  const positions = legs.map(([type, expiryBlock, strike, qty]) => ({ type, expiryBlock, strike, qty }));
  return [name, spot, legs.map(leg => leg.map(scalar).join(":")).join(";"), String(options.portfolioMaintenance(positions, spot))].join("\t");
});
for (const [file, body] of [
  ["options_tickers.tsv", "# id\tticker_utf8_hex\tisOptionTicker\tseries_utf8_hex_or_null\texpiry\ttype\tstrike\n" + tickerRows.join("\n") + "\n"],
  ["options_calculations.tsv", "# name\tmethod\tmethod_arguments...\texpected_outputs...\n" + rows.join("\n") + "\n"],
  ["options_portfolios.tsv", "# name\tspot\ttype:expiry:strike:qty;...\tmaintenance\n" + portfolioRows.join("\n") + "\n"],
]) {
  const target = path.join(__dirname, "..", "fixtures", file);
  const expected = header + body;
  if (args.includes("--write")) fs.writeFileSync(target, expected);
  else assert.equal(fs.readFileSync(target, "utf8"), expected, `source vector drift: ${file}`);
}
console.log(`Pinned options source verified: ${tickerRows.length} ticker, ${rows.length} calculation and ${portfolioRows.length} portfolio vectors; all nine methods executed`);
