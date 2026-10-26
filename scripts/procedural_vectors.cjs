#!/usr/bin/env node
"use strict";

// Expected values come from complete, unmodified archived procedural.js.
// The adapter supplies only detached NeDB-like findOne/$set operations.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SOURCE_COMMIT = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const SOURCE_SHA256 = "0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e";
const args = process.argv.slice(2);
const index = args.indexOf("--source-root");
const sourceRoot = index < 0 ? null : args[index + 1];
if (index >= 0) assert.ok(sourceRoot, "--source-root requires a directory");
const source = readPinnedSource(sourceRoot ? path.join(sourceRoot, "src", "procedural.js") : path.join(fixtures, "legacy_procedural_source.txt"), SOURCE_SHA256);
const clone = value => structuredClone(value);

function newRegistry() {
  const records = new Map();
  const db = { async getDatabase(name) {
    assert.equal(name, "procedural");
    return {
      async findOneAsync(query) { return clone(records.get(query._id) ?? null); },
      async updateAsync(query, update, options) {
        assert.equal(options.upsert, true);
        assert.deepEqual(Object.keys(update), ["$set"]);
        const prev = records.get(query._id) || { _id: query._id };
        const next = { ...clone(prev), ...clone(update.$set) };
        // NeDB model.modify checks `_id` even in modifier-based upserts.
        if (next._id !== query._id) throw new Error("You can't change a document's _id");
        records.set(query._id, next);
        return 1;
      }
    };
  } };
  const module = { exports: {} };
  vm.runInNewContext(source, {
    module, exports: module.exports,
    require(name) { assert.equal(name, "./db"); return db; }
  }, { filename: "pinned-procedural.js", timeout: 1000 });
  return { ...module.exports, records };
}

function encode(value) {
  if (value === null) return "z";
  if (typeof value === "boolean") return value ? "b1" : "b0";
  if (typeof value === "string") return `s${Buffer.from(value, "utf8").toString("hex")};`;
  if (typeof value === "number") { assert.ok(Number.isSafeInteger(value)); return `n${value};`; }
  if (Array.isArray(value)) return `a${value.length}:${value.map(encode).join("")}`;
  assert.equal(typeof value, "object");
  const entries = Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0);
  return `o${entries.length}:${entries.map(([key, item]) => encode(key) + encode(item)).join("")}`;
}

