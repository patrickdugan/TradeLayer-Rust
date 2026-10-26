#!/usr/bin/env node
"use strict";

// Execute the complete unmodified vesting.js. Dependencies expose only local
// call journals; no RPC, persistent legacy DB, wallet, or insurance transfer.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const SOURCE_SHA = "4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b";
const sourceIndex = process.argv.indexOf("--source");
const sourcePath = sourceIndex < 0 ? path.join(fixtures, "legacy_vesting_source.txt") : process.argv[sourceIndex + 1];
const source = readPinnedSource(sourcePath, SOURCE_SHA);
const number = n => Object.is(n, -0) ? "-0" : String(n);
const list = values => values.map(number).join(",");
const rows = [];
const row = (...fields) => rows.push(fields.join("\t"));

function load(options = {}) {
  const journal = [], balances = [], attempts = [], delays = [];
  let property = options.firstProperty ?? 1, initialized = options.initialized ?? false;
  const mark = (...fields) => journal.push(fields.map(String).join("/"));
  const tally = {
    checkInitializationFlag: async () => { mark("check"); return initialized; },
    updateBalance: async (...args) => {
      mark("balance", ...args);
      if (options.failBalance && balances.length === options.failBalance - 1) throw Error("injected balance failure");
      balances.push(args);
    },
    getAddressBalances: async address => { mark("balances", address); return balances.filter(b => b[0] === address); },
    setInitializationFlag: async () => { mark("flag"); initialized = true; }
  };
  const propertyManager = {
    getInstance: () => { mark("pm"); return propertyManager; },
    createToken: async (...args) => { mark("token", ...args, undefined, undefined); return property++; },
    getPropertyData: async id => options.properties?.[id] ?? null,
  };
  class Insurance {
    constructor(...args) { mark("insurance", ...args); }
    deposit(...args) { mark("deposit", ...args); return Promise.resolve(); }
  }
  const sandbox = {
    module: { exports: {} }, console: { log() {}, error() {} },
    setTimeout: (resolve, delay) => { delays.push(delay); resolve(); },
    require: name => {
      if (name === "bignumber.js") return loadBigNumber();
      if (name === "./insurance.js") return Insurance;
      if (name === "./property.js") return propertyManager;
      if (name === "./tally.js") return tally;
      if (name === "./clearlist.js") return { createClearlist: (...args) => { mark("clear", ...args); return Promise.resolve(); } };
      if (name === "./contractRegistry.js") return { createContractSeries: async (admin, p, block) => {
        mark("contract", admin, p.expiryInterval, p.leverage, block,
          p.initialMargin, p.maintenanceMargin, p.expiry, p.onChainData.join(","),
          p.seriesLength, p.native, p.inverse, p.fee, p.notionalPropertyId, p.notionalValue, p.collateralPropertyId);
        // Neither expiryPeriod nor series is passed to the actual registry.
        assert.equal(p.expiryPeriod, undefined); assert.equal(p.series, undefined);
        return 7;
      } };
      if (name === "./client.js") return { getInstance: async () => ({
        getChain: async () => {
          const attempt = options.probes?.[attempts.length] ?? [null, undefined];
          attempts.push(attempt);
          if (attempt instanceof Error) throw attempt;
          return attempt[0];
        },
        getTest: async () => attempts.at(-1)[1],
      }) };
      throw Error(`unexpected dependency ${name}`);
    },
  };
  vm.runInNewContext(source, sandbox, { filename: sourcePath, timeout: 1000 });
  return { Manager: sandbox.module.exports, sandbox, journal, balances, attempts, delays,
    initialized: () => initialized };
}

