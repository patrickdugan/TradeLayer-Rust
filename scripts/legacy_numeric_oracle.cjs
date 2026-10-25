"use strict";

// Shared offline source loader, not a replacement decimal implementation.
// The complete reviewed test reference and its MIT licence live in fixtures.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const fixtures = path.join(__dirname, "..", "fixtures");
const BIG_NUMBER_VERSION = "9.3.1";
const BIG_NUMBER_SHA256 = "58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16";

function readPinnedSource(filename, expectedHash) {
  const code = fs.readFileSync(filename, "utf8").replace(/\r\n/g, "\n");
  const hash = crypto.createHash("sha256").update(code).digest("hex");
  assert.equal(hash, expectedHash, `LF-normalized source SHA256: ${filename}`);
  return code;
}

function loadBigNumber() {
  const code = readPinnedSource(path.join(fixtures, "legacy_bignumber_source.txt"), BIG_NUMBER_SHA256);
  const module = { exports: {} };
  vm.runInNewContext(code, { module, exports: module.exports }, {
    filename: "pinned-bignumber-9.3.1.js", timeout: 1000,
  });
  const BigNumber = module.exports;
  assert.equal(typeof BigNumber, "function");
  assert.equal(BigNumber.config().DECIMAL_PLACES, 20);
  assert.equal(BigNumber.config().ROUNDING_MODE, BigNumber.ROUND_HALF_UP);
  return BigNumber;
}

module.exports = { fixtures, readPinnedSource, loadBigNumber, BIG_NUMBER_VERSION, BIG_NUMBER_SHA256 };
