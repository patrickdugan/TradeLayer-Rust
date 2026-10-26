#!/usr/bin/env node
"use strict";

// The COMPLETE unchanged adjuster.js executes, including its top-level example.
// A synchronous DB-shaped recording adapter exposes successful event arguments;
// the independently executed unchanged db.js demonstrates the real failure.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const util = require("node:util");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SHA = "b6eea50581a9cc19f2296bae180b528c0116900a73662e5ce9a8ce762c36f77c";
const DB_SHA = "b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38";
const args = process.argv.slice(2);
const sourceIndex = args.indexOf("--source");
if (sourceIndex >= 0) assert.ok(args[sourceIndex + 1], "--source requires adjuster.js");
assert.ok(!(args.includes("--check") && args.includes("--write")), "choose --check or --write");
const source = readPinnedSource(sourceIndex < 0 ? path.join(fixtures, "legacy_adjuster_source.txt") : args[sourceIndex + 1], SHA);
const dbSource = readPinnedSource(path.join(fixtures, "legacy_db_source.txt"), DB_SHA);
const hex = value => Buffer.from(String(value), "utf8").toString("hex") || "-";
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString("hex"); };
const fromBits = value => Buffer.from(value, "hex").readDoubleBE();
const pack = rows => rows.length ? rows.join(";") : "-";
const eventText = event => [hex(event.type), bits(event.value), event.time].join(":");
const A = (value, time = 100) => ({ method: "A", value, time });
const N = (time = 100) => ({ method: "N", time });
const S = (type, value, time = 100) => ({ method: "S", type, value, time });

async function actualDb() {
  let updates = 0;
  class Datastore {
    constructor(options) { assert.equal(options.autoload, true); }
    find() {} insert() {} remove() {} findOne() {} count() {}
    update() { updates++; throw Error("real adjuster must never reach a Datastore update"); }
  }
  const module = { exports: {} };
  vm.runInNewContext(dbSource, {
    module, exports: module.exports, __dirname: "/pinned/source/src",
    process: { env: { TL_SKIP_RPC_BOOT: "1", CHAIN: "BTC", TL_FORCE_TEST: "0" } },
    console: { log() {}, error() {} },
    require(name) {
      if (name === "nedb") return Datastore;
      if (name === "path") return path;
      if (name === "util") return util;
      if (name === "./client") return { getInstance() { throw Error("RPC unexpectedly used"); } };
      throw Error("unexpected db dependency " + name);
    },
  }, { filename: "complete-pinned-db.js", timeout: 1000 });
  const database = module.exports;
  await database.init();
  assert.equal(Object.keys(database.databases).length, 32);
  assert.equal(Object.hasOwn(database.databases, "contractAdjustments"), false);
  assert.equal(await database.getDatabase("contractAdjustments"), undefined);
  return { database, updates: () => updates };
}

async function load(database = null) {
  const updates = [], failures = [], pending = [], logs = [];
  const clock = { value: 1000 };
  // Record rejections without changing any source method. Symbol.species keeps
  // the host observation handler from recursively creating observed promises.
  class ObservedPromise extends Promise {
    static get [Symbol.species]() { return Promise; }
    constructor(executor) {
      super(executor);
      pending.push(this);
      Promise.prototype.then.call(this, () => {}, error => failures.push(String(error.message)));
    }
  }
  class DeterministicDate extends Date {
    constructor(...values) { super(...(values.length ? values : [clock.value])); }
    static now() { return clock.value; }
  }
  const recording = { getDatabase(category) {
    assert.equal(category, "contractAdjustments");
    return { async updateAsync(query, update, options) {
      assert.equal(query.category, "contractAdjustments");
      assert.equal(options.upsert, true);
      assert.deepEqual(Object.keys(update), ["$push"]);
      const event = update.$push.events;
      updates.push({ type: event.type, value: event.value, time: event.timestamp.getTime() });
      return 1;
    } };
  } };
  const module = { exports: {} };
  const context = vm.createContext({
    module, exports: module.exports, Date: DeterministicDate, Promise: ObservedPromise,
    console: { log: (...values) => logs.push(values.join(" ")) },
    require(name) { assert.equal(name, "./db.js"); return database || recording; },
  });
  vm.runInContext(source, context, { filename: "complete-pinned-adjuster.js", timeout: 1000 });
  await Promise.allSettled(pending);
  const Class = vm.runInContext("TickSizeAdjuster", context);
  const example = vm.runInContext("tickSizeAdjuster", context);
  assert.equal(example.tickSize, 4_000_000);
  assert.equal(example.minContractSize, 10);
  assert.equal(example.consecutiveDoublings, 1);
  assert.equal(logs.length, 9);
  assert.deepEqual(Object.keys(module.exports), [], "source does not export its class");
  const exampleEvents = updates.slice(), exampleFailures = failures.slice();
  const exampleLogs = logs.slice();
  updates.length = 0; failures.length = 0; pending.length = 0;
  return { adjuster: new Class(), Class, clock, updates, failures, pending,
    example, exampleEvents, exampleFailures, exampleLogs };
}

