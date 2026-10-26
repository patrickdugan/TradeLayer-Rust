#!/usr/bin/env node
"use strict";

// Complete unchanged source, genuine BigNumber 9.3.1 and the actual archived
// Base94 + bigint-base-converter. No arithmetic/encoder stub or source rewrite.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { readPinnedSource, loadBigNumber } = require("./legacy_numeric_oracle.cjs");
const root = path.resolve(__dirname, "..");
const args = process.argv.slice(2);
const write = args.includes("--write");
assert(args.every((arg, index) => ["--write", "--check", "--source"].includes(arg) || args[index - 1] === "--source"), "unknown argument");
assert(!(write && args.includes("--check")), "choose --write or --check");
function archive(file, gitBlob) {
  const bytes = Buffer.from(fs.readFileSync(path.join(root, "fixtures", file), "utf8").replace(/\r\n/g, "\n"));
  assert.equal(crypto.createHash("sha1").update(Buffer.from(`blob ${bytes.length}\0`)).update(bytes).digest("hex"), gitBlob);
  return bytes.toString("utf8");
}
function load(code, imports) {
  const module = { exports: {} };
  vm.runInNewContext(code, {
    module, exports: module.exports, console: { log() {}, warn() {}, error() {} },
    require(name) {
      assert(Object.hasOwn(imports, name), `unexpected source import ${name}`);
      return imports[name];
    },
  }, { timeout: 2000 });
  return module.exports;
}
const converter = load(archive("legacy_base_converter_source.txt", "c38f00093bf022f4d2ef136a3e34480d409c60fd"), {});
const base94 = load(archive("legacy_base94_source.txt", "8d9f3d14d0ca29488c99e460c52020e8dd66b334"), { "bigint-base-converter": converter });
const base256 = load(archive("legacy_base256_source.txt", "90305dec38f6be043f039d0e75b296344ac57ba0"), { "bigint-base-converter": converter });
const sourcePath = args.includes("--source") ? args[args.indexOf("--source") + 1] : path.join(root, "fixtures", "legacy_types_source_txEncoder.txt");
const code = readPinnedSource(sourcePath, "c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42");
const encoder = load(code, { "bignumber.js": loadBigNumber(), "./base94.js": base94, "./base256.js": base256 });
const methods = Object.keys(encoder);
assert.equal(methods.length, 39);
const hex = text => Buffer.from(text, "utf8").toString("hex") || "-";
const json = input => hex(JSON.stringify(input));
const vectorRows = [];
const quirkRows = [];
const methodCounts = new Map(methods.map(method => [method, 0]));
function run(method, input) {
  try {
    const value = encoder[method](input);
    assert(value === undefined || typeof value === "string");
    return [value === undefined ? "undefined" : "string", value === undefined ? "-" : hex(value)];
  } catch (error) { return ["throw", hex(`${error.name}:${error.message}`)]; }
}
function add(name, method, input) {
  const result = run(method, input);
  assert.notEqual(result[0], "throw", `${name}: ${Buffer.from(result[1], "hex")}`);
  vectorRows.push(["V", name, method, json(input), ...result].join("\t"));
  methodCounts.set(method, methodCounts.get(method) + 1);
}
function quirk(name, method, input, note) {
  quirkRows.push(["Q", name, method, json(input), ...run(method, input), note].join("\t"));
}
const base = {
  encodeAmount: 0.5,
  encodeActivateTradeLayer: { txTypeToActivate: [0, 18, 35], codeHash: "f".repeat(64) },
  encodeTokenIssue: { initialAmount: 36, ticker: "TK", whitelists: [7, 36], managed: true, backupAddress: "backup", nft: false, coloredCoinHybrid: true, proceduralType: 2 },
  encodeSend: { sendAll: false, address: "alice", propertyId: 7, amount: 0.5 },
  encodePropertyId: "s-36-71",
  encodeTradeTokenForUTXO: { propertyId: 7, amount: 0.5, columnA: true, satsExpected: 0.00000001, tokenOutput: 2, payToAddress: "bob", isColoredOutput: true },
  encodeCommit: { propertyId: 7, amount: 0.5, channelAddress: "channel", payEnabled: true, clearLists: [7, 36], isColoredOutput: false, commitClearlistId: 11 },
  encodeOnChainTokenForToken: { propertyIdOffered: 7, propertyIdDesired: 11, amountOffered: 0.5, amountExpected: 2, stop: true, post: false },
  encodeCancelOrder: { isContract: true, contractId: 11, cancelAll: false, cancelParams: { price: 36, side: 1, txid: "tx" } },
  encodeCreateWhitelist: { backupAddress: "backup", name: "list", url: "site", description: "note" },
  encodeUpdateAdmin: { newAddress: "admin", whitelist: true, oracle: false, token: true, id: 36, updateBackup: false },
  encodeIssueOrRevokeAttestation: { revoke: true, id: 36, targetAddress: "alice", metaData: "ok" },
  encodeAMMPool: { isRedeem: "1", isContract: true, id: 36, amount: "1Z", id2: "72", amount2: 12.9 },
  encodeCreateOracle: { ticker: "BTC", url: "site", backupAddress: "backup", whitelists: [7, 11], lag: 36, enum: "USD" },
  encodeGrantManagedToken: { propertyId: 36, amountGranted: 0.5, addressToGrantTo: "alice", dlcTemplateId: "template", dlcContractId: "dlc", settlementState: "state", dlcHash: "hash" },
  encodeRedeemManagedToken: { propertyId: 36, amountDestroyed: 0.5, dlcTemplateId: "template", dlcContractId: "dlc", settlementState: "state" },
  encodePublishOracleData: { oracleid: 36, price: 1.2345, high: 72, low: 36, close: 35 },
  encodeCloseOracle: 36,
  encodeCreateFutureContractSeries: { native: true, underlyingOracleId: 36, onChainData: [[[7, 11]], [[36, 72]]], notionalPropertyId: 7, notionalValue: 36, collateralPropertyId: 11, leverage: 10, expiryPeriod: 72, series: 1, inverse: false, fee: true },
  encodeExerciseDerivative: { derivativeContractId: 36, amount: 72 },
  encodeTradeContractOnchain: { contractId: 36, price: 1.234567895, amount: 3, sell: true, insurance: false, reduce: true, post: false, stop: true },
  encodeTradeContractChannel: { contractId: 36, price: 1.234567895, amount: 3, columnAIsSeller: true, expiryBlock: 72, insurance: false, columnAIsMaker: true },
  encodeTradeTokensChannel: { propertyId1: 7, propertyId2: 11, amountOffered1: 0.5, amountDesired2: 2, columnAIsOfferer: true, expiryBlock: 72, columnAIsMaker: true, Id1ColoredOutput: false, Id2ColoredOutput: true },
  encodeWithdrawal: { withdrawAll: true, propertyId: 7, amountOffered: 0.5, column: 1, channelAddress: "channel" },
  encodeTransfer: { propertyId: 7, amount: 0.5, isColumnA: true, destinationAddr: "channel", pay: true, payRef: "ref" },
  encodeSettleChannelPNL: { txidNeutralized1: "trade", txidNeutralized2: "settle", markPrice: "1.001", settleType: "2", columnAIsSeller: true, columnAIsMaker: false, netAmount: 91, expiryBlock: 94, blockStart: 1, blockEnd: 2, propertyId: 7, aPaysBDirection: true, channelRoot: "root", totalContracts: 10, neutralizedCount: 2 },
  encodeKingSettle: { blockStart: 1, blockEnd: 94, propertyId: 7, netAmount: "1.001", aPaysBDirection: true, channelRoot: "root", totalContracts: 91, neutralizedCount: 2 },
  encodeMintSynthetic: { propertyIdUsed: 7, contractIdUsed: 36, amount: 72 },
  encodeRedeemSynthetic: { propertyIdUsed: 7, contractIdUsed: 36, amount: 72 },
  encodePayToTokens: { propertyIdTarget: 7, propertyIdUsed: 36, amount: 72 },
  encodeOptionTrade: { contractId: "1-100-C", price: 0.5, amount: 3, columnAIsSeller: true, expiryBlock: 72, columnAIsMaker: false, comboTicker: "1-110-C", comboPrice: 2, comboAmount: 1 },
  encodeTradeBaiUrbun: { propertyIdDownPayment: 7, propertyIdToBeSold: 11, price: 36, amount: 72, expiryBlock: 100, tradeExpiryBlock: 200 },
  encodeTradeMurabaha: { propertyIdDownPayment: 7, downPaymentPercent: 10, propertyIdToBeSold: 11, price: 36, amount: 72, expiryBlock: 100, installmentInterval: 12, tradeExpiryBlock: 200 },
  encodeStakeFraudProof: { action: "2", oracleId: "36", stakedPropertyId: 7, amount: 0.000000019, accusedAddress: "alice", evidenceHash: "evidence", relayType: 1, stateHash: "state", dlcRef: "dlc", settlementState: "settle", relayBlob: "blob", autoRoll: true, nextDlcRef: "next" },
  encodeBatchSettlement: { ignored: true },
  encodeBatchMoveZkRollup: { proof: "proof", payments: [{ fromAddress: "alice", propertyIds: [7, 11], amounts: [36, 72], toAddress: "bob", sentPropertyIds: [11], sentAmounts: [35] }], miscLogic: { "2": "two", "1": "one", condition: true }, miscData: [1, null, "text"] },
  encodeColoredCoin: { encodeDecodeRecode: 2, propertyId: 7, SatsRatio: 36, homeAddress: "home" },
  encodeAbstractionBridge: { propertyId: 7, amount: 36, destinationAddr: "destination" },
  encodeBindSmartContract: { contractHash: "hash", type: 2, hostChain: "BTC" },
};
assert.deepEqual(Object.keys(base).sort(), methods.slice().sort(), "all own source exports inventoried");
for (const method of methods) add(`base-${method}`, method, base[method]);

