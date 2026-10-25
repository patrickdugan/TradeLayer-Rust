#!/usr/bin/env node
"use strict";

// Execute the actual archived helpers and bigint-base-converter 0.1.3.
// No wallet, RPC, mocks, network, or transaction admission is involved.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.resolve(__dirname, "..");

function source(file, expected) {
  const raw = fs.readFileSync(path.join(root, "fixtures", file));
  const blob = crypto.createHash("sha1")
    .update(Buffer.from(`blob ${raw.length}\0`)).update(raw).digest("hex");
  assert.equal(blob, expected, `source Git blob ${file}`);
  return raw.toString("utf8");
}

function load(code, imports = {}) {
  const module = { exports: {} };
  vm.runInNewContext(code, {
    module, exports: module.exports,
    require(name) {
      assert(Object.hasOwn(imports, name), `disallowed import ${name}`);
      return imports[name];
    },
  }, { timeout: 2000 });
  return module.exports;
}

const converter = load(source("legacy_base_converter_source.txt",
  "c38f00093bf022f4d2ef136a3e34480d409c60fd"));
const base94 = load(source("legacy_base94_source.txt",
  "8d9f3d14d0ca29488c99e460c52020e8dd66b334"), {
  "bigint-base-converter": converter,
});
const base256 = load(source("legacy_base256_source.txt",
  "90305dec38f6be043f039d0e75b296344ac57ba0"), {
  "bigint-base-converter": converter,
});

const rows = [];
function hex(s) { return Buffer.from(s, "utf8").toString("hex"); }
function add(label, operation, input) {
  const [group, method] = operation.split(".");
  const obj = group === "94" ? base94 : base256;
  let status = "ok", output;
  try {
    output = obj[method](input);
    if (output === null) { status = "null"; output = ""; }
    if (output === undefined) output = "";
  } catch (e) { status = "error"; output = e.message; }
  assert.equal(typeof output, "string");
  rows.push([label, operation, input === null ? "-" : hex(input), status, hex(output)].join("\t"));
}

const decimals = ["", "0", "0000", "0001", "1", "15", "90", "91", "92", "93", "94", "255", "256",
  "65535", "65536", "9007199254740991", "9007199254740992", "18446744073709551616",
  "115792089237316195423570985008687907853269984665640564039457584007913129639935",
  "-1", "+1", "1.5", "1e3", "NaN", "Infinity", "0x10", "1,234", " 12 ", "1\u00852", "1\ufeff2", "bad?"];
decimals.forEach((s, i) => {
  add(`decimal94-${i}`, "94.toBase94", s);
  add(`decimal256-${i}`, "256.toBase256", s);
  add(`decimalhex94-${i}`, "94.decimalToHex", s);
  add(`decimalhex256-${i}`, "256.decimalToHex", s);
});
const fractions = [null, "", "0", ".5", "1.001", "1.0100", "1.0000", "1.", "1..2", "1.2.3", "91.91", "-0.1", "+1.2", "1.e3", "0001.002", "0.00000001"];
fractions.forEach((s, i) => add(`fraction-${i}`, "94.decimalToBase94", s));
const baseInputs = [null, "", "!", "!!", "!\"", "0", "|", "||", "|||", "|!", "!|!", "!||\"", "\"|\"|~",
  "-1", "abc", " ", "!\n", "é", "\u007f", "\u00a0", "\u00ad", "\u0143", "\u0144", "\u0085", ",;|"];
baseInputs.forEach((s, i) => {
  add(`decode94-${i}`, "94.fromBase94", s);
  add(`decodehex94-${i}`, "94.base94ToHex", s);
  add(`decodehex256-${i}`, "256.base256ToHex", s);
  if (s !== null) {
    add(`decode256-${i}`, "256.fromBase256", s);
    add(`validate94-${i}`, "94.validateBase94Input", s);
  }
});
const hexes = [null, "", "0", "00", "0001", "f", "ff", "Ff", "100", "000000ff", "ff ", " ff", "f f", "\t f\n f ",
  "0x01", "0X01", "0b1", "-1", "xyz", "é", "\ufeff", "f\ufefff", "f\u0085f", "f".repeat(64), "0".repeat(63) + "1"];
hexes.forEach((s, i) => {
  add(`hexdecimal256-${i}`, "256.hexToDecimal", s);
  add(`hex256-${i}`, "256.hexToBase256", s);
  if (s !== null) {
    add(`hexdecimal94-${i}`, "94.hexToDecimal", s);
    add(`hex94-${i}`, "94.hexToBase94", s);
  }
});
const bigintSyntax = ["-0", "-15", "-16", "-255", "-256", "+15", "  +15\ufeff", "0b101", "0o17", "0XFF", "-0x1", "+0b1", "+", "-", "  ", "1_000"];
bigintSyntax.forEach((s, i) => {
  add(`bigint94-${i}`, "94.decimalToHex", s);
  add(`bigint256-${i}`, "256.decimalToHex", s);
});
for (let i = 0; i < 256; i++) {
  const encoded = base256.toBase256(String(i));
  assert.equal(encoded.length, 1);
  add(`alphabet256-encode-${i}`, "256.toBase256", String(i));
  add(`alphabet256-decode-${i}`, "256.fromBase256", encoded);
}
for (let i = 0; i < 94; i++) {
  add(`alphabet94-encode-${i}`, "94.toBase94", String(i));
  add(`alphabet94-decode-${i}`, "94.fromBase94", String.fromCharCode(33 + i));
}
// Seeded, portable large integer samples; these do not derive expected Rust output.
for (let i = 0; i < 32; i++) {
  const h = crypto.createHash("sha256").update(`legacy-base-${i}`).digest("hex");
  add(`large94-${i}`, "94.hexToBase94", h);
  add(`large256-${i}`, "256.hexToBase256", h);
  add(`large256-roundtrip-${i}`, "256.base256ToHex", base256.hexToBase256(h));
  add(`large94-decode-${i}`, "94.fromBase94", base94.hexToBase94(h));
}

// This source error contains a lone UTF-16 low surrogate; Rust deliberately
// reports UnrepresentableUtf16Error instead of substituting a different string.
assert.throws(() => base94.toBase94("😀"), e => e.message.charCodeAt(e.message.length - 1) === 0xde00);
assert.equal(base94.fromBase94(base94.toBase94("91")), "15");
assert.equal(base94.fromBase94(base94.decimalToBase94("1.001")), "1.1");
assert.equal(base256.decimalToHex("-16"), "0-10");
const text = "# Exact pinned JS helper outputs, not consensus rules. Input/output are UTF-8 hex; - means null.\n"
  + "# label\toperation\tinput\tstatus\toutput\n" + rows.join("\n") + "\n";
const target = path.join(root, "fixtures", "legacy_base_vectors.tsv");
if (process.argv.includes("--write")) fs.writeFileSync(target, text);
else assert.equal(fs.readFileSync(target, "utf8"), text, "golden vectors differ from actual JavaScript");
console.log(`Verified ${rows.length} legacy base vectors against actual pinned sources and converter 0.1.3.`);
