#!/usr/bin/env node
"use strict";

// Compose COMPLETE unchanged source modules. Adapters supply data and record
// effects; registry, fee, average-price and validity equations run in source.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const hashes = {
  registry: ["legacy_contract_registry_source.txt", "d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121"],
  margin: ["legacy_margin_map_source.txt", "ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636"],
  order: ["legacy_orderbook_source.txt", "c368acbc04f988b989fa320b3a1aefdb9522b9f9f5eff536137e7a2e8e9f4f98"],
  options: ["legacy_options_source.txt", "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703"],
};
const BN = loadBigNumber();
const scalar = value => value == null ? "null" : new BN(value).toFixed();
const rows = [];
const output = (...fields) => rows.push(fields.join("\t"));

function load() {
  const stores = new Map(), trace = [], modules = {};
  const settings = { available: 1000, contracts: 0, avgPrice: 0, country: "CA", oracle: 100, missingProperty: null, membership: false };
  const clone = value => value == null ? value : JSON.parse(JSON.stringify(value));
  const database = name => {
    if (!stores.has(name)) stores.set(name, []);
    const data = stores.get(name);
    const matches = (row, query) => Object.entries(query || {}).every(([key, value]) => row[key] === value);
    return {
      async findAsync(query) { return data.filter(row => matches(row, query)).map(clone); },
      async findOneAsync(query) { return clone(data.find(row => matches(row, query)) || null); },
      async updateAsync(query, value) {
        const index = data.findIndex(row => matches(row, query));
        const next = value.$set ? { ...(data[index] || {}), ...clone(value.$set) } : clone(value);
        if (index < 0) data.push(next); else data[index] = next;
      },
      async insertAsync(value) { data.push(clone(value)); },
    };
  };
  const db = { getDatabase: async name => database(name) };
  const property = { async getPropertyData(id) {
    trace.push(`property:${id}`);
    return id === settings.missingProperty ? null : { ticker: id === 7 ? "USD" : "ASSET", whitelistId: [4, 5] };
  } };
  const oracle = {
    async getOracleInfo(id) { trace.push(`oracle:${id}`); return id === 404 ? null : { name: { ticker: "INDEX" } }; },
    async getOraclePrice(id) { trace.push(`price:${id}`); return settings.oracle; },
  };
  const activation = { isTxTypeActive: async () => true, getAdmin: () => "admin" };
  const tally = {
    async getTally() { return { available: settings.available, reserved: 0, margin: 100 }; },
    async hasSufficientBalance(owner, id, amount) {
      trace.push(`balance:${owner}:${id}:${scalar(amount)}`);
      return { hasSufficient: new BN(settings.available).gte(amount), shortfall: new BN(amount).minus(settings.available).toNumber() };
    },
    async updateBalance(...args) { trace.push(`update:${args.map(String).join(":")}`); },
  };
  const inert = name => new Proxy({}, { get(_target, key) { throw Error(`unused dependency ${name}.${String(key)}`); } });
  function dependency(name) {
    if (name === "bignumber.js") return BN;
    if (name === "./db" || name === "./db.js") return db;
    if (name === "path" || name === "util" || name === "crypto") return require(name);
    if (name === "uuid") return { v4: () => "source-id" };
    if (name === "./contractRegistry.js") return modules.registry;
    if (name === "./marginMap.js") return modules.margin;
    if (name === "./orderbook.js") return modules.order;
    if (name === "./options.js") return modules.options;
    if (name === "./tally.js") return tally;
    if (name === "./property.js") return property;
    if (name === "./oracle.js") return oracle;
    if (name === "./activation.js") return { getInstance: () => activation };
    if (name === "./amm.js") return class AmmDependency {
      constructor(position, maxPosition, maxQuoteSize, contractType) { Object.assign(this, { position, maxPosition, maxQuoteSize, contractType }); }
    };
    if (name === "./clearlist.js") return {
      getBanlist: async () => null,
      getCountryCodeByAddress: async () => settings.country == null ? null : { countryCode: settings.country },
      isAddressInClearlist: async id => { trace.push(`member:${id}`); return settings.membership && id === 5; },
    };
    if (name === "./volumeIndex.js") return { getLastPrice: async pair => { trace.push(`index:${pair}`); return null; } };
    if (name === "bip32") return { BIP32Factory: () => ({}) };
    if (name === "bech32") return { bech32: inert("bech32") };
    if (name === "./procedural.js" || name === "./scaling.js") return {};
    if (["bitcoinjs-lib", "tiny-secp256k1"].includes(name)) return {};
    if (name === "./tradeHistoryManager.js") return class TradeHistory {};
    if (["./channels.js", "./consensus.js", "./iou.js", "./clearing.js", "./txUtils.js", "./vaults.js", "./vesting.js", "./scaling.js", "./dlcOracleBridge.js", "./procedural.js"].includes(name)) return inert(name);
    throw Error(`unrecognized source dependency ${name}`);
  }
  function evaluate(key, source) {
    const box = { exports: {} };
    vm.runInNewContext(source, { module: box, exports: box.exports, require: dependency,
      console: { log() {}, error() {}, warn() {} } }, { filename: `pinned-${key}.js`, timeout: 1000 });
    modules[key] = box.exports;
  }
  for (const key of ["options", "registry", "margin", "order"]) {
    const [filename, hash] = hashes[key];
    evaluate(key, readPinnedSource(path.join(fixtures, filename), hash));
  }
  const validityFilename = path.join(fixtures, "legacy_contract_validity_source.txt");
  if (!fs.existsSync(validityFilename)) throw Error("Full pinned validity archive is required; no rewritten admission fallback");
  const validityHash = "ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00";
  evaluate("validity", readPinnedSource(validityFilename, validityHash));
  const definition = (inverse, native, notional, leverage) => ({ native, inverse, underlyingOracleId: native ? 0 : 1,
    notionalPropertyId: 8, notionalValue: Number(notional), collateralPropertyId: 7, leverage: Number(leverage),
    onChainData: native ? [[8, 7]] : [], expiryPeriod: 0, series: 1, fee: false, whitelist: 0 });
  async function create(inverse = false, native = false, notional = "1", leverage = "10") {
    const params = definition(inverse, native, notional, leverage);
    const id = await modules.registry.createContractSeries("issuer", params, 2);
    return { id, params };
  }
  function position(contracts = settings.contracts, avgPrice = settings.avgPrice) {
    return { address: "alice", contracts, margin: 100, avgPrice, unrealizedPNL: 0, liqPrice: null, bankruptcyPrice: null };
  }
  function seedPosition() {
    const value = { _id: JSON.stringify({ seriesId: 1 }), value: JSON.stringify([["alice", position()]]) };
    stores.set("marginMaps", [value]);
  }
  return { ...modules, settings, trace, stores, create, definition, position, seedPosition };
}

