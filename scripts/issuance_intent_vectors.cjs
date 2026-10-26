#!/usr/bin/env node
"use strict";
// Full source modules, unchanged. The repaired metadata adapter below is NOT
// NeDB persistence; the real db.js category failure is executed separately.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const util = require("node:util");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const HASH = "dafabd4d8a51e805cac299b33e815a2367ffb1b934f943c4519454a671aec932";
const DB_HASH = "b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38";
const source = readPinnedSource(path.join(fixtures, "legacy_issuance_intent_source.txt"), HASH);
const dbSource = readPinnedSource(path.join(fixtures, "legacy_db_source.txt"), DB_HASH);
const rows = [];
const out = (...fields) => rows.push(fields.join("\t"));
const json = value => JSON.stringify(value);
const clone = value => JSON.parse(json(value));
function load(database, time) {
  const box = { exports: {} };
  vm.runInNewContext(source, { module: box, exports: box.exports,
    Date: { now: () => time.value }, require: name => { assert.equal(name, "./db.js"); return database; } },
    { filename: "pinned-issuanceIntent.js", timeout: 1000 });
  return box.exports;
}
async function missingCategory() {
  const categories = [];
  class DatastoreDependency {
    constructor(options) { categories.push(path.basename(options.filename, ".db")); }
  }
  for (const name of ["find", "insert", "remove", "update", "findOne", "count"]) {
    DatastoreDependency.prototype[name] = function (...args) { args.at(-1)(null, null); };
  }
  const box = { exports: {} };
  vm.runInNewContext(dbSource, { module: box, exports: box.exports, __dirname: "/pinned/src",
    process: { env: { TL_SKIP_RPC_BOOT: "1", TL_FORCE_TEST: "0", CHAIN: "LTC" } },
    console: { log() {}, error() {} }, require: name => {
      if (name === "nedb") return DatastoreDependency;
      if (name === "path" || name === "util") return require(name);
      if (name === "./client") return { getInstance() { throw Error("RPC must remain skipped"); } };
      throw Error(`unexpected db.js dependency ${name}`);
    } }, { filename: "pinned-db.js", timeout: 1000 });
  const database = box.exports;
  await database.init();
  assert.equal(categories.length, 32);
  assert(!categories.includes("issuanceIntent"));
  assert.equal(await database.getDatabase("issuanceIntent"), undefined);
  out("OBS", "real-db-router", categories.length, "issuanceIntent=undefined");
  const intent = load(database, { value: 123 });
  for (const [name, method, expected] of [
    ["record", () => intent.recordIntent("tx", {}), "updateAsync"],
    ["get", () => intent.getIntent("tx"), "findOneAsync"],
    ["address", () => intent.getIntentsByAddress("r"), "findAsync"],
  ]) {
    try { await method(); assert.fail("missing category should fail"); }
    catch (error) { assert.equal(error.name, "TypeError"); assert(error.message.includes(expected)); out("OBS", `real-db-${name}`, error.name, expected); }
  }
  await assert.rejects(intent.recordIntent("", {}), /txid is required/);
  out("OBS", "missing-selector-first", "Error", "txid is required");
}
function repairedAdapter() {
  const data = [], calls = [];
  const addressMatches = (value, wanted) => value === wanted || Array.isArray(value) && value.some(item => addressMatches(item, wanted));
  return { data, calls, async getDatabase(name) {
    assert.equal(name, "issuanceIntent"); calls.push("open");
    return {
      async updateAsync(query, document, options) {
        calls.push("update"); assert.equal(options.upsert, true);
        const at = data.findIndex(value => value._id === query._id);
        if (at >= 0 && document._id !== data[at]._id) throw Error("ImmutableId");
        if (at < 0 && data.some(value => json(value._id) === json(document._id))) throw Error("DuplicateId");
        if (at < 0) data.push(clone(document)); else data[at] = clone(document);
      },
      async findOneAsync(query) { calls.push("one"); return clone(data.find(value => value._id === query._id) || null); },
      async findAsync(query) { calls.push("address"); return data.filter(value => addressMatches(value.recipientAddress, query.recipientAddress)).map(clone); },
    };
  } };
}
async function workingAdapter() {
  const time = { value: 0 }; let database, api;
  const reset = name => { database = repairedAdapter(); api = load(database, time); out("RESET", name); };
  const write = async (label, selector, millis, text, expected = "ok") => {
    time.value = millis; let result = null, code = "ok";
    try { result = await api.recordIntent(selector, JSON.parse(text)); }
    catch (error) { code = error.message === "txid is required" ? "MissingSelector" : error.message; }
    assert.equal(code, expected, label);
    out("WRITE", label, json(selector), millis, text, code, json(result));
  };
  const get = async (label, selector) => out("GET", label, json(selector), json(await api.getIntent(selector)));
  const address = async (label, value) => out("ADDRESS", label, json(value), json(await api.getIntentsByAddress(value)));
  const all = label => out("DOCUMENTS", label, json(database.data));
  reset("arbitrary-fields-and-index");
  await write("full-object", "z", 100, '{"recipientAddress":"r","custom":{"nested":[null,true,false,1.25,{"memo":"雪😀"}]},"issuer":"i","quantity":12,"8":"eight","2":"two"}');
  await write("second-row", "a", 101, '{"recipientAddress":"r","anything":{"different":true}}');
  await address("insertion-query-order", "r");
  await write("replace-not-merge", "z", 102, '{"recipientAddress":"s","newField":"only"}');
  await get("old-fields-gone", "z"); await address("old-index-removed", "r"); await address("new-index", "s");
  await write("array-address", "m", 103, '{"recipientAddress":["r","s","r"],"updatedAt":{"discarded":[1,2]},"nullable":null}');
  await address("array-membership-deduplicated", "r"); await address("stable-update-slot", "s");
  await write("nested-array-address", "nested", 104, '{"recipientAddress":["s",[["r","r"],["r2",{"ignored":"r3"}]]],"unrelated":{"recipientAddress":"r3"}}');
  await address("nested-array-recursion", "r"); await address("deep-nested-array", "r2"); await address("object-values-ignored", "r3");
  await write("nested-replaced", "nested", 105, '{"recipientAddress":[["other"]]}');
  await address("nested-old-membership-removed", "r2"); await address("nested-replacement-active", "other"); all("all-rows");
  reset("spread-identity");
  await write("override-id", "original", 200, '{"updatedAt":-1,"_id":"alias","recipientAddress":"r","tail":1}');
  await get("selector-is-not-stored-id", "original"); await get("overridden-id", "alias");
  await write("same-selector-collides", "original", 201, '{"_id":"alias","recipientAddress":"changed"}', "DuplicateId");
  await write("existing-id-immutable", "alias", 202, '{"_id":"other"}', "ImmutableId");
  await write("alias-normal-update", "alias", 203, '{"recipientAddress":"s"}');
  await address("failed-updates-left-no-index", "r"); await get("alias-updated", "alias");
  await write("empty-stored-id", "nonempty", 204, '{"_id":"","recipientAddress":"r"}'); await get("empty-query-allowed", "");
  await write("empty-selector", "", 205, '{}', "MissingSelector"); all("identity-rows");
  reset("generic-json-and-enumeration");
  for (const [label, text] of [
    ["null-id", '{"_id":null,"recipientAddress":"r"}'],
    ["number-id", '{"_id":7,"recipientAddress":"r"}'],
    ["object-id", '{"_id":{"part":"x"},"recipientAddress":"r"}'],
    ["array-id-repair", '{"_id":[],"recipientAddress":"r"}'],
    ["all-keys-metadata", '{"__proto__":{"polluted":true},"constructor":"data","$field":1,"a.b":2,"recipientAddress":"r"}'],
    ["key-order", '{"updatedAt":999,"4294967295":"not-index","10":"ten","01":"not-index","0":"zero","2":"two"}'],
    ["number-boundaries", '{"n":[-0,1e-7,1e-6,1e20,1e21,1e23,9007199254740993,5e-324,1.7976931348623157e308]}'],
  ]) await write(label, label, 300, text);
  await address("all-generic-documents", "r"); all("generic-row-order");
  for (const [label, input] of [["null-spread", null], ["array-spread", ["a", "b"]], ["string-spread", "ab"], ["number-spread", 7]]) {
    time.value = 400; const result = await api.recordIntent(label, input);
    out("OBS", label, json(input), json(result));
  }
}
function numberCases() {
  const values = ["-0", "0", "1.25", "1e-7", "1e-6", "1e20", "1e21", "1e23", "9007199254740993", "1e-999", "5e-324", "2.2250738585072014e-308", "1.7976931348623157e308", "1000000000000000100"];
  let bits = 0x91e10da5c79e7b1dn;
  for (let count = 0; count < 256; count++) {
    bits = BigInt.asUintN(64, bits * 6364136223846793005n + 1442695040888963407n);
    const buffer = Buffer.alloc(8); buffer.writeBigUInt64BE(bits);
    const value = buffer.readDoubleBE(); if (Number.isFinite(value)) values.push(value.toString());
  }
  for (const text of values) {
    const value = JSON.parse(text); assert(Number.isFinite(value));
    out("NUMBER", text, json(value));
  }
}
async function genuineNedb(root) {
  // Older NeDB uses Node utility predicates removed by modern Node. These
  // compatibility adapters do not replace NeDB's query/index/update code.
  util.isDate ||= value => Object.prototype.toString.call(value) === "[object Date]";
  util.isRegExp ||= value => Object.prototype.toString.call(value) === "[object RegExp]";
  const Datastore = require(path.join(path.resolve(root), "node_modules", "nedb"));
  const make = () => { const base = new Datastore({ inMemoryOnly: true });
    for (const name of ["update", "findOne", "find"]) base[`${name}Async`] = util.promisify(base[name].bind(base));
    return { base, api: load({ getDatabase: async () => base }, { value: 500 }) };
  };
  const { base, api } = make();
  await api.recordIntent("original", { _id: "alias", recipientAddress: "r" });
  assert.equal(await api.getIntent("original"), null); assert.equal((await api.getIntent("alias"))._id, "alias");
  await assert.rejects(api.recordIntent("original", { _id: "alias" }), /unique constraint/);
  await assert.rejects(api.recordIntent("alias", { _id: "other" }), /cannot change.*_id/);
  await api.recordIntent("z", { recipientAddress: "sorted" }); await api.recordIntent("a", { recipientAddress: "sorted" });
  assert.deepEqual((await api.getIntentsByAddress("sorted")).map(doc => doc._id), ["a", "z"]);
  await assert.rejects(api.recordIntent("field", { "a.b": 1 }), /Field names/);
  const empty = make(); await empty.api.recordIntent("array", { _id: [] });
  assert.deepEqual(await empty.base.findAsync({}), []);
  const nested = make();
  await nested.api.recordIntent("nested", { recipientAddress: ["s", [["r", "r"], ["r2", { ignored: "r3" }]]], unrelated: { recipientAddress: "r3" } });
  assert.deepEqual((await nested.api.getIntentsByAddress("r")).map(doc => doc._id), ["nested"]);
  assert.deepEqual((await nested.api.getIntentsByAddress("r2")).map(doc => doc._id), ["nested"]);
  assert.deepEqual(await nested.api.getIntentsByAddress("r3"), []);
  await nested.api.recordIntent("nested", { recipientAddress: [["other"]] });
  assert.deepEqual(await nested.api.getIntentsByAddress("r2"), []);
  assert.deepEqual((await nested.api.getIntentsByAddress("other")).map(doc => doc._id), ["nested"]);
  assert.equal((await base.findAsync({})).length, 3);
  console.log("genuine NeDB in-memory checks: ID mismatch/duplicates/immutability, nested-array address matching/replacement/object exclusion, index ordering, key rejection, empty-array ID anomaly; explicit Node compatibility predicates");
}
async function main() {
  await missingCategory(); await workingAdapter(); numberCases();
  const text = "# Full unchanged issuanceIntent.js+db.js; original missing-category failure and explicitly repaired metadata adapter.\n" + rows.join("\n") + "\n";
  const filename = path.join(fixtures, "issuance_intent_vectors.tsv");
  if (process.argv.includes("--check")) assert.equal(fs.readFileSync(filename, "utf8").replace(/\r\n/g, "\n"), text);
  else fs.writeFileSync(filename, text);
  const flag = process.argv.indexOf("--nedb-root"); if (flag >= 0) { assert(process.argv[flag + 1]); await genuineNedb(process.argv[flag + 1]); }
  console.log(`issuance intent: ${rows.length} source observations; source SHA ${HASH}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
