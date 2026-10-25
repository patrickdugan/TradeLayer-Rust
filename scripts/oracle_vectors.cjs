#!/usr/bin/env node
"use strict";

// Every expected lifecycle result is produced by unmodified pinned oracle.js.
// Database/insurance mocks expose ordering and effects, not replacement logic.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const SOURCE_COMMIT = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
const SOURCE_SHA256 = "70778913a22a323f64a1b2de0b28095a204fba113f39597dedb65938eeb30e3d";
const BRIDGE_SHA256 = "954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f";
const args = process.argv.slice(2);
const sourceIndex = args.indexOf("--source-root");
const sourceRoot = sourceIndex < 0 ? null : args[sourceIndex + 1];
if (sourceIndex >= 0) assert.ok(sourceRoot, "--source-root needs a directory");
const source = readPinnedSource(sourceRoot ? path.join(sourceRoot, "src", "oracle.js") : path.join(fixtures, "legacy_oracle_source.txt"), SOURCE_SHA256);
const bridgeSource = readPinnedSource(sourceRoot ? path.join(sourceRoot, "src", "dlcOracleBridge.js") : path.join(fixtures, "legacy_oracle_bridge_source.txt"), BRIDGE_SHA256);
const BigNumber = loadBigNumber();
const clone = value => structuredClone(value);
const quiet = { log() {}, error() {}, warn() {} };

function matches(row, query) {
  return Object.entries(query).every(([key, wanted]) => {
    if (wanted && typeof wanted === "object") {
      return Object.entries(wanted).every(([op, value]) => op === "$gte" ? row[key] >= value : op === "$lte" ? row[key] <= value : false);
    }
    return row[key] === wanted;
  });
}

function newOracle() {
  const stores = new Map();
  const calls = [];
  const faults = { update: false, insert: false, insurance: false };
  const db = { async getDatabase(name) {
    if (!stores.has(name)) stores.set(name, []);
    const records = stores.get(name);
    return {
      async findAsync(query) { return records.filter(row => matches(row, query)).map(clone); },
      async findOneAsync(query) { const row = records.find(row => matches(row, query)); return row ? clone(row) : null; },
      async insertAsync(doc) {
        if (faults.insert) throw new Error("injected insert failure");
        if (records.some(row => row._id === doc._id)) throw new Error("duplicate ID");
        records.push(clone(doc)); return clone(doc);
      },
      async updateAsync(query, update, options = {}) {
        if (faults.update) throw new Error("injected update failure");
        const index = records.findIndex(row => matches(row, query));
        if (index >= 0) records[index] = update.$set ? { ...records[index], ...clone(update.$set) } : clone(update);
        else if (options.upsert) records.push(update.$set ? { ...query, ...clone(update.$set) } : clone(update));
        return index >= 0 ? 1 : 0;
      }
    };
  } };
  const bridgeModule = { exports: {} };
  vm.runInNewContext(bridgeSource, {
    module: bridgeModule, exports: bridgeModule.exports, Buffer,
    require(name) {
      if (name === "crypto") return crypto;
      if (name === "tiny-secp256k1") return { verify() { throw new Error("signature verification outside oracle.js"); } };
      throw new Error(`unexpected bridge dependency ${name}`);
    }
  }, { filename: "pinned-dlcOracleBridge.js", timeout: 1000 });
  const module = { exports: {} };
  vm.runInNewContext(source, {
    module, exports: module.exports, console: quiet,
    require(name) {
      if (name === "./db") return db;
      if (name === "bignumber.js") return BigNumber;
      if (name === "crypto") return crypto;
      if (name === "./dlcOracleBridge.js") return bridgeModule.exports;
      if (name === "./insurance.js") return { async liquidate(admin, closure) {
        calls.push([admin, closure]);
        if (faults.insurance) throw new Error("injected insurance failure");
      } };
      throw new Error(`unexpected oracle dependency ${name}`);
    }
  }, { filename: "pinned-oracle.js", timeout: 1000 });
  return { Oracle: module.exports, db, stores, calls, faults, bridge: bridgeModule.exports };
}

