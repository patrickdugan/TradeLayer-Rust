#!/usr/bin/env node
"use strict";

// Exact-source audit only. This is not a mint plan or an authorization adapter.
// Four complete CommonJS source files execute unchanged. Only their explicit
// DB/RPC/contract/insurance/clearlist/UUID/timer capabilities are supplied here.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const crypto = require("node:crypto");
const assert = require("node:assert/strict");
const repo = path.resolve(__dirname, "..");
const dir = path.join(repo, "fixtures/native_genesis");
require("./native_source_oracle.cjs").verifyNativeSources(repo);
const { loadBigNumber, BIG_NUMBER_SHA256 } = require(path.join(repo, "scripts/legacy_numeric_oracle.cjs"));
const BN = loadBigNumber();
const pin = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const hashes = {
  activation: "268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e",
  vesting: "4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b",
  property: "ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1",
  tally: "cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef",
};
const sources = Object.fromEntries(Object.entries(hashes).map(([name, hash]) => {
  const bytes = fs.readFileSync(path.join(dir, "source", name + ".js"));
  assert.equal(crypto.createHash("sha256").update(bytes).digest("hex"), hash);
  return [name, bytes.toString("utf8")];
}));
const copy = value => structuredClone(value);
function diagnostic(value) {
  if (value === undefined) return { $undefined: true };
  if (typeof value === "number" && !Number.isFinite(value)) return { $number: String(value) };
  if (Object.is(value, -0)) return { $number: "-0" };
  if (value instanceof Error || Object.prototype.toString.call(value) === "[object Error]") return { $error: { name: value.name, message: value.message } };
  if (Array.isArray(value)) return Array.from(value, diagnostic);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, diagnostic(v)]));
  return value;
}
function environment(options = {}) {
  const chain = options.chain ?? "LTC", test = options.test ?? false;
  const tables = new Map(), cache = new Map(), events = [], pending = [];
  let uuid = 0, deltaInserts = 0, tallySaves = 0, contract = 0;
  const mark = (target, args = []) => {
    if (events.length >= 2048) throw Error("local audit event bound");
    events.push({ target, arguments: diagnostic(args) });
  };
  const table = name => {
    if (!tables.has(name)) tables.set(name, new Map());
    return tables.get(name);
  };
  if (options.existingProperty) {
    table("propertyList").set("propertyIndex", { _id: "propertyIndex", value: JSON.stringify([[5, { ticker: "OLD", totalInCirculation: 7, type: 1, whitelistId: 0, issuer: "old" }]]) });
  }
  if (options.initialized) table("tallyMap").set("$TLinit", { _id: "$TLinit", initialized: true });
  function database(category) {
    const data = table(category);
    return {
      async findAsync(query) { mark("DB.findAsync", [category, query]); return Array.from(data.values(), copy); },
      async findOneAsync(query) { mark("DB.findOneAsync", [category, query]); return copy(data.get(query._id) ?? null); },
      async updateAsync(query, update, opts) {
        mark("DB.updateAsync", [category, query, update, opts]);
        if (category === "tallyMap" && query._id === "tallyMap" && ++tallySaves === options.failTallySaveAt) throw Error("injected tally save failure");
        if (category === "tallyMap" && query._id === "$TLinit" && options.failFlag) throw Error("injected initialization flag failure");
        const old = data.get(query._id) ?? { _id: query._id };
        data.set(query._id, copy(update.$set ? { ...old, ...update.$set } : update));
        if (data.size > 64) throw Error("local audit DB bound");
        return 1;
      },
      async insertAsync(doc) {
        mark("DB.insertAsync", [category, doc]);
        if (category === "tallyMapDelta" && ++deltaInserts === options.failDeltaAt) throw Error("injected delta write failure");
        data.set(doc._id, copy(doc)); return copy(doc);
      },
    };
  }
  function launched(target, args) {
    mark(target, args);
    if (options.deferLaunches) {
      let resolve;
      const promise = new Promise(r => { resolve = r; });
      pending.push({ target, resolve });
      return promise;
    }
    return Promise.resolve(0);
  }
  class Insurance {
    constructor(...args) { mark("Insurance.constructor", args); }
    deposit(...args) { return launched("Insurance.deposit", args); }
  }
  const capabilities = {
    "./db.js": { getDatabase(category) { mark("DB.getDatabase", [category]); return options.synchronousDbFactory ? database(category) : Promise.resolve(database(category)); } },
    "./client.js": { async getInstance() { mark("Client.getInstance"); return {
      async getChain(...args) { mark("Client.getChain", args); return chain; },
      async getTest() { mark("Client.getTest"); return test; },
    }; } },
    "./consensus": { async pushLatestActivationToConsensusVector() { mark("Consensus.pushLatestActivationToConsensusVector"); } },
    "./insurance.js": Insurance,
    "./clearlist.js": { createClearlist(...args) { return launched("ClearList.createClearlist", args); } },
    "./contractRegistry.js": { async createContractSeries(...args) { mark("Contract.createContractSeries", args); return ++contract; } },
    "./txUtils.js": {}, "./orderbook.js": {},
    "bignumber.js": BN, "uuid": { v4() { mark("UUID.v4"); return "genesis-audit-" + ++uuid; } },
    "path": path,
  };
  function load(name) {
    if (cache.has(name)) return cache.get(name).exports;
    assert(Object.hasOwn(sources, name), name);
    const module = { exports: {} };
    cache.set(name, module);
    vm.runInNewContext(sources[name], {
      module, exports: module.exports,
      console: { log() {}, error() {}, warn() {} },
      setTimeout(resolve, delay) { mark("Timer.setTimeout", [delay]); resolve(); },
      require(target) {
        const relative = /^\.\/(activation|vesting|property|tally)\.js$/.exec(target);
        if (relative) return load(relative[1]);
        assert(Object.hasOwn(capabilities, target), target);
        return capabilities[target];
      },
    }, { filename: `pinned-${name}.js`, timeout: 2000 });
    return module.exports;
  }
  const Activation = load("activation"), Manager = load("vesting"), Property = load("property"), Tally = load("tally");
  async function settle() { for (let i = 0; i < 12; ++i) await Promise.resolve(); }
  function snapshot() {
    const propertyDoc = table("propertyList").get("propertyIndex");
    const tallyDoc = table("tallyMap").get("tallyMap");
    const activationDoc = table("activations").get("activationsList");
    return {
      activations: activationDoc ? JSON.parse(activationDoc.value) : null,
      initializationFlag: table("tallyMap").get("$TLinit")?.initialized ?? false,
      properties: propertyDoc ? JSON.parse(propertyDoc.value) : [],
      balances: tallyDoc ? JSON.parse(tallyDoc.data) : [],
      liveBalances: Tally.addresses ? Array.from(Tally.addresses) : [],
      deltaRecords: Array.from(table("tallyMapDelta").values()),
      manager: Manager.instance ? Object.fromEntries(Object.entries(Manager.instance)) : null,
      pendingLaunches: pending.map(x => x.target),
    };
  }
  return { Activation, Manager, Property, Tally, tables, events, settle, snapshot, release() { for (const p of pending) p.resolve(0); } };
}
function supply(snapshot, events) {
  return snapshot.properties.map(([id, p]) => {
    let owned = new BN(0), vestingMarker = new BN(0);
    for (const [, properties] of snapshot.balances) {
      const b = properties[id]; if (!b) continue;
      for (const key of ["available", "reserved", "margin", "channelBalance"]) owned = owned.plus(b[key] || 0);
      vestingMarker = vestingMarker.plus(b.vesting || 0);
    }
    const deposits = events.filter(x => x.target === "Insurance.deposit" && x.arguments[0] === id).reduce((sum, x) => sum.plus(x.arguments[1]), new BN(0));
    return { id, ticker: p.ticker, declared: String(p.totalInCirculation), tallyOwned: owned.toFixed(), vestingMarker: vestingMarker.toFixed(), insuranceLaunched: deposits.toFixed(), declaredLessOwnedAndLaunched: new BN(p.totalInCirculation).minus(owned).minus(deposits).toFixed() };
  });
}
async function activationScene(name, options, actions) {
  const env = environment(options), activation = env.Activation.getInstance();
  if (!options.skipInit) await activation.init(options.chain);
  const beforeAdmin = activation.getAdmin(), outputs = [];
  for (const [height, hash] of actions) {
    let outcome;
    try { outcome = { returned: await activation.activate(0, height, hash) }; }
    catch (e) { outcome = { thrown: { name: e.name, message: e.message } }; }
    await env.settle();
    const state = env.snapshot();
    outputs.push({ height, outcome, state: copy(state), supply: supply(state, env.events) });
  }
  const result = diagnostic({ name, options, beforeAdmin, outputs, events: env.events });
  env.release(); await env.settle();
  return result;
}
async function tallySemantics() {
  const env = environment();
  await env.Tally.updateBalance("alice", 2, 10, 0, 0, 10, "genesis", 77);
  const first = await env.Tally.getTally("alice", 2);
  const total = await env.Tally.getTotalTally(2);
  const check = await env.Tally.hasSufficientBalance("alice", 2, 10);
  await env.Tally.updateBalance("alice", 2, -4, 0, 0, 0, "send", 78);
  const after = await env.Tally.getTally("alice", 2);
  assert.equal(first.amount, 10); assert.equal(first.available, 10); assert.equal(first.vesting, 10);
  assert.equal(total.amount, 10); assert.equal(total.vesting, 10); assert.equal(check.hasSufficient, true);
  assert.equal(after.available, 6); assert.equal(after.vesting, 10);
  return diagnostic({ name: "vesting-is-overlapping-marker-not-owned-bucket", first, total, check, after, state: env.snapshot(), events: env.events });
}
async function redemptionNoop() {
  const env = environment({ chain: "LTC" }), act = env.Activation.getInstance();
  await act.init("LTC"); await act.activate(0, 77, "first");
  const before = env.snapshot();
  const holders = await env.Tally.getAddressesWithBalanceForProperty(2);
  await env.Tally.applyVesting(2, 125000, 78);
  await env.settle();
  const after = env.snapshot();
  assert.deepEqual(Array.from(holders), []);
  assert.deepEqual(after.balances, before.balances);
  return diagnostic({ name: "applyVesting-normal-async-db-no-op", boundary: "Pinned db.getDatabase is async; the unawaited member call is caught and yields no holders", before, holders, after, events: env.events });
}
async function redemptionReplay() {
  // Deliberately separate capability repair: a synchronous factory lets the
  // unchanged downstream source expose its cumulative-target replay behavior.
  // This does NOT describe the ordinary pinned async DB composition.
  const env = environment({ chain: "LTC", synchronousDbFactory: true }), act = env.Activation.getInstance();
  await act.init("LTC"); await act.activate(0, 77, "first");
  const amount = 125000;
  const before = env.snapshot();
  await env.Tally.applyVesting(2, amount, 78);
  const once = env.snapshot();
  await env.Tally.applyVesting(2, amount, 79);
  const twice = env.snapshot();
  let third;
  try { await env.Tally.applyVesting(2, amount, 80); third = { returned: true }; }
  catch (e) { third = { thrown: { name: e.name, message: e.message } }; }
  const address = act.getAdmin();
  const balance = (state, id, field) => state.balances.find(x => x[0] === address)[1][id][field];
  assert.equal(balance(before, 2, "vesting"), 250000);
  assert.equal(balance(once, 2, "vesting"), 125000);
  assert.equal(balance(twice, 2, "vesting"), 0);
  assert.equal(balance(twice, 2, "available"), 250000);
  assert.equal(balance(twice, 1, "available"), 300000);
  assert(third.thrown);
  return diagnostic({ name: "conditional-sync-db-repair-repeats-cumulative-target", boundary: "Explicit synchronous DB-factory repair only; source modules remain unchanged", amount, before, once, twice, third, events: env.events });
}
async function main() {
  const scenes = [];
  for (const chain of ["BTC", "LTC", "DOGE"]) {
    scenes.push(await activationScene("fresh-" + chain, { chain }, [[77, "first"]]));
    scenes.push(await activationScene("fallback-admin-" + chain, { chain, skipInit: true }, [[77, "first"]]));
    scenes.push(await activationScene("repeat-" + chain, { chain }, [[77, "first"], [78, "second"]]));
  }
  scenes.push(await activationScene("nonempty-property-catalog", { chain: "LTC", existingProperty: true }, [[77, "first"]]));
  scenes.push(await activationScene("already-initialized-without-genesis-record", { chain: "LTC", initialized: true }, [[77, "first"]]));
  scenes.push(await activationScene("deferred-insurance-and-clearlists", { chain: "LTC", deferLaunches: true }, [[77, "first"]]));
  scenes.push(await activationScene("delta-failure-and-retry", { chain: "LTC", failDeltaAt: 2 }, [[77, "first"], [78, "second"]]));
  scenes.push(await activationScene("swallowed-tally-save-failure", { chain: "LTC", failTallySaveAt: 3 }, [[77, "first"]]));
  scenes.push(await activationScene("initialization-flag-failure-and-retry", { chain: "LTC", failFlag: true }, [[77, "first"], [78, "second"]]));
  scenes.push(await tallySemantics());
  scenes.push(await redemptionNoop());
  scenes.push(await redemptionReplay());
  for (const chain of ["BTC", "LTC", "DOGE"]) {
    const s = scenes.find(x => x.name === "fresh-" + chain), output = s.outputs[0];
    assert.equal(output.state.initializationFlag, true);
    assert.deepEqual(output.state.properties.map(x => x[0]), [1, 2, 3, 4]);
    assert.equal(output.state.properties[2][1].totalInCirculation, { BTC: 300000, LTC: 1500000, DOGE: 600000000 }[chain]);
    assert.equal(output.supply[2].tallyOwned, "1500000");
    const hedge = s.events.find(x => x.target === "Contract.createContractSeries").arguments;
    assert.deepEqual(hedge[0], { $undefined: true });
    assert.deepEqual(hedge[1].expiryInterval, { $undefined: true });
    assert.equal(hedge[1].leverage, 5); assert.equal(hedge[1].collateralPropertyId, 1);
    for (const list of s.events.filter(x => x.target === "ClearList.createClearlist")) {
      assert.deepEqual(list.arguments[0], { $undefined: true });
      assert.deepEqual(list.arguments[4], { $undefined: true });
    }
    assert(output.state.deltaRecords.some(x => x.data.total?.$number === "NaN"));
  }
  const deferred = scenes.find(x => x.name === "deferred-insurance-and-clearlists").outputs[0].state;
  assert.equal(deferred.initializationFlag, true); assert.equal(deferred.pendingLaunches.length, 4);
  const evidence = { sourceCommit: pin, sourceHashes: hashes, bignumber: { version: "9.3.1", sha256: BIG_NUMBER_SHA256 }, boundary: "Source evidence over explicit deterministic capabilities; no Ledger minting or historical/authenticated admission", scenes };
  const bytes = JSON.stringify(evidence, null, 2) + "\n", file = path.join(dir, "evidence.json");
  if (process.argv.includes("--write")) fs.writeFileSync(file, bytes);
  else assert.equal(fs.readFileSync(file, "utf8"), bytes);
  const reconciliation = {
    interpretation: "Arithmetic observations only. Source totalInCirculation is a declared field; it is not proven equal to current issued custody or a formally enforced cap. Insurance amounts are launched calls, not proven custody. Vesting markers overlap available coupon units. Differences below authorize no mint and identify no beneficiary.",
    networks: ["BTC", "LTC", "DOGE"].map(chain => {
      const scene = scenes.find(x => x.name === "fresh-" + chain), s = scene.outputs[0].state, rows = scene.outputs[0].supply;
      const admin = scene.beforeAdmin, p = s.manager;
      const amount = (owner, id, bucket) => s.balances.find(x => x[0] === owner)?.[1][id]?.[bucket] ?? 0;
      const atoms = value => new BN(value).times(100000000).toFixed();
      const unrepresented = new BN(rows[0].declaredLessOwnedAndLaunched);
      return {
        chain, admin, sponsor: p.sponsorAddress, initialParameterTokens: String(p.initialTokenAmount),
        fractions: { sale: p.salePortion, free: p.freePortion, insurance: p.insurancePortion, vesting: p.vestingPortion },
        roles: rows.map(row => ({ ...row, sourceType: s.properties.find(x => x[0] === row.id)[1].type, declaredAtoms: atoms(row.declared), tallyOwnedAtoms: atoms(row.tallyOwned), vestingMarkerAtoms: atoms(row.vestingMarker) })),
        property1Arithmetic: {
          adminAvailable: String(amount(admin, 1, "available")), sponsorAvailable: String(amount(p.sponsorAddress, 1, "available")),
          insuranceCallLaunched: rows[0].insuranceLaunched,
          declarationLessCurrentTallyAndLaunchedInsurance: unrepresented.toFixed(),
          property2MarkerPotentialRedemption: rows[1].vestingMarker,
          differenceAfterHypotheticalFullProperty2Backing: unrepresented.minus(rows[1].vestingMarker).toFixed(),
          noResidualBeneficiaryOrMintAuthorityInferred: true,
        },
        nativeContractRequest: scene.events.find(x => x.target === "Contract.createContractSeries").arguments,
        clearlistRequests: scene.events.filter(x => x.target === "ClearList.createClearlist").map(x => x.arguments),
      };
    }),
    failureSummary: scenes.filter(x => x.outputs && !x.name.startsWith("fresh-") && !x.name.startsWith("fallback-") && !x.name.startsWith("repeat-")).map(s => ({
      name: s.name,
      outputs: s.outputs.map(o => ({ height: o.height, outcome: o.outcome, initialized: o.state.initializationFlag, activation0: o.state.activations?.[0], ids: o.state.properties.map(x => x[0]), tallyOwned: o.supply.map(x => [x.id, x.tallyOwned]), pendingLaunches: o.state.pendingLaunches })),
    })),
  };
  const report = JSON.stringify(reconciliation, null, 2) + "\n", reportFile = path.join(dir, "supply-reconciliation.json");
  if (process.argv.includes("--write")) fs.writeFileSync(reportFile, report);
  else assert.equal(fs.readFileSync(reportFile, "utf8"), report);
  console.log(JSON.stringify({ scenes: scenes.length, events: scenes.reduce((n, s) => n + s.events.length, 0), sha256: crypto.createHash("sha256").update(bytes).digest("hex"), sourceCommit: pin }));
}
main().catch(e => { console.error(e); process.exitCode = 1; });