async function main() {
  for (const native of [false, true]) for (const inverse of [false, true]) {
    for (const [notional, leverage, price, amount] of [
      ["1", "10", "3", "0.25"], ["1", "10", "1.00000005", "1"],
      ["10000", "10", "104", "1.5"], ["2.5", "3", "7.12345678", "0.12345678"],
      ["0.00000001", "10", "100", "1"], ["9007199254740993", "10", "100", "0.5"],
    ]) for (const maker of [false, true]) {
      const env = load(); const { id } = await env.create(inverse, native, notional, leverage);
      const margin = await env.registry.getInitialMargin(id, price);
      const valued = await env.registry.getNotionalValue(id, price);
      const fee = new env.order("1").calculateFee({ amountBuy: amount, amountSell: amount,
        buyMaker: maker, sellMaker: false, isInverse: inverse, lastMark: price,
        notionalValue: valued.notionalValue, channel: false });
      const maintenance = await new env.margin(1).computeMaintenanceMarginRequirement(1, env.position(Number(amount), Number(price)));
      output("CONTEXT", +native, +inverse, scalar(Number(notional)), scalar(Number(leverage)), price, amount, +maker,
        scalar(margin), scalar(valued.notionalValue), scalar(fee.buyerFee), scalar(fee.sellerFee), scalar(maintenance));
    }
  }
  for (const inverse of [false, true]) {
    const env = load(); await env.create(inverse, false, inverse ? "10000" : "1", "10");
    const map = new env.margin(1); let position = env.position(0, 0);
    for (const [amount, price, sell, close, flip] of [[1, 100, false, 0, 0], [1, 200, false, 0, 0],
      [0.5, 125, true, 0.5, 0], [2, 90, true, 1.5, 0.5], [0.25, 80, true, 0, 0],
      [0.25, 85, false, 0.25, 0], [0.5, 95, false, 0.5, 0]]) {
      position = await map.updateContractBalances("alice", amount, price, sell, position, inverse, close, flip, 1, false, 3, false);
      const im = position.contracts === 0 ? 0 : await env.registry.getInitialMargin(1, position.avgPrice);
      const maintenance = await map.computeMaintenanceMarginRequirement(1, position);
      output("POSITION", +inverse, amount, price, +sell, close, flip, scalar(position.contracts), scalar(position.avgPrice), scalar(im), scalar(maintenance));
    }
  }
  const admission = [
    ["inside", {}, true], ["lower-equality", { price: 95.65 }, false], ["upper-equality", { price: 104.35 }, false],
    ["inside-one-tick", { price: 104.34999999 }, true], ["zero-price", { price: 0 }, false],
    ["missing-country", { country: null }, false], ["banned-country", { country: "US" }, false],
    ["negative-amount", { amount: -1 }, true], ["fractional-undo-negative", { amount: 0.25, available: 0 }, true],
    ["reducing-no-available", { amount: 1, sell: true, contracts: 1, avgPrice: 100, available: 0 }, true],
    ["flip-tail-exact", { amount: 2, sell: true, contracts: 1, avgPrice: 100, available: 10 }, true],
    ["property-list-enabled", { whitelist: 99, membership: true }, true],
    ["property-list-denied", { whitelist: 99 }, false],
  ];
  for (const [name, changes, expected] of admission) {
    const env = load(); await env.create(); Object.assign(env.settings, changes); env.seedPosition();
    const doc = env.stores.get("contractList")[0].data;
    if ("whitelist" in changes) doc.whitelist = changes.whitelist;
    const params = { contractId: 1, price: 100, amount: 1, sell: false, senderAddress: "alice", block: 3, ...changes };
    env.trace.length = 0;
    const result = await env.validity.validateTradeContractOnchain("alice", params, "tx");
    assert.equal(result.valid, expected, name);
    output("ADMISSION", name, +result.valid, scalar(result.amount), result.reason.replace(/[\r\n\t]/g, " "), env.trace.join("|"));
  }
  for (const [name, changes, expected] of [
    ["oracle-good", {}, true], ["missing-oracle", { underlyingOracleId: 404 }, false],
    ["native-empty", { native: true, onChainData: [] }, false],
    ["native-missing-property-promise", { native: true, onChainData: [[404, 7]] }, true],
    ["negative-notional", { notionalValue: -1 }, true], ["zero-leverage", { leverage: 0 }, true],
    ["negative-expiry", { expiryPeriod: -1 }, true], ["zero-series", { series: 0 }, true],
  ]) {
    const env = load(); env.settings.missingProperty = 404;
    const params = { ...env.definition(false, false, "1", "10"), block: 2, ...changes };
    const result = await env.validity.validateCreateContractSeries("issuer", params, "tx");
    assert.equal(result.valid, expected, name);
    output("CREATION", name, +result.valid, result.reason.replace(/[\r\n\t]/g, " "), env.trace.join("|"));
  }
  const text = "# full-source composed contract risk and separately characterized admission; not chain authorization\n" + rows.join("\n") + "\n";
  const filename = path.join(fixtures, "contract_binding_vectors.tsv");
  if (process.argv.includes("--check")) assert.equal(fs.readFileSync(filename, "utf8").replace(/\r\n/g, "\n"), text);
  else fs.writeFileSync(filename, text);
  console.log(`contract binding source vectors: ${rows.length} cases (${rows.filter(row => row.startsWith("CONTEXT")).length} composed numeric, ${rows.filter(row => row.startsWith("POSITION")).length} position, ${rows.filter(row => row.startsWith("ADMISSION")).length + rows.filter(row => row.startsWith("CREATION")).length} admission observations)`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