const scenarios = [
  ["lifecycle", [
    ["create", "", "", "alice", "bob", "", "1", "1,2"],
    ["info", "1"], ["next"], ["admin", "1", "alice"], ["admin", "1", "bob"], ["verify", "1", "bob"], ["admin", "1", "eve"],
    ["update", "1", "carol", "primary"], ["admin", "1", "alice"], ["verify", "1", "carol"],
    ["update", "1", "dave", "backup"], ["admin", "1", "bob"], ["admin", "1", "dave"],
    ["create", "Euro", "EUR", "admin", "", "https://prices", "-0.5", ""], ["info", "2"], ["all"],
    ["close", "1"], ["info", "1"], ["close", "1"], ["close", "99"], ["calls"]
  ]],
  ["publish", [
    ["create", "BTC", "BTC", "alice", "bob", "", "1", ""],
    ["price", "1"], ["twap", "1", "0", "10"],
    ["publish", "1", "10", "100", "120", "80", "95"],
    ["publish", "1", "11", "200", "777", "888", "999"],
    ["publish", "1", "11", "300", "-", "-", "-"],
    ["publish", "1", "9", "90", "-", "-", "-"],
    ["publish", "1", "12", "90", "-", "-", "-"],
    ["price", "1"], ["info", "1"], ["history", "1", "10", "12"],
    ["history", "1", "11", "11"], ["history", "1", "12", "10"],
    ["twap", "1", "12", "2"], ["twap", "1", "11", "0"],
    ["save", "1", "8", "333.123456789", "-", "-", "-"],
    ["history", "1", "0", "100"], ["price", "1"],
    ["save", "1", "12", "22.000000005", "-", "-", "-"],
    ["price", "1"], ["twap", "1", "12", "0"],
    ["reset"], ["publish", "1", "12", "25", "-", "-", "-"],
    ["history", "1", "0", "100"], ["info", "1"], ["all"], ["info", "1"],
    ["publish", "1", "13", "40", "-", "-", "-"], ["update", "1", "carol", "primary"], ["info", "1"],
    ["publish", "1", "14", "40", "-", "-", "-"], ["close", "1"], ["info", "1"],
    ["publish", "1", "15", "40", "-", "-", "-"], ["info", "1"]
  ]],
  ["sentinel", [
    ["create", "ONE", "", "alice", "", "", "1", ""],
    ["publish", "1", "1", "1", "-", "-", "-"],
    ["publish", "1", "2", "900", "-", "-", "-"],
    ["save", "1", "3", "1.00000000000000000001", "-", "-", "-"],
    ["publish", "1", "4", "50", "-", "-", "-"],
    ["save", "1", "5", "0", "-", "-", "-"],
    ["publish", "1", "6", "80", "-", "-", "-"],
    ["save", "1", "7", "-10", "-", "-", "-"],
    ["publish", "1", "8", "-10", "-", "-", "-"],
    ["save", "1", "9", "0.000047619047619047619047619047619", "-", "-", "-"],
    ["publish", "1", "10", "2", "-", "-", "-"],
    ["twap", "1", "10", "100"]
  ]],
  ["stake_fraud", [
    ["create", "stake", "", "alice", "", "", "1", ""],
    ["stakeget", "1", "alice"],
    ["stake", "1", "alice", "2", "1.123456789", "1"],
    ["stake", "1", "alice", "3", "0.000000009", "2"],
    ["stake", "1", "bob", "2", "-1.123456789", "2"],
    ["stakeget", "1", "alice"], ["stakeget", "1", "bob"],
    ["fraud", "1", "absent", "challenger", "10", "proof", "3"],
    ["fraud", "1", "alice", "challenger", "0.2", "proof", "3"],
    ["stakeget", "1", "alice"],
    ["fraud", "1", "alice", "challenger", "-1", "negative", "4"],
    ["stakeget", "1", "alice"],
    ["fraud", "1", "alice", "challenger", "100", "cap", "5"],
    ["fraud", "1", "bob", "challenger", "2", "negative_stake", "5"],
    ["stakeget", "1", "alice"], ["stakeget", "1", "bob"],
    ["stake", "1", "large", "2", "9007199254740993.12345678", "6"],
    ["stake", "1", "large", "2", "1", "7"],
    ["save", "1", "1", "10", "-", "-", "-"],
    ["price", "1"], ["twap", "1", "1", "0"], ["twap", "1", "7", "0"],
    ["save", "1", "8", "20", "-", "-", "-"], ["twap", "1", "8", "0"],
    ["history", "1", "0", "10"]
  ]],
  ["relay", [
    ["create", "relay", "", "alice", "", "", "1", ""],
    ["relay", "1", "alice", "state", "s1", "dlc", "1", " AABB ", "fallback"],
    ["relay", "1", "bob", "state", "s1", "dlc", "2", "aabb", "other"],
    ["relay", "1", "bob", "state", "s2", "dlc", "3", "AABB", ""],
    ["relay", "1", "bob", "state", "s1", "other-dlc", "3", "aabb", ""],
    ["relay", "2", "bob", "state", "s2", "other-dlc", "3", "aabb", ""],
    ["relay", "1", "bob", "state", "", "d2", "3", "ccdd", "from-blob"],
    ["relay", "1", "bob", "state", "from-blob", "d2", "3", "CCDD", ""],
    ["relay", "1", "bob", "state", "new-state", "d2", "3", "", ""],
    ["relay", "1", "bob", "state", "whatever", "", "4", "not-hex", ""],
    ["save", "1", "4", "10", "-", "-", "-"], ["twap", "1", "4", "0"],
    ["history", "1", "0", "10"], ["relayrows"]
  ]],
  ["twap_precision", [
    ["save", "42", "1", "0.000000005", "-", "-", "-"],
    ["twap", "42", "1", "0"],
    ["save", "42", "2", "-0.000000005", "-", "-", "-"],
    ["twap", "42", "2", "0"], ["twap", "42", "2", "1"],
    ["save", "42", "3", "0.000000004999999999999", "-", "-", "-"],
    ["twap", "42", "3", "0"],
    ["save", "42", "4", "0.000000014999999999999", "-", "-", "-"],
    ["twap", "42", "4", "0"], ["history", "42", "1", "4"]
  ]]
];

