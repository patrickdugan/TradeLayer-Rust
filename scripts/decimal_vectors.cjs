#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { loadBigNumber, BIG_NUMBER_VERSION, BIG_NUMBER_SHA256 } = require("./legacy_numeric_oracle.cjs");
const BigNumber = loadBigNumber();
const rows = [];
function vector(method, left, right = "-", places = 20, rounding = "half-up") {
  const a = new BigNumber(left);
  const b = right === "-" ? null : new BigNumber(right);
  assert(a.isFinite() && (b === null || b.isFinite()));
  let result;
  const mode = rounding === "down" ? BigNumber.ROUND_DOWN : BigNumber.ROUND_HALF_UP;
  switch (method) {
    case "parse": result = a; break;
    case "add": result = a.plus(b); break;
    case "sub": result = a.minus(b); break;
    case "mul": result = a.times(b); break;
    case "div": {
      assert(!b.isZero());
      const Arithmetic = BigNumber.clone({ DECIMAL_PLACES: places, ROUNDING_MODE: mode });
      result = new Arithmetic(left).div(right);
      break;
    }
    case "quantize": result = a.decimalPlaces(places, mode); break;
    case "cmp": result = a.comparedTo(b); break;
    default: throw Error(method);
  }
  const text = method === "cmp" ? String(result) : result.toFixed();
  rows.push([method, left, right, places, rounding, text].join("\t"));
}

for (const raw of ["0", "-0", "+0.00", "1.2300", ".000005", "-0.000005", "1.",
  "1e-20", "-12.34E+17", "000001.000010", "1e100", "1e-100", "9".repeat(160),
  `0.${"0".repeat(200)}1`]) vector("parse", raw);
for (const left of ["0", "-0.5", "1.25", "-999999999999999999999999999999.999", "1e80", "1e-80"]) {
  for (const right of ["1", "-1", "3", "-7.000001", "1e-25", "99999999999999999999"]) {
    for (const method of ["add", "sub", "mul", "cmp"]) vector(method, left, right);
    for (const places of [0, 8, 20, 40]) {
      vector("div", left, right, places);
      vector("div", left, right, places, "down");
    }
  }
}
for (const left of ["0.5", "-0.5", "1.005", "-1.005", "1e-40", "9.999999999999999999995", "-9.999999999999999999995"]) {
  for (const places of [0, 2, 8, 20]) {
    vector("quantize", left, "-", places);
    vector("quantize", left, "-", places, "down");
  }
}
// Deterministic, independent arithmetic over many signs/scales/digit lengths.
let seed = 0x5eeda11;
function next() { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return seed >>> 0; }
function decimal() {
  const length = 1 + next() % 90;
  let coefficient = String(1 + next() % 9);
  for (let i = 1; i < length; i++) coefficient += String(next() % 10);
  const point = next() % (length + 1);
  const magnitude = point === 0 ? `0.${coefficient}` : point === length ? coefficient
    : `${coefficient.slice(0, point)}.${coefficient.slice(point)}`;
  return (next() & 1 ? "-" : "") + magnitude;
}
for (let i = 0; i < 240; i++) {
  const left = decimal(), right = decimal();
  for (const method of ["add", "sub", "mul", "cmp"]) vector(method, left, right);
  vector("div", left, right, 20, i & 1 ? "down" : "half-up");
  vector("quantize", left, "-", next() % 35, i & 1 ? "half-up" : "down");
}
const text = `# BigNumber=${BIG_NUMBER_VERSION}; LF source SHA256=${BIG_NUMBER_SHA256}\n`
  + "# method\tleft\tright\tdecimal_places\trounding\tresult\n" + rows.join("\n") + "\n";
const filename = path.join(__dirname, "..", "fixtures", "decimal_vectors.tsv");
if (process.argv.includes("--write")) fs.writeFileSync(filename, text);
else assert.equal(fs.readFileSync(filename, "utf8"), text, "decimal oracle fixture drift");
console.log(`PASS: ${rows.length} decimal vectors from the actual SHA256-pinned BigNumber implementation`);
