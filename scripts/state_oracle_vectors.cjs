#!/usr/bin/env node
"use strict";

// Execute COMPLETE unchanged stateOracle.js. Crypto and Buffer are Node builtins;
// only sequential tally observations are injected. No source/prototype rewrites.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { fixtures, readPinnedSource } = require("./legacy_numeric_oracle.cjs");
const SHA = "4a8a06227544ffa12c43c5fe5f987dc02b1ae2c1c0f27aa70de3645fdcc57988";
const args = process.argv.slice(2), sourceIndex = args.indexOf("--source");
assert.ok(!(args.includes("--write") && args.includes("--check")), "choose --write or --check");
if (sourceIndex >= 0) assert.ok(args[sourceIndex + 1], "--source requires stateOracle.js");
const source = readPinnedSource(sourceIndex < 0 ? path.join(fixtures, "legacy_state_oracle_source.txt") : args[sourceIndex + 1], SHA);
const hex = text => Buffer.from(text, "utf8").toString("hex") || "-";
const bits = number => { const b = Buffer.alloc(8); b.writeDoubleBE(number); return b.toString("hex"); };
const fromBits = text => Buffer.from(text, "hex").readDoubleBE();
const sha = data => crypto.createHash("sha256").update(data).digest("hex");
let pending = [], nextQuery = 0, currentProperty = 0;
const tally = { async getTally(address, propertyId) {
  assert.equal(propertyId, currentProperty);
  const observation = pending[nextQuery++];
  assert.ok(observation, "unexpected tally query");
  assert.equal(address, observation.address, "source queries addresses in input order");
  if (observation.available === "m") return undefined;
  if (observation.available === "e") return {};
  if (observation.available === "n") return null;
  return { available: observation.available };
} };
const moduleReference = { exports: {} };
const context = vm.createContext({ module: moduleReference, exports: moduleReference.exports, Buffer,
  require(name) {
    if (name === "crypto") return crypto;
    if (name === "./tally.js") return tally;
    throw Error("unexpected stateOracle dependency " + name);
  },
});
vm.runInContext(source, context, { filename: "complete-pinned-stateOracle.js", timeout: 1000 });
const sourceApi = moduleReference.exports;
assert.deepEqual(Object.keys(sourceApi), ["buildAddressBalancePayload", "encodeBalancePayload", "payloadHashFromB64"]);
const locale = vm.runInContext("new Intl.Collator().resolvedOptions()", context);
assert.equal(locale.locale, "en-US", "source oracle requires en-US ambient locale (LC_ALL=en_US.UTF-8)");
assert.equal(locale.sensitivity, "variant");
assert.equal(locale.ignorePunctuation, false);
assert.equal(locale.numeric, false);
assert.equal(locale.caseFirst, "false");

function encoded(payload) {
  const base64 = sourceApi.encodeBalancePayload(payload);
  const bytes = Buffer.from(base64, "base64");
  const hash = sourceApi.payloadHashFromB64(base64);
  assert.equal(hash, sha(bytes));
  return { json: bytes.toString("utf8"), base64, hash };
}
function neighbour(value, direction) {
  if (value === 0) return direction > 0 ? Number.MIN_VALUE : -Number.MIN_VALUE;
  let raw = BigInt("0x" + bits(value));
  raw += (value > 0) === (direction > 0) ? 1n : -1n;
  return fromBits(raw.toString(16).padStart(16, "0"));
}
let randomState = 0x9e3779b97f4a7c15n;
function random64() {
  randomState ^= BigInt.asUintN(64, randomState << 13n);
  randomState ^= randomState >> 7n;
  randomState ^= BigInt.asUintN(64, randomState << 17n);
  randomState = BigInt.asUintN(64, randomState);
  return randomState;
}
const observation = (address, available) => ({ address, available });