// Independent exact-decimal baselines around rounded circuit thresholds test
// Number comparison collapse as well as half-up and negative-bound ordering.
const circuitCases = [];
for (let index = 0; index < 64; index++) {
  const coefficient = ((index * 7919 + 104729) % 999983) + 1;
  const baseline = new BigNumber(coefficient).dividedBy("10000").times(index % 7 === 0 ? -1 : 1);
  const boundary = baseline.times(index % 2 ? "1.05" : "0.95").decimalPlaces(4);
  const delta = new BigNumber(index % 3 === 0 ? "-1e-21" : index % 3 === 1 ? "0" : "1e-21");
  circuitCases.push(["save", "1", String(index * 2 + 1), baseline.toFixed(), "-", "-", "-"]);
  circuitCases.push(["publish", "1", String(index * 2 + 2), boundary.plus(delta).toFixed(), "-", "-", "-"]);
}
scenarios.push(["circuit_boundaries", [["create", "bounds", "", "admin", "", "", "1", ""], ...circuitCases]]);

const n = value => value === undefined || value === null ? "-" : new BigNumber(value).toFixed();
const dataText = value => [value.price, value.high, value.low, value.close].map(n).join("/");
const infoText = row => row ? [row.id, row.name, row.ticker, row.adminAddress, row.backupAddress || "-", row.url || "-", row.lag, (row.clearlists || []).join(",") || "-", Boolean(row.closed), row.lastPublishedBlock ?? "-", row.data && row.data.price !== undefined ? dataText(row.data) : "-"].join(";") : "null";
const stakeText = row => row ? `${row.oracleId}/${row.stakerAddress}/${row.stakedPropertyId}/${n(row.amount)}/${row.blockHeight}` : "null";