async function invoke(environment, action) {
  environment.clock.value = action.time;
  let result;
  if (action.method === "A") result = environment.adjuster.adjustTickSize(action.value);
  else if (action.method === "N") result = environment.adjuster.increaseNotionalSize();
  else result = await environment.adjuster.saveAdjustmentEvent(action.type, action.value);
  await Promise.allSettled(environment.pending);
  return result;
}

async function main() {
  const rows = [];
  const base = await load();
  assert.equal(base.exampleEvents.length, 10);
  assert.equal(base.exampleFailures.length, 0);
  rows.push(["Q", "complete-source-example", "tick=" + bits(base.example.tickSize),
    "minimum=" + bits(base.example.minContractSize), "counter=1", "events=" + pack(base.exampleEvents.map(eventText)),
    "logs=" + pack(base.exampleLogs.map(hex)), "exports=empty"].join("\t"));
  const below = fromBits("3fffffffffffffff"), above = fromBits("4000000000000001");
  const scenarios = [
    ["constructor", []],
    ["threshold-neighbours", [A(below, 0), A(2, 1), A(above, 2)]],
    ["third-double-before-increase", [A(2, 8), A(2, 7), A(2, 0), A(2, 9), A(2, 8), A(2, 7)]],
    ["below-resets-streak", [A(2), A(2), A(below), A(2), A(2), A(2)]],
    ["finite-nonqualifiers", [A(-0), A(-Number.MAX_VALUE), A(Number.MIN_VALUE), A(0), A(1)]],
    ["minimum-increase-keeps-counter", [A(2), A(2), N(), A(20), A(below * 10), A(20), A(20), A(20)]],
    ["original-example", [A(1), A(5), A(10), A(20), A(50), A(100), A(500), A(1000), N()]],
    ["direct-event-is-not-state", [S("DoubleTickSize", 999, 0), S("", -0, 8_640_000_000_000_000), S("λ🙂", Number.MIN_VALUE, 1)]],
    ["finite-tick-stress", Array.from({ length: 280 }, (_, i) => A(2, i))],
    ["infinite-threshold-finite-state", [A(2), A(2), ...Array.from({ length: 308 }, (_, i) => N(i)), A(Number.MAX_VALUE, 0)]],
  ];
  let finiteRows = 0;
  for (const [name, actions] of scenarios) {
    const environment = await load(), adjuster = environment.adjuster;
    const constructor = ["V", name, 0, "C", "-", 0, "constructed", bits(adjuster.tickSize),
      bits(adjuster.minContractSize), bits(adjuster.notionalThreshold), bits(adjuster.notionalMultiplier),
      adjuster.doublingInterval, adjuster.consecutiveDoublings, "-"];
    rows.push(constructor.join("\t")); finiteRows++;
    for (let index = 0; index < actions.length; index++) {
      const action = actions[index], start = environment.updates.length;
      const returned = await invoke(environment, action);
      assert.equal(environment.failures.length, 0);
      assert.ok(Number.isFinite(adjuster.tickSize) && Number.isFinite(adjuster.minContractSize));
      const arg = action.method === "A" ? bits(action.value) : action.method === "N" ? "-" : hex(action.type) + ":" + bits(action.value);
      rows.push(["V", name, index + 1, action.method, arg, action.time,
        returned === undefined ? "undefined" : bits(returned), bits(adjuster.tickSize), bits(adjuster.minContractSize),
        bits(adjuster.notionalThreshold), bits(adjuster.notionalMultiplier), adjuster.doublingInterval,
        adjuster.consecutiveDoublings, pack(environment.updates.slice(start).map(eventText))].join("\t"));
      finiteRows++;
    }
  }
  for (const [label, value] of [["NaN", NaN], ["positive-Infinity", Infinity], ["negative-Infinity", -Infinity],
    ["undefined", undefined], ["null", null], ["true", true], ["false", false], ["numeric-string", "2"],
    ["invalid-string", "nonsense"], ["empty-array", []], ["array-two", [2]], ["object", {}], ["bigint", 2n], ["symbol", Symbol("x")]]) {
    const environment = await load();
    let error = "-", result;
    try { result = await invoke(environment, A(value)); } catch (caught) { error = hex(caught.message); }
    rows.push(["Q", "coercion-" + label, "return=" + (typeof result === "number" ? bits(result) : String(result)),
      "error=" + error, "tick=" + bits(environment.adjuster.tickSize),
      "counter=" + environment.adjuster.consecutiveDoublings, "events=" + pack(environment.updates.map(eventText))].join("\t"));
  }
  for (const [name, method] of [["tick-state-overflow", "A"], ["minimum-state-overflow", "N"]]) {
    const environment = await load(); let count = 0, before;
    while (Number.isFinite(method === "A" ? environment.adjuster.tickSize : environment.adjuster.minContractSize)) {
      before = method === "A" ? environment.adjuster.tickSize : environment.adjuster.minContractSize;
      await invoke(environment, method === "A" ? A(2, count) : N(count)); count++;
      assert.ok(count < 400);
    }
    rows.push(["Q", name, "call=" + count, "before=" + bits(before), "tick=" + bits(environment.adjuster.tickSize),
      "minimum=" + bits(environment.adjuster.minContractSize), "last-events=" + pack(environment.updates.slice(-2).map(eventText))].join("\t"));
  }
  const direct = await load();
  await invoke(direct, S(17, Infinity, 0));
  assert.equal(direct.updates[0].type, 17);
  assert.equal(direct.updates[0].value, Infinity);
  rows.push(["Q", "untyped-direct-event", "type=number17", "value=" + bits(Infinity), "tick=" + bits(direct.adjuster.tickSize)].join("\t"));
  const real = await actualDb(), broken = await load(real.database);
  assert.equal(broken.exampleFailures.length, 10);
  assert.ok(broken.exampleFailures.every(message => message.includes("updateAsync is not a function")));
  const tick = await invoke(broken, A(2, 0));
  assert.equal(tick, 1); assert.equal(broken.failures.length, 1); assert.equal(real.updates(), 0);
  let directFailure;
  try { await invoke(broken, S("direct", 2, 0)); } catch (error) { directFailure = error.message; }
  assert.ok(directFailure.includes("updateAsync is not a function"));
  rows.push(["Q", "real-db-async-and-missing-category", "categories=32", "category-absent=true", "awaited-category=undefined",
    "example-rejected-promises=10", "fire-forget-return=" + bits(tick), "counter=1", "datastore-updates=0",
    "actual-error=" + hex(directFailure)].join("\t"));
  const text = "# Complete unchanged adjuster.js dca0888b827c5bc28fa9189bc9e1d507f5c5670c\n# source LF SHA256 " + SHA + "\n# actual complete db.js LF SHA256 " + DB_SHA +
    "\n# V name step method arg-bits-or-typehex:valuebits time actual-return tick-bits minimum-bits threshold-bits multiplier-bits interval counter event-typehex:valuebits:time\n# Q rows are separately observed source quirks, not finite Rust parity inputs.\n" + rows.join("\n") + "\n";
  const target = path.join(fixtures, "adjuster_vectors.tsv");
  if (args.includes("--write")) fs.writeFileSync(target, text);
  else assert.equal(fs.readFileSync(target, "utf8"), text, "adjuster source oracle drift");
  console.log(`adjuster: ${finiteRows} actual finite state/action rows, ${rows.length - finiteRows} source-only observations; all 3 methods + constructor + example`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
