#!/usr/bin/env node
"use strict";

// Independent composition corpus: execute complete unchanged source modules.
// Validity is an explicitly recorded identity adapter, never real validation.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { readPinnedSource, loadBigNumber, BIG_NUMBER_VERSION, BIG_NUMBER_SHA256 } = require("./legacy_numeric_oracle.cjs");
const root = path.resolve(__dirname, "..");
const argv = process.argv.slice(2);
assert(argv.every(arg => ["--check", "--write"].includes(arg)), "unknown argument");
assert(!(argv.includes("--check") && argv.includes("--write")), "choose check or write");
const pins = {
  encoder: ["legacy_types_source_txEncoder.txt", "c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42"],
  decoder: ["legacy_types_source_txDecoder.txt", "a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076"],
  types: ["legacy_types_source_types.txt", "a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1"],
  converter: ["legacy_base_converter_source.txt", "a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62"],
  base94: ["legacy_base94_source.txt", "0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb"],
  base256: ["legacy_base256_source.txt", "11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc"],
};
function load(which, imports) {
  const [file, sha] = pins[which];
  const code = readPinnedSource(path.join(root, "fixtures", file), sha);
  const module = { exports: {} };
  vm.runInNewContext(code, {
    module, exports: module.exports, console: { log() {}, error() {}, warn() {} },
    require(name) { assert(Object.hasOwn(imports, name), `unapproved import ${name}`); return imports[name]; },
  }, { timeout: 2000, filename: `unchanged-${file}` });
  return module.exports;
}
const converter = load("converter", {});
const references = {
  "bignumber.js": loadBigNumber(),
  "./base94.js": load("base94", { "bigint-base-converter": converter }),
  "./base256.js": load("base256", { "bigint-base-converter": converter }),
};
const encoder = load("encoder", references);
const decoder = load("decoder", references);
let trace = [];
const validity = new Proxy({}, { get(_target, method) {
  return async (...args) => { trace.push([String(method), tagged(args)]); return args[1]; };
} });
const types = load("types", {
  "./txEncoder": encoder, "./txDecoder": decoder, "./validity": validity,
  "./txUtils": Object.freeze({}), "./txIndex.js": Object.freeze({}), "bignumber.js": references["bignumber.js"],
});
assert.equal(Object.keys(encoder).length, 39);
assert.equal(Object.keys(decoder).length, 36);
function tagged(value) {
  if (value === undefined) return ["undefined"];
  if (value === null) return ["null"];
  if (typeof value === "boolean") return ["boolean", value];
  if (typeof value === "string") return ["string", value];
  if (typeof value === "number") {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    return ["number", Number.isNaN(value) ? "NaN" : value === Infinity ? "Infinity" : value === -Infinity ? "-Infinity" : bytes.toString("hex")];
  }
  if (Array.isArray(value)) return ["array", Array.from(value, tagged)];
  assert.equal(typeof value, "object");
  return ["object", Object.keys(value).map(key => [key, tagged(value[key])])];
}
const hex = text => Buffer.from(text, "utf8").toString("hex") || "-";
const jhex = value => hex(JSON.stringify(value));
function observe(fn) {
  try { return { status: "ok", value: fn() }; }
  catch (error) { return { status: "throw", value: [error.name, error.message] }; }
}
// New independent arguments, not imported from any encoder/decoder fixture.
const seeds = [
  ["encodeActivateTradeLayer", "decodeActivateTradeLayer", { txTypeToActivate: [2, 9, 18, 30], codeHash: "a5".repeat(32) }],
  ["encodeTokenIssue", "decodeTokenIssue", { initialAmount: 1297, ticker: "PAIR", whitelists: [5, 37], managed: false, backupAddress: "reserve", nft: true, coloredCoinHybrid: false, proceduralType: 3 }],
  ["encodeSend", "decodeSend", { sendAll: false, address: "receiver", propertyId: 37, amount: "0.12500001" }],
  ["encodeTradeTokenForUTXO", "decodeTradeTokenForUTXO", { propertyId: 37, amount: "0.12500001", columnA: 1, satsExpected: "0.00000361", tokenOutput: 2, payToAddress: 3, isColoredOutput: true }],
  ["encodeCommit", "decodeCommitToken", { propertyId: 37, amount: "0.12500001", channelAddress: "duplex", payEnabled: true, clearLists: [5, 37], isColoredOutput: true, commitClearlistId: 73 }],
  ["encodeOnChainTokenForToken", "decodeOnChainTokenForToken", { propertyIdOffered: 37, propertyIdDesired: 73, amountOffered: "0.12500001", amountExpected: "0.25000003", stop: false, post: true }],
  ["encodeCancelOrder", "decodeCancelOrder", { isContract: 1, contractId: 37, cancelAll: false, cancelParams: { price: 73, side: 0, txid: "e".repeat(64) } }],
  ["encodeCreateWhitelist", "decodeCreateWhitelist", { backupAddress: "reserve", name: "Pair list", url: "https://pair.invalid", description: "a,b;c|d" }],
  ["encodeUpdateAdmin", "decodeUpdateAdmin", { newAddress: "next", whitelist: false, oracle: true, token: false, id: 37, updateBackup: true }],
  ["encodeIssueOrRevokeAttestation", "decodeIssueOrRevokeAttestation", { revoke: true, id: 37, targetAddress: "receiver", metaData: "mixedCase" }],
  ["encodeAMMPool", "decodeAMMPool", { isRedeem: 1, isContract: false, id: 37, amount: "2B", id2: 73, amount2: 1297 }],
  ["encodeGrantManagedToken", "decodeGrantManagedToken", { propertyId: 37, amountGranted: "0.12500001", addressToGrantTo: "receiver", dlcTemplateId: "t", dlcContractId: "c", settlementState: "s", dlcHash: "h" }],
  ["encodeRedeemManagedToken", "decodeRedeemManagedToken", { propertyId: 37, amountDestroyed: "0.25000003", dlcTemplateId: "t", dlcContractId: "c", settlementState: "s" }],
  ["encodeCreateOracle", "decodeCreateOracle", { ticker: "PAIR", url: "feed", backupAddress: "reserve", whitelists: [5, 37], lag: 73, enum: "X" }],
  ["encodePublishOracleData", "decodePublishOracleData", { oracleid: 37, price: "0.0123", high: 73, low: 37, close: 36 }],
  ["encodeCloseOracle", "decodeCloseOracle", 1297],
  ["encodeCreateFutureContractSeries", "decodeCreateFutureContractSeries", { native: false, underlyingOracleId: 37, onChainData: [[[5, 37]], [[73, 1297]]], notionalPropertyId: 37, notionalValue: 73, collateralPropertyId: 5, leverage: 7.5, expiryPeriod: 1297, series: 2, inverse: true, fee: false }],
  ["encodeExerciseDerivative", "decodeExerciseDerivative", { derivativeContractId: 37, amount: 73 }],
  ["encodeTradeContractOnchain", "decodeTradeContractOnchain", { contractId: 37, price: "0.000000015", amount: 73, sell: false, insurance: true, reduce: false, post: true, stop: false }],
  ["encodeTradeContractChannel", "decodeTradeContractChannel", { contractId: 37, price: "0.000000015", amount: 73, columnAIsSeller: false, expiryBlock: 1297, insurance: true, columnAIsMaker: false }],
  ["encodeTradeTokensChannel", "decodeTradeTokensChannel", { propertyId1: 37, propertyId2: 73, amountOffered1: "0.12500001", amountDesired2: "0.25000003", columnAIsOfferer: false, expiryBlock: 1297, columnAIsMaker: true, Id1ColoredOutput: true, Id2ColoredOutput: false }],
  ["encodeWithdrawal", "decodeWithdrawal", { withdrawAll: true, propertyId: 37, amountOffered: "0.12500001", column: 1, channelAddress: "duplex" }],
  ["encodeTransfer", "decodeTransfer", { propertyId: 37, amount: "0.12500001", isColumnA: false, destinationAddr: "duplex", pay: true, payRef: "receipt" }],
  ["encodeSettleChannelPNL", "decodeSettleChannelPNL", { txidNeutralized1: "raw-one", txidNeutralized2: "raw-two", markPrice: "0.125", settleType: 3, columnAIsSeller: false, columnAIsMaker: true, netAmount: "0.25", expiryBlock: 1297, blockStart: 37, blockEnd: 73, propertyId: 5, aPaysBDirection: false, channelRoot: "root", totalContracts: 73, neutralizedCount: 37 }],
  ["encodeMintSynthetic", "decodeMintSynthetic", { propertyIdUsed: "s-5-37", contractIdUsed: 37, amount: 73 }],
  ["encodeRedeemSynthetic", "decodeRedeemSynthetic", { propertyIdUsed: "s-5-37", contractIdUsed: 37, amount: 73 }],
  ["encodePayToTokens", "decodePayToTokens", { propertyIdTarget: 37, propertyIdUsed: 73, amount: 1297 }],
  ["encodeOptionTrade", "decodeOptionTrade", { contractId: "37-73-C", price: "0.12500001", amount: 73, columnAIsSeller: false, expiryBlock: 1297, columnAIsMaker: true, comboTicker: "37-1297-P", comboPrice: 2, comboAmount: 3 }],
  ["encodeTradeBaiUrbun", "decodeTradeBaiUrbun", { propertyIdDownPayment: 37, propertyIdToBeSold: 73, price: 1297, amount: 3, expiryBlock: 37, tradeExpiryBlock: 73 }],
  ["encodeTradeMurabaha", "decodeTradeMurabaha", { propertyIdDownPayment: 37, downPaymentPercent: 7, propertyIdToBeSold: 73, price: 1297, amount: 3, expiryBlock: 37, installmentInterval: 5, tradeExpiryBlock: 73 }],
  ["encodeStakeFraudProof", "decodeStakeFraudProof", { action: 2, oracleId: 37, stakedPropertyId: 73, amount: "0.000000019", accusedAddress: "accused", evidenceHash: "evidence", relayType: 2, stateHash: "state", dlcRef: "first", settlementState: "settled", relayBlob: "relay", autoRoll: false, nextDlcRef: "second" }],
  ["encodeKingSettle", "decodeKingSettle", { blockStart: 37, blockEnd: 73, propertyId: 5, netAmount: "0.125", aPaysBDirection: false, channelRoot: "root", totalContracts: 73, neutralizedCount: 37 }],
  ["encodeBatchMoveZkRollup", "decodeBatchMoveZkRollup", { proof: "p|q", payments: [{ fromAddress: "giver", propertyIds: [37, 73], amounts: [1297, 3], toAddress: "receiver", sentPropertyIds: [5], sentAmounts: [7] }], miscLogic: { z: false, "3": 37, a: "|;," }, miscData: [null, 0, "x"] }],
  ["encodeColoredCoin", "decodeColoredCoin", { encodeDecodeRecode: 2, propertyId: 37, SatsRatio: 73, homeAddress: "home" }],
  ["encodeAbstractionBridge", "decodeAbstractionBridge", { propertyId: 37, amount: 73, destinationAddr: "remote" }],
  ["encodeBindSmartContract", "decodeSmartContractBind", { contractHash: "contract", type: 3, hostChain: "LTC" }],
];
const routeDecoders = seeds.map(seed => seed[1]);
routeDecoders[34] = "decodeCrossLayerBridge";
const rows = [];
const coveredEncoders = new Set(), coveredDecoders = new Set();
async function add(name, type, input, options = {}) {
  const seed = type === null ? null : seeds[type];
  const method = options.encoder || seed?.[0] || "-";
  const route = options.route || "direct";
  const inputJson = options.inputJson === undefined ? JSON.stringify(input) : options.inputJson;
  const args = JSON.parse(inputJson);
  const encoding = route === "raw" ? { status: "ok", value: options.body } : observe(() => route === "generic" ? types.encodePayload(type, args) : encoder[method](args));
  if (route !== "raw") coveredEncoders.add(method);
  if (encoding.status === "ok" && encoding.value === undefined) encoding.status = "undefined";
  const decoderMethod = options.decoder || (route === "generic" ? routeDecoders[type] : seed?.[1]) || "-";
  const bodyPolicy = options.policy || (route === "generic" ? "strip-one" : type !== null && type <= 31 ? "strip-one-if-present" : "identity");
  let body = "", decoded = { status: "unreached", value: undefined }, dispatched = decoded;
  trace = [];
  if (encoding.status === "ok") {
    assert.equal(typeof encoding.value, "string", name);
    const prefix = type === null ? "" : `tl${type.toString(36)}`;
    body = bodyPolicy === "strip-one" ? encoding.value.slice(3)
      : ["strip-one-if-present", "truncate", "append"].includes(bodyPolicy) && prefix && encoding.value.startsWith(prefix) ? encoding.value.slice(3) : encoding.value;
    if (options.transform) body = options.transform(body);
    assert(Buffer.byteLength(body) <= 4096, name);
    if (typeof decoder[decoderMethod] === "function") {
      coveredDecoders.add(decoderMethod);
      decoded = observe(() => decoder[decoderMethod](body));
    } else decoded = { status: "missing", value: decoderMethod };
    if (type !== null) {
      try {
        const value = options.threeArgs ? await types.decodePayload("wire-tx", "tl", body)
          : await types.decodePayload("wire-tx", type, options.marker || "tl", body, "wire-sender", options.reference || [], 17, 23, 101);
        // Cross-realm Error objects deliberately returned, not thrown.
        dispatched = value && Object.prototype.toString.call(value) === "[object Error]"
          ? { status: "error", value: [value.name, value.message] } : { status: "ok", value };
      } catch (error) { dispatched = { status: "throw", value: [error.name, error.message] }; }
    }
  }
  const result = observation => jhex(observation.status === "ok" ? tagged(observation.value) : observation.value ?? null);
  rows.push(["W", name, route, type ?? "-", method, hex(inputJson), options.domain || (route === "raw" ? "raw" : "exact"), encoding.status,
    jhex(encoding.status === "ok" ? encoding.value : encoding.value ?? null), bodyPolicy, decoderMethod, hex(body), decoded.status, result(decoded), dispatched.status, result(dispatched), jhex(trace), options.note || "source-observation"].join("\t"));
}
async function main() {
  for (let type = 0; type < seeds.length; type++) await add(`direct-${type}`, type, seeds[type][2]);
  await add("helper-amount", null, "0.12500001", { encoder: "encodeAmount", decoder: "decodeAmount" });
  await add("helper-property", null, "s-5-37", { encoder: "encodePropertyId", decoder: "decodePropertyId" });
  await add("empty-batch-stub", null, {}, { encoder: "encodeBatchSettlement", decoder: "-" });
  for (let type = 0; type < seeds.length; type++) await add(`generic-${type}`, type, seeds[type][2], { route: "generic", note: "Types-prefixes-before-calling-source-export" });
  for (let type = 0; type < seeds.length; type++) await add(`truncated-${type}`, type, seeds[type][2], { policy: "truncate", transform: text => text.slice(0, Math.min(2, text.length)), note: "raw-source-return-truncated-to-two-code-units" });
  const patch = async (name, type, changes, options) => add(name, type, { ...seeds[type][2], ...changes }, options);
  for (const [name, changes] of [
    ["all", { sendAll: true }], ["empty-address", { address: "" }],
    ["multi", { propertyId: [37, 73], amount: ["0.12500001", 3] }],
    ["unequal", { propertyId: [37, 73, 5], amount: [3] }],
    ["array-amount", { amount: ["0.12500001", 3] }], ["zero", { propertyId: 0, amount: 0 }],
  ]) await patch(`send-${name}`, 2, changes);
  for (const [i, value] of ["0.0000000049999999", "0.0000000050000001", "-0.0000000050000001", "90071992.54740989", 90071992.54740989, "0.999999995", "-0", 1295, 1296, 1297].entries()) {
    await add(`amount-boundary-${i}`, null, value, { encoder: "encodeAmount", decoder: "decodeAmount" });
  }
  await add("amount-negative-number-zero", null, 0, { inputJson: "-0", encoder: "encodeAmount", decoder: "decodeAmount" });
  for (const [name, type, changes, domain] of [
    ["attestation-unicode", 9, { metaData: "Straßeıſﬃé☃" }, "ascii-reject"],
    ["send-unicode", 2, { address: "受信者😀" }, "ascii-reject"],
    ["name-delimiter", 7, { name: "x,y;z|q" }, "exact"],
    ["transfer-delimiter", 22, { destinationAddr: "a,b;c|d" }, "exact"],
    ["empty-optional", 13, { whitelists: [], ticker: "", enum: "" }, "exact"],
    ["oracle-high-only", 14, { low: undefined, close: undefined, high: 0 }, "exact"],
    ["oracle-close-only", 14, { high: undefined, low: undefined, close: 73 }, "exact"],
    ["native-discards-pairs", 16, { native: true }, "exact"],
    ["fractional-contract-number", 18, { amount: 0.125 }, "radix-reject"],
    ["fractional-contract-string", 18, { amount: "0.125" }, "exact"],
    ["unsafe-contract-id", 18, { contractId: 9007199254740992 }, "radix-reject"],
    ["withdraw-reference", 21, { channelAddress: "C".repeat(43), ref: "-0" }, "exact"],
    ["transfer-reference", 22, { destinationAddr: "C".repeat(43), ref: "04" }, "exact"],
    ["option-no-combo", 27, { comboAmount: 0 }, "exact"],
    ["settle-empty-extension", 23, { channelRoot: "", settleType: "NaN", netAmount: 0 }, "exact"],
    ["commit-empty-list", 4, { clearLists: [] }, "exact"],
    ["commit-single-list", 4, { clearLists: [37] }, "exact"],
    ["commit-missing-channel", 4, { channelAddress: undefined }, "source-throw"],
    ["future-malformed-nesting", 16, { onChainData: [null] }, "source-throw"],
    ["contract-nan-price", 18, { price: "NaN" }, "numeric-reject"],
    ["contract-infinite-price", 18, { price: "Infinity" }, "numeric-reject"],
    ["settlement-negative-base94", 23, { netAmount: "-0.25" }, "source-throw"],
  ]) await patch(name, type, changes, { domain });
  for (const type of [2, 4, 9, 18, 23, 32, 33, 34]) await add(`extra-fields-${type}`, type, seeds[type][2], { policy: "append", transform: text => text + ",extra;|,0", note: "raw-source-return-with-extra-fields" });
  const raw = async (name, type, method, body, options = {}) => add(name, type, body, { route: "raw", encoder: "-", decoder: method, body, policy: "raw", ...options });
  await raw("newtx-opaque", null, "decodePublishNewTx", '{"p":"wire;|,😀"}\n');
  await raw("negative-zero-close", 15, "decodeCloseOracle", "-0");
  await raw("nan-close", 15, "decodeCloseOracle", "?");
  await raw("infinite-amount", null, "decodeAmount", "z".repeat(240));
  await raw("negative-infinite-amount", null, "decodeAmount", "-" + "z".repeat(240));
  await raw("send-unreachable-return", 2, "decodeSend", "0;x;7;1;0;0");
  await raw("bad-marker-returned-error", 18, "decodeTradeContractOnchain", "11,2,1", { marker: "xx", note: "returned-Error-not-throw" });
  await raw("txindex-three-args", 18, "decodeTradeContractOnchain", "11,2,1", { threeArgs: true, note: "misordered-three-argument-call" });
  await raw("txindex-three-args-tl", 18, "decodeTradeContractOnchain", "tl", { threeArgs: true, note: "string-type-misses-numeric-switch" });
  await raw("utxo-no-refs", 3, "decodeTradeTokenForUTXO", "11,1,1,1,2,3,0");
  await raw("utxo-identity-validator", 3, "decodeTradeTokenForUTXO", "11,1,1,1,2,3,0", { reference: [{ vout: 2, address: "delivery" }, { vout: 3, address: "payment" }], note: "identity-validator-not-authentication" });
  assert.deepEqual([...coveredEncoders].sort(), Object.keys(encoder).sort());
  assert.deepEqual([...coveredDecoders].sort(), Object.keys(decoder).sort());
  assert(rows.length >= 100 && rows.length <= 200, `case count ${rows.length}`);
  const header = [
    "# Complete unchanged source dca0888b827c5bc28fa9189bc9e1d507f5c5670c; genuine BigNumber9.3.1/base helpers.",
    "# W\tname\troute\ttype-or-dash\tencoder\tinputJSONhex\trust-encoder-disposition\tencode-status\tencoded-string-or-error-JSONhex\tbody-policy\tdecoder\tbodyUTF8hex\tdecode-status\ttagged-result-or-error-JSONhex\tTypes-decode-status\ttagged-result-or-error-JSONhex\tidentity-validity-traceJSONhex\tnote",
    ...Object.entries(pins).map(([name, [file, hash]]) => `# sha256 ${name} ${hash} ${file}`),
    `# sha256 BigNumber${BIG_NUMBER_VERSION} ${BIG_NUMBER_SHA256} legacy_bignumber_source.txt`,
    "# Hex '-' means empty bytes. Error values are [name,message]; missing is method name; unreached/undefined is JSON null. Number diagnostics are exact bits or NaN/Infinity/-Infinity.",
  ].join("\n");
  const output = header + "\n" + rows.join("\n") + "\n";
  const filename = path.join(root, "fixtures/wire_roundtrip_vectors.tsv");
  if (argv.includes("--write")) fs.writeFileSync(filename, output);
  else assert.equal(fs.readFileSync(filename, "utf8").replace(/\r\n/g, "\n"), output, "wire fixture differs; regenerate --write");
  console.log(`PASS wire composition: ${rows.length} cases; ${coveredEncoders.size} encoder exports; ${coveredDecoders.size} decoder exports; 36 generic encode routes; Node ${process.version}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