async function operate(context, command) {
  const [op, ...a] = command;
  const { Oracle, stores, calls } = context;
  const data = offset => Object.fromEntries(["price", "high", "low", "close"].map((key, index) => [key, a[offset + index] === "-" ? undefined : a[offset + index]]));
  if (op === "create") return String(await Oracle.createOracle({ name: a[0], ticker: a[1], adminAddress: a[2], backupAddress: a[3], url: a[4], lag: a[5], clearlists: a[6] ? a[6].split(",").map(Number) : [] }));
  if (op === "next") return String(await Oracle.getNextId());
  if (op === "all") return (await Oracle.getAllOracles()).map(row => row.id).join(",");
  if (op === "info") return infoText(await Oracle.getOracleInfo(Number(a[0])));
  if (op === "admin") return String(await Oracle.isAdmin(a[1], Number(a[0])));
  if (op === "verify") return String(Boolean(await Oracle.verifyAdmin(Number(a[0]), a[1])));
  if (op === "update") { await Oracle.updateAdmin(Number(a[0]), a[1], a[2] === "backup"); return "ok"; }
  if (op === "price") return n(await Oracle.getOraclePrice(Number(a[0])));
  if (op === "twap") { const value = await Oracle.getTWAP(Number(a[0]), Number(a[1]), Number(a[2])); return value === null ? "null" : n(value); }
  if (op === "publish") {
    const count = (stores.get("oracleData") || []).length;
    const previous = Oracle.lastOracleUpdateBlock.get(Number(a[0]));
    const values = data(2);
    await Oracle.publishData(Number(a[0]), values.price, values.high, values.low, values.close, Number(a[1]));
    if (previous !== undefined && previous >= Number(a[1])) {
      assert.equal((stores.get("oracleData") || []).length, count);
      return `skip:${previous}`;
    }
    return dataText(Oracle.getInstance().oracles.get(`oracle-${a[0]}`).data);
  }
  if (op === "save") { await Oracle.getInstance().saveOracleData(Number(a[0]), data(2), Number(a[1])); return "ok"; }
  if (op === "history") return (await Oracle.getInstance().loadOracleData(Number(a[0]), Number(a[1]), Number(a[2]))).map(row => `${row.blockHeight}:${dataText(row.data)}`).join(",") || "-";
  if (op === "reset") { Oracle.lastOracleUpdateBlock.clear(); return "ok"; }
  if (op === "close") { try { await Oracle.closeOracle(Number(a[0])); return "ok"; } catch (error) { return `error:${error.message}`; } }
  if (op === "calls") return calls.map(row => `${row[0]}:${row[1]}`).join(",") || "-";
  if (op === "stake") return stakeText(await Oracle.recordStake(Number(a[0]), a[1], Number(a[2]), a[3], Number(a[4])));
  if (op === "stakeget") return stakeText(await Oracle.getStake(Number(a[0]), a[1]));
  if (op === "fraud") return n((await Oracle.applyFraudProof(Number(a[0]), a[1], a[2], a[3], a[4], Number(a[5]))).slashed);
  if (op === "relay") {
    const blob = JSON.stringify({ signatureHex: a[6], stateHash: a[7] });
    try {
      const row = await Oracle.relayTradeLayerState(Number(a[0]), a[1], a[2], a[3], a[4], Number(a[5]), blob);
      return `${row.oracleId}/${row.senderAddress}/${row.relayType}/${row.stateHash || "-"}/${row.dlcRef || "-"}/${row.blockHeight}`;
    } catch (error) { return error.message.startsWith("Relay signature replay") ? "error:replay" : `error:${error.message}`; }
  }
  if (op === "relayrows") return (stores.get("oracleData") || []).filter(row => row.type === "relaySigUse").map(row => `${row.signatureHex}/${row.stateHash || "-"}/${row.dlcRef || "-"}/${row.firstSeenBlock}`).join(",");
  throw new Error(`unknown op ${op}`);
}

