"use strict";

// Explicit reference substitution; archived fixture provenance stays unchanged.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { createRequire } = require("node:module");

const prefix = process.env.TL_BIGNUMBER_LOCK_ROOT;
assert(prefix && prefix.trim(), "TL_BIGNUMBER_LOCK_ROOT must name an npm prefix containing bignumber.js@9.1.2");
const referenceRequire = createRequire(path.join(path.resolve(prefix), "package.json"));
const manifestPath = referenceRequire.resolve("bignumber.js/package.json");
const sourcePath = referenceRequire.resolve("bignumber.js");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
assert.equal(manifest.name, "bignumber.js");
assert.equal(manifest.version, "9.1.2", "source-lock BigNumber version");
const source = fs.readFileSync(sourcePath);
assert.equal(crypto.createHash("sha256").update(source).digest("hex"),
  "20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e",
  "raw official BigNumber 9.1.2 source SHA256");
const code = source.toString("utf8");

const loader = require("./legacy_numeric_oracle.cjs");
loader.loadBigNumber = function loadVerifiedLockReference() {
  const module = { exports: {} };
  vm.runInNewContext(code, { module, exports: module.exports }, {
    filename: sourcePath, timeout: 1000,
  });
  const BigNumber = module.exports;
  assert.equal(typeof BigNumber, "function");
  assert.equal(BigNumber.config().DECIMAL_PLACES, 20);
  assert.equal(BigNumber.config().ROUNDING_MODE, BigNumber.ROUND_HALF_UP);
  return BigNumber;
};
process.stderr.write("BigNumber lock reference: verified 9.1.2 override; archived fixture provenance remains 9.3.1.\n");
