"use strict";
// Loads the pinned legacy tradelayer.js txDecoder (patrickdugan/tradelayer.js@dca0888b) in a
// Node VM, offline. The loading code is the one in scripts/audit_legacy_txdecoder_source.cjs:
// the same pinned BigNumber, Base94, Base256 and converter sources, the same hash checks, the
// same module whitelist. No npm package is installed or required.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const vm = require("node:vm");

const ROOT = path.resolve(__dirname, "..", "..");
const { readPinnedSource, loadBigNumber } = require(path.join(ROOT, "scripts", "legacy_numeric_oracle.cjs"));

function loadReference(name, hash, dependencies = {}) {
  const code = readPinnedSource(path.join(ROOT, "fixtures", name), hash);
  const module = { exports: {} };
  vm.runInNewContext(
    code,
    {
      module,
      exports: module.exports,
      console: { log() {}, error() {} },
      require(key) {
        assert(Object.hasOwn(dependencies, key), "unapproved reference import " + key);
        return dependencies[key];
      },
    },
    { timeout: 2000, filename: name },
  );
  return module.exports;
}

function loadLegacyDecoder() {
  const filename = path.join(ROOT, "fixtures", "legacy_types_source_txDecoder.txt");
  const bytes = Buffer.from(fs.readFileSync(filename, "utf8").replace(/\r\n/g, "\n"));
  const sourceSha = crypto
    .createHash("sha1")
    .update(Buffer.concat([Buffer.from("blob " + bytes.length + "\0"), bytes]))
    .digest("hex");
  assert.equal(sourceSha, "4728f1bdce1190428f91ae5b02f380584073c431");
  const BigNumber = loadBigNumber();
  const converter = loadReference(
    "legacy_base_converter_source.txt",
    "a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62",
  );
  const base94 = loadReference(
    "legacy_base94_source.txt",
    "0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb",
    { "bigint-base-converter": converter },
  );
  const base256 = loadReference(
    "legacy_base256_source.txt",
    "11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc",
    { "bigint-base-converter": converter },
  );
  const sourceModule = { exports: {} };
  vm.runInNewContext(
    bytes.toString("utf8"),
    {
      module: sourceModule,
      exports: sourceModule.exports,
      console: { log() {}, error() {} },
      require(key) {
        if (key === "bignumber.js") return BigNumber;
        if (key === "./base94.js") return base94;
        if (key === "./base256.js") return base256;
        throw Error("unapproved module import " + key);
      },
    },
    { timeout: 1000, filename: "pinned-txDecoder.js" },
  );
  return sourceModule.exports;
}

// Tagged JSON of a decoded value, as in the audit script (and the fixture's expected column).
function tagged(value) {
  if (value === undefined) return ["undefined"];
  if (value === null) return ["null"];
  if (typeof value === "boolean") return ["boolean", value];
  if (typeof value === "string") return ["string", value];
  if (typeof value === "number") {
    let text = Number.isNaN(value) ? "NaN" : value === Infinity ? "Infinity" : value === -Infinity ? "-Infinity" : null;
    if (text === null) {
      const b = Buffer.alloc(8);
      b.writeDoubleBE(value);
      text = b.toString("hex");
    }
    return ["number", text];
  }
  if (Array.isArray(value)) return ["array", Array.from(value, tagged)];
  assert.equal(typeof value, "object");
  return ["object", Object.keys(value).map((key) => [key, tagged(value[key])])];
}

const hexToText = (h) => (h === "-" ? "" : Buffer.from(h, "hex").toString("utf8"));

// Rows of the historical decoder corpus: { method, body, ok, expected | name, message }.
function loadCorpus() {
  const file = path.join(ROOT, "fixtures", "legacy_txdecoder_historical_vectors.tsv");
  return fs
    .readFileSync(file, "utf8")
    .replace(/\r\n/g, "\n")
    .split("\n")
    .filter((l) => l && !l.startsWith("#"))
    .map((line) => {
      const f = line.split("\t");
      return f[3] === "ok"
        ? { method: f[0], body: hexToText(f[2]), ok: true, expected: hexToText(f[4]) }
        : { method: f[0], body: hexToText(f[2]), ok: false, name: f[4], message: hexToText(f[5]) };
    });
}

module.exports = { ROOT, loadLegacyDecoder, loadCorpus, tagged, hexToText };