const scenario = (name, operations) => [name, operations];
const scenarios = [
  scenario("gates", [
    ["issuance", "", "", "", ""], ["issuance", "t", "c", "", ""],
    ["issuance", "t", "c", "", "hash"], ["redemption", "", "", ""],
    ["redemption", "t", "c", ""], ["template", "t", {}],
    ["issuance", "t", "c", "", "hash"],
    ["template", "t", { templateHash: " HASH ", note: "retained" }],
    ["issuance", "t", "c", "", "wrong"], ["issuance", "t", "c", "", "hash"],
    ["contractDefault", "c", "other", {}], ["issuance", "t", "c", "", "hash"],
    ["contractDefault", "c", "t", { note: "contract metadata" }],
    ["issuance", "t", "c", "", "hash"], ["issuance", "t", "c", "open", " HASH "],
    ["issuance", "t", "c", "funded", "hash"], ["redemption", "t", "c", ""],
    ["transition", "c", "funded", { state: "DISPUTED", oracleId: 7, blockHeight: 900000 }],
    ["issuance", "t", "c", "FuNdEd", "hash"], ["issuance", "t", "c", " funded ", "hash"],
    ["transition", "c", "settled", { stateHash: "opaque-state-hash" }],
    ["issuance", "t", "c", "", "hash"], ["redemption", "t", "c", "settled"],
    ["redemption", "t", "c", "closed"], ["transition", "c", "closed", {}],
    ["redemption", "t", "c", "CLOSED"], ["getTemplate", "t"], ["getContract", "c"], ["all"]
  ]),
  scenario("all_states", [
    ["template", "t", { hash: "ab" }], ["contractDefault", "c", "t", {}],
    ...["TEMPLATE", "FUNDED", "OPEN", "SETTLED", "CLOSED", "DISPUTED", "unknown", "", " open ", "ß", "σ"].flatMap(state => [
      ["transition", "c", state, {}], ["issuance", "t", "c", "", "AB"], ["redemption", "t", "c", ""]
    ]), ["constants"], ["getContract", "absent"], ["getTemplate", "absent"]
  ]),
  scenario("hash_fallback", [
    ["contractDefault", "c", "t", {}],
    ...[
      [{ templateHash: null, dlcHash: " ABC ", hash: "ignored" }, "abc"],
      [{ templateHash: "", dlcHash: "", hash: "fallback" }, "FALLBACK"],
      [{ templateHash: " ", dlcHash: "valid" }, "valid"],
      [{ templateHash: false, dlcHash: 0, hash: "last" }, "last"],
      [{ templateHash: 123 }, "123"], [{ templateHash: true }, "true"],
      [{ templateHash: [] }, "anything"], [{ templateHash: [" a", null, "B "] }, "a,,b"],
      [{ templateHash: { a: "b" } }, "[OBJECT OBJECT]"],
      [{ templateHash: "\ufeff\u00a0 AΣB \u2028" }, "aσb"],
      [{ templateHash: "Σ" }, "σ"], [{ templateHash: "ΟΣ" }, "ος"],
      [{ templateHash: "\u0085ABC\u0085" }, "abc"]
    ].flatMap(([metadata, hash]) => [
      ["template", "t", metadata], ["issuance", "t", "c", "", hash], ["getTemplate", "t"]
    ]),
    ["template", "whitespace", { templateHash: "aa" }],
    ["contractDefault", "space-contract", "whitespace", {}],
    ["issuance", "whitespace", "space-contract", "", "   "],
    ...[" \t A\r\n", "\ufeffABC\ufeff", "\u0085ABC\u0085", "ΟΣ", "İ", "\u200bABC\u200b"].map(value => ["normalize", value])
  ]),
  scenario("upsert_merge", [
    ["template", "", { templateHash: "empty-id" }], ["getTemplate", ""],
    ["template", "t", { templateHash: "H", old: "persisted", nested: { a: [1, null, true, "x"] } }],
    ["template", "t", { note: "new write" }], ["getTemplate", "t"],
    ["contract", "c", "t", "funded", { old: "retained" }],
    ["contractDefault", "c", "t", {}], ["getContract", "c"],
    ["contract", "c", "t", "", {}], ["issuance", "t", "c", "", "h"],
    ["contractDefault", "c", "t", { templateId: 7, state: ["open"], type: "arbitrary", contractId: "overridden" }],
    ["template", "7", { hash: "h" }], ["issuance", "7", "c", "", "h"],
    ["contractDefault", "c", "t", { templateId: true, state: false }],
    ["redemption", "true", "c", ""],
    ["contractDefault", "c", "t", { templateId: null, state: { raw: "x" } }],
    ["redemption", "null", "c", ""], ["all"]
  ]),
  scenario("orphan_and_overrides", [
    ["transition", "orphan", "settled", { oracleId: 9 }],
    ["redemption", "undefined", "orphan", ""], ["issuance", "undefined", "orphan", "", "h"],
    ["transition", "orphan", "closed", { templateId: "never-published", rolledFrom: "old" }],
    ["redemption", "never-published", "orphan", "closed"],
    ["transition", "next", "funded", { rolledFrom: "orphan", state: "SETTLED", templateId: "never-published" }],
    ["redemption", "never-published", "next", ""],
    ["template", "t", { _id: "wrong" }], ["getTemplate", "t"],
    ["template", "t", { hash: "h" }], ["template", "t", { _id: "wrong", hash: "bad" }],
    ["getTemplate", "t"], ["contractDefault", "c", "t", { _id: "wrong" }],
    ["getContract", "c"], ["transition", "c", "open", { _id: "wrong" }],
    ["getContract", "c"], ["template", "t", { _id: "template-t", type: "different", templateId: "overridden" }],
    ["contractDefault", "c", "t", {}], ["issuance", "t", "c", "", "h"], ["all"]
  ])
];

async function execute(registry, [op, ...a]) {
  const R = registry.ProceduralRegistry;
  switch (op) {
    case "template": return R.upsertTemplate(...a);
    case "contract": return R.upsertContract(...a);
    case "contractDefault": return R.upsertContract(a[0], a[1], undefined, a[2]);
    case "transition": return R.transitionContract(...a);
    case "getTemplate": return R.getTemplate(...a);
    case "getContract": return R.getContract(...a);
    case "issuance": return R.ensureIssuanceContext(...a);
    case "redemption": return R.ensureRedemptionContext(...a);
    case "normalize": return R._normalizeHash(...a);
    case "constants": return registry.PROCEDURAL_STATES;
    case "all": return Object.fromEntries([...registry.records].sort(([a], [b]) => a < b ? -1 : 1));
    default: throw new Error(`unknown operation ${op}`);
  }
}

(async () => {
  const lines = ["# Actual pinned procedural.js metadata results; no proof/issuance authorization implied.", `# Source ${SOURCE_COMMIT} SHA256 ${SOURCE_SHA256}`, "# scenario\tstep\toperation\tencoded-input-array\tencoded-result"];
  let count = 0;
  for (const [name, operations] of scenarios) {
    const registry = newRegistry();
    for (const [i, operation] of operations.entries()) {
      let expected;
      try { expected = await execute(registry, operation); }
      catch (error) {
        assert.match(error.message, /change a document's _id/);
        expected = { error: "IMMUTABLE_ID" };
      }
      lines.push([name, i, operation[0], encode(operation.slice(1)), encode(expected)].join("\t"));
      count++;
    }
  }
  const result = lines.join("\n") + "\n";
  const target = path.join(fixtures, "procedural_vectors.tsv");
  if (args.includes("--write")) fs.writeFileSync(target, result);
  else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), result, "procedural vectors differ");
  console.log(`procedural: ${count} actual source operations across ${scenarios.length} isolated stores passed`);
})().catch(error => { console.error(error); process.exitCode = 1; });