// Every Number integer representation; strings deliberately remain unchanged.
for (const value of [null, false, true, 0, -1, 1, 35, 36, 71, 72, 1295, 1296, 9007199254740991, -9007199254740991, "36", "001", "aB", "s-36-71-extra", "s-bad-2", "s-0x24-71", "s-3.9-+71tail", "s-1", [], [36, null, "A"], { id: 1 }]) {
  add(`property-${json(value)}`, "encodePropertyId", value);
}
for (const value of [null, false, true, 0, -1, 1, "36", "-2", "1.0", "0.5", 0.5, -0.5, 0.000000005, -0.000000005, "0.0000000049", "-0.0000000049", "9.007199254740991e7", 90071992.54740991, "90071992.54740991"]) {
  add(`amount-${json(value)}`, "encodeAmount", value);
}
const patch = (method, suffix, changes) => add(`${method}-${suffix}`, method, { ...base[method], ...changes });
patch("encodeSend", "all-missing-address", { sendAll: "0", address: undefined });
patch("encodeSend", "all-null-address", { sendAll: true, address: null });
patch("encodeSend", "multi-unequal-arrays", { propertyId: [7, 11, 36], amount: [1, 0.5] });
patch("encodeSend", "scalar-array-amount", { propertyId: 7, amount: [0.5, 1] });
patch("encodeSend", "array-property-scalar-amount", { propertyId: [7, 11], amount: 1 });
patch("encodeTokenIssue", "nullable-fields", { initialAmount: null, ticker: null, whitelists: [null, "36", true], managed: "0", backupAddress: undefined, proceduralType: "36" });
patch("encodeCreateOracle", "nullable-fields", { whitelists: [null, "36"], ticker: null, lag: null, enum: false });
for (const clearLists of [undefined, null, false, 0, "0", [], [null, "36"], 36]) {
  patch("encodeCommit", `lists-${json(clearLists === undefined ? "missing" : clearLists)}`, { clearLists, commitClearlistId: "36" });
}
for (const reference of [undefined, 0, -1, true, "false", "0", [1, 2], { ref: 1 }]) {
  patch("encodeCommit", `reference-${json(reference === undefined ? "missing" : reference)}`, { channelAddress: "C".repeat(43), ref: reference });
  patch("encodeTransfer", `reference-${json(reference === undefined ? "missing" : reference)}`, { destinationAddr: "C".repeat(43), ref: reference });
}
for (const isContract of [undefined, null, false, true, 0, 1, 2, "0", "1", [], {}]) {
  patch("encodeCancelOrder", `kind-${json(isContract === undefined ? "missing" : isContract)}`, { isContract, offeredPropertyId: 7, desiredPropertyId: 11, cancelParams: { price: 0.5, side: "sell", txid: "tx" } });
}
// The contract branch above asks for fractional Number.toString(36), which is
// outside the Rust safe-integer domain. Move those source observations to Q.
for (let index = vectorRows.length - 1; index >= 0; index--) {
  const cells = vectorRows[index].split("\t");
  if (cells[1].startsWith("encodeCancelOrder-kind-")) {
    const input = JSON.parse(Buffer.from(cells[3], "hex"));
    if (Number(input.isContract) === 1) {
      vectorRows.splice(index, 1); methodCounts.set(cells[2], methodCounts.get(cells[2]) - 1);
      quirk(cells[1], cells[2], input, "fractional-Number-radix36-outside-typed-domain");
    }
  }
}
for (const value of [undefined, null, false, true, 0, 1, "0", "1", "true", [], {}, "ABC", "-ABC", "12.9", "1e2", "-2.9", "0x24", "Infinity", "!bad"]) {
  patch("encodeAMMPool", `coerce-${json(value === undefined ? "missing" : value)}`, { isRedeem: value, isContract: value, id: value, amount: value, id2: value, amount2: value });
}
patch("encodeGrantManagedToken", "fallbacks", { propertyid: 0, propertyId: 36, dlcTemplateId: 0, dlcContractId: false, settlementState: [], dlcHash: {} });
patch("encodeRedeemManagedToken", "fallback-amount", { propertyid: null, propertyId: "36", amountDestroyed: null, amountGranted: 0.5 });
patch("encodeRedeemManagedToken", "missing-all", { propertyId: undefined, amountDestroyed: undefined });
patch("encodePublishOracleData", "close-only", { high: undefined, low: undefined, close: null });
patch("encodePublishOracleData", "string-fields", { oracleid: "36", high: "1.25", low: null, close: false });
patch("encodeCreateFutureContractSeries", "null-defaults", { onChainData: [], notionalValue: "1.25", leverage: null, expiryPeriod: null, fee: "0" });
patch("encodeCreateFutureContractSeries", "source-extra-nesting", { onChainData: [[[7, 11], [999, 998]], [[36]]], leverage: "10", expiryPeriod: undefined });
patch("encodeOptionTrade", "combo-zero-skipped", { comboAmount: 0, comboPrice: undefined });
patch("encodeOptionTrade", "combo-string-zero-truthy", { comboAmount: "0", comboPrice: "0.5" });
patch("encodeSettleChannelPNL", "legacy-fallbacks", { txidNeutralized1: "", txidNeutralized2: 0, tradeid: "old-trade", settleid: "old-settle", settleType: undefined, close: "0", blockStart: undefined, blockEnd: undefined, propertyId: undefined });
for (const settleType of [null, false, true, "", "garbage", [], {}, "Infinity", "-Infinity", 1.5]) {
  patch("encodeSettleChannelPNL", `settle-type-${json(settleType)}`, { settleType });
}
patch("encodeSettleChannelPNL", "null-extension-included", { propertyId: null, blockStart: undefined, blockEnd: undefined });
patch("encodeBatchMoveZkRollup", "empty-missing-misc", { proof: null, payments: [], miscLogic: undefined, miscData: undefined });
patch("encodeBatchMoveZkRollup", "json-order-and-null", { miscLogic: { z: 1, 1: "one", "01": 2, a: [null, true, "|,;"] }, miscData: null });
patch("encodeBindSmartContract", "raw-coercions", { contractHash: [], type: { a: 1 }, hostChain: null });
patch("encodeIssueOrRevokeAttestation", "raw-delimiters", { revoke: "1", targetAddress: "a,b", metaData: "x;y|z" });
for (const settleType of ["INF", "inf", "+inf", "NaN", "0x24", "0b100100", "0o44", "-0x24", "  36  ", "1e2", "1.2.3", "0xno"]) {
  patch("encodeSettleChannelPNL", `number-grammar-${json(settleType)}`, { settleType });
}
for (const value of ["0.000000000000000000009", "-0.000000000000000000009", "0.999999999999999999999999999999999999", "-0.999999999999999999999999999999999999"]) {
  patch("encodeCommit", `radix-rounding-${value}`, { amount: value });
}