async function sourceQuirks() {
  const result = [];
  const c = newOracle();
  await c.Oracle.createOracle({ name: "fault", adminAddress: "admin" });
  c.faults.update = true;
  await assert.rejects(c.Oracle.publishData(1, "10", undefined, undefined, undefined, 5), /injected/);
  result.push(`failed-publish-watermark\t${c.Oracle.lastOracleUpdateBlock.get(1)}`);
  result.push(`failed-publish-memory-price\t${n(c.Oracle.getInstance().oracles.get("oracle-1").data.price)}`);
  c.faults.update = false;
  await c.Oracle.publishData(1, "20", undefined, undefined, undefined, 5);
  result.push(`retry-same-block-record-count\t${(c.stores.get("oracleData") || []).length}`);
  c.faults.insurance = true;
  await assert.rejects(c.Oracle.closeOracle(1), /injected insurance/);
  result.push(`failed-insurance-still-closed\t${c.Oracle.getInstance().oracles.get("oracle-1").closed}`);
  const d = newOracle();
  await d.Oracle.publishData(42, "3", undefined, undefined, undefined, 1);
  result.push(`unknown-publish-creates-memory\t${d.Oracle.getInstance().oracles.get("oracle-42").id}`);
  result.push(`unknown-publish-db-metadata-count\t${(d.stores.get("oracleList") || []).length}`);
  await d.Oracle.createOracle({ name: "missingadmin" });
  result.push(`undefined-sender-admin-on-missing\t${await d.Oracle.isAdmin(undefined, 999)}`);
  const nested = { _id: "oracle-7", id: 7, name: { adminAddress: "nested", backupAddress: "backup" } };
  await d.Oracle.getInstance().addOracle("oracle-7", nested);
  result.push(`nested-is-admin\t${await d.Oracle.isAdmin("nested", 7)}`);
  result.push(`nested-verify-admin\t${Boolean(await d.Oracle.verifyAdmin(7, "nested"))}`);
  await assert.rejects(d.Oracle.getInstance().addOracle("oracle-7", { ...nested, name: "overwritten-memory" }), /duplicate/);
  result.push(`duplicate-add-memory-name\t${d.Oracle.getInstance().oracles.get("oracle-7").name}`);
  result.push(`duplicate-add-db-name-object\t${typeof (d.stores.get("oracleList") || []).find(row => row._id === "oracle-7").name}`);
  const bridge = newOracle().bridge;
  const alias = bridge.parseRelayBlob(`b64:${Buffer.from(JSON.stringify({ signatures: ["ABCD"], state_hash: "alias" })).toString("base64")}`);
  result.push(`bridge-base64-alias\t${alias.signatureHex}/${alias.stateHash}`);
  result.push(`bridge-malformed\t${bridge.parseRelayBlob("bad-json")}`);
  const e = newOracle();
  await e.Oracle.getInstance().saveOracleData(1, { price: "not-a-number" }, 1);
  result.push(`invalid-price-query-default\t${await e.Oracle.getOraclePrice(1)}`);
  result.push(`invalid-price-twap-is-nan\t${Number.isNaN(await e.Oracle.getTWAP(1, 1, 0))}`);
  await e.Oracle.recordStake(1, "accused", 2, "5", 2);
  const prefix = "a".repeat(24);
  await e.Oracle.applyFraudProof(1, "accused", "challenger", "1", `${prefix}x`, 3);
  await e.Oracle.applyFraudProof(1, "accused", "challenger", "1", `${prefix}y`, 3);
  const fraudRows = (e.stores.get("oracleData") || []).filter(row => row.type === "fraudProof");
  result.push(`fraud-prefix-collision-count\t${fraudRows.length}`);
  result.push(`fraud-prefix-collision-last-evidence\t${fraudRows[0].evidenceHash}`);
  return result;
}

(async () => {
  const rows = [`# oracle.js ${SOURCE_COMMIT} SHA256 ${SOURCE_SHA256}`, "# scenario\top\tpipe-separated args\tactual source result"];
  let count = 0;
  for (const [name, commands] of scenarios) {
    const context = newOracle();
    for (const command of commands) {
      const value = await operate(context, command);
      rows.push([name, command[0], command.slice(1).join("|"), value].join("\t"));
      count++;
    }
  }
  const artifacts = [["oracle_vectors.tsv", rows.join("\n") + "\n"], ["oracle_quirks.tsv", `# Actual source failure/unsafe-domain characterizations; not Rust success claims.\n${(await sourceQuirks()).join("\n")}\n`]];
  for (const [file, text] of artifacts) {
    if (args.includes("--write")) fs.writeFileSync(path.join(fixtures, file), text);
    else assert.equal(fs.readFileSync(path.join(fixtures, file), "utf8").replace(/\r\n/g, "\n"), text, file);
  }
  console.log(`oracle.js: ${count} lifecycle vectors and ${artifacts[1][1].split("\n").length - 2} source quirks passed`);
})().catch(error => { console.error(error); process.exitCode = 1; });