async function main() {
  const rows = [], counts = { N: 0, P: 0, E: 0, H: 0 };
  function numberRow(name, value) {
    assert.ok(Number.isFinite(value));
    const output = encoded([value]);
    assert.equal(output.json[0], "[");
    assert.equal(output.json.at(-1), "]");
    rows.push(["N", name, bits(value), hex(output.json.slice(1, -1)), output.base64, output.hash].join("\t"));
    counts.N++;
  }
  const explicit = [0, -0, Number.MIN_VALUE, -Number.MIN_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE,
    Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER, 9007199254740992, 9007199254740994,
    1000000000000000128, 0.1, 0.1 + 0.2, 0.3, 2.2250738585072014e-308,
    2.225073858507201e-308, 1e-308, 1e-307, 1e-300, 1e-100, 1e-23,
    1e-9, 1e-8, 1e-7, 1e-6, 1e-5, 1e-4, 1, 2, 10, 1e8, 1e15, 1e16,
    1e20, 1e21, 1e22, 1e23, 1e24, 1e100, 1e300, 0.0000010000000000000002,
    1.2345678901234567, 999999999999999900000, 1000000000000000100];
  for (let i = 0; i < explicit.length; i++) numberRow("explicit-" + i, explicit[i]);
  for (const value of [1e-7, 1e-6, 1e-5, 1e-8, 1e20, 1e21, 1e22, 1e23, 2.2, 0.3, 9007199254740992]) {
    for (const sign of [1, -1]) for (const direction of [-1, 1]) {
      numberRow("neighbour-" + bits(value * sign) + "-" + direction, neighbour(value * sign, direction));
    }
  }
  for (let i = 0; i < 2048;) {
    const value = fromBits(random64().toString(16).padStart(16, "0"));
    if (Number.isFinite(value)) numberRow("seed-" + i++, value);
  }

  async function payloadRow(name, propertyId, bucketSize, observations) {
    currentProperty = propertyId; pending = observations; nextQuery = 0;
    const request = { propertyId, addresses: observations.map(row => row.address) };
    if (bucketSize !== "default") request.bucketSize = bucketSize;
    const payload = await sourceApi.buildAddressBalancePayload(request);
    assert.equal(nextQuery, observations.length);
    const output = encoded(payload);
    const input = observations.map(row => hex(row.address) + ":" +
      (typeof row.available === "string" ? row.available : bits(row.available))).join(";") || "-";
    rows.push(["P", name, propertyId, bucketSize === "default" ? "default" : bits(bucketSize), input,
      hex(output.json), output.base64, output.hash].join("\t"));
    counts.P++;
    return payload;
  }
  await payloadRow("empty-default", 0, "default", []);
  await payloadRow("missing-balances", 4294967295, 1, [observation("missing", "m"), observation("empty", "e"), observation("null", "n")]);
  const all94 = Array.from({ length: 94 }, (_, index) => String.fromCharCode(index + 33)).reverse();
  await payloadRow("all94-printable-singletons", 7, 0.3, all94.map((address, index) => observation(address, (index - 45) / 7)));
  const mixed = ["a", "A", "aa", "aA", "Aa", "AA", "a0", "A1", "a1", "A0", "a-", "A_", "a_", "A-", "a!", "A?",
    "a.1", "A.01", "a.01", "A.1", "a10", "a2", "A10", "A2", "bc1q", "BC1Q", "bc1Q", "ltc1q", "Lc1q", "1Aa", "1aa", "3abc"];
  await payloadRow("mixed-prefix-case-punctuation", 1, 1, mixed.map((address, i) => observation(address, i / 10)));
  await payloadRow("all94-with-mixed-prefixes-and-duplicates", 1, 0.1,
    [...all94, ...mixed, "a", "A", "a", "A", "aA", "Aa", "aA", "Aa"].map((address, i) => observation(address, i + 0.3)));
  const duplicates = await payloadRow("stable-exact-duplicates", 1, 2.5,
    ["a", "A", "a", "A", "aA", "Aa", "aA", "Aa"].map((address, i) => observation(address, i + 1)));
  assert.deepEqual(Array.from(duplicates.rows).filter(row => row.address === "a").map(row => row.available), [1, 3]);
  const pairs = [["ab", "aB"], ["Ab", "aB"], ["aA0", "AA1"], ["a0", "A1"], ["a1", "A0"], ["A", "aa"],
    ["a", "A0"], ["a_", "A-"], ["a!", "A?"], ["a10", "a2"], ["[", "{"], ["/", "\\"], ["$", "~"], ["#", "&"]];
  for (let i = 0; i < pairs.length; i++) await payloadRow("locale-pair-" + i, 1, 1,
    pairs[i].map((address, j) => observation(address, j + 1)));
  const balances = [0, -0, Number.MIN_VALUE, -Number.MIN_VALUE, 0.3, -0.3, 1.000000009, -1.000000009,
    0.000000009, -0.000000009, 123456789.12345679, -123456789.12345679, 9007199254740992, -9007199254740992];
  const buckets = ["default", 0, -0, -1, Number.MIN_VALUE, -Number.MIN_VALUE, 1e-9, 1e-8, 1e-7, 0.1, 0.3, 1, 2.5, 10.000000019, 1e12];
  for (let i = 0; i < buckets.length; i++) await payloadRow("bucket-grid-" + i, i, buckets[i],
    balances.map((value, j) => observation("balance-" + j, value)));
  for (let i = 0; i < 48; i++) {
    const values = Array.from({ length: 8 }, (_, j) => {
      const magnitude = Number(random64() % 1_000_000_000_000_000n) / 10 ** (j % 7);
      return observation("seed-" + i + "-" + j, j % 2 ? -magnitude : magnitude);
    });
    await payloadRow("seeded-buckets-" + i, i, buckets[2 + i % (buckets.length - 2)], values);
  }
  await payloadRow("unsupported-empty-address", 1, 1, [observation("", "m")]);
  await payloadRow("unsupported-space-address", 1, 1, [observation(" ", "m")]);
  await payloadRow("unsupported-non-ASCII-address", 1, 1, [observation("é", 1)]);
  await payloadRow("unsupported-quantize-available-overflow", 1, 1, [observation("alice", Number.MAX_VALUE)]);
  await payloadRow("unsupported-quantize-bucket-overflow", 1, Number.MAX_VALUE, [observation("alice", 1)]);
  await payloadRow("unsupported-nonfinite-available-NaN", 1, 1, [observation("alice", NaN)]);
  await payloadRow("unsupported-nonfinite-available-Infinity", 1, 1, [observation("alice", Infinity)]);
  await payloadRow("unsupported-nonfinite-bucket-NaN", 1, NaN, [observation("alice", 1)]);

  const generic = [
    ["null-fallback", "null"], ["false-fallback", "false"], ["zero-fallback", "0"], ["negative-zero-fallback", "-0"],
    ["empty-string-fallback", '""'], ["true", "true"], ["number", "1"], ["small-number", "1e-7"],
    ["empty-array", "[]"], ["empty-object", "{}"], ["text", JSON.stringify('quote" slash/ backslash\\\n\t\u0000 λ🙂\u2028\u2029')],
    ["mixed-array", '[null,false,true,0,-0,1e-7,1e21,"x",[],{}]'],
    ["integer-property-order", '{"b":1,"10":"ten","2":"two","01":3,"a":4,"0":"zero"}'],
    ["duplicate-property-last-value-original-slot", '{"z":1,"a":2,"z":3,"1":4,"a":5}'],
    ["nested", '{"rows":[{"a":[1,2,3]},{"null":null,"flag":false}],"label":"λ🙂"}'],
    ["escaped-keys", '{"quote\\\"":"a\\tb","slash\\\\":"c\\nd","nul\\u0000":0}'],
    ["integer-key-boundaries", '{"4294967295":1,"4294967294":2,"00":3,"-0":4,"0":5,"1":6}'],
  ];
  for (const [name, input] of generic) {
    const output = encoded(JSON.parse(input));
    rows.push(["E", name, hex(input), hex(output.json), output.base64, output.hash].join("\t"));
    counts.E++;
  }
  const malformed = ["", "A", "AA", "AAA", "AAAA", "AAAAA", "Q", "Q=", "Q==", "QQ", "QQ=", "QQ==", "QQ===", "QQ===garbage",
    "Q!Q==", "Q Q= =", "\nQ\tQ==\r", "QUJD", "QUJD====", "QUJD#x?y", "_w==", "/w==", "-w==", "+w==", "SGVsbG8td29ybGQ_",
    "SGVsbG8td29ybGQ/", "A-B_C-/_+", "====QQ", "=QQ==", "QQ=AA", "QQ==AA", "Q===Q", "[QQ]", "💩Q==", "ŁQ==", "šQ==", "éQ==",
    "AĀQ==", "汉QQ", "Q\u0000Q==", "\u00a0QQ==", "ＱＱ==", "%51%51=="];
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/_-!? \n\r\t=ĀŁš💩";
  for (let i = 0; i < 32; i++) {
    let input = "";
    for (let j = 0; j < i + 1; j++) input += alphabet[Number(random64() % BigInt(alphabet.length))];
    // Array indexing can select one half of an emoji; make fixture UTF-8 input
    // round-trip explicit so the source and Rust consume the same string.
    malformed.push(Buffer.from(input, "utf8").toString("utf8"));
  }
  for (let i = 0; i < malformed.length; i++) {
    const input = malformed[i], decoded = Buffer.from(input, "base64"), hash = sourceApi.payloadHashFromB64(input);
    assert.equal(hash, sha(decoded));
    rows.push(["H", "permissive-" + i, hex(input), decoded.toString("hex") || "-", hash].join("\t"));
    counts.H++;
  }
  const output = "# Complete unchanged stateOracle.js dca0888b827c5bc28fa9189bc9e1d507f5c5670c\n# source LF SHA256 " + SHA +
    "\n# Locale en-US variant, punctuation-sensitive, numeric=false, caseFirst=false; no source/prototype rewrites.\n" +
    "# N name f64bits actual-number-JSONhex actual-array-B64 actual-array-SHA256\n" +
    "# P name property-u32 bucketbits-or-default observations(addresshex:availablebits-or-m/e/n joined;) actual-JSONhex actual-B64 actual-SHA256\n" +
    "# E name JSON-inputhex actual-JSONhex actual-B64 actual-SHA256\n" +
    "# H name UTF8-Base64-inputhex actual-decoded-byteshex actual-SHA256\n" +
    "# '-' means empty hex/list. m=missing tally,e=empty tally,n=null tally. P unsupported-* are source observations outside typed builder domain.\n" + rows.join("\n") + "\n";
  const target = path.join(fixtures, "state_oracle_vectors.tsv");
  if (args.includes("--write")) fs.writeFileSync(target, output);
  else assert.equal(fs.readFileSync(target, "utf8"), output, "state oracle source drift");
  console.log(`stateOracle: N=${counts.N} P=${counts.P} E=${counts.E} H=${counts.H}; all 3 exported methods; SHA ${SHA}`);
}
main().catch(error => { console.error(error); process.exitCode = 1; });
