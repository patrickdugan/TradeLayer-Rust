#!/usr/bin/env node
"use strict";

// Execute the complete unmodified source with an in-memory NeDB-shaped store.
// Mocks supply dependencies and record effects; they contain no registry math.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber, BIG_NUMBER_VERSION } = require("./legacy_numeric_oracle.cjs");
const args = process.argv.slice(2);
const sourceFlag = args.indexOf("--source");
if (sourceFlag >= 0 && !args[sourceFlag + 1]) throw Error("--source requires a path");
const digest = "d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121";
const source = readPinnedSource(sourceFlag < 0 ? path.join(fixtures, "legacy_contract_registry_source.txt") : path.resolve(args[sourceFlag + 1]), digest);
const BN = loadBigNumber();
const databases = new Map();
const effects = [];
let account = { available: 0, reserved: 0, margin: 0 };
let channelBalance = 0;
const clone = value => JSON.parse(JSON.stringify(value));
function database(name) {
  if (!databases.has(name)) databases.set(name, []);
  const docs = databases.get(name);
  const matches = (doc, query = {}) => Object.entries(query).every(([key, value]) => doc[key] === value);
  return {
    async findAsync(query) { return docs.filter(doc => matches(doc, query)).map(clone); },
    async findOneAsync(query) { const doc = docs.find(doc => matches(doc, query)); return doc ? clone(doc) : null; },
    async updateAsync(query, value) {
      const index = docs.findIndex(doc => matches(doc, query));
      if (index < 0) docs.push(clone(value)); else docs[index] = clone(value);
    },
    async insertAsync(value) { docs.push(clone(value)); return clone(value); },
  };
}
class AmmMock {
  constructor(position, maxPosition, maxQuoteSize, contractType) {
    Object.assign(this, { position, maxPosition, maxQuoteSize, contractType, lpAddresses: {}, ammOrders: [] });
  }
}
const propertyTickers = { 0: "ignored", 1: "USD", 2: "GOLD", 3: "SILVER" };
const oraclePrices = { 0: "9", 5: "3", 7: "0" };
function record(available, reserved, margin, channel, reason) {
  effects.push([available, reserved, margin, channel].map(scalar).concat(reason).join(":"));
}
const tally = {
  async hasSufficientBalance(_sender, _property, amount) { return { hasSufficient: new BN(account.available).gte(amount) }; },
  async getTally() { return clone(account); },
  async updateBalance(_sender, _property, available, reserved, margin, _vesting, reason) {
    record(available, reserved, margin, 0, reason);
    account.available = new BN(account.available).plus(available).toNumber();
    account.reserved = new BN(account.reserved).plus(reserved).toNumber();
    account.margin = new BN(account.margin).plus(margin).toNumber();
  },
  async hasSufficientChannel(_address, _property, amount) {
    return { hasSufficient: new BN(channelBalance).gte(amount), shortfall: new BN(amount).minus(channelBalance).toNumber(), reason: "insufficient" };
  },
  async updateChannelBalance(_address, _property, delta, reason) {
    record(0, 0, 0, delta, reason);
    channelBalance = new BN(channelBalance).plus(delta).toNumber();
  },
};
let marginSet = null;
let channelDebit = 0;
const context = { module: { exports: {} }, console: { log() {}, error() {} }, require(name) {
  if (name === "./db" || name === "./db.js") return { getDatabase: database };
  if (name === "path" || name === "util") return require(name);
  if (name === "bignumber.js") return BN;
  if (name === "./amm.js") return AmmMock;
  if (name === "./property.js") return { async getPropertyData(id) { return { ticker: propertyTickers[id], value: "17.25" }; } };
  if (name === "./oracle.js") return {
    async getOracleInfo() { return { name: { ticker: "BTC" } }; },
    async getOraclePrice(id) { return oraclePrices[id]; },
    async getOracleData() { return "23.75"; },
  };
  if (name === "./channels.js") return { async debitInitMarginFromChannel(_channel, _sender, _property, amount) { channelDebit = amount; } };
  if (name === "./volumeIndex.js") return { async getLastPrice(pair, block) { return { pair, block, price: "12.5" }; } };
  if (name === "./tally.js") return tally;
  if (name === "./marginMap.js") return { async getInstance() { return { async setInitialMargin(_sender, _id, total) { marginSet = total; return { initialMargin: total }; } }; } };
  if (name === "./Oracles") throw Error("MODULE_NOT_FOUND: ./Oracles");
  throw Error(`unmocked source dependency ${name}`);
} };
vm.runInNewContext(source, context, { filename: "pinned-contractRegistry.js", timeout: 1000 });
const registry = context.module.exports;
const rows = [];
const scalar = value => {
  if (value === null || value === undefined) return "null";
  if (typeof value === "boolean") return value ? "1" : "0";
  const number = new BN(value);
  return number.isFinite() ? number.toFixed() : String(value);
};
const add = (kind, name, input, output) => rows.push([kind, name, input, output].join("\t"));
const ids = values => values.join(",") || "-";
function summary(series) {
  return [series.id, series.ticker, series.issuer, series.native ? 1 : 0, series.inverse ? 1 : 0,
    scalar(series.underlyingOracleId), series.notionalPropertyId, scalar(series.notionalValue), series.collateralPropertyId,
    scalar(series.leverage), series.expiryPeriod, series.series, scalar(series.fee), series.whitelist,
    series.contracts.unexpired.map(contract => `${contract.id}:${contract.expirationBlock}`).join(";") || "-",
    [series.ammPool.position, series.ammPool.maxPosition, series.ammPool.maxQuoteSize, series.ammPool.contractType].join(":")].join("|");
}
async function main() {
  const definitions = [
    // native,inverse,oracle,notionalPid,notional,collateral,leverage,expiry,count,fee,whitelist,ticker,block,pairs
    [1, 1, 0, 0, "1", 1, "10", 0, 3, "0.01", null, "ignored", 100, [[0, 1]]],
    [1, 0, null, 2, "2.5", 1, "0", 144, 3, "0", 4, "ignored", 100, [[2, 1], [3, 1]]],
    [0, 0, 5, 2, "0.333333333333333333333", 1, "3", 144, 2, "0.002", null, null, 101, []],
    [0, 1, 5, 2, "123456789.123456789", 0, "7", 7, 2, "0", 0, null, 101, []],
    [0, 0, 0, 3, "5", 2, "11", 10, 1, "0", null, "TERM", 200, []],
    [0, 0, null, 3, "5", 2, "11", 10, 0, "0", null, "", 200, []],
    [0, 1, 7, 3, "-5", 2, "-3", 0, 0, "0", null, "CUSTOM", 200, []],
    [1, 1, null, 2, "0.00000001", 3, "3", 1, 1, "0", null, null, 9007199254740990, [[2, 3]]],
  ];
  for (const [index, definition] of definitions.entries()) {
    const [native, inverse, oracle, notionalPid, notional, collateral, leverage, expiry, count, fee, whitelist, ticker, block, pairs] = definition;
    const parameters = { native: !!native, inverse: !!inverse, underlyingOracleId: oracle, notionalPropertyId: notionalPid,
      notionalValue: notional, collateralPropertyId: collateral, leverage: leverage === "0" ? 0 : leverage, expiryPeriod: expiry, series: count, fee, whitelist,
      ticker, onChainData: pairs };
    const id = await registry.createContractSeries("issuer", parameters, block);
    const input = [native, inverse, scalar(oracle), notionalPid, notional, collateral, leverage, expiry, count, fee, scalar(whitelist), ticker || "-", block,
      pairs.map(pair => pair.join(":")).join(";") || "-"].join("|");
    add("create", `create-${index}`, input, summary(await registry.getContractSeries(id)));
  }
  add("query", "all", "all", ids((await registry.getAllContracts()).map(contract => contract.id)));
  add("query", "perpetual", "perpetual", ids(await registry.getAllPerpContracts()));
  add("query", "next-id", "next", scalar(await registry.getNextId()));
  add("query", "next-map-prefix", "next-map", scalar(await registry.getNextIdFromMap(new Map([["12-tail", {}], ["bad", {}], ["3", {}], ["+0x10-tail", {}]]))));
  for (const collateral of [0, 1, 2, 3, 99]) add("collateral", `collateral-${collateral}`, String(collateral), ids(await registry.getAllContractsForCollateral("unused", collateral)));
  for (const text of ["1", "1-100", "  +02suffix", "3.5", "0x10", "-1", "999", "word", "\ufeff4-rest", "\u00851", "\u20032", "+3", "0004"]) {
    const info = await registry.getContractInfo(text);
    add("lookup", Buffer.from(text).toString("hex"), Buffer.from(text).toString("hex"), info ? String(info.id) : "null");
  }
  await registry.loadContractSeries();
  for (const [index, [collateral, a, b, notional]] of [[1, 0, 1, 99], [1, 1, 0, 99], [1, 1, 0, 0], [1, 2, 1, 99], [1, 1, 2, 99], [1, 1, 2, 2], [3, 2, 3, 99], [2, 2, 3, 2]].entries()) {
    add("duplicate", `duplicate-${index}`, [collateral, a, b, notional].join("|"), scalar(await registry.isDuplicateNativeContract(collateral, [a, b], notional)));
  }
  for (const collateral of [0, 1, 2, 3]) {
    const matches = await registry.lookupInverseNativeByNotionalPid(collateral);
    add("inverse-native", `inverse-native-${collateral}`, String(collateral), matches.map(match => [match.contractId, match.seriesId, match.symbol, match.notionalPropertyId].join(":")).join(";") || "-");
  }
  for (const id of [1, 2, 3, 4, 5, 6, 7, 8, 999]) {
    add("flags", `flags-${id}`, String(id), [scalar(await registry.isNativeContract(id)), scalar(await registry.isOracleContract(id)), scalar(await registry.isInverse(id))].join("|"));
    for (const mark of ["0", "-1", "3", "0.00000003", "123456789.123456789", "NaN"]) {
      const value = await registry.getNotionalValue(id, mark);
      add("notional", `notional-${id}-${mark}`, [id, mark].join("|"), [scalar(value.notionalValue), scalar(value.notionalPerContract)].join("|"));
    }
    if (id === 999) continue;
    const amm = await registry.getAMM(id);
    add("amm", `amm-${id}`, String(id), [amm.position, amm.maxPosition, amm.maxQuoteSize, amm.contractType].map(scalar).join("|"));
    add("metadata", `metadata-${id}`, String(id), [await registry.getContractType(id), scalar(await registry.getCollateralId(id)), scalar(await registry.getOracleId(id))].join("|"));
    for (const price of ["0", "-3", "0.00000003", "3", "19.123456789123456789"]) {
      const value = await registry.getInitialMargin(id, price);
      add("margin", `margin-${id}-${price}`, [id, price].join("|"), scalar(value));
    }
    const info = await registry.getContractInfo(id);
    add("collateral-value", `value-${id}`, String(id), scalar(await registry.getCollateralValue(info)));
  }
  for (const flag of [true, false]) {
    await registry.setModFlag(flag);
    add("mod-flag", `mod-${flag}`, flag ? "1" : "0", registry.modFlag ? "1" : "0");
  }
  for (const [index, [period, count, id, block]] of [[0, 3, 90, 100], [7, 3, 90, 100], [5, 0, 90, 100], [1, 1, 90, 9007199254740990]].entries()) {
    const generated = await registry.generateContracts(period, count, id, block);
    add("generate", `generate-${index}`, [period, count, id, block].join("|"), generated.map(value => `${value.id}:${value.expirationBlock}`).join(";") || "-");
  }
  for (const [index, prices] of [[], [1, 2, 3], [0.1, 0.2, 0.3], [1e16, 1, -1e16], [1e16, -1e16, 1], [-3, 2, 4]].entries()) {
    add("average", `average-${index}`, prices.map(scalar).join("|") || "-", scalar(registry.calculateAveragePrice(prices.map(price => ({ price })))));
  }
  for (const [index, [size, rate]] of [[2, 0.001], [-10, 0.003], [0.1, 0.2], [0, -1], [123456789.5, -0.00003]].entries()) {
    add("funding-amount", `funding-${index}`, [size, rate].map(scalar).join("|"), scalar(registry.calculateFundingAmount(size, rate)));
  }
  for (const [index, [available, reserved, amount, price, inProcess]] of [[10, 0, -3, 2, false], [0, 3, 3, 2, true], [10, 0, 0, 2, true], [0, 0, 0.0000000001, 3, false]].entries()) {
    account = { available, reserved, margin: 0 }; effects.length = 0;
    const result = await registry.moveCollateralToReserve("trader", 1, amount, price, 200, "tx", inProcess);
    add("reserve", `reserve-${index}`, [available, reserved, amount, price, inProcess ? 1 : 0].map(String).join("|"), [scalar(result), effects.join(";") || "-"].join("|"));
  }
  const transfers = [
    // avail,res,margin,total,orderprice,amount,maker,channel,channelBal,sender,buyfee,sellfee,buySide
    [0, 5, 0, 3, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
    [5, 1, 0, 3, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
    [0, 1, 3, 3, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
    [1, 1, 1, 3, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
    [0, 0, 0, 3, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
    [0, 10, 0, 1, 0.01, 1, 0, 0, 0, "trader", 0, 0, 1],
    [5, 0, 0, 3, 2, 1, 0, 0, 0, "trader", 0, 0, 1],
    [0, 0, 0, 3, 2, 1, 1, 0, 0, "amm:1", 0.25, 0, 1],
    [0, 0, 0, 3, 2, 1, 1, 1, 4, "trader", 0.25, 0, 1],
    [2, 0, 0, 3, 2, 1, 1, 1, 1, "trader", 0, 0.5, 0],
    [0, 3, 0, 3, 2, 1, 1, 0, 0, "trader", 0.25, 0, 1],
    [0, 3, 0, 3, 2, 1, 1, 0, 0, "trader", 0, 0.25, 0],
    [0, 0, 0, 0, 2, 1, 1, 0, 0, "trader", 0, 0, 1],
  ];
  for (const [index, input] of transfers.entries()) {
    const [available, reserved, margin, total, orderPrice, amount, maker, channel, channelValue, sender, buyFee, sellFee, buySide] = input;
    account = { available, reserved, margin }; channelBalance = channelValue; effects.length = 0; marginSet = null; channelDebit = 0;
    const feeInfo = { buyFeeFromReserve: buyFee !== 0, sellFeeFromReserve: sellFee !== 0, buyerFee: buyFee, sellerFee: sellFee };
    try {
      await registry.moveCollateralToMargin(sender, 1, amount, 2, orderPrice, !!buySide, total, !!channel, "channel", 200, feeInfo, !!maker, false, "tx", {});
      add("transfer", `transfer-${index}`, input.join("|"), [scalar(marginSet), scalar(channelDebit), effects.join(";") || "-"].join("|"));
    } catch (error) {
      assert.match(error.message, /^Insufficient collateral/);
      add("transfer", `transfer-${index}`, input.join("|"), "insufficient");
    }
  }
  // Native-inverse invalid marks coerce notionalPerContract through Number,
  // unlike the valid-mark branch which returns the exact original metadata.
  const precisionInput = "1|1|null|2|1.00000000000000000001|3|3|0|0|0|null|-|200|2:3";
  const precisionId = await registry.createContractSeries("issuer", {
    native: true, inverse: true, underlyingOracleId: null,
    notionalPropertyId: 2, notionalValue: "1.00000000000000000001",
    collateralPropertyId: 3, leverage: "3", expiryPeriod: 0, series: 0,
    fee: "0", whitelist: null, ticker: null, onChainData: [[2, 3]],
  }, 200);
  add("create", "create-notional-metadata-precision", precisionInput, summary(await registry.getContractSeries(precisionId)));
  for (const mark of ["0", "-1", "NaN", "1"]) {
    const result = await registry.getNotionalValue(precisionId, mark);
    add("notional", `notional-metadata-precision-${mark}`, [precisionId, mark].join("|"),
      [scalar(result.notionalValue), scalar(result.notionalPerContract)].join("|"));
  }
  // Source defects are asserted here, rather than forged as passing Rust math.
  const quirks = [];
  quirks.push(["static-isValid-after-create", scalar(registry.isValidSeriesId(1))]);
  await assert.rejects(registry.updateAMM(1, "lp", 1, null), /undefined/); quirks.push(["static-updateAMM", "TypeError"]);
  await assert.rejects(new registry().hasOpenPositions({ seriesId: 1 }), /MarginMap is not defined/); quirks.push(["hasOpenPositions", "ReferenceError"]);
  assert.equal(await registry.calculateFundingRate(1), 0); quirks.push(["native-funding-rate", "0"]);
  await assert.rejects(registry.calculateFundingRate(3), /MODULE_NOT_FOUND/); quirks.push(["oracle-funding-rate", "MODULE_NOT_FOUND"]);
  await assert.rejects(new registry().applyFundingRateToSystem(3, 100), /ContractsRegistry is not defined/); quirks.push(["applyFundingRateToSystem", "ReferenceError"]);
  assert.equal(await registry.getPriceAtBlock(3, 100), null); quirks.push(["oracle-price-empty", "null"]);
  databases.get("contractList").push({ oracleId: 5, blockHeight: 99, data: { price: 5 } });
  await assert.rejects(registry.getPriceAtBlock(3, 100), /undefined/); quirks.push(["oracle-price-normal-record", "TypeError"]);
  const nativePrice = await registry.getPriceAtBlock(2, 123);
  assert.equal(nativePrice.pair, "2-1"); assert.equal(nativePrice.block, 123); quirks.push(["native-price-first-pair", "2-1:123:12.5"]);
  databases.set("oracleData", [{ oracleId: 5, blockHeight: 1, price: 10 }, { oracleId: 5, blockHeight: 999, price: 20 }]);
  assert.equal((await registry.getLatestOracleData(5)).blockHeight, 1); quirks.push(["latest-oracle-is-first", "1"]);
  await registry.saveFundingEvent(1, 0.01, 90); await registry.saveFundingEvent(2, -0.02, 80); await registry.saveFundingEvent(1, 0.03, 70);
  assert.equal((await registry.loadFundingEvents(1)).map(event => event.blockHeight).join(","), "90,70"); quirks.push(["funding-event-insertion-order", "90,70"]);
  for (const [name, result] of quirks) add("quirk", name, "-", result);
  const target = path.join(fixtures, "contract_registry_vectors.tsv");
  const content = `# contractRegistry.js LF SHA256=${digest}; source=dca0888b827c5bc28fa9189bc9e1d507f5c5670c; bignumber.js=${BIG_NUMBER_VERSION}\n# kind\tname\tinput\tresult\n${rows.join("\n")}\n`;
  if (args.includes("--write")) fs.writeFileSync(target, content, "utf8");
  else assert.equal(fs.readFileSync(target, "utf8"), content, "actual pinned contractRegistry.js oracle");
  console.log(`contractRegistry.js: ${rows.length} actual-source vectors (${quirks.length} characterized paths)`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
