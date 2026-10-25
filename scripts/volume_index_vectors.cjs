#!/usr/bin/env node
"use strict";
// Executes the pinned class with an in-memory DB stub. No daemon/DB/network runs.
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const crypto = require("node:crypto");
const args = process.argv.slice(2);
const sourceFlag = args.indexOf("--source-root");
if (sourceFlag < 0 || !args[sourceFlag + 1]) {
  throw Error("usage: node scripts/volume_index_vectors.cjs --source-root PATH [--write]");
}
const sourceRoot = path.resolve(args[sourceFlag + 1]);
const source = fs.readFileSync(path.join(sourceRoot, "src", "volumeIndex.js"), "utf8").replace(/\r\n/g, "\n");
const digest = crypto.createHash("sha256").update(source).digest("hex");
if (digest !== "d4d4329254125ce8e4217d22b0ba57a1581c6279df999af60281b0365dffb08c") {
  throw Error("pinned volumeIndex.js source hash mismatch");
}
const BigNumber = require(require.resolve("bignumber.js", { paths: [sourceRoot] }));
let docs = [];
const database = {
  findAsync: async () => docs,
  findOneAsync: async () => null,
  updateAsync: async () => { throw Error("unexpected source DB write"); },
};
const context = {
  module: { exports: {} },
  console: { log() {}, warn() {}, error() {} },
  require(name) {
    if (name === "bignumber.js") return BigNumber;
    if (name === "./db.js") return { getDatabase: async () => database };
    if (["node-fetch", "litecoin", "util", "./contractRegistry.js"].includes(name)) return {};
    throw Error(`unexpected dependency: ${name}`);
  },
};
vm.runInNewContext(source, context, { filename: "pinned-volumeIndex.js", timeout: 1000 });
const VolumeIndex = context.module.exports;
const cases = [
  ["empty", false, []],
  ["zero_weight", false, [[0, 900000000]]],
  ["weighted", false, [[200000000, 125000000], [300000000, 250000000]]],
  ["amounts", false, [[200000000, 1000000000], [300000000, 2000000000]]],
  ["contract_swaps", true, [[200000000, 1000000000], [300000000, 2000000000]]],
  ["fractional", false, [[1000000, 123000000], [2000000, 321000000]]],
  ["half_atom_rounds_up", false, [[100000000, 1], [100000000, 2]]],
  ["below_half_atom", false, [[200000000, 1], [100000000, 2]]],
];
async function main() {
  const rows = [];
  for (const [name, contract, entries] of cases) {
    const inputs = entries.map(([a, b]) => ({ amount1: a / 1e8, amount2: b / 1e8 }));
    const raw = VolumeIndex.calculateVWAP(inputs, contract);
    const normalized = entries.map(([a, b]) => contract ? [b, a] : [a, b]);
    docs = normalized.map(([volume, price]) => ({ value: { volume: volume / 1e8, price: price / 1e8 } }));
    const rounded = await VolumeIndex.getVWAP(1, 2, 100, 10);
    const atoms = rounded === null ? "null" : new BigNumber(rounded).times(1e8).toFixed(0);
    rows.push([name, contract ? "1" : "0", entries.map(e => e.join(":")).join(","), String(raw), atoms].join("\t"));
  }
  const vwap = "# pinned=" + digest + "\n# scenario\tcontract\tamount1_atoms:amount2_atoms\tcalculateVWAP_number\tgetVWAP_price_atoms\n" + rows.join("\n") + "\n";
  const fees = [[0,35,100],[1,35,100],[9,35,100],[100000000,35,100],[123456789,2,100],[987654321,99,100],[101,1,1000]];
  const feeRows = fees.map(([fee, n, d]) => {
    const reward = VolumeIndex.calculateAttenuatedLiquidityReward(fee / 1e8, 0, { maxShare: n/d, minShare: n/d, slope: 0 });
    return [fee,n,d,new BigNumber(reward).times(1e8).toFixed(0)].join("\t");
  });
  const feeText = "# Source attenuation's constant-share subset only; no logarithm parity claim.\n# fee_atoms\tshare_numerator\tshare_denominator\treward_atoms\n" + feeRows.join("\n") + "\n";
  for (const [file, expected] of [["volume_index_vwap.tsv", vwap], ["volume_index_fee.tsv", feeText]]) {
    const target = path.join(__dirname, "..", "fixtures", file);
    if (args.includes("--write")) fs.writeFileSync(target, expected);
    else if (fs.readFileSync(target, "utf8") !== expected) throw Error(`source vector drift: ${file}`);
  }
  console.log(`Pinned volume source verified: ${rows.length} VWAP and ${feeRows.length} constant-share fee vectors`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