async function main() {
  for (const chain of ["BTC", "DOGE", "LTC", "unknown", "btc"]) {
    const { Manager } = load();
    const manager = new Manager("admin", chain, false);
    manager.setChainParams("IGNORED");
    row("CONFIG", chain, manager.baseVolume, manager.minRebate, manager.maxRebate,
      manager.initialTokenAmount, manager.tickerSymbol, manager.hedgeLeverage,
      manager.expiryInterval, manager.sponsorAddress, manager.freePortion,
      manager.insurancePortion, manager.vestingPortion, manager.salePortion);
    for (const volume of [-1, 0, 10, manager.baseVolume, manager.baseVolume * 10,
      1e8, 1e10, Infinity, NaN]) row("REBATE", chain, number(volume), number(Manager.calculateTradeRebates(volume)));
  }
  const numeric = load({ properties: { 2: { totalInCirculation: 250000 }, 3: { totalInCirculation: 1500000 } } }).Manager;
  for (const v of [
    [0, 0, 0, 0], [999, 1, 9999999999, 1], [999, 2, 9999999999, 2],
    [500000000, 0, 500000000000, 0], [1e9, 1, 1e12, 1],
    [-1000, -10, -10, -1], [Infinity, 0, Infinity, 0], [NaN, 0, NaN, 0],
  ]) { const out = await numeric.updateVesting(...v); row("VEST", list(v), list([out.two, out.three])); }
  assert.equal(await load().Manager.updateVesting(1, 2, 3, 4), null);
  row("ERROR", "missing_properties", "null");
  for (const v of [[0, 100, 1000, 50], [100, 100, 1000, 50], [101, 100, 1000, 50],
    [500, 100, 1000, 50], [1000, 100, 1000, 50], [2000, 100, 1000, 50],
    [500, 100, 1000, 0], [500, 100, 1000, -1], [500, 100, 0, 50], [NaN, 1, 2, 1]])
    row("FRACTION", list(v), number(numeric.vestFraction(...v)));
  for (const v of [[100, 0], [100, -1], [100, 1e-18], [100, 0.05], [0, 0.1], [100, 1], [100, NaN]])
    row("PROJECT", list(v), number(numeric.projectNextYearVolumeBTC(...v)));
  for (const v of [[0, 1000, 100, 0], [1000, 1000, 0, 0.1], [0, 1000, 0, 0.1],
    [0, 1000, 100, 0.05], [0, 1001, 100, -0.1], [0, 1000, 100, NaN], [Infinity, 1000, 100, 0]])
    row("MONTHS", list(v), number(numeric.monthsToReach(...v)));
  for (const value of [-1, -0.1, 0, 0.01, 0.05, 0.1, 1, NaN]) row("ANNUAL", number(value), number(numeric.annualizeFromCMGR(value)));
  for (const v of [[0.08, 0], [0.08, 12], [0.08, 240], [0.08, Infinity], [0, Infinity], [-0.1, 12], [NaN, 1]])
    row("DISCOUNT", list(v), number(numeric.discountFactorMonths(...v)));
  for (const series of [[], [100], [100, 110], [100, 110, 121], [100, 0, 100],
    [0, 0, 0], [-1, 2, -3], [100, 80, 200, 300], [NaN, 100]])
    row("CMGR", series.length ? list(series) : "-", number(numeric.cmgrFromSeries(series)));
  const books = [[], [[2, 5], [1, 10]], [[2, 5], [1, 10]], [[0, 3], [1, 7]],
    [[2, 5]], [[2, -3]], [[-2, 3]], [[NaN, 1]], [[Infinity, 1]], [[1, 0], [2, 3]]];
  const funds = [10, 10, 13, 2, -1, 10, 10, 10, 10, 3];
  books.forEach((book, i) => {
    const output = numeric.calculateBuybackAmount({ funds: funds[i] }, book.map(([price, amount]) => ({ price, amount })));
    row("BUYBACK", number(funds[i]), book.length ? book.map(list).join(";") : "-", number(output));
  });
  assert.equal(numeric.fetchOrderbookForToken(1), undefined);
  assert.equal(numeric.performBuyback([]), undefined);
  assert.throws(() => numeric.performBuyback([{ tokenId: 1, funds: 1 }]), /orderbook is not iterable/);
  assert.throws(() => numeric.computeTokensForTrade({ notionalBtc: 1 }), /projectedYearVolumeBTC is not defined/);
  row("ERROR", "fetch_orderbook", "undefined"); row("ERROR", "perform_empty", "undefined");
  row("ERROR", "perform_nonempty", "TypeError:orderbook is not iterable");
  row("ERROR", "compute_tokens", "ReferenceError:projectedYearVolumeBTC is not defined");

  // Explicit repair oracle: source bytes are unchanged, but an ambient binding
  // resolves its misspelled return identifier to the independently called
  // projection helper. Ordinary source failure is asserted above.
  const quoteCases = [
    [1, false, {}], [1, true, {}], [0, false, {}], [-1, true, {}],
    [100, false, { cmgr12: 0 }], [100, false, { cmgr12: -0.1 }],
    [10, true, { monthlyVolumeBTC: 0 }], [10, false, { tlvestEffectiveSupply: 0 }],
    [10, false, { priceTLIinBTC: 0 }], [10, false, { cumulativeVolumeBTC: 21000000 }],
    [10, false, { discountRateAnnual: -1, cmgr12: 0 }], [10, true, { feeRate: 0 }],
  ];
  for (const [notionalBtc, vip, params] of quoteCases) {
    const env = load();
    env.sandbox.projectedYearVolumeBTC = env.Manager.projectNextYearVolumeBTC(
      params.monthlyVolumeBTC ?? 50000, params.cmgr12 ?? 0.05);
    const output = env.Manager.computeTokensForTrade({ notionalBtc, vip }, params);
    row("REPAIRED_QUOTE", number(notionalBtc), vip,
      Object.keys(params).length ? Object.entries(params).map(([k, v]) => `${k}=${number(v)}`).join(",") : "-",
      list([output.tokens, output.awardValueBTC, output.pvTokenBTC, output.vestFractionNow,
        output.monthsToFullVest, output.components.pvDividendBTC, output.components.pvConversionBTC,
        output.components.gAnnual, output.components.projectedYearVolumeBTC,
        output.components.perTokenDividendBTC, output.components.feeBTC]));
  }

  const initializationCases = [
    ["BTC", false, undefined, 1, false], ["BTC", true, undefined, 1, false],
    ["DOGE", false, undefined, 1, false], ["DOGE", true, undefined, 1, false],
    ["LTC", false, undefined, 1, false], ["LTC", true, undefined, 1, false],
    ["unknown", false, undefined, 1, false], ["LTC", true, "specified-admin", 17, false],
    ["LTC", false, undefined, 1, true],
  ];
  for (const [chain, test, admin, firstProperty, initialized] of initializationCases) {
    const env = load({ firstProperty, initialized });
    const manager = new env.Manager(admin, chain, test); manager.setChainParams(chain);
    const result = await manager.initializeTokens(77);
    assert.equal(env.initialized(), true);
    row("INIT", chain, test, admin ?? "undefined", firstProperty, initialized,
      String(manager.adminAddress), result === undefined ? "undefined" : result.length,
      env.journal.join(";"));
    if (!initialized) {
      env.journal.length = 0;
      assert.equal(await manager.initializeTokens(77), undefined);
      assert.deepEqual(env.journal, ["check"]);
    }
  }
  const failed = load({ failBalance: 3 }); const fm = new failed.Manager("admin", "LTC", false); fm.setChainParams("LTC");
  await assert.rejects(() => fm.initializeTokens(77), /injected balance failure/);
  assert.equal(failed.initialized(), false);
  row("INIT_FAILURE", failed.journal.join(";"));

  const chainCases = [
    ["LTC", false, []], ["BTC", undefined, [["DOGE", true]]],
    [undefined, undefined, [[null, undefined], ["", false], ["LTC", false]]],
    ["DOGE", undefined, []], [undefined, true, []],
    [undefined, undefined, [Error("offline"), ["BTC", false]]],
  ];
  for (let index = 0; index < chainCases.length; index++) {
    const [chain, test, probes] = chainCases[index]; const env = load({ probes });
    const manager = new env.Manager(undefined, chain, test); await manager.ensureChainInitialized();
    row("CHAIN", index, String(manager.getChain()), String(manager.getTest()), env.attempts.length,
      env.delays.length, manager.tickerSymbol);
  }
  const singleton = load({ probes: [["LTC", true]] });
  const first = await singleton.Manager.getInstance("first-admin", "BTC", false);
  const second = await singleton.Manager.getInstance("second-admin", "DOGE", false);
  assert.equal(first, second); assert.equal(first.test, true);
  row("SINGLETON", first.adminAddress, first.chain, first.test, singleton.attempts.length);
  row("EPS", numeric.EPS);
  const output = "# Pinned unmodified vesting.js dca0888; binary64 math allows host-libm tolerance in Rust.\n" + rows.join("\n") + "\n";
  const target = path.join(fixtures, "vesting_vectors.tsv");
  if (process.argv.includes("--write")) fs.writeFileSync(target, output);
  else assert.equal(fs.readFileSync(target, "utf8"), output, "vesting source vectors");
  console.log(`vesting: ${rows.length} source cases (12 explicitly repaired-return quotes), SHA ${SOURCE_SHA}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