// Decimal-string inputs preserve every significant digit through genuine BN;
// Number bridges are exercised only where their radix36 result is safe integer.
const decimalAmounts = ["0", "1", "-1", "0.5", "-0.5", "1e-8", "1e-9", "5e-9", "-5e-9", "1.234567895", "-1.234567895", "0.000000000000000000001", "900719925.47409915", "123456789123456789.123456789", "1e30"];
for (const amount of decimalAmounts) {
  patch("encodeCommit", `bn-${amount}`, { amount });
  patch("encodeTransfer", `bn-${amount}`, { amount });
  patch("encodeTradeContractOnchain", `round-${amount}`, { price: amount });
  patch("encodeStakeFraudProof", `down-${amount}`, { amount });
}
let seed = 0x8b71dca0;
for (let index = 0; index < 96; index++) {
  seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5;
  const amount = ((seed | 0) / 100000000).toFixed(8);
  patch("encodeTradeTokenForUTXO", `seed-${index}`, { amount, satsExpected: "0.00000001", propertyId: (seed >>> 0) % 100000 });
  patch("encodeCommit", `seed-${index}`, { amount });
}

// Concrete errors, coercions and dispatch bugs, not repaired payloads.
for (const method of methods) {
  if (!["encodeCloseOracle", "encodeBatchSettlement"].includes(method)) quirk(`empty-${method}`, method, {}, "source-missing-field-observation");
}
for (const input of [0.5, 9007199254740992, -9007199254740992]) quirk(`number-radix-${input}`, "encodePropertyId", input, "Number-radix36-outside-safe-integer-domain");
for (const input of ["NaN", "Infinity", "bad", "", {}, []]) quirk(`amount-coercion-${json(input)}`, "encodeAmount", input, "nonfinite-or-unsupported-BigNumber-input");
quirk("property-own-toString", "encodePropertyId", { toString: "not-callable" }, "JSON-own-property-shadows-method");
quirk("amount-nonfinite-intermediate", "encodeTradeTokenForUTXO", { ...base.encodeTradeTokenForUTXO, amount: "1e400" }, "nonfinite-source-number-bridge-rejected-by-typed-domain");
quirk("future-shallow-onchain-data", "encodeCreateFutureContractSeries", { ...base.encodeCreateFutureContractSeries, onChainData: [[7, 11]] }, "source-reads-data0-0-and-data0-1-not-pair");
quirk("future-object-length-map-error", "encodeCreateFutureContractSeries", { ...base.encodeCreateFutureContractSeries, onChainData: { length: 1 } }, "positive-length-object-enters-map-and-throws");
quirk("batch-undefined", "encodeBatchSettlement", {}, "empty-stub-returns-undefined-not-empty-payload");
quirk("half-up-not-truncate", "encodeTradeContractOnchain", { ...base.encodeTradeContractOnchain, price: "-0.000000005" }, "default-BigNumber-integerValue-rounds-half-up");
quirk("base94-delimiter-digit", "encodeSettleChannelPNL", { ...base.encodeSettleChannelPNL, markPrice: 91 }, "legal-base94-pipe-digit-collides-with-fraction-and-batch-delimiter");
quirk("unicode-number-whitespace", "encodeSettleChannelPNL", { ...base.encodeSettleChannelPNL, settleType: "\u0085" }, "non-ASCII-input-outside-typed-domain-and-not-JS-whitespace");
quirk("very-large-prefixed-number", "encodeSettleChannelPNL", { ...base.encodeSettleChannelPNL, settleType: "0x10000000000000000" }, "prefixed-integer-coercion-beyond-local-u64-domain");
quirk("fractional-number-rounded-to-unsafe", "encodeTradeTokenForUTXO", { ...base.encodeTradeTokenForUTXO, amount: "90071992.547409915" }, "BigNumber-toNumber-rounded-before-radix36-outside-safe-domain");
const types = load(archive("legacy_types_source_types.txt", "fb894e8a25d560cc9481f9913aedf003fdde382f"), {
  "./txEncoder": encoder, "./txDecoder": {}, "./validity": {}, "./txUtils": {}, "./txIndex.js": {}, "bignumber.js": loadBigNumber(),
});
for (const [type, method] of [[2, "encodeSend"], [4, "encodeCommit"], [15, "encodeCloseOracle"], [31, "encodeKingSettle"], [32, "encodeBatchMoveZkRollup"]]) {
  const input = base[method];
  let result;
  try { result = ["string", hex(types.encodePayload(type, input))]; }
  catch (error) { result = ["throw", hex(`${error.name}:${error.message}`)]; }
  quirkRows.push(["D", `generic-type-${type}`, method, json(input), ...result, "actual-Types-dispatch-not-Rust-admission"].join("\t"));
}
for (const [method, count] of methodCounts) assert(count > 0, `uncovered exported method ${method}`);
function emit(file, rows, header) {
  const output = `${header}\n${rows.join("\n")}\n`;
  const target = path.join(root, "fixtures", file);
  if (write) fs.writeFileSync(target, output);
  else assert.equal(fs.readFileSync(target, "utf8").replace(/\r\n/g, "\n"), output, `${file} differs; regenerate explicitly with --write`);
}
emit("txencoder_vectors.tsv", vectorRows, "# V\tname\tmethod\tinput-JSON-UTF8hex\tstring|undefined\toutput-UTF8hex-or-dash\n# Complete unchanged dca0888b txEncoder.js; BigNumber9.3.1; actual Base94/converter; local bounded typed-domain cases.");
emit("txencoder_quirks.tsv", quirkRows, "# Q|D\tname\tmethod\tinput-JSON-UTF8hex\tstring|undefined|throw\toutput-or-error-UTF8hex-or-dash\tnote\n# Source observations; includes unsupported typed-domain inputs and actual Types dispatch quirks.");
console.log(`PASS txEncoder: ${methods.length} complete-source methods, ${vectorRows.length} differential cases, ${quirkRows.length} separate source/dispatch observations`);
